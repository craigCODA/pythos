//! Final transition from the PythOS loader into PythCore.
//!
//! This is the only place the loader gives up control. It runs after
//! `ExitBootServices()` succeeded, so no firmware services are used here;
//! only registers, the loader-built page tables, and the bootstrap stack.

use core::arch::asm;
use pythos_shared::boot_protocol::PythBootInfo;

const EFER_MSR: u32 = 0xC000_0080;
const EFER_NO_EXECUTE_ENABLE: u64 = 1 << 11;

/// Enter PythCore and never return.
///
/// Performs the kernel entry contract from `docs/PythOS-TDD-001.md`:
/// disables maskable interrupts, clears the direction flag, enables
/// `EFER.NXE` for the no-execute leaf permissions in the loader-built
/// tables, activates the temporary kernel page table, switches to the
/// bootstrap stack, places `PythBootInfo` in `RDI`, and jumps to the
/// PythCore ELF entry point with System V AMD64 entry stack alignment.
///
/// # Safety
///
/// The caller must guarantee that `ExitBootServices()` succeeded, that
/// `pml4_phys` names loader-owned page tables mapping the executing loader
/// code identity-style plus the kernel image, framebuffer, and bootstrap
/// stack, that `stack_top_virt` is the 16-byte-aligned top of the mapped
/// bootstrap stack, that `boot_info` points to a populated `PythBootInfo`
/// reachable through those tables, and that `entry_virt` is the PythCore
/// ELF entry point mapped executable in those tables.
pub(crate) unsafe fn enter_pythcore(
    pml4_phys: u64,
    stack_top_virt: u64,
    boot_info: *const PythBootInfo,
    entry_virt: u64,
) -> ! {
    // SAFETY:
    // 1. Invariant: `pml4_phys` is an active-mappable PML4 that identity-maps
    //    this code, `stack_top_virt` is mapped writable below itself,
    //    `boot_info` is reachable through the new tables, and `entry_virt` is
    //    mapped executable.
    // 2. Established by: the caller's contract; `paging::build()` constructed
    //    the tables and `BootstrapStack::allocate()` provided the stack.
    // 3. Lifetime: the invariants must hold from here onward; control never
    //    returns to the loader.
    // 4. Pointer ownership: ownership of all loader allocations transfers to
    //    PythCore at the jump.
    // 5. Alignment: `pml4_phys` is 4 KiB aligned; `stack_top_virt` is page
    //    aligned, and the pushed zero return address yields the System V
    //    AMD64 function-entry alignment (`rsp % 16 == 8`).
    // 6. Mapped length: the tables map the full kernel image, stack, and
    //    boot-info ranges as built by `paging::build()`.
    // 7. Concurrency: single-core execution with interrupts disabled by the
    //    leading `cli`.
    // 8. Violation: a wrong mapping or entry point faults with no handler,
    //    hanging the machine instead of entering PythCore.
    unsafe {
        asm!(
            "cli",
            "cld",
            "mov ecx, {efer_msr}",
            "rdmsr",
            "or eax, {nxe}",
            "wrmsr",
            "mov cr3, r8",
            "mov rsp, r9",
            "push 0",
            "jmp r10",
            efer_msr = const EFER_MSR,
            nxe = const EFER_NO_EXECUTE_ENABLE as u32,
            in("r8") pml4_phys,
            in("r9") stack_top_virt,
            in("r10") entry_virt,
            in("rdi") boot_info,
            options(noreturn)
        );
    }
}
