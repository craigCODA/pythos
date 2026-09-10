//! Live SYSTEM_STATUS graph adapter for the retained normal session.

use pythos_shared::{
    capability_abi::PackedCapability,
    normal_session_abi::NormalSessionReturnReason,
    pyth_command_abi::{
        COMMAND_KIND_SYSTEM_STATUS, COMMAND_RESULT_STATUS_OK, PythCommand, PythCommandResult,
    },
    pyth_runtime_abi::{
        GRAPH_RESULT_UNIT, GraphExitRecord, HostCallResult, MAX_PYTH_GRAPH_IMPORTS,
    },
    pyth_tig::{NO_VALUE, format::MAX_RUNTIME_VALUES, verify::VerifiedGraph},
    session_runtime_lifecycle::{SessionGraphLifecycleAction, session_graph_lifecycle_action},
};
use pythos_user_pyth_runtime::{interpreter::Interpreter, value::Value};

use crate::session_command_host::SessionCommandHost;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalGraphInvocation {
    pub result: PythCommandResult,
    pub exit: GraphExitRecord,
}

pub trait NormalGraphExitSink {
    fn write_exit(&mut self, exit: GraphExitRecord);
}

pub struct NormalGraphRunner<'package, 'storage> {
    verified: VerifiedGraph<'package>,
    command_capability: PackedCapability,
    instruction_budget: u64,
    imports: &'storage [PackedCapability; MAX_PYTH_GRAPH_IMPORTS],
    values: &'storage mut [Option<Value>; MAX_RUNTIME_VALUES],
    host_results: &'storage mut [Option<HostCallResult>; MAX_RUNTIME_VALUES],
}

impl<'package, 'storage> NormalGraphRunner<'package, 'storage> {
    pub fn new(
        verified: VerifiedGraph<'package>,
        command_capability: PackedCapability,
        instruction_budget: u64,
        imports: &'storage [PackedCapability; MAX_PYTH_GRAPH_IMPORTS],
        values: &'storage mut [Option<Value>; MAX_RUNTIME_VALUES],
        host_results: &'storage mut [Option<HostCallResult>; MAX_RUNTIME_VALUES],
    ) -> Self {
        Self {
            verified,
            command_capability,
            instruction_budget,
            imports,
            values,
            host_results,
        }
    }

    pub fn run_status(
        &mut self,
        payload: &[u8],
        exit_sink: &mut impl NormalGraphExitSink,
    ) -> Result<NormalGraphInvocation, NormalSessionReturnReason> {
        let mut command = PythCommand::empty(COMMAND_KIND_SYSTEM_STATUS);
        command.payload_len = payload.len() as u64;
        let mut host = SessionCommandHost::new(self.command_capability, &command, payload)
            .map_err(|_| NormalSessionReturnReason::Graph)?;
        let exit = Interpreter::new(
            self.verified,
            self.imports,
            self.instruction_budget,
            self.values,
            self.host_results,
        )
        .execute(&mut host);
        exit_sink.write_exit(exit);
        let result = host.result().ok_or(NormalSessionReturnReason::Graph)?;
        if !invocation_is_valid(&command, payload, result, exit) {
            return Err(NormalSessionReturnReason::Graph);
        }
        Ok(NormalGraphInvocation { result, exit })
    }
}

