#![no_main]
#![no_std]

mod graphics;
mod serial;

use core::panic::PanicInfo;

type EfiHandle = *mut core::ffi::c_void;
type EfiStatus = usize;

#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(
    _image_handle: EfiHandle,
    system_table: *mut graphics::EfiSystemTable,
) -> EfiStatus {
    serial::init_com1();
    serial::write_line("PYTHOS:LOADER:ENTER");
    match graphics::initialize_gop(system_table) {
        Ok(()) => serial::write_line("PYTHOS:LOADER:GOP_READY"),
        Err(()) => serial::write_line("PYTHOS:LOADER:FAIL"),
    }
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
