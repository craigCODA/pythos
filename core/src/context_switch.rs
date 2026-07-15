//! Cooperative context-switch self-test for Phase 2.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
#[cfg(not(test))]
use core::cell::UnsafeCell;
#[cfg(not(test))]
use core::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(test))]
use core::{arch::global_asm, hint};

#[cfg(not(test))]
const TEST_STACK_SIZE: usize = 16 * 4096;
#[cfg(not(test))]
const EXPECTED_SWITCH_STEPS: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextFrame {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rdi: u64,
    rsi: u64,
    rbp: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
    rsp: u64,
    rip: u64,
    rflags: u64,
}

impl ContextFrame {
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
            rdi: 0,
            rsi: 0,
            rbp: 0,
            rdx: 0,
            rcx: 0,
            rbx: 0,
            rax: 0,
            rsp: 0,
            rip: 0,
            rflags: 0x202,
        }
    }

    #[cfg(not(test))]
    pub fn new(entry: extern "C" fn() -> !, stack_top: u64) -> Self {
        let mut frame = Self::empty();
        frame.rsp = stack_top - 8;
        frame.rip = entry as usize as u64;
        frame
    }
}

#[cfg(not(test))]
#[repr(align(16))]
#[allow(dead_code)]
struct TaskStack([u8; TEST_STACK_SIZE]);

#[cfg(not(test))]
struct TaskStackStorage(UnsafeCell<TaskStack>);

#[cfg(not(test))]
// SAFETY: context-switch self-test stacks are initialized during single-core
// boot, and only the active context mutates its own stack.
unsafe impl Sync for TaskStackStorage {}

#[cfg(not(test))]
struct ContextStorage(UnsafeCell<ContextFrame>);

#[cfg(not(test))]
// SAFETY: each context frame is mutated only by the cooperative switch path
// while Phase 2 still has no scheduler, SMP, or concurrent task migration.
unsafe impl Sync for ContextStorage {}

#[cfg(not(test))]
static TASK_A_STACK: TaskStackStorage =
    TaskStackStorage(UnsafeCell::new(TaskStack([0; TEST_STACK_SIZE])));
#[cfg(not(test))]
static TASK_B_STACK: TaskStackStorage =
    TaskStackStorage(UnsafeCell::new(TaskStack([0; TEST_STACK_SIZE])));
#[cfg(not(test))]
static BOOT_CONTEXT: ContextStorage = ContextStorage(UnsafeCell::new(ContextFrame::empty()));
#[cfg(not(test))]
static TASK_A_CONTEXT: ContextStorage = ContextStorage(UnsafeCell::new(ContextFrame::empty()));
#[cfg(not(test))]
static TASK_B_CONTEXT: ContextStorage = ContextStorage(UnsafeCell::new(ContextFrame::empty()));
#[cfg(not(test))]
static SWITCH_STEP: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextSwitchError {
    BadSwitchOrder,
}

#[cfg(not(test))]
unsafe extern "C" {
    fn context_switch_abi(current: *mut ContextFrame, next: *const ContextFrame);
}

#[cfg(not(test))]
pub unsafe fn switch(current: *mut ContextFrame, next: *const ContextFrame) {
    // SAFETY:
    // 1. Invariant: `current` points to a writable mapped `ContextFrame`, and
    //    `next` points to a readable mapped `ContextFrame` with valid `RSP`,
    //    `RIP`, and `RFLAGS` fields for kernel-mode execution.
    // 2. Established by: caller-owned context initialization for the active
    //    boot slice before invoking this low-level switch helper.
    // 3. Lifetime: both frames and the selected stack must remain live until
    //    the switched-away context is resumed or intentionally abandoned.
    // 4. Pointer ownership: the switch path mutably saves only `current` and
    //    reads `next`; no other writer may mutate either during the switch.
    // 5. Alignment: `ContextFrame` uses `repr(C)` and caller supplies aligned
    //    frame storage; `next.rsp` must satisfy the callee entry contract.
    // 6. Mapped length: exactly one `ContextFrame` at each pointer and the
    //    active stack range named by `next.rsp` are mapped.
    // 7. Concurrency: single-core cooperative switching; no concurrent task
    //    migration, SMP, or scheduler mutation exists in this slice.
    // 8. Violation: bad frames, stale stack pointers, or unmapped contexts
    //    fault through the diagnostic exception path.
    unsafe {
        context_switch_abi(current, next);
    }
}

