//! Normal (non-verification) boot path (ADR 0052).
//!
//! Skips the verification proof sequence entirely and constructs only the
//! production substrate a running system needs, initializes COM2 (the
//! interactive object-shell transport), initializes the retained object service,
//! then enters `shell.elf` as the persistent ring-3 program.

use crate::memory::physical::PhysicalMemory;
use crate::{
    audio, boot_assets, cinematic_boot, normal_init, process_context::ActiveUserProcess,
    qemu_exit, retained_services, serial,
};
use crate::{shell_objects::ObjectKind, syscall, user_mode};
use pythos_shared::boot_protocol::{PythBootInfo, PythFramebufferInfo};
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
    if retained_services::initialize_object_service_from_device(substrate.block_device).is_err() {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY");

    serial::init_com2();
    serial::write_line("PYTHOS:CORE:COM2_READY");

    if play_boot_cinematic_and_audio(&boot_info.framebuffer).is_err() {
        serial::write_line("PYTHOS:CORE:NORMAL_BOOT:AUDIO_VISUAL_SKIPPED");
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
        objects,
    })
}

#[cfg(not(test))]
fn count_bootstrap_objects(objects: &[ObjectListEntry; MAX_SHELL_OBJECT_CAPS]) -> Result<u16, ()> {
    let count = objects.iter().filter(|entry| entry.object_id != 0).count();
    u16::try_from(count).map_err(|_| ())
}
