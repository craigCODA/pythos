//! Direct COM1/COM2 serial I/O.
//!
//! The PythOS loader initializes COM1 before handoff, so PythCore only
//! transmits on it; it never reconfigures that UART. COM2 (ADR 0052) is
//! PythCore-owned from a cold, unconfigured state: normal boot initializes it
//! itself as the interactive object-shell transport, distinct from COM1's
//! verification-oracle role.
//!
//! The COM2 items below are unused under `--features verify` (only
//! `normal_boot` calls them, and that module is normal-boot-only); allow dead
//! code for that one build configuration rather than gating each item.
#![cfg_attr(feature = "verify", allow(dead_code))]

use core::arch::asm;

const COM1_BASE: u16 = 0x3F8;
const COM2_BASE: u16 = 0x2F8;
const LINE_STATUS_OFFSET: u16 = 5;
const RECEIVE_READY: u8 = 0x01;
const TRANSMIT_EMPTY: u8 = 0x20;

pub const fn line_status_port(base: u16) -> u16 {
    base + LINE_STATUS_OFFSET
}

/// One `(port, value)` write in a UART initialization sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UartWrite {
    port: u16,
    value: u8,
}

impl UartWrite {
    pub const fn new(port: u16, value: u8) -> Self {
        Self { port, value }
    }
}

/// The standard 16550-compatible init sequence for a UART at `base`: disable
/// interrupts, set DLAB and the 115200-baud divisor (1), restore DLAB, set
/// 8N1 line control, enable and clear the FIFOs, and assert
/// DTR/RTS/OUT2 (required for QEMU's serial backend to deliver input).
pub const fn uart_init_sequence(base: u16) -> [UartWrite; 7] {
    [
        UartWrite::new(base + 1, 0x00),
        UartWrite::new(base + 3, 0x80),
        UartWrite::new(base, 0x01),
        UartWrite::new(base + 1, 0x00),
        UartWrite::new(base + 3, 0x03),
        UartWrite::new(base + 2, 0xC7),
        UartWrite::new(base + 4, 0x0B),
    ]
}

/// Initialize COM2 from a cold, firmware-untouched state. Unlike COM1 (which
/// the loader already configured), PythCore owns COM2's full setup.
#[cfg(not(test))]
pub fn init_com2() {
    for write in uart_init_sequence(COM2_BASE) {
        outb(write.port, write.value);
    }
}

/// Write one byte to COM2, blocking until the transmit holding register is empty.
#[cfg(not(test))]
pub fn write_byte_com2(byte: u8) {
    while (inb(line_status_port(COM2_BASE)) & TRANSMIT_EMPTY) == 0 {
        core::hint::spin_loop();
    }
    outb(COM2_BASE, byte);
}

/// Non-blocking COM2 byte read: `None` if no byte is waiting.
#[cfg(not(test))]
pub fn try_read_byte_com2() -> Option<u8> {
    if (inb(line_status_port(COM2_BASE)) & RECEIVE_READY) == 0 {
        return None;
    }
    Some(inb(COM2_BASE))
}

pub fn write_line(line: &str) {
    write_str(line);
    write_str("\r\n");
}

pub fn write_hex_u64(label: &str, value: u64) {
    write_str(label);
    write_str("0x");
    for shift in (0..64).step_by(4).rev() {
        let digit = ((value >> shift) & 0xF) as u8;
        let byte = if digit < 10 {
            b'0' + digit
        } else {
            b'A' + (digit - 10)
        };
        write_byte(byte);
    }
    write_str("\r\n");
}

pub fn write_str(value: &str) {
    for byte in value.bytes() {
        write_byte(byte);
    }
}

fn write_byte(byte: u8) {
    while (inb(line_status_port(COM1_BASE)) & TRANSMIT_EMPTY) == 0 {
        core::hint::spin_loop();
    }
    outb(COM1_BASE, byte);
}

fn outb(port: u16, value: u8) {
    // SAFETY:
    // 1. Invariant: `port` names an x86 I/O port and `value` is the byte to write.
    // 2. Established by: callers only pass COM1/COM2 UART port constants; the
    //    loader initialized COM1 before handoff, and `init_com2` configures
    //    COM2 itself before any other COM2 port write.
    // 3. Lifetime: valid for this single `out` instruction.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: milestone 1 is single-core entry with no shared serial lock.
    // 8. Violation: writing an incorrect port can reconfigure unrelated hardware.
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY:
    // 1. Invariant: `port` names an x86 I/O port readable as one byte.
    // 2. Established by: callers only pass COM1/COM2 UART port constants.
    // 3. Lifetime: valid for this single `in` instruction.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: milestone 1 is single-core entry with no shared serial lock.
    // 8. Violation: reading an incorrect port can observe unrelated hardware state.
    unsafe {
        asm!(
            "in al, dx",
            out("al") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn com2_init_sequence_targets_legacy_base() {
        let sequence = uart_init_sequence(COM2_BASE);
        assert_eq!(COM2_BASE, 0x2F8);
        assert_eq!(line_status_port(COM2_BASE), 0x2FD);
        assert_eq!(sequence[0], UartWrite::new(COM2_BASE + 1, 0x00));
        assert_eq!(sequence[1], UartWrite::new(COM2_BASE + 3, 0x80));
        assert_eq!(sequence[2], UartWrite::new(COM2_BASE, 0x01));
        assert_eq!(sequence[3], UartWrite::new(COM2_BASE + 1, 0x00));
        assert_eq!(sequence[4], UartWrite::new(COM2_BASE + 3, 0x03));
        assert_eq!(sequence[5], UartWrite::new(COM2_BASE + 2, 0xC7));
        assert_eq!(sequence[6], UartWrite::new(COM2_BASE + 4, 0x0B));
    }

    #[test]
    fn com1_and_com2_line_status_ports_are_distinct() {
        assert_eq!(line_status_port(COM1_BASE), COM1_BASE + 5);
        assert_ne!(line_status_port(COM1_BASE), line_status_port(COM2_BASE));
    }
}
