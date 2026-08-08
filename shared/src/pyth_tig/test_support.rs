use crate::pyth_tig::{
    NO_VALUE,
    format::{
        CapabilityImportRecord, NodeRecord, PYTH_TIG_MAGIC, PYTH_TIG_MAJOR, PYTH_TIG_MINOR,
        PythGraphHeader,
    },
    opcode::{
        Opcode, RESOURCE_OBJECT, RESOURCE_OBJECT_WORKSPACE, RIGHTS_CREATE, RIGHTS_READ,
        RIGHTS_REVISE,
    },
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
const ADD_BOOL_PACKAGE_LEN: usize = HEADER_SIZE + BLOCK_RECORD_SIZE + 4 * NODE_RECORD_SIZE;
const EFFECT_FORK_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 6 * NODE_RECORD_SIZE + IMPORT_RECORD_SIZE + 5;
const CAPABILITY_CONSTANT_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 5 * NODE_RECORD_SIZE + IMPORT_RECORD_SIZE + 5;
const OBJECT_REVISE_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 6 * NODE_RECORD_SIZE + IMPORT_RECORD_SIZE + 8;
const LOG_WITH_IMPORT_CAPABILITY_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 5 * NODE_RECORD_SIZE + IMPORT_RECORD_SIZE + 5;
const LOG_WITHOUT_CAPABILITY_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 4 * NODE_RECORD_SIZE + IMPORT_RECORD_SIZE + 5;
const LOG_CAPABILITY_HOST_RESULT_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 6 * NODE_RECORD_SIZE + IMPORT_RECORD_SIZE + 5;
const JUMP_ARGUMENT_COUNT_MISMATCH_PACKAGE_LEN: usize =
    HEADER_SIZE + 2 * BLOCK_RECORD_SIZE + 2 * NODE_RECORD_SIZE;
const JUMP_ARGUMENT_TYPE_MISMATCH_PACKAGE_LEN: usize =
    HEADER_SIZE + 2 * BLOCK_RECORD_SIZE + 4 * NODE_RECORD_SIZE;
const SELF_JUMP_BUDGET_LOOP_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 2 * NODE_RECORD_SIZE + IMPORT_RECORD_SIZE + 5;
const HOST_RESULT_WITHOUT_PRODUCER_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 4 * NODE_RECORD_SIZE;
const OBJECT_CREATE_HOST_RESULT_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 6 * NODE_RECORD_SIZE + IMPORT_RECORD_SIZE + 4;
const OBJECT_INSPECT_HOST_RESULT_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 6 * NODE_RECORD_SIZE + IMPORT_RECORD_SIZE + 8;
const OBJECT_CREATE_DYNAMIC_REVISE_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 9 * NODE_RECORD_SIZE + IMPORT_RECORD_SIZE + 5 + 4;
const OBJECT_NOTE_FLOW_PACKAGE_LEN: usize =
    HEADER_SIZE + BLOCK_RECORD_SIZE + 11 * NODE_RECORD_SIZE + IMPORT_RECORD_SIZE + 5 + 4;

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

pub fn structurally_valid_package_with_type_table() -> AlignedPackage {
    let mut package = minimal_log_package();
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
    write_node_record(
        &mut package.bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::Return.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    refresh_checksum(&mut package.bytes);
    package
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
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    refresh_checksum(&mut package.bytes);
    package
}

fn structurally_valid_terminated_package_with_return_input()
-> FixturePackage<TERMINATED_PACKAGE_LEN> {
    let mut package = structurally_valid_terminated_package();
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
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

pub fn package_with_add_bool() -> FixturePackage<ADD_BOOL_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; ADD_BOOL_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 4, 0, 0, 0);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 4,
            parameter_count: 0,
            flags: 0,
            terminator_node: 3,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset,
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
        &mut package.bytes,
        nodes_offset + NODE_RECORD_SIZE,
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
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::AddU64.code(),
            result_type: PythType::U64.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, NO_VALUE, NO_VALUE],
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
            block_index: 0,
            inputs: [2, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    refresh_checksum(&mut package.bytes);
    package
}

pub fn package_with_effect_fork() -> FixturePackage<EFFECT_FORK_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; EFFECT_FORK_PACKAGE_LEN],
    };
    initialize_log_effect_package(&mut package.bytes, RIGHTS_READ);
    package
}

