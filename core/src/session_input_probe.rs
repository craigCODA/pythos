//! Opt-in Phase 13.5 finite ring-3 session-input delivery proof.
//!
//! This module composes the already accepted session-input syscall and queue
//! contracts without interpreting events in the kernel. The ring-3 probe is
//! the one bound consumer for this boot.

use crate::memory::{
    physical::PhysicalMemory,
    r#virtual::{KernelAddressSpace, UserAddressSpace},
};
use crate::{
    process_context::{self, ActiveUserProcess},
    ps2, runtime_loader, serial,
    service_identity::ServiceId,
    syscall, user_elf, user_mode, user_stacks,
};
use core::cell::UnsafeCell;
use pythos_shared::{
    boot_protocol::PythBootInfo,
    user_program_manifest::{SESSION_INPUT_PROBE_PRINCIPAL_ID, SESSION_INPUT_PROBE_PROGRAM_NAME},
};

const SESSION_INPUT_PROBE_SERVICE_ID: u64 = 0x5349_4E50;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionInputProbeError {
    Load(runtime_loader::RuntimeLoadError),
    PrincipalMismatch,
    Elf(user_elf::UserElfError),
    AddressSpace(crate::memory::r#virtual::VmError),
    LoadedElfMismatch,
    PreparedElfMismatch,
    Process(crate::user_copy::UserCopyError),
    ConsoleCapability(syscall::SyscallError),
    InputCapability(syscall::SyscallError),
    Ps2(ps2::Ps2Error),
    UserMode(user_mode::UserModeError),
    CallerStillBound,
}

pub struct PreparedSessionInputProbe {
    address_space: crate::memory::r#virtual::RetainedUserAddressSpace,
    entry: u64,
    segment_count: usize,
}

struct PreparedProbeSlot(UnsafeCell<Option<PreparedSessionInputProbe>>);

// SAFETY: the opt-in verification path is single-core; preparation stores one
// root before the terminal probe removes it for the one finite launch.
unsafe impl Sync for PreparedProbeSlot {}

static PREPARED_PROBE: PreparedProbeSlot = PreparedProbeSlot(UnsafeCell::new(None));

/// Build the probe's isolated address space while the loader's temporary
/// identity mappings still make the packaged ELF source directly available.
pub fn prepare(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    _kernel_address_space: &KernelAddressSpace,
) -> Result<(), SessionInputProbeError> {
    let manifest =
        runtime_loader::load_named_user_program(boot_info, SESSION_INPUT_PROBE_PROGRAM_NAME)
            .map_err(SessionInputProbeError::Load)?;
    if manifest.principal_id() != SESSION_INPUT_PROBE_PRINCIPAL_ID {
        return Err(SessionInputProbeError::PrincipalMismatch);
    }
    let image = user_elf::validate(manifest.elf()).map_err(SessionInputProbeError::Elf)?;
    let (address_space, loaded) =
        UserAddressSpace::build_with_user_elf(physical_memory, boot_info, &image, manifest.elf())
            .map_err(SessionInputProbeError::AddressSpace)?;
    if loaded.entry() != image.entry()
        || loaded.segment_count() != image.segment_count()
        || !loaded.bss_zeroed()
    {
        return Err(SessionInputProbeError::LoadedElfMismatch);
    }
    address_space
        .validate_user_elf_entry(image.entry())
        .map_err(SessionInputProbeError::AddressSpace)?;
    let prepared = PreparedSessionInputProbe {
        address_space: address_space.retain_for_boot(),
        entry: image.entry(),
        segment_count: image.segment_count(),
    };
    // SAFETY:
    // 1. Invariant: only this opt-in boot path prepares the probe root.
    // 2. Established by: the feature gate calls `prepare` exactly once.
    // 3. Lifetime: the retained root and its frames outlive the finite launch.
    // 4. Pointer ownership: the slot owns the prepared root until `run` takes it.
    // 5. Alignment: `UnsafeCell<Option<_>>` has its natural Rust alignment.
    // 6. Mapped length: exactly one static option value is accessed.
    // 7. Concurrency: the verification boot is single-core with interrupts disabled.
    // 8. Violation: duplicate preparation would overwrite a retained root.
    let slot = unsafe { &mut *PREPARED_PROBE.0.get() };
    if slot.is_some() {
        return Err(SessionInputProbeError::PreparedElfMismatch);
    }
    *slot = Some(prepared);
    Ok(())
}

/// Execute the feature-gated delivery proof after the ordinary verification
/// boot substrate has made the kernel address space active.
pub fn run(
    boot_info: &PythBootInfo,
    _physical_memory: &mut PhysicalMemory,
    kernel_address_space: &KernelAddressSpace,
) -> Result<(), SessionInputProbeError> {
    // SAFETY:
    // 1. Invariant: `prepare` installed one retained root before kernel CR3 activation.
    // 2. Established by: the opt-in main branch calls `prepare` before this run.
    // 3. Lifetime: the retained root and frames last through this finite launch.
    // 4. Pointer ownership: taking the option transfers unique root ownership to this run.
    // 5. Alignment: `UnsafeCell<Option<_>>` has its natural Rust alignment.
    // 6. Mapped length: exactly one static option value is accessed.
    // 7. Concurrency: this verification boot is single-core.
    // 8. Violation: an absent root suppresses terminal readiness through a typed error.
    let prepared = unsafe { (&mut *PREPARED_PROBE.0.get()).take() }
        .ok_or(SessionInputProbeError::PreparedElfMismatch)?;
    serial::init_com2();
    serial::write_line("PYTHOS:CORE:SESSION_INPUT_BRIDGE:COM2_READY");

    let manifest =
        runtime_loader::load_named_user_program(boot_info, SESSION_INPUT_PROBE_PROGRAM_NAME)
            .map_err(SessionInputProbeError::Load)?;
    if manifest.principal_id() != SESSION_INPUT_PROBE_PRINCIPAL_ID {
        return Err(SessionInputProbeError::PrincipalMismatch);
    }

    let image = user_elf::validate(manifest.elf()).map_err(SessionInputProbeError::Elf)?;
    if prepared.entry != image.entry() || prepared.segment_count != image.segment_count() {
        return Err(SessionInputProbeError::PreparedElfMismatch);
    }

    let stack_region = user_stacks::regions()[0];
    let process = ActiveUserProcess::from_user_elf_launch(
        ServiceId::from_raw(SESSION_INPUT_PROBE_SERVICE_ID),
        manifest.principal_id(),
        manifest.elf_digest(),
        &image,
        stack_region,
    )
    .map_err(SessionInputProbeError::Process)?;
    let console_capability = syscall::grant_console_capability(process)
        .map_err(SessionInputProbeError::ConsoleCapability)?;
    let input_capability = syscall::bind_session_input_capability(process)
        .map_err(SessionInputProbeError::InputCapability)?;
    serial::write_line("PYTHOS:CORE:SESSION_INPUT_BRIDGE:STREAM_BOUND");

    ps2::initialize().map_err(SessionInputProbeError::Ps2)?;
    serial::write_line("PYTHOS:CORE:SESSION_INPUT_BRIDGE:PS2_READY");

    // SAFETY:
    // 1. Invariant: the retained root maps the validated probe ELF, the first
    //    guarded user stack, kernel trap path, and COM2 syscall path.
    // 2. Established by: successful user-ELF build and validation above.
    // 3. Lifetime: the retained root and its backing frames outlive the finite probe.
    // 4. Pointer ownership: the CPU borrows this page-table hierarchy through CR3.
    // 5. Alignment: the root was allocated as a 4 KiB physical page.
    // 6. Mapped length: the complete probe user and required supervisor mappings.
    // 7. Concurrency: one CPU, one bound process, and the producer starts after binding.
    // 8. Violation: a missing mapping traps instead of producing terminal success.
    unsafe {
        prepared.address_space.activate();
    }
    serial::write_line("PYTHOS:CORE:SESSION_INPUT_BRIDGE:RING3_ENTER");
    let user_result = user_mode::run_dynamic_process_breakpoint_test(
        process,
        image.entry(),
        stack_region.stack_start + stack_region.stack_len - 16,
        input_capability.raw(),
        console_capability.raw(),
    );

    // SAFETY:
    // 1. Invariant: this is the already validated kernel root active before the probe.
    // 2. Established by: the verification boot activated and validated it before this run.
    // 3. Lifetime: kernel page tables remain retained for the complete boot.
    // 4. Pointer ownership: the CPU borrows this page-table hierarchy through CR3.
    // 5. Alignment: the root is a 4 KiB-aligned physical page.
    // 6. Mapped length: it covers the complete continuing kernel execution surface.
    // 7. Concurrency: recovery is single-core and no second address-space switch occurs.
    // 8. Violation: subsequent kernel evidence would fault instead of reporting success.
    unsafe {
        kernel_address_space.activate();
    }
    user_result.map_err(SessionInputProbeError::UserMode)?;
    serial::write_line("PYTHOS:CORE:SESSION_INPUT_BRIDGE:RING3_RETURN");

    if process_context::current_caller().is_ok() {
        return Err(SessionInputProbeError::CallerStillBound);
    }
    serial::write_line("PYTHOS:CORE:SESSION_INPUT_BRIDGE:NO_DISK_WRITES");
    serial::write_line("PYTHOS:CORE:SESSION_INPUT_BRIDGE:READY");
    Ok(())
}
