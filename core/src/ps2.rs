//! Real PS/2 keyboard + mouse controller driver (ADR 0053).
//!
//! Scope is QEMU's emulated legacy PS/2 controller only — see ADR 0053 and
//! ADR 0049's precedent of deferring real-hardware (USB HID) input. This is
//! the first module in the tree that performs real input-device port I/O;
//! `input_drivers.rs`/`input_events.rs` are decode-only proofs fed
//! already-supplied bytes, with no hardware access of their own.
// Only `handle_keyboard_interrupt`/`handle_mouse_interrupt` are called from
// `normal_boot.rs`'s only caller, `interrupts.rs` (unconditionally, in both
// boot paths). `initialize`/`poll_event` and everything else are wired in by
// `normal_boot.rs` itself (ADR 0053, Task D) and `normal_boot` is compiled
// out entirely under `--features verify` (see `main.rs`'s `mod normal_boot`
// gate), so the verify build sees most of this module as unused; the host
// `cargo test` build never calls the hardware-facing functions at all, only
// the pure decode/state-machine logic exercised by this file's own tests.
#![cfg_attr(any(test, feature = "verify"), allow(dead_code))]

use core::arch::asm;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::architecture::x86_64::interrupts;
use crate::input_drivers::{RawInputEvent, mouse_byte0_is_valid, scancode_to_keycode};

const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_COMMAND_PORT: u16 = 0x64;

const STATUS_OUTPUT_FULL: u8 = 1 << 0;
const STATUS_INPUT_FULL: u8 = 1 << 1;

const CMD_READ_CONFIG: u8 = 0x20;
const CMD_WRITE_CONFIG: u8 = 0x60;
const CMD_DISABLE_PORT2: u8 = 0xA7;
const CMD_ENABLE_PORT2: u8 = 0xA8;
const CMD_TEST_CONTROLLER: u8 = 0xAA;
const CMD_DISABLE_PORT1: u8 = 0xAD;
const CMD_ENABLE_PORT1: u8 = 0xAE;
const CMD_WRITE_PORT2_INPUT: u8 = 0xD4;

const CONTROLLER_SELF_TEST_PASS: u8 = 0x55;
const MOUSE_ENABLE_DATA_REPORTING: u8 = 0xF4;
const MOUSE_ACK: u8 = 0xFA;

const CONFIG_PORT1_IRQ_ENABLE: u8 = 1 << 0;
const CONFIG_PORT2_IRQ_ENABLE: u8 = 1 << 1;
const CONFIG_PORT1_TRANSLATION: u8 = 1 << 6;

/// Bound on port-status polling loops. QEMU's emulated controller responds
/// essentially immediately; this only guards against a genuinely absent or
/// wedged controller (e.g. a QEMU profile without PS/2 emulated) turning into
/// an infinite hang.
const IO_WAIT_LIMIT: usize = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ps2Error {
    OutputTimeout,
    InputTimeout,
    ControllerSelfTestFailed,
    MouseAckTimeout,
    Irq,
}