fn invocation_is_valid(
    command: &PythCommand,
    payload: &[u8],
    result: PythCommandResult,
    exit: GraphExitRecord,
) -> bool {
    session_graph_lifecycle_action(exit.status) == SessionGraphLifecycleAction::Reinvoke
        && exit.error_code == 0
        && exit.last_node != NO_VALUE
        && exit.executed_nodes != 0
        && exit.result_type == GRAPH_RESULT_UNIT
        && exit.reserved0 == 0
        && exit.reserved1 == 0
        && exit.result_raw == 0
        && result.status == COMMAND_RESULT_STATUS_OK
        && result.kind == COMMAND_KIND_SYSTEM_STATUS
        && result.kind == command.kind
        && result.reserved0 == 0
        && result.object_id == 0
        && result.object_id == command.object_id
        && result.task_id == 0
        && result.task_id == command.task_id
        && result.proposal_id == 0
        && result.proposal_id == command.proposal_id
        && result.bytes_written == payload.len() as u64
        && result.reserved1 == 0
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use pythc::{encode::encode_verified_graph, lower::lower_program, typecheck::typecheck_source};
    use pythos_shared::{
        capability_abi::PackedCapability,
        normal_session_abi::NORMAL_SESSION_GRAPH_INSTRUCTION_BUDGET,
        pyth_command_abi::{
            COMMAND_KIND_SYSTEM_STATUS, COMMAND_RESULT_STATUS_OK, PythCommandResult,
        },
        pyth_runtime_abi::{
            GRAPH_EXIT_OK, GRAPH_RESULT_UNIT, GraphExitRecord, HostCallResult,
            MAX_PYTH_GRAPH_IMPORTS,
        },
        pyth_tig::{
            format::{MAX_RUNTIME_VALUES, PythGraphPackage},
            verify::verify_package,
        },
    };
    use pythos_user_pyth_runtime::value::Value;

    const COMMAND_CAPABILITY: PackedCapability = PackedCapability::from_parts(9, 2);
    const FIRST_PAYLOAD: &[u8] = b"e0000000000000000c0000000000000001r0000000000000000a0x000y000";
    const SECOND_PAYLOAD: &[u8] = b"e0000000000000008c0000000000000002r0000000000000008a1x140y0f0";

    #[derive(Default)]
    struct RecordedExitSink {
        records: std::vec::Vec<GraphExitRecord>,
    }

    impl NormalGraphExitSink for RecordedExitSink {
        fn write_exit(&mut self, exit: GraphExitRecord) {
            self.records.push(exit);
        }
    }

    #[test]
    fn runner_returns_exact_live_status_and_resets_dirty_invocation_tables() {
        // Catches fixture success, fabricated IDs, or graph-local values leaking across commands.
        let bytes = compile_source(include_str!(
            "../../../programs/normal-session-manager/main.pyth"
        ));
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut imports = [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS];
        imports[0] = COMMAND_CAPABILITY;
        let mut values = [Some(Value::U64(u64::MAX)); MAX_RUNTIME_VALUES];
        let mut host_results = [Some(HostCallResult::empty(u16::MAX)); MAX_RUNTIME_VALUES];
        let mut runner = NormalGraphRunner::new(
            verified,
            COMMAND_CAPABILITY,
            NORMAL_SESSION_GRAPH_INSTRUCTION_BUDGET,
            &imports,
            &mut values,
            &mut host_results,
        );
        let mut exits = RecordedExitSink::default();

        let first = runner.run_status(FIRST_PAYLOAD, &mut exits).unwrap();
        let second = runner.run_status(SECOND_PAYLOAD, &mut exits).unwrap();

        assert_eq!(
            first.result,
            PythCommandResult {
                status: COMMAND_RESULT_STATUS_OK,
                kind: COMMAND_KIND_SYSTEM_STATUS,
                reserved0: 0,
                object_id: 0,
                task_id: 0,
                proposal_id: 0,
                bytes_written: FIRST_PAYLOAD.len() as u64,
                reserved1: 0,
            }
        );
        assert_eq!(first.exit.status, GRAPH_EXIT_OK);
        assert_eq!(first.exit.error_code, 0);
        assert_eq!(first.exit.result_type, GRAPH_RESULT_UNIT);
        assert_eq!(first.exit.reserved0, 0);
        assert_eq!(first.exit.reserved1, 0);
        assert_eq!(first.exit.result_raw, 0);
        assert_eq!(second.result.bytes_written, SECOND_PAYLOAD.len() as u64);
        assert_eq!(second.exit, first.exit);
        assert_eq!(exits.records, [first.exit, second.exit]);
    }

    #[test]
    fn runner_requests_recovery_for_missing_result_budget_and_runtime_error() {
        // Catches reinvoking after an admitted graph does not prove the exact status exchange.
        let old_bytes = compile_source(include_str!("../../../programs/session-manager/main.pyth"));
        let old_package = PythGraphPackage::decode(&old_bytes).unwrap();
        let old_verified = verify_package(&old_package).unwrap();
        let mut imports = [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS];
        imports[0] = COMMAND_CAPABILITY;
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];
        let mut old_runner = NormalGraphRunner::new(
            old_verified,
            COMMAND_CAPABILITY,
            NORMAL_SESSION_GRAPH_INSTRUCTION_BUDGET,
            &imports,
            &mut values,
            &mut host_results,
        );
        let mut old_exits = RecordedExitSink::default();
        assert_eq!(
            old_runner.run_status(FIRST_PAYLOAD, &mut old_exits),
            Err(pythos_shared::normal_session_abi::NormalSessionReturnReason::Graph)
        );
        assert_eq!(old_exits.records.len(), 1);
        assert_eq!(old_exits.records[0].status, GRAPH_EXIT_OK);

        let bytes = compile_source(include_str!(
            "../../../programs/normal-session-manager/main.pyth"
        ));
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut budget_runner = NormalGraphRunner::new(
            verified,
            COMMAND_CAPABILITY,
            1,
            &imports,
            &mut values,
            &mut host_results,
        );
        let mut budget_exits = RecordedExitSink::default();
        assert_eq!(
            budget_runner.run_status(FIRST_PAYLOAD, &mut budget_exits),
            Err(pythos_shared::normal_session_abi::NormalSessionReturnReason::Graph)
        );
        assert_eq!(budget_exits.records.len(), 1);
        assert_eq!(
            budget_exits.records[0].status,
            pythos_shared::pyth_runtime_abi::GRAPH_EXIT_BUDGET_EXHAUSTED
        );

        imports[0] = PackedCapability::from_raw(0);
        let mut runtime_runner = NormalGraphRunner::new(
            verified,
            COMMAND_CAPABILITY,
            NORMAL_SESSION_GRAPH_INSTRUCTION_BUDGET,
            &imports,
            &mut values,
            &mut host_results,
        );
        let mut runtime_exits = RecordedExitSink::default();
        assert_eq!(
            runtime_runner.run_status(FIRST_PAYLOAD, &mut runtime_exits),
            Err(pythos_shared::normal_session_abi::NormalSessionReturnReason::Graph)
        );
        assert_eq!(runtime_exits.records.len(), 1);
        assert_eq!(
            runtime_exits.records[0].status,
            pythos_shared::pyth_runtime_abi::GRAPH_EXIT_RUNTIME_ERROR
        );

        let mut invalid_runner = NormalGraphRunner::new(
            verified,
            COMMAND_CAPABILITY,
            NORMAL_SESSION_GRAPH_INSTRUCTION_BUDGET,
            &imports,
            &mut values,
            &mut host_results,
        );
        let mut no_exit = RecordedExitSink::default();
        assert_eq!(
            invalid_runner.run_status(&[0xff], &mut no_exit),
            Err(pythos_shared::normal_session_abi::NormalSessionReturnReason::Graph)
        );
        assert!(no_exit.records.is_empty());
    }

    #[test]
    fn unknown_graph_exit_is_not_a_valid_status_invocation() {
        // Catches treating an unrecognized graph lifecycle status as reinvokable success.
        let command =
            pythos_shared::pyth_command_abi::PythCommand::empty(COMMAND_KIND_SYSTEM_STATUS);
        let result = PythCommandResult::empty(COMMAND_RESULT_STATUS_OK, COMMAND_KIND_SYSTEM_STATUS);
        let exit = GraphExitRecord {
            status: u16::MAX,
            error_code: 0,
            last_node: 0,
            executed_nodes: 0,
            result_type: GRAPH_RESULT_UNIT,
            reserved0: 0,
            reserved1: 0,
            result_raw: 0,
        };
        assert!(!invocation_is_valid(&command, b"", result, exit));
    }

    fn compile_source(source: &str) -> std::vec::Vec<u8> {
        let typed = typecheck_source(source).unwrap();
        let graph = lower_program(&typed).unwrap();
        let bytes = encode_verified_graph(&graph).unwrap();
        let package = PythGraphPackage::decode(&bytes).unwrap();
        verify_package(&package).unwrap();
        bytes
    }
}
