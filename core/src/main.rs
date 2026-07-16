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
mod object_browser;
mod object_relationships;
mod permission_validation;
mod persistent_objects;
mod process;
mod qemu_exit;
mod resource_quotas;
mod revision_history;
mod runtime_loader;
mod scheduler;
mod serial;
mod service_identity;
mod service_manager;
mod service_runtimes;
mod shared_memory;
mod shell_apps;
mod shell_objects;
mod software_renderer;
mod storage_journal;
mod storage_service;
mod syscall;
mod system_api;
mod tasks;
mod typed_object_format;
mod user_mode;
mod user_stacks;
mod value_validation;
mod widgets;
mod window_interaction;
mod workspace_objects;

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
        let user_address_space =
            match memory::r#virtual::UserAddressSpace::build(&mut physical_memory, boot_info) {
                Ok(address_space) => address_space,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
        let service_runtime_address_space_a =
            match memory::r#virtual::UserAddressSpace::build(&mut physical_memory, boot_info) {
                Ok(address_space) => address_space,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
        let service_runtime_address_space_b =
            match memory::r#virtual::UserAddressSpace::build(&mut physical_memory, boot_info) {
                Ok(address_space) => address_space,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
        let process_address_space =
            match memory::r#virtual::UserAddressSpace::build(&mut physical_memory, boot_info) {
                Ok(address_space) => address_space,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
        if user_address_space
            .validate_isolated_from(&address_space)
            .is_err()
        {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        if service_runtime_address_space_a
            .validate_isolated_from(&address_space)
            .is_err()
            || service_runtime_address_space_b
                .validate_isolated_from(&address_space)
                .is_err()
            || process_address_space
                .validate_isolated_from(&address_space)
                .is_err()
        {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
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
        if storage_journal::run_crash_recovery_self_test(_block_device).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:CRASH_RECOVERY_READY");
        if typed_object_format::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY");
        if object_relationships::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY");
        if revision_history::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:REVISION_HISTORY_READY");
        if workspace_objects::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:WORKSPACE_OBJECTS_READY");
        if object_browser::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:OBJECT_BROWSER_READY");
        if let Err(error) = persistent_objects::run_self_test(_block_device) {
            persistent_objects::write_error(error);
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:PHASE_7_COMPLETE");
        if user_mode::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:RING3_EXECUTION_READY");
        serial::write_line("PYTHOS:CORE:ADDRESS_SPACE:CREATED");
        serial::write_line("PYTHOS:CORE:ADDRESS_SPACE:ISOLATED");
        // SAFETY:
        // 1. Invariant: `user_address_space` is a distinct PML4 root whose
        //    supervisor mappings cover the current kernel execution path and
        //    whose user mappings cover only the fixed proof code/stack pages.
        // 2. Established by: `UserAddressSpace::build` and
        //    `validate_isolated_from` before the first PythCore CR3 switch.
        // 3. Lifetime: both kernel and user roots are retained for this proof.
        // 4. Pointer ownership: the CPU borrows the user page-table hierarchy.
        // 5. Alignment: the root was allocated as a 4 KiB physical page.
        // 6. Mapped length: the hierarchy maps the active stack, kernel path,
        //    and user proof pages.
        // 7. Concurrency: single-core Phase 8 proof with interrupts disabled.
        // 8. Violation: missing mappings fault through the diagnostic path.
        unsafe {
            user_address_space.activate();
        }
        serial::write_line("PYTHOS:CORE:ADDRESS_SPACE:SWITCHED");
        let isolated_user_result = user_mode::run_self_test();
        // SAFETY:
        // 1. Invariant: `address_space` is the validated kernel root that was
        //    active before the isolated user proof.
        // 2. Established by: the earlier successful `activate` and
        //    `validate_active` calls.
        // 3. Lifetime: the kernel root remains retained for this whole boot.
        // 4. Pointer ownership: the CPU borrows the kernel page-table hierarchy.
        // 5. Alignment: the root was allocated as a 4 KiB physical page.
        // 6. Mapped length: the full active early-core address surface is mapped.
        // 7. Concurrency: single-core proof with interrupts disabled.
        // 8. Violation: failure to restore would leave later kernel work under
        //    the isolated proof root.
        unsafe {
            address_space.activate();
        }
        if isolated_user_result.is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:ADDRESS_SPACE:RESTORED");
        serial::write_line("PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY");
        // SAFETY:
        // 1. Invariant: the user proof root maps the fixed user syscall code
        //    and stack as user-accessible while keeping PythCore text/data
        //    supervisor-only for the syscall handler.
        // 2. Established by: `UserAddressSpace::build` and the completed
        //    separate-address-spaces validation above.
        // 3. Lifetime: both roots remain retained for the whole Phase 8 proof.
        // 4. Pointer ownership: the CPU borrows the user page-table hierarchy.
        // 5. Alignment: the root was allocated as a 4 KiB physical page.
        // 6. Mapped length: the hierarchy maps the active stack, syscall
        //    handler path, and fixed user proof pages.
        // 7. Concurrency: single-core Phase 8 proof with interrupts disabled.
        // 8. Violation: missing mappings fault through the diagnostic path.
        unsafe {
            user_address_space.activate();
        }
        let syscall_result = syscall::run_self_test();
        // SAFETY:
        // 1. Invariant: `address_space` is the validated kernel root used by
        //    the remaining boot path after the syscall proof completes.
        // 2. Established by: successful VM activation and validation earlier.
        // 3. Lifetime: the kernel root remains retained for this whole boot.
        // 4. Pointer ownership: the CPU borrows the kernel page-table hierarchy.
        // 5. Alignment: the root was allocated as a 4 KiB physical page.
        // 6. Mapped length: the full active early-core address surface is mapped.
        // 7. Concurrency: single-core proof with interrupts disabled.
        // 8. Violation: failure to restore leaves later kernel work under the
        //    syscall proof root.
        unsafe {
            address_space.activate();
        }
        if syscall_result.is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:SYSCALL_ENTRY_READY");
        if user_stacks::run_self_test().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:USER_STACK:ALLOCATED");
        // SAFETY:
        // 1. Invariant: the user root maps usable user stack pages with user
        //    access and keeps each guard page supervisor-only.
        // 2. Established by: the static user-stack pool layout and validated
        //    immediately after this CR3 switch while the user root's own table
        //    frames are mapped.
        // 3. Lifetime: the root and stack pool remain retained for this proof.
        // 4. Pointer ownership: the CPU borrows the user page-table hierarchy
        //    and uses the selected user stack while running CPL3 code.
        // 5. Alignment: the root and stack pages are 4 KiB aligned.
        // 6. Mapped length: the hierarchy maps the active kernel path, fixed
        //    user proof page, and guarded user stack pages.
        // 7. Concurrency: single-core Phase 8 proof with one active user stack.
        // 8. Violation: bad stack mappings fault through the diagnostic path.
        unsafe {
            user_address_space.activate();
        }
        let stack_protection_result = user_address_space.validate_user_stack_protections();
        if stack_protection_result.is_ok() {
            serial::write_line("PYTHOS:CORE:USER_STACK:GUARD_PAGE");
        }
        let guarded_stack_result = if stack_protection_result.is_ok() {
            user_mode::run_self_test()
        } else {
            Err(user_mode::UserModeError::DidNotReturn)
        };
        // SAFETY:
        // 1. Invariant: `address_space` is the validated kernel root required
        //    for the remaining boot path after the user-stack proof.
        // 2. Established by: successful VM activation and validation earlier.
        // 3. Lifetime: the kernel root remains retained for this whole boot.
        // 4. Pointer ownership: the CPU borrows the kernel page-table hierarchy.
        // 5. Alignment: the root was allocated as a 4 KiB physical page.
        // 6. Mapped length: the full active early-core address surface is mapped.
        // 7. Concurrency: single-core proof with interrupts disabled.
        // 8. Violation: failure to restore leaves later kernel work under the
        //    user proof root.
        unsafe {
            address_space.activate();
        }
        if stack_protection_result.is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        if guarded_stack_result.is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:USER_STACKS_READY");
        let service_runtime_proof = match service_runtimes::run_self_test(
            runtime_payload.source,
            service_runtime_address_space_a.root_table_phys(),
            service_runtime_address_space_b.root_table_phys(),
        ) {
            Ok(proof) => proof,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        if service_runtime_proof.local_instances {
            serial::write_line("PYTHOS:CORE:RUNTIME:LOCAL_INSTANCE");
        }
        if service_runtime_proof.address_spaces_isolated {
            serial::write_line("PYTHOS:CORE:RUNTIME:ADDRESS_SPACE");
        }
        if service_runtime_proof.state_isolated {
            serial::write_line("PYTHOS:CORE:RUNTIME:STATE_ISOLATED");
        }
        if !service_runtime_proof.local_instances
            || !service_runtime_proof.address_spaces_isolated
            || !service_runtime_proof.state_isolated
        {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY");
        let guarded_shared_memory_proof = match shared_memory::run_guarded_phase8_self_test(
            service_runtime_address_space_a.root_table_phys(),
            service_runtime_address_space_b.root_table_phys(),
        ) {
            Ok(proof) => proof,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        if guarded_shared_memory_proof.ring3_read {
            serial::write_line("PYTHOS:CORE:SHM:RING3_READ");
        }
        if guarded_shared_memory_proof.cross_space_write_denied {
            serial::write_line("PYTHOS:CORE:SHM:CROSS_SPACE_WRITE_DENIED");
        }
        if !guarded_shared_memory_proof.ring3_read
            || !guarded_shared_memory_proof.cross_space_write_denied
        {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY");
        let process_root = process_address_space.root_table_phys();
        let process_table_frame_count = process_address_space.table_frame_count();
        let free_pages_before_process_reclaim = physical_memory.free_pages;
        let reclaimed_frame_count = match process_address_space.reclaim(&mut physical_memory) {
            Ok(count) => count,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        let expected_free_pages =
            match free_pages_before_process_reclaim.checked_add(reclaimed_frame_count as u64) {
                Some(count) => count,
                None => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
        if reclaimed_frame_count != process_table_frame_count
            || physical_memory.free_pages != expected_free_pages
        {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        let process_termination_proof = match process::run_termination_self_test(
            process_root,
            process_table_frame_count,
            reclaimed_frame_count,
        ) {
            Ok(proof) => proof,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        if process_termination_proof.terminated {
            serial::write_line("PYTHOS:CORE:PROCESS:TERMINATED");
        }
        if process_termination_proof.unschedulable {
            serial::write_line("PYTHOS:CORE:PROCESS:UNSCHEDULABLE");
        }
        if process_termination_proof.address_space_reclaimed {
            serial::write_line("PYTHOS:CORE:PROCESS:ADDRESS_SPACE_RECLAIMED");
        }
        if !process_termination_proof.terminated
            || !process_termination_proof.unschedulable
            || !process_termination_proof.address_space_reclaimed
        {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:PROCESS_TERMINATION_READY");
        let memory_quota_proof = match resource_quotas::run_memory_self_test() {
            Ok(proof) => proof,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        if memory_quota_proof.allocation_granted {
            serial::write_line("PYTHOS:CORE:QUOTA:MEMORY_GRANTED");
        }
        if memory_quota_proof.allocation_denied {
            serial::write_line("PYTHOS:CORE:QUOTA:MEMORY_DENIED");
        }
        if !memory_quota_proof.allocation_granted || !memory_quota_proof.allocation_denied {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:MEMORY_QUOTAS_READY");
        let cpu_quota_proof = match resource_quotas::run_cpu_self_test() {
            Ok(proof) => proof,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        if cpu_quota_proof.tick_recorded {
            serial::write_line("PYTHOS:CORE:QUOTA:CPU_TICK");
        }
        if cpu_quota_proof.throttle_denied {
            serial::write_line("PYTHOS:CORE:QUOTA:CPU_THROTTLED");
        }
        if !cpu_quota_proof.tick_recorded || !cpu_quota_proof.throttle_denied {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:CPU_QUOTAS_READY");
        // SAFETY:
        // 1. Invariant: the user root maps the fixed user fault probe with
        //    user access while keeping kernel text/data supervisor-only.
        // 2. Established by: `UserAddressSpace::build` and the completed
        //    separate-address-spaces validation earlier in Phase 8.
        // 3. Lifetime: both the user and kernel roots are retained for the
        //    whole boot path.
        // 4. Pointer ownership: the CPU borrows the user page-table hierarchy
        //    during the one-shot fault probe.
        // 5. Alignment: the root was allocated as a 4 KiB physical page.
        // 6. Mapped length: the hierarchy maps the active kernel path, trap
        //    stack, and fixed user proof pages.
        // 7. Concurrency: single-core Phase 8 proof with one user fault probe.
        // 8. Violation: bad mappings fault through the diagnostic path.
        unsafe {
            user_address_space.activate();
        }
        let crash_fault_result = user_mode::run_illegal_instruction_fault_test();
        // SAFETY:
        // 1. Invariant: `address_space` is the validated kernel root required
        //    for the remaining boot path after the user fault probe.
        // 2. Established by: successful VM activation and validation earlier.
        // 3. Lifetime: the kernel root remains retained for this whole boot.
        // 4. Pointer ownership: the CPU borrows the kernel page-table hierarchy.
        // 5. Alignment: the root was allocated as a 4 KiB physical page.
        // 6. Mapped length: the full active early-core address surface is mapped.
        // 7. Concurrency: single-core proof with interrupts disabled.
        // 8. Violation: failure to restore leaves later kernel work under the
        //    user fault proof root.
        unsafe {
            address_space.activate();
        }
        if crash_fault_result.is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        let crash_containment_proof = match process::run_crash_containment_self_test(
            process::UserFault::IllegalInstruction,
        ) {
            Ok(proof) => proof,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        if crash_containment_proof.faulting_service_terminated {
            serial::write_line("PYTHOS:CORE:CRASH:SERVICE_TERMINATED");
        }
        if crash_containment_proof.peer_service_alive {
            serial::write_line("PYTHOS:CORE:CRASH:PEER_ALIVE");
        }
        if !crash_containment_proof.user_fault_diagnosed
            || !crash_containment_proof.faulting_service_terminated
            || !crash_containment_proof.peer_service_alive
        {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:CRASH_CONTAINMENT_READY");
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
