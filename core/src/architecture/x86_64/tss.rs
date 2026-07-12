//! Task-state segment storage for milestone-1 exception entry support.

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

static TSS: TaskStateSegment = TaskStateSegment::new();

pub fn base() -> u64 {
    (&TSS as *const TaskStateSegment) as u64
}

pub fn limit() -> u32 {
    (core::mem::size_of::<TaskStateSegment>() - 1) as u32
}
