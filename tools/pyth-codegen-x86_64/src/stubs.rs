use pythos_shared::{
    object_shell_abi::{
        FIELD_TEXT, MAX_QUERY_RESULTS, OBJECT_KIND_NOTE, OBJECT_SHELL_ABI_MAJOR,
        OBJECT_SHELL_ABI_MINOR, OP_CREATE_OBJECT, OP_GET_HISTORY, OP_INSPECT_OBJECT,
        OP_QUERY_OBJECTS, OP_REVISE_FIELD, ObjectListEntry, ObjectShellRequest,
        ObjectShellResponse, SYSCALL_OBJECT_REQUEST, SYSCALL_OK,
    },
    pyth_runtime_abi::{
        HOST_RESULT_CAPABILITY, HOST_RESULT_OBJECT_ID, HOST_RESULT_REVISION, HOST_RESULT_STATUS,
        HOST_RESULT_UTF8, HostCallResult, SYSCALL_PYTH_GRAPH_LOG,
    },
    pyth_tig::opcode::Opcode,
    task_abi::{
        OP_CREATE_PROPOSAL, OP_READ_CONTEXT_SUMMARY, SYSCALL_TASK_REQUEST, TASK_ABI_MAJOR,
        TASK_ABI_MINOR, TASK_CONTEXT_RESULT_ACTIVE_TASK_ID, TASK_CONTEXT_RESULT_CANDIDATE_TASK_ID,
        TASK_CONTEXT_RESULT_CONFIDENCE_SCORE, TASK_CONTEXT_RESULT_PROPOSAL_KIND,
        TASK_CONTEXT_RESULT_REASON_UTF8, TaskContextSummary, TaskProposalKind, TaskRequest,
        TaskResponse,
    },
};

use crate::{CodegenError, Result};

pub const OBJECT_REQUEST_MARKER: &[u8] = b"PYTH_OBJECT_REQUEST\0";
pub const TASK_REQUEST_MARKER: &[u8] = b"PYTH_TASK_REQUEST\0";

pub const OBJECT_REQUEST_BYTES: usize = core::mem::size_of::<ObjectShellRequest>();
pub const OBJECT_RESPONSE_BYTES: usize = core::mem::size_of::<ObjectShellResponse>();
pub const OBJECT_QUERY_OUTPUT_BYTES: usize =
    core::mem::size_of::<ObjectListEntry>() * MAX_QUERY_RESULTS;
pub const TASK_REQUEST_BYTES: usize = core::mem::size_of::<TaskRequest>();
pub const TASK_RESPONSE_BYTES: usize = core::mem::size_of::<TaskResponse>();
pub const TASK_CONTEXT_BYTES: usize = core::mem::size_of::<TaskContextSummary>();
pub const HOST_CALL_RESULT_BYTES: usize = core::mem::size_of::<HostCallResult>();

pub const OBJECT_REQUEST_SYSCALL: u64 = SYSCALL_OBJECT_REQUEST;
pub const TASK_REQUEST_SYSCALL: u64 = SYSCALL_TASK_REQUEST;
pub const GRAPH_LOG_SYSCALL: u64 = SYSCALL_PYTH_GRAPH_LOG;
pub const SYSCALL_OK_VALUE: u64 = SYSCALL_OK;

pub const OBJECT_REQUEST_ABI_WORD: u64 =
    (OBJECT_SHELL_ABI_MAJOR as u64) | ((OBJECT_SHELL_ABI_MINOR as u64) << 16);
pub const TASK_REQUEST_ABI_WORD: u64 = (TASK_ABI_MAJOR as u64) | ((TASK_ABI_MINOR as u64) << 16);
pub const OBJECT_KIND_NOTE_CODE: u16 = OBJECT_KIND_NOTE;
pub const OBJECT_FIELD_TEXT_CODE: u16 = FIELD_TEXT;
pub const TASK_PROPOSAL_KIND_RELATED_CODE: u16 = TaskProposalKind::Related.code();
pub const TASK_CONTEXT_OPERATION: u16 = OP_READ_CONTEXT_SUMMARY;
pub const TASK_PROPOSAL_OPERATION: u16 = OP_CREATE_PROPOSAL;

pub const OBJECT_RESPONSE_STATUS_OFFSET: usize = core::mem::offset_of!(ObjectShellResponse, status);
pub const OBJECT_RESPONSE_OBJECT_ID_OFFSET: usize =
    core::mem::offset_of!(ObjectShellResponse, object_id);
pub const OBJECT_RESPONSE_REVISION_OFFSET: usize =
    core::mem::offset_of!(ObjectShellResponse, revision);
pub const OBJECT_RESPONSE_REVISION_COUNT_OFFSET: usize =
    core::mem::offset_of!(ObjectShellResponse, revision_count);
