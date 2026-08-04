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

const PYTH_GRAPH_HEADER_SIZE: usize = core::mem::size_of::<PythGraphHeader>();
const TYPE_RECORD_SIZE: usize = core::mem::size_of::<TypeRecord>();
const BLOCK_RECORD_SIZE: usize = core::mem::size_of::<BlockRecord>();
const NODE_RECORD_SIZE: usize = core::mem::size_of::<NodeRecord>();
const CAPABILITY_IMPORT_RECORD_SIZE: usize = core::mem::size_of::<CapabilityImportRecord>();
const CHECKSUM_OFFSET: usize = 84;
const CHECKSUM_END: usize = 92;

// The provisional v1 public layout struct mirrors the candidate 96-byte header
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

    pub fn types(&self) -> &'a [TypeRecord] {
        self.record_slice::<TypeRecord>(self.header.types_offset, self.header.type_count)
    }

    pub fn blocks(&self) -> &'a [BlockRecord] {
        self.record_slice::<BlockRecord>(self.header.blocks_offset, self.header.block_count)
    }

    pub fn nodes(&self) -> &'a [NodeRecord] {
        self.record_slice::<NodeRecord>(self.header.nodes_offset, self.header.node_count)
    }

    pub fn imports(&self) -> &'a [CapabilityImportRecord] {
        self.record_slice::<CapabilityImportRecord>(
            self.header.imports_offset,
            self.header.import_count,
        )
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
        let start = usize::try_from(offset).map_err(|_| PackageDecodeError::OffsetOverflow)?;
        let len = usize::from(len);
        let end = start
            .checked_add(len)
            .ok_or(PackageDecodeError::OffsetOverflow)?;
        self.string_table()
            .get(start..end)
            .ok_or(PackageDecodeError::StringOutOfBounds)
    }

    fn record_slice<T>(&self, offset: u32, count: u32) -> &'a [T] {
        let count = usize::try_from(count).expect("validated PythTIG record count fits usize");
        if count == 0 {
            return &[];
        }
        let range = record_range(offset, count, core::mem::size_of::<T>())
            .expect("validated PythTIG record range remains in bounds");
        let ptr = self.bytes.as_ptr().wrapping_add(range.start).cast::<T>();
        // SAFETY:
        // 1. Invariant: the byte range is wholly inside the decoded package and
        //    contains exactly `count * size_of::<T>()` initialized bytes.
        // 2. Established by: `decode()` calling `validate_sections()` with the
        //    same header offsets/counts before constructing `PythGraphPackage`.
        // 3. Lifetime: the returned slice is tied to `self.bytes`, which owns no
        //    mutation authority and outlives `'a`.
        // 4. Pointer ownership: the pointer is derived from the immutable input
        //    slice; the decoder does not take ownership or create mutable aliases.
        // 5. Alignment: `validate_record_section()` checked the section pointer
        //    against `align_of::<T>()` before this accessor can run.
        // 6. Mapped length: `record_range()` and `validate_sections()` checked
        //    multiplication, addition, and package bounds for this byte range.
        // 7. Concurrency: only shared immutable references are produced.
        // 8. Violation consequence: a caller bypassing `decode()` invariants
        //    would make `from_raw_parts` undefined behavior, so construction is
        //    kept private and all public access goes through `decode()`.
        unsafe { core::slice::from_raw_parts(ptr, count) }
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

    validate_record_section(bytes, types, core::mem::align_of::<TypeRecord>())?;
    validate_record_section(bytes, blocks, core::mem::align_of::<BlockRecord>())?;
    validate_record_section(bytes, nodes, core::mem::align_of::<NodeRecord>())?;
    validate_record_section(
        bytes,
        imports,
        core::mem::align_of::<CapabilityImportRecord>(),
    )?;
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

fn validate_record_section(
    bytes: &[u8],
    range: SectionRange,
    align: usize,
) -> Result<(), PackageDecodeError> {
    validate_byte_section(bytes, range)?;
    let ptr = bytes.as_ptr().wrapping_add(range.start);
    if ptr.align_offset(align) != 0 {
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

    mod decoder {
        use super::*;

        #[test]
        fn decoder_exposes_non_overlapping_sections() {
            let bytes = crate::pyth_tig::test_support::minimal_log_package();
            let package = PythGraphPackage::decode(&bytes).unwrap();
            assert_eq!(package.header().node_count, 3);
            assert_eq!(package.blocks().len(), 1);
            assert_eq!(package.nodes().len(), 3);
            assert_eq!(package.imports().len(), 1);
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
    }
}
