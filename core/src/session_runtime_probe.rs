//! Opt-in bounded Session Manager runtime composition proof.

#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::memory::r#virtual::KernelAddressSpaceBuildOptions;
#[cfg(not(test))]
use crate::service_identity::ServiceId;
#[cfg(not(test))]
use crate::{process_context, user_stacks};
use pythos_shared::object_shell_abi::PackedCapability;
use pythos_shared::pyth_command_abi::{
    COMMAND_KIND_CREATE_NOTE, COMMAND_RESULT_STATUS_OK, PythCommand,
};
use pythos_shared::pyth_runtime_abi::{GRAPH_EXIT_OK, GRAPH_RESULT_UNIT};
use pythos_shared::session_runtime_abi::{
    SESSION_RUNTIME_ABI_MAJOR, SESSION_RUNTIME_ABI_MINOR, SESSION_RUNTIME_COMMAND_COUNT,
    SESSION_RUNTIME_FIXTURE_MAGIC, SESSION_RUNTIME_LIFECYCLE_REINVOKE,
    SESSION_RUNTIME_MAX_COMMAND_PAYLOAD, SESSION_RUNTIME_RESULT_COMPLETE,
    SessionRuntimeBootstrapV1, SessionRuntimeFixtureV1, SessionRuntimeResultV1,
    validate_session_runtime_fixture, validate_session_runtime_result,
};
#[cfg(not(test))]
use pythos_shared::session_runtime_abi::{
    SESSION_RUNTIME_BOOTSTRAP_MAGIC, validate_session_runtime_bootstrap,
};

pub const SESSION_RUNTIME_BOOTSTRAP_USER_PTR: u64 = 0x0000_0000_7200_0000;
pub const SESSION_RUNTIME_PACKAGE_USER_PTR: u64 = 0x0000_0000_7200_1000;
pub const SESSION_RUNTIME_FIXTURE_USER_PTR: u64 = 0x0000_0000_7200_2000;
pub const SESSION_RUNTIME_RESULT_USER_PTR: u64 = 0x0000_0000_7200_3000;
pub const SESSION_RUNTIME_PAYLOAD_PAGE_LEN: u64 = 4096;
pub const SESSION_RUNTIME_SERVICE_ID_RAW: u64 = 0x5059_5345_5353_0001;
pub const SESSION_RUNTIME_PRINCIPAL_ID: u64 =
    pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID;
pub const SESSION_RUNTIME_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_534D_0001;
// The current packaged runtime's PT_LOAD footprint is 26 text + 4 rodata + 36
// data/BSS pages. The probe also maps four one-page ABI payloads into the same
// owning ledger. This is a current minimum, not a permanent artifact size.
const SESSION_RUNTIME_CURRENT_ELF_FRAME_COUNT: usize = 66;
const SESSION_RUNTIME_PAYLOAD_FRAME_COUNT: usize = 4;
const SESSION_RUNTIME_MINIMUM_RETAINED_FRAME_REQUIREMENT: usize =
    SESSION_RUNTIME_CURRENT_ELF_FRAME_COUNT + SESSION_RUNTIME_PAYLOAD_FRAME_COUNT;

const SESSION_RUNTIME_GRAPH_NAME: &[u8] = b"session-manager.tig";

const fn retained_user_frame_capacity_accepts(
    capacity: usize,
    elf_frames: usize,
    payload_frames: usize,
) -> bool {
    match elf_frames.checked_add(payload_frames) {
        Some(required) => required <= capacity,
        None => false,
    }
}

const fn retained_user_frame_capacity_has_rounded_headroom(
    capacity: usize,
    elf_frames: usize,
    payload_frames: usize,
) -> bool {
    match elf_frames.checked_add(payload_frames) {
        Some(required) => required < capacity && capacity.is_power_of_two(),
        None => false,
    }
}

#[cfg(not(test))]
// `memory::virtual` is intentionally absent from host tests, so this production
// const assertion binds the host-tested arithmetic to the actual kernel ledger.
const _: () = assert!(retained_user_frame_capacity_accepts(
    crate::memory::r#virtual::MAX_RETAINED_USER_FRAMES,
    SESSION_RUNTIME_CURRENT_ELF_FRAME_COUNT,
    SESSION_RUNTIME_PAYLOAD_FRAME_COUNT,
));
#[cfg(not(test))]
const _: () = assert!(retained_user_frame_capacity_has_rounded_headroom(
    crate::memory::r#virtual::MAX_RETAINED_USER_FRAMES,
    SESSION_RUNTIME_CURRENT_ELF_FRAME_COUNT,
    SESSION_RUNTIME_PAYLOAD_FRAME_COUNT,
));

pub const SESSION_RUNTIME_COM1_CONTRACT: [&str; 15] = [
    "PYTHOS:CORE:SESSION_RUNTIME:COM2_READY",
    "PYTHOS:CORE:SESSION_RUNTIME:AUTHORITY_CREATED",
    "PYTHOS:CORE:SESSION_RUNTIME:IDENTITIES_VALID",
    "PYTHOS:CORE:SESSION_RUNTIME:STREAM_BOUND",
    "PYTHOS:CORE:SESSION_RUNTIME:PS2_READY",
    "PYTHOS:CORE:SESSION_RUNTIME:RING3_ENTER",
    "PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED",
    "PYTHOS:CORE:PS2:MOUSE_IRQ_FIRED",
    "PYTHOS:CORE:SESSION_RUNTIME:RING3_RETURN",
    "PYTHOS:CORE:SESSION_RUNTIME:INVOCATION_1_VALID",
    "PYTHOS:CORE:SESSION_RUNTIME:REINVOKE_VALID",
    "PYTHOS:CORE:SESSION_RUNTIME:INVOCATION_2_VALID",
    "PYTHOS:CORE:SESSION_RUNTIME:STATE_RETENTION_VALID",
    "PYTHOS:CORE:SESSION_RUNTIME:NO_DISK_WRITES",
    "PYTHOS:CORE:SESSION_RUNTIME:READY",
];

