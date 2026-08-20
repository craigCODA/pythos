use pyth_codegen_x86_64::{CodegenError, layout::NativeLayout, lower::lower_verified_graph};
use pythos_shared::pyth_tig::{
    NO_VALUE,
    format::{PYTH_TIG_MAGIC, PYTH_TIG_MAJOR, PYTH_TIG_MINOR},
    opcode::Opcode,
    test_support,
    types::PythType,
    verify::{VerifiedGraph, verify_bytes},
};

const HEADER_SIZE: usize = 96;
const BLOCK_RECORD_SIZE: usize = 24;
const NODE_RECORD_SIZE: usize = 40;
const CHECKSUM_OFFSET: usize = 84;
const CHECKSUM_END: usize = 92;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;
const PT_LOAD: u32 = 1;

#[test]
fn assigns_fixed_slots_and_emits_budget_check_per_node() {
    let graph = verified_graph(branch_package());
    let plan = NativeLayout::plan(&graph).unwrap();
    assert_eq!(plan.value_slot_count(), graph.package().nodes().len());

    let image = lower_verified_graph(graph).unwrap();

    assert_eq!(
        image.metadata.budget_checks,
        image.metadata.executable_nodes
    );
    assert!(image.metadata.branch_patches > 0);
}

#[test]
fn rejects_graph_larger_than_native_stack_budget() {
    let graph = verified_graph(linear_package_with_nodes(1024));

    assert_eq!(
        NativeLayout::plan(&graph),
        Err(CodegenError::StackFrameTooLarge {
            required: 16_384,
            maximum: 12_288
        })
    );
}

#[test]
fn records_block_parameter_moves_on_jump_edges() {
    let graph = verified_graph(test_support::package_with_parameterized_jump().to_vec());

    let image = lower_verified_graph(graph).unwrap();

    assert_eq!(image.metadata.block_parameter_moves, 1);
}

#[test]
fn host_operations_load_capabilities_from_bootstrap_not_immediates() {
    let graph = verified_graph(
        test_support::object_create_host_result(
            PythType::ErrorCode,
            pythos_shared::pyth_runtime_abi::HOST_RESULT_STATUS,
        )
        .to_vec(),
    );

    let image = lower_verified_graph(graph).unwrap();

    assert_eq!(image.metadata.capability_immediates, 0);
    assert!(image.metadata.bootstrap_import_loads >= 1);
    assert!(image.metadata.object_syscalls >= 1);
    assert!(image.metadata.host_result_loads >= 1);
}

#[test]
fn generated_request_buffers_are_writable_non_executable_data() {
    let graph = verified_graph(
        test_support::object_create_host_result(
            PythType::ErrorCode,
            pythos_shared::pyth_runtime_abi::HOST_RESULT_STATUS,
        )
        .to_vec(),
    );
    let image = lower_verified_graph(graph).unwrap();
    let parsed = ParsedElf::parse(&image.bytes).unwrap();

    assert!(parsed.bytes_in_segment_with_flags(b"PYTH_OBJECT_REQUEST", PF_R | PF_W));
    assert!(!parsed.bytes_in_segment_with_flags(b"PYTH_OBJECT_REQUEST", PF_R | PF_X));
}

#[test]
fn canonical_object_flows_lower_to_object_stubs() {
    for bytes in [
        test_support::object_note_flow_package().to_vec(),
        test_support::object_restore_package().to_vec(),
        test_support::object_known_denied_package().to_vec(),
    ] {
        let graph = verified_graph(bytes);

        let image = lower_verified_graph(graph).unwrap();

        assert_eq!(image.metadata.capability_immediates, 0);
        assert!(image.metadata.object_syscalls >= 1);
        assert!(image.metadata.host_result_loads >= 1);
    }
}

#[test]
fn system_log_and_graph_exit_use_existing_graph_syscalls() {
    let graph = verified_graph(test_support::system_log_with_import_capability().to_vec());

    let image = lower_verified_graph(graph).unwrap();

    assert_eq!(image.metadata.capability_immediates, 0);
    assert_eq!(image.metadata.bootstrap_import_loads, 1);
    assert_eq!(image.metadata.system_log_syscalls, 1);
    assert!(image.metadata.graph_exit_syscalls >= 1);
}

fn verified_graph(bytes: Vec<u8>) -> VerifiedGraph<'static> {
    let bytes = Box::leak(bytes.into_boxed_slice());
    verify_bytes(bytes).unwrap()
}

