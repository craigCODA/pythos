use crate::{
    object_locator::validate_locator, object_service_checkpoint::ObjectCheckpointIdentity,
    package_content_store::PackageExtent, shell_objects::ObjectId,
};
use pythos_shared::package_abi::{
    MAX_CONTENT_BYTES, MAX_CONTENT_ENTRIES, MAX_CONTENT_EXTENTS_PER_RECORD, MAX_EXPORT_RECORDS,
    MAX_LOCATOR_SEGMENT_BYTES, MAX_SCHEMA_DECLARATIONS, OBJECT_KIND_PACKAGE,
    OBJECT_KIND_SCHEMA_DEFINITION, PackageStatus,
};
use pythos_shared::sha256::sha256;

pub const PACKAGE_REGISTRY_MAGIC: &[u8; 8] = b"PYTHPKR0";
pub const PACKAGE_REGISTRY_MAJOR: u16 = 0;
pub const PACKAGE_REGISTRY_MINOR: u16 = 1;
pub const PACKAGE_REGISTRY_HEADER_LEN: usize = 88;
pub const PACKAGE_REGISTRY_CRC_LEN: usize = 4;
pub const PACKAGE_REGISTRY_PACKAGE_RECORD_LEN: usize = 64;
pub const PACKAGE_REGISTRY_SCHEMA_RECORD_LEN: usize = 96;
pub const PACKAGE_REGISTRY_CONTENT_RECORD_LEN: usize = 256;
pub const PACKAGE_REGISTRY_EXPORT_RECORD_LEN: usize = 160;
pub const PACKAGE_RECORD_FLAGS_OFFSET: usize = 50;
const SCHEMA_RECORD_FLAGS_OFFSET: usize = 26;
const EXPORT_RECORD_FLAGS_OFFSET: usize = 10;
pub const REGISTRY_RECORD_FLAG_REQUIRES_MINOR_SUPPORT: u16 = 0x8000;
pub const PACKAGE_TRANSACTION_COMMIT_V0_LEN: usize = 128;
pub const PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET: usize = 112;
const MAX_REGISTRY_PACKAGES: usize = MAX_SCHEMA_DECLARATIONS;
const MAX_REGISTRY_SCHEMAS: usize = MAX_SCHEMA_DECLARATIONS;
const MAX_REGISTRY_CONTENT: usize = MAX_CONTENT_ENTRIES;
const MAX_REGISTRY_EXPORTS: usize = MAX_EXPORT_RECORDS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageRegistryGeneration {
    pub generation: u64,
    pub root_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageTransactionCommitV0 {
    pub transaction_id: u64,
    pub operation: u16,
    pub package_registry_generation: u64,
    pub package_registry_root_digest: [u8; 32],
    pub object_checkpoint_generation: u64,
    pub object_checkpoint_root_digest: [u8; 32],
    pub package_object_id: u64,
    pub package_installed_revision: u64,
    pub commit_crc32c: u32,
}

impl PackageTransactionCommitV0 {
    pub fn new(
        transaction_id: u64,
        operation: u16,
        registry: PackageRegistryGeneration,
        object: ObjectCheckpointIdentity,
        package_object_id: u64,
        package_installed_revision: u64,
    ) -> Self {
        let mut anchor = Self {
            transaction_id,
            operation,
            package_registry_generation: registry.generation,
            package_registry_root_digest: registry.root_digest,
            object_checkpoint_generation: object.generation,
            object_checkpoint_root_digest: object.root_digest,
            package_object_id,
            package_installed_revision,
            commit_crc32c: 0,
        };
        anchor.commit_crc32c = anchor.computed_crc32c();
        anchor
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<(), PackageStatus> {
        if out.len() < PACKAGE_TRANSACTION_COMMIT_V0_LEN {
            return Err(PackageStatus::BufferTooSmall);
        }
        self.encode_without_crc(out);
        write_u32(
            out,
            PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET,
            self.computed_crc32c(),
        );
        Ok(())
    }

    pub fn decode(
        bytes: &[u8],
        expected_registry: PackageRegistryGeneration,
        expected_object: ObjectCheckpointIdentity,
    ) -> Result<Self, PackageStatus> {
        if bytes.len() != PACKAGE_TRANSACTION_COMMIT_V0_LEN {
            return Err(PackageStatus::BoundsExceeded);
        }
        if bytes[10..16].iter().any(|byte| *byte != 0)
            || bytes[116..128].iter().any(|byte| *byte != 0)
        {
            return Err(PackageStatus::BadRequest);
        }

        let stored_crc = read_u32(bytes, PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET);
        if stored_crc != transaction_anchor_crc32c(bytes) {
            return Err(PackageStatus::RegistryRecoveryDenied);
        }

        let anchor = Self {
            transaction_id: read_u64(bytes, 0),
            operation: read_u16(bytes, 8),
            package_registry_generation: read_u64(bytes, 16),
            package_registry_root_digest: read_sha256(bytes, 24),
            object_checkpoint_generation: read_u64(bytes, 56),
            object_checkpoint_root_digest: read_sha256(bytes, 64),
            package_object_id: read_u64(bytes, 96),
            package_installed_revision: read_u64(bytes, 104),
            commit_crc32c: stored_crc,
        };

        if !anchor.matches_pair(expected_registry, expected_object) {
            return Err(PackageStatus::TransactionAnchorMismatch);
        }

        Ok(anchor)
    }

    pub fn matches_pair(
        &self,
        expected_registry: PackageRegistryGeneration,
        expected_object: ObjectCheckpointIdentity,
    ) -> bool {
        self.package_registry_generation == expected_registry.generation
            && self.package_registry_root_digest == expected_registry.root_digest
            && self.object_checkpoint_generation == expected_object.generation
            && self.object_checkpoint_root_digest == expected_object.root_digest
    }

    pub fn decode_stored(bytes: &[u8]) -> Result<Self, PackageStatus> {
        if bytes.len() != PACKAGE_TRANSACTION_COMMIT_V0_LEN {
            return Err(PackageStatus::BoundsExceeded);
        }
        if bytes[10..16].iter().any(|byte| *byte != 0)
            || bytes[116..128].iter().any(|byte| *byte != 0)
        {
            return Err(PackageStatus::BadRequest);
        }
        let stored_crc = read_u32(bytes, PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET);
        if stored_crc != transaction_anchor_crc32c(bytes) {
            return Err(PackageStatus::RegistryRecoveryDenied);
        }
        Ok(Self {
            transaction_id: read_u64(bytes, 0),
            operation: read_u16(bytes, 8),
            package_registry_generation: read_u64(bytes, 16),
            package_registry_root_digest: read_sha256(bytes, 24),
            object_checkpoint_generation: read_u64(bytes, 56),
            object_checkpoint_root_digest: read_sha256(bytes, 64),
            package_object_id: read_u64(bytes, 96),
            package_installed_revision: read_u64(bytes, 104),
            commit_crc32c: stored_crc,
        })
    }

    fn computed_crc32c(&self) -> u32 {
        let mut bytes = [0u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN];
        self.encode_without_crc(&mut bytes);
        crc32c_castagnoli(&bytes)
    }

    fn encode_without_crc(&self, out: &mut [u8]) {
        out[..PACKAGE_TRANSACTION_COMMIT_V0_LEN].fill(0);
        write_u64(out, 0, self.transaction_id);
        write_u16(out, 8, self.operation);
        write_u64(out, 16, self.package_registry_generation);
        out[24..56].copy_from_slice(&self.package_registry_root_digest);
        write_u64(out, 56, self.object_checkpoint_generation);
        out[64..96].copy_from_slice(&self.object_checkpoint_root_digest);
        write_u64(out, 96, self.package_object_id);
        write_u64(out, 104, self.package_installed_revision);
    }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageRegistryContentRecord {
    pub package_object_id: u64,
    pub release_digest: [u8; 32],
    pub content_index: u16,
    pub role: u16,
    pub format: u16,
    pub digest: [u8; 32],
    pub byte_len: u64,
    pub extents: [PackageExtent; MAX_CONTENT_EXTENTS_PER_RECORD],
    pub extent_count: u16,
    pub retention_count: u16,
    pub flags: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageRegistryExportRecord {
    pub namespace_root_object_id: u64,
    package_locator: [u8; MAX_LOCATOR_SEGMENT_BYTES],
    package_locator_len: u8,
    export_name: [u8; MAX_LOCATOR_SEGMENT_BYTES],
    export_name_len: u8,
    pub package_object_id: u64,
    pub package_revision: u64,
    pub release_digest: [u8; 32],
    pub export_kind: u16,
    pub content_index: u16,
    pub entrypoint: u16,
    pub schema_object_id: u64,
    pub schema_revision: u64,
    pub schema_descriptor_digest: [u8; 32],
}

impl PackageRegistryExportRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        namespace_root_object_id: u64,
        package_locator: &[u8],
        export_name: &[u8],
        package_object_id: u64,
        package_revision: u64,
        release_digest: [u8; 32],
        export_kind: u16,
        content_index: u16,
        entrypoint: u16,
        schema_object_id: u64,
        schema_revision: u64,
        schema_descriptor_digest: [u8; 32],
    ) -> Result<Self, PackageStatus> {
        let (package_locator, package_locator_len) = copy_locator_segment(package_locator)?;
        let (export_name, export_name_len) = copy_locator_segment(export_name)?;
        Ok(Self {
            namespace_root_object_id,
            package_locator,
            package_locator_len,
            export_name,
            export_name_len,
            package_object_id,
            package_revision,
            release_digest,
            export_kind,
            content_index,
            entrypoint,
            schema_object_id,
            schema_revision,
            schema_descriptor_digest,
        })
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
    content_records: [Option<PackageRegistryContentRecord>; MAX_REGISTRY_CONTENT],
    content_count: u32,
    export_records: [Option<PackageRegistryExportRecord>; MAX_REGISTRY_EXPORTS],
    export_count: u32,
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
            content_records: [None; MAX_REGISTRY_CONTENT],
            content_count: 0,
            export_records: [None; MAX_REGISTRY_EXPORTS],
            export_count: 0,
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

    pub fn add_content_record(
        &mut self,
        record: PackageRegistryContentRecord,
    ) -> Result<(), PackageStatus> {
        validate_content_record(record)?;
        if self.content_count as usize >= MAX_REGISTRY_CONTENT {
            return Err(PackageStatus::QuotaDenied);
        }
        if self.content_records.iter().flatten().any(|existing| {
            existing.package_object_id == record.package_object_id
                && existing.release_digest == record.release_digest
                && existing.content_index == record.content_index
        }) {
            return Err(PackageStatus::DuplicateStableName);
        }
        self.content_records[self.content_count as usize] = Some(record);
        self.content_count += 1;
        Ok(())
    }

    pub fn add_export_record(
        &mut self,
        record: PackageRegistryExportRecord,
    ) -> Result<(), PackageStatus> {
        self.active_package_for_export(record)?;
        self.insert_export_record(record)
    }

    fn add_export_record_from_snapshot(
        &mut self,
        record: PackageRegistryExportRecord,
    ) -> Result<(), PackageStatus> {
        self.retainable_package_record_for_export(record)?;
        self.insert_export_record(record)
    }

    fn insert_export_record(
        &mut self,
        record: PackageRegistryExportRecord,
    ) -> Result<(), PackageStatus> {
        if self.export_count as usize >= MAX_REGISTRY_EXPORTS {
            return Err(PackageStatus::QuotaDenied);
        }
        if self.export_records.iter().flatten().any(|existing| {
            existing.namespace_root_object_id == record.namespace_root_object_id
                && locator_segment_eq(
                    existing.package_locator,
                    existing.package_locator_len,
                    record.package_locator,
                    record.package_locator_len,
                )
                && locator_segment_eq(
                    existing.export_name,
                    existing.export_name_len,
                    record.export_name,
                    record.export_name_len,
                )
        }) {
            return Err(PackageStatus::DuplicateStableName);
        }
        self.export_records[self.export_count as usize] = Some(record);
        self.export_count += 1;
        Ok(())
    }

    pub fn disable(
        &mut self,
        package_object_id: ObjectId,
    ) -> Result<PackageRegistryPackageRecord, PackageStatus> {
        let mut index = 0usize;
        while index < self.package_count as usize {
            if let Some(mut record) = self.package_records[index]
                && record.package_object_id == package_object_id.raw()
            {
                return match record.status {
                    status
                        if status == PackageStatus::Ok as u16
                            || status == PackageStatus::PackageDisabled as u16 =>
                    {
                        record.status = PackageStatus::PackageDisabled as u16;
                        self.package_records[index] = Some(record);
                        Ok(record)
                    }
                    status if status == PackageStatus::PackageTombstoned as u16 => {
                        Err(PackageStatus::PackageTombstoned)
                    }
                    _ => Err(PackageStatus::Denied),
                };
            }
            index += 1;
        }
        Err(PackageStatus::NotFound)
    }

    pub fn tombstone(
        &mut self,
        package_object_id: ObjectId,
    ) -> Result<PackageRegistryPackageRecord, PackageStatus> {
        let mut index = 0usize;
        while index < self.package_count as usize {
            if let Some(mut record) = self.package_records[index]
                && record.package_object_id == package_object_id.raw()
            {
                return match record.status {
                    status
                        if status == PackageStatus::Ok as u16
                            || status == PackageStatus::PackageDisabled as u16 =>
                    {
                        record.status = PackageStatus::PackageTombstoned as u16;
                        self.package_records[index] = Some(record);
                        self.remove_exports_for_package(record.package_object_id);
                        Ok(record)
                    }
                    status if status == PackageStatus::PackageTombstoned as u16 => {
                        Err(PackageStatus::PackageTombstoned)
                    }
                    _ => Err(PackageStatus::Denied),
                };
            }
            index += 1;
        }
        Err(PackageStatus::NotFound)
    }

    pub fn retain_schema_reference(
        &mut self,
        schema_object_id: ObjectId,
        schema_revision: u64,
    ) -> Result<(), PackageStatus> {
        self.adjust_schema_descriptor_retention(schema_object_id, schema_revision, 1)
    }

    pub fn release_schema_reference(
        &mut self,
        schema_object_id: ObjectId,
        schema_revision: u64,
    ) -> Result<(), PackageStatus> {
        self.adjust_schema_descriptor_retention(schema_object_id, schema_revision, -1)
    }

    pub fn reclaim_tombstoned_package_content(
        &mut self,
        package_object_id: ObjectId,
    ) -> Result<(), PackageStatus> {
        let package = self
            .package_record_by_object_id(package_object_id.raw())
            .ok_or(PackageStatus::NotFound)?;
        if package.status != PackageStatus::PackageTombstoned as u16 {
            return Err(PackageStatus::BadRequest);
        }

        let mut read_index = 0usize;
        let mut write_index = 0usize;
        while read_index < self.content_count as usize {
            if let Some(record) = self.content_records[read_index]
                && (record.package_object_id != package_object_id.raw()
                    || self.retained_schema_descriptor_content(record))
            {
                self.content_records[write_index] = Some(record);
                write_index += 1;
            }
            read_index += 1;
        }
        let retained_count = write_index as u32;
        while write_index < self.content_count as usize {
            self.content_records[write_index] = None;
            write_index += 1;
        }
        self.content_count = retained_count;
        Ok(())
    }

    pub fn decode_snapshot(bytes: &[u8]) -> Result<Self, PackageStatus> {
        let mut registry = Self::empty();
        Self::decode_snapshot_into(bytes, &mut registry)?;
        Ok(registry)
    }

    pub fn decode_snapshot_into(bytes: &[u8], registry: &mut Self) -> Result<(), PackageStatus> {
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
        if requirement_count != 0 || locator_binding_count != 0 || tombstone_count != 0 {
            return Err(PackageStatus::RegistryRecoveryDenied);
        }
        if package_count as usize > MAX_REGISTRY_PACKAGES
            || schema_count as usize > MAX_REGISTRY_SCHEMAS
            || content_count as usize > MAX_REGISTRY_CONTENT
            || export_count as usize > MAX_REGISTRY_EXPORTS
        {
            return Err(PackageStatus::BoundsExceeded);
        }

        let expected_len =
            encoded_len_for(package_count, schema_count, content_count, export_count)?;
        if expected_len != bytes.len() {
            return Err(PackageStatus::BoundsExceeded);
        }

        registry.clear_to_empty();
        registry.generation = read_u64(bytes, 12);
        registry.active_transaction_id = read_u64(bytes, 20);
        registry.committed_root_digest = read_sha256(bytes, 28);
        registry.root_digest = sha256(bytes);

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
        for _ in 0..content_count {
            let record = decode_content_record(bytes, offset)?;
            if minor > PACKAGE_REGISTRY_MINOR
                && (record.flags & REGISTRY_RECORD_FLAG_REQUIRES_MINOR_SUPPORT) != 0
            {
                return Err(PackageStatus::UnsupportedRequiredMinor);
            }
            registry.add_content_record(record)?;
            offset += PACKAGE_REGISTRY_CONTENT_RECORD_LEN;
        }
        for _ in 0..export_count {
            let (record, flags) = decode_export_record(bytes, offset)?;
            if minor > PACKAGE_REGISTRY_MINOR
                && (flags & REGISTRY_RECORD_FLAG_REQUIRES_MINOR_SUPPORT) != 0
            {
                return Err(PackageStatus::UnsupportedRequiredMinor);
            }
            if !registry.published_content_for_export(record) {
                return Err(PackageStatus::RegistryRecoveryDenied);
            }
            registry.add_export_record_from_snapshot(record)?;
            offset += PACKAGE_REGISTRY_EXPORT_RECORD_LEN;
        }

        Ok(())
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
        write_u32(out, 68, self.content_count);
        write_u32(out, 72, self.export_count);

        let mut offset = PACKAGE_REGISTRY_HEADER_LEN;
        let mut encoded_packages = 0usize;
        let mut previous_package_id = None;
        while encoded_packages < self.package_count as usize {
            let record = self
                .next_package_record(previous_package_id)
                .ok_or(PackageStatus::RegistryRecoveryDenied)?;
            encode_package_record(record, out, offset);
            offset += PACKAGE_REGISTRY_PACKAGE_RECORD_LEN;
            previous_package_id = Some(record.package_object_id);
            encoded_packages += 1;
        }

        let mut encoded_schemas = 0usize;
        let mut previous_schema = None;
        while encoded_schemas < self.schema_count as usize {
            let record = self
                .next_schema_record(previous_schema)
                .ok_or(PackageStatus::RegistryRecoveryDenied)?;
            encode_schema_record(record, out, offset);
            offset += PACKAGE_REGISTRY_SCHEMA_RECORD_LEN;
            previous_schema = Some((record.schema_object_id, record.schema_revision));
            encoded_schemas += 1;
        }

        let mut encoded_content = 0usize;
        let mut previous_content = None;
        while encoded_content < self.content_count as usize {
            let record = self
                .next_content_record(previous_content)
                .ok_or(PackageStatus::RegistryRecoveryDenied)?;
            encode_content_record(record, out, offset);
            offset += PACKAGE_REGISTRY_CONTENT_RECORD_LEN;
            previous_content = Some((
                record.package_object_id,
                record.release_digest,
                record.content_index,
            ));
            encoded_content += 1;
        }

        let mut encoded_exports = 0usize;
        let mut previous_export = None;
        while encoded_exports < self.export_count as usize {
            let record = self
                .next_export_record(previous_export)
                .ok_or(PackageStatus::RegistryRecoveryDenied)?;
            encode_export_record(record, out, offset);
            offset += PACKAGE_REGISTRY_EXPORT_RECORD_LEN;
            previous_export = Some(export_sort_key(record));
            encoded_exports += 1;
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
            + self.content_count as usize * PACKAGE_REGISTRY_CONTENT_RECORD_LEN
            + self.export_count as usize * PACKAGE_REGISTRY_EXPORT_RECORD_LEN
            + PACKAGE_REGISTRY_CRC_LEN
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub const fn root_digest(&self) -> [u8; 32] {
        self.root_digest
    }

    pub fn clear_to_empty(&mut self) {
        self.generation = 1;
        self.active_transaction_id = 0;
        self.committed_root_digest = [0; 32];
        self.root_digest = [0; 32];
        self.package_count = 0;
        self.schema_count = 0;
        self.content_count = 0;
        self.export_count = 0;
        self.package_records.fill(None);
        self.schema_records.fill(None);
        self.content_records.fill(None);
        self.export_records.fill(None);
    }

    pub fn record_committed_generation(&mut self, generation: PackageRegistryGeneration) {
        self.generation = generation.generation;
        self.root_digest = generation.root_digest;
        self.committed_root_digest = generation.root_digest;
        self.active_transaction_id = 0;
    }

    pub fn begin_candidate_generation(&mut self, transaction_id: u64) -> Result<(), PackageStatus> {
        if transaction_id == 0 {
            return Err(PackageStatus::BadRequest);
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(PackageStatus::LengthOverflow)?;
        self.active_transaction_id = transaction_id;
        Ok(())
    }

    pub fn copy_from_committed(&mut self, source: &Self) {
        self.generation = source.generation;
        self.active_transaction_id = source.active_transaction_id;
        self.committed_root_digest = source.committed_root_digest;
        self.root_digest = source.root_digest;
        self.package_count = source.package_count;
        self.schema_count = source.schema_count;
        self.content_count = source.content_count;
        self.export_count = source.export_count;
        self.package_records
            .copy_from_slice(&source.package_records);
        self.schema_records.copy_from_slice(&source.schema_records);
        self.content_records
            .copy_from_slice(&source.content_records);
        self.export_records.copy_from_slice(&source.export_records);
    }

    pub const fn package_count(&self) -> usize {
        self.package_count as usize
    }

    pub const fn schema_count(&self) -> usize {
        self.schema_count as usize
    }

    pub const fn content_count(&self) -> usize {
        self.content_count as usize
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

    pub fn content_record(&self, index: usize) -> Option<PackageRegistryContentRecord> {
        if index >= self.content_count as usize {
            return None;
        }
        self.content_records[index]
    }

    pub fn export_for_locator(
        &self,
        namespace_root: ObjectId,
        locator: &str,
    ) -> Result<PackageRegistryExportRecord, PackageStatus> {
        let parsed = parse_export_locator(locator)?;
        let mut index = 0usize;
        while index < self.export_count as usize {
            if let Some(record) = self.export_records[index]
                && record.namespace_root_object_id == namespace_root.raw()
                && record.package_locator_len as usize == parsed.package_locator.len()
                && record.export_name_len as usize == parsed.export_name.len()
                && &record.package_locator[..parsed.package_locator.len()] == parsed.package_locator
                && &record.export_name[..parsed.export_name.len()] == parsed.export_name
            {
                self.active_package_for_export(record)?;
                return Ok(record);
            }
            index += 1;
        }
        Err(PackageStatus::ExportMissing)
    }

    pub fn encoded_len_from_snapshot_header(bytes: &[u8]) -> Result<usize, PackageStatus> {
        if bytes.len() < PACKAGE_REGISTRY_HEADER_LEN {
            return Err(PackageStatus::BoundsExceeded);
        }
        if &bytes[0..8] != PACKAGE_REGISTRY_MAGIC {
            return Err(PackageStatus::InvalidMagic);
        }
        if read_u16(bytes, 8) != PACKAGE_REGISTRY_MAJOR {
            return Err(PackageStatus::UnsupportedMajor);
        }
        encoded_len_for(
            read_u32(bytes, 60),
            read_u32(bytes, 64),
            read_u32(bytes, 68),
            read_u32(bytes, 72),
        )
    }

    fn next_package_record(
        &self,
        previous_package_id: Option<u64>,
    ) -> Option<PackageRegistryPackageRecord> {
        let mut selected = None;
        let mut index = 0usize;
        while index < self.package_count as usize {
            if let Some(record) = self.package_records[index] {
                let after_previous =
                    previous_package_id.is_none_or(|previous| record.package_object_id > previous);
                let before_selected =
                    selected.is_none_or(|current: PackageRegistryPackageRecord| {
                        record.package_object_id < current.package_object_id
                    });
                if after_previous && before_selected {
                    selected = Some(record);
                }
            }
            index += 1;
        }
        selected
    }

    fn active_package_for_export(
        &self,
        export: PackageRegistryExportRecord,
    ) -> Result<PackageRegistryPackageRecord, PackageStatus> {
        let record = self.package_record_for_export(export)?;
        match record.status {
            status if status == PackageStatus::Ok as u16 => Ok(record),
            status if status == PackageStatus::PackageDisabled as u16 => {
                Err(PackageStatus::PackageDisabled)
            }
            status if status == PackageStatus::PackageTombstoned as u16 => {
                Err(PackageStatus::PackageTombstoned)
            }
            _ => Err(PackageStatus::ExportMissing),
        }
    }

    fn package_record_for_export(
        &self,
        export: PackageRegistryExportRecord,
    ) -> Result<PackageRegistryPackageRecord, PackageStatus> {
        let mut index = 0usize;
        while index < self.package_count as usize {
            if let Some(record) = self.package_records[index]
                && record.package_object_id == export.package_object_id
                && record.installed_revision == export.package_revision
                && record.release_digest == export.release_digest
            {
                return Ok(record);
            }
            index += 1;
        }
        Err(PackageStatus::ExportMissing)
    }

    fn retainable_package_record_for_export(
        &self,
        export: PackageRegistryExportRecord,
    ) -> Result<PackageRegistryPackageRecord, PackageStatus> {
        let record = self.package_record_for_export(export)?;
        match record.status {
            status
                if status == PackageStatus::Ok as u16
                    || status == PackageStatus::PackageDisabled as u16 =>
            {
                Ok(record)
            }
            status if status == PackageStatus::PackageTombstoned as u16 => {
                Err(PackageStatus::PackageTombstoned)
            }
            _ => Err(PackageStatus::ExportMissing),
        }
    }

    fn remove_exports_for_package(&mut self, package_object_id: u64) {
        let mut read_index = 0usize;
        let mut write_index = 0usize;
        while read_index < self.export_count as usize {
            if let Some(record) = self.export_records[read_index]
                && record.package_object_id != package_object_id
            {
                self.export_records[write_index] = Some(record);
                write_index += 1;
            }
            read_index += 1;
        }
        let retained_count = write_index as u32;
        while write_index < self.export_count as usize {
            self.export_records[write_index] = None;
            write_index += 1;
        }
        self.export_count = retained_count;
    }

    fn adjust_schema_descriptor_retention(
        &mut self,
        schema_object_id: ObjectId,
        schema_revision: u64,
        delta: i16,
    ) -> Result<(), PackageStatus> {
        let schema = self
            .schema_record_by_identity(schema_object_id.raw(), schema_revision)
            .ok_or(PackageStatus::NotFound)?;
        let index = self
            .content_index_for_schema_descriptor(schema)
            .ok_or(PackageStatus::NotFound)?;
        let mut record = self.content_records[index].ok_or(PackageStatus::NotFound)?;
        if delta > 0 {
            record.retention_count = record
                .retention_count
                .checked_add(delta as u16)
                .ok_or(PackageStatus::QuotaDenied)?;
        } else {
            let decrement = (-delta) as u16;
            record.retention_count = record
                .retention_count
                .checked_sub(decrement)
                .ok_or(PackageStatus::NotFound)?;
        }
        self.content_records[index] = Some(record);
        Ok(())
    }

    fn package_record_by_object_id(
        &self,
        package_object_id: u64,
    ) -> Option<PackageRegistryPackageRecord> {
        let mut index = 0usize;
        while index < self.package_count as usize {
            if let Some(record) = self.package_records[index]
                && record.package_object_id == package_object_id
            {
                return Some(record);
            }
            index += 1;
        }
        None
    }

    fn schema_record_by_identity(
        &self,
        schema_object_id: u64,
        schema_revision: u64,
    ) -> Option<PackageRegistrySchemaRecord> {
        let mut index = 0usize;
        while index < self.schema_count as usize {
            if let Some(record) = self.schema_records[index]
                && record.schema_object_id == schema_object_id
                && record.schema_revision == schema_revision
            {
                return Some(record);
            }
            index += 1;
        }
        None
    }

    fn content_index_for_schema_descriptor(
        &self,
        schema: PackageRegistrySchemaRecord,
    ) -> Option<usize> {
        let mut index = 0usize;
        while index < self.content_count as usize {
            if let Some(record) = self.content_records[index]
                && record.package_object_id == schema.package_object_id
                && record.content_index == schema.descriptor_content_index
                && record.digest == schema.descriptor_digest
            {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    fn retained_schema_descriptor_content(&self, content: PackageRegistryContentRecord) -> bool {
        if content.retention_count == 0 {
            return false;
        }
        let mut index = 0usize;
        while index < self.schema_count as usize {
            if let Some(schema) = self.schema_records[index]
                && content.package_object_id == schema.package_object_id
                && content.content_index == schema.descriptor_content_index
                && content.digest == schema.descriptor_digest
            {
                return true;
            }
            index += 1;
        }
        false
    }

    fn published_content_for_export(&self, export: PackageRegistryExportRecord) -> bool {
        let mut index = 0usize;
        while index < self.content_count as usize {
            if let Some(record) = self.content_records[index]
                && record.package_object_id == export.package_object_id
                && record.release_digest == export.release_digest
                && record.content_index == export.content_index
            {
                return true;
            }
            index += 1;
        }
        false
    }

    fn next_schema_record(
        &self,
        previous_schema: Option<(u64, u64)>,
    ) -> Option<PackageRegistrySchemaRecord> {
        let mut selected = None;
        let mut index = 0usize;
        while index < self.schema_count as usize {
            if let Some(record) = self.schema_records[index] {
                let key = (record.schema_object_id, record.schema_revision);
                let after_previous = previous_schema.is_none_or(|previous| key > previous);
                let before_selected =
                    selected.is_none_or(|current: PackageRegistrySchemaRecord| {
                        key < (current.schema_object_id, current.schema_revision)
                    });
                if after_previous && before_selected {
                    selected = Some(record);
                }
            }
            index += 1;
        }
        selected
    }

    fn next_content_record(
        &self,
        previous_content: Option<(u64, [u8; 32], u16)>,
    ) -> Option<PackageRegistryContentRecord> {
        let mut selected = None;
        let mut index = 0usize;
        while index < self.content_count as usize {
            if let Some(record) = self.content_records[index] {
                let key = (
                    record.package_object_id,
                    record.release_digest,
                    record.content_index,
                );
                let after_previous = previous_content.is_none_or(|previous| key > previous);
                let before_selected =
                    selected.is_none_or(|current: PackageRegistryContentRecord| {
                        key < (
                            current.package_object_id,
                            current.release_digest,
                            current.content_index,
                        )
                    });
                if after_previous && before_selected {
                    selected = Some(record);
                }
            }
            index += 1;
        }
        selected
    }

    fn next_export_record(
        &self,
        previous_export: Option<ExportSortKey>,
    ) -> Option<PackageRegistryExportRecord> {
        let mut selected = None;
        let mut index = 0usize;
        while index < self.export_count as usize {
            if let Some(record) = self.export_records[index] {
                let key = export_sort_key(record);
                let after_previous = previous_export.is_none_or(|previous| key > previous);
                let before_selected =
                    selected.is_none_or(|current: PackageRegistryExportRecord| {
                        key < export_sort_key(current)
                    });
                if after_previous && before_selected {
                    selected = Some(record);
                }
            }
            index += 1;
        }
        selected
    }
}

type ExportSortKey = (
    u64,
    [u8; MAX_LOCATOR_SEGMENT_BYTES],
    u8,
    [u8; MAX_LOCATOR_SEGMENT_BYTES],
    u8,
    u64,
    u64,
);

fn export_sort_key(record: PackageRegistryExportRecord) -> ExportSortKey {
    (
        record.namespace_root_object_id,
        record.package_locator,
        record.package_locator_len,
        record.export_name,
        record.export_name_len,
        record.package_object_id,
        record.package_revision,
    )
}

struct ExportLocatorParts<'a> {
    package_locator: &'a [u8],
    export_name: &'a [u8],
}

fn parse_export_locator(locator: &str) -> Result<ExportLocatorParts<'_>, PackageStatus> {
    validate_locator(locator).map_err(|_| PackageStatus::InvalidLocator)?;
    let bytes = locator.as_bytes();
    let Some(separator) = bytes.iter().position(|byte| *byte == b'/') else {
        return Err(PackageStatus::ExportMissing);
    };
    if bytes[separator + 1..].contains(&b'/') {
        return Err(PackageStatus::ExportMissing);
    }
    Ok(ExportLocatorParts {
        package_locator: &bytes[..separator],
        export_name: &bytes[separator + 1..],
    })
}

fn copy_locator_segment(
    bytes: &[u8],
) -> Result<([u8; MAX_LOCATOR_SEGMENT_BYTES], u8), PackageStatus> {
    if bytes.contains(&b'/') {
        return Err(PackageStatus::InvalidLocator);
    }
    let segment = core::str::from_utf8(bytes).map_err(|_| PackageStatus::InvalidLocator)?;
    validate_locator(segment).map_err(|_| PackageStatus::InvalidLocator)?;
    if bytes.len() > MAX_LOCATOR_SEGMENT_BYTES {
        return Err(PackageStatus::InvalidLocator);
    }
    let mut stored = [0u8; MAX_LOCATOR_SEGMENT_BYTES];
    stored[..bytes.len()].copy_from_slice(bytes);
    Ok((stored, bytes.len() as u8))
}

fn locator_segment_eq(
    left: [u8; MAX_LOCATOR_SEGMENT_BYTES],
    left_len: u8,
    right: [u8; MAX_LOCATOR_SEGMENT_BYTES],
    right_len: u8,
) -> bool {
    left_len == right_len && left[..left_len as usize] == right[..right_len as usize]
}

fn encoded_len_for(
    package_count: u32,
    schema_count: u32,
    content_count: u32,
    export_count: u32,
) -> Result<usize, PackageStatus> {
    let package_bytes = (package_count as usize)
        .checked_mul(PACKAGE_REGISTRY_PACKAGE_RECORD_LEN)
        .ok_or(PackageStatus::LengthOverflow)?;
    let schema_bytes = (schema_count as usize)
        .checked_mul(PACKAGE_REGISTRY_SCHEMA_RECORD_LEN)
        .ok_or(PackageStatus::LengthOverflow)?;
    let content_bytes = (content_count as usize)
        .checked_mul(PACKAGE_REGISTRY_CONTENT_RECORD_LEN)
        .ok_or(PackageStatus::LengthOverflow)?;
    let export_bytes = (export_count as usize)
        .checked_mul(PACKAGE_REGISTRY_EXPORT_RECORD_LEN)
        .ok_or(PackageStatus::LengthOverflow)?;
    PACKAGE_REGISTRY_HEADER_LEN
        .checked_add(package_bytes)
        .and_then(|value| value.checked_add(schema_bytes))
        .and_then(|value| value.checked_add(content_bytes))
        .and_then(|value| value.checked_add(export_bytes))
        .and_then(|value| value.checked_add(PACKAGE_REGISTRY_CRC_LEN))
        .ok_or(PackageStatus::LengthOverflow)
}

fn encode_package_record(record: PackageRegistryPackageRecord, out: &mut [u8], offset: usize) {
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

fn encode_content_record(record: PackageRegistryContentRecord, out: &mut [u8], offset: usize) {
    out[offset..offset + PACKAGE_REGISTRY_CONTENT_RECORD_LEN].fill(0);
    write_u16(out, offset, record.content_index);
    write_u16(out, offset + 2, record.role);
    write_u16(out, offset + 4, record.format);
    write_u16(out, offset + 6, record.extent_count);
    write_u64(out, offset + 8, record.package_object_id);
    write_u64(out, offset + 16, record.byte_len);
    out[offset + 24..offset + 56].copy_from_slice(&record.release_digest);
    out[offset + 56..offset + 88].copy_from_slice(&record.digest);
    write_u16(out, offset + 88, record.retention_count);
    write_u16(out, offset + 90, record.flags);
    let mut extent_index = 0usize;
    while extent_index < MAX_CONTENT_EXTENTS_PER_RECORD {
        let extent = record.extents[extent_index];
        let extent_offset = offset + 96 + extent_index * 4;
        write_u16(out, extent_offset, extent.start_block);
        write_u16(out, extent_offset + 2, extent.block_count);
        extent_index += 1;
    }
}

fn decode_content_record(
    bytes: &[u8],
    offset: usize,
) -> Result<PackageRegistryContentRecord, PackageStatus> {
    if bytes[offset + 92..offset + 96]
        .iter()
        .any(|byte| *byte != 0)
        || bytes[offset + 224..offset + 256]
            .iter()
            .any(|byte| *byte != 0)
    {
        return Err(PackageStatus::RegistryRecoveryDenied);
    }
    let mut extents = [PackageExtent::EMPTY; MAX_CONTENT_EXTENTS_PER_RECORD];
    let mut extent_index = 0usize;
    while extent_index < MAX_CONTENT_EXTENTS_PER_RECORD {
        let extent_offset = offset + 96 + extent_index * 4;
        extents[extent_index] = PackageExtent::new(
            read_u16(bytes, extent_offset),
            read_u16(bytes, extent_offset + 2),
        );
        extent_index += 1;
    }
    Ok(PackageRegistryContentRecord {
        package_object_id: read_u64(bytes, offset + 8),
        release_digest: read_sha256(bytes, offset + 24),
        content_index: read_u16(bytes, offset),
        role: read_u16(bytes, offset + 2),
        format: read_u16(bytes, offset + 4),
        digest: read_sha256(bytes, offset + 56),
        byte_len: read_u64(bytes, offset + 16),
        extents,
        extent_count: read_u16(bytes, offset + 6),
        retention_count: read_u16(bytes, offset + 88),
        flags: read_u16(bytes, offset + 90),
    })
}

fn encode_export_record(record: PackageRegistryExportRecord, out: &mut [u8], offset: usize) {
    out[offset..offset + PACKAGE_REGISTRY_EXPORT_RECORD_LEN].fill(0);
    write_u64(out, offset, record.namespace_root_object_id);
    out[offset + 8] = record.package_locator_len;
    out[offset + 9] = record.export_name_len;
    write_u16(out, offset + EXPORT_RECORD_FLAGS_OFFSET, 0);
    write_u16(out, offset + 12, record.export_kind);
    write_u16(out, offset + 14, record.content_index);
    write_u16(out, offset + 16, record.entrypoint);
    out[offset + 20..offset + 36].copy_from_slice(&record.package_locator);
    out[offset + 36..offset + 52].copy_from_slice(&record.export_name);
    write_u64(out, offset + 56, record.package_object_id);
    write_u64(out, offset + 64, record.package_revision);
    out[offset + 72..offset + 104].copy_from_slice(&record.release_digest);
    write_u64(out, offset + 104, record.schema_object_id);
    write_u64(out, offset + 112, record.schema_revision);
    out[offset + 120..offset + 152].copy_from_slice(&record.schema_descriptor_digest);
}

fn decode_export_record(
    bytes: &[u8],
    offset: usize,
) -> Result<(PackageRegistryExportRecord, u16), PackageStatus> {
    let package_locator_len = bytes[offset + 8] as usize;
    let export_name_len = bytes[offset + 9] as usize;
    if package_locator_len > MAX_LOCATOR_SEGMENT_BYTES
        || export_name_len > MAX_LOCATOR_SEGMENT_BYTES
    {
        return Err(PackageStatus::InvalidLocator);
    }
    if bytes[offset + 18..offset + 20]
        .iter()
        .any(|byte| *byte != 0)
        || bytes[offset + 52..offset + 56]
            .iter()
            .any(|byte| *byte != 0)
        || bytes[offset + 20 + package_locator_len..offset + 36]
            .iter()
            .any(|byte| *byte != 0)
        || bytes[offset + 36 + export_name_len..offset + 52]
            .iter()
            .any(|byte| *byte != 0)
        || bytes[offset + 152..offset + 160]
            .iter()
            .any(|byte| *byte != 0)
    {
        return Err(PackageStatus::RegistryRecoveryDenied);
    }
    let flags = read_u16(bytes, offset + EXPORT_RECORD_FLAGS_OFFSET);
    let record = PackageRegistryExportRecord::new(
        read_u64(bytes, offset),
        &bytes[offset + 20..offset + 20 + package_locator_len],
        &bytes[offset + 36..offset + 36 + export_name_len],
        read_u64(bytes, offset + 56),
        read_u64(bytes, offset + 64),
        read_sha256(bytes, offset + 72),
        read_u16(bytes, offset + 12),
        read_u16(bytes, offset + 14),
        read_u16(bytes, offset + 16),
        read_u64(bytes, offset + 104),
        read_u64(bytes, offset + 112),
        read_sha256(bytes, offset + 120),
    )?;
    Ok((record, flags))
}

fn validate_content_record(record: PackageRegistryContentRecord) -> Result<(), PackageStatus> {
    if record.byte_len > MAX_CONTENT_BYTES as u64
        || record.extent_count as usize > MAX_CONTENT_EXTENTS_PER_RECORD
    {
        return Err(PackageStatus::BoundsExceeded);
    }
    let expected_blocks = record
        .byte_len
        .checked_add(511)
        .ok_or(PackageStatus::LengthOverflow)?
        / 512;
    let mut blocks = 0u64;
    let mut index = 0usize;
    while index < MAX_CONTENT_EXTENTS_PER_RECORD {
        let extent = record.extents[index];
        if index < record.extent_count as usize {
            if extent.block_count == 0
                || u32::from(extent.start_block) + u32::from(extent.block_count)
                    > pythos_shared::package_abi::PACKAGE_CONTENT_MAX_BLOCKS as u32
            {
                return Err(PackageStatus::BoundsExceeded);
            }
            blocks += u64::from(extent.block_count);
        } else if extent != PackageExtent::EMPTY {
            return Err(PackageStatus::RegistryRecoveryDenied);
        }
        index += 1;
    }
    if blocks != expected_blocks {
        return Err(PackageStatus::BoundsExceeded);
    }
    Ok(())
}

fn snapshot_crc32c(bytes: &[u8]) -> u32 {
    let crc_offset = bytes.len() - PACKAGE_REGISTRY_CRC_LEN;
    let mut crc = !0u32;
    crc = crc32c_update(crc, &bytes[..crc_offset]);
    crc = crc32c_update(crc, &[0, 0, 0, 0]);
    !crc
}

fn transaction_anchor_crc32c(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    crc = crc32c_update(crc, &bytes[..PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET]);
    crc = crc32c_update(crc, &[0, 0, 0, 0]);
    crc = crc32c_update(
        crc,
        &bytes[PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET + 4..PACKAGE_TRANSACTION_COMMIT_V0_LEN],
    );
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
        PACKAGE_RECORD_FLAGS_OFFSET, PACKAGE_REGISTRY_HEADER_LEN,
        PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET, PACKAGE_TRANSACTION_COMMIT_V0_LEN, PackageRegistry,
        PackageRegistryContentRecord, PackageRegistryExportRecord, PackageRegistryGeneration,
        PackageRegistryPackageRecord, PackageRegistrySchemaRecord, PackageTransactionCommitV0,
        REGISTRY_RECORD_FLAG_REQUIRES_MINOR_SUPPORT, crc32c_castagnoli,
    };
    use crate::{
        block_device::BlockDeviceInfo,
        object_relationships::PACKAGE_LOCATOR_ROOT_OBJECT_ID,
        object_service_checkpoint::ObjectCheckpointIdentity,
        package_candidate_store::{
            PACKAGE_CANDIDATE_STORAGE_TEST_LOCK, read_candidate_registry_generation,
            reset_package_candidate_storage_for_test, write_candidate_registry_generation,
        },
        package_content_store::PackageExtent,
        shell_objects::ObjectId,
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
    fn package_registry_export_record_rejects_multi_segment_storage_names() {
        assert_eq!(
            PackageRegistryExportRecord::new(
                1,
                b"seed/tools",
                b"launch",
                42,
                7,
                digest(4),
                1,
                0,
                0,
                77,
                3,
                digest(7),
            ),
            Err(PackageStatus::InvalidLocator)
        );
        assert_eq!(
            PackageRegistryExportRecord::new(
                1,
                b"seed",
                b"tools/launch",
                42,
                7,
                digest(4),
                1,
                0,
                0,
                77,
                3,
                digest(7),
            ),
            Err(PackageStatus::InvalidLocator)
        );
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
    fn package_registry_rejects_unsupported_required_minor_content_record_flag() {
        let mut registry = registry_with_one_package_and_schema();
        registry
            .add_content_record(PackageRegistryContentRecord {
                package_object_id: 42,
                release_digest: digest(4),
                content_index: 0,
                role: 1,
                format: 1,
                digest: digest(8),
                byte_len: 0,
                extents: [PackageExtent::EMPTY; 32],
                extent_count: 0,
                retention_count: 0,
                flags: 0,
            })
            .unwrap();
        let mut encoded = [0u8; 512];
        registry.encode_snapshot(&mut encoded).unwrap();
        let used = registry.encoded_len();

        encoded[10..12].copy_from_slice(&2u16.to_le_bytes());
        let content_flags_offset = PACKAGE_REGISTRY_HEADER_LEN + 64 + 96 + 90;
        encoded[content_flags_offset..content_flags_offset + 2]
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
    fn package_registry_round_trips_export_records() {
        let mut registry = PackageRegistry::empty();
        registry
            .add_package_record(PackageRegistryPackageRecord::new(
                42,
                7,
                digest(4),
                PackageStatus::Ok as u16,
            ))
            .unwrap();
        registry
            .add_schema_record(PackageRegistrySchemaRecord::new(77, 3, 42, 0, digest(7)))
            .unwrap();
        let mut extents = [PackageExtent::EMPTY; 32];
        extents[0] = PackageExtent::new(7, 1);
        registry
            .add_content_record(PackageRegistryContentRecord {
                package_object_id: 42,
                release_digest: digest(4),
                content_index: 1,
                role: 2,
                format: 1,
                digest: digest(5),
                byte_len: 3,
                extents,
                extent_count: 1,
                retention_count: 0,
                flags: 0,
            })
            .unwrap();
        let export = PackageRegistryExportRecord::new(
            PACKAGE_LOCATOR_ROOT_OBJECT_ID,
            b"seed",
            b"launch",
            42,
            7,
            digest(4),
            1,
            1,
            0,
            77,
            3,
            digest(7),
        )
        .unwrap();
        registry.add_export_record(export).unwrap();
        let mut encoded = [0u8; 1024];

        registry.encode_snapshot(&mut encoded).unwrap();
        let used = registry.encoded_len();
        let restored = PackageRegistry::decode_snapshot(&encoded[..used]).unwrap();

        assert_eq!(
            u32::from_le_bytes([encoded[72], encoded[73], encoded[74], encoded[75]]),
            1
        );
        assert_eq!(
            restored
                .export_for_locator(ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID), "seed/launch")
                .unwrap(),
            export
        );
    }

    #[test]
    fn package_disable_retains_registry_records_and_persists_denial_state() {
        let mut registry = PackageRegistry::empty();
        let package = PackageRegistryPackageRecord::new(42, 7, digest(4), PackageStatus::Ok as u16);
        let schema = PackageRegistrySchemaRecord::new(77, 3, 42, 1, digest(7));
        let mut extents = [PackageExtent::EMPTY; 32];
        extents[0] = PackageExtent::new(7, 1);
        let content = PackageRegistryContentRecord {
            package_object_id: 42,
            release_digest: digest(4),
            content_index: 1,
            role: 2,
            format: 1,
            digest: digest(5),
            byte_len: 3,
            extents,
            extent_count: 1,
            retention_count: 0,
            flags: 0,
        };
        let export = PackageRegistryExportRecord::new(
            PACKAGE_LOCATOR_ROOT_OBJECT_ID,
            b"seed",
            b"launch",
            42,
            7,
            digest(4),
            1,
            1,
            0,
            77,
            3,
            digest(7),
        )
        .unwrap();
        registry.add_package_record(package).unwrap();
        registry.add_schema_record(schema).unwrap();
        registry.add_content_record(content).unwrap();
        registry.add_export_record(export).unwrap();
        assert_eq!(
            registry
                .export_for_locator(ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID), "seed/launch")
                .unwrap(),
            export
        );

        let disabled = registry.disable(ObjectId::new(42)).unwrap();

        assert_eq!(disabled.package_object_id, package.package_object_id);
        assert_eq!(disabled.installed_revision, package.installed_revision);
        assert_eq!(disabled.release_digest, package.release_digest);
        assert_eq!(disabled.status, PackageStatus::PackageDisabled as u16);
        assert_eq!(registry.package_count(), 1);
        assert_eq!(registry.schema_count(), 1);
        assert_eq!(registry.content_count(), 1);
        assert_eq!(registry.export_count, 1);
        assert_eq!(registry.schema_record(0), Some(schema));
        assert_eq!(registry.content_record(0), Some(content));
        assert_eq!(registry.export_records[0], Some(export));
        assert_eq!(
            registry
                .export_for_locator(ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID), "seed/launch"),
            Err(PackageStatus::PackageDisabled)
        );

        let mut encoded = [0u8; 1024];
        registry.encode_snapshot(&mut encoded).unwrap();
        let restored =
            PackageRegistry::decode_snapshot(&encoded[..registry.encoded_len()]).unwrap();

        assert_eq!(restored.package_count(), 1);
        assert_eq!(restored.schema_count(), 1);
        assert_eq!(restored.content_count(), 1);
        assert_eq!(restored.export_count, 1);
        assert_eq!(
            restored.package_record(0).unwrap().package_object_id,
            package.package_object_id
        );
        assert_eq!(
            restored.package_record(0).unwrap().installed_revision,
            package.installed_revision
        );
        assert_eq!(
            restored.package_record(0).unwrap().status,
            PackageStatus::PackageDisabled as u16
        );
        assert_eq!(restored.schema_record(0), Some(schema));
        assert_eq!(restored.content_record(0), Some(content));
        assert_eq!(restored.export_records[0], Some(export));
        assert_eq!(
            restored
                .export_for_locator(ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID), "seed/launch"),
            Err(PackageStatus::PackageDisabled)
        );
    }

    #[test]
    fn package_disable_snapshot_decode_does_not_accept_tombstoned_exports() {
        let mut registry = PackageRegistry::empty();
        registry
            .add_package_record(PackageRegistryPackageRecord::new(
                42,
                7,
                digest(4),
                PackageStatus::Ok as u16,
            ))
            .unwrap();
        registry
            .add_schema_record(PackageRegistrySchemaRecord::new(77, 3, 42, 1, digest(7)))
            .unwrap();
        let mut extents = [PackageExtent::EMPTY; 32];
        extents[0] = PackageExtent::new(7, 1);
        registry
            .add_content_record(PackageRegistryContentRecord {
                package_object_id: 42,
                release_digest: digest(4),
                content_index: 1,
                role: 2,
                format: 1,
                digest: digest(5),
                byte_len: 3,
                extents,
                extent_count: 1,
                retention_count: 0,
                flags: 0,
            })
            .unwrap();
        registry
            .add_export_record(
                PackageRegistryExportRecord::new(
                    PACKAGE_LOCATOR_ROOT_OBJECT_ID,
                    b"seed",
                    b"launch",
                    42,
                    7,
                    digest(4),
                    1,
                    1,
                    0,
                    77,
                    3,
                    digest(7),
                )
                .unwrap(),
            )
            .unwrap();
        registry.package_records[0].as_mut().unwrap().status =
            PackageStatus::PackageTombstoned as u16;
        let mut encoded = [0u8; 1024];
        registry.encode_snapshot(&mut encoded).unwrap();

        assert_eq!(
            PackageRegistry::decode_snapshot(&encoded[..registry.encoded_len()]),
            Err(PackageStatus::PackageTombstoned)
        );
    }

    #[test]
    fn package_registry_recovery_denies_export_without_published_content_record() {
        let mut registry = PackageRegistry::empty();
        registry
            .add_package_record(PackageRegistryPackageRecord::new(
                42,
                7,
                digest(4),
                PackageStatus::Ok as u16,
            ))
            .unwrap();
        registry
            .add_schema_record(PackageRegistrySchemaRecord::new(77, 3, 42, 0, digest(7)))
            .unwrap();
        registry
            .add_export_record(
                PackageRegistryExportRecord::new(
                    PACKAGE_LOCATOR_ROOT_OBJECT_ID,
                    b"seed",
                    b"launch",
                    42,
                    7,
                    digest(4),
                    1,
                    1,
                    0,
                    77,
                    3,
                    digest(7),
                )
                .unwrap(),
            )
            .unwrap();
        let mut encoded = [0u8; 1024];

        registry.encode_snapshot(&mut encoded).unwrap();
        let used = registry.encoded_len();

        assert_eq!(
            PackageRegistry::decode_snapshot(&encoded[..used]),
            Err(PackageStatus::RegistryRecoveryDenied)
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

        let selected = PackageRegistry::select_generation(
            &unsupported_newer[..newer_len],
            &older[..older_len],
        )
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
            PackageRegistry::select_generation(
                &corrupt[..corrupt_len],
                &unsupported[..unsupported_len]
            ),
            Err(PackageStatus::RegistryRecoveryDenied)
        );
    }

    #[test]
    fn package_transaction_anchor_crc32c_uses_zero_filled_commit_crc32c() {
        let (anchor, _, _) = anchor_fixture();
        let mut encoded = [0u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN];

        anchor.encode(&mut encoded).unwrap();
        let stored_crc = u32::from_le_bytes([
            encoded[PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET],
            encoded[PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET + 1],
            encoded[PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET + 2],
            encoded[PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET + 3],
        ]);
        encoded[PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET..PACKAGE_TRANSACTION_COMMIT_CRC_OFFSET + 4]
            .fill(0);

        assert_eq!(stored_crc, crc32c_castagnoli(&encoded));
    }

    #[test]
    fn package_transaction_anchor_rejects_wrong_object_digest() {
        let (anchor, registry_generation, mut object_identity) = anchor_fixture();
        let mut encoded = [0u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN];
        anchor.encode(&mut encoded).unwrap();
        object_identity.root_digest = digest(0xEE);

        assert_eq!(
            PackageTransactionCommitV0::decode(&encoded, registry_generation, object_identity),
            Err(PackageStatus::TransactionAnchorMismatch)
        );
    }

    #[test]
    fn package_transaction_anchor_rejects_wrong_registry_digest() {
        let (anchor, mut registry_generation, object_identity) = anchor_fixture();
        let mut encoded = [0u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN];
        anchor.encode(&mut encoded).unwrap();
        registry_generation.root_digest = digest(0xDD);

        assert_eq!(
            PackageTransactionCommitV0::decode(&encoded, registry_generation, object_identity),
            Err(PackageStatus::TransactionAnchorMismatch)
        );
    }

    #[test]
    fn package_transaction_anchor_accepts_exact_object_and_registry_pair() {
        let (anchor, registry_generation, object_identity) = anchor_fixture();
        let mut encoded = [0u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN];
        anchor.encode(&mut encoded).unwrap();

        assert_eq!(
            PackageTransactionCommitV0::decode(&encoded, registry_generation, object_identity),
            Ok(anchor)
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

    #[test]
    fn package_candidate_registry_persists_content_records_by_root() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_candidate_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut registry = registry_with_one_package_and_schema();
        let mut extents = [PackageExtent::EMPTY; 32];
        extents[0] = PackageExtent::new(7, 1);
        let record = PackageRegistryContentRecord {
            package_object_id: 42,
            release_digest: digest(4),
            content_index: 0,
            role: 1,
            format: 1,
            digest: digest(5),
            byte_len: 3,
            extents,
            extent_count: 1,
            retention_count: 0,
            flags: 0,
        };
        registry.add_content_record(record).unwrap();

        let generation = write_candidate_registry_generation(device, &registry).unwrap();
        let restored = read_candidate_registry_generation(device, generation).unwrap();

        assert_eq!(restored.package_count(), 1);
        assert_eq!(restored.schema_count(), 1);
        assert_eq!(restored.content_count(), 1);
        assert_eq!(restored.content_record(0), Some(record));
        let mut wrong_generation = generation;
        wrong_generation.root_digest[0] ^= 0xFF;
        assert!(matches!(
            read_candidate_registry_generation(device, wrong_generation),
            Err(PackageStatus::TransactionAnchorMismatch | PackageStatus::RegistryRecoveryDenied)
        ));
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

    fn anchor_fixture() -> (
        PackageTransactionCommitV0,
        PackageRegistryGeneration,
        ObjectCheckpointIdentity,
    ) {
        let registry_generation = PackageRegistryGeneration {
            generation: 12,
            root_digest: digest(0xA1),
        };
        let object_identity = ObjectCheckpointIdentity {
            generation: 44,
            root_digest: digest(0xB2),
        };
        (
            PackageTransactionCommitV0::new(99, 1, registry_generation, object_identity, 42, 7),
            registry_generation,
            object_identity,
        )
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
