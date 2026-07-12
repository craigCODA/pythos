#![no_main]
#![no_std]

mod boot_info;
mod elf;
mod exit_boot_services;
mod graphics;
mod handoff;
mod initrd;
mod memory_map;
mod paging;
mod serial;
mod uefi;

use core::panic::PanicInfo;

type EfiHandle = *mut core::ffi::c_void;
type EfiStatus = usize;

#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(
    image_handle: EfiHandle,
    system_table: *mut uefi::EfiSystemTable,
) -> EfiStatus {
    serial::init_com1();
    serial::write_line("PYTHOS:LOADER:ENTER");
    let mut framebuffer = match graphics::initialize_gop(system_table) {
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
    let stack = match paging::BootstrapStack::allocate(system_table) {
        Ok(stack) => stack,
        Err(()) => fail(),
    };
    let page_tables = match paging::build(system_table, &loaded_kernel, &framebuffer, &stack) {
        Ok(page_tables) => page_tables,
        Err(()) => fail(),
    };
    framebuffer.mapped_virtual_base = paging::DEVICE_VIRT_BASE;
    let mut memory_map = match memory_map::capture(system_table) {
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
        &stack,
    ) {
        Ok(boot_info) => boot_info,
        Err(()) => fail(),
    };
    serial::write_line("PYTHOS:LOADER:MEMORY_MAP_READY");

    match exit_boot_services::exit_once(system_table, image_handle, memory_map.map_key) {
        exit_boot_services::ExitBootServicesResult::Exited => {}
        exit_boot_services::ExitBootServicesResult::StaleMapKey => {
            if memory_map.refresh(system_table).is_err() {
                fail();
            }
            if allocated_boot_info
                .populate(
                    system_table,
                    framebuffer,
                    &loaded_kernel,
                    &init_bundle,
                    &memory_map,
                    &stack,
                )
                .is_err()
            {
                fail();
            }
            match exit_boot_services::exit_once(system_table, image_handle, memory_map.map_key) {
                exit_boot_services::ExitBootServicesResult::Exited => {}
                exit_boot_services::ExitBootServicesResult::StaleMapKey
                | exit_boot_services::ExitBootServicesResult::Failed => fail(),
            }
        }
        exit_boot_services::ExitBootServicesResult::Failed => fail(),
    }
    serial::write_line("PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK");

    // SAFETY:
    // 1. Invariant: `ExitBootServices()` succeeded, `page_tables` identity-map
    //    the running loader and map the kernel image, framebuffer, boot info,
    //    and bootstrap stack, `stack.virt_top` is the mapped stack top, and
    //    `loaded_kernel.entry` is mapped executable.
    // 2. Established by: the successful exit above, `paging::build()`, and
    //    `elf::load_pythcore()` validation of the entry point.
    // 3. Lifetime: the mappings remain valid because no code mutates them
    //    between construction and this jump.
    // 4. Pointer ownership: all loader allocations transfer to PythCore here.
    // 5. Alignment: guaranteed by `paging::build()` and the ELF loader.
    // 6. Mapped length: guaranteed by `paging::build()` page-count arithmetic.
    // 7. Concurrency: single-core; `enter_pythcore` disables interrupts first.
    // 8. Violation: a broken mapping faults with no handler and hangs.
    unsafe {
        handoff::enter_pythcore(
            page_tables.pml4_phys(),
            stack.virt_top,
            boot_info,
            loaded_kernel.entry,
        )
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