fn branch_package() -> Vec<u8> {
    let mut bytes = package_bytes(3, 4);
    let blocks_offset = HEADER_SIZE;
    let nodes_offset = blocks_offset + 3 * BLOCK_RECORD_SIZE;

    write_block_record(&mut bytes, blocks_offset, 0, 0, 2, 0, 1);
    write_block_record(&mut bytes, blocks_offset + BLOCK_RECORD_SIZE, 1, 2, 1, 0, 2);
    write_block_record(
        &mut bytes,
        blocks_offset + 2 * BLOCK_RECORD_SIZE,
        2,
        3,
        1,
        0,
        3,
    );
    write_node_record(
        &mut bytes,
        nodes_offset,
        NodeRecord {
            opcode: Opcode::ConstBool.code(),
            result_type: PythType::Bool.code(),
            block_index: 0,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 1,
        },
    );
    write_node_record(
        &mut bytes,
        nodes_offset + NODE_RECORD_SIZE,
        NodeRecord {
            opcode: Opcode::Branch.code(),
            result_type: PythType::Unit.code(),
            block_index: 0,
            inputs: [0, NO_VALUE, NO_VALUE, NO_VALUE],
            auxiliary0: 1,
            auxiliary1: 2,
            immediate: 0,
        },
    );
    write_return(&mut bytes, nodes_offset + 2 * NODE_RECORD_SIZE, 1);
    write_return(&mut bytes, nodes_offset + 3 * NODE_RECORD_SIZE, 2);
    refresh_checksum(&mut bytes);
    bytes
}

fn linear_package_with_nodes(node_count: usize) -> Vec<u8> {
    let mut bytes = package_bytes(1, node_count);
    let blocks_offset = HEADER_SIZE;
    let nodes_offset = blocks_offset + BLOCK_RECORD_SIZE;

    write_block_record(
        &mut bytes,
        blocks_offset,
        0,
        0,
        node_count as u32,
        0,
        (node_count - 1) as u32,
    );
    for node_index in 0..node_count - 1 {
        write_node_record(
            &mut bytes,
            nodes_offset + node_index * NODE_RECORD_SIZE,
            NodeRecord {
                opcode: Opcode::ConstU64.code(),
                result_type: PythType::U64.code(),
                block_index: 0,
                inputs: [NO_VALUE; 4],
                auxiliary0: 0,
                auxiliary1: 0,
                immediate: node_index as u64,
            },
        );
    }
    write_return(
        &mut bytes,
        nodes_offset + (node_count - 1) * NODE_RECORD_SIZE,
        0,
    );
    refresh_checksum(&mut bytes);
    bytes
}

fn package_bytes(block_count: usize, node_count: usize) -> Vec<u8> {
    let blocks_offset = HEADER_SIZE;
    let nodes_offset = blocks_offset + block_count * BLOCK_RECORD_SIZE;
    let package_len = nodes_offset + node_count * NODE_RECORD_SIZE;
    let mut bytes = vec![0; package_len];

    bytes[0..8].copy_from_slice(&PYTH_TIG_MAGIC);
    write_u16(&mut bytes, 8, PYTH_TIG_MAJOR);
    write_u16(&mut bytes, 10, PYTH_TIG_MINOR);
    write_u64(&mut bytes, 16, 0x5059_5448_4347_0002);
    write_u64(&mut bytes, 24, 0x5059_5448_5052_0002);
    write_u32(&mut bytes, 32, 0);
    write_u32(&mut bytes, 36, 0);
    write_u32(&mut bytes, 40, block_count as u32);
    write_u32(&mut bytes, 44, node_count as u32);
    write_u32(&mut bytes, 48, 0);
    write_u32(&mut bytes, 52, 0);
    write_u32(&mut bytes, 56, 0);
    write_u32(&mut bytes, 60, HEADER_SIZE as u32);
    write_u32(&mut bytes, 64, blocks_offset as u32);
    write_u32(&mut bytes, 68, nodes_offset as u32);
    write_u32(
        &mut bytes,
        72,
        (nodes_offset + node_count * NODE_RECORD_SIZE) as u32,
    );
    write_u32(
        &mut bytes,
        76,
        (nodes_offset + node_count * NODE_RECORD_SIZE) as u32,
    );
    write_u32(
        &mut bytes,
        80,
        (nodes_offset + node_count * NODE_RECORD_SIZE) as u32,
    );
    bytes
}

