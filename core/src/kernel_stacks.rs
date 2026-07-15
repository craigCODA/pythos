//! Guarded kernel stack records for native tasks.
#![cfg_attr(test, allow(dead_code))]

use crate::memory::physical::PAGE_SIZE;
#[cfg(not(test))]
use crate::memory::r#virtual as virtual_memory;
use crate::tasks::{BOOTSTRAP_TASK_ID, MAX_TASKS, TaskId};
#[cfg(not(test))]
use core::arch::{asm, global_asm};
#[cfg(not(test))]
use core::cell::UnsafeCell;
#[cfg(not(test))]
use core::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(test))]
use pythos_shared::boot_protocol::PythBootInfo;

#[cfg(not(test))]
unsafe extern "C" {
    fn kernel_stack_guard_probe_fault(address: u64) -> u64;
}

#[cfg(not(test))]
global_asm!(
    r#"
    .global kernel_stack_guard_probe_fault
    kernel_stack_guard_probe_fault:
        push rbx
        mov rbx, rdi
        lea rsi, [rip + .Lkernel_stack_guard_probe_recovery]
        call expect_page_fault_abi
        mov byte ptr [rbx], 0
        pop rbx
        xor eax, eax
        ret
    .Lkernel_stack_guard_probe_recovery:
        pop rbx
        mov eax, 1
        ret
    "#
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KernelStackError {
    InvalidStackBounds,
    CurrentStackOutOfRange,
    GuardPageMapped,
    StackPageUnmapped,
    GuardProbeDidNotFault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct KernelStack {
    task_id: TaskId,
    guard_page: u64,
    bottom: u64,
    top: u64,
}

impl KernelStack {
    const fn empty() -> Self {
        Self {
            task_id: BOOTSTRAP_TASK_ID,
            guard_page: 0,
            bottom: 0,
            top: 0,
        }
    }

    fn bootstrap(bottom: u64, top: u64) -> Result<Self, KernelStackError> {
        if bottom == 0
            || bottom >= top
            || !bottom.is_multiple_of(PAGE_SIZE)
            || !top.is_multiple_of(PAGE_SIZE)
        {
            return Err(KernelStackError::InvalidStackBounds);
        }
        let guard_page = bottom
            .checked_sub(PAGE_SIZE)
            .ok_or(KernelStackError::InvalidStackBounds)?;
        Ok(Self {
            task_id: BOOTSTRAP_TASK_ID,
            guard_page,
            bottom,
            top,
        })
    }

    fn contains(self, pointer: u64) -> bool {
        pointer >= self.bottom && pointer <= self.top
    }

    #[cfg(not(test))]
    fn validate_active(self) -> Result<(), KernelStackError> {
        let stack_pointer = current_stack_pointer();
        if !self.contains(stack_pointer) {
            return Err(KernelStackError::CurrentStackOutOfRange);
        }
        if virtual_memory::active_mapping_present(self.guard_page) {
            return Err(KernelStackError::GuardPageMapped);
        }
        if !virtual_memory::active_mapping_present(self.bottom)
            || !virtual_memory::active_mapping_present(self.top - 1)
        {
            return Err(KernelStackError::StackPageUnmapped);
        }

        // SAFETY:
        // 1. Invariant: `guard_page` is intentionally unmapped in the active
        //    PythCore page tables and the exception handler has a recovery path.
        // 2. Established by: the translation check above and Phase 1.5's
        //    diagnostic exception handler installation.
        // 3. Lifetime: the probe is synchronous and clears expected-fault state
        //    through the exception handler before returning.
        // 4. Pointer ownership: PythCore does not own mapped memory at this
        //    address; the absence of ownership is the property being tested.
        // 5. Alignment: `guard_page` is page-aligned by `bootstrap`.
        // 6. Mapped length: expected to be zero; any successful write is a bug.
        // 7. Concurrency: single-core boot; maskable interrupts remain disabled.
        // 8. Violation: an unexpected fault is diagnosed and enters the panic path.
        if unsafe { kernel_stack_guard_probe_fault(self.guard_page) } != 1 {
            return Err(KernelStackError::GuardProbeDidNotFault);
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct KernelStackTable {
    slots: [KernelStack; MAX_TASKS],
}

impl KernelStackTable {
    const fn new() -> Self {
        Self {
            slots: [KernelStack::empty(); MAX_TASKS],
        }
    }

    fn initialize_bootstrap(&mut self, stack: KernelStack) {
        *self = Self::new();
        self.slots[0] = stack;
    }

    fn active_count(self) -> usize {
        self.slots
            .iter()
            .filter(|stack| stack.bottom != 0 && stack.top != 0)
            .count()
    }
}

#[cfg(not(test))]
struct KernelStackStorage(UnsafeCell<KernelStackTable>);

#[cfg(not(test))]
// SAFETY: the stack table is initialized during single-core boot before any
// context switch, task migration, or SMP writer exists.
unsafe impl Sync for KernelStackStorage {}

#[cfg(not(test))]
static KERNEL_STACKS: KernelStackStorage =
    KernelStackStorage(UnsafeCell::new(KernelStackTable::new()));
#[cfg(not(test))]
static INITIALIZED: AtomicBool = AtomicBool::new(false);

#[cfg(not(test))]
pub fn initialize(boot_info: &PythBootInfo) -> Result<(), KernelStackError> {
    let bootstrap = KernelStack::bootstrap(
        boot_info.bootstrap_stack_bottom,
        boot_info.bootstrap_stack_top,
    )?;
    bootstrap.validate_active()?;

    // SAFETY:
    // 1. Invariant: the guarded-stack table is written only during this slice.
    // 2. Established by: Phase 2 has not introduced context switching or SMP.
    // 3. Lifetime: the table is static and remains mapped for all PythCore.
    // 4. Pointer ownership: PythCore exclusively owns `KERNEL_STACKS` now.
    // 5. Alignment: `UnsafeCell<KernelStackTable>` preserves table alignment.
    // 6. Mapped length: exactly one `KernelStackTable` object is accessed.
    // 7. Concurrency: single-core boot; interrupts do not touch stack records.
    // 8. Violation: concurrent mutation would corrupt later scheduler state.
    let table = unsafe { &mut *KERNEL_STACKS.0.get() };
    table.initialize_bootstrap(bootstrap);
    if table.active_count() != 1 {
        return Err(KernelStackError::InvalidStackBounds);
    }
    INITIALIZED.store(true, Ordering::SeqCst);
    Ok(())
}

#[cfg(not(test))]
fn current_stack_pointer() -> u64 {
    let rsp: u64;
    // SAFETY:
    // 1. Invariant: reading `RSP` is valid in x86-64 ring 0.
    // 2. Established by: PythCore executes as the native kernel.
    // 3. Lifetime: the returned scalar is a snapshot only.
    // 4. Pointer ownership: no pointer is dereferenced.
    // 5. Alignment: not applicable; no memory is accessed.
    // 6. Mapped length: not applicable.
    // 7. Concurrency: single-core boot; `RSP` belongs to the current context.
    // 8. Violation: outside x86-64 kernel mode this instruction would be invalid.
    unsafe {
        asm!("mov {out}, rsp", out = out(reg) rsp, options(nomem, nostack, preserves_flags));
    }
    rsp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_stack_records_guard_page_below_bottom() {
        let stack = KernelStack::bootstrap(0xFFFF_E000_0000_1000, 0xFFFF_E000_0001_1000).unwrap();

        assert_eq!(stack.task_id, BOOTSTRAP_TASK_ID);
        assert_eq!(stack.guard_page, 0xFFFF_E000_0000_0000);
        assert_eq!(stack.bottom, 0xFFFF_E000_0000_1000);
        assert_eq!(stack.top, 0xFFFF_E000_0001_1000);
        assert!(stack.contains(0xFFFF_E000_0000_8000));
    }

    #[test]
    fn bootstrap_stack_requires_page_aligned_bounds() {
        assert_eq!(
            KernelStack::bootstrap(0x1001, 0x3000),
            Err(KernelStackError::InvalidStackBounds)
        );
        assert_eq!(
            KernelStack::bootstrap(0x3000, 0x3000),
            Err(KernelStackError::InvalidStackBounds)
        );
    }

    #[test]
    fn table_records_one_active_bootstrap_stack() {
        let mut table = KernelStackTable::new();
        let stack = KernelStack::bootstrap(0x2000, 0x6000).unwrap();

        table.initialize_bootstrap(stack);

        assert_eq!(table.active_count(), 1);
    }
}
