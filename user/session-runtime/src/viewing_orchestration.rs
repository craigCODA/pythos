//! Bounded opt-in Viewing coordinator; graph admission and execution remain shared.

use crate::{
    SessionRuntimeEffectError, SessionRuntimeEffects, session_viewing::SessionViewing,
    validate_session_runtime_bootstrap_address, validate_session_runtime_fixture_contract,
    validate_session_runtime_outer_bootstrap, validate_session_runtime_recovery_boundary,
};
use pythos_shared::{
    session_input_abi::*,
    session_runtime_abi::SessionRuntimeBootstrapV1,
    session_viewing_result::{
        SESSION_VIEWING_RESULT_COMPLETE, SESSION_VIEWING_RESULT_RECOVERY, SessionViewingResultV1,
        ViewingSnapshotRecord, validate_session_viewing_result,
    },
    viewing::{MotionRoute, ViewingExtent, ViewingSnapshot},
};

pub trait ViewingRuntimeEffects: SessionRuntimeEffects {
    fn prepare_viewing(
        &mut self,
        bootstrap: &SessionRuntimeBootstrapV1,
    ) -> Result<ViewingExtent, SessionRuntimeEffectError>;
    fn next_viewing_event(&mut self) -> Result<SessionInputEventV1, SessionRuntimeEffectError>;
    fn present_viewing(
        &mut self,
        revision: u64,
        snapshot: ViewingSnapshot,
    ) -> Result<(), SessionRuntimeEffectError>;
    fn write_viewing_result(
        &mut self,
        result: &SessionViewingResultV1,
    ) -> Result<(), SessionRuntimeEffectError>;
}

/// Both independently versioned extensions occupy the second half of their
/// existing fixed pages. No pointer is dereferenced until this check succeeds.
pub fn viewing_extension_address(
    base: u64,
    offset: u64,
    length: u64,
) -> Result<u64, SessionRuntimeEffectError> {
    if !matches!(
        base,
        crate::SESSION_RUNTIME_BOOTSTRAP_ADDRESS | crate::SESSION_RUNTIME_RESULT_ADDRESS
    ) || offset != 2048
        || length == 0
        || length > 2048
    {
        return Err(SessionRuntimeEffectError::Package);
    }
    let address = base
        .checked_add(offset)
        .ok_or(SessionRuntimeEffectError::Package)?;
    let end = address
        .checked_add(length)
        .ok_or(SessionRuntimeEffectError::Package)?;
    let page_end = base
        .checked_add(crate::SESSION_RUNTIME_PAGE_SIZE)
        .ok_or(SessionRuntimeEffectError::Package)?;
    if end > page_end || !address.is_multiple_of(8) {
        return Err(SessionRuntimeEffectError::Package);
    }
    Ok(address)
}

pub fn run_viewing_orchestration<E: ViewingRuntimeEffects>(address: u64, effects: &mut E) {
    if validate_session_runtime_bootstrap_address(address).is_err() {
        effects.trap();
        return;
    }
    let bootstrap = effects.copy_bootstrap();
    if validate_session_runtime_recovery_boundary(&bootstrap).is_err() {
        effects.trap();
        return;
    }
    let mut result = SessionViewingResultV1::new(
        bootstrap.session_service_id,
        bootstrap.runtime_principal_id,
        bootstrap.graph_principal_id,
    );
    if run_viewing(&bootstrap, effects, &mut result).is_err() {
        result.terminal_status = SESSION_VIEWING_RESULT_RECOVERY;
        let _ = effects.write_viewing_result(&result);
        effects.emit_marker("PYTHOS:USER:SESSION_VIEWING:RECOVERY_REQUIRED\r\n");
    } else {
        effects.emit_marker("PYTHOS:USER:SESSION_VIEWING:COMPLETE\r\n");
    }
    effects.trap();
}