/// Return the finite probe's deliberately hardware-neutral kernel mappings.
#[cfg(not(test))]
pub const fn minimal_kernel_address_space_options() -> KernelAddressSpaceBuildOptions {
    KernelAddressSpaceBuildOptions::new()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionRuntimeProbeError {
    RuntimeLoad(crate::runtime_loader::RuntimeLoadError),
    GraphLoad(crate::pyth_graph_loader::PythGraphLoadError),
    IdentityMismatch,
    ArtifactMismatch,
    PackageTooLarge,
    Elf(crate::user_elf::UserElfError),
    Memory(crate::memory::physical::MemoryError),
    #[cfg(not(test))]
    AddressSpace(crate::memory::r#virtual::VmError),
    Process(crate::user_copy::UserCopyError),
    GraphBootstrap(crate::pyth_runtime_launch::PythRuntimeLaunchError),
    ConsoleCapability(crate::syscall::SyscallError),
    CommandCapability(crate::syscall::SyscallError),
    InputCapability(crate::syscall::SyscallError),
    Fixture,
    Bootstrap,
    Result,
    PreparedLaunch,
    Ps2(crate::ps2::Ps2Error),
    UserMode(crate::user_mode::UserModeError),
    FaultPrincipalMismatch,
    KernelRootNotRestored,
    CallerStillBound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionRuntimeUserOutcome {
    Completed,
    FaultContained(crate::user_mode::UserFaultContext),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionRuntimeArtifactIdentity {
    pub runtime_principal_id: u64,
    pub runtime_digest: u64,
    pub graph_principal_id: u64,
    pub graph_digest: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SessionRuntimeAuthority {
    console: PackedCapability,
    command: PackedCapability,
    input: PackedCapability,
}

#[cfg(not(test))]
struct PreparedSessionRuntimeProbe {
    address_space: crate::memory::r#virtual::RetainedUserAddressSpace,
    process: process_context::ActiveUserProcess,
    entry: u64,
    segment_count: usize,
    stack: user_stacks::UserStackRegion,
    bootstrap_physical: u64,
    result_physical: u64,
    artifacts: SessionRuntimeArtifactIdentity,
    graph_package_len: u64,
    fixture: SessionRuntimeFixtureV1,
}

#[cfg(not(test))]
struct PreparedProbeSlot(core::cell::UnsafeCell<Option<PreparedSessionRuntimeProbe>>);

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: the feature-gated boot path prepares and consumes one launch.
// 2. Established by: `main` calls `prepare` once and `run` once in one branch.
// 3. Lifetime: the static slot outlives the finite retained user execution.
// 4. Pointer ownership: `prepare` installs once and `run` takes unique ownership.
// 5. Alignment: `UnsafeCell<Option<_>>` preserves the option's alignment.
// 6. Mapped length: exactly one complete prepared-launch option is accessed.
// 7. Concurrency: the verification boot is single-core with no second launcher.
// 8. Violation: aliasing could activate or consume the wrong retained root.
unsafe impl Sync for PreparedProbeSlot {}

#[cfg(not(test))]
static PREPARED_PROBE: PreparedProbeSlot = PreparedProbeSlot(core::cell::UnsafeCell::new(None));

pub fn validate_session_runtime_identities(
    service_id: u64,
    runtime_principal_id: u64,
    graph_principal_id: u64,
) -> Result<(), SessionRuntimeProbeError> {
    if service_id != SESSION_RUNTIME_SERVICE_ID_RAW
        || runtime_principal_id != SESSION_RUNTIME_PRINCIPAL_ID
        || graph_principal_id != SESSION_RUNTIME_GRAPH_PRINCIPAL_ID
        || service_id == runtime_principal_id
        || service_id == graph_principal_id
        || runtime_principal_id == graph_principal_id
    {
        return Err(SessionRuntimeProbeError::IdentityMismatch);
    }
    Ok(())
}

pub fn validate_session_runtime_artifacts(
    prepared: SessionRuntimeArtifactIdentity,
    loaded: SessionRuntimeArtifactIdentity,
) -> Result<(), SessionRuntimeProbeError> {
    validate_session_runtime_identities(
        SESSION_RUNTIME_SERVICE_ID_RAW,
        loaded.runtime_principal_id,
        loaded.graph_principal_id,
    )?;
    if prepared != loaded {
        return Err(SessionRuntimeProbeError::ArtifactMismatch);
    }
    Ok(())
}

pub fn build_session_runtime_fixture() -> Result<SessionRuntimeFixtureV1, SessionRuntimeProbeError>
{
    let mut fixture = SessionRuntimeFixtureV1::empty();
    fixture.magic = SESSION_RUNTIME_FIXTURE_MAGIC;
    fixture.abi_major = SESSION_RUNTIME_ABI_MAJOR;
    fixture.abi_minor = SESSION_RUNTIME_ABI_MINOR;
    fixture.command_count = SESSION_RUNTIME_COMMAND_COUNT as u16;
    for (ordinal, payload) in [b"slice2-one".as_slice(), b"slice2-two".as_slice()]
        .into_iter()
        .enumerate()
    {
        fixture.payloads[ordinal][..payload.len()].copy_from_slice(payload);
        fixture.commands[ordinal] = PythCommand {
            object_id: (ordinal + 11) as u64,
            payload_ptr: SESSION_RUNTIME_FIXTURE_USER_PTR
                + core::mem::offset_of!(SessionRuntimeFixtureV1, payloads) as u64
                + (ordinal * SESSION_RUNTIME_MAX_COMMAND_PAYLOAD) as u64,
            payload_len: payload.len() as u64,
            ..PythCommand::empty(COMMAND_KIND_CREATE_NOTE)
        };
    }
    Ok(fixture)
}

fn establish_session_runtime_authority_with(
    grant_console: impl FnOnce() -> Result<PackedCapability, SessionRuntimeProbeError>,
    grant_command: impl FnOnce() -> Result<PackedCapability, SessionRuntimeProbeError>,
    bind_input: impl FnOnce() -> Result<PackedCapability, SessionRuntimeProbeError>,
) -> Result<SessionRuntimeAuthority, SessionRuntimeProbeError> {
    let console = grant_console()?;
    let command = grant_command()?;
    let input = bind_input()?;
    Ok(SessionRuntimeAuthority {
        console,
        command,
        input,
    })
}

pub fn validate_session_runtime_return(
    bootstrap: &SessionRuntimeBootstrapV1,
    fixture: &SessionRuntimeFixtureV1,
    result: &SessionRuntimeResultV1,
    kernel_root_restored: bool,
    caller_cleared: bool,
) -> Result<(), SessionRuntimeProbeError> {
    if !kernel_root_restored {
        return Err(SessionRuntimeProbeError::KernelRootNotRestored);
    }
    if !caller_cleared {
        return Err(SessionRuntimeProbeError::CallerStillBound);
    }
    validate_session_runtime_fixture(bootstrap, fixture)
        .map_err(|_| SessionRuntimeProbeError::Fixture)?;
    validate_session_runtime_result(bootstrap, result)
        .map_err(|_| SessionRuntimeProbeError::Result)?;
    if result.terminal_status != SESSION_RUNTIME_RESULT_COMPLETE
        || result.last_lifecycle_action != SESSION_RUNTIME_LIFECYCLE_REINVOKE
        || result.input_event_count != 2
        || result.invocation_count != 2
        || result.retained_state_before_second != 1
        || result.retained_state_final != 2
    {
        return Err(SessionRuntimeProbeError::Result);
    }
    for ordinal in 0..SESSION_RUNTIME_COMMAND_COUNT {
        let command = fixture.commands[ordinal];
        let command_result = result.command_results[ordinal];
        if command_result.status != COMMAND_RESULT_STATUS_OK
            || command_result.kind != command.kind
            || command_result.reserved0 != 0
            || command_result.object_id != command.object_id
            || command_result.task_id != command.task_id
            || command_result.proposal_id != command.proposal_id
            || command_result.bytes_written != command.payload_len
            || command_result.reserved1 != 0
        {
            return Err(SessionRuntimeProbeError::Result);
        }
        let exit = result.graph_exits[ordinal];
        if exit.status != GRAPH_EXIT_OK
            || exit.error_code != 0
            || exit.last_node == u32::MAX
            || exit.executed_nodes == 0
            || exit.executed_nodes > bootstrap.graph.instruction_budget
            || exit.result_type != GRAPH_RESULT_UNIT
            || exit.reserved0 != 0
            || exit.reserved1 != 0
            || exit.result_raw != 0
        {
            return Err(SessionRuntimeProbeError::Result);
        }
    }
    if result.graph_exits[0] != result.graph_exits[1] {
        return Err(SessionRuntimeProbeError::Result);
    }
    Ok(())
}

pub fn validate_session_runtime_user_outcome(
    user_result: Result<(), crate::user_mode::UserModeError>,
    kernel_root_restored: bool,
    caller_cleared: bool,
) -> Result<SessionRuntimeUserOutcome, SessionRuntimeProbeError> {
    if !kernel_root_restored {
        return Err(SessionRuntimeProbeError::KernelRootNotRestored);
    }
    if !caller_cleared {
        return Err(SessionRuntimeProbeError::CallerStillBound);
    }
    match user_result {
        Ok(()) => Ok(SessionRuntimeUserOutcome::Completed),
        Err(crate::user_mode::UserModeError::FaultContained(context)) => {
            if context.principal != SESSION_RUNTIME_PRINCIPAL_ID {
                return Err(SessionRuntimeProbeError::FaultPrincipalMismatch);
            }
            Ok(SessionRuntimeUserOutcome::FaultContained(context))
        }
        Err(error) => Err(SessionRuntimeProbeError::UserMode(error)),
    }
}

#[cfg(not(test))]
pub fn prepare(
    boot_info: &pythos_shared::boot_protocol::PythBootInfo,
    physical_memory: &mut crate::memory::physical::PhysicalMemory,
    _kernel_address_space: &crate::memory::r#virtual::KernelAddressSpace,
) -> Result<(), SessionRuntimeProbeError> {
    let runtime = crate::runtime_loader::load_named_user_program(
        boot_info,
        pythos_shared::user_program_manifest::SESSION_RUNTIME_PROGRAM_NAME,
    )
    .map_err(SessionRuntimeProbeError::RuntimeLoad)?;
    let graph =
        crate::pyth_graph_loader::load_named_pyth_graph(boot_info, SESSION_RUNTIME_GRAPH_NAME)
            .map_err(SessionRuntimeProbeError::GraphLoad)?;
    let artifacts = SessionRuntimeArtifactIdentity {
        runtime_principal_id: runtime.principal_id(),
        runtime_digest: runtime.elf_digest(),
        graph_principal_id: graph.manifest.principal_id(),
        graph_digest: graph.manifest.package_digest(),
    };
    validate_session_runtime_identities(
        SESSION_RUNTIME_SERVICE_ID_RAW,
        artifacts.runtime_principal_id,
        artifacts.graph_principal_id,
    )?;
    let graph_package_len = graph.manifest.package().len() as u64;
    if graph_package_len == 0 || graph_package_len > SESSION_RUNTIME_PAYLOAD_PAGE_LEN {
        return Err(SessionRuntimeProbeError::PackageTooLarge);
    }
    let image = crate::user_elf::validate(runtime.elf()).map_err(SessionRuntimeProbeError::Elf)?;
    let stack = user_stacks::regions()[0];
    let process = process_context::ActiveUserProcess::from_session_runtime_launch(
        ServiceId::from_raw(SESSION_RUNTIME_SERVICE_ID_RAW),
        runtime.principal_id(),
        runtime.elf_digest(),
        &image,
        process_context::SessionRuntimeCopyMapSpec {
            stack,
            bootstrap_user_ptr: SESSION_RUNTIME_BOOTSTRAP_USER_PTR,
            bootstrap_len: SESSION_RUNTIME_PAYLOAD_PAGE_LEN,
            package_user_ptr: SESSION_RUNTIME_PACKAGE_USER_PTR,
            package_len: SESSION_RUNTIME_PAYLOAD_PAGE_LEN,
            fixture_user_ptr: SESSION_RUNTIME_FIXTURE_USER_PTR,
            fixture_len: SESSION_RUNTIME_PAYLOAD_PAGE_LEN,
            result_user_ptr: SESSION_RUNTIME_RESULT_USER_PTR,
            result_len: SESSION_RUNTIME_PAYLOAD_PAGE_LEN,
        },
    )
    .map_err(SessionRuntimeProbeError::Process)?;

    let bootstrap_physical = physical_memory
        .allocate_zeroed_page()
        .map_err(SessionRuntimeProbeError::Memory)?;
    let package_physical = physical_memory
        .allocate_zeroed_page()
        .map_err(SessionRuntimeProbeError::Memory)?;
    let fixture_physical = physical_memory
        .allocate_zeroed_page()
        .map_err(SessionRuntimeProbeError::Memory)?;
    let result_physical = physical_memory
        .allocate_zeroed_page()
        .map_err(SessionRuntimeProbeError::Memory)?;
    let fixture = build_session_runtime_fixture()?;
    write_bytes_to_frame(package_physical, graph.manifest.package())?;
    write_value_to_frame(fixture_physical, &fixture)?;

    let payload_mappings = [
        crate::memory::r#virtual::UserPayloadMapping::read_only(
            SESSION_RUNTIME_BOOTSTRAP_USER_PTR,
            bootstrap_physical,
            SESSION_RUNTIME_PAYLOAD_PAGE_LEN,
        ),
        crate::memory::r#virtual::UserPayloadMapping::read_only(
            SESSION_RUNTIME_PACKAGE_USER_PTR,
            package_physical,
            SESSION_RUNTIME_PAYLOAD_PAGE_LEN,
        ),
        crate::memory::r#virtual::UserPayloadMapping::read_only(
            SESSION_RUNTIME_FIXTURE_USER_PTR,
            fixture_physical,
            SESSION_RUNTIME_PAYLOAD_PAGE_LEN,
        ),
        crate::memory::r#virtual::UserPayloadMapping::read_write(
            SESSION_RUNTIME_RESULT_USER_PTR,
            result_physical,
            SESSION_RUNTIME_PAYLOAD_PAGE_LEN,
        ),
    ];
    let (address_space, loaded) =
        crate::memory::r#virtual::UserAddressSpace::build_with_user_elf_payloads_and_supervisor_mappings(
            physical_memory,
            boot_info,
            &image,
            runtime.elf(),
            &payload_mappings,
            &[],
        )
        .map_err(SessionRuntimeProbeError::AddressSpace)?;
    if loaded.entry() != image.entry()
        || loaded.segment_count() != image.segment_count()
        || !loaded.bss_zeroed()
    {
        return Err(SessionRuntimeProbeError::PreparedLaunch);
    }
    address_space
        .validate_user_elf_entry(image.entry())
        .map_err(SessionRuntimeProbeError::AddressSpace)?;
    for (user_ptr, writable) in [
        (SESSION_RUNTIME_BOOTSTRAP_USER_PTR, false),
        (SESSION_RUNTIME_PACKAGE_USER_PTR, false),
        (SESSION_RUNTIME_FIXTURE_USER_PTR, false),
        (SESSION_RUNTIME_RESULT_USER_PTR, true),
    ] {
        address_space
            .validate_user_payload_mapping(user_ptr, writable)
            .map_err(SessionRuntimeProbeError::AddressSpace)?;
    }
    let prepared = PreparedSessionRuntimeProbe {
        address_space: address_space.retain_for_boot(),
        process,
        entry: image.entry(),
        segment_count: image.segment_count(),
        stack,
        bootstrap_physical,
        result_physical,
        artifacts,
        graph_package_len,
        fixture,
    };
    // SAFETY:
    // 1. Invariant: this feature prepares exactly one retained launch record.
    // 2. Established by: `main` calls this function once before kernel CR3 activation.
    // 3. Lifetime: all retained roots and frames outlive the finite run.
    // 4. Pointer ownership: the static slot owns the record until `run` takes it.
    // 5. Alignment: `UnsafeCell<Option<_>>` preserves the option's alignment.
    // 6. Mapped length: exactly one complete prepared-launch option is accessed.
    // 7. Concurrency: early boot is single-core with interrupts disabled.
    // 8. Violation: overwrite could leak or later activate an unrelated root.
    let slot = unsafe { &mut *PREPARED_PROBE.0.get() };
    if slot.is_some() {
        return Err(SessionRuntimeProbeError::PreparedLaunch);
    }
    *slot = Some(prepared);
    Ok(())
}

