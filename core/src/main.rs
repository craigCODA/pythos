#![cfg_attr(not(test), no_main)]
#![cfg_attr(not(test), no_std)]

mod architecture;
mod audio;
mod audit;
mod block_device;
mod boot_assets;
mod boot_info;
mod boot_metadata;
mod capabilities;
mod cinematic_boot;
mod compositor;
mod context_switch;
mod font;
mod font_system;
mod framebuffer;
mod input_drivers;
mod input_events;
mod interpreter;
mod ipc_channels;
mod kernel_stacks;
mod memory;
mod permission_validation;
mod qemu_exit;
mod runtime_loader;
mod scheduler;
mod serial;
mod service_identity;
mod service_manager;
mod shared_memory;
mod shell_apps;
mod shell_objects;
mod software_renderer;
mod storage_journal;
mod storage_service;
mod system_api;
mod tasks;
mod value_validation;
mod widgets;
mod window_interaction;

#[cfg(not(test))]
use core::panic::PanicInfo;
use pythos_shared::boot_protocol::PythBootInfo;

/// PythCore native entry point.
///
/// # Safety
///
/// The caller must enter from the PythOS loader after firmware handoff setup.
/// `boot_info` must point to a valid `PythBootInfo` structure for the duration
/// of early core initialization. The bootstrap stack, page mappings, direction
/// flag state, interrupt state, and COM1 availability must match the kernel
/// entry contract in `docs/PythOS-TDD-001.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pythcore_entry(boot_info: *const PythBootInfo) -> ! {
    serial::write_line("PYTHOS:CORE:ENTER");

    // SAFETY:
    // 1. Invariant: `boot_info` is the `RDI` argument the loader passed and
    //    stays mapped readable through the loader-built page tables.
    // 2. Established by: the loader handoff contract in `docs/PythOS-TDD-001.md`.
    // 3. Lifetime: valid for all of early core initialization.
    // 4. Pointer ownership: PythCore owns the allocation after entry.
    // 5. Alignment: checked inside `boot_info::validate`.
    // 6. Mapped length: one full `PythBootInfo` allocated by the loader.
    // 7. Concurrency: single-core execution with interrupts disabled.
    // 8. Violation: an invalid pointer faults with no handler and hangs.
    let boot_info = match unsafe { boot_info::validate(boot_info) } {
        Ok(info) => info,
        Err(()) => {
            serial::write_line("PYTHOS:CORE:BOOTINFO_INVALID");
            qemu_exit::panic();
        }
    };
    serial::write_line("PYTHOS:CORE:BOOTINFO_VALID");

    #[cfg_attr(test, allow(unused_mut, unused_variables))]
    let mut physical_memory = match memory::physical::initialize(boot_info) {
        Ok(memory) => memory,
        Err(_) => {
            serial::write_line("PYTHOS:CORE:MEMORY_INVALID");
            qemu_exit::panic();
        }
    };
    serial::write_line("PYTHOS:CORE:MEMORY_READY");

    if architecture::x86_64::gdt::initialize().is_err() {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
    serial::write_line("PYTHOS:CORE:GDT_READY");

    if architecture::x86_64::idt::initialize().is_err() {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
    serial::write_line("PYTHOS:CORE:IDT_READY");
    serial::write_line("PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY");

    #[cfg(not(test))]
    {
        if !architecture::x86_64::exceptions::verify_entry_hardening() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED");

        if architecture::x86_64::interrupts::initialize().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:INTERRUPTS_READY");

        let address_space =
            match memory::r#virtual::KernelAddressSpace::build(&mut physical_memory, boot_info) {
                Ok(address_space) => address_space,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
        // SAFETY:
        // 1. Invariant: `address_space` maps the currently executing PythCore
        //    code, active bootstrap stack, boot metadata, framebuffer, COM1 code
        //    path, and page-table frames required for validation.
        // 2. Established by: successful `KernelAddressSpace::build` above.
        // 3. Lifetime: the page tables are intentionally retained for this slice.
        // 4. Pointer ownership: PythCore owns the newly allocated page tables.
        // 5. Alignment: table root was allocated as a 4 KiB physical page.
        // 6. Mapped length: the full active early-core address surface is mapped.
        // 7. Concurrency: single-core execution with interrupts disabled.
        // 8. Violation: execution faults immediately after the CR3 switch.
        unsafe {
            address_space.activate();
        }
        if address_space.validate_active(boot_info).is_err() {
            serial::write_line("PYTHOS:CORE:MEMORY_INVALID");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:VM_READY");
        if memory::r#virtual::prove_old_identity_map_removed().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:IDENTITY_MAP_REMOVED");
        if boot_metadata::validate_complete(boot_info).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:BOOTINFO_COMPLETE");
        if architecture::x86_64::timer::initialize().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:TIMER_READY");
        if architecture::x86_64::clock::initialize().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:CLOCK_READY");
        if tasks::initialize(boot_info).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:TASKS_READY");
        if kernel_stacks::initialize(boot_info).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:KERNEL_STACKS_READY");
        if context_switch::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:CONTEXT_SWITCH_READY");
        if scheduler::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:SCHEDULER_READY");
        if scheduler::run_idle_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:IDLE_TASK_READY");
        if scheduler::run_preemption_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:PREEMPT_READY");
        if scheduler::run_task_termination_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:TASK_TERMINATION_READY");
        if scheduler::run_scheduler_acceptance_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:SCHEDULER_TESTS_READY");
        if service_identity::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:SERVICE_IDENTITY_READY");
        if ipc_channels::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:IPC_CHANNELS_READY");
        if ipc_channels::run_bounded_queue_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:BOUNDED_QUEUES_READY");
        if ipc_channels::run_request_reply_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:REQUEST_REPLY_READY");
        if capabilities::run_capability_handle_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:CAPABILITY_HANDLES_READY");
        if shared_memory::run_shared_memory_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:SHARED_MEMORY_HANDLES_READY");
        if permission_validation::run_permission_validation_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:PERMISSION_VALIDATION_READY");
        if capabilities::run_revocation_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:REVOCATION_READY");
        if capabilities::run_negative_authorization_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:NEGATIVE_AUTHORIZATION_READY");
        if audit::run_audit_logging_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:AUDIT_LOGGING_READY");
        serial::write_line("PYTHOS:CORE:PHASE_3_COMPLETE");
        serial::write_line("PYTHOS:CORE:RUNTIME_SELECTED");
        let runtime_payload = match runtime_loader::load_init_payload(boot_info) {
            Ok(payload) => payload,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        serial::write_line("PYTHOS:CORE:INIT_PAK_LOADED");
        let runtime_instance = match interpreter::boot(runtime_payload.source) {
            Ok(instance) => instance,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        serial::write_line("PYTHOS:CORE:INTERPRETER_BOOTED");
        if system_api::run_log_surface(&runtime_instance).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:SYSTEM_API_READY");
        if value_validation::run_self_test(&runtime_instance).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:VALUE_VALIDATION_READY");
        if service_manager::run_self_test(&runtime_instance).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:SERVICE_MANAGER_READY");
        if service_manager::run_exception_containment_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED");
        if service_manager::run_service_restart_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:SERVICE_RESTART_READY");
        if service_manager::run_async_events_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:ASYNC_EVENTS_READY");
        if input_drivers::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:INPUT_DRIVERS_READY");
        if input_events::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:INPUT_EVENT_SERVICE_READY");
        if software_renderer::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:SOFTWARE_RENDERER_READY");
        if font_system::run_self_test(boot_info).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:FONT_SYSTEM_READY");
        if compositor::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:COMPOSITOR_READY");
        if window_interaction::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        if widgets::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:WIDGETS_READY");
        if shell_apps::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:PHASE_5_COMPLETE");
        let audio_device = match audio::select_device() {
            Ok(device) => device,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        serial::write_line("PYTHOS:CORE:AUDIO_DEVICE_SELECTION_READY");
        let audio_driver = match audio::initialize_driver(audio_device) {
            Ok(driver) => driver,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        serial::write_line("PYTHOS:CORE:AUDIO_DRIVER_READY");
        let audio_buffers = match audio::initialize_buffers(audio_driver) {
            Ok(buffers) => buffers,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        serial::write_line("PYTHOS:CORE:AUDIO_BUFFERS_READY");
        let pcm_playback = match audio::play_fixed_pcm(audio_driver, audio_buffers) {
            Ok(playback) => playback,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        serial::write_line("PYTHOS:CORE:PCM_PLAYBACK_READY");
        if audio::mix_boot_audio(audio_driver, audio_buffers, pcm_playback).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:AUDIO_MIXING_READY");
        let _boot_assets = match boot_assets::load_assets() {
            Ok(assets) => assets,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        serial::write_line("PYTHOS:CORE:BOOT_ASSETS_READY");
        if cinematic_boot::run_synced_sequence(_boot_assets, &boot_info.framebuffer).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY");
        if audio::complete_graceful_fallback(audio_device).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:GRACEFUL_AUDIO_FALLBACK_READY");
        serial::write_line("PYTHOS:CORE:PHASE_6_COMPLETE");
        let _block_device = match block_device::select_device() {
            Ok(device) => device,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        serial::write_line("PYTHOS:CORE:BLOCK_DEVICE_READY");
        if storage_service::run_self_test(_block_device).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:STORAGE_SERVICE_READY");
        if storage_journal::run_self_test(_block_device).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY");
        if storage_journal::run_commit_marker_self_test(_block_device).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_READY");
    }

    #[cfg(test)]
    {
        if framebuffer::render_boot_screen(&boot_info.framebuffer).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
    }
    serial::write_line("PYTHOS:CORE:FRAMEBUFFER_READY");
    serial::write_line("PYTHOS:CORE:MILESTONE_1_COMPLETE");
    qemu_exit::success();
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial::write_line("PYTHOS:PANIC");
    qemu_exit::panic();
}