/// Bring up the PS/2 controller from a cold, firmware-untouched state:
/// disable both ports during setup, flush any stale output byte, enable
/// IRQ1/IRQ12 reporting and disable scancode translation in the controller
/// configuration byte, run the controller self-test, re-enable both ports,
/// and enable the mouse's data-reporting stream. Unmasks IRQ1/IRQ12 in the
/// PIC only on full success, mirroring `timer::initialize`'s
/// program-then-unmask pattern.
pub fn initialize() -> Result<(), Ps2Error> {
    write_command(CMD_DISABLE_PORT1)?;
    write_command(CMD_DISABLE_PORT2)?;
    flush_output_buffer();

    write_command(CMD_READ_CONFIG)?;
    let mut config = read_data_blocking()?;
    config |= CONFIG_PORT1_IRQ_ENABLE | CONFIG_PORT2_IRQ_ENABLE;
    config &= !CONFIG_PORT1_TRANSLATION;
    write_command(CMD_WRITE_CONFIG)?;
    write_data_blocking(config)?;

    write_command(CMD_TEST_CONTROLLER)?;
    if read_data_blocking()? != CONTROLLER_SELF_TEST_PASS {
        return Err(Ps2Error::ControllerSelfTestFailed);
    }

    write_command(CMD_ENABLE_PORT1)?;
    write_command(CMD_ENABLE_PORT2)?;

    write_command(CMD_WRITE_PORT2_INPUT)?;
    write_data_blocking(MOUSE_ENABLE_DATA_REPORTING)?;
    if read_data_blocking()? != MOUSE_ACK {
        return Err(Ps2Error::MouseAckTimeout);
    }
    // Defensive: discard anything already sitting in the output buffer
    // before enabling IRQs, in case any stray byte is present at this point.
    // Verified live against QEMU that this alone does not guarantee a clean
    // start — one extra byte (matching the ack just read above) was still
    // observed arriving via the *first* post-unmask interrupt in testing.
    // That's tolerated rather than chased further: `MouseAssembler::feed`
    // only re-validates 3-byte alignment while in its `Byte0` state, so one
    // stray leading byte costs at most one dropped/misread packet before it
    // naturally resynchronizes (confirmed live: the very next byte that
    // fails the byte-0 "always 1" bit check is dropped, realigning the
    // stream within a few bytes).
    flush_output_buffer();

    interrupts::unmask_irq(1).map_err(|_| Ps2Error::Irq)?;
    // IRQ12 lives on the slave PIC; the slave's interrupts only ever reach
    // the CPU through the master PIC's cascade line, IRQ2. Without also
    // unmasking IRQ2 here, the mouse's IRQ12 would never fire even though
    // `unmask_irq(12)` itself succeeds - a genuinely easy-to-miss PIC wiring
    // detail. Found via live QMP-injected mouse input against QEMU during
    // Task D: injected motion/click events had zero effect until this line
    // was added.
    interrupts::unmask_irq(2).map_err(|_| Ps2Error::Irq)?;
    interrupts::unmask_irq(12).map_err(|_| Ps2Error::Irq)?;
    Ok(())
}

/// IRQ1 top half: read the pending scancode from the data port and, if it
/// decodes to a known key, enqueue a `KeyPressed` event. Unknown scancodes
/// (e.g. key-release bytes, which set the high bit in scancode set 1 — this
/// driver only tracks key-down) are silently dropped rather than queued.
#[cfg(not(test))]
pub fn handle_keyboard_interrupt() {
    let scancode = inb(PS2_DATA_PORT);
    if let Some(key) = scancode_to_keycode(scancode) {
        QUEUE.push(RawInputEvent::KeyPressed { scancode, key });
    }
}

/// IRQ12 top half: read the pending byte from the data port and feed it into
/// the 3-byte mouse packet assembler.
#[cfg(not(test))]
pub fn handle_mouse_interrupt() {
    let byte = inb(PS2_DATA_PORT);
    MOUSE_ASSEMBLER.feed(byte);
}

/// Drain one queued input event, if any. Called from normal (non-interrupt)
/// context — `launcher_screen.rs`'s poll loop (ADR 0053, Task D).
#[cfg_attr(not(test), allow(dead_code))]
pub fn poll_event() -> Option<RawInputEvent> {
    QUEUE.pop()
}

const QUEUE_CAPACITY: usize = 16;