fn write_block_record(
    bytes: &mut [u8],
    offset: usize,
    block_id: u32,
    first_node: u32,
    node_count: u32,
    parameter_count: u16,
    terminator_node: u32,
) {
    write_u32(bytes, offset, block_id);
    write_u32(bytes, offset + 4, first_node);
    write_u32(bytes, offset + 8, node_count);
    write_u16(bytes, offset + 12, parameter_count);
    write_u16(bytes, offset + 14, 0);
    write_u32(bytes, offset + 16, terminator_node);
    write_u32(bytes, offset + 20, 0);
}

struct NodeRecord {
    opcode: u16,
    result_type: u16,
    block_index: u16,
    inputs: [u32; 4],
    auxiliary0: u32,
    auxiliary1: u32,
    immediate: u64,
}

fn write_node_record(bytes: &mut [u8], offset: usize, node: NodeRecord) {
    write_u16(bytes, offset, node.opcode);
    write_u16(bytes, offset + 2, node.result_type);
    write_u16(bytes, offset + 4, 0);
    write_u16(bytes, offset + 6, node.block_index);
    write_u32(bytes, offset + 8, node.inputs[0]);
    write_u32(bytes, offset + 12, node.inputs[1]);
    write_u32(bytes, offset + 16, node.inputs[2]);
    write_u32(bytes, offset + 20, node.inputs[3]);
    write_u32(bytes, offset + 24, node.auxiliary0);
    write_u32(bytes, offset + 28, node.auxiliary1);
    write_u64(bytes, offset + 32, node.immediate);
}

fn write_return(bytes: &mut [u8], offset: usize, block_index: u16) {
    write_node_record(
        bytes,
        offset,
        NodeRecord {
            opcode: Opcode::Return.code(),
            result_type: PythType::Unit.code(),
            block_index,
            inputs: [NO_VALUE; 4],
            auxiliary0: 0,
            auxiliary1: 0,
            immediate: 0,
        },
    );
}

fn refresh_checksum(bytes: &mut [u8]) {
    write_u64(bytes, CHECKSUM_OFFSET, 0);
    let checksum = package_checksum(bytes);
    write_u64(bytes, CHECKSUM_OFFSET, checksum);
}

fn package_checksum(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for (index, &byte) in bytes.iter().enumerate() {
        let byte = if (CHECKSUM_OFFSET..CHECKSUM_END).contains(&index) {
            0
        } else {
            byte
        };
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

#[derive(Debug)]
struct ParsedElf<'a> {
    bytes: &'a [u8],
    program_headers: Vec<ProgramHeader>,
}

#[derive(Debug)]
struct ProgramHeader {
    kind: u32,
    flags: u32,
    offset: u64,
    filesz: u64,
}

impl<'a> ParsedElf<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < 64 {
            return Err("ELF header too short".to_string());
        }
        if &bytes[0..4] != b"\x7FELF" || bytes[4] != 2 || bytes[5] != 1 {
            return Err("not ELF64 little-endian".to_string());
        }
        let phoff = read_u64_at(bytes, 32)? as usize;
        let phentsize = read_u16_at(bytes, 54)? as usize;
        let phnum = read_u16_at(bytes, 56)? as usize;
        if phentsize != 56 {
            return Err("unexpected program header size".to_string());
        }

        let mut program_headers = Vec::new();
        for index in 0..phnum {
            let offset = phoff + index * phentsize;
            program_headers.push(ProgramHeader {
                kind: read_u32_at(bytes, offset)?,
                flags: read_u32_at(bytes, offset + 4)?,
                offset: read_u64_at(bytes, offset + 8)?,
                filesz: read_u64_at(bytes, offset + 32)?,
            });
        }

        Ok(Self {
            bytes,
            program_headers,
        })
    }

    fn bytes_in_segment_with_flags(&self, needle: &[u8], flags: u32) -> bool {
        self.program_headers
            .iter()
            .filter(|header| header.kind == PT_LOAD && header.flags == flags)
            .any(|header| {
                let start = header.offset as usize;
                let end = start + header.filesz as usize;
                self.bytes
                    .get(start..end)
                    .is_some_and(|segment| segment.windows(needle.len()).any(|win| win == needle))
            })
    }
}

fn read_u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| format!("u16 out of bounds at {offset}"))?;
    Ok(u16::from_le_bytes(value.try_into().unwrap()))
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("u32 out of bounds at {offset}"))?;
    Ok(u32::from_le_bytes(value.try_into().unwrap()))
}

fn read_u64_at(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| format!("u64 out of bounds at {offset}"))?;
    Ok(u64::from_le_bytes(value.try_into().unwrap()))
}
