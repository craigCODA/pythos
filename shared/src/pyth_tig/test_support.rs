use crate::pyth_tig::{
    NO_VALUE,
    format::{
        CapabilityImportRecord, NodeRecord, PYTH_TIG_MAGIC, PYTH_TIG_MAJOR, PYTH_TIG_MINOR,
        PythGraphHeader,
    },
    opcode::Opcode,
    types::PythType,
};

const HEADER_SIZE: usize = core::mem::size_of::<PythGraphHeader>();
const TYPE_RECORD_SIZE: usize = core::mem::size_of::<crate::pyth_tig::TypeRecord>();
const BLOCK_RECORD_SIZE: usize = core::mem::size_of::<crate::pyth_tig::BlockRecord>();
const NODE_RECORD_SIZE: usize = core::mem::size_of::<NodeRecord>();
const IMPORT_RECORD_SIZE: usize = core::mem::size_of::<CapabilityImportRecord>();
const CHECKSUM_OFFSET: usize = 84;
const HEADER_RESERVED_OFFSET: usize = 92;
const FLAGS_OFFSET: usize = 12;
const MAJOR_OFFSET: usize = 8;
const MINOR_OFFSET: usize = 10;
const BLOCKS_OFFSET_OFFSET: usize = 64;
const NODES_OFFSET_OFFSET: usize = 68;
const IMPORTS_OFFSET_OFFSET: usize = 72;
const STRING_TABLE_OFFSET_OFFSET: usize = 80;
const MINIMAL_LOG_PACKAGE_LEN: usize = HEADER_SIZE
    + 3 * TYPE_RECORD_SIZE
    + BLOCK_RECORD_SIZE
    + 3 * NODE_RECORD_SIZE
    + IMPORT_RECORD_SIZE
    + 5;
const TERMINATED_PACKAGE_LEN: usize = HEADER_SIZE + BLOCK_RECORD_SIZE + 2 * NODE_RECORD_SIZE;
const ORPHAN_NODE_PACKAGE_LEN: usize = HEADER_SIZE + BLOCK_RECORD_SIZE + 3 * NODE_RECORD_SIZE;
const UNREACHABLE_BLOCK_PACKAGE_LEN: usize =
    HEADER_SIZE + 2 * BLOCK_RECORD_SIZE + 4 * NODE_RECORD_SIZE;

struct BlockSpec {
    block_id: u32,
    first_node: u32,
    node_count: u32,
    parameter_count: u16,
    flags: u16,
    terminator_node: u32,
}

struct NodeSpec {
    opcode: u16,
    result_type: u16,
    flags: u16,
    block_index: u16,
    inputs: [u32; 4],
    auxiliary0: u32,
    auxiliary1: u32,
    immediate: u64,
}

struct ImportSpec {
    name_offset: u32,
    name_len: u16,
    resource_kind: u16,
    rights: u64,
    expected_type: u16,
    import_slot: u16,
}

#[repr(align(8))]
pub struct AlignedPackage {
    bytes: [u8; MINIMAL_LOG_PACKAGE_LEN],
}

pub struct EmptyPackage {
    bytes: [u8; HEADER_SIZE],
}

#[repr(align(8))]
pub struct FixturePackage<const LEN: usize> {
    bytes: [u8; LEN],
}

impl core::ops::Deref for AlignedPackage {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

impl core::ops::DerefMut for AlignedPackage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bytes
    }
}

impl core::ops::Deref for EmptyPackage {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

impl<const LEN: usize> core::ops::Deref for FixturePackage<LEN> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

impl<const LEN: usize> core::ops::DerefMut for FixturePackage<LEN> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bytes
    }
}

impl core::ops::DerefMut for EmptyPackage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bytes
    }
}

