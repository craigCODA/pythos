use core::arch::asm;

const COM1: u16 = 0x3F8;
const LINE_STATUS: u16 = COM1 + 5;
const TRANSMIT_EMPTY: u8 = 0x20;

pub fn init_com1() {
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x80);
    outb(COM1, 0x03);
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x03);
    outb(COM1 + 2, 0xC7);
    outb(COM1 + 4, 0x0B);
}

pub fn write_line(line: &str) {
    write_str(line);
    write_str("\r\n");
}

fn write_str(value: &str) {
    for byte in value.bytes() {
        write_byte(byte);
    }
}

fn write_byte(byte: u8) {
    while (inb(LINE_STATUS) & TRANSMIT_EMPTY) == 0 {
        core::hint::spin_loop();
    }
    outb(COM1, byte);
}

fn outb(port: u16, value: u8) {
    // SAFETY:
    // 1. Invariant: `port` names an x86 I/O port and `value` is the byte to write.
    // 2. Established by: the caller only passes COM1 UART port constants.
    // 3. Lifetime: valid for this single `out` instruction.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: milestone 1 is single-core firmware entry with no shared serial lock.
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
    // 2. Established by: the caller only passes the COM1 line-status port constant.
    // 3. Lifetime: valid for this single `in` instruction.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: milestone 1 is single-core firmware entry with no shared serial lock.
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