pub const OBJECT_RESPONSE_BYTES_WRITTEN_OFFSET: usize =
    core::mem::offset_of!(ObjectShellResponse, bytes_written);
pub const OBJECT_RESPONSE_CAPABILITY_OFFSET: usize =
    core::mem::offset_of!(ObjectShellResponse, capability);
pub const OBJECT_RESPONSE_FIELD_BYTES_OFFSET: usize =
    core::mem::offset_of!(ObjectShellResponse, field_bytes);

pub const HOST_RESULT_STATUS_OFFSET: usize = core::mem::offset_of!(HostCallResult, status);
pub const HOST_RESULT_RESERVED0_OFFSET: usize = core::mem::offset_of!(HostCallResult, reserved0);
pub const HOST_RESULT_OBJECT_ID_OFFSET: usize = core::mem::offset_of!(HostCallResult, object_id);
pub const HOST_RESULT_REVISION_OFFSET: usize = core::mem::offset_of!(HostCallResult, revision);
pub const HOST_RESULT_CAPABILITY_OFFSET: usize = core::mem::offset_of!(HostCallResult, capability);
pub const HOST_RESULT_BYTES_OFFSET: usize = core::mem::offset_of!(HostCallResult, bytes);

pub const TASK_REQUEST_AUTHORITY_OFFSET: usize = core::mem::offset_of!(TaskRequest, authority);
pub const TASK_REQUEST_TARGET_TASK_ID_OFFSET: usize =
    core::mem::offset_of!(TaskRequest, target_task_id);
pub const TASK_REQUEST_OUTPUT_PTR_OFFSET: usize = core::mem::offset_of!(TaskRequest, output_ptr);
pub const TASK_REQUEST_OUTPUT_LEN_OFFSET: usize = core::mem::offset_of!(TaskRequest, output_len);
pub const TASK_REQUEST_SCORE_OFFSET: usize = core::mem::offset_of!(TaskRequest, score);
pub const TASK_RESPONSE_STATUS_OFFSET: usize = core::mem::offset_of!(TaskResponse, status);
pub const TASK_CONTEXT_ACTIVE_TASK_ID_OFFSET: usize =
    core::mem::offset_of!(TaskContextSummary, active_task_id);
pub const TASK_CONTEXT_MATCHING_TASK_ID_OFFSET: usize =
    core::mem::offset_of!(TaskContextSummary, matching_suspended_task_id);
pub const TASK_CONTEXT_PROPOSAL_KIND_OFFSET: usize =
    core::mem::offset_of!(TaskContextSummary, proposal_kind);
pub const TASK_CONTEXT_CONFIDENCE_SCORE_OFFSET: usize =
    core::mem::offset_of!(TaskContextSummary, confidence_score);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectStubOperation {
    Create,
    Query,
    Inspect,
    Revise,
    History,
}

impl ObjectStubOperation {
    pub fn from_opcode(opcode: Opcode) -> Option<Self> {
        match opcode {
            Opcode::ObjectCreate => Some(Self::Create),
            Opcode::ObjectQuery => Some(Self::Query),
            Opcode::ObjectInspect => Some(Self::Inspect),
            Opcode::ObjectRevise => Some(Self::Revise),
            Opcode::ObjectHistory => Some(Self::History),
            _ => None,
        }
    }

    pub const fn request_operation(self) -> u16 {
        match self {
            Self::Create => OP_CREATE_OBJECT,
            Self::Query => OP_QUERY_OBJECTS,
            Self::Inspect => OP_INSPECT_OBJECT,
            Self::Revise => OP_REVISE_FIELD,
            Self::History => OP_GET_HISTORY,
        }
    }

    pub const fn request_kind(self) -> u16 {
        match self {
            Self::Create | Self::Query => OBJECT_KIND_NOTE,
            Self::Inspect | Self::Revise | Self::History => 0,
        }
    }

