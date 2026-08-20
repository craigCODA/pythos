pub const PYTH_COMMAND_ABI_MAJOR: u16 = 1;
pub const PYTH_COMMAND_ABI_MINOR: u16 = 0;

pub const COMMAND_KIND_LIST_OBJECTS: u16 = 1;
pub const COMMAND_KIND_INSPECT_OBJECT: u16 = 2;
pub const COMMAND_KIND_CREATE_NOTE: u16 = 3;
pub const COMMAND_KIND_REVISE_NOTE: u16 = 4;
pub const COMMAND_KIND_LIST_TASKS: u16 = 5;
pub const COMMAND_KIND_CREATE_TASK: u16 = 6;
pub const COMMAND_KIND_LIST_PROPOSALS: u16 = 7;
pub const COMMAND_KIND_APPROVE_PROPOSAL: u16 = 8;
pub const COMMAND_KIND_SUSPEND_TASK: u16 = 9;
pub const COMMAND_KIND_REVIVE_TASK: u16 = 10;
pub const COMMAND_KIND_SYSTEM_STATUS: u16 = 11;
pub const COMMAND_KIND_REBOOT: u16 = 12;

pub const COMMAND_RESULT_STATUS_OK: u16 = 0;
pub const COMMAND_RESULT_STATUS_DENIED: u16 = 1;
pub const COMMAND_RESULT_STATUS_FAILED: u16 = 2;

pub const COMMAND_FIELD_KIND: u32 = 0;
pub const COMMAND_FIELD_OBJECT_ID: u32 = 1;
pub const COMMAND_FIELD_TASK_ID: u32 = 2;
pub const COMMAND_FIELD_PROPOSAL_ID: u32 = 3;
pub const COMMAND_FIELD_TEXT_UTF8: u32 = 4;

pub const COMMAND_FLAG_NONE: u64 = 0;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythCommand {
    pub abi_major: u16,
    pub abi_minor: u16,
    pub kind: u16,
    pub reserved0: u16,
    pub object_id: u64,
    pub task_id: u64,
    pub proposal_id: u64,
    pub payload_ptr: u64,
    pub payload_len: u64,
    pub flags: u64,
    pub reserved1: u64,
}

impl PythCommand {
    pub const fn empty(kind: u16) -> Self {
        Self {
            abi_major: PYTH_COMMAND_ABI_MAJOR,
            abi_minor: PYTH_COMMAND_ABI_MINOR,
            kind,
            reserved0: 0,
            object_id: 0,
            task_id: 0,
            proposal_id: 0,
            payload_ptr: 0,
            payload_len: 0,
            flags: COMMAND_FLAG_NONE,
            reserved1: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythCommandResult {
    pub status: u16,
    pub kind: u16,
    pub reserved0: u32,
    pub object_id: u64,
    pub task_id: u64,
    pub proposal_id: u64,
    pub bytes_written: u64,
    pub reserved1: u64,
}

impl PythCommandResult {
    pub const fn empty(status: u16, kind: u16) -> Self {
        Self {
            status,
            kind,
            reserved0: 0,
            object_id: 0,
            task_id: 0,
            proposal_id: 0,
            bytes_written: 0,
            reserved1: 0,
        }
    }
}

pub const fn command_kind_is_known(kind: u16) -> bool {
    matches!(kind, 1..=12)
}
