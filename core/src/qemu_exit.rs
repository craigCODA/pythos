//! QEMU `isa-debug-exit` support for deterministic boot acceptance tests.

use core::arch::asm;
use pythos_shared::qemu_exit;

#[cfg(feature = "verify")]
pub fn success() -> ! {
    exit(qemu_exit::SUCCESS);
}

pub fn panic() -> ! {
    exit(qemu_exit::PANIC);
}

/// Request a system reset via the i8042 keyboard controller's pulse-reset
/// line (write `0xFE` to port `0x64`) — the standard x86 warm-reset
/// mechanism, supported by the ADR 0052 QEMU q35 target.
#[cfg(not(test))]
pub fn reboot_qemu() -> ! {
    // SAFETY:
    // 1. Invariant: writing `0xFE` to port `0x64` (the i8042 command port)
    //    requests a CPU reset on the ADR 0052 QEMU q35/i8042 target.
    // 2. Established by: ADR 0052 limits this reboot mechanism to the current
    //    QEMU profile; the system-control syscall validates the caller's
    //    capability before this is ever reached.
    // 3. Lifetime: the instruction has no borrowed-memory lifetime.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable to port I/O.
    // 7. Concurrency: ADR 0051/0052 remain single-core.
    // 8. Violation: on hardware without this reset line, the write is
    //    ignored and execution falls through to the spin loop below rather
    //    than silently continuing as if nothing happened.
    unsafe {
        asm!(
            "out dx, al",
            in("dx") 0x64u16,
            in("al") 0xFEu8,
            options(nomem, nostack, preserves_flags)
        );
    }
    loop {
        core::hint::spin_loop();
    }
}

fn exit(code: u32) -> ! {
    // SAFETY:
    // 1. Invariant: x86-64 ring-0 code may write to I/O port `0xF4`.
    // 2. Established by: PythCore runs as the native kernel after firmware exit.
    // 3. Lifetime: the instruction has no borrowed-memory lifetime.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable to port I/O.
    // 7. Concurrency: single-core early boot with interrupts disabled.
    // 8. Violation: without QEMU's debug-exit device, the write is ignored and
    //    execution falls through to the spin loop.
    unsafe {
        asm!(
            "out dx, eax",
            in("dx") qemu_exit::DEBUG_EXIT_PORT,
            in("eax") code,
            options(nomem, nostack, preserves_flags)
        );
    }
    loop {
        core::hint::spin_loop();
    }
}
