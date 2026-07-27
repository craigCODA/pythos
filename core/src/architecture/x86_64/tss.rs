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

// Task 11: a dedicated fault-delivery stack for IST1, used by the #DF (8) and
// #PF (14) IDT gates (see idt.rs). Faults routed through IST always switch to
// this stack regardless of the current RSP's validity, so a syscall-stack
// overflow that faults against its guard page can still be diagnosed and
// reported instead of cascading into a double/triple fault on a stack that
// just overran its buffer.
#[cfg(not(test))]
const IST1_STACK_BYTES: usize = 16 * 1024;

#[cfg(not(test))]
#[repr(align(16))]
struct Ist1Stack(#[allow(dead_code)] [u8; IST1_STACK_BYTES]);

#[cfg(not(test))]
struct Ist1StackStorage(UnsafeCell<Ist1Stack>);

// SAFETY:
// 1. Invariant: this stack is only ever used implicitly by the CPU as an IST
//    target during #DF/#PF delivery; PythCore never reads or writes it
//    directly outside of that hardware-driven stack switch.
// 2. Established by: `idt::initialize` sets `ist = 1` only for vectors 8 and
//    14, and `set_ist1_stack` below installs this buffer's top as IST1.
// 3. Lifetime: static for all of PythCore.
// 4. Pointer ownership: the CPU exclusively uses this region as a stack
//    during fault delivery; no Rust code holds a live reference into it.
// 5. Alignment: `repr(align(16))` matches the ABI's stack alignment.
// 6. Mapped length: `IST1_STACK_BYTES` bytes are mapped kernel data.
// 7. Concurrency: single-core boot; only one fault can be in flight at once.
// 8. Violation: an unmapped or too-small IST1 stack would fault again while
//    already handling a fault, defeating its purpose.
#[cfg(not(test))]
unsafe impl Sync for Ist1StackStorage {}

#[cfg(not(test))]
static IST1_STACK: Ist1StackStorage =
    Ist1StackStorage(UnsafeCell::new(Ist1Stack([0; IST1_STACK_BYTES])));

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

#[cfg(not(test))]
pub fn initialize_ist1() {
    // SAFETY:
    // 1. Invariant: IST1's top is the highest address of a static, mapped,
    //    16 KiB-aligned-enough stack that PythCore never otherwise uses.
    // 2. Established by: `IST1_STACK` is a static buffer with no other
    //    readers/writers; this runs once during single-core boot before any
    //    IST-routed fault can occur.
    // 3. Lifetime: the buffer is static for all of PythCore.
    // 4. Pointer ownership: PythCore writes this field once; the CPU reads it
    //    on every subsequent #DF/#PF delivery.
    // 5. Alignment: the computed top is 16-byte aligned by construction.
    // 6. Mapped length: the full `IST1_STACK_BYTES` region backs this pointer.
    // 7. Concurrency: single-core boot; no concurrent TSS mutation.
    // 8. Violation: a bad IST1 top would fault (or corrupt memory) the moment
    //    any routed exception is delivered.
    unsafe {
        let top = (IST1_STACK.0.get() as u64) + IST1_STACK_BYTES as u64;
        (*TSS.0.get()).ist[0] = top;
    }
}

#[cfg(test)]
pub fn initialize_ist1() {}
