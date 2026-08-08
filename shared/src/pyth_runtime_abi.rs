use crate::object_shell_abi::PackedCapability;

pub const PYTH_GRAPH_BOOTSTRAP_MAGIC: u64 = 0x3154_4F4F_4247_5950;
pub const PYTH_GRAPH_RUNTIME_ABI_MAJOR: u16 = 1;
pub const PYTH_GRAPH_RUNTIME_ABI_MINOR: u16 = 0;
pub const MAX_PYTH_GRAPH_IMPORTS: usize = 32;
pub const GRAPH_EXIT_OK: u16 = 0;
pub const GRAPH_EXIT_RUNTIME_ERROR: u16 = 1;
pub const GRAPH_EXIT_BUDGET_EXHAUSTED: u16 = 2;
pub const GRAPH_RESULT_UNIT: u16 = 0x0000;
pub const SYSCALL_PYTH_GRAPH_LOG: u64 = 0x5059_0200;
pub const SYSCALL_PYTH_GRAPH_EXIT: u64 = 0x5059_0201;
pub const GRAPH_MAX_LOG_BYTES: u64 = 256;
pub const HOST_RESULT_STATUS: u32 = 0;
pub const HOST_RESULT_OBJECT_ID: u32 = 1;
pub const HOST_RESULT_REVISION: u32 = 2;
pub const HOST_RESULT_CAPABILITY: u32 = 3;
pub const HOST_RESULT_UTF8: u32 = 4;
pub const MAX_HOST_RESULT_BYTES: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythGraphCapabilityBinding {
    pub import_slot: u16,
    pub resource_kind: u16,
    pub reserved0: u32,
    pub rights: u64,
    pub capability: PackedCapability,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythGraphBootstrapBlock {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub import_count: u16,
    pub reserved0: u16,
    pub package_ptr: u64,
    pub package_len: u64,
    pub instruction_budget: u64,
    pub result_ptr: u64,
    pub imports: [PythGraphCapabilityBinding; MAX_PYTH_GRAPH_IMPORTS],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostCallResult {
    pub status: u16,
    pub bytes_len: u16,
    pub reserved0: u32,
    pub object_id: u64,
    pub revision: u64,
    pub capability: PackedCapability,
    pub bytes: [u8; MAX_HOST_RESULT_BYTES],
    pub reserved1: [u8; 16],
}

impl HostCallResult {
    pub const fn empty(status: u16) -> Self {
        Self {
            status,
            bytes_len: 0,
            reserved0: 0,
            object_id: 0,
            revision: 0,
            capability: PackedCapability::from_raw(0),
            bytes: [0; MAX_HOST_RESULT_BYTES],
            reserved1: [0; 16],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphExitRecord {
    pub status: u16,
    pub error_code: u16,
    pub last_node: u32,
    pub executed_nodes: u64,
    pub result_type: u16,
    pub reserved0: u16,
    pub reserved1: u32,
    pub result_raw: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_bootstrap_and_exit_layouts_are_stable() {
        assert_eq!(PYTH_GRAPH_BOOTSTRAP_MAGIC, 0x3154_4F4F_4247_5950);
        assert_eq!(PYTH_GRAPH_RUNTIME_ABI_MAJOR, 1);
        assert_eq!(PYTH_GRAPH_RUNTIME_ABI_MINOR, 0);
        assert_eq!(MAX_PYTH_GRAPH_IMPORTS, 32);
        assert_eq!(GRAPH_EXIT_OK, 0);
        assert_eq!(GRAPH_EXIT_RUNTIME_ERROR, 1);
        assert_eq!(GRAPH_EXIT_BUDGET_EXHAUSTED, 2);
        assert_eq!(GRAPH_RESULT_UNIT, 0);
        assert_eq!(SYSCALL_PYTH_GRAPH_LOG, 0x5059_0200);
        assert_eq!(SYSCALL_PYTH_GRAPH_EXIT, 0x5059_0201);
        assert_eq!(GRAPH_MAX_LOG_BYTES, 256);
        assert_eq!(core::mem::size_of::<PythGraphCapabilityBinding>(), 24);
        assert_eq!(core::mem::size_of::<PythGraphBootstrapBlock>(), 816);
        assert_eq!(core::mem::size_of::<GraphExitRecord>(), 32);
    }

    #[test]
    fn capability_binding_field_offsets_and_alignment_are_stable() {
        assert_eq!(core::mem::align_of::<PythGraphCapabilityBinding>(), 8);
        assert_eq!(
            core::mem::offset_of!(PythGraphCapabilityBinding, import_slot),
            0
        );
        assert_eq!(
            core::mem::offset_of!(PythGraphCapabilityBinding, resource_kind),
            2
        );
        assert_eq!(
            core::mem::offset_of!(PythGraphCapabilityBinding, reserved0),
            4
        );
        assert_eq!(core::mem::offset_of!(PythGraphCapabilityBinding, rights), 8);
        assert_eq!(
            core::mem::offset_of!(PythGraphCapabilityBinding, capability),
            16
        );
    }

    #[test]
    fn bootstrap_block_field_offsets_and_alignment_are_stable() {
        assert_eq!(core::mem::align_of::<PythGraphBootstrapBlock>(), 8);
        assert_eq!(core::mem::offset_of!(PythGraphBootstrapBlock, magic), 0);
        assert_eq!(core::mem::offset_of!(PythGraphBootstrapBlock, abi_major), 8);
        assert_eq!(
            core::mem::offset_of!(PythGraphBootstrapBlock, abi_minor),
            10
        );
        assert_eq!(
            core::mem::offset_of!(PythGraphBootstrapBlock, import_count),
            12
        );
        assert_eq!(
            core::mem::offset_of!(PythGraphBootstrapBlock, reserved0),
            14
        );
        assert_eq!(
            core::mem::offset_of!(PythGraphBootstrapBlock, package_ptr),
            16
        );
        assert_eq!(
            core::mem::offset_of!(PythGraphBootstrapBlock, package_len),
            24
        );
        assert_eq!(
            core::mem::offset_of!(PythGraphBootstrapBlock, instruction_budget),
            32
        );
        assert_eq!(
            core::mem::offset_of!(PythGraphBootstrapBlock, result_ptr),
            40
        );
        assert_eq!(core::mem::offset_of!(PythGraphBootstrapBlock, imports), 48);
    }

    #[test]
    fn graph_exit_record_field_offsets_and_alignment_are_stable() {
        assert_eq!(core::mem::align_of::<GraphExitRecord>(), 8);
        assert_eq!(core::mem::offset_of!(GraphExitRecord, status), 0);
        assert_eq!(core::mem::offset_of!(GraphExitRecord, error_code), 2);
        assert_eq!(core::mem::offset_of!(GraphExitRecord, last_node), 4);
        assert_eq!(core::mem::offset_of!(GraphExitRecord, executed_nodes), 8);
        assert_eq!(core::mem::offset_of!(GraphExitRecord, result_type), 16);
        assert_eq!(core::mem::offset_of!(GraphExitRecord, reserved0), 18);
        assert_eq!(core::mem::offset_of!(GraphExitRecord, reserved1), 20);
        assert_eq!(core::mem::offset_of!(GraphExitRecord, result_raw), 24);
    }

    #[test]
    fn host_call_result_layout_is_stable() {
        assert_eq!(HOST_RESULT_STATUS, 0);
        assert_eq!(HOST_RESULT_OBJECT_ID, 1);
        assert_eq!(HOST_RESULT_REVISION, 2);
        assert_eq!(HOST_RESULT_CAPABILITY, 3);
        assert_eq!(HOST_RESULT_UTF8, 4);
        assert_eq!(MAX_HOST_RESULT_BYTES, 64);
        assert_eq!(core::mem::size_of::<HostCallResult>(), 112);
        assert_eq!(core::mem::align_of::<HostCallResult>(), 8);
        assert_eq!(core::mem::offset_of!(HostCallResult, status), 0);
        assert_eq!(core::mem::offset_of!(HostCallResult, bytes_len), 2);
        assert_eq!(core::mem::offset_of!(HostCallResult, reserved0), 4);
        assert_eq!(core::mem::offset_of!(HostCallResult, object_id), 8);
        assert_eq!(core::mem::offset_of!(HostCallResult, revision), 16);
        assert_eq!(core::mem::offset_of!(HostCallResult, capability), 24);
        assert_eq!(core::mem::offset_of!(HostCallResult, bytes), 32);
        assert_eq!(core::mem::offset_of!(HostCallResult, reserved1), 96);
    }

    #[test]
    fn empty_host_call_result_has_no_ambient_capability_or_reserved_metadata() {
        let result = HostCallResult::empty(3);

        assert_eq!(result.status, 3);
        assert_eq!(result.bytes_len, 0);
        assert_eq!(result.reserved0, 0);
        assert_eq!(result.object_id, 0);
        assert_eq!(result.revision, 0);
        assert_eq!(result.capability.raw(), 0);
        assert_eq!(result.bytes, [0; MAX_HOST_RESULT_BYTES]);
        assert_eq!(result.reserved1, [0; 16]);
    }
}
