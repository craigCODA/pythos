#![no_std]

pub mod session_command_host;

use pythos_shared::session_input_abi::{
    KEY_A, SESSION_INPUT_KIND_KEY_DOWN, SESSION_INPUT_KIND_RELATIVE_MOTION,
    SESSION_INPUT_SOURCE_KEYBOARD, SESSION_INPUT_SOURCE_MOUSE, SessionInputEventV1,
};

pub use pythos_shared::session_runtime_lifecycle::SessionGraphLifecycleAction;

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
        },
        pyth_tig::{
            format::{MAX_RUNTIME_VALUES, PythGraphPackage},
            opcode::{RIGHTS_APPEND, RIGHTS_READ},
            test_support,
            verify::verify_package,
        },
    };
    use pythos_user_pyth_runtime::{interpreter::Interpreter, value::Value};

    use crate::session_command_host::SessionCommandHost;

    const SESSION_SERVICE_ID: u64 = 0x5059_5345_5353_0001;

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

    fn command(object_id: u64, payload: &[u8]) -> PythCommand {
        PythCommand {
            object_id,
            payload_len: payload.len() as u64,
            ..PythCommand::empty(COMMAND_KIND_CREATE_NOTE)
        }
    }
}
