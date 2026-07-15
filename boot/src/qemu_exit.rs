//! QEMU `isa-debug-exit` support for deterministic loader failure tests.

use core::arch::asm;
use pythos_shared::qemu_exit;

pub(crate) fn panic() -> ! {
    // SAFETY:
    // 1. Invariant: the x86-64 UEFI loader may issue port I/O before handoff.
    // 2. Established by: UEFI entered the loader on the boot CPU in x86-64 mode.
    // 3. Lifetime: the instruction has no borrowed-memory lifetime.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable to port I/O.
    // 7. Concurrency: the loader is single-threaded.
    // 8. Violation: without QEMU's debug-exit device, the write is ignored and
    //    execution falls through to the spin loop.
    unsafe {
        asm!(
            "out dx, eax",
            in("dx") qemu_exit::DEBUG_EXIT_PORT,
            in("eax") qemu_exit::PANIC,
            options(nomem, nostack, preserves_flags)
        );
    }
    loop {
        core::hint::spin_loop();
    }
}