    pub const fn request_field(self) -> u16 {
        match self {
            Self::Revise => FIELD_TEXT,
            Self::Create | Self::Query | Self::Inspect | Self::History => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeStubDataLayout {
    task_marker_offset: usize,
    object_request_offset: usize,
    object_response_offset: usize,
    object_query_output_offset: usize,
    task_request_offset: usize,
    task_response_offset: usize,
    task_context_offset: usize,
    host_result_base_offset: usize,
    total_bytes: usize,
}

impl NativeStubDataLayout {
    pub fn new(node_count: usize) -> Result<Self> {
        let object_request_offset = align_up(OBJECT_REQUEST_MARKER.len(), 8)?;
        let object_response_offset = align_up(
            object_request_offset
                .checked_add(OBJECT_REQUEST_BYTES)
                .ok_or(CodegenError::AddressOverflow)?,
            8,
        )?;
        let object_query_output_offset = align_up(
            object_response_offset
                .checked_add(OBJECT_RESPONSE_BYTES)
                .ok_or(CodegenError::AddressOverflow)?,
            8,
        )?;
        let task_marker_offset = align_up(
            object_query_output_offset
                .checked_add(OBJECT_QUERY_OUTPUT_BYTES)
                .ok_or(CodegenError::AddressOverflow)?,
            8,
        )?;
        let task_request_offset = align_up(
            task_marker_offset
                .checked_add(TASK_REQUEST_MARKER.len())
                .ok_or(CodegenError::AddressOverflow)?,
            8,
        )?;
        let task_response_offset = align_up(
            task_request_offset
                .checked_add(TASK_REQUEST_BYTES)
                .ok_or(CodegenError::AddressOverflow)?,
            8,
        )?;
        let task_context_offset = align_up(
            task_response_offset
                .checked_add(TASK_RESPONSE_BYTES)
                .ok_or(CodegenError::AddressOverflow)?,
            8,
        )?;
        let host_result_base_offset = align_up(
            task_context_offset
                .checked_add(TASK_CONTEXT_BYTES)
                .ok_or(CodegenError::AddressOverflow)?,
            8,
        )?;
        let host_result_bytes = node_count
            .checked_mul(HOST_CALL_RESULT_BYTES)
            .ok_or(CodegenError::AddressOverflow)?;
        let total_bytes = align_up(
            host_result_base_offset
                .checked_add(host_result_bytes)
                .ok_or(CodegenError::AddressOverflow)?,
            8,
        )?;

        Ok(Self {
            task_marker_offset,
            object_request_offset,
            object_response_offset,
            object_query_output_offset,
            task_request_offset,
            task_response_offset,
            task_context_offset,
            host_result_base_offset,
            total_bytes,
        })
    }

    pub fn data_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0; self.total_bytes];
        bytes[..OBJECT_REQUEST_MARKER.len()].copy_from_slice(OBJECT_REQUEST_MARKER);
        bytes[self.task_marker_offset..self.task_marker_offset + TASK_REQUEST_MARKER.len()]
            .copy_from_slice(TASK_REQUEST_MARKER);
        bytes
    }

    pub const fn object_request_offset(&self) -> usize {
        self.object_request_offset
    }

    pub const fn object_response_offset(&self) -> usize {
        self.object_response_offset
    }

    pub const fn object_query_output_offset(&self) -> usize {
        self.object_query_output_offset
    }

    pub const fn task_request_offset(&self) -> usize {
        self.task_request_offset
    }

    pub const fn task_response_offset(&self) -> usize {
        self.task_response_offset
    }

    pub const fn task_context_offset(&self) -> usize {
        self.task_context_offset
    }

    pub fn host_result_offset(&self, node_index: usize) -> Result<usize> {
        let relative = node_index
            .checked_mul(HOST_CALL_RESULT_BYTES)
            .ok_or(CodegenError::AddressOverflow)?;
        self.host_result_base_offset
            .checked_add(relative)
            .ok_or(CodegenError::AddressOverflow)
    }
}

pub const fn task_context_result_field_offset(field: u32) -> Option<usize> {
    match field {
        TASK_CONTEXT_RESULT_ACTIVE_TASK_ID => Some(HOST_RESULT_OBJECT_ID_OFFSET),
        TASK_CONTEXT_RESULT_CANDIDATE_TASK_ID => Some(HOST_RESULT_REVISION_OFFSET),
        TASK_CONTEXT_RESULT_CONFIDENCE_SCORE => Some(HOST_RESULT_CAPABILITY_OFFSET),
        TASK_CONTEXT_RESULT_PROPOSAL_KIND => Some(HOST_RESULT_RESERVED0_OFFSET),
        TASK_CONTEXT_RESULT_REASON_UTF8 => Some(HOST_RESULT_BYTES_OFFSET),
        _ => None,
    }
}

pub const fn host_result_field_offset(field: u32) -> Option<usize> {
    match field {
        HOST_RESULT_STATUS => Some(HOST_RESULT_STATUS_OFFSET),
        HOST_RESULT_OBJECT_ID => Some(HOST_RESULT_OBJECT_ID_OFFSET),
        HOST_RESULT_REVISION => Some(HOST_RESULT_REVISION_OFFSET),
        HOST_RESULT_CAPABILITY => Some(HOST_RESULT_CAPABILITY_OFFSET),
        HOST_RESULT_UTF8 => Some(HOST_RESULT_BYTES_OFFSET),
        _ => None,
    }
}

fn align_up(value: usize, alignment: usize) -> Result<usize> {
    debug_assert!(alignment.is_power_of_two());
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .ok_or(CodegenError::AddressOverflow)
}
