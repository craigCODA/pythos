#![no_main]
#![no_std]

use core::panic::PanicInfo;
use pythos_shared::boot_protocol::PythBootInfo;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pythcore_entry(_boot_info: *const PythBootInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
