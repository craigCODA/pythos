//! QEMU `isa-debug-exit` support for deterministic boot acceptance tests.

use core::arch::asm;
use pythos_shared::qemu_exit;

pub fn success() -> ! {
    exit(qemu_exit::SUCCESS);
}

pub fn panic() -> ! {
    exit(qemu_exit::PANIC);
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
