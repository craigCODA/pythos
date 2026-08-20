//! Normal (non-verification) boot path (ADR 0052).
//!
//! Skips the verification proof sequence entirely and constructs only the
//! production substrate a running system needs, initializes COM2 (the
//! interactive object-shell transport), initializes the retained object service,
//! then enters `shell.elf` as the persistent ring-3 program.

use crate::memory::physical::PhysicalMemory;
#[cfg(feature = "pythtig-phase2-test")]
use crate::pyth_runtime_launch;
use crate::{
    audio, boot_assets, cinematic_boot, framebuffer, launcher_screen, normal_init,
    process_context::ActiveUserProcess, ps2, qemu_exit, retained_services, serial,
};
use crate::{shell_objects::ObjectKind, syscall, user_mode};
use pythos_shared::boot_protocol::{PythBootInfo, PythFramebufferInfo};
#[cfg(feature = "pythtig-phase2-test")]
use pythos_shared::object_shell_abi::PackedCapability;
use pythos_shared::object_shell_abi::{
    BootstrapCapabilityBlock, MAX_SHELL_OBJECT_CAPS, OBJECT_SHELL_ABI_MAJOR,
    OBJECT_SHELL_ABI_MINOR, ObjectListEntry, SHELL_BOOTSTRAP_MAGIC,
};

