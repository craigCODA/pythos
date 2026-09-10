#![no_std]

pub mod session_command_host;
pub mod session_viewing;

use pythos_shared::session_input_abi::{
    KEY_A, SESSION_INPUT_KIND_KEY_DOWN, SESSION_INPUT_KIND_RELATIVE_MOTION,
    SESSION_INPUT_SOURCE_KEYBOARD, SESSION_INPUT_SOURCE_MOUSE, SessionInputEventV1,
};
use pythos_shared::session_runtime_abi::{
    SessionRuntimeBootstrapV1, SessionRuntimeFixtureV1, SessionRuntimeValidationError,
    validate_session_runtime_bootstrap, validate_session_runtime_fixture,
};
use pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID;
use pythos_shared::{
    pyth_graph_manifest::digest64,
    pyth_tig::format::{PackageDecodeError, PythGraphPackage},
};

pub use pythos_shared::session_runtime_lifecycle::SessionGraphLifecycleAction;

pub const SESSION_RUNTIME_BOOTSTRAP_ADDRESS: u64 = 0x0000_0000_7200_0000;
pub const SESSION_RUNTIME_PACKAGE_ADDRESS: u64 = 0x0000_0000_7200_1000;
pub const SESSION_RUNTIME_FIXTURE_ADDRESS: u64 = 0x0000_0000_7200_2000;
pub const SESSION_RUNTIME_RESULT_ADDRESS: u64 = 0x0000_0000_7200_3000;
pub const SESSION_RUNTIME_PAGE_SIZE: u64 = 4096;
pub const SESSION_RUNTIME_SERVICE_ID: u64 = 0x5059_5345_5353_0001;
pub const SESSION_MANAGER_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_534D_0001;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionRuntimeLaunchError {
    NullBootstrapPointer,
    MisalignedBootstrapPointer,
    UnexpectedBootstrapAddress,
    InvalidBootstrap(SessionRuntimeValidationError),
    UnexpectedIdentity,
    UnexpectedPackageRange,
    UnexpectedFixtureRange,
    UnexpectedResultRange,
    InvalidFixture(SessionRuntimeValidationError),
    UnexpectedFixturePayload,
    PackageLengthMismatch,
    PackageDigestMismatch,
    InvalidPackage(PackageDecodeError),
}

pub fn validate_session_runtime_bootstrap_address(
    bootstrap_address: u64,
) -> Result<(), SessionRuntimeLaunchError> {
    if bootstrap_address == 0 {
        return Err(SessionRuntimeLaunchError::NullBootstrapPointer);
    }
    if !bootstrap_address.is_multiple_of(core::mem::align_of::<SessionRuntimeBootstrapV1>() as u64)
    {
        return Err(SessionRuntimeLaunchError::MisalignedBootstrapPointer);
    }
    if bootstrap_address != SESSION_RUNTIME_BOOTSTRAP_ADDRESS {
        return Err(SessionRuntimeLaunchError::UnexpectedBootstrapAddress);
    }
    Ok(())
}

pub fn validate_session_runtime_outer_bootstrap(
    bootstrap: &SessionRuntimeBootstrapV1,
) -> Result<(), SessionRuntimeLaunchError> {
    validate_session_runtime_bootstrap(bootstrap)
        .map_err(SessionRuntimeLaunchError::InvalidBootstrap)?;
    if bootstrap.session_service_id != SESSION_RUNTIME_SERVICE_ID
        || bootstrap.runtime_principal_id != SESSION_RUNTIME_PRINCIPAL_ID
        || bootstrap.graph_principal_id != SESSION_MANAGER_GRAPH_PRINCIPAL_ID
    {
        return Err(SessionRuntimeLaunchError::UnexpectedIdentity);
    }
    if bootstrap.graph_package_digest == 0
        || bootstrap.graph.package_ptr != SESSION_RUNTIME_PACKAGE_ADDRESS
        || bootstrap.graph.package_len == 0
        || bootstrap.graph.package_len > SESSION_RUNTIME_PAGE_SIZE
    {
        return Err(SessionRuntimeLaunchError::UnexpectedPackageRange);
    }
    if bootstrap.fixture_ptr != SESSION_RUNTIME_FIXTURE_ADDRESS
        || bootstrap.fixture_len != core::mem::size_of::<SessionRuntimeFixtureV1>() as u64
    {
        return Err(SessionRuntimeLaunchError::UnexpectedFixtureRange);
    }
    if bootstrap.result_ptr != SESSION_RUNTIME_RESULT_ADDRESS
        || bootstrap.result_len
            != core::mem::size_of::<pythos_shared::session_runtime_abi::SessionRuntimeResultV1>()
                as u64
        || bootstrap.graph.result_ptr != SESSION_RUNTIME_RESULT_ADDRESS
    {
        return Err(SessionRuntimeLaunchError::UnexpectedResultRange);
    }
    Ok(())
}

pub fn validate_session_runtime_fixture_contract(
    bootstrap: &SessionRuntimeBootstrapV1,
    fixture: &SessionRuntimeFixtureV1,
) -> Result<(), SessionRuntimeLaunchError> {
    validate_session_runtime_fixture(bootstrap, fixture)
        .map_err(SessionRuntimeLaunchError::InvalidFixture)?;
    for (ordinal, expected) in [b"slice2-one".as_slice(), b"slice2-two".as_slice()]
        .into_iter()
        .enumerate()
    {
        let payload_len = fixture.commands[ordinal].payload_len as usize;
        if payload_len != expected.len() || &fixture.payloads[ordinal][..payload_len] != expected {
            return Err(SessionRuntimeLaunchError::UnexpectedFixturePayload);
        }
    }
    Ok(())
}