pub fn system_log_with_import_capability() -> FixturePackage<LOG_WITH_IMPORT_CAPABILITY_PACKAGE_LEN>
{
    let mut package = FixturePackage {
        bytes: [0u8; LOG_WITH_IMPORT_CAPABILITY_PACKAGE_LEN],
    };
    initialize_log_with_import_capability_package(&mut package.bytes);
    package
}

pub fn self_jump_budget_loop() -> FixturePackage<SELF_JUMP_BUDGET_LOOP_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; SELF_JUMP_BUDGET_LOOP_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 2, 1, 0, 5);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
    let imports_offset = read_u32(&package.bytes, IMPORTS_OFFSET_OFFSET) as usize;
    let string_table_offset = read_u32(&package.bytes, STRING_TABLE_OFFSET_OFFSET) as usize;

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
    write_effect_start(&mut package.bytes, nodes_offset, 0);
    write_node_record(
        &mut package.bytes,
        nodes_offset + NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::Jump.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_system_log_import(&mut package.bytes, imports_offset, 5, RIGHTS_READ);
    package.bytes[string_table_offset..string_table_offset + 5].copy_from_slice(b"hello");
    refresh_checksum(&mut package.bytes);
    package
}

pub fn system_log_without_capability_input() -> FixturePackage<LOG_WITHOUT_CAPABILITY_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; LOG_WITHOUT_CAPABILITY_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 4, 1, 0, 5);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
    let imports_offset = read_u32(&package.bytes, IMPORTS_OFFSET_OFFSET) as usize;
    let string_table_offset = read_u32(&package.bytes, STRING_TABLE_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 4,
            parameter_count: 0,
            flags: 0,
            terminator_node: 3,
        },
    );
    write_effect_start(&mut package.bytes, nodes_offset, 0);
    write_const_utf8(&mut package.bytes, nodes_offset + NODE_RECORD_SIZE, 0, 5);
    write_node_record(
        &mut package.bytes,
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
    write_return(&mut package.bytes, nodes_offset + 3 * NODE_RECORD_SIZE, 0);
    write_system_log_import(&mut package.bytes, imports_offset, 5, RIGHTS_READ);
    package.bytes[string_table_offset..string_table_offset + 5].copy_from_slice(b"hello");
    refresh_checksum(&mut package.bytes);
    package
}

pub fn system_log_capability_host_result() -> FixturePackage<LOG_CAPABILITY_HOST_RESULT_PACKAGE_LEN>
{
    system_log_host_result(PythType::Capability, 3)
}

pub fn system_log_object_id_host_result() -> FixturePackage<LOG_CAPABILITY_HOST_RESULT_PACKAGE_LEN>
{
    system_log_host_result(PythType::ObjectId, 1)
}

pub fn system_log_revision_id_host_result() -> FixturePackage<LOG_CAPABILITY_HOST_RESULT_PACKAGE_LEN>
{
    system_log_host_result(PythType::RevisionId, 2)
}

pub fn system_log_utf8_host_result() -> FixturePackage<LOG_CAPABILITY_HOST_RESULT_PACKAGE_LEN> {
    system_log_host_result(PythType::Utf8, 4)
}