pub fn minimal_log_package() -> AlignedPackage {
    let type_count = 3usize;
    let block_count = 1usize;
    let node_count = 3usize;
    let import_count = 1usize;
    let string_table = b"hello";

    let types_offset = HEADER_SIZE;
    let blocks_offset = types_offset + type_count * TYPE_RECORD_SIZE;
    let nodes_offset = blocks_offset + block_count * BLOCK_RECORD_SIZE;
    let imports_offset = nodes_offset + node_count * NODE_RECORD_SIZE;
    let constant_pool_offset = imports_offset + import_count * IMPORT_RECORD_SIZE;
    let string_table_offset = constant_pool_offset;
    let package_len = string_table_offset + string_table.len();
    assert_eq!(package_len, MINIMAL_LOG_PACKAGE_LEN);

    let mut package = AlignedPackage {
        bytes: [0u8; MINIMAL_LOG_PACKAGE_LEN],
    };
    let bytes = &mut package.bytes[..];
    bytes[0..8].copy_from_slice(&PYTH_TIG_MAGIC);
    write_u16(bytes, 8, PYTH_TIG_MAJOR);
    write_u16(bytes, 10, PYTH_TIG_MINOR);
    write_u64(bytes, 16, 0x5059_5448_5449_4701);
    write_u64(bytes, 24, 0x5059_5448_5052_4E01);
    write_u32(bytes, 32, 0);
    write_u32(bytes, 36, type_count as u32);
    write_u32(bytes, 40, block_count as u32);
    write_u32(bytes, 44, node_count as u32);
    write_u32(bytes, 48, import_count as u32);
    write_u32(bytes, 52, 0);
    write_u32(bytes, 56, string_table.len() as u32);
    write_u32(bytes, 60, types_offset as u32);
    write_u32(bytes, 64, blocks_offset as u32);
    write_u32(bytes, 68, nodes_offset as u32);
    write_u32(bytes, 72, imports_offset as u32);
    write_u32(bytes, 76, constant_pool_offset as u32);
    write_u32(bytes, 80, string_table_offset as u32);

    write_type_record(bytes, types_offset, PythType::Effect.code(), 0, 0);
    write_type_record(
        bytes,
        types_offset + TYPE_RECORD_SIZE,
        PythType::Utf8.code(),
        0,
        0,
    );
    write_type_record(
        bytes,
        types_offset + 2 * TYPE_RECORD_SIZE,
        PythType::Unit.code(),
        0,
        0,
    );

    write_block_record(
        bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: node_count as u32,
            parameter_count: 0,
            flags: 0,
            terminator_node: 2,
        },
    );
    write_node_record(
        bytes,
        nodes_offset,
        NodeSpec {
            opcode: Opcode::EffectStart.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        bytes,
        nodes_offset + NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ConstUtf8.code(),
            result_type: PythType::Utf8.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: string_table.len() as u32,
            immediate: 0,
        },
    );
    write_node_record(
        bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::SystemLog.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, NO_VALUE, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_import_record(
        bytes,
        imports_offset,
        ImportSpec {
            name_offset: 0,
            name_len: string_table.len() as u16,
            resource_kind: 1,
            rights: 0x1,
            expected_type: PythType::Capability.code(),
            import_slot: 0,
        },
    );
    bytes[string_table_offset..string_table_offset + string_table.len()]
        .copy_from_slice(string_table);
    refresh_checksum(bytes);
    package
}

pub fn package_without_terminator() -> AlignedPackage {
    minimal_log_package()
}

pub fn package_with_bad_branch_target() -> AlignedPackage {
    let mut package = minimal_log_package();
    let bytes = &mut package.bytes[..];
    let nodes_offset = read_u32(bytes, NODES_OFFSET_OFFSET) as usize;

    write_node_record(
        bytes,
        nodes_offset + NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ConstBool.code(),
            result_type: PythType::Bool.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 1,
        },
    );
    write_node_record(
        bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::Branch.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 0,
            inputs: [1, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 9,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    refresh_checksum(bytes);
    package
}

pub fn package_with_use_before_definition() -> AlignedPackage {
    let mut package = minimal_log_package();
    let bytes = &mut package.bytes[..];
    let nodes_offset = read_u32(bytes, NODES_OFFSET_OFFSET) as usize;

    write_node_record(
        bytes,
        nodes_offset,
        NodeSpec {
            opcode: Opcode::ConstU64.code(),
            result_type: PythType::U64.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 1,
        },
    );
    write_node_record(
        bytes,
        nodes_offset + NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::AddU64.code(),
            result_type: PythType::U64.code(),
            flags: 0,
            block_index: 0,
            inputs: [2, 0, NO_VALUE, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::Return.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 0,
            inputs: [1, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    refresh_checksum(bytes);
    package
}

pub fn structurally_valid_terminated_package() -> FixturePackage<TERMINATED_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; TERMINATED_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 2, 0, 0, 0);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 2,
            parameter_count: 0,
            flags: 0,
            terminator_node: 1,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset,
        NodeSpec {
            opcode: Opcode::ConstU64.code(),
            result_type: PythType::U64.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 7,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::Return.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    refresh_checksum(&mut package.bytes);
    package
}

pub fn package_with_orphan_node() -> FixturePackage<ORPHAN_NODE_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; ORPHAN_NODE_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 3, 0, 0, 0);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 2,
            parameter_count: 0,
            flags: 0,
            terminator_node: 1,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset,
        NodeSpec {
            opcode: Opcode::ConstU64.code(),
            result_type: PythType::U64.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 1,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::Return.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ConstU64.code(),
            result_type: PythType::U64.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 2,
        },
    );
    refresh_checksum(&mut package.bytes);
    package
}