pub fn validate_session_runtime_package<'a>(
    bootstrap: &SessionRuntimeBootstrapV1,
    package_bytes: &'a [u8],
) -> Result<PythGraphPackage<'a>, SessionRuntimeLaunchError> {
    if package_bytes.len() as u64 != bootstrap.graph.package_len {
        return Err(SessionRuntimeLaunchError::PackageLengthMismatch);
    }
    if digest64(package_bytes) != bootstrap.graph_package_digest {
        return Err(SessionRuntimeLaunchError::PackageDigestMismatch);
    }
    PythGraphPackage::decode(package_bytes).map_err(SessionRuntimeLaunchError::InvalidPackage)
}

pub fn validate_session_runtime_launch(
    bootstrap_address: u64,
    bootstrap: &SessionRuntimeBootstrapV1,
    fixture: &SessionRuntimeFixtureV1,
) -> Result<(), SessionRuntimeLaunchError> {
    validate_session_runtime_bootstrap_address(bootstrap_address)?;
    validate_session_runtime_outer_bootstrap(bootstrap)?;
    validate_session_runtime_fixture_contract(bootstrap, fixture)
}

pub fn validate_session_runtime_recovery_boundary(
    bootstrap: &SessionRuntimeBootstrapV1,
) -> Result<(), SessionRuntimeLaunchError> {
    if bootstrap.console_capability.raw() == 0 {
        return Err(SessionRuntimeLaunchError::InvalidBootstrap(
            SessionRuntimeValidationError::ZeroCapability,
        ));
    }
    if bootstrap.result_ptr != SESSION_RUNTIME_RESULT_ADDRESS
        || bootstrap.result_len
            != core::mem::size_of::<pythos_shared::session_runtime_abi::SessionRuntimeResultV1>()
                as u64
    {
        return Err(SessionRuntimeLaunchError::UnexpectedResultRange);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionRuntimeTerminalResult {
    Complete,
    RequestRecovery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionRuntimeEffectError {
    Package,
    Poll,
    CommandHost,
    Graph,
    Reset,
    Result,
}

pub trait SessionRuntimeEffects {
    fn copy_bootstrap(&mut self) -> SessionRuntimeBootstrapV1;
    fn copy_fixture(&mut self) -> SessionRuntimeFixtureV1;
    fn prepare_graph_package(
        &mut self,
        bootstrap: &SessionRuntimeBootstrapV1,
    ) -> Result<(), SessionRuntimeEffectError>;
    fn poll_input(&mut self, ordinal: usize) -> Result<(), SessionRuntimeEffectError>;
    fn run_command_host_and_graph(
        &mut self,
        ordinal: usize,
    ) -> Result<(), SessionRuntimeEffectError>;
    fn reset_invocation_local(&mut self) -> Result<(), SessionRuntimeEffectError>;
    fn final_state_is_valid(&self) -> bool;
    fn write_terminal_result(
        &mut self,
        terminal: SessionRuntimeTerminalResult,
    ) -> Result<(), SessionRuntimeEffectError>;
    fn emit_marker(&mut self, marker: &'static str);
    fn trap(&mut self);
}

pub fn run_session_runtime_orchestration<E: SessionRuntimeEffects>(
    bootstrap_address: u64,
    effects: &mut E,
) {
    if validate_session_runtime_bootstrap_address(bootstrap_address).is_err() {
        effects.trap();
        return;
    }

    let bootstrap = effects.copy_bootstrap();
    if validate_session_runtime_recovery_boundary(&bootstrap).is_err() {
        effects.trap();
        return;
    }
    if validate_session_runtime_outer_bootstrap(&bootstrap).is_err() {
        recover_orchestration(effects);
        return;
    }

    let fixture = effects.copy_fixture();
    if validate_session_runtime_fixture_contract(&bootstrap, &fixture).is_err()
        || effects.prepare_graph_package(&bootstrap).is_err()
    {
        recover_orchestration(effects);
        return;
    }

    effects.emit_marker("PYTHOS:SESSION_RUNTIME:BOOT_STATE_0\r\n");
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_1\r\n");
    if effects.poll_input(0).is_err() {
        recover_orchestration(effects);
        return;
    }
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:EVENT_1_KEY_A_SEQUENCE_0\r\n");
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:COMMAND_1_SLICE2_ONE\r\n");
    if effects.run_command_host_and_graph(0).is_err() {
        recover_orchestration(effects);
        return;
    }
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:RESULT_1_SLICE2_ONE\r\n");
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:INVOCATION_1_EXIT_OK\r\n");
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:STATE_INPUTS_1_INVOCATIONS_1\r\n");

    if effects.reset_invocation_local().is_err() {
        recover_orchestration(effects);
        return;
    }
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:INVOCATION_LOCAL_RESET\r\n");
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_2\r\n");
    if effects.poll_input(1).is_err() {
        recover_orchestration(effects);
        return;
    }
    effects
        .emit_marker("PYTHOS:SESSION_RUNTIME:EVENT_2_RELATIVE_MOTION_DX_7_DY_NEG_7_SEQUENCE_1\r\n");
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:COMMAND_2_SLICE2_TWO\r\n");
    if effects.run_command_host_and_graph(1).is_err() {
        recover_orchestration(effects);
        return;
    }
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:RESULT_2_SLICE2_TWO\r\n");
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:INVOCATION_2_EXIT_OK\r\n");
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:STATE_INPUTS_2_INVOCATIONS_2\r\n");

    if !effects.final_state_is_valid() {
        recover_orchestration(effects);
        return;
    }
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:INPUT_CONTIGUOUS\r\n");
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:SESSION_ID_STABLE\r\n");
    if effects
        .write_terminal_result(SessionRuntimeTerminalResult::Complete)
        .is_err()
    {
        recover_orchestration(effects);
        return;
    }
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:READY\r\n");
    effects.trap();
}

fn recover_orchestration<E: SessionRuntimeEffects>(effects: &mut E) {
    let _ = effects.write_terminal_result(SessionRuntimeTerminalResult::RequestRecovery);
    effects.emit_marker("PYTHOS:SESSION_RUNTIME:ERROR\r\n");
    effects.trap();
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionRuntimeState {
    pub session_service_id: u64,
    pub input_event_count: u64,
    pub graph_invocation_count: u64,
    pub previous_graph_exit_status: Option<u16>,
    pub recovery_requested: bool,
}

impl SessionRuntimeState {
    pub const fn new(session_service_id: u64) -> Self {
        Self {
            session_service_id,
            input_event_count: 0,
            graph_invocation_count: 0,
            previous_graph_exit_status: None,
            recovery_requested: false,
        }
    }

    pub fn record_input_event(&mut self) {
        self.input_event_count += 1;
    }

    pub fn record_graph_exit(&mut self, status: u16) -> SessionGraphLifecycleAction {
        self.graph_invocation_count += 1;
        self.previous_graph_exit_status = Some(status);
        let action =
            pythos_shared::session_runtime_lifecycle::session_graph_lifecycle_action(status);
        if action == SessionGraphLifecycleAction::RequestRecovery {
            self.recovery_requested = true;
        }
        action
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputSequenceValidationError {
    Complete,
    Reserved,
    Flags,
    Sequence,
    Shape,
}

pub struct InputSequenceValidator {
    next_ordinal: usize,
    next_sequence: Option<u64>,
}

impl InputSequenceValidator {
    pub const fn new() -> Self {
        Self {
            next_ordinal: 0,
            next_sequence: None,
        }
    }

    pub const fn is_complete(&self) -> bool {
        self.next_ordinal == 2
    }

    pub fn accept(
        &mut self,
        event: SessionInputEventV1,
    ) -> Result<usize, InputSequenceValidationError> {
        if self.is_complete() {
            return Err(InputSequenceValidationError::Complete);
        }
        if event.reserved0 != 0 || event.reserved1 != 0 {
            return Err(InputSequenceValidationError::Reserved);
        }
        if event.flags != 0 {
            return Err(InputSequenceValidationError::Flags);
        }
        if let Some(expected) = self.next_sequence
            && event.sequence != expected
        {
            return Err(InputSequenceValidationError::Sequence);
        }
        if !self.matches_expected(event) {
            return Err(InputSequenceValidationError::Shape);
        }

        self.next_sequence = Some(event.sequence.wrapping_add(1));
        self.next_ordinal += 1;
        Ok(self.next_ordinal)
    }

    fn matches_expected(&self, event: SessionInputEventV1) -> bool {
        match self.next_ordinal {
            0 => {
                event.kind == SESSION_INPUT_KIND_KEY_DOWN
                    && event.source == SESSION_INPUT_SOURCE_KEYBOARD
                    && event.value0 == i32::from(KEY_A)
                    && event.value1 == 0
            }
            1 => {
                event.kind == SESSION_INPUT_KIND_RELATIVE_MOTION
                    && event.source == SESSION_INPUT_SOURCE_MOUSE
                    && event.value0 == 7
                    && event.value1 == -7
            }
            _ => false,
        }
    }
}

impl Default for InputSequenceValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::session_input_abi::{
        KEY_A, SESSION_INPUT_FLAG_GAP_BEFORE, SESSION_INPUT_KIND_KEY_DOWN,
        SESSION_INPUT_KIND_RELATIVE_MOTION, SESSION_INPUT_SOURCE_KEYBOARD,
        SESSION_INPUT_SOURCE_MOUSE, SessionInputEventV1,
    };
    use pythos_shared::{
        object_shell_abi::PackedCapability,
        pyth_command_abi::{COMMAND_KIND_CREATE_NOTE, PythCommand},
        pyth_runtime_abi::{
            GRAPH_EXIT_BUDGET_EXHAUSTED, GRAPH_EXIT_OK, HostCallResult, MAX_PYTH_GRAPH_IMPORTS,
            PYTH_GRAPH_BOOTSTRAP_MAGIC, PYTH_GRAPH_RUNTIME_ABI_MAJOR, PYTH_GRAPH_RUNTIME_ABI_MINOR,
            PythGraphCapabilityBinding,
        },
        pyth_tig::{
            format::{MAX_RUNTIME_VALUES, PythGraphPackage},
            opcode::{RIGHTS_APPEND, RIGHTS_READ},
            test_support,
            verify::verify_package,
        },
        session_runtime_abi::{
            SESSION_RUNTIME_ABI_MAJOR, SESSION_RUNTIME_ABI_MINOR, SESSION_RUNTIME_BOOTSTRAP_MAGIC,
            SESSION_RUNTIME_COMMAND_COUNT, SESSION_RUNTIME_FIXTURE_MAGIC,
            SESSION_RUNTIME_MAX_COMMAND_PAYLOAD, SessionRuntimeBootstrapV1,
            SessionRuntimeFixtureV1,
        },
    };
    use pythos_user_pyth_runtime::{interpreter::Interpreter, value::Value};

    use crate::session_command_host::SessionCommandHost;

    const SESSION_SERVICE_ID: u64 = 0x5059_5345_5353_0001;
    const RUNTIME_PRINCIPAL_ID: u64 = 0x5059_5352_544D_0001;
    const GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_534D_0001;

    fn key_a(sequence: u64) -> SessionInputEventV1 {
        SessionInputEventV1 {
            sequence,
            kind: SESSION_INPUT_KIND_KEY_DOWN,
            source: SESSION_INPUT_SOURCE_KEYBOARD,
            flags: 0,
            value0: i32::from(KEY_A),
            value1: 0,
            reserved0: 0,
            reserved1: 0,
        }
    }

    fn relative_motion(sequence: u64) -> SessionInputEventV1 {
        SessionInputEventV1 {
            sequence,
            kind: SESSION_INPUT_KIND_RELATIVE_MOTION,
            source: SESSION_INPUT_SOURCE_MOUSE,
            flags: 0,
            value0: 7,
            value1: -7,
            reserved0: 0,
            reserved1: 0,
        }
    }

    #[test]
    fn state_starts_neutral_for_one_retained_session_identity() {
        // Catches a retained session that starts with stale state from an earlier boot.
        assert_eq!(
            SessionRuntimeState::new(SESSION_SERVICE_ID),
            SessionRuntimeState {
                session_service_id: SESSION_SERVICE_ID,
                input_event_count: 0,
                graph_invocation_count: 0,
                previous_graph_exit_status: None,
                recovery_requested: false,
            }
        );
    }

    #[test]
    fn input_validator_accepts_the_exact_two_event_continuous_sequence() {
        // Catches accepting an input stream other than key A then motion (7, -7).
        let mut validator = InputSequenceValidator::new();
        assert_eq!(validator.accept(key_a(41)), Ok(1));
        assert_eq!(validator.accept(relative_motion(42)), Ok(2));
        assert!(validator.is_complete());
    }

    #[test]
    fn input_validator_accepts_sequence_number_wrap_continuity() {
        // Catches treating a valid u64 sequence wrap as a delivery gap.
        let mut validator = InputSequenceValidator::new();
        assert_eq!(validator.accept(key_a(u64::MAX)), Ok(1));
        assert_eq!(validator.accept(relative_motion(0)), Ok(2));
    }

    #[test]
    fn input_validator_rejects_gaps_gap_flags_wrong_shapes_and_extra_events() {
        // Catches accepting discontinuous, malformed, duplicate, reversed, or surplus input.
        let mut gap = InputSequenceValidator::new();
        assert_eq!(gap.accept(key_a(41)), Ok(1));
        assert_eq!(
            gap.accept(relative_motion(43)),
            Err(InputSequenceValidationError::Sequence)
        );

        let mut flagged = key_a(41);
        flagged.flags = SESSION_INPUT_FLAG_GAP_BEFORE;
        assert_eq!(
            InputSequenceValidator::new().accept(flagged),
            Err(InputSequenceValidationError::Flags)
        );

        let mutations = [
            {
                let mut event = key_a(41);
                event.source = SESSION_INPUT_SOURCE_MOUSE;
                event
            },
            {
                let mut event = key_a(41);
                event.kind = SESSION_INPUT_KIND_RELATIVE_MOTION;
                event
            },
            {
                let mut event = key_a(41);
                event.value0 += 1;
                event
            },
            {
                let mut event = key_a(41);
                event.value1 = 1;
                event
            },
            {
                let mut event = key_a(41);
                event.reserved0 = 1;
                event
            },
            {
                let mut event = key_a(41);
                event.reserved1 = 1;
                event
            },
        ];
        for event in mutations {
            assert!(InputSequenceValidator::new().accept(event).is_err());
        }

        let mut duplicate = InputSequenceValidator::new();
        assert_eq!(duplicate.accept(key_a(41)), Ok(1));
        assert_eq!(
            duplicate.accept(key_a(41)),
            Err(InputSequenceValidationError::Sequence)
        );

        let mut reversed = InputSequenceValidator::new();
        assert_eq!(
            reversed.accept(relative_motion(41)),
            Err(InputSequenceValidationError::Shape)
        );

        let mut extra = InputSequenceValidator::new();
        assert_eq!(extra.accept(key_a(41)), Ok(1));
        assert_eq!(extra.accept(relative_motion(42)), Ok(2));
        assert_eq!(
            extra.accept(key_a(43)),
            Err(InputSequenceValidationError::Complete)
        );
    }

    #[test]
    fn real_interpreter_reinvokes_with_fresh_tables_and_retained_state() {
        // Catches reinvocation that retains graph-local values/results or replaces the session identity.
        let bytes =
            test_support::command_read_result_emit_with_import_rights(RIGHTS_READ | RIGHTS_APPEND);
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let command_handle = PackedCapability::from_parts(9, 2);
        let mut imports = [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS];
        imports[0] = command_handle;
        let command_one = command(11, b"slice2-one");
        let command_two = command(12, b"slice2-two");
        let mut state = SessionRuntimeState::new(SESSION_SERVICE_ID);
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];

        let mut host_one =
            SessionCommandHost::new(command_handle, &command_one, b"slice2-one").unwrap();
        let exit_one = Interpreter::new(verified, &imports, 128, &mut values, &mut host_results)
            .execute(&mut host_one);
        assert_eq!(exit_one.status, GRAPH_EXIT_OK);
        assert_eq!(
            state.record_graph_exit(exit_one.status),
            SessionGraphLifecycleAction::Reinvoke
        );
        state.record_input_event();
        assert_eq!(state.input_event_count, 1);
        assert_eq!(state.graph_invocation_count, 1);
        assert_eq!(host_one.result().unwrap().bytes_written, 10);

        values.fill(Some(Value::U64(999)));
        host_results.fill(Some(HostCallResult::empty(99)));
        let mut host_two =
            SessionCommandHost::new(command_handle, &command_two, b"slice2-two").unwrap();
        let reset_proof = Interpreter::new(verified, &imports, 128, &mut values, &mut host_results);
        drop(reset_proof);
        assert_eq!(values, [None; MAX_RUNTIME_VALUES]);
        assert_eq!(host_results, [None; MAX_RUNTIME_VALUES]);
        let interpreter = Interpreter::new(verified, &imports, 128, &mut values, &mut host_results);
        let exit_two = interpreter.execute(&mut host_two);
        assert_eq!(exit_two.status, GRAPH_EXIT_OK);
        assert_eq!(
            state.record_graph_exit(exit_two.status),
            SessionGraphLifecycleAction::Reinvoke
        );
        state.record_input_event();

        assert_eq!(host_two.result().unwrap().object_id, 12);
        assert_eq!(host_two.result().unwrap().bytes_written, 10);
        assert_eq!(state.session_service_id, SESSION_SERVICE_ID);
        assert_eq!(state.input_event_count, 2);
        assert_eq!(state.graph_invocation_count, 2);
        assert_eq!(state.previous_graph_exit_status, Some(GRAPH_EXIT_OK));
        assert!(!state.recovery_requested);
    }

    #[test]
    fn failed_first_real_interpreter_invocation_requests_recovery_without_a_second_host() {
        // Catches a graph-fault reinvocation loop after budget exhaustion.
        let bytes = test_support::self_jump_budget_loop();
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let command_handle = PackedCapability::from_parts(9, 2);
        let imports = [command_handle; MAX_PYTH_GRAPH_IMPORTS];
        let command = command(11, b"slice2-one");
        let mut state = SessionRuntimeState::new(SESSION_SERVICE_ID);
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];
        let mut hosts_created = 0;
        hosts_created += 1;
        let mut first_host =
            SessionCommandHost::new(command_handle, &command, b"slice2-one").unwrap();

        let exit = Interpreter::new(verified, &imports, 1, &mut values, &mut host_results)
            .execute(&mut first_host);
        assert_eq!(exit.status, GRAPH_EXIT_BUDGET_EXHAUSTED);
        assert_eq!(
            state.record_graph_exit(exit.status),
            SessionGraphLifecycleAction::RequestRecovery
        );
        if !state.recovery_requested {
            hosts_created += 1;
        }
        assert!(state.recovery_requested);
        assert_eq!(state.graph_invocation_count, 1);
        assert_eq!(hosts_created, 1);
    }

    #[test]
    fn bootstrap_gate_rejects_null_misaligned_and_wrong_fixed_addresses() {
        // Catches dereferencing an untrusted entry pointer or beginning work with a wrong launch map.
        let (bootstrap, fixture) = accepted_launch();
        let mutations = [
            (0, bootstrap),
            (SESSION_RUNTIME_BOOTSTRAP_ADDRESS + 1, bootstrap),
            (SESSION_RUNTIME_BOOTSTRAP_ADDRESS + 8, bootstrap),
            (
                SESSION_RUNTIME_BOOTSTRAP_ADDRESS,
                SessionRuntimeBootstrapV1 {
                    fixture_ptr: SESSION_RUNTIME_FIXTURE_ADDRESS + 8,
                    ..bootstrap
                },
            ),
            (
                SESSION_RUNTIME_BOOTSTRAP_ADDRESS,
                SessionRuntimeBootstrapV1 {
                    result_ptr: SESSION_RUNTIME_RESULT_ADDRESS + 8,
                    ..bootstrap
                },
            ),
            (
                SESSION_RUNTIME_BOOTSTRAP_ADDRESS,
                SessionRuntimeBootstrapV1 {
                    graph: pythos_shared::pyth_runtime_abi::PythGraphBootstrapBlock {
                        package_ptr: SESSION_RUNTIME_PACKAGE_ADDRESS + 8,
                        ..bootstrap.graph
                    },
                    ..bootstrap
                },
            ),
        ];

        for (bootstrap_address, candidate) in mutations {
            assert_launch_rejected(bootstrap_address, &candidate, &fixture);
        }
    }

    #[test]
    fn bootstrap_gate_rejects_wrong_lengths_versions_identities_and_reserved_fields() {
        // Catches accepting a structurally plausible but unauthenticated outer launch record.
        let (bootstrap, fixture) = accepted_launch();
        let mut candidates = [bootstrap; 12];
        candidates[0].abi_major = SESSION_RUNTIME_ABI_MAJOR + 1;
        candidates[1].abi_minor = SESSION_RUNTIME_ABI_MINOR + 1;
        candidates[2].fixture_len -= 1;
        candidates[3].result_len -= 1;
        candidates[4].graph.package_len = 0;
        candidates[5].graph.package_len = SESSION_RUNTIME_PAGE_SIZE + 1;
        candidates[6].session_service_id += 1;
        candidates[7].runtime_principal_id += 1;
        candidates[8].graph_principal_id += 1;
        candidates[9].reserved0 = 1;
        candidates[10].reserved1[0] = 1;
        candidates[11].graph.result_ptr = SESSION_RUNTIME_RESULT_ADDRESS + 8;

        for candidate in candidates {
            assert_launch_rejected(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &candidate, &fixture);
        }
    }

    #[test]
    fn bootstrap_gate_rejects_malformed_graph_imports() {
        // Catches entering the interpreter with widened or fabricated graph command authority.
        let (bootstrap, fixture) = accepted_launch();
        let mut candidates = [bootstrap; 6];
        candidates[0].graph.import_count = 0;
        candidates[1].graph.imports[0].import_slot = 1;
        candidates[2].graph.imports[0].resource_kind += 1;
        candidates[3].graph.imports[0].rights ^= 1;
        candidates[4].graph.imports[0].reserved0 = 1;
        candidates[5].graph.imports[1].capability = PackedCapability::from_raw(1);

        for candidate in candidates {
            assert_launch_rejected(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &candidate, &fixture);
        }
    }

    #[test]
    fn bootstrap_gate_rejects_wrong_fixture_version_pointer_overlap_and_reserved_fields() {
        // Catches reading commands from a malformed, aliased, or non-v1 fixture mapping.
        let (bootstrap, fixture) = accepted_launch();
        let mut fixtures = [fixture; 5];
        fixtures[0].abi_major = SESSION_RUNTIME_ABI_MAJOR + 1;
        fixtures[1].abi_minor = SESSION_RUNTIME_ABI_MINOR + 1;
        fixtures[2].commands[0].payload_ptr += 1;
        fixtures[3].reserved0 = 1;
        fixtures[4].reserved1[0] = 1;
        for candidate in fixtures {
            assert_launch_rejected(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &bootstrap, &candidate);
        }

        let overlap = SessionRuntimeBootstrapV1 {
            result_ptr: SESSION_RUNTIME_FIXTURE_ADDRESS,
            ..bootstrap
        };
        assert_launch_rejected(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &overlap, &fixture);
    }

    #[test]
    fn bootstrap_gate_accepts_only_the_authenticated_fixed_fixture() {
        // Catches rejecting the kernel-authenticated launch after all boundary checks succeed.
        let (bootstrap, fixture) = accepted_launch();
        assert_eq!(
            validate_session_runtime_launch(
                SESSION_RUNTIME_BOOTSTRAP_ADDRESS,
                &bootstrap,
                &fixture,
            ),
            Ok(())
        );
    }

    #[test]
    fn package_gate_requires_exact_authenticated_bytes_before_verifier_assumption() {
        // Catches assuming kernel verification for a substituted, truncated, or malformed package.
        let (mut bootstrap, _fixture) = accepted_launch();
        let bytes =
            test_support::command_read_result_emit_with_import_rights(RIGHTS_READ | RIGHTS_APPEND);
        bootstrap.graph.package_len = bytes.len() as u64;
        bootstrap.graph_package_digest = pythos_shared::pyth_graph_manifest::digest64(&bytes);
        assert!(validate_session_runtime_package(&bootstrap, &bytes).is_ok());

        let mut substituted =
            test_support::command_read_result_emit_with_import_rights(RIGHTS_READ | RIGHTS_APPEND);
        substituted[0] ^= 1;
        assert_eq!(
            validate_session_runtime_package(&bootstrap, &substituted),
            Err(SessionRuntimeLaunchError::PackageDigestMismatch)
        );

        bootstrap.graph.package_len += 1;
        assert_eq!(
            validate_session_runtime_package(&bootstrap, &bytes),
            Err(SessionRuntimeLaunchError::PackageLengthMismatch)
        );

        let malformed = [0u8; 64];
        bootstrap.graph.package_len = malformed.len() as u64;
        bootstrap.graph_package_digest = pythos_shared::pyth_graph_manifest::digest64(&malformed);
        assert!(matches!(
            validate_session_runtime_package(&bootstrap, &malformed),
            Err(SessionRuntimeLaunchError::InvalidPackage(_))
        ));
    }

    #[test]
    fn orchestration_traps_once_without_effects_when_entry_or_recovery_boundary_is_untrusted() {
        // Catches using console/result effects or beginning work before their exact trust boundary.
        let (bootstrap, fixture) = accepted_launch();
        let cases = [
            (0, bootstrap, 0),
            (
                SESSION_RUNTIME_BOOTSTRAP_ADDRESS,
                SessionRuntimeBootstrapV1 {
                    console_capability: PackedCapability::from_raw(0),
                    ..bootstrap
                },
                1,
            ),
            (
                SESSION_RUNTIME_BOOTSTRAP_ADDRESS,
                SessionRuntimeBootstrapV1 {
                    result_ptr: SESSION_RUNTIME_RESULT_ADDRESS + 8,
                    ..bootstrap
                },
                1,
            ),
            (
                SESSION_RUNTIME_BOOTSTRAP_ADDRESS,
                SessionRuntimeBootstrapV1 {
                    result_len: bootstrap.result_len - 1,
                    ..bootstrap
                },
                1,
            ),
        ];

        for (bootstrap_address, candidate, expected_bootstrap_copies) in cases {
            let mut effects = RecordingEffects::new(candidate, fixture);
            run_session_runtime_orchestration(bootstrap_address, &mut effects);
            assert_eq!(effects.bootstrap_copies, expected_bootstrap_copies);
            assert_eq!(effects.fixture_copies, 0);
            assert_eq!(effects.package_prepares, 0);
            assert_eq!(effects.polls, 0);
            assert_eq!(effects.host_attempts, 0);
            assert_eq!(effects.graph_invocations, 0);
            assert_eq!(effects.terminal_writes, [None, None]);
            assert_eq!(effects.marker_count, 0);
            assert_eq!(effects.traps, 1);
        }
    }

    #[test]
    fn trusted_graph_fixture_and_package_rejections_recover_once_before_work() {
        // Catches treating post-trust graph, fixture, or package rejection as a no-write trap.
        let (bootstrap, fixture) = accepted_launch();
        let mut malformed_graph = bootstrap;
        malformed_graph.graph.imports[0].import_slot = 1;
        let bad_fixture_range = SessionRuntimeBootstrapV1 {
            fixture_ptr: SESSION_RUNTIME_FIXTURE_ADDRESS + 8,
            ..bootstrap
        };
        let bad_package_range = SessionRuntimeBootstrapV1 {
            graph: pythos_shared::pyth_runtime_abi::PythGraphBootstrapBlock {
                package_ptr: SESSION_RUNTIME_PACKAGE_ADDRESS + 8,
                ..bootstrap.graph
            },
            ..bootstrap
        };
        let mut malformed_fixture = fixture;
        malformed_fixture.magic ^= 1;

        for candidate in [malformed_graph, bad_fixture_range, bad_package_range] {
            let mut effects = RecordingEffects::new(candidate, fixture);
            run_session_runtime_orchestration(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &mut effects);
            assert_recovered_before_work(&effects);
            assert_eq!(effects.fixture_copies, 0);
            assert_eq!(effects.package_prepares, 0);
        }

        let mut fixture_effects = RecordingEffects::new(bootstrap, malformed_fixture);
        run_session_runtime_orchestration(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &mut fixture_effects);
        assert_recovered_before_work(&fixture_effects);
        assert_eq!(fixture_effects.fixture_copies, 1);
        assert_eq!(fixture_effects.package_prepares, 0);

        let mut package_effects = RecordingEffects::new(bootstrap, fixture);
        package_effects.reject_package = true;
        run_session_runtime_orchestration(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &mut package_effects);
        assert_recovered_before_work(&package_effects);
        assert_eq!(package_effects.fixture_copies, 1);
        assert_eq!(package_effects.package_prepares, 1);
    }

    #[test]
    fn orchestration_observes_host_failure_and_never_starts_a_graph_or_second_event() {
        // Catches polling or graph execution after the first command host rejects its fixture.
        let (bootstrap, fixture) = accepted_launch();
        let mut effects = RecordingEffects::new(bootstrap, fixture);
        effects.reject_host_at = Some(0);

        run_session_runtime_orchestration(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &mut effects);

        assert_eq!(effects.polls, 1);
        assert_eq!(effects.host_attempts, 1);
        assert_eq!(effects.graph_invocations, 0);
        assert_eq!(
            effects.terminal_writes,
            [Some(SessionRuntimeTerminalResult::RequestRecovery), None]
        );
        effects.assert_markers(&[
            "PYTHOS:SESSION_RUNTIME:BOOT_STATE_0\r\n",
            "PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_1\r\n",
            "PYTHOS:SESSION_RUNTIME:EVENT_1_KEY_A_SEQUENCE_0\r\n",
            "PYTHOS:SESSION_RUNTIME:COMMAND_1_SLICE2_ONE\r\n",
            "PYTHOS:SESSION_RUNTIME:ERROR\r\n",
        ]);
        assert_eq!(effects.traps, 1);
    }

    #[test]
    fn production_orchestration_emits_exact_success_contract_and_one_terminal_write_and_trap() {
        // Catches marker reordering, duplicate terminal effects, or skipped retained invocations.
        let (bootstrap, fixture) = accepted_launch();
        let mut effects = RecordingEffects::new(bootstrap, fixture);

        run_session_runtime_orchestration(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &mut effects);

        assert_eq!(effects.polls, 2);
        assert_eq!(effects.host_attempts, 2);
        assert_eq!(effects.graph_invocations, 2);
        assert_eq!(effects.resets, 1);
        assert_eq!(
            effects.terminal_writes,
            [Some(SessionRuntimeTerminalResult::Complete), None]
        );
        effects.assert_markers(&[
            "PYTHOS:SESSION_RUNTIME:BOOT_STATE_0\r\n",
            "PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_1\r\n",
            "PYTHOS:SESSION_RUNTIME:EVENT_1_KEY_A_SEQUENCE_0\r\n",
            "PYTHOS:SESSION_RUNTIME:COMMAND_1_SLICE2_ONE\r\n",
            "PYTHOS:SESSION_RUNTIME:RESULT_1_SLICE2_ONE\r\n",
            "PYTHOS:SESSION_RUNTIME:INVOCATION_1_EXIT_OK\r\n",
            "PYTHOS:SESSION_RUNTIME:STATE_INPUTS_1_INVOCATIONS_1\r\n",
            "PYTHOS:SESSION_RUNTIME:INVOCATION_LOCAL_RESET\r\n",
            "PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_2\r\n",
            "PYTHOS:SESSION_RUNTIME:EVENT_2_RELATIVE_MOTION_DX_7_DY_NEG_7_SEQUENCE_1\r\n",
            "PYTHOS:SESSION_RUNTIME:COMMAND_2_SLICE2_TWO\r\n",
            "PYTHOS:SESSION_RUNTIME:RESULT_2_SLICE2_TWO\r\n",
            "PYTHOS:SESSION_RUNTIME:INVOCATION_2_EXIT_OK\r\n",
            "PYTHOS:SESSION_RUNTIME:STATE_INPUTS_2_INVOCATIONS_2\r\n",
            "PYTHOS:SESSION_RUNTIME:INPUT_CONTIGUOUS\r\n",
            "PYTHOS:SESSION_RUNTIME:SESSION_ID_STABLE\r\n",
            "PYTHOS:SESSION_RUNTIME:READY\r\n",
        ]);
        assert_eq!(effects.traps, 1);
    }

    struct RecordingEffects {
        bootstrap: SessionRuntimeBootstrapV1,
        fixture: SessionRuntimeFixtureV1,
        reject_package: bool,
        reject_host_at: Option<usize>,
        bootstrap_copies: usize,
        fixture_copies: usize,
        package_prepares: usize,
        polls: usize,
        host_attempts: usize,
        graph_invocations: usize,
        resets: usize,
        terminal_writes: [Option<SessionRuntimeTerminalResult>; 2],
        markers: [Option<&'static str>; 20],
        marker_count: usize,
        traps: usize,
    }

    impl RecordingEffects {
        fn new(bootstrap: SessionRuntimeBootstrapV1, fixture: SessionRuntimeFixtureV1) -> Self {
            Self {
                bootstrap,
                fixture,
                reject_package: false,
                reject_host_at: None,
                bootstrap_copies: 0,
                fixture_copies: 0,
                package_prepares: 0,
                polls: 0,
                host_attempts: 0,
                graph_invocations: 0,
                resets: 0,
                terminal_writes: [None, None],
                markers: [None; 20],
                marker_count: 0,
                traps: 0,
            }
        }

        fn assert_markers(&self, expected: &[&'static str]) {
            assert_eq!(self.marker_count, expected.len());
            for (actual, expected) in self.markers.iter().zip(expected) {
                assert_eq!(*actual, Some(*expected));
            }
        }
    }

    impl SessionRuntimeEffects for RecordingEffects {
        fn copy_bootstrap(&mut self) -> SessionRuntimeBootstrapV1 {
            self.bootstrap_copies += 1;
            self.bootstrap
        }

        fn copy_fixture(&mut self) -> SessionRuntimeFixtureV1 {
            self.fixture_copies += 1;
            self.fixture
        }

        fn prepare_graph_package(
            &mut self,
            _bootstrap: &SessionRuntimeBootstrapV1,
        ) -> Result<(), SessionRuntimeEffectError> {
            self.package_prepares += 1;
            if self.reject_package {
                Err(SessionRuntimeEffectError::Package)
            } else {
                Ok(())
            }
        }

        fn poll_input(&mut self, _ordinal: usize) -> Result<(), SessionRuntimeEffectError> {
            self.polls += 1;
            Ok(())
        }

        fn run_command_host_and_graph(
            &mut self,
            ordinal: usize,
        ) -> Result<(), SessionRuntimeEffectError> {
            self.host_attempts += 1;
            if self.reject_host_at == Some(ordinal) {
                return Err(SessionRuntimeEffectError::CommandHost);
            }
            self.graph_invocations += 1;
            Ok(())
        }

        fn reset_invocation_local(&mut self) -> Result<(), SessionRuntimeEffectError> {
            self.resets += 1;
            Ok(())
        }

        fn final_state_is_valid(&self) -> bool {
            true
        }

        fn write_terminal_result(
            &mut self,
            terminal: SessionRuntimeTerminalResult,
        ) -> Result<(), SessionRuntimeEffectError> {
            let Some(slot) = self.terminal_writes.iter_mut().find(|slot| slot.is_none()) else {
                return Err(SessionRuntimeEffectError::Result);
            };
            *slot = Some(terminal);
            Ok(())
        }

        fn emit_marker(&mut self, marker: &'static str) {
            self.markers[self.marker_count] = Some(marker);
            self.marker_count += 1;
        }

        fn trap(&mut self) {
            self.traps += 1;
        }
    }

    fn assert_recovered_before_work(effects: &RecordingEffects) {
        assert_eq!(effects.polls, 0);
        assert_eq!(effects.host_attempts, 0);
        assert_eq!(effects.graph_invocations, 0);
        assert_eq!(
            effects.terminal_writes,
            [Some(SessionRuntimeTerminalResult::RequestRecovery), None]
        );
        effects.assert_markers(&["PYTHOS:SESSION_RUNTIME:ERROR\r\n"]);
        assert_eq!(effects.traps, 1);
    }

    fn assert_launch_rejected(
        bootstrap_address: u64,
        bootstrap: &SessionRuntimeBootstrapV1,
        fixture: &SessionRuntimeFixtureV1,
    ) {
        assert!(validate_session_runtime_launch(bootstrap_address, bootstrap, fixture).is_err());
    }

    fn accepted_launch() -> (SessionRuntimeBootstrapV1, SessionRuntimeFixtureV1) {
        let mut bootstrap = SessionRuntimeBootstrapV1::empty();
        bootstrap.magic = SESSION_RUNTIME_BOOTSTRAP_MAGIC;
        bootstrap.abi_major = SESSION_RUNTIME_ABI_MAJOR;
        bootstrap.abi_minor = SESSION_RUNTIME_ABI_MINOR;
        bootstrap.command_count = SESSION_RUNTIME_COMMAND_COUNT as u16;
        bootstrap.session_service_id = SESSION_SERVICE_ID;
        bootstrap.runtime_principal_id = RUNTIME_PRINCIPAL_ID;
        bootstrap.graph_principal_id = GRAPH_PRINCIPAL_ID;
        bootstrap.graph_package_digest = 1;
        bootstrap.input_capability = PackedCapability::from_raw(1);
        bootstrap.console_capability = PackedCapability::from_raw(2);
        bootstrap.fixture_ptr = SESSION_RUNTIME_FIXTURE_ADDRESS;
        bootstrap.fixture_len = core::mem::size_of::<SessionRuntimeFixtureV1>() as u64;
        bootstrap.result_ptr = SESSION_RUNTIME_RESULT_ADDRESS;
        bootstrap.result_len = core::mem::size_of::<
            pythos_shared::session_runtime_abi::SessionRuntimeResultV1,
        >() as u64;
        bootstrap.graph.magic = PYTH_GRAPH_BOOTSTRAP_MAGIC;
        bootstrap.graph.abi_major = PYTH_GRAPH_RUNTIME_ABI_MAJOR;
        bootstrap.graph.abi_minor = PYTH_GRAPH_RUNTIME_ABI_MINOR;
        bootstrap.graph.import_count = 1;
        bootstrap.graph.package_ptr = SESSION_RUNTIME_PACKAGE_ADDRESS;
        bootstrap.graph.package_len = 512;
        bootstrap.graph.instruction_budget = 128;
        bootstrap.graph.result_ptr = SESSION_RUNTIME_RESULT_ADDRESS;
        bootstrap.graph.imports[0] = PythGraphCapabilityBinding {
            import_slot: 0,
            resource_kind: 6,
            reserved0: 0,
            rights: RIGHTS_READ | RIGHTS_APPEND,
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
            fixture.commands[ordinal] = command((ordinal + 11) as u64, &fixture.payloads[ordinal]);
            fixture.commands[ordinal].payload_len = 10;
            fixture.commands[ordinal].payload_ptr = SESSION_RUNTIME_FIXTURE_ADDRESS
                + core::mem::offset_of!(SessionRuntimeFixtureV1, payloads) as u64
                + (ordinal * SESSION_RUNTIME_MAX_COMMAND_PAYLOAD) as u64;
        }
        (bootstrap, fixture)
    }

    fn command(object_id: u64, payload: &[u8]) -> PythCommand {
        PythCommand {
            object_id,
            payload_len: payload.len() as u64,
            ..PythCommand::empty(COMMAND_KIND_CREATE_NOTE)
        }
    }
}