fn system_log_host_result(
    result_type: PythType,
    field: u32,
) -> FixturePackage<LOG_CAPABILITY_HOST_RESULT_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; LOG_CAPABILITY_HOST_RESULT_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 6, 1, 0, 5);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
    let imports_offset = read_u32(&package.bytes, IMPORTS_OFFSET_OFFSET) as usize;
    let string_table_offset = read_u32(&package.bytes, STRING_TABLE_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 6,
            parameter_count: 0,
            flags: 0,
            terminator_node: 5,
        },
    );
    write_effect_start(&mut package.bytes, nodes_offset, 0);
    write_import_capability_param(&mut package.bytes, nodes_offset + NODE_RECORD_SIZE, 0, 0);
    write_const_utf8(
        &mut package.bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        0,
        5,
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 3 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::SystemLog.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, 2, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 4 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::HostResult.code(),
            result_type: result_type.code(),
            flags: 0,
            block_index: 0,
            inputs: [3, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: field,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_return(&mut package.bytes, nodes_offset + 5 * NODE_RECORD_SIZE, 0);
    write_system_log_import(&mut package.bytes, imports_offset, 5, RIGHTS_READ);
    package.bytes[string_table_offset..string_table_offset + 5].copy_from_slice(b"hello");
    refresh_checksum(&mut package.bytes);
    package
}

pub fn host_result_without_producer() -> FixturePackage<HOST_RESULT_WITHOUT_PRODUCER_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; HOST_RESULT_WITHOUT_PRODUCER_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 4, 0, 0, 0);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 4,
            parameter_count: 0,
            flags: 0,
            terminator_node: 3,
        },
    );
    write_effect_start(&mut package.bytes, nodes_offset, 0);
    write_node_record(
        &mut package.bytes,
        nodes_offset + NODE_RECORD_SIZE,
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
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::HostResult.code(),
            result_type: PythType::ErrorCode.code(),
            flags: 0,
            block_index: 0,
            inputs: [1, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_return(&mut package.bytes, nodes_offset + 3 * NODE_RECORD_SIZE, 0);
    refresh_checksum(&mut package.bytes);
    package
}

pub fn object_create_host_result(
    result_type: PythType,
    field: u32,
) -> FixturePackage<OBJECT_CREATE_HOST_RESULT_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; OBJECT_CREATE_HOST_RESULT_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 6, 1, 0, 4);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
    let imports_offset = read_u32(&package.bytes, IMPORTS_OFFSET_OFFSET) as usize;
    let string_table_offset = read_u32(&package.bytes, STRING_TABLE_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 6,
            parameter_count: 0,
            flags: 0,
            terminator_node: 5,
        },
    );
    write_effect_start(&mut package.bytes, nodes_offset, 0);
    write_import_capability_param(&mut package.bytes, nodes_offset + NODE_RECORD_SIZE, 0, 0);
    write_const_utf8(
        &mut package.bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        0,
        4,
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 3 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ObjectCreate.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, 2, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 4 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::HostResult.code(),
            result_type: result_type.code(),
            flags: 0,
            block_index: 0,
            inputs: [3, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: field,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_return(&mut package.bytes, nodes_offset + 5 * NODE_RECORD_SIZE, 0);
    write_object_workspace_import(&mut package.bytes, imports_offset, 4, RIGHTS_CREATE);
    package.bytes[string_table_offset..string_table_offset + 4].copy_from_slice(b"note");
    refresh_checksum(&mut package.bytes);
    package
}

pub fn object_inspect_host_result(
    result_type: PythType,
    field: u32,
) -> FixturePackage<OBJECT_INSPECT_HOST_RESULT_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; OBJECT_INSPECT_HOST_RESULT_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 6, 1, 0, 8);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
    let imports_offset = read_u32(&package.bytes, IMPORTS_OFFSET_OFFSET) as usize;
    let string_table_offset = read_u32(&package.bytes, STRING_TABLE_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 6,
            parameter_count: 0,
            flags: 0,
            terminator_node: 5,
        },
    );
    write_effect_start(&mut package.bytes, nodes_offset, 0);
    write_import_capability_param(&mut package.bytes, nodes_offset + NODE_RECORD_SIZE, 0, 0);
    write_node_record(
        &mut package.bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ConstU64.code(),
            result_type: PythType::ObjectId.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 1042,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 3 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ObjectInspect.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, 2, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 4 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::HostResult.code(),
            result_type: result_type.code(),
            flags: 0,
            block_index: 0,
            inputs: [3, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: field,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_return(&mut package.bytes, nodes_offset + 5 * NODE_RECORD_SIZE, 0);
    write_import_record(
        &mut package.bytes,
        imports_offset,
        ImportSpec {
            name_offset: 0,
            name_len: 8,
            resource_kind: RESOURCE_OBJECT,
            rights: RIGHTS_READ,
            expected_type: PythType::Capability.code(),
            import_slot: 0,
        },
    );
    package.bytes[string_table_offset..string_table_offset + 8].copy_from_slice(b"object-0");
    refresh_checksum(&mut package.bytes);
    package
}

pub fn object_create_revise_with_dynamic_capability()
-> FixturePackage<OBJECT_CREATE_DYNAMIC_REVISE_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; OBJECT_CREATE_DYNAMIC_REVISE_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 9, 1, 5, 4);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
    let imports_offset = read_u32(&package.bytes, IMPORTS_OFFSET_OFFSET) as usize;
    let constant_pool_offset = read_u32(&package.bytes, 76) as usize;
    let string_table_offset = read_u32(&package.bytes, STRING_TABLE_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 9,
            parameter_count: 0,
            flags: 0,
            terminator_node: 8,
        },
    );
    write_effect_start(&mut package.bytes, nodes_offset, 0);
    write_import_capability_param(&mut package.bytes, nodes_offset + NODE_RECORD_SIZE, 0, 0);
    write_const_utf8(
        &mut package.bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        0,
        4,
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 3 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ObjectCreate.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, 2, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 4 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::HostResult.code(),
            result_type: PythType::ObjectId.code(),
            flags: 0,
            block_index: 0,
            inputs: [3, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 1,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 5 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::HostResult.code(),
            result_type: PythType::Capability.code(),
            flags: 0,
            block_index: 0,
            inputs: [3, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 3,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 6 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ConstBytes.code(),
            result_type: PythType::Bytes.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 5,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 7 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ObjectRevise.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [3, 5, 4, 6],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_return(&mut package.bytes, nodes_offset + 8 * NODE_RECORD_SIZE, 0);
    write_object_workspace_import(&mut package.bytes, imports_offset, 4, RIGHTS_CREATE);
    package.bytes[constant_pool_offset..constant_pool_offset + 5].copy_from_slice(b"hello");
    package.bytes[string_table_offset..string_table_offset + 4].copy_from_slice(b"note");
    refresh_checksum(&mut package.bytes);
    package
}

pub fn object_note_flow_package() -> FixturePackage<OBJECT_NOTE_FLOW_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; OBJECT_NOTE_FLOW_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 11, 1, 5, 4);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
    let imports_offset = read_u32(&package.bytes, IMPORTS_OFFSET_OFFSET) as usize;
    let constant_pool_offset = read_u32(&package.bytes, 76) as usize;
    let string_table_offset = read_u32(&package.bytes, STRING_TABLE_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 11,
            parameter_count: 0,
            flags: 0,
            terminator_node: 10,
        },
    );
    write_effect_start(&mut package.bytes, nodes_offset, 0);
    write_import_capability_param(&mut package.bytes, nodes_offset + NODE_RECORD_SIZE, 0, 0);
    write_const_utf8(
        &mut package.bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        0,
        4,
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 3 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ObjectCreate.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, 2, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 4 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::HostResult.code(),
            result_type: PythType::ObjectId.code(),
            flags: 0,
            block_index: 0,
            inputs: [3, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 1,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 5 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::HostResult.code(),
            result_type: PythType::Capability.code(),
            flags: 0,
            block_index: 0,
            inputs: [3, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 3,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 6 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ConstBytes.code(),
            result_type: PythType::Bytes.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 5,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 7 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ObjectRevise.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [3, 5, 4, 6],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 8 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ObjectInspect.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [7, 5, 4, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 9 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::HostResult.code(),
            result_type: PythType::Utf8.code(),
            flags: 0,
            block_index: 0,
            inputs: [8, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 4,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_return(&mut package.bytes, nodes_offset + 10 * NODE_RECORD_SIZE, 0);
    write_object_workspace_import(&mut package.bytes, imports_offset, 4, RIGHTS_CREATE);
    package.bytes[constant_pool_offset..constant_pool_offset + 5].copy_from_slice(b"hello");
    package.bytes[string_table_offset..string_table_offset + 4].copy_from_slice(b"note");
    refresh_checksum(&mut package.bytes);
    package
}

pub fn package_with_return_value() -> FixturePackage<TERMINATED_PACKAGE_LEN> {
    structurally_valid_terminated_package_with_return_input()
}

pub fn package_with_jump_argument_count_mismatch()
-> FixturePackage<JUMP_ARGUMENT_COUNT_MISMATCH_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; JUMP_ARGUMENT_COUNT_MISMATCH_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 2, 2, 0, 0, 0);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 1,
            parameter_count: 0,
            flags: 0,
            terminator_node: 0,
        },
    );
    write_block_record(
        &mut package.bytes,
        blocks_offset + BLOCK_RECORD_SIZE,
        BlockSpec {
            block_id: 1,
            first_node: 1,
            node_count: 1,
            parameter_count: 1,
            flags: 0,
            terminator_node: 1,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset,
        NodeSpec {
            opcode: Opcode::Jump.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 1,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_return(&mut package.bytes, nodes_offset + NODE_RECORD_SIZE, 1);
    refresh_checksum(&mut package.bytes);
    package
}

pub fn package_with_jump_argument_type_mismatch()
-> FixturePackage<JUMP_ARGUMENT_TYPE_MISMATCH_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; JUMP_ARGUMENT_TYPE_MISMATCH_PACKAGE_LEN],
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
            parameter_count: 1,
            flags: 0,
            terminator_node: 3,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset,
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
        &mut package.bytes,
        nodes_offset + NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::Jump.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 1,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::BlockParam.code(),
            result_type: PythType::U64.code(),
            flags: 0,
            block_index: 1,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_return(&mut package.bytes, nodes_offset + 3 * NODE_RECORD_SIZE, 1);
    refresh_checksum(&mut package.bytes);
    package
}

pub fn package_with_parameterized_jump() -> FixturePackage<JUMP_ARGUMENT_TYPE_MISMATCH_PACKAGE_LEN>
{
    let mut package = package_with_jump_argument_type_mismatch();
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
    write_u16(&mut package.bytes, nodes_offset, Opcode::ConstU64.code());
    write_u16(&mut package.bytes, nodes_offset + 2, PythType::U64.code());
    write_u64(&mut package.bytes, nodes_offset + 32, 7);
    refresh_checksum(&mut package.bytes);
    package
}

pub fn package_with_capability_constant() -> FixturePackage<CAPABILITY_CONSTANT_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; CAPABILITY_CONSTANT_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 5, 1, 0, 5);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
    let imports_offset = read_u32(&package.bytes, IMPORTS_OFFSET_OFFSET) as usize;
    let string_table_offset = read_u32(&package.bytes, STRING_TABLE_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 5,
            parameter_count: 0,
            flags: 0,
            terminator_node: 4,
        },
    );
    write_node_record(
        &mut package.bytes,
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
        &mut package.bytes,
        nodes_offset + NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ConstU64.code(),
            result_type: PythType::Capability.code(),
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
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ConstUtf8.code(),
            result_type: PythType::Utf8.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 5,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 3 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::SystemLog.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, 2, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 4 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::Return.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_import_record(
        &mut package.bytes,
        imports_offset,
        ImportSpec {
            name_offset: 0,
            name_len: 5,
            resource_kind: crate::pyth_tig::opcode::RESOURCE_SYSTEM_LOG,
            rights: RIGHTS_READ,
            expected_type: PythType::Capability.code(),
            import_slot: 0,
        },
    );
    package.bytes[string_table_offset..string_table_offset + 5].copy_from_slice(b"hello");
    refresh_checksum(&mut package.bytes);
    package
}

