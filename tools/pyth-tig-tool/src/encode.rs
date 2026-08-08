use pythos_shared::pyth_tig::{
    NO_VALUE,
    format::{PYTH_TIG_MAGIC, PYTH_TIG_MAJOR, PYTH_TIG_MINOR, PythGraphHeader},
    opcode::{Opcode, RESOURCE_SYSTEM_LOG, RIGHTS_READ},
    types::PythType,
};

pub(crate) const HEADER_SIZE: usize = core::mem::size_of::<PythGraphHeader>();
pub(crate) const BLOCK_RECORD_SIZE: usize = 24;
pub(crate) const NODE_RECORD_SIZE: usize = 40;
pub(crate) const IMPORT_RECORD_SIZE: usize = 24;
pub(crate) const CHECKSUM_OFFSET: usize = 84;
pub(crate) const MAJOR_OFFSET: usize = 8;
pub(crate) const MINOR_OFFSET: usize = 10;
pub(crate) const HEADER_RESERVED_OFFSET: usize = 92;
pub(crate) const HEADER_BLOCK_COUNT_OFFSET: usize = 40;
pub(crate) const HEADER_NODE_COUNT_OFFSET: usize = 44;
pub(crate) const HEADER_IMPORT_COUNT_OFFSET: usize = 48;
pub(crate) const HEADER_CONSTANT_POOL_LEN_OFFSET: usize = 52;
pub(crate) const HEADER_STRING_TABLE_LEN_OFFSET: usize = 56;
pub(crate) const HEADER_BLOCKS_OFFSET_OFFSET: usize = 64;
pub(crate) const HEADER_NODES_OFFSET_OFFSET: usize = 68;
pub(crate) const HEADER_IMPORTS_OFFSET_OFFSET: usize = 72;
pub(crate) const HEADER_CONSTANT_POOL_OFFSET_OFFSET: usize = 76;
pub(crate) const HEADER_STRING_TABLE_OFFSET_OFFSET: usize = 80;

const PACKAGE_ID: u64 = 0x5059_5448_5449_4705;
const PRINCIPAL_ID: u64 = 0x5059_5448_5052_4E05;

