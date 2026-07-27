//! Normal-boot production initialization (ADR 0052).
//!
//! Extracted from the verification proof paths: each `initialize_*` helper
//! performs only production construction, with no self-test/adversarial
//! markers and no deliberately triggered faults. `physical_memory` and
//! `boot_info` are already constructed by the common prefix in
//! `pythcore_entry` before this module runs.

use crate::block_device::{self, BlockDeviceInfo};
use crate::memory::physical::PhysicalMemory;
use crate::{architecture, kernel_stacks, serial, syscall, tasks, user_stacks};
use pythos_shared::boot_protocol::PythBootInfo;

pub struct NormalBootSubstrate {
    pub block_device: BlockDeviceInfo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalInitError {
    Memory,
    InterruptsTimer,
    TaskProcess,
    Ring3,
    UserStacks,
    BlockDevice,
}

#[cfg(not(test))]
pub fn initialize_normal_substrate(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
) -> Result<NormalBootSubstrate, NormalInitError> {
    // Build every address space that needs `PageTableBuilder`'s raw-physical-
    // address table writes *before* activating the kernel address space. Once
    // `KernelAddressSpace::activate()` switches CR3, the loader's broad low-
    // memory identity map is gone by design (the Phase 1.5 identity-map-removal
    // invariant), so a later `UserAddressSpace::build()` would fault writing
    // its own fresh page-table frames. The verify path relies on the same
    // ordering (see `pythcore_entry`): build kernel + user address spaces,
    // then activate only the kernel one.
    let kernel_address_space =
        crate::memory::r#virtual::KernelAddressSpace::build(physical_memory, boot_info, None)
            .map_err(|_| NormalInitError::Memory)?;
    let _user_address_space =
        crate::memory::r#virtual::UserAddressSpace::build(physical_memory, boot_info)
            .map_err(|_| NormalInitError::Ring3)?;
    // SAFETY:
    // 1. Invariant: `kernel_address_space` maps the currently executing
    //    PythCore code, active bootstrap stack, boot metadata, and
    //    framebuffer.
    // 2. Established by: successful `KernelAddressSpace::build` immediately
    //    above, mirroring the verification path's identical activation.
    // 3. Lifetime: the page tables are retained for the life of normal boot.
    // 4. Pointer ownership: PythCore owns the newly allocated page tables.
    // 5. Alignment: the table root was allocated as a 4 KiB physical page.
    // 6. Mapped length: the full active early-core address surface is mapped.
    // 7. Concurrency: single-core execution with interrupts disabled.
    // 8. Violation: a broken mapping faults immediately after the CR3 switch.
    unsafe {
        kernel_address_space.activate();
    }
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:MEMORY_VM_READY");
    // GDT/TSS ring-3 selectors are already installed by the common
    // `gdt::initialize()` call before the verify/normal branch; this marker
    // covers the user address-space construction proved just above.
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:RING3_READY");

    initialize_interrupts_timer_and_clock().map_err(|_| NormalInitError::InterruptsTimer)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:INTERRUPTS_TIMER_READY");

    initialize_task_process_and_kernel_stack_state(boot_info)
        .map_err(|_| NormalInitError::TaskProcess)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:TASK_PROCESS_READY");

    syscall::initialize();
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:SYSCALL_READY");

    initialize_guarded_user_stack_pool().map_err(|_| NormalInitError::UserStacks)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:USER_STACKS_READY");

    let block_device = block_device::select_device().map_err(|_| NormalInitError::BlockDevice)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY");

    Ok(NormalBootSubstrate { block_device })
}

#[cfg(not(test))]
fn initialize_interrupts_timer_and_clock() -> Result<(), NormalInitError> {
    architecture::x86_64::interrupts::initialize().map_err(|_| NormalInitError::InterruptsTimer)?;
    architecture::x86_64::timer::initialize().map_err(|_| NormalInitError::InterruptsTimer)?;
    architecture::x86_64::clock::initialize().map_err(|_| NormalInitError::InterruptsTimer)?;
    Ok(())
}

#[cfg(not(test))]
fn initialize_task_process_and_kernel_stack_state(
    boot_info: &'static PythBootInfo,
) -> Result<(), NormalInitError> {
    tasks::initialize(boot_info).map_err(|_| NormalInitError::TaskProcess)?;
    kernel_stacks::initialize(boot_info).map_err(|_| NormalInitError::TaskProcess)?;
    Ok(())
}

#[cfg(not(test))]
fn initialize_guarded_user_stack_pool() -> Result<(), NormalInitError> {
    user_stacks::initialize().map_err(|_| NormalInitError::UserStacks)
}
