//! Pure retained normal-session controller and launch validation (ADR 0093).

use pythos_shared::{
    normal_session_abi::{
        NORMAL_SESSION_BOOTSTRAP_ADDRESS, NORMAL_SESSION_COMMAND_REJECTED, NORMAL_SESSION_READY,
        NORMAL_SESSION_SERVICE_ID, NORMAL_SESSION_STATUS_PAYLOAD_LEN, NORMAL_SESSION_STATUS_PREFIX,
        NORMAL_SESSION_VIEWPORT_HEIGHT, NORMAL_SESSION_VIEWPORT_WIDTH, NormalSessionBootstrapV1,
        NormalSessionReturnReason, NormalSessionValidationError, SESSION_WAIT_READY_MASK,
    },
    pyth_graph_manifest::digest64,
    pyth_tig::format::{PackageDecodeError, PythGraphPackage},
    session_input_abi::SessionInputEventV1,
    viewing::{ViewingExtent, ViewingSnapshot},
};

use crate::session_viewing::SessionViewing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalSessionLaunchError {
    NullBootstrapPointer,
    MisalignedBootstrapPointer,
    UnexpectedBootstrapAddress,
    InvalidBootstrap(NormalSessionValidationError),
    PackageLengthMismatch,
    PackageDigestMismatch,
    InvalidPackage(PackageDecodeError),
}

/// Validate the fixed scalar bootstrap address before constructing or
/// dereferencing a `NormalSessionBootstrapV1` pointer.
pub fn validate_normal_session_bootstrap_address(
    bootstrap_address: u64,
) -> Result<(), NormalSessionLaunchError> {
    if bootstrap_address == 0 {
        return Err(NormalSessionLaunchError::NullBootstrapPointer);
    }
    if !bootstrap_address.is_multiple_of(core::mem::align_of::<NormalSessionBootstrapV1>() as u64) {
        return Err(NormalSessionLaunchError::MisalignedBootstrapPointer);
    }
    if bootstrap_address != NORMAL_SESSION_BOOTSTRAP_ADDRESS {
        return Err(NormalSessionLaunchError::UnexpectedBootstrapAddress);
    }
    Ok(())
}

pub fn validate_normal_session_launch(
    bootstrap_address: u64,
    bootstrap: &NormalSessionBootstrapV1,
) -> Result<(), NormalSessionLaunchError> {
    validate_normal_session_bootstrap_address(bootstrap_address)?;
    pythos_shared::normal_session_abi::validate_normal_session_bootstrap(bootstrap)
        .map_err(NormalSessionLaunchError::InvalidBootstrap)?;
    Ok(())
}

pub fn validate_normal_session_package<'a>(
    bootstrap: &NormalSessionBootstrapV1,
    package_bytes: &'a [u8],
) -> Result<PythGraphPackage<'a>, NormalSessionLaunchError> {
    if package_bytes.len() as u64 != bootstrap.graph.package_len {
        return Err(NormalSessionLaunchError::PackageLengthMismatch);
    }
    if digest64(package_bytes) != bootstrap.graph_package_digest {
        return Err(NormalSessionLaunchError::PackageDigestMismatch);
    }
    PythGraphPackage::decode(package_bytes).map_err(NormalSessionLaunchError::InvalidPackage)
}

pub trait NormalSessionEffects {
    fn try_input(&mut self) -> Result<Option<SessionInputEventV1>, NormalSessionReturnReason>;
    fn try_console(&mut self) -> Result<Option<u8>, NormalSessionReturnReason>;
    fn wait(&mut self) -> Result<u64, NormalSessionReturnReason>;
    fn present(
        &mut self,
        revision: u64,
        snapshot: ViewingSnapshot,
    ) -> Result<(), NormalSessionReturnReason>;
    fn run_status(&mut self, payload: &[u8]) -> Result<(), NormalSessionReturnReason>;
    fn write_console(&mut self, bytes: &[u8]) -> Result<(), NormalSessionReturnReason>;
}

pub struct NormalSession {
    service_id: u64,
    viewing: SessionViewing,
    initialized: bool,
    event_count: u64,
    command_count: u64,
    presentation_revision: u64,
    line: [u8; pythos_shared::normal_session_abi::NORMAL_SESSION_MAX_COMMAND_LEN],
    line_len: usize,
    line_invalid: bool,
    suppress_lf: bool,
    terminal: Option<NormalSessionReturnReason>,
}