/// Single-producer (the keyboard/mouse IRQ handlers, which run serialized on
/// this single core and never re-enter each other or themselves), single-
/// consumer (`poll_event`, called from normal kernel-mode context) ring
/// buffer. Interrupt-context handlers must stay allocation-free and fast;
/// this is a fixed-size array with atomic head/tail indices, no heap use.
struct EventQueue {
    slots: UnsafeCell<[Option<RawInputEvent>; QUEUE_CAPACITY]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

// SAFETY:
// 1. Invariant: `slots` is mutated only by `push` (IRQ context, one core, no
//    handler nesting) and `pop` (normal context); the atomic head/tail pair
//    ensures a push and a concurrent pop never touch the same slot index at
//    the same time.
// 2. Established by: ADR 0051's single-core constraint and the PIC's serial
//    interrupt delivery on this platform.
// 3. Lifetime: `QUEUE` is a static with `'static` lifetime.
// 4. Pointer ownership: this module exclusively owns the queue.
// 5. Alignment: `UnsafeCell<[Option<RawInputEvent>; N]>` preserves the
//    array's alignment.
// 6. Mapped length: exactly `QUEUE_CAPACITY` slots are ever indexed, each
//    index taken modulo that capacity.
// 7. Concurrency: single boot CPU; the producer only runs inside an
//    interrupt handler, which cannot itself be interrupted by another
//    instance of itself.
// 8. Violation: concurrent unsynchronized access could tear an `Option`
//    write/read, corrupting queued events.
unsafe impl Sync for EventQueue {}

static QUEUE: EventQueue = EventQueue {
    slots: UnsafeCell::new([None; QUEUE_CAPACITY]),
    head: AtomicUsize::new(0),
    tail: AtomicUsize::new(0),
};

impl EventQueue {
    fn push(&self, event: RawInputEvent) {
        let tail = self.tail.load(Ordering::Relaxed);
        let next = (tail + 1) % QUEUE_CAPACITY;
        if next == self.head.load(Ordering::Acquire) {
            // Full: drop the event rather than overwrite an unread slot or
            // block interrupt context.
            return;
        }
        // SAFETY: see the `EventQueue`/`unsafe impl Sync` block above -
        // `tail` is owned exclusively by the single producer at this point.
        unsafe {
            (*self.slots.get())[tail] = Some(event);
        }
        self.tail.store(next, Ordering::Release);
    }

    fn pop(&self) -> Option<RawInputEvent> {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) {
            return None;
        }
        // SAFETY: see the `EventQueue`/`unsafe impl Sync` block above -
        // `head` is owned exclusively by the single consumer at this point.
        let event = unsafe { (*self.slots.get())[head].take() };
        self.head
            .store((head + 1) % QUEUE_CAPACITY, Ordering::Release);
        event
    }
}

#[derive(Clone, Copy)]
enum MouseStage {
    Byte0,
    Byte1(u8),
    Byte2(u8, u8),
}

struct MouseAssemblerState {
    stage: MouseStage,
    left_down: bool,
}

struct MouseAssembler(UnsafeCell<MouseAssemblerState>);

// SAFETY:
// 1. Invariant: the assembler state is mutated only from `feed`, called
//    exclusively from the mouse IRQ12 top half.
// 2. Established by: ADR 0051's single-core constraint; the PIC delivers one
//    interrupt at a time and this handler does not re-enter itself.
// 3. Lifetime: `MOUSE_ASSEMBLER` is a static with `'static` lifetime.
// 4. Pointer ownership: this module exclusively owns the assembler state.
// 5. Alignment: `UnsafeCell<MouseAssemblerState>` preserves the struct's
//    alignment.
// 6. Mapped length: exactly one `MouseAssemblerState` is accessed.
// 7. Concurrency: single boot CPU, one producer, never concurrently entered.
// 8. Violation: concurrent access could interleave partially-assembled mouse
//    packets from different byte streams.
unsafe impl Sync for MouseAssembler {}

static MOUSE_ASSEMBLER: MouseAssembler = MouseAssembler(UnsafeCell::new(MouseAssemblerState {
    stage: MouseStage::Byte0,
    left_down: false,
}));

