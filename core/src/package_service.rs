use crate::{
    capabilities::{CapabilityHandle, CapabilityTable, ResourceId, RightsMask},
    object_service::{ObjectCreateResult, ObjectService, ObjectServiceError},
    object_service_checkpoint::{ObjectCheckpointIdentity, object_checkpoint_identity},
    package_content_store::{
        ContentId, PackageContentCommit, PackageContentStore, PackageContentTransaction,
    },
    package_registry::{
        PACKAGE_TRANSACTION_COMMIT_V0_LEN, PackageRegistry, PackageRegistryGeneration,
        PackageRegistryPackageRecord, PackageRegistrySchemaRecord, PackageTransactionCommitV0,
    },
    package_source::PackageSourceService,
    process_context::ActiveUserProcess,
    shell_objects::ObjectId,
};
use pythos_shared::{
    package_abi::{PACKAGE_INSTALL_RESOURCE_ID, PACKAGE_INSTALL_RIGHT, PackageStatus},
    package_format::{PackageArtifactV0, PackageFormatError},
};

const FIRST_PACKAGE_OBJECT_ID: u64 = 0x5059_504B_474F_0001;
const FIRST_SCHEMA_OBJECT_ID: u64 = 0x5059_5343_484F_0001;
const INSTALL_OPERATION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageInstallRequest {
    pub caller: ActiveUserProcess,
    pub source_handle: pythos_shared::package_abi::PackageSourceHandle,
    pub source_read_capability: CapabilityHandle,
    pub install_capability: CapabilityHandle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInstallResult {
    pub transaction_id: u64,
    pub package_object_id: u64,
    pub package_revision: u64,
    pub schema_object_id: u64,
    pub schema_revision: u64,
    pub release_digest: [u8; 32],
    pub schema_descriptor_content_id: ContentId,
    pub content_commit: PackageContentCommit,
    pub registry_generation: PackageRegistryGeneration,
    pub object_checkpoint_identity: ObjectCheckpointIdentity,
    pub anchor: PackageTransactionCommitV0,
    pub anchor_bytes: [u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageService<'a> {
    registry: PackageRegistry,
    content_store: PackageContentStore<'a>,
    next_transaction_id: u64,
    next_package_object_id: u64,
    next_schema_object_id: u64,
    locator_mirror_visible: bool,
}

impl<'a> PackageService<'a> {
    pub const fn new_empty_for_test() -> Self {
        Self {
            registry: PackageRegistry::empty(),
            content_store: PackageContentStore::empty(),
            next_transaction_id: 1,
            next_package_object_id: FIRST_PACKAGE_OBJECT_ID,
            next_schema_object_id: FIRST_SCHEMA_OBJECT_ID,
            locator_mirror_visible: false,
        }
    }

    pub fn install(
        &mut self,
        request: PackageInstallRequest,
        source_service: &PackageSourceService<'_>,
        capabilities: &CapabilityTable,
        object_service: &mut ObjectService,
        artifact_buffer: &'a mut [u8],
    ) -> Result<PackageInstallResult, PackageStatus> {
        validate_install_authority(request.caller, capabilities, request.install_capability)?;
        let artifact_len = source_service.read(
            request.caller.service_id(),
            capabilities,
            request.source_handle,
            request.source_read_capability,
            artifact_buffer,
        )?;
        let artifact_bytes = &artifact_buffer[..artifact_len];
        let artifact = PackageArtifactV0::parse(artifact_bytes).map_err(map_format_error)?;
        let descriptor_entry = artifact
            .content_entry(0)
            .ok_or(PackageStatus::InvalidSchema)?;
        let descriptor_bytes = artifact
            .content_bytes(descriptor_entry)
            .map_err(map_format_error)?;
        let release_digest = artifact.artifact_sha256();
        let package_object_id = ObjectId::new(self.next_package_object_id);
        let schema_object_id = ObjectId::new(self.next_schema_object_id);

        let mut staged_content =
            PackageContentTransaction::new(package_object_id.raw(), release_digest);
        let mut descriptor_content_id = None;
        let mut entry_index = 0u16;
        while let Some(entry) = artifact.content_entry(entry_index) {
            let content_bytes = artifact.content_bytes(entry).map_err(map_format_error)?;
            let content_id = self.content_store.stage_content(
                &mut staged_content,
                entry.role,
                entry.format,
                content_bytes,
                entry.sha256,
            )?;
            if entry_index == descriptor_entry.content_index {
                descriptor_content_id = Some(content_id);
            }
            entry_index = entry_index.wrapping_add(1);
        }
        let schema_descriptor_content_id =
            descriptor_content_id.ok_or(PackageStatus::InvalidSchema)?;
        if descriptor_bytes.is_empty() {
            self.content_store.rollback(&mut staged_content);
            return Err(PackageStatus::InvalidSchema);
        }

        let package = create_package_or_rollback(
            object_service,
            request.caller,
            package_object_id,
            release_digest,
            &mut self.content_store,
            &mut staged_content,
        )?;
        let schema = create_schema_or_rollback(
            object_service,
            request.caller,
            schema_object_id,
            package_object_id,
            descriptor_entry.sha256,
            &mut self.content_store,
            &mut staged_content,
        )?;

        let mut staged_registry = self.registry.clone();
        staged_registry.add_package_record(PackageRegistryPackageRecord::new(
            package.object_id.raw(),
            package.revision,
            release_digest,
            PackageStatus::Ok as u16,
        ))?;
        staged_registry.add_schema_record(PackageRegistrySchemaRecord::new(
            schema.object_id.raw(),
            schema.revision,
            package.object_id.raw(),
            descriptor_entry.content_index,
            descriptor_entry.sha256,
        ))?;

        let mut registry_snapshot = [0u8; 4096];
        let registry_generation = staged_registry.encode_snapshot(&mut registry_snapshot)?;
        let object_snapshot = object_service.encode_snapshot().map_err(map_object_error)?;
        let object_identity = object_checkpoint_identity(&object_snapshot);
        let transaction_id = self.next_transaction_id;
        let anchor = PackageTransactionCommitV0::new(
            transaction_id,
            INSTALL_OPERATION,
            registry_generation,
            object_identity,
            package.object_id.raw(),
            package.revision,
        );
        let mut anchor_bytes = [0u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN];
        anchor.encode(&mut anchor_bytes)?;
        let content_commit = self.content_store.commit(&mut staged_content)?;

        staged_registry.record_committed_generation(registry_generation);
        self.registry = staged_registry;
        self.next_transaction_id = self.next_transaction_id.wrapping_add(1);
        self.next_package_object_id = self.next_package_object_id.wrapping_add(1);
        self.next_schema_object_id = self.next_schema_object_id.wrapping_add(1);
        self.locator_mirror_visible = false;

        Ok(PackageInstallResult {
            transaction_id,
            package_object_id: package.object_id.raw(),
            package_revision: package.revision,
            schema_object_id: schema.object_id.raw(),
            schema_revision: schema.revision,
            release_digest,
            schema_descriptor_content_id,
            content_commit,
            registry_generation,
            object_checkpoint_identity: object_identity,
            anchor,
            anchor_bytes,
        })
    }

    pub const fn registry(&self) -> &PackageRegistry {
        &self.registry
    }

    pub const fn content_store(&self) -> &PackageContentStore<'a> {
        &self.content_store
    }

    pub const fn locator_mirror_visible_for_test(&self) -> bool {
        self.locator_mirror_visible
    }
}

fn validate_install_authority(
    caller: ActiveUserProcess,
    capabilities: &CapabilityTable,
    install_capability: CapabilityHandle,
) -> Result<(), PackageStatus> {
    capabilities
        .validate(
            caller.service_id(),
            install_capability,
            ResourceId::new(PACKAGE_INSTALL_RESOURCE_ID),
            RightsMask::new(PACKAGE_INSTALL_RIGHT as u32),
        )
        .map_err(|_| PackageStatus::InstallDenied)
}

fn create_package_or_rollback<'a>(
    object_service: &mut ObjectService,
    caller: ActiveUserProcess,
    package_object_id: ObjectId,
    release_digest: [u8; 32],
    content_store: &mut PackageContentStore<'a>,
    staged_content: &mut PackageContentTransaction<'a>,
) -> Result<ObjectCreateResult, PackageStatus> {
    match object_service.create_package_object(caller, package_object_id, release_digest) {
        Ok(result) => Ok(result),
        Err(error) => {
            content_store.rollback(staged_content);
            Err(map_object_error(error))
        }
    }
}