pub fn package_with_unreachable_block() -> FixturePackage<UNREACHABLE_BLOCK_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; UNREACHABLE_BLOCK_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 2, 4, 0, 0, 0);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 2,
            parameter_count: 0,
            flags: 0,
            terminator_node: 1,
        },
    );
    write_block_record(
        &mut package.bytes,
        blocks_offset + BLOCK_RECORD_SIZE,
        BlockSpec {
            block_id: 1,
            first_node: 2,
            node_count: 2,
            parameter_count: 0,
            flags: 0,
            terminator_node: 3,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset,
        NodeSpec {
            opcode: Opcode::ConstU64.code(),
            result_type: PythType::U64.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 1,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::Return.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::AddU64.code(),
            result_type: PythType::U64.code(),
            flags: 0,
            block_index: 1,
            inputs: [0, 0, NO_VALUE, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 3 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::Return.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 1,
            inputs: [2, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    refresh_checksum(&mut package.bytes);
    package
}

pub fn empty_package() -> EmptyPackage {
    let mut package = EmptyPackage {
        bytes: [0u8; HEADER_SIZE],
    };
    let bytes = &mut package.bytes[..];
    bytes[0..8].copy_from_slice(&PYTH_TIG_MAGIC);
    write_u16(bytes, MAJOR_OFFSET, PYTH_TIG_MAJOR);
    write_u16(bytes, MINOR_OFFSET, PYTH_TIG_MINOR);
    write_u64(bytes, 16, 0x5059_5448_5449_4702);
    write_u64(bytes, 24, 0x5059_5448_5052_4E02);
    write_u32(bytes, 60, HEADER_SIZE as u32);
    write_u32(bytes, BLOCKS_OFFSET_OFFSET, HEADER_SIZE as u32);
    write_u32(bytes, NODES_OFFSET_OFFSET, HEADER_SIZE as u32);
    write_u32(bytes, IMPORTS_OFFSET_OFFSET, HEADER_SIZE as u32);
    write_u32(bytes, 76, HEADER_SIZE as u32);
    write_u32(bytes, STRING_TABLE_OFFSET_OFFSET, HEADER_SIZE as u32);
    refresh_checksum(bytes);
    package
}

pub fn set_nodes_offset_equal_blocks_offset(bytes: &mut [u8]) {
    let blocks_offset = read_u32(bytes, BLOCKS_OFFSET_OFFSET);
    write_u32(bytes, NODES_OFFSET_OFFSET, blocks_offset);
    refresh_checksum(bytes);
}

pub fn corrupt_checksum(bytes: &mut [u8]) {
    bytes[CHECKSUM_OFFSET] ^= 0x01;
}

pub fn set_header_major(bytes: &mut [u8], major: u16) {
    write_u16(bytes, MAJOR_OFFSET, major);
    refresh_checksum(bytes);
}

pub fn set_header_minor(bytes: &mut [u8], minor: u16) {
    write_u16(bytes, MINOR_OFFSET, minor);
    refresh_checksum(bytes);
}

pub fn set_header_flags(bytes: &mut [u8], flags: u32) {
    write_u32(bytes, FLAGS_OFFSET, flags);
    refresh_checksum(bytes);
}

pub fn set_header_reserved(bytes: &mut [u8], reserved: u32) {
    write_u32(bytes, HEADER_RESERVED_OFFSET, reserved);
    refresh_checksum(bytes);
}

pub fn set_first_block_reserved(bytes: &mut [u8], reserved: u32) {
    let blocks_offset = read_u32(bytes, BLOCKS_OFFSET_OFFSET) as usize;
    write_u32(bytes, blocks_offset + 20, reserved);
    refresh_checksum(bytes);
}

pub fn set_first_import_reserved(bytes: &mut [u8], reserved: u32) {
    let imports_offset = read_u32(bytes, IMPORTS_OFFSET_OFFSET) as usize;
    write_u32(bytes, imports_offset + 20, reserved);
    refresh_checksum(bytes);
}

pub fn move_string_table_past_end(bytes: &mut [u8]) {
    let past_end = bytes.len() as u32 + 4;
    write_u32(bytes, STRING_TABLE_OFFSET_OFFSET, past_end);
    refresh_checksum(bytes);
}

fn write_type_record(bytes: &mut [u8], offset: usize, kind: u16, flags: u16, auxiliary: u32) {
    write_u16(bytes, offset, kind);
    write_u16(bytes, offset + 2, flags);
    write_u32(bytes, offset + 4, auxiliary);
}

fn write_block_record(bytes: &mut [u8], offset: usize, spec: BlockSpec) {
    write_u32(bytes, offset, spec.block_id);
    write_u32(bytes, offset + 4, spec.first_node);
    write_u32(bytes, offset + 8, spec.node_count);
    write_u16(bytes, offset + 12, spec.parameter_count);
    write_u16(bytes, offset + 14, spec.flags);
    write_u32(bytes, offset + 16, spec.terminator_node);
    write_u32(bytes, offset + 20, 0);
}

fn write_node_record(bytes: &mut [u8], offset: usize, spec: NodeSpec) {
    write_u16(bytes, offset, spec.opcode);
    write_u16(bytes, offset + 2, spec.result_type);
    write_u16(bytes, offset + 4, spec.flags);
    write_u16(bytes, offset + 6, spec.block_index);
    write_u32(bytes, offset + 8, spec.inputs[0]);
    write_u32(bytes, offset + 12, spec.inputs[1]);
    write_u32(bytes, offset + 16, spec.inputs[2]);
    write_u32(bytes, offset + 20, spec.inputs[3]);
    write_u32(bytes, offset + 24, spec.auxiliary0);
    write_u32(bytes, offset + 28, spec.auxiliary1);
    write_u64(bytes, offset + 32, spec.immediate);
}

fn write_import_record(bytes: &mut [u8], offset: usize, spec: ImportSpec) {
    write_u32(bytes, offset, spec.name_offset);
    write_u16(bytes, offset + 4, spec.name_len);
    write_u16(bytes, offset + 6, spec.resource_kind);
    write_u64(bytes, offset + 8, spec.rights);
    write_u16(bytes, offset + 16, spec.expected_type);
    write_u16(bytes, offset + 18, spec.import_slot);
    write_u32(bytes, offset + 20, 0);
}

fn initialize_graph_header(
    bytes: &mut [u8],
    type_count: usize,
    block_count: usize,
    node_count: usize,
    import_count: usize,
    constant_pool_len: usize,
    string_table_len: usize,
) {
    let types_offset = HEADER_SIZE;
    let blocks_offset = types_offset + type_count * TYPE_RECORD_SIZE;
    let nodes_offset = blocks_offset + block_count * BLOCK_RECORD_SIZE;
    let imports_offset = nodes_offset + node_count * NODE_RECORD_SIZE;
    let constant_pool_offset = imports_offset + import_count * IMPORT_RECORD_SIZE;
    let string_table_offset = constant_pool_offset + constant_pool_len;
    let package_len = string_table_offset + string_table_len;
    assert_eq!(package_len, bytes.len());

    bytes[0..8].copy_from_slice(&PYTH_TIG_MAGIC);
    write_u16(bytes, 8, PYTH_TIG_MAJOR);
    write_u16(bytes, 10, PYTH_TIG_MINOR);
    write_u64(bytes, 16, 0x5059_5448_5449_4703);
    write_u64(bytes, 24, 0x5059_5448_5052_4E03);
    write_u32(bytes, 32, 0);
    write_u32(bytes, 36, type_count as u32);
    write_u32(bytes, 40, block_count as u32);
    write_u32(bytes, 44, node_count as u32);
    write_u32(bytes, 48, import_count as u32);
    write_u32(bytes, 52, constant_pool_len as u32);
    write_u32(bytes, 56, string_table_len as u32);
    write_u32(bytes, 60, types_offset as u32);
    write_u32(bytes, 64, blocks_offset as u32);
    write_u32(bytes, 68, nodes_offset as u32);
    write_u32(bytes, 72, imports_offset as u32);
    write_u32(bytes, 76, constant_pool_offset as u32);
    write_u32(bytes, 80, string_table_offset as u32);
}

fn refresh_checksum(bytes: &mut [u8]) {
    write_u64(bytes, CHECKSUM_OFFSET, 0);
    let checksum = digest64(bytes);
    write_u64(bytes, CHECKSUM_OFFSET, checksum);
}

fn digest64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