impl NormalSession {
    pub fn new(service_id: u64, extent: ViewingExtent) -> Result<Self, NormalSessionReturnReason> {
        if service_id != NORMAL_SESSION_SERVICE_ID
            || extent.width() != NORMAL_SESSION_VIEWPORT_WIDTH
            || extent.height() != NORMAL_SESSION_VIEWPORT_HEIGHT
        {
            return Err(NormalSessionReturnReason::Bootstrap);
        }
        Ok(Self {
            service_id,
            viewing: SessionViewing::new(extent),
            initialized: false,
            event_count: 0,
            command_count: 0,
            presentation_revision: 0,
            line: [0; pythos_shared::normal_session_abi::NORMAL_SESSION_MAX_COMMAND_LEN],
            line_len: 0,
            line_invalid: false,
            suppress_lf: false,
            terminal: None,
        })
    }

    pub fn initialize(
        &mut self,
        effects: &mut impl NormalSessionEffects,
    ) -> Result<(), NormalSessionReturnReason> {
        self.ensure_live()?;
        if self.initialized {
            return self.fail(NormalSessionReturnReason::Bootstrap);
        }
        if let Err(reason) = effects.present(0, self.viewing.snapshot()) {
            return self.fail(reason);
        }
        if let Err(reason) = effects.write_console(NORMAL_SESSION_READY) {
            return self.fail(reason);
        }
        self.initialized = true;
        Ok(())
    }

    pub fn step(
        &mut self,
        effects: &mut impl NormalSessionEffects,
    ) -> Result<(), NormalSessionReturnReason> {
        self.ensure_live()?;
        if !self.initialized {
            return self.fail(NormalSessionReturnReason::Bootstrap);
        }

        let mut did_work = false;
        match effects.try_input() {
            Ok(Some(event)) => {
                did_work = true;
                let event_count = match self.event_count.checked_add(1) {
                    Some(value) => value,
                    None => return self.fail(NormalSessionReturnReason::CounterOverflow),
                };
                let revision = match self.presentation_revision.checked_add(1) {
                    Some(value) => value,
                    None => return self.fail(NormalSessionReturnReason::CounterOverflow),
                };
                let receipt = match self.viewing.observe(event) {
                    Ok(receipt) => receipt,
                    Err(_) => return self.fail(NormalSessionReturnReason::Input),
                };
                self.event_count = event_count;
                self.presentation_revision = revision;
                if let Err(reason) = effects.present(revision, receipt.snapshot) {
                    return self.fail(reason);
                }
            }
            Ok(None) => {}
            Err(reason) => return self.fail(reason),
        }

        match effects.try_console() {
            Ok(Some(byte)) => {
                did_work = true;
                self.consume_console_byte(effects, byte)?;
            }
            Ok(None) => {}
            Err(reason) => return self.fail(reason),
        }

        if !did_work {
            let readiness = match effects.wait() {
                Ok(readiness) => readiness,
                Err(reason) => return self.fail(reason),
            };
            if readiness & !SESSION_WAIT_READY_MASK != 0 {
                return self.fail(NormalSessionReturnReason::Bootstrap);
            }
        }
        Ok(())
    }

    pub const fn service_id(&self) -> u64 {
        self.service_id
    }

    pub const fn event_count(&self) -> u64 {
        self.event_count
    }

    pub const fn command_count(&self) -> u64 {
        self.command_count
    }

    pub const fn presentation_revision(&self) -> u64 {
        self.presentation_revision
    }

    pub fn snapshot(&self) -> ViewingSnapshot {
        self.viewing.snapshot()
    }

    pub const fn terminal_reason(&self) -> Option<NormalSessionReturnReason> {
        self.terminal
    }