fn run_viewing<E: ViewingRuntimeEffects>(
    bootstrap: &SessionRuntimeBootstrapV1,
    effects: &mut E,
    result: &mut SessionViewingResultV1,
) -> Result<(), SessionRuntimeEffectError> {
    validate_session_runtime_outer_bootstrap(bootstrap)
        .map_err(|_| SessionRuntimeEffectError::Package)?;
    let fixture = effects.copy_fixture();
    validate_session_runtime_fixture_contract(bootstrap, &fixture)
        .map_err(|_| SessionRuntimeEffectError::Package)?;
    effects.prepare_graph_package(bootstrap)?;
    let extent = effects.prepare_viewing(bootstrap)?;
    let mut viewing = SessionViewing::new(extent);
    effects.emit_marker("PYTHOS:USER:SESSION_VIEWING:BOOT_STATE_0\r\n");
    present(effects, result, 0, viewing.snapshot())?;
    for index in 0..7 {
        effects.emit_marker(WAIT_MARKERS[index]);
        let event = effects.next_viewing_event()?;
        let receipt = viewing
            .observe(event)
            .map_err(|_| SessionRuntimeEffectError::Poll)?;
        if !acceptance_event(index, event) {
            return Err(SessionRuntimeEffectError::Poll);
        }
        result.input_event_count += 1;
        if receipt.command.is_some() {
            result.activation_count += 1;
            effects.emit_marker("PYTHOS:USER:SESSION_VIEWING:ACTIVATED\r\n");
        }
        match receipt.motion_route {
            Some(MotionRoute::Traversal(_)) => {
                effects.emit_marker("PYTHOS:USER:SESSION_VIEWING:MOTION:TRAVERSAL\r\n")
            }
            Some(MotionRoute::CursorFocus(_)) => {
                effects.emit_marker("PYTHOS:USER:SESSION_VIEWING:MOTION:FOCUS_MARK\r\n")
            }
            None => (),
        }
        result.traversal_count = u64::from(viewing.traversal_intent_count());
        present(effects, result, index + 1, receipt.snapshot)?;
        if index == 2 || index == 5 {
            let ordinal = usize::from(index == 5);
            effects.run_command_host_and_graph(ordinal)?;
            result.invocation_count += 1;
            result.graph_checkpoint_events[ordinal] = result.input_event_count;
            if ordinal == 0 {
                effects.reset_invocation_local()?;
                effects.emit_marker("PYTHOS:USER:SESSION_VIEWING:INVOCATION:1:VALID\r\n");
            } else {
                effects.emit_marker("PYTHOS:USER:SESSION_VIEWING:INVOCATION:2:VALID\r\n");
            }
        }
    }
    result.terminal_status = SESSION_VIEWING_RESULT_COMPLETE;
    validate_session_viewing_result(
        result,
        [
            bootstrap.session_service_id,
            bootstrap.runtime_principal_id,
            bootstrap.graph_principal_id,
        ],
    )
    .map_err(|_| SessionRuntimeEffectError::Result)?;
    effects.write_viewing_result(result)
}

fn present<E: ViewingRuntimeEffects>(
    effects: &mut E,
    result: &mut SessionViewingResultV1,
    revision: usize,
    snapshot: ViewingSnapshot,
) -> Result<(), SessionRuntimeEffectError> {
    let (flags, x, y) = match snapshot.focus_mark {
        None => (0, 0, 0),
        Some(position) => (
            1,
            u16::try_from(position.x).map_err(|_| SessionRuntimeEffectError::Result)?,
            u16::try_from(position.y).map_err(|_| SessionRuntimeEffectError::Result)?,
        ),
    };
    effects.present_viewing(revision as u64, snapshot)?;
    result.snapshots[revision] = ViewingSnapshotRecord {
        revision: revision as u64,
        flags,
        x,
        y,
    };
    effects.emit_marker(DRAW_MARKERS[revision]);
    Ok(())
}

