#![no_main]
#![no_std]

mod elf;
mod graphics;
mod serial;
mod uefi;

use core::panic::PanicInfo;

type EfiHandle = *mut core::ffi::c_void;
type EfiStatus = usize;

#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(
    _image_handle: EfiHandle,
    system_table: *mut uefi::EfiSystemTable,
) -> EfiStatus {
    serial::init_com1();
    serial::write_line("PYTHOS:LOADER:ENTER");
    match graphics::initialize_gop(system_table) {
        Ok(()) => serial::write_line("PYTHOS:LOADER:GOP_READY"),
        Err(()) => fail(),
    }
    match elf::load_pythcore(system_table) {
        Ok(loaded_kernel) if loaded_kernel.is_well_formed() => {
            serial::write_line("PYTHOS:LOADER:KERNEL_LOADED");
            loop {
                core::hint::black_box(&loaded_kernel);
                core::hint::spin_loop();
            }
        }
        Ok(_) => fail(),
        Err(()) => fail(),
    }
}

fn fail() -> ! {
    serial::write_line("PYTHOS:LOADER:FAIL");
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
