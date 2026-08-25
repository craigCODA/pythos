use crate::package_abi::{
    MAX_CONTENT_BYTES, MAX_CONTENT_ENTRIES, MAX_CONTENT_EXTENTS_PER_RECORD,
    MAX_CONTENT_TABLE_BYTES, MAX_MANIFEST_BYTES, MAX_MANIFEST_RECORD_PAYLOAD_BYTES,
    MAX_MANIFEST_RECORDS, MAX_PACKAGE_ARTIFACT_BYTES, MAX_STABLE_NAME_BYTES,
};
use crate::sha256::{Sha256, sha256};

pub const PACKAGE_ARTIFACT_MAGIC: &[u8; 8] = b"PYTHPKG0";
pub const PACKAGE_ARTIFACT_MAJOR: u16 = 0;
pub const PACKAGE_ARTIFACT_MINOR: u16 = 1;
pub const PACKAGE_ARTIFACT_HEADER_LEN: usize = 160;
pub const PACKAGE_MANIFEST_MAGIC: &[u8; 8] = b"PYTHMAN0";
pub const PACKAGE_MANIFEST_HEADER_LEN: usize = 12;
pub const MANIFEST_RECORD_HEADER_LEN: usize = 10;
pub const CONTENT_ENTRY_V0_LEN: usize = 64;

const MAGIC_OFFSET: usize = 0;
const MAJOR_OFFSET: usize = 8;
const MINOR_OFFSET: usize = 10;
const HEADER_LEN_OFFSET: usize = 12;
const MANIFEST_OFFSET_OFFSET: usize = 16;
const MANIFEST_LENGTH_OFFSET: usize = 24;
const CONTENT_TABLE_OFFSET_OFFSET: usize = 32;
const CONTENT_TABLE_LENGTH_OFFSET: usize = 40;
const CONTENT_BYTES_OFFSET_OFFSET: usize = 48;
const CONTENT_BYTES_LENGTH_OFFSET: usize = 56;
const MANIFEST_SHA256_OFFSET: usize = 64;
const ARTIFACT_SHA256_OFFSET: usize = 96;
const RESERVED_OFFSET: usize = 128;
const RESERVED_END: usize = PACKAGE_ARTIFACT_HEADER_LEN;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageFormatError {
    TooShort,
    InvalidMagic,
    UnsupportedMajor,
    UnsupportedRequiredMinor,
    InvalidHeaderLength,
    NonZeroReserved,
    LengthOverflow,
    BoundsExceeded,
    ManifestDigestMismatch,
    ArtifactDigestMismatch,
    InvalidManifest,
    DuplicateStableName,
    UnsortedManifestRecord,
    StableNameTooLong,
    ManifestPayloadTooLong,
    TooManyManifestRecords,
    TooManyContentEntries,
    TooManyContentExtents,
    ContentRangeOutsidePayload,
    ContentDigestMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManifestV0<'a> {
    bytes: &'a [u8],
    record_count: u32,
}

impl<'a> ManifestV0<'a> {
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub const fn record_count(&self) -> u32 {
        self.record_count
    }