#[cfg(not(test))]
global_asm!(
    r#"
    .global context_switch_abi
    context_switch_abi:
        mov [rdi + 0], r15
        mov [rdi + 8], r14
        mov [rdi + 16], r13
        mov [rdi + 24], r12
        mov [rdi + 32], r11
        mov [rdi + 40], r10
        mov [rdi + 48], r9
        mov [rdi + 56], r8
        mov [rdi + 64], rdi
        mov [rdi + 72], rsi
        mov [rdi + 80], rbp
        mov [rdi + 88], rdx
        mov [rdi + 96], rcx
        mov [rdi + 104], rbx
        mov [rdi + 112], rax
        mov [rdi + 120], rsp
        lea rax, [rip + .Lcontext_switch_resume]
        mov [rdi + 128], rax
        pushfq
        pop qword ptr [rdi + 136]

        push qword ptr [rsi + 136]
        popfq
        mov rcx, [rsi + 128]
        mov rdx, [rsi + 120]
        mov r15, [rsi + 0]
        mov r14, [rsi + 8]
        mov r13, [rsi + 16]
        mov r12, [rsi + 24]
        mov r11, [rsi + 32]
        mov r10, [rsi + 40]
        mov r9, [rsi + 48]
        mov r8, [rsi + 56]
        mov rdi, [rsi + 64]
        mov rbp, [rsi + 80]
        mov rbx, [rsi + 104]
        mov rax, [rsi + 112]
        mov rsp, rdx
        push rcx
        mov rcx, [rsi + 96]
        mov rdx, [rsi + 88]
        mov rsi, [rsi + 72]
        ret

    .Lcontext_switch_resume:
        ret
    "#
);

#[cfg(not(test))]
pub fn run_self_test() -> Result<(), ContextSwitchError> {
    SWITCH_STEP.store(0, Ordering::SeqCst);
    // SAFETY:
    // 1. Invariant: context frames and test stacks are static, mapped, aligned,
    //    and not used by any scheduler or interrupt path.
    // 2. Established by: Phase 2 has not introduced scheduling, SMP, or task
    //    migration, and these statics are private to this module.
    // 3. Lifetime: all frames and stacks live for the full kernel lifetime.
    // 4. Pointer ownership: the self-test exclusively owns these contexts now.
    // 5. Alignment: `TaskStack` is 16-byte aligned; context frames use `repr(C)`.
    // 6. Mapped length: each stack has `TEST_STACK_SIZE` bytes; each frame is
    //    exactly one `ContextFrame`.
    // 7. Concurrency: cooperative single-core execution; interrupts do not
    //    access these frames.
    // 8. Violation: a bad frame or stack pointer faults through diagnostics.
    unsafe {
        *TASK_A_CONTEXT.0.get() = ContextFrame::new(task_a_entry, stack_top(TASK_A_STACK.0.get()));
        *TASK_B_CONTEXT.0.get() = ContextFrame::new(task_b_entry, stack_top(TASK_B_STACK.0.get()));
        switch(BOOT_CONTEXT.0.get(), TASK_A_CONTEXT.0.get());
    }
    if SWITCH_STEP.load(Ordering::SeqCst) != EXPECTED_SWITCH_STEPS {
        return Err(ContextSwitchError::BadSwitchOrder);
    }
    Ok(())
}

