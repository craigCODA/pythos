#![cfg_attr(not(test), no_main)]
#![no_std]

mod boot_info;
mod elf;
#[cfg(feature = "evidence-terminal")]
mod evidence_log;
mod exit_boot_services;
mod fb_debug;
mod firmware;
mod font;
mod graphics;
mod handoff;
mod initrd;
mod memory_map;
mod paging;
mod qemu_exit;
mod serial;
mod uefi;

#[cfg(not(test))]
use core::panic::PanicInfo;

type EfiHandle = *mut core::ffi::c_void;
type EfiStatus = usize;

#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(
    image_handle: EfiHandle,
    system_table: *mut uefi::EfiSystemTable,
) -> EfiStatus {
    serial::init_com1();
    #[cfg(feature = "evidence-terminal")]
    let mut evidence_log = match evidence_log::AllocatedEvidenceLog::allocate(system_table) {
        Ok(log) => Some(log),
        Err(()) => {
            serial::write_line("PYTHOS:LOADER:EVIDENCE_LOG_ALLOC_FAILED");
            None
        }
    };
    #[cfg(not(feature = "evidence-terminal"))]
    let mut evidence_log = None;
    loader_marker(&mut evidence_log, "PYTHOS:LOADER:ENTER");
    let mut framebuffer = match graphics::initialize_gop(system_table) {
        Ok(framebuffer) => {
            loader_marker(&mut evidence_log, "PYTHOS:LOADER:GOP_READY");
            framebuffer
        }
        Err(()) => fail(&mut evidence_log),
    };
    fb_debug::fill(&framebuffer, fb_debug::COLOR_GOP);
    let loaded_kernel = match elf::load_pythcore(system_table, image_handle) {
        Ok(loaded_kernel) if loaded_kernel.is_well_formed() => loaded_kernel,
        Ok(_) => fail(&mut evidence_log),
        Err(()) => fail(&mut evidence_log),
    };
    loader_marker(&mut evidence_log, "PYTHOS:LOADER:KERNEL_LOADED");
    fb_debug::fill(&framebuffer, fb_debug::COLOR_KERNEL);

    let init_bundle = match initrd::load_init_pak(system_table, image_handle) {
        Ok(init_bundle) if init_bundle.is_loaded() => init_bundle,
        Ok(_) => fail(&mut evidence_log),
        Err(()) => fail(&mut evidence_log),
    };
    let loaded_font = match font::load_font(system_table, image_handle) {
        Ok(loaded_font) if loaded_font.is_loaded() => loaded_font,
        Ok(_) => fail(&mut evidence_log),
        Err(()) => fail(&mut evidence_log),
    };
    let firmware_tables = match firmware::discover(system_table) {
        Ok(tables) => tables,
        Err(()) => fail(&mut evidence_log),
    };
    let allocated_boot_info = match boot_info::AllocatedBootInfo::allocate(system_table) {
        Ok(allocated_boot_info) => allocated_boot_info,
        Err(()) => fail(&mut evidence_log),
    };
    let stack = match paging::BootstrapStack::allocate(system_table) {
        Ok(stack) => stack,
        Err(()) => fail(&mut evidence_log),
    };
    let page_tables = match paging::build(system_table, &loaded_kernel, &framebuffer, &stack) {
        Ok(page_tables) => page_tables,
        Err(()) => fail(&mut evidence_log),
    };
    framebuffer.mapped_virtual_base = paging::DEVICE_VIRT_BASE;
    let mut memory_map = match memory_map::capture(system_table) {
        Ok(memory_map) if memory_map.is_captured() => memory_map,
        Ok(_) => fail(&mut evidence_log),
        Err(()) => fail(&mut evidence_log),
    };
    let boot_info = match allocated_boot_info.populate(boot_info::BootInfoInputs {
        framebuffer,
        kernel: &loaded_kernel,
        init_bundle: &init_bundle,
        font: &loaded_font,
        memory_map: &memory_map,
        stack: &stack,
        firmware_tables,
        evidence_log: evidence_log_input(&evidence_log),
    }) {
        Ok(boot_info) => boot_info,
        Err(()) => fail(&mut evidence_log),
    };
    loader_marker(&mut evidence_log, "PYTHOS:LOADER:MEMORY_MAP_READY");
    fb_debug::fill(&framebuffer, fb_debug::COLOR_MMAP);

    match exit_boot_services::exit_once(system_table, image_handle, memory_map.map_key) {
        exit_boot_services::ExitBootServicesResult::Exited => {}
        exit_boot_services::ExitBootServicesResult::StaleMapKey => {
            if memory_map.refresh(system_table).is_err() {
                fail(&mut evidence_log);
            }
            if allocated_boot_info
                .populate(boot_info::BootInfoInputs {
                    framebuffer,
                    kernel: &loaded_kernel,
                    init_bundle: &init_bundle,
                    font: &loaded_font,
                    memory_map: &memory_map,
                    stack: &stack,
                    firmware_tables,
                    evidence_log: evidence_log_input(&evidence_log),
                })
                .is_err()
            {
                fail(&mut evidence_log);
            }
            match exit_boot_services::exit_once(system_table, image_handle, memory_map.map_key) {
                exit_boot_services::ExitBootServicesResult::Exited => {}
                exit_boot_services::ExitBootServicesResult::StaleMapKey
                | exit_boot_services::ExitBootServicesResult::Failed => fail(&mut evidence_log),
            }
        }
        exit_boot_services::ExitBootServicesResult::Failed => fail(&mut evidence_log),
    }
    loader_marker(&mut evidence_log, "PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK");
    // Firmware paging is still active until `enter_pythcore` switches CR3, so the
    // identity-mapped framebuffer is still writable. This final paint proves the
    // loader survived ExitBootServices and is about to hand off to PythCore.
    fb_debug::fill(&framebuffer, fb_debug::COLOR_EXIT);

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

#[cfg(feature = "evidence-terminal")]
fn loader_marker(log: &mut Option<evidence_log::AllocatedEvidenceLog>, marker: &str) {
    evidence_log::write_marker(log, marker);
}

#[cfg(not(feature = "evidence-terminal"))]
fn loader_marker(_log: &mut Option<()>, marker: &str) {
    serial::write_line(marker);
}

#[cfg(feature = "evidence-terminal")]
fn evidence_log_input(
    log: &Option<evidence_log::AllocatedEvidenceLog>,
) -> boot_info::EvidenceLogInput<'_> {
    log.as_ref()
}

#[cfg(not(feature = "evidence-terminal"))]
fn evidence_log_input(_log: &Option<()>) -> boot_info::EvidenceLogInput<'_> {
    None
}

#[cfg(test)]
fn main() {}

#[cfg(feature = "evidence-terminal")]
fn fail(log: &mut Option<evidence_log::AllocatedEvidenceLog>) -> ! {
    loader_marker(log, "PYTHOS:LOADER:FAIL");
    fb_debug::fill_fail();
    qemu_exit::panic();
}

#[cfg(not(feature = "evidence-terminal"))]
fn fail(_log: &mut Option<()>) -> ! {
    serial::write_line("PYTHOS:LOADER:FAIL");
    fb_debug::fill_fail();
    qemu_exit::panic();
}

#[cfg(all(not(test), feature = "evidence-terminal"))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    // Evidence state is unavailable here because this panic handler has no local
    // reference to the in-scope loader evidence-log object.
    serial::write_line("PYTHOS:LOADER:FAIL");
    fb_debug::fill_fail();
    qemu_exit::panic();
}

#[cfg(all(not(test), not(feature = "evidence-terminal")))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial::write_line("PYTHOS:LOADER:FAIL");
    fb_debug::fill_fail();
    qemu_exit::panic();
}
