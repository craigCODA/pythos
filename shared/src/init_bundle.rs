pub const INIT_BUNDLE_MAGIC: &[u8; 16] = b"PYTHOS_BUNDLE_V0";
pub const INIT_BUNDLE_MAJOR: u16 = 0;
pub const INIT_BUNDLE_MINOR: u16 = 0;
pub const INIT_BUNDLE_HEADER_LEN: u32 = 32;
pub const RECORD_ENTRY_LEN: usize = 32;
pub const TYPE_RUNTIME_PAYLOAD: u32 = 0x0000_0001;
pub const TYPE_USER_ELF: u32 = 0x0000_0002;
/// ADR 0051/0052: a named user-program manifest record (see
/// `crate::user_program_manifest`), distinct from the ordinal `TYPE_USER_ELF`.
pub const TYPE_NAMED_USER_ELF: u32 = 0x0000_0003;
pub const TYPE_PYTH_GRAPH_PACKAGE: u32 = 0x0000_0004;
/// PythTIG Phase 6 native-artifact integrity metadata. This record binds one
/// named graph package digest to one named native ELF digest before either is
/// eligible for future native launch handling.
pub const TYPE_PYTH_NATIVE_BINDING: u32 = 0x0000_0005;
/// Phase 13 local package ingress. The record locates bounded package source
/// bytes; a separate capability authorizes reading or installing them.
pub const TYPE_PACKAGE_SOURCE: u32 = 0x0000_0006;
const HEADER_RESERVED_OFFSET: usize = 26;
const HEADER_RESERVED_LEN: usize = 6;
const RECORD_FLAGS_OFFSET: usize = 4;
const RECORD_RESERVED_OFFSET: usize = 28;
const RECORD_RESERVED_LEN: usize = 4;
const MAX_RECORDS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordType {
    RuntimePayload,
    UserElf,
    NamedUserElf,
    PythGraphPackage,
    PythNativeBinding,
    PackageSource,
}

