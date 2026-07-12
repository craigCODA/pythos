#![no_main]
#![no_std]

mod boot_info;
mod elf;
mod graphics;
mod initrd;
mod memory_map;
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
    let framebuffer = match graphics::initialize_gop(system_table) {
        Ok(framebuffer) => {
            serial::write_line("PYTHOS:LOADER:GOP_READY");
            framebuffer
        }
        Err(()) => fail(),
    };
    let loaded_kernel = match elf::load_pythcore(system_table) {
        Ok(loaded_kernel) if loaded_kernel.is_well_formed() => loaded_kernel,
        Ok(_) => fail(),
        Err(()) => fail(),
    };
    serial::write_line("PYTHOS:LOADER:KERNEL_LOADED");

    let init_bundle = match initrd::load_init_pak(system_table) {
        Ok(init_bundle) if init_bundle.is_loaded() => init_bundle,
        Ok(_) => fail(),
        Err(()) => fail(),
    };
    let allocated_boot_info = match boot_info::AllocatedBootInfo::allocate(system_table) {
        Ok(allocated_boot_info) => allocated_boot_info,
        Err(()) => fail(),
    };
    let memory_map = match memory_map::capture(system_table) {
        Ok(memory_map) if memory_map.is_captured() => memory_map,
        Ok(_) => fail(),
        Err(()) => fail(),
    };
    let boot_info = match allocated_boot_info.populate(
        system_table,
        framebuffer,
        &loaded_kernel,
        &init_bundle,
        &memory_map,
    ) {
        Ok(boot_info) => boot_info,
        Err(()) => fail(),
    };
    serial::write_line("PYTHOS:LOADER:MEMORY_MAP_READY");

    loop {
        core::hint::black_box(&loaded_kernel);
        core::hint::black_box(&init_bundle);
        core::hint::black_box(&memory_map);
        core::hint::black_box(&boot_info);
        core::hint::spin_loop();
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