    pub fn record(&self, index: u32) -> Option<ManifestRecordV0<'a>> {
        if index >= self.record_count {
            return None;
        }
        let mut offset = PACKAGE_MANIFEST_HEADER_LEN;
        let mut current = 0;
        while current < index {
            let name_len = read_u16(self.bytes, offset + 4) as usize;
            let payload_len = read_u32(self.bytes, offset + 6) as usize;
            offset += MANIFEST_RECORD_HEADER_LEN + name_len + payload_len;
            current += 1;
        }
        Some(read_manifest_record(self.bytes, offset).0)
    }

    fn parse(bytes: &'a [u8]) -> Result<Self, PackageFormatError> {
        if bytes.len() < PACKAGE_MANIFEST_HEADER_LEN {
            return Err(PackageFormatError::InvalidManifest);
        }
        if &bytes[..8] != PACKAGE_MANIFEST_MAGIC {
            return Err(PackageFormatError::InvalidManifest);
        }
        let record_count = read_u32(bytes, 8);
        if record_count as usize > MAX_MANIFEST_RECORDS {
            return Err(PackageFormatError::TooManyManifestRecords);
        }

        let mut offset = PACKAGE_MANIFEST_HEADER_LEN;
        let mut previous: Option<(u16, &'a [u8])> = None;
        let mut index = 0;
        while index < record_count {
            let (record, next) = read_manifest_record_checked(bytes, offset)?;
            if let Some((previous_type, previous_name)) = previous {
                match compare_manifest_key(
                    previous_type,
                    previous_name,
                    record.record_type,
                    record.stable_name,
                ) {
                    KeyOrder::Less => {}
                    KeyOrder::Equal => return Err(PackageFormatError::DuplicateStableName),
                    KeyOrder::Greater => return Err(PackageFormatError::UnsortedManifestRecord),
                }
            }
            previous = Some((record.record_type, record.stable_name));
            offset = next;
            index += 1;
        }
        if offset != bytes.len() {
            return Err(PackageFormatError::BoundsExceeded);
        }

        Ok(Self {
            bytes,
            record_count,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManifestRecordV0<'a> {
    record_type: u16,
    stable_name: &'a [u8],
    payload: &'a [u8],
}

impl<'a> ManifestRecordV0<'a> {
    pub const fn record_type(&self) -> u16 {
        self.record_type
    }

    pub const fn stable_name(&self) -> &'a [u8] {
        self.stable_name
    }

    pub const fn payload(&self) -> &'a [u8] {
        self.payload
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentEntryV0 {
    pub content_index: u16,
    pub role: u16,
    pub format: u16,
    pub extent_count: u16,
    pub offset: u64,
    pub length: u64,
    pub sha256: [u8; 32],
    pub declared_runtime: u16,
    pub declared_entrypoint: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageArtifactV0<'a> {
    bytes: &'a [u8],
    content_table_range: Range,
    content_payload_range: Range,
    manifest_sha256: [u8; 32],
    artifact_sha256: [u8; 32],
    manifest: ManifestV0<'a>,
}

impl<'a> PackageArtifactV0<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, PackageFormatError> {
        if bytes.len() < PACKAGE_ARTIFACT_HEADER_LEN {
            return Err(PackageFormatError::TooShort);
        }
        if bytes.len() > MAX_PACKAGE_ARTIFACT_BYTES {
            return Err(PackageFormatError::BoundsExceeded);
        }
        if &bytes[MAGIC_OFFSET..MAGIC_OFFSET + 8] != PACKAGE_ARTIFACT_MAGIC {
            return Err(PackageFormatError::InvalidMagic);
        }

        let major = read_u16(bytes, MAJOR_OFFSET);
        let minor = read_u16(bytes, MINOR_OFFSET);
        if major != PACKAGE_ARTIFACT_MAJOR {
            return Err(PackageFormatError::UnsupportedMajor);
        }
        if minor > PACKAGE_ARTIFACT_MINOR {
            return Err(PackageFormatError::UnsupportedRequiredMinor);
        }
        if read_u32(bytes, HEADER_LEN_OFFSET) as usize != PACKAGE_ARTIFACT_HEADER_LEN {
            return Err(PackageFormatError::InvalidHeaderLength);
        }
        if bytes[RESERVED_OFFSET..RESERVED_END]
            .iter()
            .any(|&byte| byte != 0)
        {
            return Err(PackageFormatError::NonZeroReserved);
        }

        let manifest_range = checked_range(
            read_u64(bytes, MANIFEST_OFFSET_OFFSET),
            read_u64(bytes, MANIFEST_LENGTH_OFFSET),
            bytes.len(),
            MAX_MANIFEST_BYTES,
        )?;
        let content_table_range = checked_range(
            read_u64(bytes, CONTENT_TABLE_OFFSET_OFFSET),
            read_u64(bytes, CONTENT_TABLE_LENGTH_OFFSET),
            bytes.len(),
            MAX_CONTENT_TABLE_BYTES,
        )?;
        let content_payload_range = checked_range(
            read_u64(bytes, CONTENT_BYTES_OFFSET_OFFSET),
            read_u64(bytes, CONTENT_BYTES_LENGTH_OFFSET),
            bytes.len(),
            MAX_CONTENT_BYTES,
        )?;

        let manifest_sha256 = read_sha256(bytes, MANIFEST_SHA256_OFFSET);
        if sha256(manifest_range.slice(bytes)) != manifest_sha256 {
            return Err(PackageFormatError::ManifestDigestMismatch);
        }
        let manifest = ManifestV0::parse(manifest_range.slice(bytes))?;

        let artifact_sha256 = read_sha256(bytes, ARTIFACT_SHA256_OFFSET);
        if artifact_digest(bytes) != artifact_sha256 {
            return Err(PackageFormatError::ArtifactDigestMismatch);
        }
        validate_content_table(
            content_table_range.slice(bytes),
            content_payload_range.slice(bytes),
        )?;

        Ok(Self {
            bytes,
            content_table_range,
            content_payload_range,
            manifest_sha256,
            artifact_sha256,
            manifest,
        })
    }

    pub const fn artifact_sha256(&self) -> [u8; 32] {
        self.artifact_sha256
    }

    pub const fn manifest_sha256(&self) -> [u8; 32] {
        self.manifest_sha256
    }

    pub const fn manifest(&self) -> ManifestV0<'a> {
        self.manifest
    }

    pub fn content_table_bytes(&self) -> &'a [u8] {
        self.content_table_range.slice(self.bytes)
    }

    pub fn content_payload_bytes(&self) -> &'a [u8] {
        self.content_payload_range.slice(self.bytes)
    }

    pub fn content_entry(&self, index: u16) -> Option<ContentEntryV0> {
        let table = self.content_table_bytes();
        let count = table.len() / CONTENT_ENTRY_V0_LEN;
        let index = index as usize;
        if index >= count {
            return None;
        }
        Some(read_content_entry(table, index * CONTENT_ENTRY_V0_LEN))
    }

    pub fn content_bytes(&self, entry: ContentEntryV0) -> Result<&'a [u8], PackageFormatError> {
        content_slice(self.content_payload_bytes(), entry)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Range {
    start: usize,
    end: usize,
}

