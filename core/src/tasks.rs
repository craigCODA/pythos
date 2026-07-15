//! Fixed native task records for Phase 2 scheduler bootstrap.
#![cfg_attr(test, allow(dead_code))]

use core::arch::asm;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use pythos_shared::boot_protocol::PythBootInfo;

pub const MAX_TASKS: usize = 8;
pub const BOOTSTRAP_TASK_ID: TaskId = TaskId(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskId(u64);

impl TaskId {
    #[allow(dead_code)]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SavedTaskFrame {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
    rip: u64,
    rflags: u64,
}

impl SavedTaskFrame {
    pub const fn empty() -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            rdx: 0,
            rcx: 0,
            rbx: 0,
            rax: 0,
            rip: 0,
            rflags: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskControlBlock {
    id: TaskId,
    state: TaskState,
    kernel_stack_bottom: u64,
    kernel_stack_top: u64,
    kernel_stack_pointer: u64,
    saved_frame: SavedTaskFrame,
}

impl TaskControlBlock {
    pub const fn terminated(slot: usize) -> Self {
        Self {
            id: TaskId(slot as u64 + 1),
            state: TaskState::Terminated,
            kernel_stack_bottom: 0,
            kernel_stack_top: 0,
            kernel_stack_pointer: 0,
            saved_frame: SavedTaskFrame::empty(),
        }
    }

    #[allow(dead_code)]
    pub const fn id(self) -> TaskId {
        self.id
    }

    #[allow(dead_code)]
    pub const fn state(self) -> TaskState {
        self.state
    }

    #[allow(dead_code)]
    pub const fn kernel_stack_pointer(self) -> u64 {
        self.kernel_stack_pointer
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum TaskInitError {
    InvalidStackBounds,
    StackPointerOutOfRange,
}

#[derive(Clone, Copy)]
pub struct TaskTable {
    slots: [TaskControlBlock; MAX_TASKS],
}

impl TaskTable {
    pub const fn new() -> Self {
        Self {
            slots: [
                TaskControlBlock::terminated(0),
                TaskControlBlock::terminated(1),
                TaskControlBlock::terminated(2),
                TaskControlBlock::terminated(3),
                TaskControlBlock::terminated(4),
                TaskControlBlock::terminated(5),
                TaskControlBlock::terminated(6),
                TaskControlBlock::terminated(7),
            ],
        }
    }

    pub fn initialize_bootstrap(
        &mut self,
        stack_bottom: u64,
        stack_top: u64,
        stack_pointer: u64,
    ) -> Result<(), TaskInitError> {
        if stack_bottom == 0 || stack_bottom >= stack_top {
            return Err(TaskInitError::InvalidStackBounds);
        }
        if stack_pointer < stack_bottom || stack_pointer > stack_top {
            return Err(TaskInitError::StackPointerOutOfRange);
        }

        *self = Self::new();
        self.slots[0] = TaskControlBlock {
            id: BOOTSTRAP_TASK_ID,
            state: TaskState::Running,
            kernel_stack_bottom: stack_bottom,
            kernel_stack_top: stack_top,
            kernel_stack_pointer: stack_pointer,
            saved_frame: SavedTaskFrame::empty(),
        };
        Ok(())
    }

    #[allow(dead_code)]
    pub const fn get(&self, index: usize) -> Option<&TaskControlBlock> {
        if index < MAX_TASKS {
            Some(&self.slots[index])
        } else {
            None
        }
    }

    pub fn running_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|task| task.state == TaskState::Running)
            .count()
    }
}

struct TaskTableStorage(UnsafeCell<TaskTable>);

// SAFETY: the task table is initialized during single-core boot before any
// scheduler or task concurrency exists. Later slices must introduce explicit
// synchronization before mutating it concurrently.
unsafe impl Sync for TaskTableStorage {}

static TASKS: TaskTableStorage = TaskTableStorage(UnsafeCell::new(TaskTable::new()));
static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn initialize(boot_info: &PythBootInfo) -> Result<(), TaskInitError> {
    let stack_pointer = current_stack_pointer();
    // SAFETY:
    // 1. Invariant: the static task table is written only during this boot slice.
    // 2. Established by: Phase 2 has no scheduler, task concurrency, or SMP yet.
    // 3. Lifetime: the table is static and remains mapped for all PythCore.
    // 4. Pointer ownership: PythCore exclusively owns `TASKS` during bootstrap.
    // 5. Alignment: `UnsafeCell<TaskTable>` preserves `TaskTable` alignment.
    // 6. Mapped length: exactly one `TaskTable` object is accessed.
    // 7. Concurrency: single-core boot; interrupts may tick but do not touch tasks.
    // 8. Violation: concurrent mutation would corrupt scheduler state later.
    let table = unsafe { &mut *TASKS.0.get() };
    table.initialize_bootstrap(
        boot_info.bootstrap_stack_bottom,
        boot_info.bootstrap_stack_top,
        stack_pointer,
    )?;
    if table.running_count() != 1 {
        return Err(TaskInitError::StackPointerOutOfRange);
    }
    INITIALIZED.store(true, Ordering::SeqCst);
    Ok(())
}

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
    fn bootstrap_initialization_records_one_running_task() {
        let mut table = TaskTable::new();

        assert_eq!(table.initialize_bootstrap(0x1000, 0x3000, 0x2800), Ok(()));

        let task = table.get(0).unwrap();
        assert_eq!(task.id(), BOOTSTRAP_TASK_ID);
        assert_eq!(task.state(), TaskState::Running);
        assert_eq!(task.kernel_stack_pointer(), 0x2800);
        assert_eq!(table.running_count(), 1);
        assert_eq!(table.get(1).unwrap().state(), TaskState::Terminated);
    }

    #[test]
    fn bootstrap_stack_pointer_must_be_inside_stack_bounds() {
        let mut table = TaskTable::new();

        assert_eq!(
            table.initialize_bootstrap(0x1000, 0x3000, 0x0800),
            Err(TaskInitError::StackPointerOutOfRange)
        );
        assert_eq!(
            table.initialize_bootstrap(0x3000, 0x1000, 0x2000),
            Err(TaskInitError::InvalidStackBounds)
        );
    }

    #[test]
    fn saved_task_frame_layout_starts_with_callee_saved_registers() {
        assert_eq!(core::mem::offset_of!(SavedTaskFrame, r15), 0);
        assert_eq!(core::mem::offset_of!(SavedTaskFrame, rbx), 104);
        assert_eq!(core::mem::offset_of!(SavedTaskFrame, rip), 120);
        assert_eq!(core::mem::offset_of!(SavedTaskFrame, rflags), 128);
    }
}
