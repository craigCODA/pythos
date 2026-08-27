//! Opt-in physical wake diagnostic for serial-less hardware input bring-up.
//!
//! This module is deliberately narrow: it recognizes the exact `wake` + Enter
//! sequence from raw keyboard bytes and reports enough state for a framebuffer
//! diagnostic panel. It is not a general keyboard layout, input service, or
//! physical HID claim.

#[cfg(feature = "physical-wake-diagnostic")]
use crate::{
    framebuffer::{self, PhysicalWakeStatus},
    ps2, qemu_exit, serial,
};
#[cfg(feature = "physical-wake-diagnostic")]
use pythos_shared::boot_protocol::PythFramebufferInfo;

const WAKE_WORD: &[u8; 4] = b"wake";
const INPUT_CAPACITY: usize = WAKE_WORD.len();
const RAW_LOG_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WakeResult {
    Waiting,
    Rejected,
    Accepted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScanSetMode {
    Unknown,
    Set1,
    Set2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WakeKey {
    Letter(u8),
    Enter,
    Backspace,
}

pub(crate) struct WakeInput {
    typed: [u8; INPUT_CAPACITY],
    typed_len: usize,
    raw: [u8; RAW_LOG_CAPACITY],
    raw_len: usize,
    mode: ScanSetMode,
    release_prefix: bool,
    extended_prefix: bool,
    last_result: WakeResult,
}

impl WakeInput {
    pub(crate) const fn new() -> Self {
        Self {
            typed: [0; INPUT_CAPACITY],
            typed_len: 0,
            raw: [0; RAW_LOG_CAPACITY],
            raw_len: 0,
            mode: ScanSetMode::Unknown,
            release_prefix: false,
            extended_prefix: false,
            last_result: WakeResult::Waiting,
        }
    }

    pub(crate) fn feed_raw_byte(&mut self, byte: u8) -> WakeResult {
        self.push_raw(byte);
        if self.last_result == WakeResult::Accepted {
            return WakeResult::Accepted;
        }
        if self.consume_non_make_byte(byte) {
            self.last_result = WakeResult::Waiting;
            return WakeResult::Waiting;
        }
        let Some(key) = self.decode_make_byte(byte) else {
            self.last_result = WakeResult::Waiting;
            return WakeResult::Waiting;
        };
        self.last_result = self.apply_key(key);
        self.last_result
    }

    pub(crate) fn input_bytes(&self) -> &[u8] {
        &self.typed[..self.typed_len]
    }

    pub(crate) fn raw_bytes(&self) -> &[u8] {
        &self.raw[..self.raw_len]
    }

    #[cfg(test)]
    pub(crate) fn last_result(&self) -> WakeResult {
        self.last_result
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

    fn decode_make_byte(&mut self, byte: u8) -> Option<WakeKey> {
        match self.mode {
            ScanSetMode::Unknown => match byte {
                0x11 => {
                    self.mode = ScanSetMode::Set1;
                    Some(WakeKey::Letter(b'w'))
                }
                0x1D => {
                    self.mode = ScanSetMode::Set2;
                    Some(WakeKey::Letter(b'w'))
                }
                _ => None,
            },
            ScanSetMode::Set1 => decode_set1_key(byte),
            ScanSetMode::Set2 => decode_set2_key(byte),
        }
    }

    fn apply_key(&mut self, key: WakeKey) -> WakeResult {
        match key {
            WakeKey::Letter(letter) => {
                if self.typed_len >= INPUT_CAPACITY {
                    self.reset_attempt();
                    return WakeResult::Rejected;
                }
                self.typed[self.typed_len] = letter;
                self.typed_len += 1;
                WakeResult::Waiting
            }
            WakeKey::Backspace => {
                if self.typed_len > 0 {
                    self.typed_len -= 1;
                }
                if self.typed_len == 0 {
                    self.mode = ScanSetMode::Unknown;
                }
                WakeResult::Waiting
            }
            WakeKey::Enter => {
                if self.input_bytes() == WAKE_WORD {
                    WakeResult::Accepted
                } else {
                    self.reset_attempt();
                    WakeResult::Rejected
                }
            }
        }
    }

    fn reset_attempt(&mut self) {
        self.typed = [0; INPUT_CAPACITY];
        self.typed_len = 0;
        self.mode = ScanSetMode::Unknown;
        self.release_prefix = false;
        self.extended_prefix = false;
    }
}

fn decode_set1_key(byte: u8) -> Option<WakeKey> {
    match byte {
        0x11 => Some(WakeKey::Letter(b'w')),
        0x1E => Some(WakeKey::Letter(b'a')),
        0x25 => Some(WakeKey::Letter(b'k')),
        0x12 => Some(WakeKey::Letter(b'e')),
        0x1C => Some(WakeKey::Enter),
        0x0E => Some(WakeKey::Backspace),
        _ => None,
    }
}

fn decode_set2_key(byte: u8) -> Option<WakeKey> {
    match byte {
        0x1D => Some(WakeKey::Letter(b'w')),
        0x1C => Some(WakeKey::Letter(b'a')),
        0x42 => Some(WakeKey::Letter(b'k')),
        0x24 => Some(WakeKey::Letter(b'e')),
        0x5A => Some(WakeKey::Enter),
        0x66 => Some(WakeKey::Backspace),
        _ => None,
    }
}

#[cfg(feature = "physical-wake-diagnostic")]
pub(crate) fn run(framebuffer: &PythFramebufferInfo) {
    serial::write_line("PYTHOS:CORE:PHYSICAL_WAKE:ENTER");
    let mut input = WakeInput::new();
    if ps2::initialize_keyboard_polling().is_err() {
        render_or_panic(framebuffer, &input, PhysicalWakeStatus::Ps2InitFailed);
        serial::write_line("PYTHOS:CORE:PHYSICAL_WAKE:PS2_INIT_FAILED");
        loop {
            core::hint::spin_loop();
        }
    }

    render_or_panic(framebuffer, &input, PhysicalWakeStatus::Ready);
    serial::write_line("PYTHOS:CORE:PHYSICAL_WAKE:READY");
    loop {
        let Some(byte) = ps2::poll_raw_output_byte() else {
            core::hint::spin_loop();
            continue;
        };
        match input.feed_raw_byte(byte) {
            WakeResult::Waiting => {
                render_or_panic(framebuffer, &input, PhysicalWakeStatus::Ready);
            }
            WakeResult::Rejected => {
                render_or_panic(framebuffer, &input, PhysicalWakeStatus::Rejected);
                serial::write_line("PYTHOS:CORE:PHYSICAL_WAKE:REJECTED");
            }
            WakeResult::Accepted => {
                render_or_panic(framebuffer, &input, PhysicalWakeStatus::Accepted);
                serial::write_line("PYTHOS:CORE:PHYSICAL_WAKE:ACCEPTED");
                return;
            }
        }
    }
}

#[cfg(feature = "physical-wake-diagnostic")]
fn render_or_panic(
    framebuffer: &PythFramebufferInfo,
    input: &WakeInput,
    status: PhysicalWakeStatus,
) {
    if framebuffer::render_physical_wake_diagnostic(
        framebuffer,
        input.input_bytes(),
        input.raw_bytes(),
        status,
    )
    .is_err()
    {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set1_wake_enter_accepts() {
        let mut input = WakeInput::new();
        let bytes = [0x11, 0x1E, 0x25, 0x12, 0x1C];
        for byte in &bytes[..bytes.len() - 1] {
            assert_eq!(input.feed_raw_byte(*byte), WakeResult::Waiting);
        }

        assert_eq!(input.feed_raw_byte(bytes[bytes.len() - 1]), WakeResult::Accepted);
        assert_eq!(input.input_bytes(), WAKE_WORD);
        assert_eq!(input.last_result(), WakeResult::Accepted);
    }

    #[test]
    fn set2_wake_enter_accepts() {
        let mut input = WakeInput::new();
        let bytes = [0x1D, 0x1C, 0x42, 0x24, 0x5A];
        for byte in &bytes[..bytes.len() - 1] {
            assert_eq!(input.feed_raw_byte(*byte), WakeResult::Waiting);
        }

        assert_eq!(input.feed_raw_byte(bytes[bytes.len() - 1]), WakeResult::Accepted);
        assert_eq!(input.input_bytes(), WAKE_WORD);
    }

    #[test]
    fn set2_release_bytes_are_ignored() {
        let mut input = WakeInput::new();
        let bytes = [0x1D, 0xF0, 0x1D, 0x1C, 0x42, 0x24, 0x5A];
        for byte in &bytes[..bytes.len() - 1] {
            assert_eq!(input.feed_raw_byte(*byte), WakeResult::Waiting);
        }

        assert_eq!(input.feed_raw_byte(bytes[bytes.len() - 1]), WakeResult::Accepted);
        assert_eq!(input.input_bytes(), WAKE_WORD);
    }

    #[test]
    fn rejected_attempt_resets_for_next_wake() {
        let mut input = WakeInput::new();
        for byte in [0x11, 0x1E, 0x12] {
            assert_eq!(input.feed_raw_byte(byte), WakeResult::Waiting);
        }
        assert_eq!(input.feed_raw_byte(0x1C), WakeResult::Rejected);
        assert_eq!(input.input_bytes(), b"");

        for byte in [0x11, 0x1E, 0x25, 0x12] {
            assert_eq!(input.feed_raw_byte(byte), WakeResult::Waiting);
        }
        assert_eq!(input.feed_raw_byte(0x1C), WakeResult::Accepted);
    }

    #[test]
    fn raw_log_keeps_latest_bytes() {
        let mut input = WakeInput::new();
        for byte in 0u8..20 {
            let _ = input.feed_raw_byte(byte);
        }

        assert_eq!(input.raw_bytes(), &[4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19]);
    }
}
