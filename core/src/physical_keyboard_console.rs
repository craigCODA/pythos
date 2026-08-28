//! Feature-gated physical keyboard ingress for the object-shell console.
//!
//! PythCore owns the i8042 port reads. This module only translates the small
//! `KeyCode` surface already used by the kernel input path into bytes that the
//! existing shell line editor understands.

use crate::input_drivers::{KeyCode, scancode_to_keycode};

#[cfg(all(not(test), feature = "physical-keyboard-console"))]
use crate::{ps2, serial};
#[cfg(all(not(test), feature = "physical-keyboard-console"))]
use core::cell::UnsafeCell;

#[cfg(all(not(test), feature = "physical-keyboard-console"))]
const POLL_DRAIN_LIMIT: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScanSetMode {
    Unknown,
    Set1,
    Set2,
}

pub(crate) struct KeyboardConsoleDecoder {
    mode: ScanSetMode,
    release_prefix: bool,
    extended_prefix: bool,
}

impl KeyboardConsoleDecoder {
    pub(crate) const fn new() -> Self {
        Self {
            mode: ScanSetMode::Unknown,
            release_prefix: false,
            extended_prefix: false,
        }
    }

    pub(crate) fn feed_raw_byte(&mut self, byte: u8) -> Option<u8> {
        if self.consume_non_make_byte(byte) {
            return None;
        }
        let key = self.decode_make_byte(byte)?;
        keycode_to_console_byte(key)
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
        if matches!(self.mode, ScanSetMode::Set1 | ScanSetMode::Unknown) && byte & 0x80 != 0 {
            self.mode = ScanSetMode::Set1;
            return true;
        }
        false
    }

    fn decode_make_byte(&mut self, byte: u8) -> Option<KeyCode> {
        match self.mode {
            ScanSetMode::Unknown => {
                if let Some(key) = scancode_to_keycode(byte) {
                    self.mode = ScanSetMode::Set1;
                    return Some(key);
                }
                if let Some(key) = decode_set2_key(byte) {
                    self.mode = ScanSetMode::Set2;
                    return Some(key);
                }
                None
            }
            ScanSetMode::Set1 => scancode_to_keycode(byte),
            ScanSetMode::Set2 => decode_set2_key(byte),
        }
    }
}

pub(crate) fn keycode_to_console_byte(key: KeyCode) -> Option<u8> {
    match key {
        KeyCode::A => Some(b'a'),
        KeyCode::B => Some(b'b'),
        KeyCode::C => Some(b'c'),
        KeyCode::D => Some(b'd'),
        KeyCode::E => Some(b'e'),
        KeyCode::F => Some(b'f'),
        KeyCode::G => Some(b'g'),
        KeyCode::H => Some(b'h'),
        KeyCode::I => Some(b'i'),
        KeyCode::J => Some(b'j'),
        KeyCode::K => Some(b'k'),
        KeyCode::L => Some(b'l'),
        KeyCode::M => Some(b'm'),
        KeyCode::N => Some(b'n'),
        KeyCode::O => Some(b'o'),
        KeyCode::P => Some(b'p'),
        KeyCode::Q => Some(b'q'),
        KeyCode::R => Some(b'r'),
        KeyCode::S => Some(b's'),
        KeyCode::T => Some(b't'),
        KeyCode::U => Some(b'u'),
        KeyCode::V => Some(b'v'),
        KeyCode::W => Some(b'w'),
        KeyCode::X => Some(b'x'),
        KeyCode::Y => Some(b'y'),
        KeyCode::Z => Some(b'z'),
        KeyCode::Digit0 => Some(b'0'),
        KeyCode::Digit1 => Some(b'1'),
        KeyCode::Digit2 => Some(b'2'),
        KeyCode::Digit3 => Some(b'3'),
        KeyCode::Digit4 => Some(b'4'),
        KeyCode::Digit5 => Some(b'5'),
        KeyCode::Digit6 => Some(b'6'),
        KeyCode::Digit7 => Some(b'7'),
        KeyCode::Digit8 => Some(b'8'),
        KeyCode::Digit9 => Some(b'9'),
        KeyCode::Space => Some(b' '),
        KeyCode::Enter => Some(b'\r'),
        KeyCode::Backspace => Some(0x08),
        KeyCode::Escape => None,
    }
}

