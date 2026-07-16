//! Task-state segment storage for milestone-1 exception entry support.

use core::cell::UnsafeCell;

#[repr(C, packed)]
pub struct TaskStateSegment {
    _reserved_0: u32,
    rsp: [u64; 3],
    _reserved_1: u64,
    ist: [u64; 7],
    _reserved_2: u64,
    _reserved_3: u16,
    iomap_base: u16,
}

impl TaskStateSegment {
    const fn new() -> Self {
        Self {
            _reserved_0: 0,
            rsp: [0; 3],
            _reserved_1: 0,
            ist: [0; 7],
            _reserved_2: 0,
            _reserved_3: 0,
            iomap_base: core::mem::size_of::<TaskStateSegment>() as u16,
        }
    }
}

struct TssStorage(UnsafeCell<TaskStateSegment>);

// SAFETY:
// 1. Invariant: the TSS is initialized once and then only its RSP0 field is
//    updated during single-core boot before the ring-3 proof is entered.
// 2. Established by: Phase 8 still has no SMP or concurrent user tasks.
// 3. Lifetime: the TSS storage is static for all of PythCore.
// 4. Pointer ownership: PythCore exclusively owns and mutates this TSS.
// 5. Alignment: `UnsafeCell<TaskStateSegment>` preserves the TSS alignment.
// 6. Mapped length: exactly one `TaskStateSegment` is stored.
// 7. Concurrency: single-core boot and no concurrent TSS mutation.
// 8. Violation: a bad RSP0 would fault during ring-3 to ring-0 entry.
unsafe impl Sync for TssStorage {}

static TSS: TssStorage = TssStorage(UnsafeCell::new(TaskStateSegment::new()));

pub fn base() -> u64 {
    TSS.0.get() as u64
}

pub fn limit() -> u32 {
    (core::mem::size_of::<TaskStateSegment>() - 1) as u32
}

#[cfg(not(test))]
pub fn set_ring0_stack(rsp0: u64) {
    // SAFETY:
    // 1. Invariant: `rsp0` names the top of a mapped kernel-owned stack usable
    //    for privilege transitions from ring 3 to ring 0.
    // 2. Established by: caller supplies a static PythCore trap stack.
    // 3. Lifetime: the stack remains mapped for the full ring-3 proof.
    // 4. Pointer ownership: the CPU consumes the stack pointer through the TSS.
    // 5. Alignment: callers provide a 16-byte aligned top-of-stack value.
    // 6. Mapped length: the backing stack has at least one page.
    // 7. Concurrency: single-core boot; no simultaneous TSS updates.
    // 8. Violation: a bad stack pointer causes a fault on user-to-kernel entry.
    unsafe {
        (*TSS.0.get()).rsp[0] = rsp0;
    }
}
