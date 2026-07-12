#![no_main]
#![no_std]

mod serial;

use core::panic::PanicInfo;
use pythos_shared::boot_protocol::PythBootInfo;

/// PythCore native entry point.
///
/// # Safety
///
/// The caller must enter from the PythOS loader after firmware handoff setup.
/// `boot_info` must point to a valid `PythBootInfo` structure for the duration
/// of early core initialization. The bootstrap stack, page mappings, direction
/// flag state, interrupt state, and COM1 availability must match the kernel
/// entry contract in `docs/PythOS-TDD-001.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pythcore_entry(_boot_info: *const PythBootInfo) -> ! {
    serial::write_line("PYTHOS:CORE:ENTER");
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial::write_line("PYTHOS:PANIC");
    loop {
        core::hint::spin_loop();
    }
}
