use pythos_shared::package_abi::{
    PackageStatus, MAX_SCHEMA_DECLARATIONS, OBJECT_KIND_PACKAGE, OBJECT_KIND_SCHEMA_DEFINITION,
};
use pythos_shared::sha256::sha256;

pub const PACKAGE_REGISTRY_MAGIC: &[u8; 8] = b"PYTHPKR0";
pub const PACKAGE_REGISTRY_MAJOR: u16 = 0;
pub const PACKAGE_REGISTRY_MINOR: u16 = 1;
pub const PACKAGE_REGISTRY_HEADER_LEN: usize = 88;
pub const PACKAGE_REGISTRY_CRC_LEN: usize = 4;
pub const PACKAGE_REGISTRY_PACKAGE_RECORD_LEN: usize = 64;
pub const PACKAGE_REGISTRY_SCHEMA_RECORD_LEN: usize = 96;
pub const PACKAGE_RECORD_FLAGS_OFFSET: usize = 50;
const SCHEMA_RECORD_FLAGS_OFFSET: usize = 26;
pub const REGISTRY_RECORD_FLAG_REQUIRES_MINOR_SUPPORT: u16 = 0x8000;
const MAX_REGISTRY_PACKAGES: usize = MAX_SCHEMA_DECLARATIONS;
const MAX_REGISTRY_SCHEMAS: usize = MAX_SCHEMA_DECLARATIONS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageRegistryGeneration {
    pub generation: u64,
    pub root_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageRegistryPackageRecord {
    pub package_object_id: u64,
    pub installed_revision: u64,
    pub release_digest: [u8; 32],
    pub status: u16,
    pub flags: u16,
    pub object_kind: u16,
}

impl PackageRegistryPackageRecord {
    pub const fn new(
        package_object_id: u64,
        installed_revision: u64,
        release_digest: [u8; 32],
        status: u16,
    ) -> Self {
        Self {
            package_object_id,
            installed_revision,
            release_digest,
            status,
            flags: 0,
            object_kind: OBJECT_KIND_PACKAGE,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageRegistrySchemaRecord {
    pub schema_object_id: u64,
    pub schema_revision: u64,
    pub package_object_id: u64,
    pub descriptor_content_index: u16,
    pub flags: u16,
    pub object_kind: u16,
    pub descriptor_digest: [u8; 32],
}

impl PackageRegistrySchemaRecord {
    pub const fn new(
        schema_object_id: u64,
        schema_revision: u64,
        package_object_id: u64,
        descriptor_content_index: u16,
        descriptor_digest: [u8; 32],
    ) -> Self {
        Self {
            schema_object_id,
            schema_revision,
            package_object_id,
            descriptor_content_index,
            flags: 0,
            object_kind: OBJECT_KIND_SCHEMA_DEFINITION,
            descriptor_digest,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageRegistry {
    generation: u64,
    active_transaction_id: u64,
    committed_root_digest: [u8; 32],
    root_digest: [u8; 32],
    package_records: [Option<PackageRegistryPackageRecord>; MAX_REGISTRY_PACKAGES],
    package_count: u32,
    schema_records: [Option<PackageRegistrySchemaRecord>; MAX_REGISTRY_SCHEMAS],
    schema_count: u32,
}

impl PackageRegistry {
    pub const fn empty() -> Self {
        Self {
            generation: 1,
            active_transaction_id: 0,
            committed_root_digest: [0; 32],
            root_digest: [0; 32],
            package_records: [None; MAX_REGISTRY_PACKAGES],
            package_count: 0,
            schema_records: [None; MAX_REGISTRY_SCHEMAS],
            schema_count: 0,
        }
    }

    pub fn add_package_record(
        &mut self,
        record: PackageRegistryPackageRecord,
    ) -> Result<(), PackageStatus> {
        if self.package_count as usize >= MAX_REGISTRY_PACKAGES {
            return Err(PackageStatus::QuotaDenied);
        }
        if self
            .package_records
            .iter()
            .flatten()
            .any(|existing| existing.package_object_id == record.package_object_id)
        {
            return Err(PackageStatus::DuplicateStableName);
        }
        self.package_records[self.package_count as usize] = Some(record);
        self.package_count += 1;
        Ok(())
    }

    pub fn add_schema_record(
        &mut self,
        record: PackageRegistrySchemaRecord,
    ) -> Result<(), PackageStatus> {
        if self.schema_count as usize >= MAX_REGISTRY_SCHEMAS {
            return Err(PackageStatus::QuotaDenied);
        }
        if self
            .schema_records
            .iter()
            .flatten()
            .any(|existing| existing.schema_object_id == record.schema_object_id)
        {
            return Err(PackageStatus::DuplicateStableName);
        }
        self.schema_records[self.schema_count as usize] = Some(record);
        self.schema_count += 1;
        Ok(())
    }

    pub fn decode_snapshot(bytes: &[u8]) -> Result<Self, PackageStatus> {
        if bytes.len() < PACKAGE_REGISTRY_HEADER_LEN + PACKAGE_REGISTRY_CRC_LEN {
            return Err(PackageStatus::BoundsExceeded);
        }
        if &bytes[0..8] != PACKAGE_REGISTRY_MAGIC {
            return Err(PackageStatus::InvalidMagic);
        }
        let stored_crc = read_u32(bytes, bytes.len() - PACKAGE_REGISTRY_CRC_LEN);
        if stored_crc != snapshot_crc32c(bytes) {
            return Err(PackageStatus::RegistryRecoveryDenied);
        }

        let major = read_u16(bytes, 8);
        let minor = read_u16(bytes, 10);
        if major != PACKAGE_REGISTRY_MAJOR {
            return Err(PackageStatus::UnsupportedMajor);
        }

        let package_count = read_u32(bytes, 60);
        let schema_count = read_u32(bytes, 64);
        let content_count = read_u32(bytes, 68);
        let export_count = read_u32(bytes, 72);
        let requirement_count = read_u32(bytes, 76);
        let locator_binding_count = read_u32(bytes, 80);
        let tombstone_count = read_u32(bytes, 84);
        if content_count != 0
            || export_count != 0
            || requirement_count != 0
            || locator_binding_count != 0
            || tombstone_count != 0
        {
            return Err(PackageStatus::RegistryRecoveryDenied);
        }
        if package_count as usize > MAX_REGISTRY_PACKAGES
            || schema_count as usize > MAX_REGISTRY_SCHEMAS
        {
            return Err(PackageStatus::BoundsExceeded);
        }

        let expected_len = encoded_len_for(package_count, schema_count)?;
        if expected_len != bytes.len() {
            return Err(PackageStatus::BoundsExceeded);
        }

        let mut registry = Self {
            generation: read_u64(bytes, 12),
            active_transaction_id: read_u64(bytes, 20),
            committed_root_digest: read_sha256(bytes, 28),
            root_digest: sha256(bytes),
            package_records: [None; MAX_REGISTRY_PACKAGES],
            package_count: 0,
            schema_records: [None; MAX_REGISTRY_SCHEMAS],
            schema_count: 0,
        };

        let mut offset = PACKAGE_REGISTRY_HEADER_LEN;
        for _ in 0..package_count {
            let record = decode_package_record(bytes, offset);
            if minor > PACKAGE_REGISTRY_MINOR
                && (record.flags & REGISTRY_RECORD_FLAG_REQUIRES_MINOR_SUPPORT) != 0
            {
                return Err(PackageStatus::UnsupportedRequiredMinor);
            }
            registry.add_package_record(record)?;
            offset += PACKAGE_REGISTRY_PACKAGE_RECORD_LEN;
        }
        for _ in 0..schema_count {
            let record = decode_schema_record(bytes, offset);
            if minor > PACKAGE_REGISTRY_MINOR
                && (record.flags & REGISTRY_RECORD_FLAG_REQUIRES_MINOR_SUPPORT) != 0
            {
                return Err(PackageStatus::UnsupportedRequiredMinor);
            }
            registry.add_schema_record(record)?;
            offset += PACKAGE_REGISTRY_SCHEMA_RECORD_LEN;
        }

        Ok(registry)
    }

    pub fn select_generation(slot_a: &[u8], slot_b: &[u8]) -> Result<Self, PackageStatus> {
        let candidate_a = Self::decode_snapshot(slot_a).ok();
        let candidate_b = Self::decode_snapshot(slot_b).ok();

        match (candidate_a, candidate_b) {
            (Some(a), Some(b)) => {
                if a.generation >= b.generation {
                    Ok(a)
                } else {
                    Ok(b)
                }
            }
            (Some(a), None) => Ok(a),
            (None, Some(b)) => Ok(b),
            (None, None) => Err(PackageStatus::RegistryRecoveryDenied),
        }
    }

    pub fn encode_snapshot(
        &self,
        out: &mut [u8],
    ) -> Result<PackageRegistryGeneration, PackageStatus> {
        let encoded_len = self.encoded_len();
        if out.len() < encoded_len {
            return Err(PackageStatus::BufferTooSmall);
        }
        out[..encoded_len].fill(0);
        out[0..8].copy_from_slice(PACKAGE_REGISTRY_MAGIC);
        write_u16(out, 8, PACKAGE_REGISTRY_MAJOR);
        write_u16(out, 10, PACKAGE_REGISTRY_MINOR);
        write_u64(out, 12, self.generation);
        write_u64(out, 20, self.active_transaction_id);
        out[28..60].copy_from_slice(&self.committed_root_digest);
        write_u32(out, 60, self.package_count);
        write_u32(out, 64, self.schema_count);

        let mut packages = self.package_records;
        sort_package_records(&mut packages, self.package_count as usize);
        let mut schemas = self.schema_records;
        sort_schema_records(&mut schemas, self.schema_count as usize);

        let mut offset = PACKAGE_REGISTRY_HEADER_LEN;
        for slot in packages.iter().take(self.package_count as usize) {
            encode_package_record(slot.unwrap(), out, offset);
            offset += PACKAGE_REGISTRY_PACKAGE_RECORD_LEN;
        }
        for slot in schemas.iter().take(self.schema_count as usize) {
            encode_schema_record(slot.unwrap(), out, offset);
            offset += PACKAGE_REGISTRY_SCHEMA_RECORD_LEN;
        }

        let crc = crc32c_castagnoli(&out[..encoded_len]);
        write_u32(out, encoded_len - PACKAGE_REGISTRY_CRC_LEN, crc);
        Ok(PackageRegistryGeneration {
            generation: self.generation,
            root_digest: sha256(&out[..encoded_len]),
        })
    }

    pub fn encoded_len(&self) -> usize {
        PACKAGE_REGISTRY_HEADER_LEN
            + self.package_count as usize * PACKAGE_REGISTRY_PACKAGE_RECORD_LEN
            + self.schema_count as usize * PACKAGE_REGISTRY_SCHEMA_RECORD_LEN
            + PACKAGE_REGISTRY_CRC_LEN
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub const fn root_digest(&self) -> [u8; 32] {
        self.root_digest
    }

    pub const fn package_count(&self) -> usize {
        self.package_count as usize
    }

    pub const fn schema_count(&self) -> usize {
        self.schema_count as usize
    }

    pub fn package_record(&self, index: usize) -> Option<PackageRegistryPackageRecord> {
        if index >= self.package_count as usize {
            return None;
        }
        self.package_records[index]
    }

    pub fn schema_record(&self, index: usize) -> Option<PackageRegistrySchemaRecord> {
        if index >= self.schema_count as usize {
            return None;
        }
        self.schema_records[index]
    }
}

fn encoded_len_for(package_count: u32, schema_count: u32) -> Result<usize, PackageStatus> {
    let package_bytes = (package_count as usize)
        .checked_mul(PACKAGE_REGISTRY_PACKAGE_RECORD_LEN)
        .ok_or(PackageStatus::LengthOverflow)?;
    let schema_bytes = (schema_count as usize)
        .checked_mul(PACKAGE_REGISTRY_SCHEMA_RECORD_LEN)
        .ok_or(PackageStatus::LengthOverflow)?;
    PACKAGE_REGISTRY_HEADER_LEN
        .checked_add(package_bytes)
        .and_then(|value| value.checked_add(schema_bytes))
        .and_then(|value| value.checked_add(PACKAGE_REGISTRY_CRC_LEN))
        .ok_or(PackageStatus::LengthOverflow)
}

fn encode_package_record(
    record: PackageRegistryPackageRecord,
    out: &mut [u8],
    offset: usize,
) {
    write_u64(out, offset, record.package_object_id);
    write_u64(out, offset + 8, record.installed_revision);
    out[offset + 16..offset + 48].copy_from_slice(&record.release_digest);
    write_u16(out, offset + 48, record.status);
    write_u16(out, offset + PACKAGE_RECORD_FLAGS_OFFSET, record.flags);
    write_u16(out, offset + 52, record.object_kind);
}

fn decode_package_record(bytes: &[u8], offset: usize) -> PackageRegistryPackageRecord {
    PackageRegistryPackageRecord {
        package_object_id: read_u64(bytes, offset),
        installed_revision: read_u64(bytes, offset + 8),
        release_digest: read_sha256(bytes, offset + 16),
        status: read_u16(bytes, offset + 48),
        flags: read_u16(bytes, offset + PACKAGE_RECORD_FLAGS_OFFSET),
        object_kind: read_u16(bytes, offset + 52),
    }
}

fn encode_schema_record(record: PackageRegistrySchemaRecord, out: &mut [u8], offset: usize) {
    write_u64(out, offset, record.schema_object_id);
    write_u64(out, offset + 8, record.schema_revision);
    write_u64(out, offset + 16, record.package_object_id);
    write_u16(out, offset + 24, record.descriptor_content_index);
    write_u16(out, offset + SCHEMA_RECORD_FLAGS_OFFSET, record.flags);
    write_u16(out, offset + 28, record.object_kind);
    out[offset + 32..offset + 64].copy_from_slice(&record.descriptor_digest);
}

fn decode_schema_record(bytes: &[u8], offset: usize) -> PackageRegistrySchemaRecord {
    PackageRegistrySchemaRecord {
        schema_object_id: read_u64(bytes, offset),
        schema_revision: read_u64(bytes, offset + 8),
        package_object_id: read_u64(bytes, offset + 16),
        descriptor_content_index: read_u16(bytes, offset + 24),
        flags: read_u16(bytes, offset + SCHEMA_RECORD_FLAGS_OFFSET),
        object_kind: read_u16(bytes, offset + 28),
        descriptor_digest: read_sha256(bytes, offset + 32),
    }
}

fn sort_package_records(
    records: &mut [Option<PackageRegistryPackageRecord>; MAX_REGISTRY_PACKAGES],
    count: usize,
) {
    let mut i = 0;
    while i < count {
        let mut min = i;
        let mut j = i + 1;
        while j < count {
            if records[j].unwrap().package_object_id < records[min].unwrap().package_object_id {
                min = j;
            }
            j += 1;
        }
        records.swap(i, min);
        i += 1;
    }
}

fn sort_schema_records(
    records: &mut [Option<PackageRegistrySchemaRecord>; MAX_REGISTRY_SCHEMAS],
    count: usize,
) {
    let mut i = 0;
    while i < count {
        let mut min = i;
        let mut j = i + 1;
        while j < count {
            let left = records[j].unwrap();
            let right = records[min].unwrap();
            if (left.schema_object_id, left.schema_revision)
                < (right.schema_object_id, right.schema_revision)
            {
                min = j;
            }
            j += 1;
        }
        records.swap(i, min);
        i += 1;
    }
}

fn snapshot_crc32c(bytes: &[u8]) -> u32 {
    let crc_offset = bytes.len() - PACKAGE_REGISTRY_CRC_LEN;
    let mut crc = !0u32;
    crc = crc32c_update(crc, &bytes[..crc_offset]);
    crc = crc32c_update(crc, &[0, 0, 0, 0]);
    !crc
}

pub(crate) fn crc32c_castagnoli(bytes: &[u8]) -> u32 {
    !crc32c_update(!0u32, bytes)
}

fn crc32c_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        let mut bit = 0;
        while bit < 8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0x82F6_3B78 & mask);
            bit += 1;
        }
    }
    crc
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

fn read_sha256(bytes: &[u8], offset: usize) -> [u8; 32] {
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&bytes[offset..offset + 32]);
    digest
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

#[cfg(test)]
mod tests {
    use super::{
        crc32c_castagnoli, PackageRegistry, PackageRegistryPackageRecord,
        PackageRegistrySchemaRecord, PACKAGE_RECORD_FLAGS_OFFSET, PACKAGE_REGISTRY_HEADER_LEN,
        REGISTRY_RECORD_FLAG_REQUIRES_MINOR_SUPPORT,
    };
    use pythos_shared::package_abi::PackageStatus;

    #[test]
    fn package_registry_sorts_package_and_schema_records_canonically() {
        let mut registry = PackageRegistry::empty();

        registry
            .add_package_record(PackageRegistryPackageRecord::new(30, 1, digest(3), 1))
            .unwrap();
        registry
            .add_package_record(PackageRegistryPackageRecord::new(10, 1, digest(1), 1))
            .unwrap();
        registry
            .add_schema_record(PackageRegistrySchemaRecord::new(90, 1, 30, 0, digest(9)))
            .unwrap();
        registry
            .add_schema_record(PackageRegistrySchemaRecord::new(80, 1, 10, 0, digest(8)))
            .unwrap();

        let restored = round_trip(&registry);

        assert_eq!(restored.package_record(0).unwrap().package_object_id, 10);
        assert_eq!(restored.package_record(1).unwrap().package_object_id, 30);
        assert_eq!(restored.schema_record(0).unwrap().schema_object_id, 80);
        assert_eq!(restored.schema_record(1).unwrap().schema_object_id, 90);
    }

    #[test]
    fn package_registry_snapshot_crc32c_uses_zero_filled_crc_domain() {
        let mut registry = PackageRegistry::empty();
        registry
            .add_package_record(PackageRegistryPackageRecord::new(1, 7, digest(1), 1))
            .unwrap();
        let mut encoded = [0u8; 512];

        registry.encode_snapshot(&mut encoded).unwrap();
        let used = registry.encoded_len();
        let crc_offset = used - 4;
        let stored_crc = u32::from_le_bytes([
            encoded[crc_offset],
            encoded[crc_offset + 1],
            encoded[crc_offset + 2],
            encoded[crc_offset + 3],
        ]);
        encoded[crc_offset..crc_offset + 4].fill(0);

        assert_eq!(stored_crc, crc32c_castagnoli(&encoded[..used]));
    }

    #[test]
    fn package_registry_rejects_unknown_major_version() {
        let registry = registry_with_one_package_and_schema();
        let mut encoded = [0u8; 512];
        registry.encode_snapshot(&mut encoded).unwrap();
        let used = registry.encoded_len();

        encoded[8..10].copy_from_slice(&1u16.to_le_bytes());
        refresh_crc(&mut encoded[..used]);

        assert_eq!(
            PackageRegistry::decode_snapshot(&encoded[..used]),
            Err(PackageStatus::UnsupportedMajor)
        );
    }

    #[test]
    fn package_registry_rejects_unsupported_required_minor_record_flag() {
        let registry = registry_with_one_package_and_schema();
        let mut encoded = [0u8; 512];
        registry.encode_snapshot(&mut encoded).unwrap();
        let used = registry.encoded_len();

        encoded[10..12].copy_from_slice(&2u16.to_le_bytes());
        let flags_offset = PACKAGE_REGISTRY_HEADER_LEN + PACKAGE_RECORD_FLAGS_OFFSET;
        encoded[flags_offset..flags_offset + 2]
            .copy_from_slice(&REGISTRY_RECORD_FLAG_REQUIRES_MINOR_SUPPORT.to_le_bytes());
        refresh_crc(&mut encoded[..used]);

        assert_eq!(
            PackageRegistry::decode_snapshot(&encoded[..used]),
            Err(PackageStatus::UnsupportedRequiredMinor)
        );
    }

    #[test]
    fn package_registry_round_trips_one_package_and_one_schema_record() {
        let registry = registry_with_one_package_and_schema();
        let mut encoded = [0u8; 512];

        let generation = registry.encode_snapshot(&mut encoded).unwrap();
        let used = registry.encoded_len();
        let restored = PackageRegistry::decode_snapshot(&encoded[..used]).unwrap();

        assert_eq!(generation.generation, 1);
        assert_eq!(generation.root_digest, restored.root_digest());
        assert_eq!(restored.package_count(), 1);
        assert_eq!(restored.schema_count(), 1);
        assert_eq!(
            restored.package_record(0).unwrap(),
            PackageRegistryPackageRecord::new(42, 7, digest(4), 1)
        );
        assert_eq!(
            restored.schema_record(0).unwrap(),
            PackageRegistrySchemaRecord::new(77, 3, 42, 0, digest(7))
        );
    }

    #[test]
    fn package_registry_recovery_selects_highest_valid_generation() {
        let (older, older_len) = encoded_registry_generation(1);
        let (newer, newer_len) = encoded_registry_generation(3);

        let selected =
            PackageRegistry::select_generation(&older[..older_len], &newer[..newer_len]).unwrap();

        assert_eq!(selected.generation(), 3);
    }

    #[test]
    fn package_registry_recovery_ignores_corrupt_crc_generation() {
        let (older, older_len) = encoded_registry_generation(1);
        let (mut corrupt_newer, newer_len) = encoded_registry_generation(4);
        corrupt_newer[newer_len - 1] ^= 0x55;

        let selected =
            PackageRegistry::select_generation(&older[..older_len], &corrupt_newer[..newer_len])
                .unwrap();

        assert_eq!(selected.generation(), 1);
    }

    #[test]
    fn package_registry_recovery_ignores_unsupported_major_generation() {
        let (older, older_len) = encoded_registry_generation(2);
        let (mut unsupported_newer, newer_len) = encoded_registry_generation(5);
        unsupported_newer[8..10].copy_from_slice(&1u16.to_le_bytes());
        refresh_crc(&mut unsupported_newer[..newer_len]);

        let selected =
            PackageRegistry::select_generation(&unsupported_newer[..newer_len], &older[..older_len])
                .unwrap();

        assert_eq!(selected.generation(), 2);
    }

    #[test]
    fn package_registry_recovery_denies_when_no_valid_package_generation_exists() {
        let (mut corrupt, corrupt_len) = encoded_registry_generation(1);
        corrupt[corrupt_len - 1] ^= 0xAA;
        let (mut unsupported, unsupported_len) = encoded_registry_generation(2);
        unsupported[8..10].copy_from_slice(&1u16.to_le_bytes());
        refresh_crc(&mut unsupported[..unsupported_len]);

        assert_eq!(
            PackageRegistry::select_generation(&corrupt[..corrupt_len], &unsupported[..unsupported_len]),
            Err(PackageStatus::RegistryRecoveryDenied)
        );
    }

    fn registry_with_one_package_and_schema() -> PackageRegistry {
        let mut registry = PackageRegistry::empty();
        registry
            .add_package_record(PackageRegistryPackageRecord::new(42, 7, digest(4), 1))
            .unwrap();
        registry
            .add_schema_record(PackageRegistrySchemaRecord::new(77, 3, 42, 0, digest(7)))
            .unwrap();
        registry
    }

    fn round_trip(registry: &PackageRegistry) -> PackageRegistry {
        let mut encoded = [0u8; 512];
        registry.encode_snapshot(&mut encoded).unwrap();
        PackageRegistry::decode_snapshot(&encoded[..registry.encoded_len()]).unwrap()
    }

    fn encoded_registry_generation(generation: u64) -> ([u8; 512], usize) {
        let registry = registry_with_one_package_and_schema();
        let mut encoded = [0u8; 512];
        registry.encode_snapshot(&mut encoded).unwrap();
        let used = registry.encoded_len();
        encoded[12..20].copy_from_slice(&generation.to_le_bytes());
        refresh_crc(&mut encoded[..used]);
        (encoded, used)
    }

    fn refresh_crc(bytes: &mut [u8]) {
        let crc_offset = bytes.len() - 4;
        bytes[crc_offset..crc_offset + 4].fill(0);
        let crc = crc32c_castagnoli(bytes);
        bytes[crc_offset..crc_offset + 4].copy_from_slice(&crc.to_le_bytes());
    }

    fn digest(seed: u8) -> [u8; 32] {
        [seed; 32]
    }
}