fn decode_set2_key(byte: u8) -> Option<KeyCode> {
    match byte {
        0x1C => Some(KeyCode::A),
        0x32 => Some(KeyCode::B),
        0x21 => Some(KeyCode::C),
        0x23 => Some(KeyCode::D),
        0x24 => Some(KeyCode::E),
        0x2B => Some(KeyCode::F),
        0x34 => Some(KeyCode::G),
        0x33 => Some(KeyCode::H),
        0x43 => Some(KeyCode::I),
        0x3B => Some(KeyCode::J),
        0x42 => Some(KeyCode::K),
        0x4B => Some(KeyCode::L),
        0x3A => Some(KeyCode::M),
        0x31 => Some(KeyCode::N),
        0x44 => Some(KeyCode::O),
        0x4D => Some(KeyCode::P),
        0x15 => Some(KeyCode::Q),
        0x2D => Some(KeyCode::R),
        0x1B => Some(KeyCode::S),
        0x2C => Some(KeyCode::T),
        0x3C => Some(KeyCode::U),
        0x2A => Some(KeyCode::V),
        0x1D => Some(KeyCode::W),
        0x22 => Some(KeyCode::X),
        0x35 => Some(KeyCode::Y),
        0x1A => Some(KeyCode::Z),
        0x45 => Some(KeyCode::Digit0),
        0x16 => Some(KeyCode::Digit1),
        0x1E => Some(KeyCode::Digit2),
        0x26 => Some(KeyCode::Digit3),
        0x25 => Some(KeyCode::Digit4),
        0x2E => Some(KeyCode::Digit5),
        0x36 => Some(KeyCode::Digit6),
        0x3D => Some(KeyCode::Digit7),
        0x3E => Some(KeyCode::Digit8),
        0x46 => Some(KeyCode::Digit9),
        0x5A => Some(KeyCode::Enter),
        0x29 => Some(KeyCode::Space),
        0x66 => Some(KeyCode::Backspace),
        0x76 => Some(KeyCode::Escape),
        _ => None,
    }
}

#[cfg(all(not(test), feature = "physical-keyboard-console"))]
struct KeyboardConsoleDecoderCell(UnsafeCell<KeyboardConsoleDecoder>);

#[cfg(all(not(test), feature = "physical-keyboard-console"))]
// SAFETY:
// 1. Invariant: the decoder is mutated only from the shell's synchronous
//    console-read syscall path on the single boot CPU.
// 2. Established by: ADR 0051 normal boot runs one persistent shell process.
// 3. Lifetime: static decoder state is retained for the boot session.
// 4. Pointer ownership: this module exclusively owns the decoder.
// 5. Alignment: `UnsafeCell<KeyboardConsoleDecoder>` preserves alignment.
// 6. Mapped length: exactly one decoder value is accessed.
// 7. Concurrency: console reads are serviced one syscall at a time.
// 8. Violation: concurrent mutation could corrupt release-prefix tracking.
unsafe impl Sync for KeyboardConsoleDecoderCell {}

#[cfg(all(not(test), feature = "physical-keyboard-console"))]
static KEYBOARD_CONSOLE_DECODER: KeyboardConsoleDecoderCell =
    KeyboardConsoleDecoderCell(UnsafeCell::new(KeyboardConsoleDecoder::new()));

#[cfg(all(not(test), feature = "physical-keyboard-console"))]
pub(crate) fn mark_ready() {
    serial::write_line("PYTHOS:CORE:PHYSICAL_KEYBOARD_CONSOLE:READY");
}

#[cfg(all(not(test), feature = "physical-keyboard-console"))]
pub(crate) fn mark_ps2_init_failed() {
    serial::write_line("PYTHOS:CORE:PHYSICAL_KEYBOARD_CONSOLE:PS2_INIT_FAILED");
}

#[cfg(all(not(test), feature = "physical-keyboard-console"))]
pub(crate) fn poll_console_byte() -> Option<u8> {
    let mut drained = 0;
    while drained < POLL_DRAIN_LIMIT {
        let Some(raw) = ps2::poll_raw_output_byte() else {
            return None;
        };
        drained += 1;
        // SAFETY: see `KeyboardConsoleDecoderCell`'s `Sync` safety contract.
        let decoded = unsafe { (&mut *KEYBOARD_CONSOLE_DECODER.0.get()).feed_raw_byte(raw) };
        if let Some(byte) = decoded {
            serial::write_line("PYTHOS:CORE:PHYSICAL_KEYBOARD_CONSOLE:BYTE");
            return Some(byte);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_drivers::KeyCode;

    #[test]
    fn keycodes_map_to_shell_console_bytes() {
        assert_eq!(keycode_to_console_byte(KeyCode::H), Some(b'h'));
        assert_eq!(keycode_to_console_byte(KeyCode::E), Some(b'e'));
        assert_eq!(keycode_to_console_byte(KeyCode::L), Some(b'l'));
        assert_eq!(keycode_to_console_byte(KeyCode::P), Some(b'p'));
        assert_eq!(keycode_to_console_byte(KeyCode::Space), Some(b' '));
        assert_eq!(keycode_to_console_byte(KeyCode::Enter), Some(b'\r'));
        assert_eq!(keycode_to_console_byte(KeyCode::Backspace), Some(0x08));
    }

    #[test]
    fn set1_help_enter_decodes_to_console_bytes() {
        let mut decoder = KeyboardConsoleDecoder::new();
        let bytes = [0x23, 0x12, 0x26, 0x19, 0x1C];
        let mut output = [0u8; 5];
        let mut len = 0;
        for byte in bytes {
            if let Some(console_byte) = decoder.feed_raw_byte(byte) {
                output[len] = console_byte;
                len += 1;
            }
        }

        assert_eq!(&output[..len], b"help\r");
    }

    #[test]
    fn set2_release_bytes_do_not_emit_extra_console_bytes() {
        let mut decoder = KeyboardConsoleDecoder::new();
        assert_eq!(decoder.feed_raw_byte(0x33), Some(b'h'));
        assert_eq!(decoder.feed_raw_byte(0xF0), None);
        assert_eq!(decoder.feed_raw_byte(0x33), None);
    }
}