pub fn object_revise_with_read_only_import() -> FixturePackage<OBJECT_REVISE_PACKAGE_LEN> {
    let mut package = FixturePackage {
        bytes: [0u8; OBJECT_REVISE_PACKAGE_LEN],
    };
    initialize_graph_header(&mut package.bytes, 0, 1, 6, 1, 0, 8);
    let blocks_offset = read_u32(&package.bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(&package.bytes, NODES_OFFSET_OFFSET) as usize;
    let imports_offset = read_u32(&package.bytes, IMPORTS_OFFSET_OFFSET) as usize;
    let string_table_offset = read_u32(&package.bytes, STRING_TABLE_OFFSET_OFFSET) as usize;

    write_block_record(
        &mut package.bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 6,
            parameter_count: 0,
            flags: 0,
            terminator_node: 5,
        },
    );
    write_node_record(
        &mut package.bytes,
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
        &mut package.bytes,
        nodes_offset + NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::BlockParam.code(),
            result_type: PythType::Capability.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
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
            result_type: PythType::ObjectId.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0xA11C_E001,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 3 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ConstBytes.code(),
            result_type: PythType::Bytes.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 4 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::ObjectRevise.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, 2, 3],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        &mut package.bytes,
        nodes_offset + 5 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::Return.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_import_record(
        &mut package.bytes,
        imports_offset,
        ImportSpec {
            name_offset: 0,
            name_len: 8,
            resource_kind: RESOURCE_OBJECT,
            rights: RIGHTS_READ & !RIGHTS_REVISE,
            expected_type: PythType::Capability.code(),
            import_slot: 0,
        },
    );
    package.bytes[string_table_offset..string_table_offset + 8].copy_from_slice(b"object-0");
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

pub fn set_first_type_flags(bytes: &mut [u8], flags: u16) {
    let types_offset = read_u32(bytes, 60) as usize;
    write_u16(bytes, types_offset + 2, flags);
    refresh_checksum(bytes);
}

pub fn set_first_type_auxiliary(bytes: &mut [u8], auxiliary: u32) {
    let types_offset = read_u32(bytes, 60) as usize;
    write_u32(bytes, types_offset + 4, auxiliary);
    refresh_checksum(bytes);
}

pub fn set_first_block_flags(bytes: &mut [u8], flags: u16) {
    let blocks_offset = read_u32(bytes, BLOCKS_OFFSET_OFFSET) as usize;
    write_u16(bytes, blocks_offset + 14, flags);
    refresh_checksum(bytes);
}

pub fn set_node_flags(bytes: &mut [u8], node_index: usize, flags: u16) {
    let nodes_offset = read_u32(bytes, NODES_OFFSET_OFFSET) as usize;
    write_u16(
        bytes,
        nodes_offset + node_index * NODE_RECORD_SIZE + 4,
        flags,
    );
    refresh_checksum(bytes);
}

pub fn set_node_auxiliary1(bytes: &mut [u8], node_index: usize, auxiliary1: u32) {
    let nodes_offset = read_u32(bytes, NODES_OFFSET_OFFSET) as usize;
    write_u32(
        bytes,
        nodes_offset + node_index * NODE_RECORD_SIZE + 28,
        auxiliary1,
    );
    refresh_checksum(bytes);
}

pub fn set_node_auxiliary0(bytes: &mut [u8], node_index: usize, auxiliary0: u32) {
    let nodes_offset = read_u32(bytes, NODES_OFFSET_OFFSET) as usize;
    write_u32(
        bytes,
        nodes_offset + node_index * NODE_RECORD_SIZE + 24,
        auxiliary0,
    );
    refresh_checksum(bytes);
}

pub fn set_node_immediate(bytes: &mut [u8], node_index: usize, immediate: u64) {
    let nodes_offset = read_u32(bytes, NODES_OFFSET_OFFSET) as usize;
    write_u64(
        bytes,
        nodes_offset + node_index * NODE_RECORD_SIZE + 32,
        immediate,
    );
    refresh_checksum(bytes);
}

pub fn set_first_import_name_range(bytes: &mut [u8], offset: u32, len: u16) {
    let imports_offset = read_u32(bytes, IMPORTS_OFFSET_OFFSET) as usize;
    write_u32(bytes, imports_offset, offset);
    write_u16(bytes, imports_offset + 4, len);
    refresh_checksum(bytes);
}

pub fn set_first_import_rights(bytes: &mut [u8], rights: u64) {
    let imports_offset = read_u32(bytes, IMPORTS_OFFSET_OFFSET) as usize;
    write_u64(bytes, imports_offset + 8, rights);
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

fn write_effect_start(bytes: &mut [u8], offset: usize, block_index: u16) {
    write_node_record(
        bytes,
        offset,
        NodeSpec {
            opcode: Opcode::EffectStart.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
}

fn write_import_capability_param(
    bytes: &mut [u8],
    offset: usize,
    block_index: u16,
    import_slot: u32,
) {
    write_node_record(
        bytes,
        offset,
        NodeSpec {
            opcode: Opcode::BlockParam.code(),
            result_type: PythType::Capability.code(),
            flags: 0,
            block_index,
            inputs: [NO_VALUE; 4],
            auxiliary0: import_slot,
            auxiliary1: 0,
            immediate: 0,
        },
    );
}

fn write_const_utf8(bytes: &mut [u8], offset: usize, block_index: u16, len: u32) {
    write_node_record(
        bytes,
        offset,
        NodeSpec {
            opcode: Opcode::ConstUtf8.code(),
            result_type: PythType::Utf8.code(),
            flags: 0,
            block_index,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: len,
            immediate: 0,
        },
    );
}

fn write_return(bytes: &mut [u8], offset: usize, block_index: u16) {
    write_node_record(
        bytes,
        offset,
        NodeSpec {
            opcode: Opcode::Return.code(),
            result_type: PythType::Unit.code(),
            flags: 0,
            block_index,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
}

fn write_system_log_import(bytes: &mut [u8], offset: usize, name_len: u16, rights: u64) {
    write_import_record(
        bytes,
        offset,
        ImportSpec {
            name_offset: 0,
            name_len,
            resource_kind: crate::pyth_tig::opcode::RESOURCE_SYSTEM_LOG,
            rights,
            expected_type: PythType::Capability.code(),
            import_slot: 0,
        },
    );
}

fn write_object_workspace_import(bytes: &mut [u8], offset: usize, name_len: u16, rights: u64) {
    write_import_record(
        bytes,
        offset,
        ImportSpec {
            name_offset: 0,
            name_len,
            resource_kind: RESOURCE_OBJECT_WORKSPACE,
            rights,
            expected_type: PythType::Capability.code(),
            import_slot: 0,
        },
    );
}

fn initialize_log_with_import_capability_package(bytes: &mut [u8]) {
    initialize_graph_header(bytes, 0, 1, 5, 1, 0, 5);
    let blocks_offset = read_u32(bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(bytes, NODES_OFFSET_OFFSET) as usize;
    let imports_offset = read_u32(bytes, IMPORTS_OFFSET_OFFSET) as usize;
    let string_table_offset = read_u32(bytes, STRING_TABLE_OFFSET_OFFSET) as usize;

    write_block_record(
        bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 5,
            parameter_count: 0,
            flags: 0,
            terminator_node: 4,
        },
    );
    write_effect_start(bytes, nodes_offset, 0);
    write_import_capability_param(bytes, nodes_offset + NODE_RECORD_SIZE, 0, 0);
    write_const_utf8(bytes, nodes_offset + 2 * NODE_RECORD_SIZE, 0, 5);
    write_node_record(
        bytes,
        nodes_offset + 3 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::SystemLog.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, 2, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_return(bytes, nodes_offset + 4 * NODE_RECORD_SIZE, 0);
    write_system_log_import(bytes, imports_offset, 5, RIGHTS_READ);
    bytes[string_table_offset..string_table_offset + 5].copy_from_slice(b"hello");
    refresh_checksum(bytes);
}

fn initialize_log_effect_package(bytes: &mut [u8], import_rights: u64) {
    initialize_graph_header(bytes, 0, 1, 6, 1, 0, 5);
    let blocks_offset = read_u32(bytes, BLOCKS_OFFSET_OFFSET) as usize;
    let nodes_offset = read_u32(bytes, NODES_OFFSET_OFFSET) as usize;
    let imports_offset = read_u32(bytes, IMPORTS_OFFSET_OFFSET) as usize;
    let string_table_offset = read_u32(bytes, STRING_TABLE_OFFSET_OFFSET) as usize;

    write_block_record(
        bytes,
        blocks_offset,
        BlockSpec {
            block_id: 0,
            first_node: 0,
            node_count: 6,
            parameter_count: 0,
            flags: 0,
            terminator_node: 5,
        },
    );
    write_effect_start(bytes, nodes_offset, 0);
    write_import_capability_param(bytes, nodes_offset + NODE_RECORD_SIZE, 0, 0);
    write_const_utf8(bytes, nodes_offset + 2 * NODE_RECORD_SIZE, 0, 5);
    write_node_record(
        bytes,
        nodes_offset + 3 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::SystemLog.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, 2, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_node_record(
        bytes,
        nodes_offset + 4 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::SystemLog.code(),
            result_type: PythType::Effect.code(),
            flags: 0,
            block_index: 0,
            inputs: [0, 1, 2, NO_VALUE],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_return(bytes, nodes_offset + 5 * NODE_RECORD_SIZE, 0);
    write_system_log_import(bytes, imports_offset, 5, import_rights);
    bytes[string_table_offset..string_table_offset + 5].copy_from_slice(b"hello");
    refresh_checksum(bytes);
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