#[cfg(not(test))]
pub fn run(
    boot_info: &pythos_shared::boot_protocol::PythBootInfo,
    _physical_memory: &mut crate::memory::physical::PhysicalMemory,
    kernel_address_space: &crate::memory::r#virtual::KernelAddressSpace,
) -> Result<(), SessionRuntimeProbeError> {
    // SAFETY:
    // 1. Invariant: `prepare` installed one launch before the kernel-root switch.
    // 2. Established by: the feature-gated `main` sequence completed preparation.
    // 3. Lifetime: the retained root and payload frames outlive this finite run.
    // 4. Pointer ownership: taking the option transfers unique launch ownership.
    // 5. Alignment: `UnsafeCell<Option<_>>` preserves the option's alignment.
    // 6. Mapped length: exactly one prepared-launch option is accessed.
    // 7. Concurrency: this verification boot is single-core.
    // 8. Violation: duplicate consumption could reuse authority or an old root.
    let prepared = unsafe { (&mut *PREPARED_PROBE.0.get()).take() }
        .ok_or(SessionRuntimeProbeError::PreparedLaunch)?;
    crate::serial::init_com2();
    crate::serial::write_line(SESSION_RUNTIME_COM1_CONTRACT[0]);

    let runtime = crate::runtime_loader::load_named_user_program(
        boot_info,
        pythos_shared::user_program_manifest::SESSION_RUNTIME_PROGRAM_NAME,
    )
    .map_err(SessionRuntimeProbeError::RuntimeLoad)?;
    let graph =
        crate::pyth_graph_loader::load_named_pyth_graph(boot_info, SESSION_RUNTIME_GRAPH_NAME)
            .map_err(SessionRuntimeProbeError::GraphLoad)?;
    let loaded_artifacts = SessionRuntimeArtifactIdentity {
        runtime_principal_id: runtime.principal_id(),
        runtime_digest: runtime.elf_digest(),
        graph_principal_id: graph.manifest.principal_id(),
        graph_digest: graph.manifest.package_digest(),
    };
    validate_session_runtime_artifacts(prepared.artifacts, loaded_artifacts)?;
    if prepared.entry
        != crate::user_elf::validate(runtime.elf())
            .map_err(SessionRuntimeProbeError::Elf)?
            .entry()
        || prepared.segment_count
            != crate::user_elf::validate(runtime.elf())
                .map_err(SessionRuntimeProbeError::Elf)?
                .segment_count()
        || prepared.graph_package_len != graph.manifest.package().len() as u64
    {
        return Err(SessionRuntimeProbeError::ArtifactMismatch);
    }

    let process = prepared.process;
    let authority = establish_session_runtime_authority_with(
        || {
            crate::syscall::grant_console_capability(process)
                .map_err(SessionRuntimeProbeError::ConsoleCapability)
        },
        || {
            crate::syscall::grant_session_command_capability(process)
                .map_err(SessionRuntimeProbeError::CommandCapability)
        },
        || {
            crate::syscall::bind_session_input_capability(process)
                .map_err(SessionRuntimeProbeError::InputCapability)
        },
    )?;
    crate::serial::write_line(SESSION_RUNTIME_COM1_CONTRACT[1]);
    crate::serial::write_line(SESSION_RUNTIME_COM1_CONTRACT[2]);
    crate::serial::write_line(SESSION_RUNTIME_COM1_CONTRACT[3]);

    let graph_bootstrap = crate::pyth_runtime_launch::build_pyth_command_graph_bootstrap(
        &graph.verified,
        SESSION_RUNTIME_PACKAGE_USER_PTR,
        prepared.graph_package_len,
        SESSION_RUNTIME_RESULT_USER_PTR,
        authority.command,
    )
    .map_err(SessionRuntimeProbeError::GraphBootstrap)?;
    let mut bootstrap = SessionRuntimeBootstrapV1::empty();
    bootstrap.magic = SESSION_RUNTIME_BOOTSTRAP_MAGIC;
    bootstrap.abi_major = SESSION_RUNTIME_ABI_MAJOR;
    bootstrap.abi_minor = SESSION_RUNTIME_ABI_MINOR;
    bootstrap.command_count = SESSION_RUNTIME_COMMAND_COUNT as u16;
    bootstrap.session_service_id = SESSION_RUNTIME_SERVICE_ID_RAW;
    bootstrap.runtime_principal_id = prepared.artifacts.runtime_principal_id;
    bootstrap.graph_principal_id = prepared.artifacts.graph_principal_id;
    bootstrap.graph_package_digest = prepared.artifacts.graph_digest;
    bootstrap.input_capability = authority.input;
    bootstrap.console_capability = authority.console;
    bootstrap.fixture_ptr = SESSION_RUNTIME_FIXTURE_USER_PTR;
    bootstrap.fixture_len = core::mem::size_of::<SessionRuntimeFixtureV1>() as u64;
    bootstrap.result_ptr = SESSION_RUNTIME_RESULT_USER_PTR;
    bootstrap.result_len = core::mem::size_of::<SessionRuntimeResultV1>() as u64;
    bootstrap.graph = graph_bootstrap;
    validate_session_runtime_bootstrap(&bootstrap)
        .map_err(|_| SessionRuntimeProbeError::Bootstrap)?;
    validate_session_runtime_fixture(&bootstrap, &prepared.fixture)
        .map_err(|_| SessionRuntimeProbeError::Fixture)?;
    write_value_to_frame(prepared.bootstrap_physical, &bootstrap)?;

    crate::ps2::initialize().map_err(SessionRuntimeProbeError::Ps2)?;
    crate::serial::write_line(SESSION_RUNTIME_COM1_CONTRACT[4]);
    // SAFETY:
    // 1. Invariant: this retained root contains the validated session-runtime
    //    ELF, selected guarded stack, four exact payload pages, and kernel trap path.
    // 2. Established by: `prepare` built and validated every mapping before retention.
    // 3. Lifetime: the root and all backing frames outlive the finite ring-3 run.
    // 4. Pointer ownership: the CPU borrows the PythCore-owned page hierarchy.
    // 5. Alignment: the root is a physical allocator-owned 4 KiB PML4 page.
    // 6. Mapped length: the full validated user and required supervisor surface is mapped.
    // 7. Concurrency: one CPU runs one bound session runtime with no root mutation.
    // 8. Violation: a missing mapping traps and prevents the readiness contract.
    unsafe {
        prepared.address_space.activate();
    }
    crate::serial::write_line(SESSION_RUNTIME_COM1_CONTRACT[5]);
    let user_result = crate::user_mode::run_returnable_user_process(
        process,
        prepared.entry,
        prepared.stack.stack_start + prepared.stack.stack_len - 16,
        SESSION_RUNTIME_BOOTSTRAP_USER_PTR,
        0,
    );

    // SAFETY:
    // 1. Invariant: this is the kernel root validated immediately before probe setup.
    // 2. Established by: the feature branch built, activated, and validated this root.
    // 3. Lifetime: its page tables are retained for the complete kernel boot.
    // 4. Pointer ownership: the CPU borrows the PythCore-owned hierarchy through CR3.
    // 5. Alignment: the physical allocator supplied a page-aligned root.
    // 6. Mapped length: it covers all continuing kernel code, data, stack, and scratch paths.
    // 7. Concurrency: recovery is single-core with no competing CR3 switch.
    // 8. Violation: validation below fails or execution faults before readiness.
    unsafe {
        kernel_address_space.activate();
    }
    let kernel_root_restored = kernel_address_space.validate_active(boot_info).is_ok();
    let caller_cleared = process_context::current_caller().is_err();
    match validate_session_runtime_user_outcome(user_result, kernel_root_restored, caller_cleared)?
    {
        SessionRuntimeUserOutcome::Completed => {}
        SessionRuntimeUserOutcome::FaultContained(context) => {
            crate::serial::write_str("PYTHOS:CORE:SESSION_RUNTIME:FAULT_CONTAINED principal:");
            crate::serial::write_hex_u64_value(context.principal);
            crate::serial::write_str(" vector:");
            crate::serial::write_dec_u64_value(context.vector);
            crate::serial::write_str(" rip:");
            crate::serial::write_hex_u64_value(context.rip);
            crate::serial::write_str(" rsp:");
            crate::serial::write_hex_u64_value(context.rsp);
            crate::serial::write_str(" cr2:");
            crate::serial::write_hex_u64_value(context.cr2);
            crate::serial::write_str("\r\n");
            crate::serial::write_line("PYTHOS:CORE:SESSION_RUNTIME:RECOVERY_REQUESTED");
            return Ok(());
        }
    }
    crate::serial::write_line(SESSION_RUNTIME_COM1_CONTRACT[8]);
    let result = read_result_from_frame(prepared.result_physical)?;
    validate_session_runtime_return(
        &bootstrap,
        &prepared.fixture,
        &result,
        kernel_root_restored,
        caller_cleared,
    )?;
    for marker in &SESSION_RUNTIME_COM1_CONTRACT[9..] {
        crate::serial::write_line(marker);
    }
    Ok(())
}