#[cfg(not(test))]
extern "C" fn task_a_entry() -> ! {
    if SWITCH_STEP.compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst) != Ok(0) {
        halt_bad_switch();
    }
    serial::write_line("PYTHOS:CORE:CONTEXT_SWITCH:TASK_A");
    // SAFETY:
    // 1. Invariant: task A and task B context frames are initialized and mapped.
    // 2. Established by: `run_self_test` before entering task A.
    // 3. Lifetime: static frames live for the full kernel lifetime.
    // 4. Pointer ownership: the switch path mutably saves only the current task.
    // 5. Alignment: `ContextFrame` statics preserve frame alignment.
    // 6. Mapped length: exactly one current and one next frame are accessed.
    // 7. Concurrency: cooperative single-core context switch.
    // 8. Violation: a bad frame pointer faults through diagnostics.
    unsafe {
        switch(TASK_A_CONTEXT.0.get(), TASK_B_CONTEXT.0.get());
    }

    if SWITCH_STEP.compare_exchange(2, 3, Ordering::SeqCst, Ordering::SeqCst) != Ok(2) {
        halt_bad_switch();
    }
    serial::write_line("PYTHOS:CORE:CONTEXT_SWITCH:TASK_A");
    // SAFETY:
    // 1. Invariant: task A and task B context frames are still initialized and mapped.
    // 2. Established by: the first task B switch returning to task A.
    // 3. Lifetime: static frames live for the full kernel lifetime.
    // 4. Pointer ownership: the switch path mutably saves only task A.
    // 5. Alignment: `ContextFrame` statics preserve frame alignment.
    // 6. Mapped length: exactly one current and one next frame are accessed.
    // 7. Concurrency: cooperative single-core context switch.
    // 8. Violation: a bad frame pointer faults through diagnostics.
    unsafe {
        switch(TASK_A_CONTEXT.0.get(), TASK_B_CONTEXT.0.get());
    }
    halt_bad_switch();
}

#[cfg(not(test))]
extern "C" fn task_b_entry() -> ! {
    if SWITCH_STEP.compare_exchange(1, 2, Ordering::SeqCst, Ordering::SeqCst) != Ok(1) {
        halt_bad_switch();
    }
    serial::write_line("PYTHOS:CORE:CONTEXT_SWITCH:TASK_B");
    // SAFETY:
    // 1. Invariant: task B and task A context frames are initialized and mapped.
    // 2. Established by: `run_self_test` before entering task A, which switched here.
    // 3. Lifetime: static frames live for the full kernel lifetime.
    // 4. Pointer ownership: the switch path mutably saves only the current task.
    // 5. Alignment: `ContextFrame` statics preserve frame alignment.
    // 6. Mapped length: exactly one current and one next frame are accessed.
    // 7. Concurrency: cooperative single-core context switch.
    // 8. Violation: a bad frame pointer faults through diagnostics.
    unsafe {
        switch(TASK_B_CONTEXT.0.get(), TASK_A_CONTEXT.0.get());
    }

    if SWITCH_STEP.compare_exchange(3, 4, Ordering::SeqCst, Ordering::SeqCst) != Ok(3) {
        halt_bad_switch();
    }
    serial::write_line("PYTHOS:CORE:CONTEXT_SWITCH:TASK_B");
    // SAFETY:
    // 1. Invariant: task B and bootstrap context frames are initialized and mapped.
    // 2. Established by: the original bootstrap-to-task switch saved `BOOT_CONTEXT`.
    // 3. Lifetime: static frames live for the full kernel lifetime.
    // 4. Pointer ownership: the switch path mutably saves only task B.
    // 5. Alignment: `ContextFrame` statics preserve frame alignment.
    // 6. Mapped length: exactly one current and one next frame are accessed.
    // 7. Concurrency: cooperative single-core context switch.
    // 8. Violation: a bad frame pointer faults through diagnostics.
    unsafe {
        switch(TASK_B_CONTEXT.0.get(), BOOT_CONTEXT.0.get());
    }
    halt_bad_switch();
}

#[cfg(not(test))]
fn stack_top(stack: *mut TaskStack) -> u64 {
    stack as usize as u64 + TEST_STACK_SIZE as u64
}

#[cfg(not(test))]
fn halt_bad_switch() -> ! {
    loop {
        hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_frame_layout_matches_assembly_offsets() {
        assert_eq!(core::mem::offset_of!(ContextFrame, r15), 0);
        assert_eq!(core::mem::offset_of!(ContextFrame, rdi), 64);
        assert_eq!(core::mem::offset_of!(ContextFrame, rdx), 88);
        assert_eq!(core::mem::offset_of!(ContextFrame, rax), 112);
        assert_eq!(core::mem::offset_of!(ContextFrame, rsp), 120);
        assert_eq!(core::mem::offset_of!(ContextFrame, rip), 128);
        assert_eq!(core::mem::offset_of!(ContextFrame, rflags), 136);
    }

    #[test]
    fn empty_context_starts_with_interrupt_flag_set() {
        assert_eq!(ContextFrame::empty().rflags, 0x202);
    }
}