impl Range {
    pub fn slice(self, bytes: &[u8]) -> &[u8] {
        &bytes[self.start..self.end]
    }
}

fn checked_range(
    offset: u64,
    length: u64,
    bytes_len: usize,
    max_len: usize,
) -> Result<Range, PackageFormatError> {
    let start = usize::try_from(offset).map_err(|_| PackageFormatError::BoundsExceeded)?;
    let len = usize::try_from(length).map_err(|_| PackageFormatError::BoundsExceeded)?;
    if len > max_len {
        return Err(PackageFormatError::BoundsExceeded);
    }
    let end = start
        .checked_add(len)
        .ok_or(PackageFormatError::LengthOverflow)?;
    if start < PACKAGE_ARTIFACT_HEADER_LEN || end > bytes_len {
        return Err(PackageFormatError::BoundsExceeded);
    }
    Ok(Range { start, end })
}

fn read_manifest_record_checked<'a>(
    bytes: &'a [u8],
    offset: usize,
) -> Result<(ManifestRecordV0<'a>, usize), PackageFormatError> {
    let header_end = offset
        .checked_add(MANIFEST_RECORD_HEADER_LEN)
        .ok_or(PackageFormatError::LengthOverflow)?;
    if header_end > bytes.len() {
        return Err(PackageFormatError::BoundsExceeded);
    }
    let flags = read_u16(bytes, offset + 2);
    if flags != 0 {
        return Err(PackageFormatError::InvalidManifest);
    }
    let name_len = read_u16(bytes, offset + 4) as usize;
    let payload_len = read_u32(bytes, offset + 6) as usize;
    if name_len == 0 {
        return Err(PackageFormatError::InvalidManifest);
    }
    if name_len > MAX_STABLE_NAME_BYTES {
        return Err(PackageFormatError::StableNameTooLong);
    }
    if payload_len > MAX_MANIFEST_RECORD_PAYLOAD_BYTES {
        return Err(PackageFormatError::ManifestPayloadTooLong);
    }
    let name_end = header_end
        .checked_add(name_len)
        .ok_or(PackageFormatError::LengthOverflow)?;
    let payload_end = name_end
        .checked_add(payload_len)
        .ok_or(PackageFormatError::LengthOverflow)?;
    if payload_end > bytes.len() {
        return Err(PackageFormatError::BoundsExceeded);
    }

    let stable_name = &bytes[header_end..name_end];
    if stable_name.iter().any(|byte| !byte.is_ascii_graphic()) {
        return Err(PackageFormatError::InvalidManifest);
    }

    Ok((
        ManifestRecordV0 {
            record_type: read_u16(bytes, offset),
            stable_name,
            payload: &bytes[name_end..payload_end],
        },
        payload_end,
    ))
}

