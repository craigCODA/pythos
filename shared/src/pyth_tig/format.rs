pub const PYTH_TIG_MAGIC: [u8; 8] = *b"PYTHTIG1";
pub const PYTH_TIG_MAJOR: u16 = 1;
pub const PYTH_TIG_MINOR: u16 = 1;

pub const MAX_PACKAGE_BYTES: usize = 131_072;
pub const MAX_GRAPH_NODES: usize = 1_024;
pub const MAX_BLOCKS: usize = 128;
pub const MAX_CAPABILITY_IMPORTS: usize = 32;
pub const MAX_CONSTANT_POOL_BYTES: usize = 65_536;
pub const MAX_STRING_TABLE_BYTES: usize = 16_384;
pub const MAX_RUNTIME_VALUES: usize = 1_024;
pub const MAX_EXECUTED_NODES_PER_INVOCATION: usize = 65_536;

pub const NO_VALUE: u32 = u32::MAX;

const PYTH_GRAPH_HEADER_SIZE: usize = core::mem::size_of::<PythGraphHeader>();
const TYPE_RECORD_SIZE: usize = core::mem::size_of::<TypeRecord>();
const BLOCK_RECORD_SIZE: usize = core::mem::size_of::<BlockRecord>();
const NODE_RECORD_SIZE: usize = core::mem::size_of::<NodeRecord>();
const CAPABILITY_IMPORT_RECORD_SIZE: usize = core::mem::size_of::<CapabilityImportRecord>();
const RECORD_SECTION_ALIGNMENT: usize = 4;
const CHECKSUM_OFFSET: usize = 84;
const CHECKSUM_END: usize = 92;

// The frozen v1 public layout struct mirrors the 96-byte header
// with `checksum` at byte 84. Codecs must still read/write explicit LE fields.
#[derive(Clone, Copy)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct TypeRecord {
    pub kind: u16,
    pub flags: u16,
    pub auxiliary: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeRecords<'a> {
    bytes: &'a [u8],
}

impl TypeRecords<'_> {
    pub const fn len(&self) -> usize {
        self.bytes.len() / TYPE_RECORD_SIZE
    }

    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<TypeRecord> {
        record_bytes(self.bytes, index, TYPE_RECORD_SIZE)
            .and_then(|bytes| decode_type_record(bytes, 0).ok())
    }

    pub const fn iter(&self) -> TypeRecordIter<'_> {
        TypeRecordIter {
            records: *self,
            index: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeRecordIter<'a> {
    records: TypeRecords<'a>,
    index: usize,
}

impl Iterator for TypeRecordIter<'_> {
    type Item = TypeRecord;

    fn next(&mut self) -> Option<Self::Item> {
        let record = self.records.get(self.index)?;
        self.index += 1;
        Some(record)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockRecords<'a> {
    bytes: &'a [u8],
}

impl BlockRecords<'_> {
    pub const fn len(&self) -> usize {
        self.bytes.len() / BLOCK_RECORD_SIZE
    }

    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<BlockRecord> {
        record_bytes(self.bytes, index, BLOCK_RECORD_SIZE)
            .and_then(|bytes| decode_block_record(bytes, 0).ok())
    }

    pub const fn iter(&self) -> BlockRecordIter<'_> {
        BlockRecordIter {
            records: *self,
            index: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockRecordIter<'a> {
    records: BlockRecords<'a>,
    index: usize,
}

impl Iterator for BlockRecordIter<'_> {
    type Item = BlockRecord;

    fn next(&mut self) -> Option<Self::Item> {
        let record = self.records.get(self.index)?;
        self.index += 1;
        Some(record)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeRecords<'a> {
    bytes: &'a [u8],
}

