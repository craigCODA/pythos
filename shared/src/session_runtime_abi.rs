use crate::object_shell_abi::PackedCapability;
use crate::pyth_command_abi::{
    COMMAND_FLAG_NONE, COMMAND_KIND_CREATE_NOTE, PYTH_COMMAND_ABI_MAJOR, PYTH_COMMAND_ABI_MINOR,
    PythCommand, PythCommandResult,
};
use crate::pyth_runtime_abi::{
    GRAPH_RESULT_UNIT, GraphExitRecord, PYTH_GRAPH_BOOTSTRAP_MAGIC, PYTH_GRAPH_RUNTIME_ABI_MAJOR,
    PYTH_GRAPH_RUNTIME_ABI_MINOR, PythGraphBootstrapBlock, PythGraphCapabilityBinding,
};

pub const SESSION_RUNTIME_BOOTSTRAP_MAGIC: u64 = 0x3154_5253_4553_5950; // "PYSESRT1"
pub const SESSION_RUNTIME_FIXTURE_MAGIC: u64 = 0x314D_4353_4553_5950; // "PYSESCM1"
pub const SESSION_RUNTIME_RESULT_MAGIC: u64 = 0x3130_5253_4553_5950; // "PYSESR01"
pub const SESSION_RUNTIME_ABI_MAJOR: u16 = 1;
pub const SESSION_RUNTIME_ABI_MINOR: u16 = 0;
pub const SESSION_RUNTIME_COMMAND_COUNT: usize = 2;
pub const SESSION_RUNTIME_MAX_COMMAND_PAYLOAD: usize = 32;
pub const SESSION_RUNTIME_EMPTY_POLL_LIMIT: u64 = 100_000_000;
pub const SESSION_COMMAND_RESOURCE_ID: u64 = 0x5059_5345_5343_4D44;
pub const SESSION_RUNTIME_RESULT_UNINITIALIZED: u16 = 0;
pub const SESSION_RUNTIME_RESULT_COMPLETE: u16 = 1;
pub const SESSION_RUNTIME_RESULT_REQUEST_RECOVERY: u16 = 2;
pub const SESSION_RUNTIME_LIFECYCLE_NONE: u16 = 0;
pub const SESSION_RUNTIME_LIFECYCLE_REINVOKE: u16 = 1;
pub const SESSION_RUNTIME_LIFECYCLE_REQUEST_RECOVERY: u16 = 2;

