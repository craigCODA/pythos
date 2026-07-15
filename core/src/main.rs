#![cfg_attr(not(test), no_main)]
#![cfg_attr(not(test), no_std)]

mod architecture;
mod boot_info;
mod boot_metadata;
mod font;
mod framebuffer;
mod kernel_stacks;
mod memory;
mod qemu_exit;
mod serial;
mod tasks;

#[cfg(not(test))]
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
pub unsafe extern "C" fn pythcore_entry(boot_info: *const PythBootInfo) -> ! {
    serial::write_line("PYTHOS:CORE:ENTER");

    // SAFETY:
    // 1. Invariant: `boot_info` is the `RDI` argument the loader passed and
    //    stays mapped readable through the loader-built page tables.
    // 2. Established by: the loader handoff contract in `docs/PythOS-TDD-001.md`.
    // 3. Lifetime: valid for all of early core initialization.
    // 4. Pointer ownership: PythCore owns the allocation after entry.
    // 5. Alignment: checked inside `boot_info::validate`.
    // 6. Mapped length: one full `PythBootInfo` allocated by the loader.
    // 7. Concurrency: single-core execution with interrupts disabled.
    // 8. Violation: an invalid pointer faults with no handler and hangs.
    let boot_info = match unsafe { boot_info::validate(boot_info) } {
        Ok(info) => info,
        Err(()) => {
            serial::write_line("PYTHOS:CORE:BOOTINFO_INVALID");
            qemu_exit::panic();
        }
    };
    serial::write_line("PYTHOS:CORE:BOOTINFO_VALID");

    #[cfg_attr(test, allow(unused_mut, unused_variables))]
    let mut physical_memory = match memory::physical::initialize(boot_info) {
        Ok(memory) => memory,
        Err(_) => {
            serial::write_line("PYTHOS:CORE:MEMORY_INVALID");
            qemu_exit::panic();
        }
    };
    serial::write_line("PYTHOS:CORE:MEMORY_READY");

    if architecture::x86_64::gdt::initialize().is_err() {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
    serial::write_line("PYTHOS:CORE:GDT_READY");

    if architecture::x86_64::idt::initialize().is_err() {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
    serial::write_line("PYTHOS:CORE:IDT_READY");
    serial::write_line("PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY");

    #[cfg(not(test))]
    {
        if !architecture::x86_64::exceptions::verify_entry_hardening() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED");

        if architecture::x86_64::interrupts::initialize().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:INTERRUPTS_READY");

        let address_space =
            match memory::r#virtual::KernelAddressSpace::build(&mut physical_memory, boot_info) {
                Ok(address_space) => address_space,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
        // SAFETY:
        // 1. Invariant: `address_space` maps the currently executing PythCore
        //    code, active bootstrap stack, boot metadata, framebuffer, COM1 code
        //    path, and page-table frames required for validation.
        // 2. Established by: successful `KernelAddressSpace::build` above.
        // 3. Lifetime: the page tables are intentionally retained for this slice.
        // 4. Pointer ownership: PythCore owns the newly allocated page tables.
        // 5. Alignment: table root was allocated as a 4 KiB physical page.
        // 6. Mapped length: the full active early-core address surface is mapped.
        // 7. Concurrency: single-core execution with interrupts disabled.
        // 8. Violation: execution faults immediately after the CR3 switch.
        unsafe {
            address_space.activate();
        }
        if address_space.validate_active(boot_info).is_err() {
            serial::write_line("PYTHOS:CORE:MEMORY_INVALID");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:VM_READY");
        if memory::r#virtual::prove_old_identity_map_removed().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:IDENTITY_MAP_REMOVED");
        if boot_metadata::validate_complete(boot_info).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:BOOTINFO_COMPLETE");
        if architecture::x86_64::timer::initialize().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:TIMER_READY");
        if architecture::x86_64::clock::initialize().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:CLOCK_READY");
        if tasks::initialize(boot_info).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:TASKS_READY");
        if kernel_stacks::initialize(boot_info).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:KERNEL_STACKS_READY");
    }

    if framebuffer::render_boot_screen(&boot_info.framebuffer).is_err() {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
    serial::write_line("PYTHOS:CORE:FRAMEBUFFER_READY");
    serial::write_line("PYTHOS:CORE:MILESTONE_1_COMPLETE");
    qemu_exit::success();
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial::write_line("PYTHOS:PANIC");
    qemu_exit::panic();
}
