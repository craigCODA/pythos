pub const TASK_ABI_MAJOR: u16 = 1;
pub const TASK_ABI_MINOR: u16 = 0;
pub const SYSCALL_TASK_REQUEST: u64 = 0x5059_0140;

pub const OBJECT_KIND_TASK: u16 = 20;
pub const OBJECT_KIND_TASK_PROPOSAL: u16 = 21;
pub const OBJECT_KIND_TASK_EVENT: u16 = 22;
pub const OBJECT_KIND_TASK_RELATION: u16 = 23;
pub const OBJECT_KIND_RELEVANCE_ASSERTION: u16 = 24;
pub const OBJECT_KIND_CAPABILITY_REQUEST: u16 = 25;

pub const OP_CREATE_TASK: u16 = 1;
pub const OP_READ_ACTIVE_TASK: u16 = 2;
pub const OP_APPEND_TASK_EVENT: u16 = 3;
pub const OP_CREATE_PROPOSAL: u16 = 4;
pub const OP_LIST_PROPOSALS: u16 = 5;
pub const OP_APPROVE_PROPOSAL: u16 = 6;
pub const OP_REJECT_PROPOSAL: u16 = 7;
pub const OP_SUSPEND_TASK: u16 = 8;
pub const OP_REVIVE_TASK: u16 = 9;
pub const OP_COMPLETE_TASK: u16 = 10;
pub const OP_ABANDON_TASK: u16 = 11;
pub const OP_READ_CONTEXT_SUMMARY: u16 = 12;

pub const TASK_RIGHT_READ_CONTEXT: u64 = 0x0001;
pub const TASK_RIGHT_APPEND_EVENT: u64 = 0x0010;
pub const TASK_RIGHT_CREATE_PROPOSAL: u64 = 0x0008;
pub const TASK_RIGHT_APPROVE_PROPOSAL: u64 = 0x0020;
pub const TASK_RIGHT_CONTROL_STATE: u64 = 0x0040;

pub const TASK_CONTEXT_RESULT_ACTIVE_TASK_ID: u32 = 0;
pub const TASK_CONTEXT_RESULT_CANDIDATE_TASK_ID: u32 = 1;
pub const TASK_CONTEXT_RESULT_CONFIDENCE_SCORE: u32 = 2;
pub const TASK_CONTEXT_RESULT_PROPOSAL_KIND: u32 = 3;
pub const TASK_CONTEXT_RESULT_REASON_UTF8: u32 = 4;
pub const MAX_TASK_PROPOSAL_RESULTS: usize = 4;
pub const TASK_REQUEST_SUSPEND_CURRENT: u64 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum TaskStatus {
    Active = 1,
    Suspended = 2,
    Completed = 3,
    Abandoned = 4,
}