impl NodeRecords<'_> {
    pub const fn len(&self) -> usize {
        self.bytes.len() / NODE_RECORD_SIZE
    }

    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<NodeRecord> {
        record_bytes(self.bytes, index, NODE_RECORD_SIZE)
            .and_then(|bytes| decode_node_record(bytes, 0).ok())
    }

    pub const fn iter(&self) -> NodeRecordIter<'_> {
        NodeRecordIter {
            records: *self,
            index: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeRecordIter<'a> {
    records: NodeRecords<'a>,
    index: usize,
}

impl Iterator for NodeRecordIter<'_> {
    type Item = NodeRecord;

    fn next(&mut self) -> Option<Self::Item> {
        let record = self.records.get(self.index)?;
        self.index += 1;
        Some(record)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityImportRecords<'a> {
    bytes: &'a [u8],
}

impl CapabilityImportRecords<'_> {
    pub const fn len(&self) -> usize {
        self.bytes.len() / CAPABILITY_IMPORT_RECORD_SIZE
    }

    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<CapabilityImportRecord> {
        record_bytes(self.bytes, index, CAPABILITY_IMPORT_RECORD_SIZE)
            .and_then(|bytes| decode_capability_import_record(bytes, 0).ok())
    }

    pub const fn iter(&self) -> CapabilityImportRecordIter<'_> {
        CapabilityImportRecordIter {
            records: *self,
            index: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityImportRecordIter<'a> {
    records: CapabilityImportRecords<'a>,
    index: usize,
}

impl Iterator for CapabilityImportRecordIter<'_> {
    type Item = CapabilityImportRecord;

    fn next(&mut self) -> Option<Self::Item> {
        let record = self.records.get(self.index)?;
        self.index += 1;
        Some(record)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageDecodeError {
    PackageTooLarge,
    HeaderTooShort,
    BadMagic,
    UnsupportedMajor,
    UnsupportedMinor,
    UnsupportedFlags,
    NonZeroReserved,
    CountLimit,
    LengthLimit,
    OffsetOverflow,
    SectionOutOfBounds,
    SectionUnaligned,
    SectionOrder,
    SectionOverlap,
    ChecksumMismatch,
    StringOutOfBounds,
}

#[derive(Clone, Copy)]
pub struct PythGraphPackage<'a> {
    bytes: &'a [u8],
    header: PythGraphHeader,
}

impl<'a> PythGraphPackage<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<Self, PackageDecodeError> {
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err(PackageDecodeError::PackageTooLarge);
        }
        if bytes.len() < PYTH_GRAPH_HEADER_SIZE {
            return Err(PackageDecodeError::HeaderTooShort);
        }

        let header = read_header(bytes)?;
        validate_header(header)?;
        validate_sections(bytes, header)?;

        if package_checksum(bytes) != header.checksum {
            return Err(PackageDecodeError::ChecksumMismatch);
        }

        Ok(Self { bytes, header })
    }

    pub const fn header(&self) -> PythGraphHeader {
        self.header
    }

    pub fn types(&self) -> TypeRecords<'a> {
        TypeRecords {
            bytes: self.record_section(
                self.header.types_offset,
                self.header.type_count,
                TYPE_RECORD_SIZE,
            ),
        }
    }

    pub fn blocks(&self) -> BlockRecords<'a> {
        BlockRecords {
            bytes: self.record_section(
                self.header.blocks_offset,
                self.header.block_count,
                BLOCK_RECORD_SIZE,
            ),
        }
    }

    pub fn nodes(&self) -> NodeRecords<'a> {
        NodeRecords {
            bytes: self.record_section(
                self.header.nodes_offset,
                self.header.node_count,
                NODE_RECORD_SIZE,
            ),
        }
    }

    pub fn imports(&self) -> CapabilityImportRecords<'a> {
        CapabilityImportRecords {
            bytes: self.record_section(
                self.header.imports_offset,
                self.header.import_count,
                CAPABILITY_IMPORT_RECORD_SIZE,
            ),
        }
    }

    pub fn constant_pool(&self) -> &'a [u8] {
        self.byte_section(
            self.header.constant_pool_offset,
            self.header.constant_pool_len,
        )
    }

    pub fn string_table(&self) -> &'a [u8] {
        self.byte_section(
            self.header.string_table_offset,
            self.header.string_table_len,
        )
    }

    pub fn string_at(&self, offset: u32, len: u16) -> Result<&'a [u8], PackageDecodeError> {
        let end_offset = offset
            .checked_add(u32::from(len))
            .ok_or(PackageDecodeError::OffsetOverflow)?;
        let start = usize::try_from(offset).map_err(|_| PackageDecodeError::OffsetOverflow)?;
        let end = usize::try_from(end_offset).map_err(|_| PackageDecodeError::OffsetOverflow)?;
        self.string_table()
            .get(start..end)
            .ok_or(PackageDecodeError::StringOutOfBounds)
    }

    fn record_section(&self, offset: u32, count: u32, record_size: usize) -> &'a [u8] {
        let count = usize::try_from(count).expect("validated PythTIG record count fits usize");
        let range = record_range(offset, count, record_size)
            .expect("validated PythTIG record range remains in bounds");
        &self.bytes[range.start..range.end]
    }

    fn byte_section(&self, offset: u32, len: u32) -> &'a [u8] {
        let range =
            byte_range(offset, len).expect("validated PythTIG byte range remains in bounds");
        &self.bytes[range.start..range.end]
    }
}