    fn ensure_live(&self) -> Result<(), NormalSessionReturnReason> {
        match self.terminal {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }

    fn fail<T>(
        &mut self,
        reason: NormalSessionReturnReason,
    ) -> Result<T, NormalSessionReturnReason> {
        self.terminal = Some(reason);
        Err(reason)
    }

    fn consume_console_byte(
        &mut self,
        effects: &mut impl NormalSessionEffects,
        byte: u8,
    ) -> Result<(), NormalSessionReturnReason> {
        if self.suppress_lf {
            self.suppress_lf = false;
            if byte == b'\n' {
                return Ok(());
            }
        }
        if byte == b'\r' {
            self.suppress_lf = true;
            return self.finish_line(effects);
        }
        if byte == b'\n' {
            return self.finish_line(effects);
        }
        if self.line_invalid {
            return Ok(());
        }
        if !byte.is_ascii() || self.line_len == self.line.len() {
            self.line_invalid = true;
            return Ok(());
        }
        self.line[self.line_len] = byte;
        self.line_len += 1;
        Ok(())
    }

    fn finish_line(
        &mut self,
        effects: &mut impl NormalSessionEffects,
    ) -> Result<(), NormalSessionReturnReason> {
        let is_status = !self.line_invalid && &self.line[..self.line_len] == b"status";
        let is_recover = !self.line_invalid && &self.line[..self.line_len] == b"recover";
        self.line[..self.line_len].fill(0);
        self.line_len = 0;
        self.line_invalid = false;

        if is_status {
            return self.run_status(effects);
        }
        if is_recover {
            return self.fail(NormalSessionReturnReason::ExplicitRecovery);
        }
        if let Err(reason) = effects.write_console(NORMAL_SESSION_COMMAND_REJECTED) {
            return self.fail(reason);
        }
        Ok(())
    }

    fn run_status(
        &mut self,
        effects: &mut impl NormalSessionEffects,
    ) -> Result<(), NormalSessionReturnReason> {
        let completed_commands = match self.command_count.checked_add(1) {
            Some(value) => value,
            None => return self.fail(NormalSessionReturnReason::CounterOverflow),
        };
        let payload = status_payload(
            self.event_count,
            completed_commands,
            self.presentation_revision,
            self.viewing.snapshot(),
        );
        if let Err(reason) = effects.run_status(&payload) {
            return self.fail(reason);
        }
        self.command_count = completed_commands;
        let mut line = [0u8; 97];
        line[..NORMAL_SESSION_STATUS_PREFIX.len()].copy_from_slice(NORMAL_SESSION_STATUS_PREFIX);
        let payload_start = NORMAL_SESSION_STATUS_PREFIX.len();
        let payload_end = payload_start + payload.len();
        line[payload_start..payload_end].copy_from_slice(&payload);
        line[payload_end..].copy_from_slice(b"\r\n");
        if let Err(reason) = effects.write_console(&line) {
            return self.fail(reason);
        }
        Ok(())
    }
}

fn status_payload(
    event_count: u64,
    command_count: u64,
    revision: u64,
    snapshot: ViewingSnapshot,
) -> [u8; NORMAL_SESSION_STATUS_PAYLOAD_LEN] {
    let mut payload = [0u8; NORMAL_SESSION_STATUS_PAYLOAD_LEN];
    payload[0] = b'e';
    write_hex_u64(&mut payload[1..17], event_count);
    payload[17] = b'c';
    write_hex_u64(&mut payload[18..34], command_count);
    payload[34] = b'r';
    write_hex_u64(&mut payload[35..51], revision);
    payload[51] = b'a';
    let (active, x, y) = match snapshot.focus_mark {
        Some(position) => (b'1', position.x, position.y),
        None => (b'0', 0, 0),
    };
    payload[52] = active;
    payload[53] = b'x';
    write_hex_u12(&mut payload[54..57], x);
    payload[57] = b'y';
    write_hex_u12(&mut payload[58..61], y);
    payload
}

fn write_hex_u64(output: &mut [u8], value: u64) {
    for (index, byte) in output.iter_mut().enumerate() {
        let shift = (15 - index) * 4;
        *byte = hex_digit(((value >> shift) & 0x0f) as u8);
    }
}

fn write_hex_u12(output: &mut [u8], value: u32) {
    for (index, byte) in output.iter_mut().enumerate() {
        let shift = (2 - index) * 4;
        *byte = hex_digit(((value >> shift) & 0x0f) as u8);
    }
}

const fn hex_digit(value: u8) -> u8 {
    if value < 10 {
        b'0' + value
    } else {
        b'a' + (value - 10)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use pythos_shared::{
        capability_abi::PackedCapability,
        normal_session_abi::{
            NORMAL_SESSION_ABI_MAJOR, NORMAL_SESSION_ABI_MINOR, NORMAL_SESSION_BOOTSTRAP_ADDRESS,
            NORMAL_SESSION_BOOTSTRAP_MAGIC, NORMAL_SESSION_COMMAND_REJECTED,
            NORMAL_SESSION_GRAPH_PRINCIPAL_ID, NORMAL_SESSION_GRAPH_RESULT_ADDRESS,
            NORMAL_SESSION_READY, NORMAL_SESSION_RETURN_ADDRESS, NORMAL_SESSION_SERVICE_ID,
            NORMAL_SESSION_STATUS_PREFIX, NormalSessionBootstrapV1, NormalSessionReturnReason,
        },
        pyth_graph_manifest::digest64,
        pyth_runtime_abi::{
            PYTH_GRAPH_BOOTSTRAP_MAGIC, PYTH_GRAPH_RUNTIME_ABI_MAJOR, PYTH_GRAPH_RUNTIME_ABI_MINOR,
            PythGraphCapabilityBinding,
        },
        pyth_tig::{format::PackageDecodeError, opcode::RIGHTS_READ, test_support},
        session_input_abi::{
            KEY_BACKSPACE, KEY_ENTER, KEY_SPACE, SESSION_INPUT_FLAG_GAP_BEFORE,
            SESSION_INPUT_KIND_KEY_DOWN, SESSION_INPUT_KIND_RELATIVE_MOTION,
            SESSION_INPUT_SOURCE_KEYBOARD, SESSION_INPUT_SOURCE_MOUSE, SessionInputEventV1,
        },
        user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID,
        viewing::{FocusMarkPosition, ViewingExtent, ViewingSnapshot},
    };
    use std::{collections::VecDeque, vec, vec::Vec};

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum EffectCall {
        Input,
        Console,
        Wait,
        Present(u64, ViewingSnapshot),
        Status(Vec<u8>),
        Write(Vec<u8>),
    }

    struct RecordedEffects {
        inputs: VecDeque<Result<Option<SessionInputEventV1>, NormalSessionReturnReason>>,
        console: VecDeque<Result<Option<u8>, NormalSessionReturnReason>>,
        waits: VecDeque<Result<u64, NormalSessionReturnReason>>,
        present_error: Option<NormalSessionReturnReason>,
        status_error: Option<NormalSessionReturnReason>,
        write_error: Option<NormalSessionReturnReason>,
        calls: Vec<EffectCall>,
    }

    impl RecordedEffects {
        fn new() -> Self {
            Self {
                inputs: VecDeque::new(),
                console: VecDeque::new(),
                waits: VecDeque::new(),
                present_error: None,
                status_error: None,
                write_error: None,
                calls: Vec::new(),
            }
        }

        fn written(&self) -> Vec<u8> {
            let mut bytes = Vec::new();
            for call in &self.calls {
                if let EffectCall::Write(part) = call {
                    bytes.extend_from_slice(part);
                }
            }
            bytes
        }
    }

    impl NormalSessionEffects for RecordedEffects {
        fn try_input(&mut self) -> Result<Option<SessionInputEventV1>, NormalSessionReturnReason> {
            self.calls.push(EffectCall::Input);
            self.inputs.pop_front().unwrap_or(Ok(None))
        }

        fn try_console(&mut self) -> Result<Option<u8>, NormalSessionReturnReason> {
            self.calls.push(EffectCall::Console);
            self.console.pop_front().unwrap_or(Ok(None))
        }

        fn wait(&mut self) -> Result<u64, NormalSessionReturnReason> {
            self.calls.push(EffectCall::Wait);
            self.waits.pop_front().unwrap_or(Ok(0))
        }

        fn present(
            &mut self,
            revision: u64,
            snapshot: ViewingSnapshot,
        ) -> Result<(), NormalSessionReturnReason> {
            self.calls.push(EffectCall::Present(revision, snapshot));
            self.present_error.map_or(Ok(()), Err)
        }

        fn run_status(&mut self, payload: &[u8]) -> Result<(), NormalSessionReturnReason> {
            self.calls.push(EffectCall::Status(payload.to_vec()));
            self.status_error.map_or(Ok(()), Err)
        }

        fn write_console(&mut self, bytes: &[u8]) -> Result<(), NormalSessionReturnReason> {
            self.calls.push(EffectCall::Write(bytes.to_vec()));
            self.write_error.map_or(Ok(()), Err)
        }
    }

    #[test]
    fn launch_checks_address_outer_contract_and_admitted_package_separately() {
        assert_eq!(
            validate_normal_session_bootstrap_address(0),
            Err(NormalSessionLaunchError::NullBootstrapPointer)
        );
        assert_eq!(
            validate_normal_session_bootstrap_address(0x7200_0001),
            Err(NormalSessionLaunchError::MisalignedBootstrapPointer)
        );
        assert_eq!(
            validate_normal_session_bootstrap_address(0x7200_1000),
            Err(NormalSessionLaunchError::UnexpectedBootstrapAddress)
        );

        let package = test_support::command_read_result_emit_with_import_rights(RIGHTS_READ | 0x10);
        let mut bootstrap = accepted_bootstrap(package.len() as u64, digest64(&package));
        assert_eq!(
            validate_normal_session_launch(NORMAL_SESSION_BOOTSTRAP_ADDRESS, &bootstrap),
            Ok(())
        );
        assert!(validate_normal_session_package(&bootstrap, &package).is_ok());

        bootstrap.graph.package_len += 1;
        assert_eq!(
            validate_normal_session_package(&bootstrap, &package),
            Err(NormalSessionLaunchError::PackageLengthMismatch)
        );
        bootstrap.graph.package_len -= 1;
        bootstrap.graph_package_digest ^= 1;
        assert_eq!(
            validate_normal_session_package(&bootstrap, &package),
            Err(NormalSessionLaunchError::PackageDigestMismatch)
        );

        let malformed = [0u8; 8];
        bootstrap.graph.package_len = malformed.len() as u64;
        bootstrap.graph_package_digest = digest64(&malformed);
        assert_eq!(
            validate_normal_session_package(&bootstrap, &malformed),
            Err(NormalSessionLaunchError::InvalidPackage(
                PackageDecodeError::HeaderTooShort
            ))
        );
    }

    #[test]
    fn initialize_presents_inactive_revision_zero_before_ready_and_only_once() {
        assert_eq!(
            NormalSession::new(0, extent()).err(),
            Some(NormalSessionReturnReason::Bootstrap)
        );
        assert_eq!(
            NormalSession::new(0x5059_5345_5353_0002, extent()).err(),
            Some(NormalSessionReturnReason::Bootstrap)
        );
        assert_eq!(
            NormalSession::new(0x5059_5345_5353_0001, ViewingExtent::new(639, 480).unwrap()).err(),
            Some(NormalSessionReturnReason::Bootstrap)
        );
        let mut session = session();
        let mut effects = RecordedEffects::new();
        session.initialize(&mut effects).unwrap();

        assert_eq!(
            effects.calls,
            vec![
                EffectCall::Present(
                    0,
                    ViewingSnapshot {
                        extent: extent(),
                        focus_mark: None,
                    }
                ),
                EffectCall::Write(NORMAL_SESSION_READY.to_vec()),
            ]
        );
        assert_eq!(session.event_count(), 0);
        assert_eq!(session.command_count(), 0);
        assert_eq!(session.presentation_revision(), 0);

        assert_eq!(
            session.initialize(&mut effects),
            Err(NormalSessionReturnReason::Bootstrap)
        );
        let calls_after_failure = effects.calls.len();
        assert_eq!(
            session.step(&mut effects),
            Err(NormalSessionReturnReason::Bootstrap)
        );
        assert_eq!(effects.calls.len(), calls_after_failure);
    }

    #[test]
    fn each_step_is_fair_and_waits_once_only_when_both_sources_are_idle() {
        let mut session = initialized_session();
        let mut effects = RecordedEffects::new();
        effects.inputs.push_back(Ok(Some(key(0, KEY_ENTER))));
        effects.inputs.push_back(Ok(Some(key(1, KEY_ENTER))));
        effects.console.push_back(Ok(Some(b'x')));
        effects.console.push_back(Ok(Some(b'y')));

        session.step(&mut effects).unwrap();
        assert_eq!(effects.inputs.len(), 1);
        assert_eq!(effects.console.len(), 1);
        assert_eq!(session.event_count(), 1);
        assert!(!effects.calls.contains(&EffectCall::Wait));

        effects.calls.clear();
        session.step(&mut effects).unwrap();
        assert_eq!(session.event_count(), 2);
        assert!(!effects.calls.contains(&EffectCall::Wait));

        effects.calls.clear();
        effects.waits.push_back(Ok(0));
        session.step(&mut effects).unwrap();
        assert_eq!(
            effects.calls,
            vec![EffectCall::Input, EffectCall::Console, EffectCall::Wait]
        );

        for readiness in [1, 2, 3] {
            effects.calls.clear();
            effects.waits.push_back(Ok(readiness));
            session.step(&mut effects).unwrap();
            assert_eq!(
                effects.calls,
                vec![EffectCall::Input, EffectCall::Console, EffectCall::Wait]
            );
        }
    }

    #[test]
    fn retained_viewing_survives_idle_and_four_status_commands_across_twenty_events() {
        let mut session = initialized_session();
        let mut effects = RecordedEffects::new();

        feed_event(&mut session, &mut effects, key(0, KEY_SPACE));
        idle(&mut session, &mut effects);
        send_line(&mut session, &mut effects, b"status\r\n");
        feed_event(&mut session, &mut effects, key(1, KEY_SPACE));
        send_line(&mut session, &mut effects, b"status\r");
        idle(&mut session, &mut effects);
        feed_event(&mut session, &mut effects, key(2, KEY_BACKSPACE));
        send_line(&mut session, &mut effects, b"status\n");
        feed_event(&mut session, &mut effects, key(3, KEY_BACKSPACE));
        idle(&mut session, &mut effects);
        feed_event(&mut session, &mut effects, motion(4, 7, -7));
        for sequence in 5..20 {
            feed_event(&mut session, &mut effects, key(sequence, KEY_ENTER));
        }
        send_line(&mut session, &mut effects, b"status\r\n");

        assert_eq!(session.event_count(), 20);
        assert_eq!(session.command_count(), 4);
        assert_eq!(session.presentation_revision(), 20);
        assert_eq!(
            session.snapshot().focus_mark,
            Some(FocusMarkPosition { x: 327, y: 233 })
        );
        let statuses: Vec<_> = effects
            .calls
            .iter()
            .filter_map(|call| match call {
                EffectCall::Status(payload) => Some(payload.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(statuses.len(), 4);
        assert_eq!(
            statuses[0],
            b"e0000000000000001c0000000000000001r0000000000000001a0x000y000"
        );
        assert_eq!(
            statuses[1],
            b"e0000000000000002c0000000000000002r0000000000000002a0x000y000"
        );
        assert_eq!(
            statuses[2],
            b"e0000000000000003c0000000000000003r0000000000000003a0x000y000"
        );
        assert_eq!(
            statuses[3],
            b"e0000000000000014c0000000000000004r0000000000000014a1x147y0e9"
        );
    }

    #[test]
    fn duplicate_activation_is_idempotent_and_enter_never_requests_recovery() {
        let mut session = initialized_session();
        let mut effects = RecordedEffects::new();
        let keys = [KEY_SPACE, KEY_SPACE, KEY_BACKSPACE, KEY_BACKSPACE];
        for (sequence, key_code) in keys.into_iter().enumerate() {
            feed_event(&mut session, &mut effects, key(sequence as u64, key_code));
        }
        feed_event(&mut session, &mut effects, motion(4, 5, 6));
        let before = session.snapshot();
        assert_eq!(
            before.focus_mark,
            Some(FocusMarkPosition { x: 325, y: 246 })
        );
        for (offset, key_code) in keys.into_iter().enumerate() {
            feed_event(&mut session, &mut effects, key(5 + offset as u64, key_code));
        }
        feed_event(&mut session, &mut effects, key(9, KEY_ENTER));
        assert_eq!(session.snapshot(), before);
        assert_eq!(session.terminal_reason(), None);
    }

    #[test]
    fn command_admission_handles_crlf_unknown_overlong_and_explicit_recovery() {
        let mut session = initialized_session();
        let mut effects = RecordedEffects::new();
        send_line(&mut session, &mut effects, b"status\r\n");
        assert_eq!(
            effects.written(),
            [
                NORMAL_SESSION_STATUS_PREFIX,
                b"e0000000000000000c0000000000000001r0000000000000000a0x000y000",
                b"\r\n",
            ]
            .concat()
        );

        effects.calls.clear();
        send_line(&mut session, &mut effects, b"unknown\n");
        assert_eq!(effects.written(), NORMAL_SESSION_COMMAND_REJECTED);
        assert!(
            !effects
                .calls
                .iter()
                .any(|call| matches!(call, EffectCall::Status(_)))
        );

        effects.calls.clear();
        send_line(
            &mut session,
            &mut effects,
            b"abcdefghijklmnopqrstuvwxyz1234567\r",
        );
        assert_eq!(effects.written(), NORMAL_SESSION_COMMAND_REJECTED);
        assert_eq!(session.command_count(), 1);
        effects.calls.clear();
        send_line(&mut session, &mut effects, b"status\r");
        assert_eq!(session.command_count(), 2);
        assert_eq!(
            effects
                .calls
                .iter()
                .filter(|call| matches!(call, EffectCall::Status(_)))
                .count(),
            1
        );

        effects.calls.clear();
        let result = send_line_result(&mut session, &mut effects, b"recover\r");
        assert_eq!(result, Err(NormalSessionReturnReason::ExplicitRecovery));
        assert!(
            !effects
                .calls
                .iter()
                .any(|call| matches!(call, EffectCall::Status(_)))
        );
        let calls = effects.calls.len();
        assert_eq!(
            session.step(&mut effects),
            Err(NormalSessionReturnReason::ExplicitRecovery)
        );
        assert_eq!(effects.calls.len(), calls);
    }

    #[test]
    fn gap_resets_partial_activation_while_malformed_or_sequence_errors_are_sticky() {
        let mut session = initialized_session();
        let mut effects = RecordedEffects::new();
        feed_event(&mut session, &mut effects, key(0, KEY_SPACE));
        let mut gap = key(9, KEY_BACKSPACE);
        gap.flags = SESSION_INPUT_FLAG_GAP_BEFORE;
        feed_event(&mut session, &mut effects, gap);
        for (sequence, key_code) in [(10, KEY_SPACE), (11, KEY_SPACE), (12, KEY_BACKSPACE)] {
            feed_event(&mut session, &mut effects, key(sequence, key_code));
        }
        assert_eq!(session.snapshot().focus_mark, None);

        let malformed = SessionInputEventV1 {
            reserved0: 1,
            ..key(13, KEY_BACKSPACE)
        };
        effects.inputs.push_back(Ok(Some(malformed)));
        assert_eq!(
            session.step(&mut effects),
            Err(NormalSessionReturnReason::Input)
        );
        let calls = effects.calls.len();
        assert_eq!(
            session.step(&mut effects),
            Err(NormalSessionReturnReason::Input)
        );
        assert_eq!(effects.calls.len(), calls);

        let mut discontinuous = initialized_session();
        let mut effects = RecordedEffects::new();
        feed_event(&mut discontinuous, &mut effects, key(0, KEY_SPACE));
        effects.inputs.push_back(Ok(Some(key(2, KEY_SPACE))));
        assert_eq!(
            discontinuous.step(&mut effects),
            Err(NormalSessionReturnReason::Input)
        );
    }

    #[test]
    fn rejected_effects_are_terminal_and_failed_status_never_commits_or_claims_success() {
        for (kind, reason) in [
            (0, NormalSessionReturnReason::Presentation),
            (1, NormalSessionReturnReason::Graph),
            (2, NormalSessionReturnReason::Console),
            (3, NormalSessionReturnReason::Input),
            (4, NormalSessionReturnReason::Input),
            (5, NormalSessionReturnReason::Console),
        ] {
            let mut session = session();
            let mut effects = RecordedEffects::new();
            match kind {
                0 => {
                    effects.present_error = Some(reason);
                    assert_eq!(session.initialize(&mut effects), Err(reason));
                }
                1 => {
                    session.initialize(&mut effects).unwrap();
                    effects.calls.clear();
                    effects.status_error = Some(reason);
                    assert_eq!(
                        send_line_result(&mut session, &mut effects, b"status\r"),
                        Err(reason)
                    );
                    assert_eq!(session.command_count(), 0);
                    assert_eq!(effects.written(), b"");
                }
                2 => {
                    session.initialize(&mut effects).unwrap();
                    effects.calls.clear();
                    effects.write_error = Some(reason);
                    assert_eq!(
                        send_line_result(&mut session, &mut effects, b"unknown\r"),
                        Err(reason)
                    );
                }
                _ => {
                    session.initialize(&mut effects).unwrap();
                    effects.calls.clear();
                    if kind == 3 {
                        effects.waits.push_back(Err(reason));
                    } else if kind == 4 {
                        effects.inputs.push_back(Err(reason));
                    } else {
                        effects.console.push_back(Err(reason));
                    }
                    assert_eq!(session.step(&mut effects), Err(reason));
                }
            }
            let calls = effects.calls.len();
            assert_eq!(session.step(&mut effects), Err(reason));
            assert_eq!(effects.calls.len(), calls);
        }

        let mut invalid_mask = initialized_session();
        let mut effects = RecordedEffects::new();
        effects.waits.push_back(Ok(4));
        assert_eq!(
            invalid_mask.step(&mut effects),
            Err(NormalSessionReturnReason::Bootstrap)
        );
    }

    #[test]
    fn checked_event_revision_and_command_counters_never_wrap() {
        let mut effects = RecordedEffects::new();
        let mut event_overflow = initialized_session();
        event_overflow.event_count = u64::MAX;
        effects.inputs.push_back(Ok(Some(key(0, KEY_ENTER))));
        assert_eq!(
            event_overflow.step(&mut effects),
            Err(NormalSessionReturnReason::CounterOverflow)
        );

        let mut revision_overflow = initialized_session();
        revision_overflow.presentation_revision = u64::MAX;
        let mut effects = RecordedEffects::new();
        effects.inputs.push_back(Ok(Some(key(0, KEY_ENTER))));
        assert_eq!(
            revision_overflow.step(&mut effects),
            Err(NormalSessionReturnReason::CounterOverflow)
        );

        let mut command_overflow = initialized_session();
        command_overflow.command_count = u64::MAX;
        let mut effects = RecordedEffects::new();
        assert_eq!(
            send_line_result(&mut command_overflow, &mut effects, b"status\r"),
            Err(NormalSessionReturnReason::CounterOverflow)
        );
        assert!(
            !effects
                .calls
                .iter()
                .any(|call| matches!(call, EffectCall::Status(_)))
        );
    }

    fn extent() -> ViewingExtent {
        ViewingExtent::new(640, 480).unwrap()
    }

    fn session() -> NormalSession {
        NormalSession::new(0x5059_5345_5353_0001, extent()).unwrap()
    }

    fn initialized_session() -> NormalSession {
        let mut session = session();
        session.initialize(&mut RecordedEffects::new()).unwrap();
        session
    }

    fn feed_event(
        session: &mut NormalSession,
        effects: &mut RecordedEffects,
        event: SessionInputEventV1,
    ) {
        effects.inputs.push_back(Ok(Some(event)));
        session.step(effects).unwrap();
    }

    fn idle(session: &mut NormalSession, effects: &mut RecordedEffects) {
        effects.waits.push_back(Ok(0));
        session.step(effects).unwrap();
    }

    fn send_line(session: &mut NormalSession, effects: &mut RecordedEffects, line: &[u8]) {
        send_line_result(session, effects, line).unwrap();
    }

    fn send_line_result(
        session: &mut NormalSession,
        effects: &mut RecordedEffects,
        line: &[u8],
    ) -> Result<(), NormalSessionReturnReason> {
        let mut result = Ok(());
        for &byte in line {
            effects.console.push_back(Ok(Some(byte)));
            result = session.step(effects);
            if result.is_err() {
                break;
            }
        }
        result
    }

    fn key(sequence: u64, key_code: u16) -> SessionInputEventV1 {
        SessionInputEventV1 {
            sequence,
            kind: SESSION_INPUT_KIND_KEY_DOWN,
            source: SESSION_INPUT_SOURCE_KEYBOARD,
            flags: 0,
            value0: i32::from(key_code),
            value1: 0,
            reserved0: 0,
            reserved1: 0,
        }
    }

    fn motion(sequence: u64, dx: i32, dy: i32) -> SessionInputEventV1 {
        SessionInputEventV1 {
            sequence,
            kind: SESSION_INPUT_KIND_RELATIVE_MOTION,
            source: SESSION_INPUT_SOURCE_MOUSE,
            flags: 0,
            value0: dx,
            value1: dy,
            reserved0: 0,
            reserved1: 0,
        }
    }

    fn accepted_bootstrap(package_len: u64, digest: u64) -> NormalSessionBootstrapV1 {
        let mut bootstrap = NormalSessionBootstrapV1::empty();
        bootstrap.magic = NORMAL_SESSION_BOOTSTRAP_MAGIC;
        bootstrap.abi_major = NORMAL_SESSION_ABI_MAJOR;
        bootstrap.abi_minor = NORMAL_SESSION_ABI_MINOR;
        bootstrap.session_service_id = NORMAL_SESSION_SERVICE_ID;
        bootstrap.runtime_principal_id = SESSION_RUNTIME_PRINCIPAL_ID;
        bootstrap.graph_principal_id = NORMAL_SESSION_GRAPH_PRINCIPAL_ID;
        bootstrap.console_capability = PackedCapability::from_raw(1);
        bootstrap.input_capability = PackedCapability::from_raw(2);
        bootstrap.presentation_capability = PackedCapability::from_raw(3);
        bootstrap.graph_package_digest = digest;
        bootstrap.return_ptr = NORMAL_SESSION_RETURN_ADDRESS;
        bootstrap.return_len = 32;
        bootstrap.width = 640;
        bootstrap.height = 480;
        bootstrap.graph.magic = PYTH_GRAPH_BOOTSTRAP_MAGIC;
        bootstrap.graph.abi_major = PYTH_GRAPH_RUNTIME_ABI_MAJOR;
        bootstrap.graph.abi_minor = PYTH_GRAPH_RUNTIME_ABI_MINOR;
        bootstrap.graph.import_count = 1;
        bootstrap.graph.package_ptr = 0x7200_1000;
        bootstrap.graph.package_len = package_len;
        bootstrap.graph.instruction_budget = 128;
        bootstrap.graph.result_ptr = NORMAL_SESSION_GRAPH_RESULT_ADDRESS;
        bootstrap.graph.imports[0] = PythGraphCapabilityBinding {
            import_slot: 0,
            resource_kind: 6,
            reserved0: 0,
            rights: 0x11,
            capability: PackedCapability::from_raw(4),
        };
        bootstrap
    }
}