impl RecordType {
    const fn from_raw(value: u32) -> Option<Self> {
        match value {
            TYPE_RUNTIME_PAYLOAD => Some(Self::RuntimePayload),
            TYPE_USER_ELF => Some(Self::UserElf),
            TYPE_NAMED_USER_ELF => Some(Self::NamedUserElf),
            TYPE_PYTH_GRAPH_PACKAGE => Some(Self::PythGraphPackage),
            TYPE_PYTH_NATIVE_BINDING => Some(Self::PythNativeBinding),
            TYPE_PACKAGE_SOURCE => Some(Self::PackageSource),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitBundleError {
    TooShort,
    BadMagic,
    UnsupportedMajor,
    BadHeaderLength,
    EmptyRecordTable,
    NonZeroReserved,
    LengthOverflow,
    BadRecordTable,
    UnsupportedRecordType,
    BadRecordRange,
    OverlappingRecords,
    BadChecksum,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Record<'a> {
    record_type: RecordType,
    bytes: &'a [u8],
}

impl<'a> Record<'a> {
    pub const fn record_type(&self) -> RecordType {
        self.record_type
    }

    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitBundle<'a> {
    records: [Option<Record<'a>>; MAX_RECORDS],
    record_count: usize,
}

impl<'a> InitBundle<'a> {
    pub fn record(&self, record_type: RecordType) -> Option<Record<'a>> {
        self.record_at(record_type, 0)
    }

    pub fn record_at(&self, record_type: RecordType, ordinal: usize) -> Option<Record<'a>> {
        let mut index = 0;
        let mut seen = 0;
        while index < self.record_count {
            if let Some(record) = self.records[index]
                && record.record_type == record_type
            {
                if seen == ordinal {
                    return Some(record);
                }
                seen += 1;
            }
            index += 1;
        }
        None
    }

    pub const fn record_count(&self) -> usize {
        self.record_count
    }
}

pub fn validate(bytes: &[u8]) -> Result<InitBundle<'_>, InitBundleError> {
    if bytes.len() < INIT_BUNDLE_HEADER_LEN as usize {
        return Err(InitBundleError::TooShort);
    }
    if &bytes[..INIT_BUNDLE_MAGIC.len()] != INIT_BUNDLE_MAGIC {
        return Err(InitBundleError::BadMagic);
    }

    let major = read_u16(bytes, 16)?;
    let header_len = read_u32(bytes, 20)?;
    let record_count = usize::from(read_u16(bytes, 24)?);

    if major != INIT_BUNDLE_MAJOR {
        return Err(InitBundleError::UnsupportedMajor);
    }
    if header_len != INIT_BUNDLE_HEADER_LEN || header_len as usize > bytes.len() {
        return Err(InitBundleError::BadHeaderLength);
    }
    if record_count == 0 {
        return Err(InitBundleError::EmptyRecordTable);
    }
    if record_count > MAX_RECORDS {
        return Err(InitBundleError::BadRecordTable);
    }
    if bytes[HEADER_RESERVED_OFFSET..HEADER_RESERVED_OFFSET + HEADER_RESERVED_LEN]
        .iter()
        .any(|&byte| byte != 0)
    {
        return Err(InitBundleError::NonZeroReserved);
    }

    let header_len_usize = header_len as usize;
    let table_len = record_count
        .checked_mul(RECORD_ENTRY_LEN)
        .ok_or(InitBundleError::LengthOverflow)?;
    let table_end = header_len_usize
        .checked_add(table_len)
        .ok_or(InitBundleError::LengthOverflow)?;
    if table_end > bytes.len() {
        return Err(InitBundleError::BadRecordTable);
    }

    let mut ranges = [(0usize, 0usize); MAX_RECORDS];
    let mut records: [Option<Record<'_>>; MAX_RECORDS] = [None; MAX_RECORDS];
    let mut index = 0;
    while index < record_count {
        let entry = header_len_usize
            .checked_add(
                index
                    .checked_mul(RECORD_ENTRY_LEN)
                    .ok_or(InitBundleError::LengthOverflow)?,
            )
            .ok_or(InitBundleError::LengthOverflow)?;
        let record_type = RecordType::from_raw(read_u32(bytes, entry)?)
            .ok_or(InitBundleError::UnsupportedRecordType)?;
        if bytes[entry + RECORD_FLAGS_OFFSET..entry + RECORD_FLAGS_OFFSET + 4]
            .iter()
            .any(|&byte| byte != 0)
            || bytes[entry + RECORD_RESERVED_OFFSET
                ..entry + RECORD_RESERVED_OFFSET + RECORD_RESERVED_LEN]
                .iter()
                .any(|&byte| byte != 0)
        {
            return Err(InitBundleError::NonZeroReserved);
        }
        let start_u64 = read_u64(bytes, entry + 8)?;
        let len_u64 = read_u64(bytes, entry + 16)?;
        let end_u64 = start_u64
            .checked_add(len_u64)
            .ok_or(InitBundleError::LengthOverflow)?;
        if end_u64 > bytes.len() as u64 {
            return Err(InitBundleError::BadRecordRange);
        }
        let start = usize::try_from(start_u64).map_err(|_| InitBundleError::BadRecordRange)?;
        let end = usize::try_from(end_u64).map_err(|_| InitBundleError::BadRecordRange)?;
        if start < table_end || start > end {
            return Err(InitBundleError::BadRecordRange);
        }
        let mut previous = 0;
        while previous < index {
            let (previous_start, previous_end) = ranges[previous];
            if start < previous_end && previous_start < end {
                return Err(InitBundleError::OverlappingRecords);
            }
            previous += 1;
        }
        let payload = bytes
            .get(start..end)
            .ok_or(InitBundleError::BadRecordRange)?;
        if checksum(payload) != read_u32(bytes, entry + 24)? {
            return Err(InitBundleError::BadChecksum);
        }
        ranges[index] = (start, end);
        records[index] = Some(Record {
            record_type,
            bytes: payload,
        });
        index += 1;
    }

    Ok(InitBundle {
        records,
        record_count,
    })
}

pub fn checksum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |sum, &byte| sum.wrapping_add(u32::from(byte)))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, InitBundleError> {
    let end = offset
        .checked_add(2)
        .ok_or(InitBundleError::LengthOverflow)?;
    let raw = bytes.get(offset..end).ok_or(InitBundleError::TooShort)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, InitBundleError> {
    let end = offset
        .checked_add(4)
        .ok_or(InitBundleError::LengthOverflow)?;
    let raw = bytes.get(offset..end).ok_or(InitBundleError::TooShort)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, InitBundleError> {
    let end = offset
        .checked_add(8)
        .ok_or(InitBundleError::LengthOverflow)?;
    let raw = bytes.get(offset..end).ok_or(InitBundleError::TooShort)?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::vec;
    use std::vec::Vec;

    fn build_bundle(records: &[(u32, &[u8])]) -> Vec<u8> {
        let header_len = INIT_BUNDLE_HEADER_LEN as usize;
        let table_len = records.len() * RECORD_ENTRY_LEN;
        let mut bytes = vec![0u8; header_len + table_len];
        bytes[..INIT_BUNDLE_MAGIC.len()].copy_from_slice(INIT_BUNDLE_MAGIC);
        bytes[16..18].copy_from_slice(&INIT_BUNDLE_MAJOR.to_le_bytes());
        bytes[18..20].copy_from_slice(&INIT_BUNDLE_MINOR.to_le_bytes());
        bytes[20..24].copy_from_slice(&INIT_BUNDLE_HEADER_LEN.to_le_bytes());
        bytes[24..26].copy_from_slice(&(records.len() as u16).to_le_bytes());

        let mut cursor = header_len + table_len;
        for (index, (record_type, payload)) in records.iter().enumerate() {
            let entry = header_len + index * RECORD_ENTRY_LEN;
            bytes[entry..entry + 4].copy_from_slice(&record_type.to_le_bytes());
            bytes[entry + 8..entry + 16].copy_from_slice(&(cursor as u64).to_le_bytes());
            bytes[entry + 16..entry + 24].copy_from_slice(&(payload.len() as u64).to_le_bytes());
            bytes[entry + 24..entry + 28].copy_from_slice(&checksum(payload).to_le_bytes());
            bytes.extend_from_slice(payload);
            cursor += payload.len();
        }
        bytes
    }

    #[test]
    fn valid_bundle_exposes_runtime_and_user_elf_records() {
        let bundle = build_bundle(&[(TYPE_RUNTIME_PAYLOAD, b"runtime"), (TYPE_USER_ELF, b"elf")]);

        let parsed = validate(&bundle).unwrap();

        assert_eq!(
            parsed.record(RecordType::RuntimePayload).unwrap().bytes(),
            b"runtime"
        );
        assert_eq!(parsed.record(RecordType::UserElf).unwrap().bytes(), b"elf");
    }

    #[test]
    fn named_user_elf_record_is_addressable_alongside_existing_types() {
        let bundle = build_bundle(&[
            (TYPE_RUNTIME_PAYLOAD, b"runtime"),
            (TYPE_USER_ELF, b"elf"),
            (TYPE_NAMED_USER_ELF, b"named-manifest"),
        ]);

        let parsed = validate(&bundle).unwrap();

        assert_eq!(
            parsed.record(RecordType::NamedUserElf).unwrap().bytes(),
            b"named-manifest"
        );
        // Existing ordinal lookups are unaffected by the new record type.
        assert_eq!(parsed.record(RecordType::UserElf).unwrap().bytes(), b"elf");
    }

    #[test]
    fn pyth_graph_package_record_is_addressable_without_disturbing_existing_types() {
        let bundle = build_bundle(&[
            (TYPE_RUNTIME_PAYLOAD, b"runtime"),
            (TYPE_USER_ELF, b"elf"),
            (TYPE_NAMED_USER_ELF, b"named-manifest"),
            (TYPE_PYTH_GRAPH_PACKAGE, b"graph-manifest"),
        ]);

        let parsed = validate(&bundle).unwrap();

        assert_eq!(
            parsed.record(RecordType::PythGraphPackage).unwrap().bytes(),
            b"graph-manifest"
        );
        assert_eq!(
            parsed.record(RecordType::NamedUserElf).unwrap().bytes(),
            b"named-manifest"
        );
        assert_eq!(parsed.record(RecordType::UserElf).unwrap().bytes(), b"elf");
    }

    #[test]
    fn pyth_native_binding_record_is_addressable_alongside_graph_and_elf_records() {
        let bundle = build_bundle(&[
            (TYPE_PYTH_GRAPH_PACKAGE, b"graph-manifest"),
            (TYPE_NAMED_USER_ELF, b"elf-manifest"),
            (TYPE_PYTH_NATIVE_BINDING, b"native-binding"),
        ]);

        let parsed = validate(&bundle).unwrap();

        assert_eq!(
            parsed
                .record(RecordType::PythNativeBinding)
                .unwrap()
                .bytes(),
            b"native-binding"
        );
        assert_eq!(
            parsed.record(RecordType::PythGraphPackage).unwrap().bytes(),
            b"graph-manifest"
        );
        assert_eq!(
            parsed.record(RecordType::NamedUserElf).unwrap().bytes(),
            b"elf-manifest"
        );
    }

    #[test]
    fn duplicate_user_elf_records_are_addressable_by_ordinal() {
        let bundle = build_bundle(&[
            (TYPE_RUNTIME_PAYLOAD, b"runtime"),
            (TYPE_USER_ELF, b"first"),
            (TYPE_USER_ELF, b"second"),
        ]);

        let parsed = validate(&bundle).unwrap();

        assert_eq!(
            parsed.record_at(RecordType::UserElf, 0).unwrap().bytes(),
            b"first"
        );
        assert_eq!(
            parsed.record_at(RecordType::UserElf, 1).unwrap().bytes(),
            b"second"
        );
        assert_eq!(parsed.record_at(RecordType::UserElf, 2), None);
    }

    #[test]
    fn phase_2_runtime_bundle_record_count_is_admitted() {
        let bundle = build_bundle(&[
            (TYPE_RUNTIME_PAYLOAD, b"runtime"),
            (TYPE_NAMED_USER_ELF, b"shell"),
            (TYPE_NAMED_USER_ELF, b"pyth-runtime"),
            (TYPE_PYTH_GRAPH_PACKAGE, b"hello"),
            (TYPE_PYTH_GRAPH_PACKAGE, b"budget"),
            (TYPE_PYTH_GRAPH_PACKAGE, b"invalid"),
            (TYPE_USER_ELF, b"fault-breakpoint"),
            (TYPE_USER_ELF, b"fault-ud2"),
            (TYPE_USER_ELF, b"fault-bad-pointer"),
            (TYPE_USER_ELF, b"fault-hardware"),
        ]);

        let parsed = validate(&bundle).unwrap();

        assert_eq!(parsed.record_count(), 10);
        assert_eq!(
            parsed
                .record_at(RecordType::PythGraphPackage, 2)
                .unwrap()
                .bytes(),
            b"invalid"
        );
        assert_eq!(
            parsed.record_at(RecordType::UserElf, 3).unwrap().bytes(),
            b"fault-hardware"
        );
    }

    #[test]
    fn record_range_overflow_is_rejected() {
        let mut bundle = build_bundle(&[(TYPE_RUNTIME_PAYLOAD, b"runtime")]);
        bundle[40..48].copy_from_slice(&u64::MAX.to_le_bytes());

        assert_eq!(validate(&bundle), Err(InitBundleError::LengthOverflow));
    }

    #[test]
    fn checksum_mismatch_is_rejected() {
        let mut bundle = build_bundle(&[(TYPE_RUNTIME_PAYLOAD, b"runtime")]);
        let last = bundle.len() - 1;
        bundle[last] ^= 0xFF;

        assert_eq!(validate(&bundle), Err(InitBundleError::BadChecksum));
    }
}