impl core::fmt::Debug for PythGraphPackage<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PythGraphPackage")
            .field("bytes_len", &self.bytes.len())
            .finish()
    }
}

impl PartialEq for PythGraphPackage<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for PythGraphPackage<'_> {}

#[derive(Clone, Copy)]
struct SectionRange {
    start: usize,
    end: usize,
}

fn read_header(bytes: &[u8]) -> Result<PythGraphHeader, PackageDecodeError> {
    Ok(PythGraphHeader {
        magic: read_magic(bytes)?,
        major: read_u16(bytes, 8)?,
        minor: read_u16(bytes, 10)?,
        flags: read_u32(bytes, 12)?,
        package_id: read_u64(bytes, 16)?,
        principal_id: read_u64(bytes, 24)?,
        entry_block: read_u32(bytes, 32)?,
        type_count: read_u32(bytes, 36)?,
        block_count: read_u32(bytes, 40)?,
        node_count: read_u32(bytes, 44)?,
        import_count: read_u32(bytes, 48)?,
        constant_pool_len: read_u32(bytes, 52)?,
        string_table_len: read_u32(bytes, 56)?,
        types_offset: read_u32(bytes, 60)?,
        blocks_offset: read_u32(bytes, 64)?,
        nodes_offset: read_u32(bytes, 68)?,
        imports_offset: read_u32(bytes, 72)?,
        constant_pool_offset: read_u32(bytes, 76)?,
        string_table_offset: read_u32(bytes, 80)?,
        checksum: read_u64(bytes, CHECKSUM_OFFSET)?,
        reserved: read_u32(bytes, CHECKSUM_END)?,
    })
}

fn validate_header(header: PythGraphHeader) -> Result<(), PackageDecodeError> {
    if header.magic != PYTH_TIG_MAGIC {
        return Err(PackageDecodeError::BadMagic);
    }
    if header.major != PYTH_TIG_MAJOR {
        return Err(PackageDecodeError::UnsupportedMajor);
    }
    if header.minor > PYTH_TIG_MINOR {
        return Err(PackageDecodeError::UnsupportedMinor);
    }
    if header.flags != 0 {
        return Err(PackageDecodeError::UnsupportedFlags);
    }
    if header.reserved != 0 {
        return Err(PackageDecodeError::NonZeroReserved);
    }
    validate_count(header.type_count, MAX_RUNTIME_VALUES)?;
    validate_count(header.block_count, MAX_BLOCKS)?;
    validate_count(header.node_count, MAX_GRAPH_NODES)?;
    validate_count(header.import_count, MAX_CAPABILITY_IMPORTS)?;
    validate_len(header.constant_pool_len, MAX_CONSTANT_POOL_BYTES)?;
    validate_len(header.string_table_len, MAX_STRING_TABLE_BYTES)?;
    Ok(())
}

