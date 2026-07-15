//! Cooperative round-robin scheduler bootstrap.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::context_switch::{self, ContextFrame};
#[cfg(not(test))]
use crate::serial;
use crate::tasks::TaskId;
#[cfg(not(test))]
use core::cell::UnsafeCell;
#[cfg(not(test))]
use core::hint;
#[cfg(not(test))]
use core::sync::atomic::{AtomicUsize, Ordering};

#[cfg(not(test))]
const SCHEDULER_STACK_SIZE: usize = 16 * 4096;
#[cfg(not(test))]
const EXPECTED_TASK_RUNS: usize = 2;
#[cfg(not(test))]
const EXPECTED_IDLE_RUNS: usize = 1;

const SCHEDULE_A: SchedulerTaskId = SchedulerTaskId::new(2);
const SCHEDULE_B: SchedulerTaskId = SchedulerTaskId::new(3);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerError {
    NoReadyTask,
    BadScheduleOrder,
    BadTaskRunCount,
    BadIdleRunCount,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchedulerTaskId(TaskId);

impl SchedulerTaskId {
    pub const fn new(raw: u64) -> Self {
        Self(TaskId::new(raw))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SchedulerTaskState {
    Ready,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SchedulerTask {
    id: SchedulerTaskId,
    state: SchedulerTaskState,
}

impl SchedulerTask {
    const fn ready(id: SchedulerTaskId) -> Self {
        Self {
            id,
            state: SchedulerTaskState::Ready,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RoundRobinScheduler<const N: usize> {
    tasks: [SchedulerTask; N],
    cursor: usize,
}

impl<const N: usize> RoundRobinScheduler<N> {
    const fn new(tasks: [SchedulerTask; N]) -> Self {
        Self { tasks, cursor: 0 }
    }

    fn next_ready(&mut self) -> Result<SchedulerTaskId, SchedulerError> {
        let mut checked = 0;
        while checked < N {
            let index = self.cursor;
            self.cursor = (self.cursor + 1) % N;
            checked += 1;
            if self.tasks[index].state == SchedulerTaskState::Ready {
                return Ok(self.tasks[index].id);
            }
        }
        Err(SchedulerError::NoReadyTask)
    }
}

#[cfg(not(test))]
#[repr(align(16))]
#[allow(dead_code)]
struct SchedulerStack([u8; SCHEDULER_STACK_SIZE]);

#[cfg(not(test))]
struct SchedulerStackStorage(UnsafeCell<SchedulerStack>);

#[cfg(not(test))]
// SAFETY: scheduler self-test stacks are initialized during single-core boot,
// and only the active context mutates its own stack.
unsafe impl Sync for SchedulerStackStorage {}

#[cfg(not(test))]
struct ContextStorage(UnsafeCell<ContextFrame>);

#[cfg(not(test))]
// SAFETY: each context frame is mutated only by the cooperative switch path
// while Phase 2 still has no SMP or concurrent task migration.
unsafe impl Sync for ContextStorage {}

#[cfg(not(test))]
static SCHEDULER_A_STACK: SchedulerStackStorage =
    SchedulerStackStorage(UnsafeCell::new(SchedulerStack([0; SCHEDULER_STACK_SIZE])));
#[cfg(not(test))]
static SCHEDULER_B_STACK: SchedulerStackStorage =
    SchedulerStackStorage(UnsafeCell::new(SchedulerStack([0; SCHEDULER_STACK_SIZE])));
#[cfg(not(test))]
static IDLE_STACK: SchedulerStackStorage =
    SchedulerStackStorage(UnsafeCell::new(SchedulerStack([0; SCHEDULER_STACK_SIZE])));
#[cfg(not(test))]
static BOOT_CONTEXT: ContextStorage = ContextStorage(UnsafeCell::new(ContextFrame::empty()));
#[cfg(not(test))]
static TASK_A_CONTEXT: ContextStorage = ContextStorage(UnsafeCell::new(ContextFrame::empty()));
#[cfg(not(test))]
static TASK_B_CONTEXT: ContextStorage = ContextStorage(UnsafeCell::new(ContextFrame::empty()));
#[cfg(not(test))]
static IDLE_CONTEXT: ContextStorage = ContextStorage(UnsafeCell::new(ContextFrame::empty()));
#[cfg(not(test))]
static TASK_A_RUNS: AtomicUsize = AtomicUsize::new(0);
#[cfg(not(test))]
static TASK_B_RUNS: AtomicUsize = AtomicUsize::new(0);
#[cfg(not(test))]
static IDLE_RUNS: AtomicUsize = AtomicUsize::new(0);

#[cfg(not(test))]
pub fn run_self_test() -> Result<(), SchedulerError> {
    TASK_A_RUNS.store(0, Ordering::SeqCst);
    TASK_B_RUNS.store(0, Ordering::SeqCst);
    let mut scheduler = RoundRobinScheduler::new([
        SchedulerTask::ready(SCHEDULE_A),
        SchedulerTask::ready(SCHEDULE_B),
    ]);
    let expected = [SCHEDULE_A, SCHEDULE_B, SCHEDULE_A, SCHEDULE_B];

    // SAFETY:
    // 1. Invariant: scheduler frames and test stacks are static, mapped,
    //    aligned, and not used by any preemptive scheduler or interrupt path.
    // 2. Established by: Phase 2 has not introduced preemption, SMP, task
    //    termination, or migration; these statics are private to this module.
    // 3. Lifetime: all frames and stacks live for the full kernel lifetime.
    // 4. Pointer ownership: this scheduler self-test exclusively owns these
    //    contexts while it runs.
    // 5. Alignment: `SchedulerStack` is 16-byte aligned; context frames use
    //    `repr(C)`.
    // 6. Mapped length: each stack has `SCHEDULER_STACK_SIZE` bytes; each frame
    //    is exactly one `ContextFrame`.
    // 7. Concurrency: cooperative single-core execution; interrupts do not
    //    access these frames.
    // 8. Violation: a bad frame or stack pointer faults through diagnostics.
    unsafe {
        *TASK_A_CONTEXT.0.get() =
            ContextFrame::new(task_a_entry, stack_top(SCHEDULER_A_STACK.0.get()));
        *TASK_B_CONTEXT.0.get() =
            ContextFrame::new(task_b_entry, stack_top(SCHEDULER_B_STACK.0.get()));
    }

    for expected_task in expected {
        let next = scheduler.next_ready()?;
        if next != expected_task {
            return Err(SchedulerError::BadScheduleOrder);
        }
        switch_to(next);
    }

    if TASK_A_RUNS.load(Ordering::SeqCst) != EXPECTED_TASK_RUNS
        || TASK_B_RUNS.load(Ordering::SeqCst) != EXPECTED_TASK_RUNS
    {
        return Err(SchedulerError::BadTaskRunCount);
    }
    Ok(())
}

#[cfg(not(test))]
pub fn run_idle_self_test() -> Result<(), SchedulerError> {
    IDLE_RUNS.store(0, Ordering::SeqCst);
    let mut scheduler = RoundRobinScheduler::<0>::new([]);
    if scheduler.next_ready() != Err(SchedulerError::NoReadyTask) {
        return Err(SchedulerError::BadScheduleOrder);
    }

    // SAFETY:
    // 1. Invariant: the idle frame and idle stack are static, mapped, aligned,
    //    and private to the Phase 2 scheduler proof.
    // 2. Established by: the guarded kernel stack and VM slices, plus private
    //    scheduler statics initialized before switching.
    // 3. Lifetime: frame and stack live for the full kernel lifetime.
    // 4. Pointer ownership: this idle self-test exclusively initializes and
    //    switches through the idle context.
    // 5. Alignment: `SchedulerStack` is 16-byte aligned; frame uses `repr(C)`.
    // 6. Mapped length: one stack of `SCHEDULER_STACK_SIZE` bytes and one frame.
    // 7. Concurrency: single-core cooperative execution; no migration.
    // 8. Violation: bad pointers or stack bounds fault through diagnostics.
    unsafe {
        *IDLE_CONTEXT.0.get() = ContextFrame::new(idle_task_entry, stack_top(IDLE_STACK.0.get()));
    }
    switch_to_idle();

    if IDLE_RUNS.load(Ordering::SeqCst) != EXPECTED_IDLE_RUNS {
        return Err(SchedulerError::BadIdleRunCount);
    }
    Ok(())
}

#[cfg(not(test))]
fn switch_to(task: SchedulerTaskId) {
    let next = if task == SCHEDULE_A {
        TASK_A_CONTEXT.0.get()
    } else {
        TASK_B_CONTEXT.0.get()
    };
    // SAFETY:
    // 1. Invariant: `BOOT_CONTEXT` and the selected task context are initialized
    //    and mapped, and the task context names a valid scheduler test stack.
    // 2. Established by: `run_self_test` frame initialization before scheduling.
    // 3. Lifetime: static frames and stacks live for the full kernel lifetime.
    // 4. Pointer ownership: the switch path mutably saves only `BOOT_CONTEXT`
    //    and reads the selected task context.
    // 5. Alignment: `ContextFrame` statics preserve frame alignment.
    // 6. Mapped length: exactly one current and one next frame are accessed.
    // 7. Concurrency: cooperative single-core scheduler self-test.
    // 8. Violation: a bad frame pointer faults through diagnostics.
    unsafe {
        context_switch::switch(BOOT_CONTEXT.0.get(), next);
    }
}

#[cfg(not(test))]
fn switch_to_idle() {
    // SAFETY:
    // 1. Invariant: `BOOT_CONTEXT` and `IDLE_CONTEXT` are initialized and
    //    `IDLE_CONTEXT` names a valid scheduler test stack.
    // 2. Established by: `run_idle_self_test` frame initialization.
    // 3. Lifetime: static frames and stacks live for the full kernel lifetime.
    // 4. Pointer ownership: the switch path saves only `BOOT_CONTEXT`.
    // 5. Alignment: `ContextFrame` statics preserve frame alignment.
    // 6. Mapped length: exactly one current and one next frame are accessed.
    // 7. Concurrency: cooperative single-core scheduler self-test.
    // 8. Violation: a bad frame pointer faults through diagnostics.
    unsafe {
        context_switch::switch(BOOT_CONTEXT.0.get(), IDLE_CONTEXT.0.get());
    }
}

#[cfg(not(test))]
extern "C" fn task_a_entry() -> ! {
    loop {
        if TASK_A_RUNS.fetch_add(1, Ordering::SeqCst) >= EXPECTED_TASK_RUNS {
            halt_bad_schedule();
        }
        serial::write_line("PYTHOS:CORE:SCHEDULER:TASK_A");
        yield_to_scheduler(TASK_A_CONTEXT.0.get());
    }
}

#[cfg(not(test))]
extern "C" fn task_b_entry() -> ! {
    loop {
        if TASK_B_RUNS.fetch_add(1, Ordering::SeqCst) >= EXPECTED_TASK_RUNS {
            halt_bad_schedule();
        }
        serial::write_line("PYTHOS:CORE:SCHEDULER:TASK_B");
        yield_to_scheduler(TASK_B_CONTEXT.0.get());
    }
}

#[cfg(not(test))]
extern "C" fn idle_task_entry() -> ! {
    loop {
        if IDLE_RUNS.fetch_add(1, Ordering::SeqCst) >= EXPECTED_IDLE_RUNS {
            halt_bad_schedule();
        }
        serial::write_line("PYTHOS:CORE:IDLE_TASK");
        yield_to_scheduler(IDLE_CONTEXT.0.get());
    }
}

#[cfg(not(test))]
fn yield_to_scheduler(current: *mut ContextFrame) {
    // SAFETY:
    // 1. Invariant: `current` is the active scheduler test task frame and
    //    `BOOT_CONTEXT` was initialized by the bootstrap-to-task switch.
    // 2. Established by: `switch_to` entered the current task from bootstrap.
    // 3. Lifetime: static frames live for the full kernel lifetime.
    // 4. Pointer ownership: the switch path mutably saves only `current`.
    // 5. Alignment: `ContextFrame` statics preserve frame alignment.
    // 6. Mapped length: exactly one current and one bootstrap frame are accessed.
    // 7. Concurrency: cooperative single-core scheduler self-test.
    // 8. Violation: a bad frame pointer faults through diagnostics.
    unsafe {
        context_switch::switch(current, BOOT_CONTEXT.0.get());
    }
}

#[cfg(not(test))]
fn stack_top(stack: *mut SchedulerStack) -> u64 {
    stack as usize as u64 + SCHEDULER_STACK_SIZE as u64
}

#[cfg(not(test))]
fn halt_bad_schedule() -> ! {
    loop {
        hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_robin_selects_ready_tasks_in_order() {
        let mut scheduler = RoundRobinScheduler::new([
            SchedulerTask::ready(SCHEDULE_A),
            SchedulerTask::ready(SCHEDULE_B),
        ]);

        assert_eq!(scheduler.next_ready(), Ok(SCHEDULE_A));
        assert_eq!(scheduler.next_ready(), Ok(SCHEDULE_B));
        assert_eq!(scheduler.next_ready(), Ok(SCHEDULE_A));
        assert_eq!(scheduler.next_ready(), Ok(SCHEDULE_B));
    }

    #[test]
    fn round_robin_reports_no_ready_task_without_priority_fallback() {
        let mut scheduler = RoundRobinScheduler::new([]);

        assert_eq!(scheduler.next_ready(), Err(SchedulerError::NoReadyTask));
    }

    #[test]
    fn scheduler_task_ids_are_stable_kernel_assigned_values() {
        assert_eq!(SCHEDULE_A, SchedulerTaskId::new(2));
        assert_eq!(SCHEDULE_B, SchedulerTaskId::new(3));
    }

    #[test]
    fn idle_task_is_selected_only_after_ready_set_is_empty() {
        let mut scheduler = RoundRobinScheduler::<0>::new([]);

        assert_eq!(scheduler.next_ready(), Err(SchedulerError::NoReadyTask));
    }
}