impl MouseAssembler {
    /// Feed one byte of a 3-byte PS/2 mouse packet. On completing a packet,
    /// pushes a `MouseButton` event if the left-button state changed since
    /// the previous packet, then a `MouseMoved` event if the packet reports
    /// nonzero motion. `dx`/`dy` are truncated to `i8` (matching
    /// `MouseDriver::decode`'s existing interpretation) rather than
    /// widened to the full signed 9-bit range the sign/overflow bits in
    /// byte 0 would allow — an accepted simplification for a slow,
    /// deliberate menu click, not a silent gap.
    fn feed(&self, byte: u8) {
        // SAFETY: see the `MouseAssembler`/`unsafe impl Sync` block above.
        let state = unsafe { &mut *self.0.get() };
        match state.stage {
            MouseStage::Byte0 => {
                if !mouse_byte0_is_valid(byte) {
                    // Not a valid packet-start byte; drop it and stay
                    // resynchronized against the next byte rather than
                    // assembling a bogus packet from a misaligned stream.
                    return;
                }
                state.stage = MouseStage::Byte1(byte);
            }
            MouseStage::Byte1(byte0) => {
                state.stage = MouseStage::Byte2(byte0, byte);
            }
            MouseStage::Byte2(byte0, byte1) => {
                state.stage = MouseStage::Byte0;
                let left_down = byte0 & 0x01 != 0;
                if left_down != state.left_down {
                    state.left_down = left_down;
                    QUEUE.push(RawInputEvent::MouseButton { left: left_down });
                }
                let dx = byte1 as i8;
                let dy = byte as i8;
                if dx != 0 || dy != 0 {
                    QUEUE.push(RawInputEvent::MouseMoved { dx, dy });
                }
            }
        }
    }
}

fn write_command(command: u8) -> Result<(), Ps2Error> {
    wait_input_ready()?;
    outb(PS2_STATUS_COMMAND_PORT, command);
    Ok(())
}

fn write_data_blocking(value: u8) -> Result<(), Ps2Error> {
    wait_input_ready()?;
    outb(PS2_DATA_PORT, value);
    Ok(())
}

fn read_data_blocking() -> Result<u8, Ps2Error> {
    let mut guard = 0;
    while inb(PS2_STATUS_COMMAND_PORT) & STATUS_OUTPUT_FULL == 0 {
        guard += 1;
        if guard >= IO_WAIT_LIMIT {
            return Err(Ps2Error::OutputTimeout);
        }
        core::hint::spin_loop();
    }
    Ok(inb(PS2_DATA_PORT))
}

fn wait_input_ready() -> Result<(), Ps2Error> {
    let mut guard = 0;
    while inb(PS2_STATUS_COMMAND_PORT) & STATUS_INPUT_FULL != 0 {
        guard += 1;
        if guard >= IO_WAIT_LIMIT {
            return Err(Ps2Error::InputTimeout);
        }
        core::hint::spin_loop();
    }
    Ok(())
}

fn flush_output_buffer() {
    let mut guard = 0;
    while inb(PS2_STATUS_COMMAND_PORT) & STATUS_OUTPUT_FULL != 0 && guard < IO_WAIT_LIMIT {
        let _ = inb(PS2_DATA_PORT);
        guard += 1;
    }
}