fn create_schema_or_rollback<'a>(
    object_service: &mut ObjectService,
    caller: ActiveUserProcess,
    schema_object_id: ObjectId,
    package_object_id: ObjectId,
    descriptor_digest: [u8; 32],
    content_store: &mut PackageContentStore<'a>,
    staged_content: &mut PackageContentTransaction<'a>,
) -> Result<ObjectCreateResult, PackageStatus> {
    match object_service.create_schema_definition_object(
        caller,
        schema_object_id,
        package_object_id,
        descriptor_digest,
    ) {
        Ok(result) => Ok(result),
        Err(error) => {
            content_store.rollback(staged_content);
            Err(map_object_error(error))
        }
    }
}

fn map_format_error(error: PackageFormatError) -> PackageStatus {
    match error {
        PackageFormatError::InvalidMagic => PackageStatus::InvalidMagic,
        PackageFormatError::UnsupportedMajor => PackageStatus::UnsupportedMajor,
        PackageFormatError::UnsupportedRequiredMinor => PackageStatus::UnsupportedRequiredMinor,
        PackageFormatError::LengthOverflow => PackageStatus::LengthOverflow,
        PackageFormatError::BoundsExceeded
        | PackageFormatError::TooShort
        | PackageFormatError::InvalidHeaderLength
        | PackageFormatError::NonZeroReserved => PackageStatus::BoundsExceeded,
        PackageFormatError::ManifestDigestMismatch
        | PackageFormatError::ArtifactDigestMismatch
        | PackageFormatError::ContentDigestMismatch => PackageStatus::DigestMismatch,
        PackageFormatError::InvalidManifest
        | PackageFormatError::DuplicateStableName
        | PackageFormatError::UnsortedManifestRecord
        | PackageFormatError::StableNameTooLong
        | PackageFormatError::ManifestPayloadTooLong
        | PackageFormatError::TooManyManifestRecords
        | PackageFormatError::TooManyContentEntries
        | PackageFormatError::TooManyContentExtents
        | PackageFormatError::ContentRangeOutsidePayload => PackageStatus::InvalidManifest,
    }
}

