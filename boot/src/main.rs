#![no_main]
#![no_std]

mod serial;

use core::panic::PanicInfo;

type EfiHandle = *mut core::ffi::c_void;
type EfiStatus = usize;

#[repr(C)]
pub struct EfiSystemTable {
    _private: [u8; 0],
}

#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(
    _image_handle: EfiHandle,
    _system_table: *mut EfiSystemTable,
) -> EfiStatus {
    serial::init_com1();
    serial::write_line("PYTHOS:LOADER:ENTER");
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial::write_line("PYTHOS:LOADER:FAIL");
    loop {
        core::hint::spin_loop();
    }
}