fn outb(port: u16, value: u8) {
    // SAFETY:
    // 1. Invariant: `port` names a legacy PS/2 controller data or
    //    status/command port.
    // 2. Established by: this private helper is only called with the two
    //    module constants above.
    // 3. Lifetime: each write completes before the helper returns.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: single boot CPU; PS/2 setup and IRQ handling never
    //    overlap with themselves.
    // 8. Violation: writing the wrong port could reconfigure unrelated
    //    hardware.
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
    // 1. Invariant: `port` names a legacy PS/2 controller data or
    //    status/command port.
    // 2. Established by: this private helper is only called with the two
    //    module constants above.
    // 3. Lifetime: the read completes before the helper returns.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: single boot CPU; PS/2 setup and IRQ handling never
    //    overlap with themselves.
    // 8. Violation: reading the wrong port could observe unrelated hardware
    //    state.
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_assembler_emits_move_for_a_complete_packet() {
        // Drain anything a prior test in this process might have left
        // queued, and reset the shared assembler state - `MOUSE_ASSEMBLER`
        // and `QUEUE` are statics shared across the whole test binary, so
        // tests in this module cannot assume a fresh `left_down: false`.
        while QUEUE.pop().is_some() {}
        // SAFETY (test-only): see the other tests in this module for the
        // same reset pattern.
        unsafe {
            *MOUSE_ASSEMBLER.0.get() = MouseAssemblerState {
                stage: MouseStage::Byte0,
                left_down: false,
            };
        }

        MOUSE_ASSEMBLER.feed(0x08); // byte 0: valid, no buttons
        MOUSE_ASSEMBLER.feed(5); // dx
        MOUSE_ASSEMBLER.feed((-3i8) as u8); // dy

        assert_eq!(
            QUEUE.pop(),
            Some(RawInputEvent::MouseMoved { dx: 5, dy: -3 })
        );
        assert_eq!(QUEUE.pop(), None);
    }

    #[test]
    fn mouse_assembler_emits_button_transition_on_change() {
        while QUEUE.pop().is_some() {}
        // SAFETY (test-only): reset shared static state so this test is
        // independent of ordering against other tests in the same binary.
        unsafe {
            *MOUSE_ASSEMBLER.0.get() = MouseAssemblerState {
                stage: MouseStage::Byte0,
                left_down: false,
            };
        }

        MOUSE_ASSEMBLER.feed(0x09); // byte 0: valid, left button down
        MOUSE_ASSEMBLER.feed(0);
        MOUSE_ASSEMBLER.feed(0);

        assert_eq!(QUEUE.pop(), Some(RawInputEvent::MouseButton { left: true }));
        assert_eq!(QUEUE.pop(), None);
    }

    #[test]
    fn mouse_assembler_resynchronizes_on_invalid_byte0() {
        while QUEUE.pop().is_some() {}
        unsafe {
            *MOUSE_ASSEMBLER.0.get() = MouseAssemblerState {
                stage: MouseStage::Byte0,
                left_down: false,
            };
        }

        // Byte 0 without bit 3 set is invalid and must be dropped, not
        // treated as the start of a packet.
        MOUSE_ASSEMBLER.feed(0x00);
        MOUSE_ASSEMBLER.feed(0x08); // now a real, valid byte 0
        MOUSE_ASSEMBLER.feed(2);
        MOUSE_ASSEMBLER.feed(0);

        assert_eq!(
            QUEUE.pop(),
            Some(RawInputEvent::MouseMoved { dx: 2, dy: 0 })
        );
    }

    #[test]
    fn event_queue_drops_events_when_full_without_panicking() {
        let queue = EventQueue {
            slots: UnsafeCell::new([None; QUEUE_CAPACITY]),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        };
        for _ in 0..QUEUE_CAPACITY + 4 {
            queue.push(RawInputEvent::MouseMoved { dx: 1, dy: 1 });
        }
        let mut popped = 0;
        while queue.pop().is_some() {
            popped += 1;
        }
        assert_eq!(popped, QUEUE_CAPACITY - 1);
    }

    #[test]
    fn scancode_table_decodes_letters_digits_and_control_keys() {
        use crate::input_drivers::KeyCode;
        assert_eq!(scancode_to_keycode(0x1E), Some(KeyCode::A));
        assert_eq!(scancode_to_keycode(0x0B), Some(KeyCode::Digit0));
        assert_eq!(scancode_to_keycode(0x1C), Some(KeyCode::Enter));
        assert_eq!(scancode_to_keycode(0x39), Some(KeyCode::Space));
        assert_eq!(scancode_to_keycode(0x0E), Some(KeyCode::Backspace));
        assert_eq!(scancode_to_keycode(0x01), Some(KeyCode::Escape));
        assert_eq!(scancode_to_keycode(0xFF), None);
    }
}