struct HeaderSpec {
    node_count: u32,
    import_count: u32,
    constant_pool_len: u32,
    string_table_len: u32,
    blocks_offset: u32,
    nodes_offset: u32,
    imports_offset: u32,
    constant_pool_offset: u32,
    string_table_offset: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct NodeSpec {
    pub(crate) opcode: u16,
    pub(crate) result_type: u16,
    pub(crate) inputs: [u32; 4],
    pub(crate) auxiliary0: u32,
    pub(crate) auxiliary1: u32,
    pub(crate) immediate: u64,
}

pub(crate) fn minimal_log_package() -> Vec<u8> {
    build_log_package(
        &[
            effect_start(),
            import_capability(),
            const_utf8(5),
            system_log(0),
            ret(),
        ],
        RIGHTS_READ,
        RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    )
}

pub(crate) fn budget_loop_package() -> Vec<u8> {
    build_log_package(
        &[effect_start(), jump(0)],
        RIGHTS_READ,
        RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    )
}

pub(crate) fn invalid_effect_fork_package() -> Vec<u8> {
    build_log_package(
        &[
            effect_start(),
            import_capability(),
            const_utf8(5),
            system_log(0),
            system_log(0),
            ret(),
        ],
        RIGHTS_READ,
        RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    )
}

pub(crate) fn unsupported_phase2_package() -> Vec<u8> {
    build_log_package(
        &[const_u64(), ret()],
        RIGHTS_READ,
        RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    )
}

pub(crate) fn invalid_string_reference_package() -> Vec<u8> {
    let mut bytes = minimal_log_package();
    let nodes_offset = read_u32(&bytes, HEADER_NODES_OFFSET_OFFSET) as usize;
    write_u32(&mut bytes, nodes_offset + 2 * NODE_RECORD_SIZE + 28, 6);
    refresh_checksum(&mut bytes);
    bytes
}

pub(crate) fn parameterized_jump_package() -> Vec<u8> {
    let blocks_offset = HEADER_SIZE;
    let nodes_offset = blocks_offset + 2 * BLOCK_RECORD_SIZE;
    let imports_offset = nodes_offset + 4 * NODE_RECORD_SIZE;
    let package_len = imports_offset;
    let mut bytes = vec![0u8; package_len];

    write_header(
        &mut bytes,
        HeaderSpec {
            node_count: 4,
            import_count: 0,
            constant_pool_len: 0,
            string_table_len: 0,
            blocks_offset: blocks_offset as u32,
            nodes_offset: nodes_offset as u32,
            imports_offset: imports_offset as u32,
            constant_pool_offset: imports_offset as u32,
            string_table_offset: imports_offset as u32,
        },
    );
    write_u32(&mut bytes, HEADER_BLOCK_COUNT_OFFSET, 2);

    write_u32(&mut bytes, blocks_offset, 0);
    write_u32(&mut bytes, blocks_offset + 4, 0);
    write_u32(&mut bytes, blocks_offset + 8, 2);
    write_u16(&mut bytes, blocks_offset + 12, 0);
    write_u16(&mut bytes, blocks_offset + 14, 0);
    write_u32(&mut bytes, blocks_offset + 16, 1);

    let second_block = blocks_offset + BLOCK_RECORD_SIZE;
    write_u32(&mut bytes, second_block, 1);
    write_u32(&mut bytes, second_block + 4, 2);
    write_u32(&mut bytes, second_block + 8, 2);
    write_u16(&mut bytes, second_block + 12, 1);
    write_u16(&mut bytes, second_block + 14, 0);
    write_u32(&mut bytes, second_block + 16, 3);

    write_node_record(&mut bytes, nodes_offset, const_u64());
    write_node_record(
        &mut bytes,
        nodes_offset + NODE_RECORD_SIZE,
        jump_with_input(1, 0),
    );
    write_node_record(
        &mut bytes,
        nodes_offset + 2 * NODE_RECORD_SIZE,
        NodeSpec {
            opcode: Opcode::BlockParam.code(),
            result_type: PythType::U64.code(),
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
    write_u16(&mut bytes, nodes_offset + 2 * NODE_RECORD_SIZE + 6, 1);
    write_node_record(&mut bytes, nodes_offset + 3 * NODE_RECORD_SIZE, ret());
    write_u16(&mut bytes, nodes_offset + 3 * NODE_RECORD_SIZE + 6, 1);
    refresh_checksum(&mut bytes);
    bytes
}

pub(crate) fn object_create_package() -> Vec<u8> {
    pythos_shared::pyth_tig::test_support::object_note_flow_package().to_vec()
}

pub(crate) fn object_restore_package() -> Vec<u8> {
    pythos_shared::pyth_tig::test_support::object_restore_package().to_vec()
}

pub(crate) fn object_known_denied_package() -> Vec<u8> {
    pythos_shared::pyth_tig::test_support::object_known_denied_package().to_vec()
}

pub(crate) fn object_forgery_package() -> Vec<u8> {
    pythos_shared::pyth_tig::test_support::object_forgery_package().to_vec()
}

pub(crate) fn build_log_package(
    nodes: &[NodeSpec],
    import_rights: u64,
    resource_kind: u16,
    expected_type: u16,
) -> Vec<u8> {
    let string_table = b"hello";
    let block_count = 1usize;
    let import_count = 1usize;

    let blocks_offset = HEADER_SIZE;
    let nodes_offset = blocks_offset + block_count * BLOCK_RECORD_SIZE;
    let imports_offset = nodes_offset + nodes.len() * NODE_RECORD_SIZE;
    let constant_pool_offset = imports_offset + import_count * IMPORT_RECORD_SIZE;
    let string_table_offset = constant_pool_offset;
    let package_len = string_table_offset + string_table.len();

    let mut bytes = vec![0u8; package_len];
    write_header(
        &mut bytes,
        HeaderSpec {
            node_count: nodes.len() as u32,
            import_count: import_count as u32,
            constant_pool_len: 0,
            string_table_len: string_table.len() as u32,
            blocks_offset: blocks_offset as u32,
            nodes_offset: nodes_offset as u32,
            imports_offset: imports_offset as u32,
            constant_pool_offset: constant_pool_offset as u32,
            string_table_offset: string_table_offset as u32,
        },
    );
    write_block_record(&mut bytes, blocks_offset, nodes.len() as u32);
    for (index, node) in nodes.iter().enumerate() {
        write_node_record(&mut bytes, nodes_offset + index * NODE_RECORD_SIZE, *node);
    }
    write_import_record(
        &mut bytes,
        imports_offset,
        string_table.len() as u16,
        resource_kind,
        import_rights,
        expected_type,
    );
    bytes[string_table_offset..string_table_offset + string_table.len()]
        .copy_from_slice(string_table);
    refresh_checksum(&mut bytes);
    bytes
}

pub(crate) fn effect_start() -> NodeSpec {
    NodeSpec {
        opcode: Opcode::EffectStart.code(),
        result_type: PythType::Effect.code(),
        inputs: [NO_VALUE; 4],
        auxiliary0: 0,
        auxiliary1: 0,
        immediate: 0,
    }
}

pub(crate) fn import_capability() -> NodeSpec {
    NodeSpec {
        opcode: Opcode::BlockParam.code(),
        result_type: PythType::Capability.code(),
        inputs: [NO_VALUE; 4],
        auxiliary0: 0,
        auxiliary1: 0,
        immediate: 0,
    }
}

pub(crate) fn const_utf8(len: u32) -> NodeSpec {
    NodeSpec {
        opcode: Opcode::ConstUtf8.code(),
        result_type: PythType::Utf8.code(),
        inputs: [NO_VALUE; 4],
        auxiliary0: 0,
        auxiliary1: len,
        immediate: 0,
    }
}

pub(crate) fn const_bytes(len: u32) -> NodeSpec {
    NodeSpec {
        opcode: Opcode::ConstBytes.code(),
        result_type: PythType::Bytes.code(),
        inputs: [NO_VALUE; 4],
        auxiliary0: 0,
        auxiliary1: len,
        immediate: 0,
    }
}

pub(crate) fn const_bool() -> NodeSpec {
    NodeSpec {
        opcode: Opcode::ConstBool.code(),
        result_type: PythType::Bool.code(),
        inputs: [NO_VALUE; 4],
        auxiliary0: 0,
        auxiliary1: 0,
        immediate: 1,
    }
}

pub(crate) fn const_u64() -> NodeSpec {
    NodeSpec {
        opcode: Opcode::ConstU64.code(),
        result_type: PythType::U64.code(),
        inputs: [NO_VALUE; 4],
        auxiliary0: 0,
        auxiliary1: 0,
        immediate: 1,
    }
}

pub(crate) fn const_capability() -> NodeSpec {
    NodeSpec {
        opcode: Opcode::ConstU64.code(),
        result_type: PythType::Capability.code(),
        inputs: [NO_VALUE; 4],
        auxiliary0: 0,
        auxiliary1: 0,
        immediate: 1,
    }
}

pub(crate) fn system_log(effect_input: u32) -> NodeSpec {
    NodeSpec {
        opcode: Opcode::SystemLog.code(),
        result_type: PythType::Effect.code(),
        inputs: [effect_input, 1, 2, NO_VALUE],
        auxiliary0: 0,
        auxiliary1: 0,
        immediate: 0,
    }
}

pub(crate) fn add_u64(left: u32, right: u32) -> NodeSpec {
    NodeSpec {
        opcode: Opcode::AddU64.code(),
        result_type: PythType::U64.code(),
        inputs: [left, right, NO_VALUE, NO_VALUE],
        auxiliary0: 0,
        auxiliary1: 0,
        immediate: 0,
    }
}

pub(crate) fn jump(target: u32) -> NodeSpec {
    NodeSpec {
        opcode: Opcode::Jump.code(),
        result_type: PythType::Unit.code(),
        inputs: [NO_VALUE; 4],
        auxiliary0: target,
        auxiliary1: 0,
        immediate: 0,
    }
}

pub(crate) fn jump_with_input(target: u32, input: u32) -> NodeSpec {
    NodeSpec {
        opcode: Opcode::Jump.code(),
        result_type: PythType::Unit.code(),
        inputs: [input, NO_VALUE, NO_VALUE, NO_VALUE],
        auxiliary0: target,
        auxiliary1: 0,
        immediate: 0,
    }
}

pub(crate) fn ret() -> NodeSpec {
    NodeSpec {
        opcode: Opcode::Return.code(),
        result_type: PythType::Unit.code(),
        inputs: [NO_VALUE; 4],
        auxiliary0: 0,
        auxiliary1: 0,
        immediate: 0,
    }
}

pub(crate) fn refresh_checksum(bytes: &mut [u8]) {
    write_u64(bytes, CHECKSUM_OFFSET, 0);
    let checksum = digest64(bytes);
    write_u64(bytes, CHECKSUM_OFFSET, checksum);
}

fn write_header(bytes: &mut [u8], spec: HeaderSpec) {
    bytes[0..8].copy_from_slice(&PYTH_TIG_MAGIC);
    write_u16(bytes, MAJOR_OFFSET, PYTH_TIG_MAJOR);
    write_u16(bytes, 10, PYTH_TIG_MINOR);
    write_u64(bytes, 16, PACKAGE_ID);
    write_u64(bytes, 24, PRINCIPAL_ID);
    write_u32(bytes, 32, 0);
    write_u32(bytes, 36, 0);
    write_u32(bytes, 40, 1);
    write_u32(bytes, HEADER_NODE_COUNT_OFFSET, spec.node_count);
    write_u32(bytes, HEADER_IMPORT_COUNT_OFFSET, spec.import_count);
    write_u32(
        bytes,
        HEADER_CONSTANT_POOL_LEN_OFFSET,
        spec.constant_pool_len,
    );
    write_u32(bytes, HEADER_STRING_TABLE_LEN_OFFSET, spec.string_table_len);
    write_u32(bytes, 60, HEADER_SIZE as u32);
    write_u32(bytes, HEADER_BLOCKS_OFFSET_OFFSET, spec.blocks_offset);
    write_u32(bytes, HEADER_NODES_OFFSET_OFFSET, spec.nodes_offset);
    write_u32(bytes, HEADER_IMPORTS_OFFSET_OFFSET, spec.imports_offset);
    write_u32(
        bytes,
        HEADER_CONSTANT_POOL_OFFSET_OFFSET,
        spec.constant_pool_offset,
    );
    write_u32(
        bytes,
        HEADER_STRING_TABLE_OFFSET_OFFSET,
        spec.string_table_offset,
    );
}

fn write_block_record(bytes: &mut [u8], offset: usize, node_count: u32) {
    write_u32(bytes, offset, 0);
    write_u32(bytes, offset + 4, 0);
    write_u32(bytes, offset + 8, node_count);
    write_u16(bytes, offset + 12, 0);
    write_u16(bytes, offset + 14, 0);
    write_u32(bytes, offset + 16, node_count - 1);
    write_u32(bytes, offset + 20, 0);
}

fn write_node_record(bytes: &mut [u8], offset: usize, spec: NodeSpec) {
    write_u16(bytes, offset, spec.opcode);
    write_u16(bytes, offset + 2, spec.result_type);
    write_u16(bytes, offset + 4, 0);
    write_u16(bytes, offset + 6, 0);
    write_u32(bytes, offset + 8, spec.inputs[0]);
    write_u32(bytes, offset + 12, spec.inputs[1]);
    write_u32(bytes, offset + 16, spec.inputs[2]);
    write_u32(bytes, offset + 20, spec.inputs[3]);
    write_u32(bytes, offset + 24, spec.auxiliary0);
    write_u32(bytes, offset + 28, spec.auxiliary1);
    write_u64(bytes, offset + 32, spec.immediate);
}

fn write_import_record(
    bytes: &mut [u8],
    offset: usize,
    name_len: u16,
    resource_kind: u16,
    rights: u64,
    expected_type: u16,
) {
    write_u32(bytes, offset, 0);
    write_u16(bytes, offset + 4, name_len);
    write_u16(bytes, offset + 6, resource_kind);
    write_u64(bytes, offset + 8, rights);
    write_u16(bytes, offset + 16, expected_type);
    write_u16(bytes, offset + 18, 0);
    write_u32(bytes, offset + 20, 0);
}

fn digest64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

pub(crate) fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