#[cfg(not(test))]
pub fn run(boot_info: &'static PythBootInfo, physical_memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT:FAST_PATH");
    let substrate = match normal_init::initialize_normal_substrate(boot_info, physical_memory) {
        Ok(substrate) => substrate,
        Err(_) => {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
    };
    let _ = &substrate.kernel_address_space;
    #[cfg(feature = "pythtig-phase2-test")]
    let pyth_graph_mode =
        match pyth_runtime_launch::read_and_clear_pyth_graph_control_sector(substrate.block_device)
        {
            Ok(mode) => mode,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
    if retained_services::initialize_object_service_from_device(substrate.block_device).is_err() {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY");

    #[cfg(feature = "pythtig-phase2-test")]
    match pyth_graph_mode {
        pyth_runtime_launch::PythGraphBootMode::LaunchHello => {
            let Some(launch) = substrate.pyth_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            launch_pyth_graph_runtime(launch);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchBudget => {
            let Some(launch) = substrate.pyth_budget_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            launch_pyth_graph_runtime(launch);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchInvalid => {
            let Some(code) = substrate.pyth_invalid_graph_rejection else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            pyth_runtime_launch::emit_package_rejected_marker(code);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchUnsupported => {
            let Some(code) = substrate.pyth_unsupported_graph_rejection else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            pyth_runtime_launch::emit_package_rejected_marker(code);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchInvalidString => {
            let Some(code) = substrate.pyth_invalid_string_graph_rejection else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            pyth_runtime_launch::emit_package_rejected_marker(code);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchParameterized => {
            let Some(code) = substrate.pyth_parameterized_graph_rejection else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            pyth_runtime_launch::emit_package_rejected_marker(code);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchObjectCreate => {
            let Some(launch) = substrate.pyth_object_create_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            let capability = graph_workspace_capability(launch.process);
            launch_pyth_graph_runtime_with_deferred_import(launch, capability);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchObjectRestore => {
            let Some(launch) = substrate.pyth_object_restore_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            let capability = graph_workspace_capability(launch.process);
            launch_pyth_graph_runtime_with_deferred_import(launch, capability);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchObjectKnownDenied => {
            let Some(launch) = substrate.pyth_object_known_denied_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            let capability = graph_workspace_capability(launch.process);
            launch_pyth_graph_runtime_with_deferred_import(launch, capability);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchObjectForgery => {
            let Some(launch) = substrate.pyth_object_forgery_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            let capability = copied_shell_object_capability_for_forgery();
            pyth_runtime_launch::arm_object_flow_completion_marker();
            launch_pyth_graph_runtime_with_deferred_import(launch, capability);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchTaskSteward => {
            let Some(launch) = substrate.pyth_task_steward_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            let capability = retained_services::with_task_service(|service| {
                service.grant_steward_proposal_capability(launch.process)
            })
            .map_err(|_| ());
            launch_pyth_graph_runtime_with_deferred_import(launch, capability);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchNativeHello => {
            let Some(launch) = substrate.pyth_native_hello_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            launch_pyth_graph_runtime(launch);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchNativeBudget => {
            let Some(launch) = substrate.pyth_native_budget_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            launch_pyth_graph_runtime(launch);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchNativeObjectCreate => {
            let Some(launch) = substrate.pyth_native_object_create_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            let capability = graph_workspace_capability(launch.process);
            launch_pyth_graph_runtime_with_deferred_import(launch, capability);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchNativeObjectRestore => {
            let Some(launch) = substrate.pyth_native_object_restore_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            let capability = graph_workspace_capability(launch.process);
            launch_pyth_graph_runtime_with_deferred_import(launch, capability);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchNativeObjectKnownDenied => {
            let Some(launch) = substrate
                .pyth_native_object_known_denied_runtime_launch
                .as_ref()
            else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            let capability = graph_workspace_capability(launch.process);
            launch_pyth_graph_runtime_with_deferred_import(launch, capability);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchNativeObjectForgery => {
            let Some(launch) = substrate.pyth_native_object_forgery_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            let capability = copied_shell_object_capability_for_forgery();
            pyth_runtime_launch::arm_object_flow_completion_marker();
            launch_pyth_graph_runtime_with_deferred_import(launch, capability);
        }
        pyth_runtime_launch::PythGraphBootMode::LaunchNativeTaskSteward => {
            let Some(launch) = substrate.pyth_native_task_steward_runtime_launch.as_ref() else {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            };
            let capability = retained_services::with_task_service(|service| {
                service.grant_steward_proposal_capability(launch.process)
            })
            .map_err(|_| ());
            launch_pyth_graph_runtime_with_deferred_import(launch, capability);
        }
        pyth_runtime_launch::PythGraphBootMode::DefaultShell => {}
    }

    serial::init_com2();
    serial::write_line("PYTHOS:CORE:COM2_READY");

    if play_boot_cinematic_and_audio(&boot_info.framebuffer).is_err() {
        serial::write_line("PYTHOS:CORE:NORMAL_BOOT:AUDIO_VISUAL_SKIPPED");
    }

    // ADR 0053, Task D: render the launcher tile, then block in kernel mode
    // (still on the kernel's own CR3) until a real click lands on it. If the
    // PS/2 controller itself fails to come up (e.g. a QEMU profile without
    // PS/2 emulated), degrade to auto-launching immediately rather than
    // hanging normal boot forever waiting for input nothing can deliver.
    let _ = framebuffer::render_launcher_screen(&boot_info.framebuffer, 0, 0);
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY");
    if ps2::initialize().is_ok() {
        if launcher_screen::run_until_click(&boot_info.framebuffer).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
    } else {
        serial::write_line("PYTHOS:CORE:NORMAL_BOOT:PS2_INIT_FAILED");
    }

    let shell_process = match build_shell_process(&substrate.shell_launch) {
        Ok(process) => process,
        Err(_) => {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
    };
    let bootstrap = match build_bootstrap_block(shell_process) {
        Ok(block) => block,
        Err(_) => {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
    };
    substrate.shell_launch.write_bootstrap_block(&bootstrap);

    serial::write_line("PYTHOS:CORE:NORMAL_SERVICES_READY");
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT_ALIVE");

    // SAFETY:
    // 1. Invariant: the retained shell address space maps validated shell ELF
    //    pages, guarded user stack pages, the read-only bootstrap block, and
    //    supervisor-only kernel fault/syscall paths.
    // 2. Established by: `initialize_normal_substrate` builds and validates the
    //    shell root before activating the normal kernel root.
    // 3. Lifetime: the root and backing frames are intentionally retained for
    //    the persistent shell process.
    // 4. Pointer ownership: the CPU borrows the shell page-table hierarchy.
    // 5. Alignment: the root was allocated as a 4 KiB page-table frame.
    // 6. Mapped length: one complete hierarchy covers shell ELF, stack,
    //    bootstrap block, kernel trap path, and page-table frames.
    // 7. Concurrency: ADR 0051 normal boot is single-core with one shell.
    // 8. Violation: incomplete mappings fault through shell containment.
    unsafe {
        substrate.shell_launch.address_space.activate();
    }
    user_mode::enter_persistent_user_process(
        shell_process,
        substrate.shell_launch.entry,
        substrate.shell_launch.user_stack_top(),
        substrate.shell_launch.bootstrap_user_ptr,
    );
}

#[cfg(all(not(test), feature = "pythtig-phase2-test"))]
fn launch_pyth_graph_runtime(launch: &pyth_runtime_launch::PreparedPythRuntimeLaunch) -> ! {
    launch_pyth_graph_runtime_with_deferred_import(launch, Ok(PackedCapability::from_raw(0)))
}

#[cfg(all(not(test), feature = "pythtig-phase2-test"))]
fn launch_pyth_graph_runtime_with_deferred_import(
    launch: &pyth_runtime_launch::PreparedPythRuntimeLaunch,
    deferred_capability: Result<PackedCapability, ()>,
) -> ! {
    let deferred_capability = match deferred_capability {
        Ok(capability) => capability,
        Err(_) => {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
    };
    // SAFETY:
    // 1. Invariant: the retained graph runtime address space maps the
    //    validated runtime ELF, guarded stack, read-only bootstrap/package
    //    pages, writable result page, a supervisor-only bootstrap alias, and
    //    kernel syscall/fault paths.
    // 2. Established by: `initialize_normal_substrate` builds and validates
    //    this root before activating the normal kernel root.
    // 3. Lifetime: the root and payload frames are retained for this one-shot
    //    graph runtime invocation.
    // 4. Pointer ownership: the CPU borrows the graph page-table root.
    // 5. Alignment: the root was allocated as a 4 KiB page-table frame.
    // 6. Mapped length: one complete hierarchy covers runtime ELF, stack,
    //    bootstrap, package, result, trap stack, syscall path, and the
    //    supervisor-only bootstrap alias.
    // 7. Concurrency: Phase 3 graph runtime launches one process on one CPU.
    // 8. Violation: incomplete mappings fault through user containment.
    unsafe {
        launch.address_space.activate();
    }
    if launch
        .bind_deferred_import_after_activation(deferred_capability)
        .is_err()
    {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
    pyth_runtime_launch::emit_native_elf_valid_marker(launch);
    pyth_runtime_launch::emit_package_valid_marker(launch);
    pyth_runtime_launch::emit_bootstrap_bound_marker(launch);
    match launch.execution_kind {
        pyth_runtime_launch::PythGraphExecutionKind::Interpreter => {
            user_mode::enter_pyth_graph_runtime(
                launch.process,
                launch.entry,
                launch.user_stack_top(),
                launch.bootstrap_user_ptr,
                launch.package_digest,
            );
        }
        pyth_runtime_launch::PythGraphExecutionKind::Native => {
            user_mode::enter_pyth_native_graph(
                launch.process,
                launch.entry,
                launch.user_stack_top(),
                launch.bootstrap_user_ptr,
                launch.package_digest,
            );
        }
    }
}

#[cfg(all(not(test), feature = "pythtig-phase2-test"))]
fn graph_workspace_capability(process: ActiveUserProcess) -> Result<PackedCapability, ()> {
    retained_services::with_object_service(|service| service.grant_workspace_capability(process))
        .map_err(|_| ())?
        .map_err(|_| ())
}

#[cfg(all(not(test), feature = "pythtig-phase2-test"))]
fn copied_shell_object_capability_for_forgery() -> Result<PackedCapability, ()> {
    let capability = retained_services::with_object_service(|service| {
        let shell = service.shell_caller();
        let workspace = service.shell_workspace_capability();
        service
            .query_objects(shell, workspace, ObjectKind::Note)
            .map(|entries| entries[0].capability)
    })
    .map_err(|_| ())?
    .map_err(|_| ())?;
    if capability.raw() == 0 {
        return Err(());
    }
    Ok(capability)
}

/// Play the boot cinematic and AC97 audio (ADR 0053), reusing the verify
/// path's exact call order (`main.rs`'s Phase 6 sequence) and marker names.
/// Unlike the verify path's fail-fast proof harness, failures here are
/// soft-skipped: a machine with no audio device is a legitimate QEMU
/// configuration and must not block reaching the shell.
//
// TODO(stretch, HDA): normal boot only wires the AC97 pipeline. HDA needs its
// MMIO window mapped into the kernel address space via
// `KernelAddressSpace::build`'s `hda_mmio` parameter, which
// `normal_init.rs::initialize_normal_substrate` currently hardcodes to
// `None`. Reintroducing HDA here means mirroring `main.rs:164-168` (probe
// before `KernelAddressSpace::build`) and `main.rs:525-543` (init/enumerate/
// start after activation) into `normal_init.rs`/`normal_boot.rs`.
// Deliberately distinct marker names from the verify path's Phase 6 sequence
// (`AUDIO_DEVICE_SELECTION_READY` etc. in `main.rs`): `test-normal-fast-boot.py`
// already asserts those exact verify-only names never appear in a normal
// boot's serial log, as the oracle proving the two boot paths stay distinct.
// Reusing them here would both break that assertion and make normal-boot and
// verify-boot output indistinguishable in a shared log.
#[cfg(not(test))]
fn play_boot_cinematic_and_audio(framebuffer: &PythFramebufferInfo) -> Result<(), ()> {
    let audio_device = audio::select_device().map_err(|_| ())?;
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT:AUDIO_DEVICE_SELECTION_READY");
    let audio_driver = audio::initialize_driver(audio_device).map_err(|_| ())?;
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT:AUDIO_DRIVER_READY");
    let audio_buffers = audio::initialize_buffers(audio_driver).map_err(|_| ())?;
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT:AUDIO_BUFFERS_READY");
    let pcm_playback = audio::play_fixed_pcm(audio_driver, audio_buffers).map_err(|_| ())?;
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT:PCM_PLAYBACK_READY");
    audio::mix_boot_audio(audio_driver, audio_buffers, pcm_playback).map_err(|_| ())?;
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT:AUDIO_MIXING_READY");
    let assets = boot_assets::load_assets().map_err(|_| ())?;
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT:BOOT_ASSETS_READY");
    cinematic_boot::run_synced_sequence(assets, framebuffer).map_err(|_| ())?;
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT:AUDIO_VISUAL_SYNC_READY");
    audio::complete_graceful_fallback(audio_device).map_err(|_| ())?;
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT:GRACEFUL_AUDIO_FALLBACK_READY");
    Ok(())
}

#[cfg(not(test))]
fn build_shell_process(launch: &normal_init::PreparedShellLaunch) -> Result<ActiveUserProcess, ()> {
    let service_caller =
        retained_services::with_object_service(|service| service.shell_caller()).map_err(|_| ())?;
    if service_caller.principal_id() != launch.principal_id {
        return Err(());
    }
    ActiveUserProcess::from_validated_launch(
        service_caller.service_id(),
        launch.principal_id,
        launch.program_digest,
        &launch.image,
        launch.stack_region,
        launch.bootstrap_user_ptr,
    )
    .map_err(|_| ())
}

#[cfg(not(test))]
fn build_bootstrap_block(process: ActiveUserProcess) -> Result<BootstrapCapabilityBlock, ()> {
    let console = syscall::grant_console_capability(process).map_err(|_| ())?;
    let system_control = syscall::grant_system_control_capability(process).map_err(|_| ())?;
    let task_control =
        retained_services::with_task_service(|service| service.user_task_control_capability())
            .map_err(|_| ())?;
    let (workspace, objects) = retained_services::with_object_service(|service| {
        let workspace = service.shell_workspace_capability();
        service
            .query_objects(process, workspace, ObjectKind::Note)
            .map(|objects| (workspace, objects))
    })
    .map_err(|_| ())?
    .map_err(|_| ())?;
    let object_count = count_bootstrap_objects(&objects)?;
    Ok(BootstrapCapabilityBlock {
        magic: SHELL_BOOTSTRAP_MAGIC,
        abi_major: OBJECT_SHELL_ABI_MAJOR,
        abi_minor: OBJECT_SHELL_ABI_MINOR,
        object_count,
        reserved0: 0,
        console,
        workspace,
        system_control,
        task_control,
        objects,
    })
}

#[cfg(not(test))]
fn count_bootstrap_objects(objects: &[ObjectListEntry; MAX_SHELL_OBJECT_CAPS]) -> Result<u16, ()> {
    let count = objects.iter().filter(|entry| entry.object_id != 0).count();
    u16::try_from(count).map_err(|_| ())
}
