use crate::object_shell_abi::PackedCapability;

pub const PYTH_GRAPH_BOOTSTRAP_MAGIC: u64 = 0x3154_4F4F_4247_5950;
pub const PYTH_GRAPH_RUNTIME_ABI_MAJOR: u16 = 1;
pub const PYTH_GRAPH_RUNTIME_ABI_MINOR: u16 = 0;
pub const MAX_PYTH_GRAPH_IMPORTS: usize = 32;
pub const GRAPH_EXIT_OK: u16 = 0;
pub const GRAPH_EXIT_RUNTIME_ERROR: u16 = 1;
pub const GRAPH_EXIT_BUDGET_EXHAUSTED: u16 = 2;
pub const GRAPH_RESULT_UNIT: u16 = 0x0000;

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
}
