use crate::{
    graph::{GraphBlock, GraphImport, GraphNode, OwnedGraph},
    span::{Diagnostic, Span},
};
use pythos_shared::pyth_tig::{
    format::{
        CapabilityImportRecord, NodeRecord, PYTH_TIG_MAGIC, PYTH_TIG_MAJOR, PYTH_TIG_MINOR,
        PythGraphHeader, PythGraphPackage, TypeRecord,
    },
    verify::verify_package,
};

const HEADER_SIZE: usize = core::mem::size_of::<PythGraphHeader>();
const TYPE_RECORD_SIZE: usize = core::mem::size_of::<TypeRecord>();
const BLOCK_RECORD_SIZE: usize = core::mem::size_of::<pythos_shared::pyth_tig::BlockRecord>();
const NODE_RECORD_SIZE: usize = core::mem::size_of::<NodeRecord>();
const IMPORT_RECORD_SIZE: usize = core::mem::size_of::<CapabilityImportRecord>();
const CHECKSUM_OFFSET: usize = 84;
const CHECKSUM_END: usize = 92;
const PACKAGE_ID: u64 = 0x5059_5448_5449_4706;

pub fn encode_verified_graph(graph: &OwnedGraph) -> Result<Vec<u8>, Diagnostic> {
    let bytes = encode_graph(graph)?;
    let package = PythGraphPackage::decode(&bytes).map_err(|_| verifier_rejected())?;
    verify_package(&package).map_err(|_| verifier_rejected())?;
    Ok(bytes)
}

fn encode_graph(graph: &OwnedGraph) -> Result<Vec<u8>, Diagnostic> {
    let type_count = 0usize;
    let block_count = graph.blocks.len();
    let node_count = graph.nodes.len();
    let import_count = graph.imports.len();
    let constant_pool_len = graph.constant_pool.len();
    let string_table_len = graph.string_table.len();

    let types_offset = HEADER_SIZE;
    let blocks_offset = types_offset + type_count * TYPE_RECORD_SIZE;
    let nodes_offset = blocks_offset + block_count * BLOCK_RECORD_SIZE;
    let imports_offset = nodes_offset + node_count * NODE_RECORD_SIZE;
    let constant_pool_offset = imports_offset + import_count * IMPORT_RECORD_SIZE;
    let string_table_offset = constant_pool_offset + constant_pool_len;
    let package_len = string_table_offset + string_table_len;

    let mut bytes = vec![0u8; package_len];
    write_header(
        &mut bytes,
        graph,
        block_count,
        node_count,
        import_count,
        constant_pool_len,
        string_table_len,
        types_offset,
        blocks_offset,
        nodes_offset,
        imports_offset,
        constant_pool_offset,
        string_table_offset,
    )?;

    for (index, block) in graph.blocks.iter().enumerate() {
        write_block(&mut bytes, blocks_offset + index * BLOCK_RECORD_SIZE, block);
    }
    for (index, node) in graph.nodes.iter().enumerate() {
        write_node(&mut bytes, nodes_offset + index * NODE_RECORD_SIZE, node);
    }
    for (index, import) in graph.imports.iter().enumerate() {
        write_import(
            &mut bytes,
            imports_offset + index * IMPORT_RECORD_SIZE,
            import,
        );
    }

    bytes[constant_pool_offset..constant_pool_offset + constant_pool_len]
        .copy_from_slice(&graph.constant_pool);
    bytes[string_table_offset..string_table_offset + string_table_len]
        .copy_from_slice(&graph.string_table);

    refresh_checksum(&mut bytes);
    Ok(bytes)
}

#[allow(clippy::too_many_arguments)]
fn write_header(
    bytes: &mut [u8],
    graph: &OwnedGraph,
    block_count: usize,
    node_count: usize,
    import_count: usize,
    constant_pool_len: usize,
    string_table_len: usize,
    types_offset: usize,
    blocks_offset: usize,
    nodes_offset: usize,
    imports_offset: usize,
    constant_pool_offset: usize,
    string_table_offset: usize,
) -> Result<(), Diagnostic> {
    bytes[0..8].copy_from_slice(&PYTH_TIG_MAGIC);
    write_u16(bytes, 8, PYTH_TIG_MAJOR);
    write_u16(bytes, 10, PYTH_TIG_MINOR);
    write_u64(bytes, 16, PACKAGE_ID);
    write_u64(bytes, 24, graph.principal_id);
    write_u32(bytes, 32, 0);
    write_u32(bytes, 36, 0);
    write_u32(bytes, 40, checked_u32(block_count)?);
    write_u32(bytes, 44, checked_u32(node_count)?);
    write_u32(bytes, 48, checked_u32(import_count)?);
    write_u32(bytes, 52, checked_u32(constant_pool_len)?);
    write_u32(bytes, 56, checked_u32(string_table_len)?);
    write_u32(bytes, 60, checked_u32(types_offset)?);
    write_u32(bytes, 64, checked_u32(blocks_offset)?);
    write_u32(bytes, 68, checked_u32(nodes_offset)?);
    write_u32(bytes, 72, checked_u32(imports_offset)?);
    write_u32(bytes, 76, checked_u32(constant_pool_offset)?);
    write_u32(bytes, 80, checked_u32(string_table_offset)?);
    Ok(())
}

fn write_block(bytes: &mut [u8], offset: usize, block: &GraphBlock) {
    write_u32(bytes, offset, block.block_id);
    write_u32(bytes, offset + 4, block.first_node);
    write_u32(bytes, offset + 8, block.node_count);
    write_u16(bytes, offset + 12, block.parameter_count);
    write_u16(bytes, offset + 14, 0);
    write_u32(bytes, offset + 16, block.terminator_node);
    write_u32(bytes, offset + 20, 0);
}

fn write_node(bytes: &mut [u8], offset: usize, node: &GraphNode) {
    write_u16(bytes, offset, node.opcode.code());
    write_u16(bytes, offset + 2, node.result_type.code());
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

fn write_import(bytes: &mut [u8], offset: usize, import: &GraphImport) {
    write_u32(bytes, offset, import.name_offset);
    write_u16(bytes, offset + 4, import.name_len);
    write_u16(bytes, offset + 6, import.resource_kind);
    write_u64(bytes, offset + 8, import.rights);
    write_u16(bytes, offset + 16, import.expected_type.code());
    write_u16(bytes, offset + 18, import.import_slot);
    write_u32(bytes, offset + 20, 0);
}

fn refresh_checksum(bytes: &mut [u8]) {
    write_u64(bytes, CHECKSUM_OFFSET, 0);
    let checksum = digest64(bytes);
    write_u64(bytes, CHECKSUM_OFFSET, checksum);
}

fn digest64(bytes: &[u8]) -> u64 {
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

fn checked_u32(value: usize) -> Result<u32, Diagnostic> {
    u32::try_from(value).map_err(|_| verifier_rejected())
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

fn verifier_rejected() -> Diagnostic {
    Diagnostic::new(
        "G0001",
        "shared verifier rejected compiler output",
        Span::default(),
    )
}