fn map_object_error(error: ObjectServiceError) -> PackageStatus {
    match error {
        ObjectServiceError::Quota(_) => PackageStatus::QuotaDenied,
        _ => PackageStatus::BadRequest,
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{PackageInstallRequest, PackageService};
    use crate::{
        capabilities::{CapabilityTable, ResourceId, RightsMask},
        object_service::ObjectService,
        package_content_store::ContentId,
        package_source::PackageSourceService,
        process_context::ActiveUserProcess,
        service_identity::ServiceId,
        shell_objects::ObjectKind,
    };
    use pythos_shared::{
        init_bundle::{self, INIT_BUNDLE_HEADER_LEN, RECORD_ENTRY_LEN, TYPE_PACKAGE_SOURCE},
        package_abi::{
            MAX_PACKAGE_ARTIFACT_BYTES, PACKAGE_INSTALL_RESOURCE_ID, PACKAGE_INSTALL_RIGHT,
            PACKAGE_SOURCE_READ_RIGHT, PACKAGE_SOURCE_RESOURCE_ID,
        },
        package_format::{CONTENT_ENTRY_V0_LEN, PACKAGE_ARTIFACT_HEADER_LEN},
        sha256::{Sha256, sha256},
    };
    use std::vec;
    use std::vec::Vec;

    const CALLER_SERVICE: ServiceId = ServiceId::from_raw(0x5059_504B_494E_5301);
    const SCHEMA_DESCRIPTOR: &[u8] = b"schema:seed.v0";

    #[test]
    fn package_install_transaction_commits_package_schema_content_registry_and_anchor_as_one_unit()
    {
        let artifact = build_schema_package_artifact();
        let source_record = build_source_record(0, b"phase13-install.pkg", &artifact);
        let bundle_bytes = build_bundle(&[(TYPE_PACKAGE_SOURCE, &source_record)]);
        let init_bundle = init_bundle::validate(&bundle_bytes).unwrap();
        let source_service = PackageSourceService::from_init_bundle(&init_bundle).unwrap();
        let source_handle = source_service.handle_at(0).unwrap();
        let caller = ActiveUserProcess::new(CALLER_SERVICE, 0x504B_494E_5354, 0x13);
        let mut capabilities = CapabilityTable::new();
        let source_read_capability = capabilities
            .grant(
                caller.service_id(),
                ResourceId::new(PACKAGE_SOURCE_RESOURCE_ID),
                RightsMask::new(PACKAGE_SOURCE_READ_RIGHT as u32),
            )
            .unwrap();
        let install_capability = capabilities
            .grant(
                caller.service_id(),
                ResourceId::new(PACKAGE_INSTALL_RESOURCE_ID),
                RightsMask::new(PACKAGE_INSTALL_RIGHT as u32),
            )
            .unwrap();
        let mut object_service = ObjectService::new_for_test();
        let mut package_service = PackageService::new_empty_for_test();
        let mut artifact_buffer = vec![0u8; MAX_PACKAGE_ARTIFACT_BYTES];

        let installed = package_service
            .install(
                PackageInstallRequest {
                    caller,
                    source_handle,
                    source_read_capability,
                    install_capability,
                },
                &source_service,
                &capabilities,
                &mut object_service,
                &mut artifact_buffer,
            )
            .unwrap();

        assert_eq!(package_service.registry().package_count(), 1);
        assert_eq!(package_service.registry().schema_count(), 1);
        assert_eq!(installed.package_revision, 1);
        assert_eq!(installed.schema_revision, 1);
        assert_eq!(installed.content_commit.record_count(), 1);
        assert_eq!(
            package_service
                .content_store()
                .read_committed(ContentId::new(
                    installed.package_object_id,
                    installed.release_digest,
                    0,
                ))
                .unwrap(),
            SCHEMA_DESCRIPTOR
        );
        assert_eq!(
            package_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            installed.package_object_id
        );
        assert_eq!(
            package_service
                .registry()
                .schema_record(0)
                .unwrap()
                .schema_object_id,
            installed.schema_object_id
        );
        assert_eq!(
            crate::package_registry::PackageTransactionCommitV0::decode(
                &installed.anchor_bytes,
                installed.registry_generation,
                installed.object_checkpoint_identity,
            )
            .unwrap(),
            installed.anchor
        );
        assert_eq!(
            object_service
                .dynamic_object_for_test(crate::shell_objects::ObjectId::new(
                    installed.package_object_id,
                ))
                .unwrap()
                .object_kind(),
            ObjectKind::Package
        );
        assert!(!package_service.locator_mirror_visible_for_test());
    }

    fn build_schema_package_artifact() -> Vec<u8> {
        let mut manifest = b"PYTHMAN0".to_vec();
        manifest.extend_from_slice(&1u32.to_le_bytes());
        push_manifest_record(&mut manifest, 1, b"seed.v0", &0u16.to_le_bytes());

        let mut content_table = vec![0u8; CONTENT_ENTRY_V0_LEN];
        content_table[0..2].copy_from_slice(&0u16.to_le_bytes());
        content_table[2..4].copy_from_slice(&2u16.to_le_bytes());
        content_table[4..6].copy_from_slice(&1u16.to_le_bytes());
        content_table[6..8].copy_from_slice(&1u16.to_le_bytes());
        content_table[8..16].copy_from_slice(&0u64.to_le_bytes());
        content_table[16..24].copy_from_slice(&(SCHEMA_DESCRIPTOR.len() as u64).to_le_bytes());
        content_table[24..56].copy_from_slice(&sha256(SCHEMA_DESCRIPTOR));

        let manifest_offset = PACKAGE_ARTIFACT_HEADER_LEN;
        let content_table_offset = manifest_offset + manifest.len();
        let content_offset = content_table_offset + content_table.len();
        let mut header = vec![0u8; PACKAGE_ARTIFACT_HEADER_LEN];
        header[0..8].copy_from_slice(b"PYTHPKG0");
        header[8..10].copy_from_slice(&0u16.to_le_bytes());
        header[10..12].copy_from_slice(&1u16.to_le_bytes());
        header[12..16].copy_from_slice(&(PACKAGE_ARTIFACT_HEADER_LEN as u32).to_le_bytes());
        header[16..24].copy_from_slice(&(manifest_offset as u64).to_le_bytes());
        header[24..32].copy_from_slice(&(manifest.len() as u64).to_le_bytes());
        header[32..40].copy_from_slice(&(content_table_offset as u64).to_le_bytes());
        header[40..48].copy_from_slice(&(content_table.len() as u64).to_le_bytes());
        header[48..56].copy_from_slice(&(content_offset as u64).to_le_bytes());
        header[56..64].copy_from_slice(&(SCHEMA_DESCRIPTOR.len() as u64).to_le_bytes());
        header[64..96].copy_from_slice(&sha256(&manifest));

        let mut artifact = header;
        artifact.extend_from_slice(&manifest);
        artifact.extend_from_slice(&content_table);
        artifact.extend_from_slice(SCHEMA_DESCRIPTOR);
        let digest = artifact_digest(&artifact);
        artifact[96..128].copy_from_slice(&digest);
        artifact
    }

    fn push_manifest_record(out: &mut Vec<u8>, record_type: u16, name: &[u8], payload: &[u8]) {
        out.extend_from_slice(&record_type.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(payload);
    }

    fn artifact_digest(artifact: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&artifact[..96]);
        hasher.update(&[0u8; 32]);
        hasher.update(&artifact[128..]);
        hasher.finalize()
    }

    fn build_source_record(source_id: u16, label: &[u8], artifact: &[u8]) -> Vec<u8> {
        let source_header_len = 64usize;
        let artifact_offset = source_header_len + label.len();
        let mut out = vec![0u8; source_header_len];
        out[0..8].copy_from_slice(b"PYPKGS01");
        out[8..10].copy_from_slice(&source_id.to_le_bytes());
        out[10..12].copy_from_slice(&(label.len() as u16).to_le_bytes());
        out[16..24].copy_from_slice(&(artifact_offset as u64).to_le_bytes());
        out[24..32].copy_from_slice(&(artifact.len() as u64).to_le_bytes());
        out[32..64].copy_from_slice(&sha256(artifact));
        out.extend_from_slice(label);
        out.extend_from_slice(artifact);
        out
    }

    fn build_bundle(records: &[(u32, &[u8])]) -> Vec<u8> {
        let header_len = INIT_BUNDLE_HEADER_LEN as usize;
        let table_len = records.len() * RECORD_ENTRY_LEN;
        let mut bytes = vec![0u8; header_len + table_len];
        bytes[..init_bundle::INIT_BUNDLE_MAGIC.len()]
            .copy_from_slice(init_bundle::INIT_BUNDLE_MAGIC);
        bytes[16..18].copy_from_slice(&init_bundle::INIT_BUNDLE_MAJOR.to_le_bytes());
        bytes[18..20].copy_from_slice(&init_bundle::INIT_BUNDLE_MINOR.to_le_bytes());
        bytes[20..24].copy_from_slice(&INIT_BUNDLE_HEADER_LEN.to_le_bytes());
        bytes[24..26].copy_from_slice(&(records.len() as u16).to_le_bytes());

        let mut cursor = header_len + table_len;
        for (index, (record_type, payload)) in records.iter().enumerate() {
            let entry = header_len + index * RECORD_ENTRY_LEN;
            bytes[entry..entry + 4].copy_from_slice(&record_type.to_le_bytes());
            bytes[entry + 8..entry + 16].copy_from_slice(&(cursor as u64).to_le_bytes());
            bytes[entry + 16..entry + 24].copy_from_slice(&(payload.len() as u64).to_le_bytes());
            bytes[entry + 24..entry + 28]
                .copy_from_slice(&init_bundle::checksum(payload).to_le_bytes());
            bytes.extend_from_slice(payload);
            cursor += payload.len();
        }
        bytes
    }
}
