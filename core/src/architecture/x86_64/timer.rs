//! PIT-backed monotonic tick source bootstrap for Phase 2.
#![cfg_attr(test, allow(dead_code))]

use crate::architecture::x86_64::interrupts;
use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

const PIT_CHANNEL0: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;
const PIT_INPUT_HZ: u32 = 1_193_182;
pub const TIMER_HZ: u32 = 100;
const PIT_DIVISOR: u16 = (PIT_INPUT_HZ / TIMER_HZ) as u16;
const PIT_COMMAND_RATE_GENERATOR: u8 = 0x34;
const FIRST_TICK_SPIN_LIMIT: usize = 20_000_000;

static TICKS: AtomicU64 = AtomicU64::new(0);

pub fn initialize() -> Result<(), ()> {
    let start = ticks();
    program_pit(PIT_DIVISOR);
    interrupts::unmask_irq(0)?;
    enable_interrupts();
    wait_for_next_tick(start)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn ticks() -> u64 {
    TICKS.load(Ordering::SeqCst)
}

pub fn handle_timer_interrupt() {
    TICKS.fetch_add(1, Ordering::SeqCst);
}

fn wait_for_next_tick(start: u64) -> Result<(), ()> {
    for _ in 0..FIRST_TICK_SPIN_LIMIT {
        if ticks() != start {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(())
}

fn program_pit(divisor: u16) {
    outb(PIT_COMMAND, PIT_COMMAND_RATE_GENERATOR);
    outb(PIT_CHANNEL0, (divisor & 0x00FF) as u8);
    outb(PIT_CHANNEL0, (divisor >> 8) as u8);
}

fn enable_interrupts() {
    // SAFETY:
    // 1. Invariant: IDT, GDT/TSS, exception entry, and PIC routing are initialized.
    // 2. Established by: caller invokes this only after the interrupt-controller slice.
    // 3. Lifetime: enabling IF persists until later code changes RFLAGS.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable.
    // 6. Mapped length: not applicable.
    // 7. Concurrency: Phase 2 remains single-core; only IRQ0 is unmasked here.
    // 8. Violation: enabling interrupts before valid handlers would fault or reset.
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags));
    }
}

fn outb(port: u16, value: u8) {
    // SAFETY:
    // 1. Invariant: `port` names PIT command/channel registers.
    // 2. Established by: this private helper is only called with PIT constants above.
    // 3. Lifetime: each write completes before the helper returns.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: timer setup runs before scheduler concurrency exists.
    // 8. Violation: writing wrong PIT ports can leave the timer unconfigured.
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pit_divisor_matches_requested_tick_rate() {
        assert_eq!(PIT_DIVISOR, 11_931);
    }

    #[test]
    fn timer_interrupt_advances_tick_counter() {
        let before = ticks();
        handle_timer_interrupt();
        assert_eq!(ticks(), before + 1);
    }
}