fn acceptance_event(index: usize, event: SessionInputEventV1) -> bool {
    if event.sequence != index as u64 || event.flags != 0 {
        return false;
    }
    if index == 0 || index == 5 {
        event.kind == SESSION_INPUT_KIND_RELATIVE_MOTION
            && event.source == SESSION_INPUT_SOURCE_MOUSE
            && event.value0 == 7
            && event.value1 == -7
    } else {
        let key = match index {
            1 | 2 => KEY_SPACE,
            3 | 4 => KEY_BACKSPACE,
            6 => KEY_ENTER,
            _ => return false,
        };
        event.kind == SESSION_INPUT_KIND_KEY_DOWN
            && event.source == SESSION_INPUT_SOURCE_KEYBOARD
            && event.value0 == i32::from(key)
            && event.value1 == 0
    }
}

const WAIT_MARKERS: [&str; 7] = [
    "PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:0\r\n",
    "PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:1\r\n",
    "PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:2\r\n",
    "PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:3\r\n",
    "PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:4\r\n",
    "PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:5\r\n",
    "PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:6\r\n",
];
const DRAW_MARKERS: [&str; 8] = [
    "PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:0\r\n",
    "PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:1\r\n",
    "PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:2\r\n",
    "PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:3\r\n",
    "PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:4\r\n",
    "PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:5\r\n",
    "PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:6\r\n",
    "PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:7\r\n",
];

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::*;
    use pythos_shared::viewing::FocusMarkPosition;
    use std::vec::Vec;

    #[test]
    fn extension_range_rejects_overflow_alignment_wrong_page_and_page_crossing() {
        assert_eq!(
            viewing_extension_address(0x7200_0000, 2048, 64),
            Ok(0x7200_0800)
        );
        assert_eq!(
            viewing_extension_address(0x7200_3000, 2048, 432),
            Ok(0x7200_3800)
        );
        for (base, offset, len) in [
            (0, 2048, 64),
            (0x7200_1000, 2048, 64),
            (0x7200_0000, 2049, 64),
            (0x7200_0000, 2048, 2049),
            (0x7200_0000, u64::MAX, 64),
            (0x7200_0000, 2048, 0),
        ] {
            assert!(viewing_extension_address(base, offset, len).is_err());
        }
    }

    #[test]
    fn seven_events_keep_activation_recognition_across_a_real_fresh_graph_reset() {
        let mut effects = Effects::new();
        run_viewing_orchestration(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &mut effects);
        let result = effects.result.unwrap();
        assert_eq!(result.terminal_status, 1);
        assert_eq!(result.graph_checkpoint_events, [3, 6]);
        assert_eq!(result.input_event_count, 7);
        assert_eq!(result.traversal_count, 1);
        assert_eq!(result.activation_count, 1);
        assert_eq!(effects.draws.len(), 8);
        assert_eq!(effects.draws[4].focus_mark, None);
        assert_eq!(
            effects.draws[5].focus_mark,
            Some(FocusMarkPosition { x: 320, y: 240 })
        );
        assert_eq!(
            effects.draws[6].focus_mark,
            Some(FocusMarkPosition { x: 327, y: 233 })
        );
        assert_eq!(effects.graph_inputs, [3, 6]);
        assert_eq!(effects.resets, 1);
        assert_eq!(effects.traps, 1);
        assert_eq!(effects.results[0].unwrap().object_id, 11);
        assert_eq!(effects.results[1].unwrap().object_id, 12);
    }

    #[test]
    fn unknown_transport_graph_and_presenter_errors_recover_once_without_more_work() {
        for failure in [
            Failure::Bootstrap,
            Failure::Input,
            Failure::Graph,
            Failure::Draw,
        ] {
            let mut effects = Effects::new();
            effects.failure = Some(failure);
            run_viewing_orchestration(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &mut effects);
            assert_eq!(effects.result.unwrap().terminal_status, 2);
            assert_eq!(effects.traps, 1);
            assert!(
                !effects
                    .markers
                    .iter()
                    .any(|marker| marker.contains(":COMPLETE"))
            );
            assert!(effects.graph_inputs.len() <= 1);
        }
    }

    #[test]
    fn bad_address_is_never_read_and_unflagged_loss_never_finishes_activation() {
        let mut effects = Effects::new();
        run_viewing_orchestration(0, &mut effects);
        assert_eq!(effects.copies, 0);
        assert_eq!(effects.traps, 1);
        assert!(effects.result.is_none());
        let mut effects = Effects::new();
        effects.events[4].sequence = 8;
        run_viewing_orchestration(SESSION_RUNTIME_BOOTSTRAP_ADDRESS, &mut effects);
        assert_eq!(effects.result.unwrap().terminal_status, 2);
        assert_eq!(effects.draws.len(), 5);
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Failure {
        Bootstrap,
        Input,
        Graph,
        Draw,
    }
    struct Effects {
        bootstrap: pythos_shared::session_runtime_abi::SessionRuntimeBootstrapV1,
        fixture: pythos_shared::session_runtime_abi::SessionRuntimeFixtureV1,
        events: [SessionInputEventV1; 7],
        event_index: usize,
        draws: Vec<ViewingSnapshot>,
        graph_inputs: Vec<usize>,
        results: [Option<pythos_shared::pyth_command_abi::PythCommandResult>; 2],
        values: [Option<pythos_user_pyth_runtime::value::Value>;
            pythos_shared::pyth_tig::format::MAX_RUNTIME_VALUES],
        host_results: [Option<pythos_shared::pyth_runtime_abi::HostCallResult>;
            pythos_shared::pyth_tig::format::MAX_RUNTIME_VALUES],
        result: Option<SessionViewingResultV1>,
        failure: Option<Failure>,
        markers: Vec<&'static str>,
        copies: usize,
        resets: usize,
        traps: usize,
    }
    impl Effects {
        fn new() -> Self {
            let (bootstrap, fixture) = crate::tests::accepted_launch();
            let mut events = [SessionInputEventV1::empty(); 7];
            for (index, key) in [
                0,
                KEY_SPACE,
                KEY_SPACE,
                KEY_BACKSPACE,
                KEY_BACKSPACE,
                0,
                KEY_ENTER,
            ]
            .into_iter()
            .enumerate()
            {
                events[index] = SessionInputEventV1 {
                    sequence: index as u64,
                    kind: if key == 0 {
                        SESSION_INPUT_KIND_RELATIVE_MOTION
                    } else {
                        SESSION_INPUT_KIND_KEY_DOWN
                    },
                    source: if key == 0 {
                        SESSION_INPUT_SOURCE_MOUSE
                    } else {
                        SESSION_INPUT_SOURCE_KEYBOARD
                    },
                    value0: if key == 0 { 7 } else { i32::from(key) },
                    value1: if key == 0 { -7 } else { 0 },
                    ..SessionInputEventV1::empty()
                };
            }
            Self {
                bootstrap,
                fixture,
                events,
                event_index: 0,
                draws: Vec::new(),
                graph_inputs: Vec::new(),
                results: [None; 2],
                values: [None; pythos_shared::pyth_tig::format::MAX_RUNTIME_VALUES],
                host_results: [None; pythos_shared::pyth_tig::format::MAX_RUNTIME_VALUES],
                result: None,
                failure: None,
                markers: Vec::new(),
                copies: 0,
                resets: 0,
                traps: 0,
            }
        }
    }
    impl SessionRuntimeEffects for Effects {
        fn copy_bootstrap(
            &mut self,
        ) -> pythos_shared::session_runtime_abi::SessionRuntimeBootstrapV1 {
            self.copies += 1;
            self.bootstrap
        }
        fn copy_fixture(&mut self) -> pythos_shared::session_runtime_abi::SessionRuntimeFixtureV1 {
            self.fixture
        }
        fn prepare_graph_package(
            &mut self,
            _: &pythos_shared::session_runtime_abi::SessionRuntimeBootstrapV1,
        ) -> Result<(), SessionRuntimeEffectError> {
            Ok(())
        }
        fn poll_input(&mut self, _: usize) -> Result<(), SessionRuntimeEffectError> {
            panic!("old coordinator input must stay separate")
        }
        fn run_command_host_and_graph(
            &mut self,
            ordinal: usize,
        ) -> Result<(), SessionRuntimeEffectError> {
            use pythos_shared::{
                object_shell_abi::PackedCapability,
                pyth_runtime_abi::{GRAPH_EXIT_OK, MAX_PYTH_GRAPH_IMPORTS},
                pyth_tig::{
                    format::PythGraphPackage,
                    opcode::{RIGHTS_APPEND, RIGHTS_READ},
                    test_support,
                    verify::verify_package,
                },
            };
            use pythos_user_pyth_runtime::interpreter::Interpreter;
            self.graph_inputs.push(self.event_index);
            if self.failure == Some(Failure::Graph) {
                return Err(SessionRuntimeEffectError::Graph);
            }
            let bytes = test_support::command_read_result_emit_with_import_rights(
                RIGHTS_READ | RIGHTS_APPEND,
            );
            let package = PythGraphPackage::decode(&bytes).unwrap();
            let verified = verify_package(&package).unwrap();
            let handle = self.bootstrap.graph.imports[0].capability;
            let mut imports = [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS];
            imports[0] = handle;
            let command = self.fixture.commands[ordinal];
            let payload = &self.fixture.payloads[ordinal][..command.payload_len as usize];
            let mut host =
                session_command_host::SessionCommandHost::new(handle, &command, payload).unwrap();
            let exit = Interpreter::new(
                verified,
                &imports,
                128,
                &mut self.values,
                &mut self.host_results,
            )
            .execute(&mut host);
            assert_eq!(exit.status, GRAPH_EXIT_OK);
            self.results[ordinal] = host.result();
            Ok(())
        }
        fn reset_invocation_local(&mut self) -> Result<(), SessionRuntimeEffectError> {
            self.resets += 1;
            self.values
                .fill(Some(pythos_user_pyth_runtime::value::Value::U64(u64::MAX)));
            self.host_results.fill(Some(
                pythos_shared::pyth_runtime_abi::HostCallResult::empty(u16::MAX),
            ));
            Ok(())
        }
        fn final_state_is_valid(&self) -> bool {
            panic!("old coordinator final gate")
        }
        fn write_terminal_result(
            &mut self,
            _: SessionRuntimeTerminalResult,
        ) -> Result<(), SessionRuntimeEffectError> {
            panic!("V1 result must stay separate")
        }
        fn emit_marker(&mut self, marker: &'static str) {
            self.markers.push(marker);
        }
        fn trap(&mut self) {
            self.traps += 1;
        }
    }
    impl ViewingRuntimeEffects for Effects {
        fn prepare_viewing(
            &mut self,
            _: &pythos_shared::session_runtime_abi::SessionRuntimeBootstrapV1,
        ) -> Result<ViewingExtent, SessionRuntimeEffectError> {
            if self.failure == Some(Failure::Bootstrap) {
                return Err(SessionRuntimeEffectError::Package);
            }
            Ok(ViewingExtent::new(640, 480).unwrap())
        }
        fn next_viewing_event(&mut self) -> Result<SessionInputEventV1, SessionRuntimeEffectError> {
            if self.failure == Some(Failure::Input) {
                return Err(SessionRuntimeEffectError::Poll);
            }
            let event = self.events[self.event_index];
            self.event_index += 1;
            Ok(event)
        }
        fn present_viewing(
            &mut self,
            revision: u64,
            snapshot: ViewingSnapshot,
        ) -> Result<(), SessionRuntimeEffectError> {
            if self.failure == Some(Failure::Draw) {
                return Err(SessionRuntimeEffectError::Result);
            }
            assert_eq!(revision as usize, self.draws.len());
            self.draws.push(snapshot);
            Ok(())
        }
        fn write_viewing_result(
            &mut self,
            result: &SessionViewingResultV1,
        ) -> Result<(), SessionRuntimeEffectError> {
            assert!(self.result.is_none());
            self.result = Some(*result);
            Ok(())
        }
    }
}