fn read_manifest_record<'a>(bytes: &'a [u8], offset: usize) -> (ManifestRecordV0<'a>, usize) {
    let header_end = offset + MANIFEST_RECORD_HEADER_LEN;
    let name_len = read_u16(bytes, offset + 4) as usize;
    let payload_len = read_u32(bytes, offset + 6) as usize;
    let name_end = header_end + name_len;
    let payload_end = name_end + payload_len;
    (
        ManifestRecordV0 {
            record_type: read_u16(bytes, offset),
            stable_name: &bytes[header_end..name_end],
            payload: &bytes[name_end..payload_end],
        },
        payload_end,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyOrder {
    Less,
    Equal,
    Greater,
}

fn compare_manifest_key(
    previous_type: u16,
    previous_name: &[u8],
    current_type: u16,
    current_name: &[u8],
) -> KeyOrder {
    if previous_type < current_type {
        return KeyOrder::Less;
    }
    if previous_type > current_type {
        return KeyOrder::Greater;
    }
    compare_bytes(previous_name, current_name)
}

fn compare_bytes(left: &[u8], right: &[u8]) -> KeyOrder {
    let mut index = 0;
    while index < left.len() && index < right.len() {
        if left[index] < right[index] {
            return KeyOrder::Less;
        }
        if left[index] > right[index] {
            return KeyOrder::Greater;
        }
        index += 1;
    }
    if left.len() < right.len() {
        KeyOrder::Less
    } else if left.len() > right.len() {
        KeyOrder::Greater
    } else {
        KeyOrder::Equal
    }
}

fn validate_content_table(table: &[u8], content_payload: &[u8]) -> Result<(), PackageFormatError> {
    if !table.len().is_multiple_of(CONTENT_ENTRY_V0_LEN) {
        return Err(PackageFormatError::BoundsExceeded);
    }
    let count = table.len() / CONTENT_ENTRY_V0_LEN;
    if count > MAX_CONTENT_ENTRIES {
        return Err(PackageFormatError::TooManyContentEntries);
    }

    let mut index = 0;
    while index < count {
        let entry = read_content_entry(table, index * CONTENT_ENTRY_V0_LEN);
        if entry.content_index as usize != index {
            return Err(PackageFormatError::InvalidManifest);
        }
        if entry.extent_count as usize > MAX_CONTENT_EXTENTS_PER_RECORD {
            return Err(PackageFormatError::TooManyContentExtents);
        }
        content_slice(content_payload, entry)?;
        index += 1;
    }
    Ok(())
}

fn read_content_entry(bytes: &[u8], offset: usize) -> ContentEntryV0 {
    ContentEntryV0 {
        content_index: read_u16(bytes, offset),
        role: read_u16(bytes, offset + 2),
        format: read_u16(bytes, offset + 4),
        extent_count: read_u16(bytes, offset + 6),
        offset: read_u64(bytes, offset + 8),
        length: read_u64(bytes, offset + 16),
        sha256: read_sha256(bytes, offset + 24),
        declared_runtime: read_u16(bytes, offset + 56),
        declared_entrypoint: read_u16(bytes, offset + 58),
    }
}

fn content_slice(
    content_payload: &[u8],
    entry: ContentEntryV0,
) -> Result<&[u8], PackageFormatError> {
    let start = usize::try_from(entry.offset).map_err(|_| PackageFormatError::BoundsExceeded)?;
    let len = usize::try_from(entry.length).map_err(|_| PackageFormatError::BoundsExceeded)?;
    let end = start
        .checked_add(len)
        .ok_or(PackageFormatError::LengthOverflow)?;
    if end > content_payload.len() {
        return Err(PackageFormatError::ContentRangeOutsidePayload);
    }
    let bytes = &content_payload[start..end];
    if sha256(bytes) != entry.sha256 {
        return Err(PackageFormatError::ContentDigestMismatch);
    }
    Ok(bytes)
}

fn artifact_digest(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(&bytes[..ARTIFACT_SHA256_OFFSET]);
    hasher.update(&[0u8; 32]);
    hasher.update(&bytes[ARTIFACT_SHA256_OFFSET + 32..]);
    hasher.finalize()
}

fn read_sha256(bytes: &[u8], offset: usize) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes[offset..offset + 32]);
    out
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256::sha256;

    const MANIFEST: &[u8; 12] = b"PYTHMAN0\0\0\0\0";

    fn write_u16(out: &mut [u8], offset: usize, value: u16) {
        out[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(out: &mut [u8], offset: usize, value: u32) {
        out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u64(out: &mut [u8], offset: usize, value: u64) {
        out[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    struct ArtifactFixture {
        bytes: [u8; 8192],
        len: usize,
        artifact_digest: [u8; 32],
    }

    impl ArtifactFixture {
        fn bytes(&self) -> &[u8] {
            &self.bytes[..self.len]
        }
    }

    fn minimal_artifact() -> ArtifactFixture {
        artifact_with_regions(MANIFEST, &[], &[])
    }

    fn artifact_with_regions(
        manifest: &[u8],
        content_table: &[u8],
        content: &[u8],
    ) -> ArtifactFixture {
        let manifest_offset = PACKAGE_ARTIFACT_HEADER_LEN;
        let content_table_offset = manifest_offset + manifest.len();
        let content_offset = content_table_offset + content_table.len();
        let len = content_offset + content.len();
        let mut bytes = [0u8; 8192];
        bytes[0..8].copy_from_slice(b"PYTHPKG0");
        write_u16(&mut bytes, 8, 0);
        write_u16(&mut bytes, 10, 1);
        write_u32(&mut bytes, 12, PACKAGE_ARTIFACT_HEADER_LEN as u32);
        write_u64(&mut bytes, 16, manifest_offset as u64);
        write_u64(&mut bytes, 24, manifest.len() as u64);
        write_u64(&mut bytes, 32, content_table_offset as u64);
        write_u64(&mut bytes, 40, content_table.len() as u64);
        write_u64(&mut bytes, 48, content_offset as u64);
        write_u64(&mut bytes, 56, content.len() as u64);
        bytes[MANIFEST_SHA256_OFFSET..MANIFEST_SHA256_OFFSET + 32]
            .copy_from_slice(&sha256(manifest));
        bytes[manifest_offset..content_table_offset].copy_from_slice(manifest);
        bytes[content_table_offset..content_offset].copy_from_slice(content_table);
        bytes[content_offset..len].copy_from_slice(content);

        let mut zeroed = bytes;
        zeroed[ARTIFACT_SHA256_OFFSET..ARTIFACT_SHA256_OFFSET + 32].fill(0);
        let artifact_digest = sha256(&zeroed[..len]);
        bytes[ARTIFACT_SHA256_OFFSET..ARTIFACT_SHA256_OFFSET + 32]
            .copy_from_slice(&artifact_digest);
        ArtifactFixture {
            bytes,
            len,
            artifact_digest,
        }
    }

    fn manifest_with_records(records: &[(u16, &[u8], &[u8])]) -> ([u8; 2048], usize) {
        let mut bytes = [0u8; 2048];
        bytes[..8].copy_from_slice(PACKAGE_MANIFEST_MAGIC);
        write_u32(&mut bytes, 8, records.len() as u32);
        let mut offset = PACKAGE_MANIFEST_HEADER_LEN;
        for (record_type, stable_name, payload) in records {
            write_u16(&mut bytes, offset, *record_type);
            write_u16(&mut bytes, offset + 2, 0);
            write_u16(&mut bytes, offset + 4, stable_name.len() as u16);
            write_u32(&mut bytes, offset + 6, payload.len() as u32);
            offset += MANIFEST_RECORD_HEADER_LEN;
            bytes[offset..offset + stable_name.len()].copy_from_slice(stable_name);
            offset += stable_name.len();
            bytes[offset..offset + payload.len()].copy_from_slice(payload);
            offset += payload.len();
        }
        (bytes, offset)
    }

    fn manifest_with_count(record_count: u32) -> ([u8; 16], usize) {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(PACKAGE_MANIFEST_MAGIC);
        write_u32(&mut bytes, 8, record_count);
        (bytes, PACKAGE_MANIFEST_HEADER_LEN)
    }

    fn content_table_with_entries(entries: &[(u16, u16, u64, u64, &[u8])]) -> ([u8; 8192], usize) {
        let mut table = [0u8; 8192];
        let mut offset = 0;
        for (content_index, extent_count, content_offset, content_length, content) in entries {
            write_u16(&mut table, offset, *content_index);
            write_u16(&mut table, offset + 2, 1);
            write_u16(&mut table, offset + 4, 1);
            write_u16(&mut table, offset + 6, *extent_count);
            write_u64(&mut table, offset + 8, *content_offset);
            write_u64(&mut table, offset + 16, *content_length);
            table[offset + 24..offset + 56].copy_from_slice(&sha256(content));
            write_u16(&mut table, offset + 56, 0);
            write_u16(&mut table, offset + 58, 0);
            write_u32(&mut table, offset + 60, 0);
            offset += CONTENT_ENTRY_V0_LEN;
        }
        (table, offset)
    }

    #[test]
    fn package_header_validates_zero_filled_artifact_digest_domain() {
        let fixture = minimal_artifact();

        let artifact = PackageArtifactV0::parse(fixture.bytes()).unwrap();

        assert_eq!(artifact.artifact_sha256(), fixture.artifact_digest);
        assert_eq!(artifact.manifest_sha256(), sha256(MANIFEST));
        assert_eq!(artifact.manifest().bytes(), MANIFEST);
    }

    #[test]
    fn package_format_parses_manifest_records_and_content_entries() {
        let (manifest, manifest_len) = manifest_with_records(&[(1, b"alpha", b"schema")]);
        let content = b"graph-package";
        let (table, table_len) =
            content_table_with_entries(&[(0, 1, 0, content.len() as u64, content)]);
        let fixture =
            artifact_with_regions(&manifest[..manifest_len], &table[..table_len], content);

        let artifact = PackageArtifactV0::parse(fixture.bytes()).unwrap();
        let record = artifact.manifest().record(0).unwrap();
        let entry = artifact.content_entry(0).unwrap();

        assert_eq!(record.record_type(), 1);
        assert_eq!(record.stable_name(), b"alpha");
        assert_eq!(record.payload(), b"schema");
        assert_eq!(entry.content_index, 0);
        assert_eq!(artifact.content_bytes(entry), Ok(content.as_slice()));
    }

    #[test]
    fn package_format_rejects_duplicate_stable_names() {
        let (manifest, manifest_len) =
            manifest_with_records(&[(1, b"alpha", b"a"), (1, b"alpha", b"b")]);
        let fixture = artifact_with_regions(&manifest[..manifest_len], &[], &[]);

        assert_eq!(
            PackageArtifactV0::parse(fixture.bytes()),
            Err(PackageFormatError::DuplicateStableName)
        );
    }

    #[test]
    fn package_format_rejects_unsorted_manifest_records() {
        let (manifest, manifest_len) =
            manifest_with_records(&[(2, b"beta", b"b"), (1, b"alpha", b"a")]);
        let fixture = artifact_with_regions(&manifest[..manifest_len], &[], &[]);

        assert_eq!(
            PackageArtifactV0::parse(fixture.bytes()),
            Err(PackageFormatError::UnsortedManifestRecord)
        );
    }

    #[test]
    fn package_format_rejects_manifest_bounds() {
        let oversized_name = [b'a'; crate::package_abi::MAX_STABLE_NAME_BYTES + 1];
        let oversized_payload = [0x55; crate::package_abi::MAX_MANIFEST_RECORD_PAYLOAD_BYTES + 1];

        let (manifest, manifest_len) = manifest_with_records(&[(1, &oversized_name, b"x")]);
        let fixture = artifact_with_regions(&manifest[..manifest_len], &[], &[]);
        assert_eq!(
            PackageArtifactV0::parse(fixture.bytes()),
            Err(PackageFormatError::StableNameTooLong)
        );

        let (manifest, manifest_len) = manifest_with_records(&[(1, b"alpha", &oversized_payload)]);
        let fixture = artifact_with_regions(&manifest[..manifest_len], &[], &[]);
        assert_eq!(
            PackageArtifactV0::parse(fixture.bytes()),
            Err(PackageFormatError::ManifestPayloadTooLong)
        );

        let (manifest, manifest_len) =
            manifest_with_count(crate::package_abi::MAX_MANIFEST_RECORDS as u32 + 1);
        let fixture = artifact_with_regions(&manifest[..manifest_len], &[], &[]);
        assert_eq!(
            PackageArtifactV0::parse(fixture.bytes()),
            Err(PackageFormatError::TooManyManifestRecords)
        );
    }

    #[test]
    fn package_format_rejects_content_table_bounds() {
        let (manifest, manifest_len) = manifest_with_records(&[]);
        let content = b"payload";

        let mut table = [0u8; (crate::package_abi::MAX_CONTENT_ENTRIES + 1) * CONTENT_ENTRY_V0_LEN];
        let mut offset = 0;
        while offset < table.len() {
            let index = offset / CONTENT_ENTRY_V0_LEN;
            write_u16(&mut table, offset, index as u16);
            write_u16(&mut table, offset + 2, 1);
            write_u16(&mut table, offset + 4, 1);
            write_u16(&mut table, offset + 6, 1);
            table[offset + 24..offset + 56].copy_from_slice(&sha256(&[]));
            offset += CONTENT_ENTRY_V0_LEN;
        }
        let fixture = artifact_with_regions(&manifest[..manifest_len], &table, &[]);
        assert_eq!(
            PackageArtifactV0::parse(fixture.bytes()),
            Err(PackageFormatError::TooManyContentEntries)
        );

        let (table, table_len) = content_table_with_entries(&[(
            0,
            crate::package_abi::MAX_CONTENT_EXTENTS_PER_RECORD as u16 + 1,
            0,
            content.len() as u64,
            content,
        )]);
        let fixture =
            artifact_with_regions(&manifest[..manifest_len], &table[..table_len], content);
        assert_eq!(
            PackageArtifactV0::parse(fixture.bytes()),
            Err(PackageFormatError::TooManyContentExtents)
        );

        let (table, table_len) =
            content_table_with_entries(&[(0, 1, 3, content.len() as u64, content)]);
        let fixture =
            artifact_with_regions(&manifest[..manifest_len], &table[..table_len], content);
        assert_eq!(
            PackageArtifactV0::parse(fixture.bytes()),
            Err(PackageFormatError::ContentRangeOutsidePayload)
        );
    }
}
