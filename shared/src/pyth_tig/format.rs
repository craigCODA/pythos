pub const PYTH_TIG_MAGIC: [u8; 8] = *b"PYTHTIG1";
pub const PYTH_TIG_MAJOR: u16 = 1;
pub const PYTH_TIG_MINOR: u16 = 0;

pub const MAX_PACKAGE_BYTES: usize = 131_072;
pub const MAX_GRAPH_NODES: usize = 1_024;
pub const MAX_BLOCKS: usize = 128;
pub const MAX_CAPABILITY_IMPORTS: usize = 32;
pub const MAX_CONSTANT_POOL_BYTES: usize = 65_536;
pub const MAX_STRING_TABLE_BYTES: usize = 16_384;
pub const MAX_RUNTIME_VALUES: usize = 1_024;
pub const MAX_EXECUTED_NODES_PER_INVOCATION: usize = 65_536;

pub const NO_VALUE: u32 = u32::MAX;

// The provisional v1 public layout struct mirrors the candidate 96-byte header
// with `checksum` at byte 84. Codecs must still read/write explicit LE fields.
#[repr(C, packed(4))]
pub struct PythGraphHeader {
    pub magic: [u8; 8],
    pub major: u16,
    pub minor: u16,
    pub flags: u32,
    pub package_id: u64,
    pub principal_id: u64,
    pub entry_block: u32,
    pub type_count: u32,
    pub block_count: u32,
    pub node_count: u32,
    pub import_count: u32,
    pub constant_pool_len: u32,
    pub string_table_len: u32,
    pub types_offset: u32,
    pub blocks_offset: u32,
    pub nodes_offset: u32,
    pub imports_offset: u32,
    pub constant_pool_offset: u32,
    pub string_table_offset: u32,
    pub checksum: u64,
    pub reserved: u32,
}

#[repr(C)]
pub struct TypeRecord {
    pub kind: u16,
    pub flags: u16,
    pub auxiliary: u32,
}

#[repr(C)]
pub struct BlockRecord {
    pub block_id: u32,
    pub first_node: u32,
    pub node_count: u32,
    pub parameter_count: u16,
    pub flags: u16,
    pub terminator_node: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct NodeRecord {
    pub opcode: u16,
    pub result_type: u16,
    pub flags: u16,
    pub block_index: u16,
    pub input0: u32,
    pub input1: u32,
    pub input2: u32,
    pub input3: u32,
    pub auxiliary0: u32,
    pub auxiliary1: u32,
    pub immediate: u64,
}

#[repr(C)]
pub struct CapabilityImportRecord {
    pub name_offset: u32,
    pub name_len: u16,
    pub resource_kind: u16,
    pub rights: u64,
    pub expected_type: u16,
    pub import_slot: u16,
    pub reserved: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pyth_tig::{opcode::Opcode, types::PythType};

    #[test]
    fn v1_layouts_and_codes_are_recorded() {
        assert_eq!(PYTH_TIG_MAGIC, *b"PYTHTIG1");
        assert_eq!(PYTH_TIG_MAJOR, 1);
        assert_eq!(PYTH_TIG_MINOR, 0);
        assert_eq!(core::mem::size_of::<PythGraphHeader>(), 96);
        assert_eq!(core::mem::size_of::<TypeRecord>(), 8);
        assert_eq!(core::mem::size_of::<BlockRecord>(), 24);
        assert_eq!(core::mem::size_of::<NodeRecord>(), 40);
        assert_eq!(core::mem::size_of::<CapabilityImportRecord>(), 24);
        assert_eq!(PythType::Capability.code(), 0x000A);
        assert_eq!(PythType::Effect.code(), 0x000B);
        assert_eq!(Opcode::SystemLog.code(), 0x1000);
        assert_eq!(Opcode::TaskProposalEmit.code(), 0x1201);
        assert_eq!(NO_VALUE, u32::MAX);
    }
}