impl TaskStatus {
    pub const fn code(self) -> u16 {
        self as u16
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum TaskProposalKind {
    NewTask = 1,
    Continuation = 2,
    Child = 3,
    Branch = 4,
    Related = 5,
}

impl TaskProposalKind {
    pub const fn code(self) -> u16 {
        self as u16
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskRequest {
    pub abi_major: u16,
    pub abi_minor: u16,
    pub operation: u16,
    pub proposal_kind: u16,
    pub authority: u64,
    pub task_id: u64,
    pub proposal_id: u64,
    pub target_task_id: u64,
    pub input_ptr: u64,
    pub input_len: u64,
    pub output_ptr: u64,
    pub output_len: u64,
    pub flags: u64,
    pub score: u64,
    pub reserved0: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskResponse {
    pub status: u16,
    pub operation: u16,
    pub proposal_kind: u16,
    pub reserved0: u16,
    pub task_id: u64,
    pub proposal_id: u64,
    pub active_task_id: u64,
    pub bytes_written: u64,
    pub score: u64,
    pub reserved1: u64,
    pub reserved2: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskEventInput {
    pub tag_hash: u64,
    pub object_kind: u16,
    pub tool_domain: u16,
    pub flags: u16,
    pub reserved0: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskProposalListEntry {
    pub status: u16,
    pub proposal_kind: u16,
    pub reserved0: u32,
    pub proposal_id: u64,
    pub target_task_id: u64,
    pub candidate_task_id: u64,
    pub score: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskContextSummary {
    pub active_task_id: u64,
    pub matching_suspended_task_id: u64,
    pub dominant_object_kind: u16,
    pub dominant_tool_domain: u16,
    pub proposal_kind: u16,
    pub event_count: u16,
    pub active_match_count: u16,
    pub candidate_match_count: u16,
    pub tool_domain_changed: u16,
    pub reserved0: u16,
    pub confidence_score: u64,
    pub candidate_tag_hash: u64,
    pub source_event_ids: [u64; 4],
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pyth_tig::opcode::{
        Opcode, RESOURCE_TASK, RIGHTS_APPEND, RIGHTS_APPROVE, RIGHTS_CONTROL, RIGHTS_CREATE,
        RIGHTS_READ,
    };
    use crate::pyth_tig::types::PythType;

    #[test]
    fn task_codes_and_layouts_are_stable() {
        assert_eq!(TASK_ABI_MAJOR, 1);
        assert_eq!(SYSCALL_TASK_REQUEST, 0x5059_0140);
        assert_eq!(OBJECT_KIND_TASK, 20);
        assert_eq!(OBJECT_KIND_TASK_PROPOSAL, 21);
        assert_eq!(OBJECT_KIND_TASK_EVENT, 22);
        assert_eq!(OBJECT_KIND_TASK_RELATION, 23);
        assert_eq!(OBJECT_KIND_RELEVANCE_ASSERTION, 24);
        assert_eq!(OBJECT_KIND_CAPABILITY_REQUEST, 25);
        assert_eq!(TaskStatus::Active.code(), 1);
        assert_eq!(TaskStatus::Suspended.code(), 2);
        assert_eq!(TaskProposalKind::NewTask.code(), 1);
        assert_eq!(TaskProposalKind::Continuation.code(), 2);
        assert_eq!(TaskProposalKind::Child.code(), 3);
        assert_eq!(TaskProposalKind::Branch.code(), 4);
        assert_eq!(TaskProposalKind::Related.code(), 5);
        assert_eq!(OP_CREATE_TASK, 1);
        assert_eq!(OP_READ_ACTIVE_TASK, 2);
        assert_eq!(OP_APPEND_TASK_EVENT, 3);
        assert_eq!(OP_CREATE_PROPOSAL, 4);
        assert_eq!(OP_LIST_PROPOSALS, 5);
        assert_eq!(OP_APPROVE_PROPOSAL, 6);
        assert_eq!(OP_REJECT_PROPOSAL, 7);
        assert_eq!(OP_SUSPEND_TASK, 8);
        assert_eq!(OP_REVIVE_TASK, 9);
        assert_eq!(OP_COMPLETE_TASK, 10);
        assert_eq!(OP_ABANDON_TASK, 11);
        assert_eq!(OP_READ_CONTEXT_SUMMARY, 12);
        assert_eq!(MAX_TASK_PROPOSAL_RESULTS, 4);
        assert_eq!(TASK_REQUEST_SUSPEND_CURRENT, 1);
        assert_eq!(core::mem::size_of::<TaskRequest>(), 96);
        assert_eq!(core::mem::size_of::<TaskResponse>(), 64);
        assert_eq!(core::mem::size_of::<TaskEventInput>(), 16);
        assert_eq!(core::mem::size_of::<TaskProposalListEntry>(), 40);
        assert_eq!(core::mem::size_of::<TaskContextSummary>(), 80);
    }

    #[test]
    fn task_rights_are_explicit_capability_bits() {
        assert_eq!(TASK_RIGHT_READ_CONTEXT, RIGHTS_READ);
        assert_eq!(
            TASK_RIGHT_APPEND_EVENT,
            crate::pyth_tig::opcode::RIGHTS_APPEND
        );
        assert_eq!(TASK_RIGHT_CREATE_PROPOSAL, RIGHTS_CREATE);
        assert_eq!(TASK_RIGHT_APPROVE_PROPOSAL, RIGHTS_APPROVE);
        assert_eq!(TASK_RIGHT_CONTROL_STATE, RIGHTS_CONTROL);
    }

    #[test]
    fn task_context_opcode_signature_is_stable() {
        let signature = Opcode::TaskContextRead.signature();

        assert_eq!(Opcode::TaskContextRead.code(), 0x1205);
        assert_eq!(signature.input_count, 2);
        assert_eq!(signature.inputs[0], PythType::Effect);
        assert_eq!(signature.inputs[1], PythType::Capability);
        assert_eq!(signature.required_resource_kind, Some(RESOURCE_TASK));
        assert_eq!(signature.required_rights, TASK_RIGHT_READ_CONTEXT);
    }

    #[test]
    fn task_proposal_emit_signature_is_proposal_only() {
        let signature = Opcode::TaskProposalEmit.signature();

        assert_eq!(Opcode::TaskProposalEmit.code(), 0x1201);
        assert_eq!(signature.input_count, 4);
        assert_eq!(signature.inputs[0], PythType::Effect);
        assert_eq!(signature.inputs[1], PythType::Capability);
        assert_eq!(signature.inputs[2], PythType::TaskId);
        assert_eq!(signature.inputs[3], PythType::U64);
        assert_eq!(signature.required_resource_kind, Some(RESOURCE_TASK));
        assert_eq!(signature.required_rights, TASK_RIGHT_CREATE_PROPOSAL);
        assert_ne!(signature.required_rights, RIGHTS_APPEND);
        assert_ne!(signature.required_rights, RIGHTS_APPROVE);
        assert_ne!(signature.required_rights, RIGHTS_CONTROL);
    }

    #[test]
    fn task_context_field_offsets_are_stable() {
        assert_eq!(core::mem::align_of::<TaskContextSummary>(), 8);
        assert_eq!(core::mem::offset_of!(TaskContextSummary, active_task_id), 0);
        assert_eq!(
            core::mem::offset_of!(TaskContextSummary, matching_suspended_task_id),
            8
        );
        assert_eq!(
            core::mem::offset_of!(TaskContextSummary, dominant_object_kind),
            16
        );
        assert_eq!(
            core::mem::offset_of!(TaskContextSummary, confidence_score),
            32
        );
        assert_eq!(
            core::mem::offset_of!(TaskContextSummary, candidate_tag_hash),
            40
        );
        assert_eq!(
            core::mem::offset_of!(TaskContextSummary, source_event_ids),
            48
        );
    }

    #[test]
    fn task_event_input_offsets_are_stable() {
        assert_eq!(core::mem::align_of::<TaskEventInput>(), 8);
        assert_eq!(core::mem::offset_of!(TaskEventInput, tag_hash), 0);
        assert_eq!(core::mem::offset_of!(TaskEventInput, object_kind), 8);
        assert_eq!(core::mem::offset_of!(TaskEventInput, tool_domain), 10);
        assert_eq!(core::mem::offset_of!(TaskEventInput, flags), 12);
        assert_eq!(core::mem::offset_of!(TaskEventInput, reserved0), 14);
    }

    #[test]
    fn task_proposal_list_entry_offsets_are_stable() {
        assert_eq!(core::mem::align_of::<TaskProposalListEntry>(), 8);
        assert_eq!(core::mem::offset_of!(TaskProposalListEntry, status), 0);
        assert_eq!(
            core::mem::offset_of!(TaskProposalListEntry, proposal_kind),
            2
        );
        assert_eq!(core::mem::offset_of!(TaskProposalListEntry, reserved0), 4);
        assert_eq!(core::mem::offset_of!(TaskProposalListEntry, proposal_id), 8);
        assert_eq!(
            core::mem::offset_of!(TaskProposalListEntry, target_task_id),
            16
        );
        assert_eq!(
            core::mem::offset_of!(TaskProposalListEntry, candidate_task_id),
            24
        );
        assert_eq!(core::mem::offset_of!(TaskProposalListEntry, score), 32);
    }
}