fn validate_sections(bytes: &[u8], header: PythGraphHeader) -> Result<(), PackageDecodeError> {
    let type_count =
        usize::try_from(header.type_count).map_err(|_| PackageDecodeError::CountLimit)?;
    let block_count =
        usize::try_from(header.block_count).map_err(|_| PackageDecodeError::CountLimit)?;
    let node_count =
        usize::try_from(header.node_count).map_err(|_| PackageDecodeError::CountLimit)?;
    let import_count =
        usize::try_from(header.import_count).map_err(|_| PackageDecodeError::CountLimit)?;

    let types = record_range(header.types_offset, type_count, TYPE_RECORD_SIZE)?;
    let blocks = record_range(header.blocks_offset, block_count, BLOCK_RECORD_SIZE)?;
    let nodes = record_range(header.nodes_offset, node_count, NODE_RECORD_SIZE)?;
    let imports = record_range(
        header.imports_offset,
        import_count,
        CAPABILITY_IMPORT_RECORD_SIZE,
    )?;
    let constant_pool = byte_range(header.constant_pool_offset, header.constant_pool_len)?;
    let string_table = byte_range(header.string_table_offset, header.string_table_len)?;

    validate_record_section(bytes, types)?;
    validate_record_section(bytes, blocks)?;
    validate_record_section(bytes, nodes)?;
    validate_record_section(bytes, imports)?;
    validate_record_reserved(bytes, blocks, block_count, BLOCK_RECORD_SIZE, 20)?;
    validate_record_reserved(
        bytes,
        imports,
        import_count,
        CAPABILITY_IMPORT_RECORD_SIZE,
        20,
    )?;
    validate_byte_section(bytes, constant_pool)?;
    validate_byte_section(bytes, string_table)?;
    validate_section_layout(
        bytes.len(),
        [types, blocks, nodes, imports, constant_pool, string_table],
    )
}

fn validate_count(count: u32, limit: usize) -> Result<(), PackageDecodeError> {
    let count = usize::try_from(count).map_err(|_| PackageDecodeError::CountLimit)?;
    if count > limit {
        return Err(PackageDecodeError::CountLimit);
    }
    Ok(())
}

fn validate_len(len: u32, limit: usize) -> Result<(), PackageDecodeError> {
    let len = usize::try_from(len).map_err(|_| PackageDecodeError::LengthLimit)?;
    if len > limit {
        return Err(PackageDecodeError::LengthLimit);
    }
    Ok(())
}

fn record_range(
    offset: u32,
    count: usize,
    record_size: usize,
) -> Result<SectionRange, PackageDecodeError> {
    let start = usize::try_from(offset).map_err(|_| PackageDecodeError::OffsetOverflow)?;
    let len = count
        .checked_mul(record_size)
        .ok_or(PackageDecodeError::LengthLimit)?;
    range_from_parts(start, len)
}

fn byte_range(offset: u32, len: u32) -> Result<SectionRange, PackageDecodeError> {
    let start = usize::try_from(offset).map_err(|_| PackageDecodeError::OffsetOverflow)?;
    let len = usize::try_from(len).map_err(|_| PackageDecodeError::LengthLimit)?;
    range_from_parts(start, len)
}

fn range_from_parts(start: usize, len: usize) -> Result<SectionRange, PackageDecodeError> {
    let end = start
        .checked_add(len)
        .ok_or(PackageDecodeError::OffsetOverflow)?;
    Ok(SectionRange { start, end })
}

fn validate_record_section(bytes: &[u8], range: SectionRange) -> Result<(), PackageDecodeError> {
    validate_byte_section(bytes, range)?;
    if !range.start.is_multiple_of(RECORD_SECTION_ALIGNMENT) {
        return Err(PackageDecodeError::SectionUnaligned);
    }
    Ok(())
}

