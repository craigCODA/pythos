//! Minimal milestone-1 exception sink.

use crate::serial;

pub extern "C" fn panic_stub() -> ! {
    serial::write_line("PYTHOS:PANIC");
    loop {
        core::hint::spin_loop();
    }
}