// PythTIG v1 freezes these values. This ABI must stay available without the
// optional `pyth-tig` feature, so it carries their fixed wire values rather
// than widening the PythTIG feature boundary.
const PYTH_TIG_RESOURCE_COMMAND: u16 = 6;
const PYTH_TIG_SESSION_COMMAND_RIGHTS: u64 = 0x0001 | 0x0010;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionRuntimeBootstrapV1 {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub command_count: u16,
    pub reserved0: u16,
    pub session_service_id: u64,
    pub runtime_principal_id: u64,
    pub graph_principal_id: u64,
    pub graph_package_digest: u64,
    pub input_capability: PackedCapability,
    pub console_capability: PackedCapability,
    pub fixture_ptr: u64,
    pub fixture_len: u64,
    pub result_ptr: u64,
    pub result_len: u64,
    pub graph: PythGraphBootstrapBlock,
    pub reserved1: [u64; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionRuntimeFixtureV1 {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub command_count: u16,
    pub reserved0: u16,
    pub commands: [PythCommand; SESSION_RUNTIME_COMMAND_COUNT],
    pub payloads: [[u8; SESSION_RUNTIME_MAX_COMMAND_PAYLOAD]; SESSION_RUNTIME_COMMAND_COUNT],
    pub reserved1: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionRuntimeResultV1 {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub terminal_status: u16,
    pub last_lifecycle_action: u16,
    pub session_service_id: u64,
    pub runtime_principal_id: u64,
    pub graph_principal_id: u64,
    pub input_event_count: u64,
    pub invocation_count: u64,
    pub retained_state_before_second: u64,
    pub retained_state_final: u64,
    pub command_results: [PythCommandResult; SESSION_RUNTIME_COMMAND_COUNT],
    pub graph_exits: [GraphExitRecord; SESSION_RUNTIME_COMMAND_COUNT],
    pub reserved1: [u64; 3],
}

impl SessionRuntimeBootstrapV1 {
    pub const fn empty() -> Self {
        Self {
            magic: 0,
            abi_major: 0,
            abi_minor: 0,
            command_count: 0,
            reserved0: 0,
            session_service_id: 0,
            runtime_principal_id: 0,
            graph_principal_id: 0,
            graph_package_digest: 0,
            input_capability: PackedCapability::from_raw(0),
            console_capability: PackedCapability::from_raw(0),
            fixture_ptr: 0,
            fixture_len: 0,
            result_ptr: 0,
            result_len: 0,
            graph: empty_graph_bootstrap(),
            reserved1: [0; 4],
        }
    }
}

impl SessionRuntimeFixtureV1 {
    pub const fn empty() -> Self {
        Self {
            magic: 0,
            abi_major: 0,
            abi_minor: 0,
            command_count: 0,
            reserved0: 0,
            commands: [PythCommand::empty(0); SESSION_RUNTIME_COMMAND_COUNT],
            payloads: [[0; SESSION_RUNTIME_MAX_COMMAND_PAYLOAD]; SESSION_RUNTIME_COMMAND_COUNT],
            reserved1: [0; 2],
        }
    }
}

impl SessionRuntimeResultV1 {
    pub const fn empty() -> Self {
        Self {
            magic: 0,
            abi_major: 0,
            abi_minor: 0,
            terminal_status: 0,
            last_lifecycle_action: 0,
            session_service_id: 0,
            runtime_principal_id: 0,
            graph_principal_id: 0,
            input_event_count: 0,
            invocation_count: 0,
            retained_state_before_second: 0,
            retained_state_final: 0,
            command_results: [PythCommandResult::empty(0, 0); SESSION_RUNTIME_COMMAND_COUNT],
            graph_exits: [empty_graph_exit_record(); SESSION_RUNTIME_COMMAND_COUNT],
            reserved1: [0; 3],
        }
    }
}

const fn empty_graph_bootstrap() -> PythGraphBootstrapBlock {
    PythGraphBootstrapBlock {
        magic: 0,
        abi_major: 0,
        abi_minor: 0,
        import_count: 0,
        reserved0: 0,
        package_ptr: 0,
        package_len: 0,
        instruction_budget: 0,
        result_ptr: 0,
        imports: [empty_graph_import(); crate::pyth_runtime_abi::MAX_PYTH_GRAPH_IMPORTS],
    }
}

const fn empty_graph_import() -> PythGraphCapabilityBinding {
    PythGraphCapabilityBinding {
        import_slot: 0,
        resource_kind: 0,
        reserved0: 0,
        rights: 0,
        capability: PackedCapability::from_raw(0),
    }
}

const fn empty_graph_exit_record() -> GraphExitRecord {
    GraphExitRecord {
        status: 0,
        error_code: 0,
        last_node: 0,
        executed_nodes: 0,
        result_type: 0,
        reserved0: 0,
        reserved1: 0,
        result_raw: 0,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionRuntimeValidationError {
    BadBootstrapMagic,
    BadFixtureMagic,
    BadResultMagic,
    UnsupportedVersion,
    BadCommandCount,
    NonZeroReserved,
    ZeroIdentity,
    IdentityCollision,
    ZeroCapability,
    BadFixtureRange,
    BadResultRange,
    ResultOverlapsFixture,
    BadGraphBootstrap,
    BadGraphImport,
    BadCommandVersion,
    UnexpectedCommandKind,
    BadCommandFlags,
    NonZeroCommandReserved,
    PayloadTooLong,
    BadPayloadPointer,
    BadPayload,
    NonZeroPayloadTail,
    BadTerminalStatus,
    BadLifecycleAction,
    ResultIdentityMismatch,
    BadCommandResult,
    BadGraphExit,
}

pub fn validate_session_runtime_bootstrap(
    bootstrap: &SessionRuntimeBootstrapV1,
) -> Result<(), SessionRuntimeValidationError> {
    if bootstrap.magic != SESSION_RUNTIME_BOOTSTRAP_MAGIC {
        return Err(SessionRuntimeValidationError::BadBootstrapMagic);
    }
    validate_version(bootstrap.abi_major, bootstrap.abi_minor)?;
    if bootstrap.command_count as usize != SESSION_RUNTIME_COMMAND_COUNT {
        return Err(SessionRuntimeValidationError::BadCommandCount);
    }
    if bootstrap.reserved0 != 0 || bootstrap.reserved1 != [0; 4] {
        return Err(SessionRuntimeValidationError::NonZeroReserved);
    }
    validate_identities(
        bootstrap.session_service_id,
        bootstrap.runtime_principal_id,
        bootstrap.graph_principal_id,
    )?;
    if bootstrap.input_capability.raw() == 0 || bootstrap.console_capability.raw() == 0 {
        return Err(SessionRuntimeValidationError::ZeroCapability);
    }
    if bootstrap.fixture_ptr == 0
        || bootstrap.fixture_len != core::mem::size_of::<SessionRuntimeFixtureV1>() as u64
    {
        return Err(SessionRuntimeValidationError::BadFixtureRange);
    }
    if bootstrap.result_ptr == 0
        || bootstrap.result_len != core::mem::size_of::<SessionRuntimeResultV1>() as u64
    {
        return Err(SessionRuntimeValidationError::BadResultRange);
    }
    if ranges_overlap(
        bootstrap.fixture_ptr,
        bootstrap.fixture_len,
        bootstrap.result_ptr,
        bootstrap.result_len,
    )? {
        return Err(SessionRuntimeValidationError::ResultOverlapsFixture);
    }
    validate_graph(&bootstrap.graph)
}

pub fn validate_session_runtime_fixture(
    bootstrap: &SessionRuntimeBootstrapV1,
    fixture: &SessionRuntimeFixtureV1,
) -> Result<(), SessionRuntimeValidationError> {
    validate_session_runtime_bootstrap(bootstrap)?;
    if fixture.magic != SESSION_RUNTIME_FIXTURE_MAGIC {
        return Err(SessionRuntimeValidationError::BadFixtureMagic);
    }
    validate_version(fixture.abi_major, fixture.abi_minor)?;
    if fixture.command_count as usize != SESSION_RUNTIME_COMMAND_COUNT {
        return Err(SessionRuntimeValidationError::BadCommandCount);
    }
    if fixture.reserved0 != 0 || fixture.reserved1 != [0; 2] {
        return Err(SessionRuntimeValidationError::NonZeroReserved);
    }

    let payload_base = bootstrap
        .fixture_ptr
        .checked_add(core::mem::offset_of!(SessionRuntimeFixtureV1, payloads) as u64)
        .ok_or(SessionRuntimeValidationError::BadFixtureRange)?;
    for ordinal in 0..SESSION_RUNTIME_COMMAND_COUNT {
        let command = fixture.commands[ordinal];
        if command.abi_major != PYTH_COMMAND_ABI_MAJOR
            || command.abi_minor != PYTH_COMMAND_ABI_MINOR
        {
            return Err(SessionRuntimeValidationError::BadCommandVersion);
        }
        if command.kind != COMMAND_KIND_CREATE_NOTE {
            return Err(SessionRuntimeValidationError::UnexpectedCommandKind);
        }
        if command.flags != COMMAND_FLAG_NONE {
            return Err(SessionRuntimeValidationError::BadCommandFlags);
        }
        if command.reserved0 != 0 || command.reserved1 != 0 {
            return Err(SessionRuntimeValidationError::NonZeroCommandReserved);
        }
        if command.payload_len > SESSION_RUNTIME_MAX_COMMAND_PAYLOAD as u64 {
            return Err(SessionRuntimeValidationError::PayloadTooLong);
        }
        let expected_payload_ptr = payload_base
            .checked_add((ordinal * SESSION_RUNTIME_MAX_COMMAND_PAYLOAD) as u64)
            .ok_or(SessionRuntimeValidationError::BadPayloadPointer)?;
        if command.payload_ptr != expected_payload_ptr {
            return Err(SessionRuntimeValidationError::BadPayloadPointer);
        }
        let payload_len = command.payload_len as usize;
        let payload = &fixture.payloads[ordinal][..payload_len];
        if core::str::from_utf8(payload).is_err() || !payload.is_ascii() {
            return Err(SessionRuntimeValidationError::BadPayload);
        }
        if fixture.payloads[ordinal][payload_len..]
            != [0; SESSION_RUNTIME_MAX_COMMAND_PAYLOAD][payload_len..]
        {
            return Err(SessionRuntimeValidationError::NonZeroPayloadTail);
        }
    }
    Ok(())
}

pub fn validate_session_runtime_result(
    bootstrap: &SessionRuntimeBootstrapV1,
    result: &SessionRuntimeResultV1,
) -> Result<(), SessionRuntimeValidationError> {
    validate_session_runtime_bootstrap(bootstrap)?;
    if result.magic != SESSION_RUNTIME_RESULT_MAGIC {
        return Err(SessionRuntimeValidationError::BadResultMagic);
    }
    validate_version(result.abi_major, result.abi_minor)?;
    if result.reserved1 != [0; 3] {
        return Err(SessionRuntimeValidationError::NonZeroReserved);
    }
    validate_identities(
        result.session_service_id,
        result.runtime_principal_id,
        result.graph_principal_id,
    )?;
    if result.session_service_id != bootstrap.session_service_id
        || result.runtime_principal_id != bootstrap.runtime_principal_id
        || result.graph_principal_id != bootstrap.graph_principal_id
    {
        return Err(SessionRuntimeValidationError::ResultIdentityMismatch);
    }
    if !matches!(
        result.terminal_status,
        SESSION_RUNTIME_RESULT_UNINITIALIZED
            | SESSION_RUNTIME_RESULT_COMPLETE
            | SESSION_RUNTIME_RESULT_REQUEST_RECOVERY
    ) {
        return Err(SessionRuntimeValidationError::BadTerminalStatus);
    }
    if !matches!(
        result.last_lifecycle_action,
        SESSION_RUNTIME_LIFECYCLE_NONE
            | SESSION_RUNTIME_LIFECYCLE_REINVOKE
            | SESSION_RUNTIME_LIFECYCLE_REQUEST_RECOVERY
    ) {
        return Err(SessionRuntimeValidationError::BadLifecycleAction);
    }
    for command_result in result.command_results {
        if command_result.reserved0 != 0 || command_result.reserved1 != 0 {
            return Err(SessionRuntimeValidationError::BadCommandResult);
        }
    }
    for graph_exit in result.graph_exits {
        if graph_exit.result_type != GRAPH_RESULT_UNIT
            || graph_exit.reserved0 != 0
            || graph_exit.reserved1 != 0
        {
            return Err(SessionRuntimeValidationError::BadGraphExit);
        }
    }
    Ok(())
}

fn validate_version(major: u16, minor: u16) -> Result<(), SessionRuntimeValidationError> {
    if major == SESSION_RUNTIME_ABI_MAJOR && minor == SESSION_RUNTIME_ABI_MINOR {
        Ok(())
    } else {
        Err(SessionRuntimeValidationError::UnsupportedVersion)
    }
}

fn validate_identities(
    service_id: u64,
    runtime_principal_id: u64,
    graph_principal_id: u64,
) -> Result<(), SessionRuntimeValidationError> {
    if service_id == 0 || runtime_principal_id == 0 || graph_principal_id == 0 {
        return Err(SessionRuntimeValidationError::ZeroIdentity);
    }
    if service_id == runtime_principal_id
        || service_id == graph_principal_id
        || runtime_principal_id == graph_principal_id
    {
        return Err(SessionRuntimeValidationError::IdentityCollision);
    }
    Ok(())
}

fn ranges_overlap(
    first_start: u64,
    first_len: u64,
    second_start: u64,
    second_len: u64,
) -> Result<bool, SessionRuntimeValidationError> {
    let first_end = first_start
        .checked_add(first_len)
        .ok_or(SessionRuntimeValidationError::BadFixtureRange)?;
    let second_end = second_start
        .checked_add(second_len)
        .ok_or(SessionRuntimeValidationError::BadResultRange)?;
    Ok(first_start < second_end && second_start < first_end)
}

fn validate_graph(graph: &PythGraphBootstrapBlock) -> Result<(), SessionRuntimeValidationError> {
    if graph.magic != PYTH_GRAPH_BOOTSTRAP_MAGIC
        || graph.abi_major != PYTH_GRAPH_RUNTIME_ABI_MAJOR
        || graph.abi_minor != PYTH_GRAPH_RUNTIME_ABI_MINOR
        || graph.import_count != 1
        || graph.reserved0 != 0
    {
        return Err(SessionRuntimeValidationError::BadGraphBootstrap);
    }
    let import = graph.imports[0];
    if import.import_slot != 0
        || import.resource_kind != PYTH_TIG_RESOURCE_COMMAND
        || import.reserved0 != 0
        || import.rights != PYTH_TIG_SESSION_COMMAND_RIGHTS
        || import.capability.raw() == 0
    {
        return Err(SessionRuntimeValidationError::BadGraphImport);
    }
    for tail in &graph.imports[1..] {
        if *tail != empty_graph_import() {
            return Err(SessionRuntimeValidationError::BadGraphImport);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    const FIXTURE_PTR: u64 = 0x0000_0000_0020_0000;
    const RESULT_PTR: u64 = 0x0000_0000_0020_1000;

    #[test]
    fn session_runtime_records_have_the_frozen_c_layout() {
        assert_eq!(size_of::<SessionRuntimeBootstrapV1>(), 944);
        assert_eq!(align_of::<SessionRuntimeBootstrapV1>(), 8);
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, magic), 0);
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, abi_major), 8);
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, abi_minor), 10);
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, command_count), 12);
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, reserved0), 14);
        assert_eq!(
            offset_of!(SessionRuntimeBootstrapV1, session_service_id),
            16
        );
        assert_eq!(
            offset_of!(SessionRuntimeBootstrapV1, runtime_principal_id),
            24
        );
        assert_eq!(
            offset_of!(SessionRuntimeBootstrapV1, graph_principal_id),
            32
        );
        assert_eq!(
            offset_of!(SessionRuntimeBootstrapV1, graph_package_digest),
            40
        );
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, input_capability), 48);
        assert_eq!(
            offset_of!(SessionRuntimeBootstrapV1, console_capability),
            56
        );
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, fixture_ptr), 64);
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, fixture_len), 72);
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, result_ptr), 80);
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, result_len), 88);
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, graph), 96);
        assert_eq!(offset_of!(SessionRuntimeBootstrapV1, reserved1), 912);

        assert_eq!(size_of::<SessionRuntimeFixtureV1>(), 224);
        assert_eq!(align_of::<SessionRuntimeFixtureV1>(), 8);
        assert_eq!(offset_of!(SessionRuntimeFixtureV1, magic), 0);
        assert_eq!(offset_of!(SessionRuntimeFixtureV1, abi_major), 8);
        assert_eq!(offset_of!(SessionRuntimeFixtureV1, abi_minor), 10);
        assert_eq!(offset_of!(SessionRuntimeFixtureV1, command_count), 12);
        assert_eq!(offset_of!(SessionRuntimeFixtureV1, reserved0), 14);
        assert_eq!(offset_of!(SessionRuntimeFixtureV1, commands), 16);
        assert_eq!(offset_of!(SessionRuntimeFixtureV1, payloads), 144);
        assert_eq!(offset_of!(SessionRuntimeFixtureV1, reserved1), 208);

        assert_eq!(size_of::<SessionRuntimeResultV1>(), 256);
        assert_eq!(align_of::<SessionRuntimeResultV1>(), 8);
        assert_eq!(offset_of!(SessionRuntimeResultV1, magic), 0);
        assert_eq!(offset_of!(SessionRuntimeResultV1, abi_major), 8);
        assert_eq!(offset_of!(SessionRuntimeResultV1, abi_minor), 10);
        assert_eq!(offset_of!(SessionRuntimeResultV1, terminal_status), 12);
        assert_eq!(
            offset_of!(SessionRuntimeResultV1, last_lifecycle_action),
            14
        );
        assert_eq!(offset_of!(SessionRuntimeResultV1, session_service_id), 16);
        assert_eq!(offset_of!(SessionRuntimeResultV1, runtime_principal_id), 24);
        assert_eq!(offset_of!(SessionRuntimeResultV1, graph_principal_id), 32);
        assert_eq!(offset_of!(SessionRuntimeResultV1, input_event_count), 40);
        assert_eq!(offset_of!(SessionRuntimeResultV1, invocation_count), 48);
        assert_eq!(
            offset_of!(SessionRuntimeResultV1, retained_state_before_second),
            56
        );
        assert_eq!(offset_of!(SessionRuntimeResultV1, retained_state_final), 64);
        assert_eq!(offset_of!(SessionRuntimeResultV1, command_results), 72);
        assert_eq!(offset_of!(SessionRuntimeResultV1, graph_exits), 168);
        assert_eq!(offset_of!(SessionRuntimeResultV1, reserved1), 232);

        assert!(size_of::<SessionRuntimeBootstrapV1>() <= 4096);
        assert!(size_of::<SessionRuntimeFixtureV1>() <= 4096);
        assert!(size_of::<SessionRuntimeResultV1>() <= 4096);
    }

    #[test]
    fn two_create_note_commands_use_their_matching_zero_filled_payload_slots() {
        let (bootstrap, fixture) = accepted_bootstrap_and_fixture();

        assert_eq!(fixture.commands[0].kind, COMMAND_KIND_CREATE_NOTE);
        assert_eq!(fixture.commands[1].kind, COMMAND_KIND_CREATE_NOTE);
        assert_eq!(&fixture.payloads[0][..10], b"slice2-one");
        assert_eq!(&fixture.payloads[1][..10], b"slice2-two");
        assert_eq!(fixture.payloads[0][10..], [0; 22]);
        assert_eq!(fixture.payloads[1][10..], [0; 22]);
        assert_eq!(
            validate_session_runtime_fixture(&bootstrap, &fixture),
            Ok(())
        );
    }

    #[test]
    fn bootstrap_rejects_zero_or_colliding_identities_capabilities_and_overlapping_result() {
        let (mut bootstrap, _) = accepted_bootstrap_and_fixture();
        bootstrap.session_service_id = 0;
        assert_eq!(
            validate_session_runtime_bootstrap(&bootstrap),
            Err(SessionRuntimeValidationError::ZeroIdentity)
        );

        bootstrap.session_service_id = bootstrap.runtime_principal_id;
        assert_eq!(
            validate_session_runtime_bootstrap(&bootstrap),
            Err(SessionRuntimeValidationError::IdentityCollision)
        );

        bootstrap.session_service_id = 0x5059_5345_5353_0001;
        bootstrap.input_capability = PackedCapability::from_raw(0);
        assert_eq!(
            validate_session_runtime_bootstrap(&bootstrap),
            Err(SessionRuntimeValidationError::ZeroCapability)
        );

        bootstrap.input_capability = PackedCapability::from_raw(1);
        bootstrap.result_ptr = FIXTURE_PTR + 1;
        assert_eq!(
            validate_session_runtime_bootstrap(&bootstrap),
            Err(SessionRuntimeValidationError::ResultOverlapsFixture)
        );
    }

    #[test]
    fn fixture_rejects_commands_outside_the_single_create_note_ascii_contract() {
        let (bootstrap, mut fixture) = accepted_bootstrap_and_fixture();
        fixture.commands[0].kind = 0xffff;
        assert_eq!(
            validate_session_runtime_fixture(&bootstrap, &fixture),
            Err(SessionRuntimeValidationError::UnexpectedCommandKind)
        );

        let (_, mut fixture) = accepted_bootstrap_and_fixture();
        fixture.commands[0].flags = 1;
        assert_eq!(
            validate_session_runtime_fixture(&bootstrap, &fixture),
            Err(SessionRuntimeValidationError::BadCommandFlags)
        );

        let (_, mut fixture) = accepted_bootstrap_and_fixture();
        fixture.commands[0].payload_ptr += 1;
        assert_eq!(
            validate_session_runtime_fixture(&bootstrap, &fixture),
            Err(SessionRuntimeValidationError::BadPayloadPointer)
        );

        let (_, mut fixture) = accepted_bootstrap_and_fixture();
        fixture.payloads[0][0] = 0xff;
        assert_eq!(
            validate_session_runtime_fixture(&bootstrap, &fixture),
            Err(SessionRuntimeValidationError::BadPayload)
        );

        let (_, mut fixture) = accepted_bootstrap_and_fixture();
        fixture.payloads[0][10] = b'x';
        assert_eq!(
            validate_session_runtime_fixture(&bootstrap, &fixture),
            Err(SessionRuntimeValidationError::NonZeroPayloadTail)
        );
    }

    #[test]
    fn result_rejects_unknown_discriminants_and_reserved_words() {
        let (bootstrap, _) = accepted_bootstrap_and_fixture();
        let mut result = accepted_result(&bootstrap);
        result.terminal_status = 99;
        assert_eq!(
            validate_session_runtime_result(&bootstrap, &result),
            Err(SessionRuntimeValidationError::BadTerminalStatus)
        );

        result.terminal_status = SESSION_RUNTIME_RESULT_COMPLETE;
        result.last_lifecycle_action = 99;
        assert_eq!(
            validate_session_runtime_result(&bootstrap, &result),
            Err(SessionRuntimeValidationError::BadLifecycleAction)
        );

        result.last_lifecycle_action = SESSION_RUNTIME_LIFECYCLE_REINVOKE;
        result.reserved1[0] = 1;
        assert_eq!(
            validate_session_runtime_result(&bootstrap, &result),
            Err(SessionRuntimeValidationError::NonZeroReserved)
        );
    }

    fn accepted_bootstrap_and_fixture() -> (SessionRuntimeBootstrapV1, SessionRuntimeFixtureV1) {
        let mut bootstrap = SessionRuntimeBootstrapV1::empty();
        bootstrap.magic = SESSION_RUNTIME_BOOTSTRAP_MAGIC;
        bootstrap.abi_major = SESSION_RUNTIME_ABI_MAJOR;
        bootstrap.abi_minor = SESSION_RUNTIME_ABI_MINOR;
        bootstrap.command_count = SESSION_RUNTIME_COMMAND_COUNT as u16;
        bootstrap.session_service_id = 0x5059_5345_5353_0001;
        bootstrap.runtime_principal_id = 0x5059_5352_544D_0001;
        bootstrap.graph_principal_id = 0x5059_5448_534D_0001;
        bootstrap.graph_package_digest = 1;
        bootstrap.input_capability = PackedCapability::from_raw(1);
        bootstrap.console_capability = PackedCapability::from_raw(2);
        bootstrap.fixture_ptr = FIXTURE_PTR;
        bootstrap.fixture_len = size_of::<SessionRuntimeFixtureV1>() as u64;
        bootstrap.result_ptr = RESULT_PTR;
        bootstrap.result_len = size_of::<SessionRuntimeResultV1>() as u64;
        bootstrap.graph.magic = PYTH_GRAPH_BOOTSTRAP_MAGIC;
        bootstrap.graph.abi_major = PYTH_GRAPH_RUNTIME_ABI_MAJOR;
        bootstrap.graph.abi_minor = PYTH_GRAPH_RUNTIME_ABI_MINOR;
        bootstrap.graph.import_count = 1;
        bootstrap.graph.imports[0] = PythGraphCapabilityBinding {
            import_slot: 0,
            resource_kind: PYTH_TIG_RESOURCE_COMMAND,
            reserved0: 0,
            rights: PYTH_TIG_SESSION_COMMAND_RIGHTS,
            capability: PackedCapability::from_raw(3),
        };

        let mut fixture = SessionRuntimeFixtureV1::empty();
        fixture.magic = SESSION_RUNTIME_FIXTURE_MAGIC;
        fixture.abi_major = SESSION_RUNTIME_ABI_MAJOR;
        fixture.abi_minor = SESSION_RUNTIME_ABI_MINOR;
        fixture.command_count = SESSION_RUNTIME_COMMAND_COUNT as u16;
        fixture.payloads[0][..10].copy_from_slice(b"slice2-one");
        fixture.payloads[1][..10].copy_from_slice(b"slice2-two");
        for ordinal in 0..SESSION_RUNTIME_COMMAND_COUNT {
            fixture.commands[ordinal] = PythCommand::empty(COMMAND_KIND_CREATE_NOTE);
            fixture.commands[ordinal].payload_ptr = FIXTURE_PTR
                + offset_of!(SessionRuntimeFixtureV1, payloads) as u64
                + (ordinal * SESSION_RUNTIME_MAX_COMMAND_PAYLOAD) as u64;
            fixture.commands[ordinal].payload_len = 10;
        }
        (bootstrap, fixture)
    }

    fn accepted_result(bootstrap: &SessionRuntimeBootstrapV1) -> SessionRuntimeResultV1 {
        let mut result = SessionRuntimeResultV1::empty();
        result.magic = SESSION_RUNTIME_RESULT_MAGIC;
        result.abi_major = SESSION_RUNTIME_ABI_MAJOR;
        result.abi_minor = SESSION_RUNTIME_ABI_MINOR;
        result.terminal_status = SESSION_RUNTIME_RESULT_COMPLETE;
        result.last_lifecycle_action = SESSION_RUNTIME_LIFECYCLE_REINVOKE;
        result.session_service_id = bootstrap.session_service_id;
        result.runtime_principal_id = bootstrap.runtime_principal_id;
        result.graph_principal_id = bootstrap.graph_principal_id;
        for graph_exit in &mut result.graph_exits {
            graph_exit.result_type = GRAPH_RESULT_UNIT;
        }
        result
    }
}