fn validate_record_reserved(
    bytes: &[u8],
    range: SectionRange,
    count: usize,
    record_size: usize,
    field_offset: usize,
) -> Result<(), PackageDecodeError> {
    for index in 0..count {
        let record_delta = index
            .checked_mul(record_size)
            .ok_or(PackageDecodeError::LengthLimit)?;
        let record_start = range
            .start
            .checked_add(record_delta)
            .ok_or(PackageDecodeError::OffsetOverflow)?;
        let reserved_offset = record_start
            .checked_add(field_offset)
            .ok_or(PackageDecodeError::OffsetOverflow)?;
        if read_u32(bytes, reserved_offset)? != 0 {
            return Err(PackageDecodeError::NonZeroReserved);
        }
    }
    Ok(())
}

fn validate_byte_section(bytes: &[u8], range: SectionRange) -> Result<(), PackageDecodeError> {
    if range.end > bytes.len() {
        return Err(PackageDecodeError::SectionOutOfBounds);
    }
    Ok(())
}

fn validate_section_layout(
    bytes_len: usize,
    sections: [SectionRange; 6],
) -> Result<(), PackageDecodeError> {
    let mut expected_start = PYTH_GRAPH_HEADER_SIZE;
    for section in sections {
        if section.start < expected_start {
            return Err(PackageDecodeError::SectionOverlap);
        }
        if section.start > expected_start {
            return Err(PackageDecodeError::SectionOrder);
        }
        expected_start = section.end;
    }
    if expected_start != bytes_len {
        return Err(PackageDecodeError::SectionOutOfBounds);
    }
    Ok(())
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

fn read_magic(bytes: &[u8]) -> Result<[u8; 8], PackageDecodeError> {
    read_bytes::<8>(bytes, 0)
}

fn decode_type_record(bytes: &[u8], offset: usize) -> Result<TypeRecord, PackageDecodeError> {
    Ok(TypeRecord {
        kind: read_u16(bytes, offset)?,
        flags: read_u16(bytes, offset + 2)?,
        auxiliary: read_u32(bytes, offset + 4)?,
    })
}

fn decode_block_record(bytes: &[u8], offset: usize) -> Result<BlockRecord, PackageDecodeError> {
    Ok(BlockRecord {
        block_id: read_u32(bytes, offset)?,
        first_node: read_u32(bytes, offset + 4)?,
        node_count: read_u32(bytes, offset + 8)?,
        parameter_count: read_u16(bytes, offset + 12)?,
        flags: read_u16(bytes, offset + 14)?,
        terminator_node: read_u32(bytes, offset + 16)?,
        reserved: read_u32(bytes, offset + 20)?,
    })
}

fn decode_node_record(bytes: &[u8], offset: usize) -> Result<NodeRecord, PackageDecodeError> {
    Ok(NodeRecord {
        opcode: read_u16(bytes, offset)?,
        result_type: read_u16(bytes, offset + 2)?,
        flags: read_u16(bytes, offset + 4)?,
        block_index: read_u16(bytes, offset + 6)?,
        input0: read_u32(bytes, offset + 8)?,
        input1: read_u32(bytes, offset + 12)?,
        input2: read_u32(bytes, offset + 16)?,
        input3: read_u32(bytes, offset + 20)?,
        auxiliary0: read_u32(bytes, offset + 24)?,
        auxiliary1: read_u32(bytes, offset + 28)?,
        immediate: read_u64(bytes, offset + 32)?,
    })
}

fn decode_capability_import_record(
    bytes: &[u8],
    offset: usize,
) -> Result<CapabilityImportRecord, PackageDecodeError> {
    Ok(CapabilityImportRecord {
        name_offset: read_u32(bytes, offset)?,
        name_len: read_u16(bytes, offset + 4)?,
        resource_kind: read_u16(bytes, offset + 6)?,
        rights: read_u64(bytes, offset + 8)?,
        expected_type: read_u16(bytes, offset + 16)?,
        import_slot: read_u16(bytes, offset + 18)?,
        reserved: read_u32(bytes, offset + 20)?,
    })
}

fn record_bytes(bytes: &[u8], index: usize, record_size: usize) -> Option<&[u8]> {
    let start = index.checked_mul(record_size)?;
    let end = start.checked_add(record_size)?;
    bytes.get(start..end)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, PackageDecodeError> {
    Ok(u16::from_le_bytes(read_bytes::<2>(bytes, offset)?))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, PackageDecodeError> {
    Ok(u32::from_le_bytes(read_bytes::<4>(bytes, offset)?))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, PackageDecodeError> {
    Ok(u64::from_le_bytes(read_bytes::<8>(bytes, offset)?))
}

fn read_bytes<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], PackageDecodeError> {
    let end = offset
        .checked_add(N)
        .ok_or(PackageDecodeError::OffsetOverflow)?;
    let raw = bytes
        .get(offset..end)
        .ok_or(PackageDecodeError::HeaderTooShort)?;
    Ok(raw.try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pyth_tig::{opcode::Opcode, types::PythType};

    #[test]
    fn v1_layouts_and_codes_are_recorded() {
        assert_eq!(PYTH_TIG_MAGIC, *b"PYTHTIG1");
        assert_eq!(PYTH_TIG_MAJOR, 1);
        assert_eq!(PYTH_TIG_MINOR, 1);
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

    mod decoder {
        use super::*;
        extern crate std;
        use std::vec::Vec;

        #[test]
        fn decoder_exposes_non_overlapping_sections() {
            let bytes = crate::pyth_tig::test_support::minimal_log_package();
            let package = PythGraphPackage::decode(&bytes).unwrap();
            assert_eq!(package.header().node_count, 3);
            assert_eq!(package.blocks().len(), 1);
            assert_eq!(package.nodes().len(), 3);
            assert_eq!(package.imports().len(), 1);
            assert_eq!(package.types().get(1).unwrap().kind, PythType::Utf8.code());
            assert_eq!(package.blocks().get(0).unwrap().terminator_node, 2);
            assert_eq!(
                package.nodes().get(1).unwrap().opcode,
                Opcode::ConstUtf8.code()
            );
            assert_eq!(package.imports().get(0).unwrap().name_len, 5);
            assert_eq!(package.string_at(0, 5).unwrap(), b"hello");
        }

        #[test]
        fn decoder_accepts_valid_package_from_unaligned_slice_address() {
            let bytes = crate::pyth_tig::test_support::minimal_log_package();
            let mut hosted = Vec::new();
            hosted.push(0xA5);
            hosted.extend_from_slice(&bytes);

            let package = PythGraphPackage::decode(&hosted[1..]).unwrap();
            assert_eq!(package.nodes().len(), 3);
            assert_eq!(package.string_at(0, 5).unwrap(), b"hello");
        }

        #[test]
        fn decoder_rejects_overlapping_sections_and_nonzero_reserved_fields() {
            let mut overlapping = crate::pyth_tig::test_support::minimal_log_package();
            crate::pyth_tig::test_support::set_nodes_offset_equal_blocks_offset(&mut overlapping);
            assert_eq!(
                PythGraphPackage::decode(&overlapping),
                Err(PackageDecodeError::SectionOverlap)
            );

            let mut reserved = crate::pyth_tig::test_support::minimal_log_package();
            crate::pyth_tig::test_support::set_header_reserved(&mut reserved, 1);
            assert_eq!(
                PythGraphPackage::decode(&reserved),
                Err(PackageDecodeError::NonZeroReserved)
            );
        }

        #[test]
        fn decoder_rejects_nonzero_record_reserved_fields() {
            let mut block_reserved = crate::pyth_tig::test_support::minimal_log_package();
            crate::pyth_tig::test_support::set_first_block_reserved(&mut block_reserved, 1);
            assert_eq!(
                PythGraphPackage::decode(&block_reserved),
                Err(PackageDecodeError::NonZeroReserved)
            );

            let mut import_reserved = crate::pyth_tig::test_support::minimal_log_package();
            crate::pyth_tig::test_support::set_first_import_reserved(&mut import_reserved, 1);
            assert_eq!(
                PythGraphPackage::decode(&import_reserved),
                Err(PackageDecodeError::NonZeroReserved)
            );
        }

        #[test]
        fn decoder_rejects_checksum_mismatch_and_out_of_bounds_section_ranges() {
            let mut checksum = crate::pyth_tig::test_support::minimal_log_package();
            crate::pyth_tig::test_support::corrupt_checksum(&mut checksum);
            assert_eq!(
                PythGraphPackage::decode(&checksum),
                Err(PackageDecodeError::ChecksumMismatch)
            );

            let mut out_of_bounds = crate::pyth_tig::test_support::minimal_log_package();
            crate::pyth_tig::test_support::move_string_table_past_end(&mut out_of_bounds);
            assert_eq!(
                PythGraphPackage::decode(&out_of_bounds),
                Err(PackageDecodeError::SectionOutOfBounds)
            );
        }

        #[test]
        fn decoder_handles_zero_count_sections() {
            let bytes = crate::pyth_tig::test_support::empty_package();
            let package = PythGraphPackage::decode(&bytes).unwrap();
            assert_eq!(package.types().len(), 0);
            assert_eq!(package.blocks().len(), 0);
            assert_eq!(package.nodes().len(), 0);
            assert_eq!(package.imports().len(), 0);
            assert_eq!(package.constant_pool(), b"");
            assert_eq!(package.string_table(), b"");
            assert!(package.types().get(0).is_none());
            assert!(package.blocks().get(0).is_none());
            assert!(package.nodes().get(0).is_none());
            assert!(package.imports().get(0).is_none());
        }

        #[test]
        fn decoder_rejects_unsupported_flags_and_versions() {
            let mut flags = crate::pyth_tig::test_support::minimal_log_package();
            crate::pyth_tig::test_support::set_header_flags(&mut flags, 1);
            assert_eq!(
                PythGraphPackage::decode(&flags),
                Err(PackageDecodeError::UnsupportedFlags)
            );

            let mut major = crate::pyth_tig::test_support::minimal_log_package();
            crate::pyth_tig::test_support::set_header_major(&mut major, PYTH_TIG_MAJOR + 1);
            assert_eq!(
                PythGraphPackage::decode(&major),
                Err(PackageDecodeError::UnsupportedMajor)
            );

            let mut minor = crate::pyth_tig::test_support::minimal_log_package();
            crate::pyth_tig::test_support::set_header_minor(&mut minor, 2);
            assert_eq!(
                PythGraphPackage::decode(&minor),
                Err(PackageDecodeError::UnsupportedMinor)
            );
        }

        #[test]
        fn decoder_accepts_version_1_0_and_1_1_packages() {
            let mut v1_0 = crate::pyth_tig::test_support::minimal_log_package();
            crate::pyth_tig::test_support::set_header_minor(&mut v1_0, 0);
            assert!(PythGraphPackage::decode(&v1_0).is_ok());

            let mut v1_1 = crate::pyth_tig::test_support::minimal_log_package();
            crate::pyth_tig::test_support::set_header_minor(&mut v1_1, 1);
            assert!(PythGraphPackage::decode(&v1_1).is_ok());
        }

        #[test]
        fn string_at_rejects_out_of_range_and_overflow_ranges() {
            let bytes = crate::pyth_tig::test_support::minimal_log_package();
            let package = PythGraphPackage::decode(&bytes).unwrap();

            assert_eq!(
                package.string_at(4, 2),
                Err(PackageDecodeError::StringOutOfBounds)
            );
            assert_eq!(
                package.string_at(u32::MAX, 1),
                Err(PackageDecodeError::OffsetOverflow)
            );
        }
    }
}