#[cfg(not(test))]
fn write_bytes_to_frame(physical: u64, bytes: &[u8]) -> Result<(), SessionRuntimeProbeError> {
    if bytes.is_empty() || bytes.len() > SESSION_RUNTIME_PAYLOAD_PAGE_LEN as usize {
        return Err(SessionRuntimeProbeError::PackageTooLarge);
    }
    crate::memory::r#virtual::with_writable_physical_frame(physical, |page| {
        page.fill(0);
        page[..bytes.len()].copy_from_slice(bytes);
    })
    .map_err(SessionRuntimeProbeError::AddressSpace)
}

#[cfg(not(test))]
fn write_value_to_frame<T: Copy>(physical: u64, value: &T) -> Result<(), SessionRuntimeProbeError> {
    if core::mem::size_of::<T>() > SESSION_RUNTIME_PAYLOAD_PAGE_LEN as usize {
        return Err(SessionRuntimeProbeError::PackageTooLarge);
    }
    crate::memory::r#virtual::with_writable_physical_frame(physical, |page| {
        page.fill(0);
        // SAFETY:
        // 1. Invariant: `value` is a fully initialized `Copy` ABI record and
        //    the page-aligned destination can hold exactly one such record.
        // 2. Established by: callers construct the fixture/bootstrap by value and validate it.
        // 3. Lifetime: the source borrow remains live for this synchronous copy.
        // 4. Pointer ownership: source is read-only; the destination frame is exclusively owned.
        // 5. Alignment: the page-aligned destination satisfies every admitted
        //    type because a type larger-aligned than one page cannot pass the size check.
        // 6. Mapped length: the checked type size fits inside the complete page.
        // 7. Concurrency: early boot and late scratch writes are single-core and serialized.
        // 8. Violation: a bad size or uninitialized source could corrupt the launch record.
        unsafe {
            core::ptr::copy_nonoverlapping(value as *const T, page.as_mut_ptr().cast::<T>(), 1);
        }
    })
    .map_err(SessionRuntimeProbeError::AddressSpace)
}

