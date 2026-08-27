//! Opt-in physical input event diagnostic for post-wake keyboard bring-up.
//!
//! This module samples raw keyboard bytes, normalizes only the fixed key set
//! needed for the diagnostic sequence, and reports the observed state. It is
//! not a shell input path, HID stack, or generic keyboard layout.

#[cfg(feature = "physical-input-event-diagnostic")]
use crate::input_events::InputEventKind;
use crate::{
    input_drivers::{KeyCode, RawInputEvent},
    input_events::{self, InputEvent},
};

#[cfg(feature = "physical-input-event-diagnostic")]
use crate::{
    framebuffer::{self, PhysicalInputStatus},
    ps2, qemu_exit, serial,
};
#[cfg(feature = "physical-input-event-diagnostic")]
use pythos_shared::boot_protocol::PythFramebufferInfo;

const TEXT_CAPACITY: usize = 16;
const RAW_LOG_CAPACITY: usize = 16;
const KEY_LOG_CAPACITY: usize = 12;
const ACCEPT_SEQUENCE: [KeyCode; 9] = [
    KeyCode::Space,
    KeyCode::Space,
    KeyCode::Backspace,
    KeyCode::Backspace,
    KeyCode::W,
    KeyCode::A,
    KeyCode::K,
    KeyCode::E,
    KeyCode::Enter,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputDiagnosticResult {
    Waiting,
    Rejected,
    Accepted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InputDiagnosticStep {
    pub(crate) event: Option<InputEvent>,
    pub(crate) result: InputDiagnosticResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScanSetMode {
    Unknown,
    Set1,
    Set2,
}

pub(crate) struct InputDiagnostic {
    text: [u8; TEXT_CAPACITY],
    text_len: usize,
    raw: [u8; RAW_LOG_CAPACITY],
    raw_len: usize,
    keys: [KeyCode; KEY_LOG_CAPACITY],
    key_len: usize,
    mode: ScanSetMode,
    release_prefix: bool,
    extended_prefix: bool,
    last_result: InputDiagnosticResult,
}

impl InputDiagnostic {
    pub(crate) const fn new() -> Self {
        Self {
            text: [0; TEXT_CAPACITY],
            text_len: 0,
            raw: [0; RAW_LOG_CAPACITY],
            raw_len: 0,
            keys: [KeyCode::A; KEY_LOG_CAPACITY],
            key_len: 0,
            mode: ScanSetMode::Unknown,
            release_prefix: false,
            extended_prefix: false,
            last_result: InputDiagnosticResult::Waiting,
        }
    }

    pub(crate) fn feed_raw_byte(&mut self, byte: u8) -> InputDiagnosticStep {
        self.push_raw(byte);
        if self.last_result == InputDiagnosticResult::Accepted {
            return InputDiagnosticStep {
                event: None,
                result: InputDiagnosticResult::Accepted,
            };
        }
        if self.consume_non_make_byte(byte) {
            self.last_result = InputDiagnosticResult::Waiting;
            return InputDiagnosticStep {
                event: None,
                result: InputDiagnosticResult::Waiting,
            };
        }
        let Some(key) = self.decode_make_byte(byte) else {
            self.last_result = InputDiagnosticResult::Waiting;
            return InputDiagnosticStep {
                event: None,
                result: InputDiagnosticResult::Waiting,
            };
        };
        let raw = RawInputEvent::KeyPressed {
            scancode: byte,
            key,
        };
        let Ok(event) = input_events::normalize(raw) else {
            self.last_result = InputDiagnosticResult::Waiting;
            return InputDiagnosticStep {
                event: None,
                result: InputDiagnosticResult::Waiting,
            };
        };
        self.last_result = self.apply_key_event(key);
        InputDiagnosticStep {
            event: Some(event),
            result: self.last_result,
        }
    }

    pub(crate) fn text_bytes(&self) -> &[u8] {
        &self.text[..self.text_len]
    }

    pub(crate) fn raw_bytes(&self) -> &[u8] {
        &self.raw[..self.raw_len]
    }

    pub(crate) fn key_events(&self) -> &[KeyCode] {
        &self.keys[..self.key_len]
    }

    fn push_raw(&mut self, byte: u8) {
        if self.raw_len < RAW_LOG_CAPACITY {
            self.raw[self.raw_len] = byte;
            self.raw_len += 1;
            return;
        }
        self.raw.copy_within(1..RAW_LOG_CAPACITY, 0);
        self.raw[RAW_LOG_CAPACITY - 1] = byte;
    }

    fn push_key(&mut self, key: KeyCode) {
        if self.key_len < KEY_LOG_CAPACITY {
            self.keys[self.key_len] = key;
            self.key_len += 1;
            return;
        }
        self.keys.copy_within(1..KEY_LOG_CAPACITY, 0);
        self.keys[KEY_LOG_CAPACITY - 1] = key;
    }

    fn consume_non_make_byte(&mut self, byte: u8) -> bool {
        if self.extended_prefix {
            self.extended_prefix = false;
            if byte == 0xF0 {
                self.release_prefix = true;
            }
            return true;
        }
        if byte == 0xE0 {
            self.extended_prefix = true;
            return true;
        }
        if byte == 0xF0 {
            self.release_prefix = true;
            return true;
        }
        if self.release_prefix {
            self.release_prefix = false;
            return true;
        }
        byte & 0x80 != 0
    }

    fn decode_make_byte(&mut self, byte: u8) -> Option<KeyCode> {
        match self.mode {
            ScanSetMode::Unknown => {
                if let Some(key) = decode_set1_key(byte) {
                    self.mode = ScanSetMode::Set1;
                    return Some(key);
                }
                if let Some(key) = decode_set2_key(byte) {
                    self.mode = ScanSetMode::Set2;
                    return Some(key);
                }
                None
            }
            ScanSetMode::Set1 => decode_set1_key(byte),
            ScanSetMode::Set2 => decode_set2_key(byte),
        }
    }

    fn apply_key_event(&mut self, key: KeyCode) -> InputDiagnosticResult {
        self.push_key(key);
        match key {
            KeyCode::W => self.push_text_byte(b'w'),
            KeyCode::A => self.push_text_byte(b'a'),
            KeyCode::K => self.push_text_byte(b'k'),
            KeyCode::E => self.push_text_byte(b'e'),
            KeyCode::Space => self.push_text_byte(b' '),
            KeyCode::Backspace => {
                self.text_len = self.text_len.saturating_sub(1);
            }
            KeyCode::Enter => {
                if self.key_events().ends_with(&ACCEPT_SEQUENCE) {
                    return InputDiagnosticResult::Accepted;
                }
                return InputDiagnosticResult::Rejected;
            }
            _ => {}
        }
        InputDiagnosticResult::Waiting
    }

    fn push_text_byte(&mut self, byte: u8) {
        if self.text_len < TEXT_CAPACITY {
            self.text[self.text_len] = byte;
            self.text_len += 1;
            return;
        }
        self.text.copy_within(1..TEXT_CAPACITY, 0);
        self.text[TEXT_CAPACITY - 1] = byte;
    }
}

fn decode_set1_key(byte: u8) -> Option<KeyCode> {
    match byte {
        0x11 => Some(KeyCode::W),
        0x1E => Some(KeyCode::A),
        0x25 => Some(KeyCode::K),
        0x12 => Some(KeyCode::E),
        0x1C => Some(KeyCode::Enter),
        0x0E => Some(KeyCode::Backspace),
        0x39 => Some(KeyCode::Space),
        _ => None,
    }
}

fn decode_set2_key(byte: u8) -> Option<KeyCode> {
    match byte {
        0x1D => Some(KeyCode::W),
        0x1C => Some(KeyCode::A),
        0x42 => Some(KeyCode::K),
        0x24 => Some(KeyCode::E),
        0x5A => Some(KeyCode::Enter),
        0x66 => Some(KeyCode::Backspace),
        0x29 => Some(KeyCode::Space),
        _ => None,
    }
}

#[cfg(feature = "physical-input-event-diagnostic")]
pub(crate) fn run(framebuffer: &PythFramebufferInfo) {
    serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:ENTER");
    let mut diagnostic = InputDiagnostic::new();
    if ps2::initialize_keyboard_polling().is_err() {
        render_or_panic(framebuffer, &diagnostic, PhysicalInputStatus::Ps2InitFailed);
        serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:PS2_INIT_FAILED");
        loop {
            core::hint::spin_loop();
        }
    }

    render_or_panic(framebuffer, &diagnostic, PhysicalInputStatus::Ready);
    serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:READY");
    loop {
        let Some(byte) = ps2::poll_raw_output_byte() else {
            core::hint::spin_loop();
            continue;
        };
        write_raw_byte(byte);
        let step = diagnostic.feed_raw_byte(byte);
        if let Some(event) = step.event {
            write_event(event);
        }
        match step.result {
            InputDiagnosticResult::Waiting => {
                render_or_panic(framebuffer, &diagnostic, PhysicalInputStatus::Ready);
            }
            InputDiagnosticResult::Rejected => {
                render_or_panic(framebuffer, &diagnostic, PhysicalInputStatus::Rejected);
                serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:REJECTED");
            }
            InputDiagnosticResult::Accepted => {
                render_or_panic(framebuffer, &diagnostic, PhysicalInputStatus::Accepted);
                serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:ACCEPTED");
                return;
            }
        }
    }
}

#[cfg(feature = "physical-input-event-diagnostic")]
fn render_or_panic(
    framebuffer: &PythFramebufferInfo,
    diagnostic: &InputDiagnostic,
    status: PhysicalInputStatus,
) {
    if framebuffer::render_physical_input_diagnostic(
        framebuffer,
        diagnostic.text_bytes(),
        diagnostic.key_events(),
        diagnostic.raw_bytes(),
        status,
    )
    .is_err()
    {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
}

#[cfg(feature = "physical-input-event-diagnostic")]
fn write_raw_byte(byte: u8) {
    serial::write_hex_u64("PYTHOS:CORE:PHYSICAL_INPUT:RAW:", u64::from(byte));
}

#[cfg(feature = "physical-input-event-diagnostic")]
fn write_event(event: InputEvent) {
    let InputEventKind::KeyDown(key) = event.kind else {
        return;
    };
    match key {
        KeyCode::W => serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:KEY:W"),
        KeyCode::A => serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:KEY:A"),
        KeyCode::K => serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:KEY:K"),
        KeyCode::E => serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:KEY:E"),
        KeyCode::Enter => serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:KEY:ENTER"),
        KeyCode::Backspace => serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:KEY:BACKSPACE"),
        KeyCode::Space => serial::write_line("PYTHOS:CORE:PHYSICAL_INPUT:KEY:SPACE"),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_drivers::KeyCode;

    #[test]
    fn set1_space_backspace_wake_sequence_accepts() {
        let mut diagnostic = InputDiagnostic::new();
        let bytes = [0x39, 0x39, 0x0E, 0x0E, 0x11, 0x1E, 0x25, 0x12, 0x1C];
        for byte in &bytes[..bytes.len() - 1] {
            assert_eq!(
                diagnostic.feed_raw_byte(*byte).result,
                InputDiagnosticResult::Waiting
            );
        }

        assert_eq!(
            diagnostic.feed_raw_byte(bytes[bytes.len() - 1]).result,
            InputDiagnosticResult::Accepted
        );
        assert_eq!(diagnostic.text_bytes(), b"wake");
        assert_eq!(
            diagnostic.key_events(),
            &[
                KeyCode::Space,
                KeyCode::Space,
                KeyCode::Backspace,
                KeyCode::Backspace,
                KeyCode::W,
                KeyCode::A,
                KeyCode::K,
                KeyCode::E,
                KeyCode::Enter,
            ]
        );
    }

    #[test]
    fn set2_release_bytes_are_ignored_before_acceptance() {
        let mut diagnostic = InputDiagnostic::new();
        let bytes = [
            0x29, 0xF0, 0x29, 0x29, 0x66, 0x66, 0x1D, 0x1C, 0x42, 0x24, 0x5A,
        ];
        for byte in &bytes[..bytes.len() - 1] {
            assert_ne!(
                diagnostic.feed_raw_byte(*byte).result,
                InputDiagnosticResult::Accepted
            );
        }

        assert_eq!(
            diagnostic.feed_raw_byte(bytes[bytes.len() - 1]).result,
            InputDiagnosticResult::Accepted
        );
        assert_eq!(diagnostic.text_bytes(), b"wake");
    }

    #[test]
    fn raw_log_keeps_latest_bytes() {
        let mut diagnostic = InputDiagnostic::new();
        for byte in 0u8..20 {
            let _ = diagnostic.feed_raw_byte(byte);
        }

        assert_eq!(
            diagnostic.raw_bytes(),
            &[4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19]
        );
    }
}