#[cfg(not(test))]
fn read_result_from_frame(
    physical: u64,
) -> Result<SessionRuntimeResultV1, SessionRuntimeProbeError> {
    crate::memory::r#virtual::with_writable_physical_frame(physical, |page| {
        // SAFETY:
        // 1. Invariant: the retained result frame contains one aligned result record written before int3.
        // 2. Established by: the exact RW user mapping and successful expected-breakpoint return.
        // 3. Lifetime: the frame is retained and scratch-mapped for this synchronous read.
        // 4. Pointer ownership: user execution ended; PythCore now reads the owned frame by value.
        // 5. Alignment: both identity and scratch aliases are page-aligned, satisfying the record.
        // 6. Mapped length: one full page exceeds `SessionRuntimeResultV1` size.
        // 7. Concurrency: no user execution or other result writer remains active.
        // 8. Violation: malformed bytes are rejected by complete result validation before readiness.
        unsafe { (page.as_ptr() as *const SessionRuntimeResultV1).read() }
    })
    .map_err(SessionRuntimeProbeError::AddressSpace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};
    use pythos_shared::object_shell_abi::PackedCapability;
    use pythos_shared::pyth_command_abi::{
        COMMAND_KIND_CREATE_NOTE, COMMAND_RESULT_STATUS_OK, PythCommandResult,
    };
    use pythos_shared::pyth_runtime_abi::{GRAPH_EXIT_OK, GRAPH_RESULT_UNIT, GraphExitRecord};
    use pythos_shared::session_runtime_abi::{
        SESSION_RUNTIME_ABI_MAJOR, SESSION_RUNTIME_ABI_MINOR, SESSION_RUNTIME_COMMAND_COUNT,
        SESSION_RUNTIME_LIFECYCLE_REINVOKE, SESSION_RUNTIME_MAX_COMMAND_PAYLOAD,
        SESSION_RUNTIME_RESULT_COMPLETE, SessionRuntimeBootstrapV1, SessionRuntimeFixtureV1,
        SessionRuntimeResultV1,
    };

    #[test]
    fn session_runtime_layout_and_identities_are_exact_and_distinct() {
        assert_eq!(SESSION_RUNTIME_BOOTSTRAP_USER_PTR, 0x7200_0000);
        assert_eq!(SESSION_RUNTIME_PACKAGE_USER_PTR, 0x7200_1000);
        assert_eq!(SESSION_RUNTIME_FIXTURE_USER_PTR, 0x7200_2000);
        assert_eq!(SESSION_RUNTIME_RESULT_USER_PTR, 0x7200_3000);
        assert_eq!(SESSION_RUNTIME_PAYLOAD_PAGE_LEN, 0x1000);
        assert_eq!(SESSION_RUNTIME_SERVICE_ID_RAW, 0x5059_5345_5353_0001);
        assert_eq!(SESSION_RUNTIME_PRINCIPAL_ID, 0x5059_5352_544D_0001);
        assert_eq!(SESSION_RUNTIME_GRAPH_PRINCIPAL_ID, 0x5059_5448_534D_0001);
        assert!(
            validate_session_runtime_identities(
                SESSION_RUNTIME_SERVICE_ID_RAW,
                SESSION_RUNTIME_PRINCIPAL_ID,
                SESSION_RUNTIME_GRAPH_PRINCIPAL_ID,
            )
            .is_ok()
        );
        assert!(
            validate_session_runtime_identities(
                SESSION_RUNTIME_SERVICE_ID_RAW,
                SESSION_RUNTIME_SERVICE_ID_RAW,
                SESSION_RUNTIME_GRAPH_PRINCIPAL_ID,
            )
            .is_err()
        );
    }

    #[test]
    fn session_runtime_retained_frame_capacity_covers_current_minimum() {
        assert_eq!(SESSION_RUNTIME_MINIMUM_RETAINED_FRAME_REQUIREMENT, 66 + 4);
        assert!(!retained_user_frame_capacity_accepts(69, 66, 4));
        assert!(retained_user_frame_capacity_accepts(70, 66, 4));
        assert!(!retained_user_frame_capacity_accepts(
            usize::MAX,
            usize::MAX,
            1,
        ));
    }

    #[test]
    fn session_runtime_retained_frame_capacity_requires_rounded_headroom() {
        assert!(!retained_user_frame_capacity_has_rounded_headroom(
            70, 66, 4,
        ));
        assert!(!retained_user_frame_capacity_has_rounded_headroom(
            96, 66, 4,
        ));
        assert!(retained_user_frame_capacity_has_rounded_headroom(
            128, 66, 4,
        ));
        assert!(!retained_user_frame_capacity_has_rounded_headroom(
            128, 125, 4,
        ));
        assert!(!retained_user_frame_capacity_has_rounded_headroom(
            usize::MAX,
            usize::MAX,
            1,
        ));
    }

    #[test]
    fn session_runtime_com1_contract_is_exact_and_ordered() {
        assert_eq!(
            SESSION_RUNTIME_COM1_CONTRACT,
            [
                "PYTHOS:CORE:SESSION_RUNTIME:COM2_READY",
                "PYTHOS:CORE:SESSION_RUNTIME:AUTHORITY_CREATED",
                "PYTHOS:CORE:SESSION_RUNTIME:IDENTITIES_VALID",
                "PYTHOS:CORE:SESSION_RUNTIME:STREAM_BOUND",
                "PYTHOS:CORE:SESSION_RUNTIME:PS2_READY",
                "PYTHOS:CORE:SESSION_RUNTIME:RING3_ENTER",
                "PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED",
                "PYTHOS:CORE:PS2:MOUSE_IRQ_FIRED",
                "PYTHOS:CORE:SESSION_RUNTIME:RING3_RETURN",
                "PYTHOS:CORE:SESSION_RUNTIME:INVOCATION_1_VALID",
                "PYTHOS:CORE:SESSION_RUNTIME:REINVOKE_VALID",
                "PYTHOS:CORE:SESSION_RUNTIME:INVOCATION_2_VALID",
                "PYTHOS:CORE:SESSION_RUNTIME:STATE_RETENTION_VALID",
                "PYTHOS:CORE:SESSION_RUNTIME:NO_DISK_WRITES",
                "PYTHOS:CORE:SESSION_RUNTIME:READY",
            ]
        );
    }

    #[test]
    fn session_runtime_fixture_has_exact_two_create_note_commands() {
        let fixture = build_session_runtime_fixture().unwrap();

        assert_eq!(
            fixture.command_count as usize,
            SESSION_RUNTIME_COMMAND_COUNT
        );
        for (ordinal, expected) in [b"slice2-one".as_slice(), b"slice2-two".as_slice()]
            .into_iter()
            .enumerate()
        {
            let command = fixture.commands[ordinal];
            assert_eq!(command.kind, COMMAND_KIND_CREATE_NOTE);
            assert_eq!(command.object_id, (ordinal + 11) as u64);
            assert_eq!(command.task_id, 0);
            assert_eq!(command.proposal_id, 0);
            assert_eq!(command.payload_len, expected.len() as u64);
            assert_eq!(
                command.payload_ptr,
                SESSION_RUNTIME_FIXTURE_USER_PTR
                    + offset_of!(SessionRuntimeFixtureV1, payloads) as u64
                    + (ordinal * SESSION_RUNTIME_MAX_COMMAND_PAYLOAD) as u64
            );
            assert_eq!(&fixture.payloads[ordinal][..expected.len()], expected);
            assert_eq!(
                &fixture.payloads[ordinal][expected.len()..],
                &[0; SESSION_RUNTIME_MAX_COMMAND_PAYLOAD][expected.len()..]
            );
        }
    }

    #[test]
    fn session_runtime_artifact_gate_rejects_graph_identity_or_digest_mismatch() {
        let exact = SessionRuntimeArtifactIdentity {
            runtime_principal_id: SESSION_RUNTIME_PRINCIPAL_ID,
            runtime_digest: 0xAAAA,
            graph_principal_id: SESSION_RUNTIME_GRAPH_PRINCIPAL_ID,
            graph_digest: 0xBBBB,
        };

        assert!(validate_session_runtime_artifacts(exact, exact).is_ok());
        assert!(
            validate_session_runtime_artifacts(
                exact,
                SessionRuntimeArtifactIdentity {
                    graph_principal_id: 7,
                    ..exact
                },
            )
            .is_err()
        );
        assert!(
            validate_session_runtime_artifacts(
                exact,
                SessionRuntimeArtifactIdentity {
                    graph_digest: 8,
                    ..exact
                },
            )
            .is_err()
        );
    }

    #[test]
    fn session_runtime_authority_setup_grants_command_and_binds_input_once() {
        let mut console_grants = 0usize;
        let mut command_grants = 0usize;
        let mut input_binds = 0usize;

        let authority = establish_session_runtime_authority_with(
            || {
                console_grants += 1;
                Ok(PackedCapability::from_raw(1))
            },
            || {
                command_grants += 1;
                Ok(PackedCapability::from_raw(2))
            },
            || {
                input_binds += 1;
                Ok(PackedCapability::from_raw(3))
            },
        )
        .unwrap();

        assert_eq!(authority.console.raw(), 1);
        assert_eq!(authority.command.raw(), 2);
        assert_eq!(authority.input.raw(), 3);
        assert_eq!(console_grants, 1);
        assert_eq!(command_grants, 1);
        assert_eq!(input_binds, 1);
    }

    #[test]
    fn session_runtime_readiness_rejects_malformed_result_or_uncleared_boundary() {
        let fixture = build_session_runtime_fixture().unwrap();
        let bootstrap = accepted_bootstrap();
        let result = accepted_result(&bootstrap, &fixture);

        assert!(validate_session_runtime_return(&bootstrap, &fixture, &result, true, true).is_ok());

        let mut malformed = result;
        malformed.command_results[1].bytes_written += 1;
        assert!(
            validate_session_runtime_return(&bootstrap, &fixture, &malformed, true, true).is_err()
        );
        assert!(
            validate_session_runtime_return(&bootstrap, &fixture, &result, true, false).is_err()
        );
        assert!(
            validate_session_runtime_return(&bootstrap, &fixture, &result, false, true).is_err()
        );
    }

    #[test]
    fn session_runtime_accepts_only_matching_contained_fault_after_boundary_cleanup() {
        let context = crate::user_mode::UserFaultContext {
            principal: SESSION_RUNTIME_PRINCIPAL_ID,
            vector: 6,
            rip: 0x0040_0000,
            rsp: 0x7200_5000,
            cr2: 0,
        };

        assert_eq!(
            validate_session_runtime_user_outcome(
                Err(crate::user_mode::UserModeError::FaultContained(context)),
                true,
                true,
            ),
            Ok(SessionRuntimeUserOutcome::FaultContained(context))
        );
        assert_eq!(
            validate_session_runtime_user_outcome(
                Err(crate::user_mode::UserModeError::FaultContained(
                    crate::user_mode::UserFaultContext {
                        principal: 7,
                        ..context
                    },
                )),
                true,
                true,
            ),
            Err(SessionRuntimeProbeError::FaultPrincipalMismatch)
        );
        assert_eq!(
            validate_session_runtime_user_outcome(
                Err(crate::user_mode::UserModeError::FaultContained(context)),
                true,
                false,
            ),
            Err(SessionRuntimeProbeError::CallerStillBound)
        );
        assert_eq!(
            validate_session_runtime_user_outcome(
                Err(crate::user_mode::UserModeError::FaultContained(context)),
                false,
                true,
            ),
            Err(SessionRuntimeProbeError::KernelRootNotRestored)
        );
    }

    #[test]
    fn session_runtime_normal_return_contract_remains_distinct_from_contained_fault() {
        assert_eq!(
            validate_session_runtime_user_outcome(Ok(()), true, true),
            Ok(SessionRuntimeUserOutcome::Completed)
        );
        assert_eq!(
            validate_session_runtime_user_outcome(
                Err(crate::user_mode::UserModeError::DidNotReturn),
                true,
                true,
            ),
            Err(SessionRuntimeProbeError::UserMode(
                crate::user_mode::UserModeError::DidNotReturn
            ))
        );
    }

    fn accepted_bootstrap() -> SessionRuntimeBootstrapV1 {
        let mut bootstrap = SessionRuntimeBootstrapV1::empty();
        bootstrap.magic = pythos_shared::session_runtime_abi::SESSION_RUNTIME_BOOTSTRAP_MAGIC;
        bootstrap.abi_major = SESSION_RUNTIME_ABI_MAJOR;
        bootstrap.abi_minor = SESSION_RUNTIME_ABI_MINOR;
        bootstrap.command_count = SESSION_RUNTIME_COMMAND_COUNT as u16;
        bootstrap.session_service_id = SESSION_RUNTIME_SERVICE_ID_RAW;
        bootstrap.runtime_principal_id = SESSION_RUNTIME_PRINCIPAL_ID;
        bootstrap.graph_principal_id = SESSION_RUNTIME_GRAPH_PRINCIPAL_ID;
        bootstrap.graph_package_digest = 0xBBBB;
        bootstrap.input_capability = PackedCapability::from_raw(3);
        bootstrap.console_capability = PackedCapability::from_raw(1);
        bootstrap.fixture_ptr = SESSION_RUNTIME_FIXTURE_USER_PTR;
        bootstrap.fixture_len = size_of::<SessionRuntimeFixtureV1>() as u64;
        bootstrap.result_ptr = SESSION_RUNTIME_RESULT_USER_PTR;
        bootstrap.result_len = size_of::<SessionRuntimeResultV1>() as u64;
        bootstrap.graph.magic = pythos_shared::pyth_runtime_abi::PYTH_GRAPH_BOOTSTRAP_MAGIC;
        bootstrap.graph.abi_major = pythos_shared::pyth_runtime_abi::PYTH_GRAPH_RUNTIME_ABI_MAJOR;
        bootstrap.graph.abi_minor = pythos_shared::pyth_runtime_abi::PYTH_GRAPH_RUNTIME_ABI_MINOR;
        bootstrap.graph.import_count = 1;
        bootstrap.graph.package_ptr = SESSION_RUNTIME_PACKAGE_USER_PTR;
        bootstrap.graph.package_len = 512;
        bootstrap.graph.instruction_budget = 128;
        bootstrap.graph.result_ptr = SESSION_RUNTIME_RESULT_USER_PTR;
        bootstrap.graph.imports[0] = pythos_shared::pyth_runtime_abi::PythGraphCapabilityBinding {
            import_slot: 0,
            resource_kind: pythos_shared::pyth_tig::RESOURCE_COMMAND,
            reserved0: 0,
            rights: pythos_shared::pyth_tig::opcode::RIGHTS_READ
                | pythos_shared::pyth_tig::opcode::RIGHTS_APPEND,
            capability: PackedCapability::from_raw(2),
        };
        bootstrap
    }

    fn accepted_result(
        bootstrap: &SessionRuntimeBootstrapV1,
        fixture: &SessionRuntimeFixtureV1,
    ) -> SessionRuntimeResultV1 {
        let mut result = SessionRuntimeResultV1::empty();
        result.magic = pythos_shared::session_runtime_abi::SESSION_RUNTIME_RESULT_MAGIC;
        result.abi_major = SESSION_RUNTIME_ABI_MAJOR;
        result.abi_minor = SESSION_RUNTIME_ABI_MINOR;
        result.terminal_status = SESSION_RUNTIME_RESULT_COMPLETE;
        result.last_lifecycle_action = SESSION_RUNTIME_LIFECYCLE_REINVOKE;
        result.session_service_id = bootstrap.session_service_id;
        result.runtime_principal_id = bootstrap.runtime_principal_id;
        result.graph_principal_id = bootstrap.graph_principal_id;
        result.input_event_count = 2;
        result.invocation_count = 2;
        result.retained_state_before_second = 1;
        result.retained_state_final = 2;
        for ordinal in 0..SESSION_RUNTIME_COMMAND_COUNT {
            result.command_results[ordinal] = PythCommandResult {
                status: COMMAND_RESULT_STATUS_OK,
                kind: fixture.commands[ordinal].kind,
                reserved0: 0,
                object_id: fixture.commands[ordinal].object_id,
                task_id: 0,
                proposal_id: 0,
                bytes_written: fixture.commands[ordinal].payload_len,
                reserved1: 0,
            };
            result.graph_exits[ordinal] = GraphExitRecord {
                status: GRAPH_EXIT_OK,
                error_code: 0,
                last_node: 2,
                executed_nodes: 3,
                result_type: GRAPH_RESULT_UNIT,
                reserved0: 0,
                reserved1: 0,
                result_raw: 0,
            };
        }
        result
    }
}
