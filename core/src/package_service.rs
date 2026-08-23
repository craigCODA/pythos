use crate::{
    block_device::BlockDeviceInfo,
    capabilities::{CapabilityHandle, CapabilityTable, ResourceId, RightsMask},
    object_locator::LOCATOR_FIELD_SEGMENT,
    object_relationships::{
        ObjectRelationship, PACKAGE_LOCATOR_BINDING_BASE_OBJECT_ID, PACKAGE_LOCATOR_ROOT_OBJECT_ID,
        PackageLocatorRelationshipStore, RelationshipError, RelationshipKind,
    },
    object_service::{ObjectCreateResult, ObjectService, ObjectServiceError},
    object_service_checkpoint::{
        ObjectCheckpointIdentity, object_candidate_checkpoint_identity,
        read_object_service_candidate_checkpoint, write_object_service_candidate_checkpoint,
    },
    package_content_store::{
        ContentId, PackageContentCommit, PackageContentStore, PackageContentTransaction,
    },
    package_registry::{
        PACKAGE_TRANSACTION_COMMIT_V0_LEN, PackageRegistry, PackageRegistryGeneration,
        PackageRegistryPackageRecord, PackageRegistrySchemaRecord, PackageTransactionCommitV0,
    },
    package_source::PackageSourceService,
    process_context::ActiveUserProcess,
    shell_objects::{ObjectId, ObjectKind},
    typed_object_format::{ObjectFormatError, TypedObjectField, TypedObjectRecord},
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

impl PackageInstallResult {
    pub const fn empty() -> Self {
        Self {
            transaction_id: 0,
            package_object_id: 0,
            package_revision: 0,
            schema_object_id: 0,
            schema_revision: 0,
            release_digest: [0; 32],
            schema_descriptor_content_id: ContentId::new(0, [0; 32], 0),
            content_commit: PackageContentCommit::empty(),
            registry_generation: PackageRegistryGeneration {
                generation: 0,
                root_digest: [0; 32],
            },
            object_checkpoint_identity: ObjectCheckpointIdentity {
                generation: 0,
                root_digest: [0; 32],
            },
            anchor: PackageTransactionCommitV0 {
                transaction_id: 0,
                operation: 0,
                package_registry_generation: 0,
                package_registry_root_digest: [0; 32],
                object_checkpoint_generation: 0,
                object_checkpoint_root_digest: [0; 32],
                package_object_id: 0,
                package_installed_revision: 0,
                commit_crc32c: 0,
            },
            anchor_bytes: [0; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageRecoveryReport {
    pub published_world_selected: bool,
    pub previous_published_world_selected: bool,
    pub unpublished_candidate_ignored: bool,
    pub candidate_content_reclaimable: bool,
    pub locator_mirrors_require_rebuild: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageService<'a> {
    registry: PackageRegistry,
    content_store: PackageContentStore<'a>,
    staged_content: PackageContentTransaction<'a>,
    staged_registry: PackageRegistry,
    registry_snapshot: [u8; 4096],
    object_checkpoint_identity: ObjectCheckpointIdentity,
    anchor_bytes: [u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
    anchored_generation_available: bool,
    previous_registry: PackageRegistry,
    previous_registry_snapshot: [u8; 4096],
    previous_object_checkpoint_identity: ObjectCheckpointIdentity,
    previous_anchor_bytes: [u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
    previous_anchored_generation_available: bool,
    unpublished_candidate_checkpoint: Option<ObjectCheckpointIdentity>,
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
            staged_content: PackageContentTransaction::new(0, [0; 32]),
            staged_registry: PackageRegistry::empty(),
            registry_snapshot: [0; 4096],
            object_checkpoint_identity: ObjectCheckpointIdentity {
                generation: 0,
                root_digest: [0; 32],
            },
            anchor_bytes: [0; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
            anchored_generation_available: false,
            previous_registry: PackageRegistry::empty(),
            previous_registry_snapshot: [0; 4096],
            previous_object_checkpoint_identity: ObjectCheckpointIdentity {
                generation: 0,
                root_digest: [0; 32],
            },
            previous_anchor_bytes: [0; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
            previous_anchored_generation_available: false,
            unpublished_candidate_checkpoint: None,
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
        let mut result = PackageInstallResult::empty();
        self.install_into(
            request,
            source_service,
            capabilities,
            object_service,
            artifact_buffer,
            &mut result,
        )?;
        Ok(result)
    }

    pub fn install_with_candidate_checkpoint(
        &mut self,
        device: BlockDeviceInfo,
        request: PackageInstallRequest,
        source_service: &PackageSourceService<'_>,
        capabilities: &CapabilityTable,
        object_service: &mut ObjectService,
        artifact_buffer: &'a mut [u8],
    ) -> Result<PackageInstallResult, PackageStatus> {
        let mut result = PackageInstallResult::empty();
        self.install_into_with_candidate_checkpoint(
            Some(device),
            request,
            source_service,
            capabilities,
            object_service,
            artifact_buffer,
            &mut result,
        )?;
        Ok(result)
    }

    pub fn install_into(
        &mut self,
        request: PackageInstallRequest,
        source_service: &PackageSourceService<'_>,
        capabilities: &CapabilityTable,
        object_service: &mut ObjectService,
        artifact_buffer: &'a mut [u8],
        result: &mut PackageInstallResult,
    ) -> Result<(), PackageStatus> {
        self.install_into_with_candidate_checkpoint(
            None,
            request,
            source_service,
            capabilities,
            object_service,
            artifact_buffer,
            result,
        )
    }

    fn install_into_with_candidate_checkpoint(
        &mut self,
        candidate_device: Option<BlockDeviceInfo>,
        request: PackageInstallRequest,
        source_service: &PackageSourceService<'_>,
        capabilities: &CapabilityTable,
        object_service: &mut ObjectService,
        artifact_buffer: &'a mut [u8],
        result: &mut PackageInstallResult,
    ) -> Result<(), PackageStatus> {
        validate_install_authority(request.caller, capabilities, request.install_capability)?;
        let previous_registry = self.registry.clone();
        let previous_registry_snapshot = self.registry_snapshot;
        let previous_object_checkpoint_identity = self.object_checkpoint_identity;
        let previous_anchor_bytes = self.anchor_bytes;
        let previous_anchored_generation_available = self.anchored_generation_available;
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

        self.staged_content
            .reset(package_object_id.raw(), release_digest);
        let mut descriptor_content_id = None;
        let mut entry_index = 0u16;
        while let Some(entry) = artifact.content_entry(entry_index) {
            let content_bytes = artifact.content_bytes(entry).map_err(map_format_error)?;
            let content_id = self.content_store.stage_content(
                &mut self.staged_content,
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
            self.content_store.rollback(&mut self.staged_content);
            return Err(PackageStatus::InvalidSchema);
        }
        let Some(device) = candidate_device else {
            return Err(PackageStatus::RegistryWriteDenied);
        };

        let mut candidate_object_service = *object_service;
        let package = create_package_or_rollback(
            &mut candidate_object_service,
            request.caller,
            package_object_id,
            release_digest,
            &mut self.content_store,
            &mut self.staged_content,
        )?;
        let schema = create_schema_or_rollback(
            &mut candidate_object_service,
            request.caller,
            schema_object_id,
            package_object_id,
            descriptor_entry.sha256,
            &mut self.content_store,
            &mut self.staged_content,
        )?;

        self.staged_registry.copy_from_committed(&self.registry);
        self.staged_registry
            .add_package_record(PackageRegistryPackageRecord::new(
                package.object_id.raw(),
                package.revision,
                release_digest,
                PackageStatus::Ok as u16,
            ))?;
        self.staged_registry
            .add_schema_record(PackageRegistrySchemaRecord::new(
                schema.object_id.raw(),
                schema.revision,
                package.object_id.raw(),
                descriptor_entry.content_index,
                descriptor_entry.sha256,
            ))?;

        let mut staged_registry_snapshot = [0; 4096];
        let registry_generation = self
            .staged_registry
            .encode_snapshot(&mut staged_registry_snapshot)?;
        let candidate_snapshot = candidate_object_service
            .encode_candidate_snapshot()
            .map_err(map_object_error)?;
        let object_identity = self.persist_candidate_snapshot(device, &candidate_snapshot)?;
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
        let content_commit = self.content_store.commit(&mut self.staged_content)?;

        self.staged_registry
            .record_committed_generation(registry_generation);
        self.registry.copy_from_committed(&self.staged_registry);
        self.registry_snapshot = staged_registry_snapshot;
        self.previous_registry = previous_registry;
        self.previous_registry_snapshot = previous_registry_snapshot;
        self.previous_object_checkpoint_identity = previous_object_checkpoint_identity;
        self.previous_anchor_bytes = previous_anchor_bytes;
        self.previous_anchored_generation_available = previous_anchored_generation_available;
        self.object_checkpoint_identity = object_identity;
        self.anchor_bytes = anchor_bytes;
        self.anchored_generation_available = true;
        self.unpublished_candidate_checkpoint = None;
        self.next_transaction_id = self.next_transaction_id.wrapping_add(1);
        self.next_package_object_id = self.next_package_object_id.wrapping_add(1);
        self.next_schema_object_id = self.next_schema_object_id.wrapping_add(1);
        self.locator_mirror_visible = false;
        candidate_object_service.publish_candidate_generation(object_identity.generation);
        *object_service = candidate_object_service;

        result.transaction_id = transaction_id;
        result.package_object_id = package.object_id.raw();
        result.package_revision = package.revision;
        result.schema_object_id = schema.object_id.raw();
        result.schema_revision = schema.revision;
        result.release_digest = release_digest;
        result.schema_descriptor_content_id = schema_descriptor_content_id;
        result.content_commit = content_commit;
        result.registry_generation = registry_generation;
        result.object_checkpoint_identity = object_identity;
        result.anchor = anchor;
        result.anchor_bytes = anchor_bytes;
        Ok(())
    }

    pub const fn registry(&self) -> &PackageRegistry {
        &self.registry
    }

    pub const fn content_store(&self) -> &PackageContentStore<'a> {
        &self.content_store
    }

    pub fn write_validate_candidate_checkpoint(
        &mut self,
        device: BlockDeviceInfo,
        object_service: &ObjectService,
    ) -> Result<ObjectCheckpointIdentity, PackageStatus> {
        let snapshot = object_service
            .encode_candidate_snapshot()
            .map_err(map_object_error)?;
        self.persist_candidate_snapshot(device, &snapshot)
    }

    pub fn recover(&mut self) -> Result<PackageRecoveryReport, PackageStatus> {
        self.recover_inner(None)
    }

    pub fn recover_with_candidate_checkpoint(
        &mut self,
        device: BlockDeviceInfo,
    ) -> Result<PackageRecoveryReport, PackageStatus> {
        self.recover_inner(Some(device))
    }

    fn recover_inner(
        &mut self,
        candidate_device: Option<BlockDeviceInfo>,
    ) -> Result<PackageRecoveryReport, PackageStatus> {
        let current_valid = self.anchored_generation_available
            && PackageTransactionCommitV0::decode(
                &self.anchor_bytes,
                self.registry_generation(),
                self.object_checkpoint_identity,
            )
            .is_ok()
            && self.candidate_checkpoint_is_recoverable(
                candidate_device,
                self.object_checkpoint_identity,
            );
        let previous_valid = self.previous_anchored_generation_available
            && PackageTransactionCommitV0::decode(
                &self.previous_anchor_bytes,
                self.previous_registry_generation(),
                self.previous_object_checkpoint_identity,
            )
            .is_ok()
            && self.candidate_checkpoint_is_recoverable(
                candidate_device,
                self.previous_object_checkpoint_identity,
            );

        let current_snapshot = if current_valid {
            &self.registry_snapshot[..self.registry.encoded_len()]
        } else {
            &[]
        };
        let previous_snapshot = if previous_valid {
            &self.previous_registry_snapshot[..self.previous_registry.encoded_len()]
        } else {
            &[]
        };
        let selected = PackageRegistry::select_generation(current_snapshot, previous_snapshot)?;
        let selected_generation = PackageRegistryGeneration {
            generation: selected.generation(),
            root_digest: selected.root_digest(),
        };
        let selected_current = current_valid && selected_generation == self.registry_generation();
        let selected_previous =
            previous_valid && selected_generation == self.previous_registry_generation();
        let selected_object_identity = if selected_current {
            Some(self.object_checkpoint_identity)
        } else if selected_previous {
            Some(self.previous_object_checkpoint_identity)
        } else {
            None
        };
        if !selected_current && !selected_previous {
            return Err(PackageStatus::RegistryRecoveryDenied);
        }
        if selected_previous {
            self.replace_current_with_previous(selected);
        } else {
            self.registry = selected;
        }

        let transient_staged_content = self.staged_content.staged_count() != 0
            || self
                .content_store
                .staged_bitmap()
                .iter()
                .any(|word| *word != 0);
        if transient_staged_content {
            self.content_store.rollback(&mut self.staged_content);
        }
        let unpublished_candidate_ignored =
            self.unpublished_candidate_checkpoint
                .is_some_and(|candidate| {
                    Some(candidate) != selected_object_identity
                        && self.candidate_checkpoint_is_recoverable(candidate_device, candidate)
                });
        if unpublished_candidate_ignored {
            self.unpublished_candidate_checkpoint = None;
        }

        Ok(PackageRecoveryReport {
            published_world_selected: true,
            previous_published_world_selected: selected_previous,
            unpublished_candidate_ignored,
            candidate_content_reclaimable: unpublished_candidate_ignored,
            locator_mirrors_require_rebuild: self.registry.package_count() != 0
                && !self.locator_mirror_visible,
        })
    }

    fn candidate_checkpoint_is_recoverable(
        &self,
        candidate_device: Option<BlockDeviceInfo>,
        identity: ObjectCheckpointIdentity,
    ) -> bool {
        if identity.generation == 0 || identity.root_digest == [0; 32] {
            return false;
        }
        match candidate_device {
            Some(device) => read_object_service_candidate_checkpoint(device, identity).is_ok(),
            None => true,
        }
    }

    pub fn rebuild_locator_mirrors(
        &mut self,
    ) -> Result<PackageLocatorRelationshipStore, PackageStatus> {
        let mut mirrors = PackageLocatorRelationshipStore::new();
        self.rebuild_locator_mirrors_into(&mut mirrors)?;
        Ok(mirrors)
    }

    pub fn rebuild_locator_mirrors_into(
        &mut self,
        mirrors: &mut PackageLocatorRelationshipStore,
    ) -> Result<(), PackageStatus> {
        mirrors.clear();
        if self.registry.package_count() == 0 {
            self.locator_mirror_visible = false;
            return Ok(());
        }

        let root = ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID);
        mirrors
            .insert_object(TypedObjectRecord::new(
                root,
                ObjectKind::WorkspaceSession,
                1,
            ))
            .map_err(map_relationship_error)?;

        let mut index = 0usize;
        while index < self.registry.package_count() {
            if let Some(record) = self.registry.package_record(index)
                && record.status == PackageStatus::Ok as u16
            {
                let package = ObjectId::new(record.package_object_id);
                let binding = ObjectId::new(PACKAGE_LOCATOR_BINDING_BASE_OBJECT_ID + index as u64);
                let mut binding_record =
                    TypedObjectRecord::new(binding, ObjectKind::NameBinding, 1);
                let segment = package_locator_segment(record.package_object_id);
                binding_record
                    .push_field(
                        TypedObjectField::new(LOCATOR_FIELD_SEGMENT, 1, &segment)
                            .map_err(map_object_format_error)?,
                    )
                    .map_err(map_object_format_error)?;

                mirrors
                    .insert_object(binding_record)
                    .map_err(map_relationship_error)?;
                mirrors
                    .insert_object(TypedObjectRecord::new(package, ObjectKind::Package, 1))
                    .map_err(map_relationship_error)?;
                mirrors
                    .add_relationship(ObjectRelationship::new(
                        root,
                        RelationshipKind::NameBinding,
                        binding,
                    ))
                    .map_err(map_relationship_error)?;
                mirrors
                    .add_relationship(ObjectRelationship::new(
                        binding,
                        RelationshipKind::BindingTarget,
                        package,
                    ))
                    .map_err(map_relationship_error)?;
            }
            index += 1;
        }

        self.locator_mirror_visible = mirrors.relationship_count() != 0;
        Ok(())
    }

    pub const fn locator_mirror_visible_for_test(&self) -> bool {
        self.locator_mirror_visible
    }

    fn registry_generation(&self) -> PackageRegistryGeneration {
        PackageRegistryGeneration {
            generation: self.registry.generation(),
            root_digest: self.registry.root_digest(),
        }
    }

    fn previous_registry_generation(&self) -> PackageRegistryGeneration {
        PackageRegistryGeneration {
            generation: self.previous_registry.generation(),
            root_digest: self.previous_registry.root_digest(),
        }
    }

    fn persist_candidate_snapshot(
        &mut self,
        device: BlockDeviceInfo,
        snapshot: &crate::object_service_checkpoint::ObjectServiceSnapshot,
    ) -> Result<ObjectCheckpointIdentity, PackageStatus> {
        let candidate = write_object_service_candidate_checkpoint(device, snapshot)
            .map_err(map_storage_error)?;
        let loaded = read_object_service_candidate_checkpoint(device, candidate.identity)
            .map_err(map_storage_error)?;
        let loaded_identity = object_candidate_checkpoint_identity(&loaded);
        if loaded_identity != candidate.identity {
            return Err(PackageStatus::RegistryRecoveryDenied);
        }
        self.unpublished_candidate_checkpoint = Some(candidate.identity);
        Ok(candidate.identity)
    }

    fn replace_current_with_previous(&mut self, selected: PackageRegistry) {
        self.registry = selected;
        self.registry_snapshot = self.previous_registry_snapshot;
        self.object_checkpoint_identity = self.previous_object_checkpoint_identity;
        self.anchor_bytes = self.previous_anchor_bytes;
        self.anchored_generation_available = self.previous_anchored_generation_available;
        self.previous_registry = PackageRegistry::empty();
        self.previous_registry_snapshot = [0; 4096];
        self.previous_object_checkpoint_identity = ObjectCheckpointIdentity {
            generation: 0,
            root_digest: [0; 32],
        };
        self.previous_anchor_bytes = [0; PACKAGE_TRANSACTION_COMMIT_V0_LEN];
        self.previous_anchored_generation_available = false;
        self.locator_mirror_visible = false;
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

fn map_storage_error(
    _error: crate::general_storage_persistence::GeneralStoragePersistenceError,
) -> PackageStatus {
    PackageStatus::RegistryRecoveryDenied
}

fn map_relationship_error(_error: RelationshipError) -> PackageStatus {
    PackageStatus::RegistryRecoveryDenied
}

fn map_object_format_error(_error: ObjectFormatError) -> PackageStatus {
    PackageStatus::InvalidLocator
}

fn package_locator_segment(package_object_id: u64) -> [u8; 16] {
    let mut segment = [0u8; 16];
    let mut index = 0usize;
    while index < segment.len() {
        let shift = 60 - (index * 4);
        let nibble = ((package_object_id >> shift) & 0xF) as u8;
        segment[index] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + (nibble - 10)
        };
        index += 1;
    }
    segment
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{
        PackageInstallRequest, PackageInstallResult, PackageRecoveryReport, PackageService,
    };
    use crate::{
        block_device::BlockDeviceInfo,
        capabilities::{CapabilityTable, ResourceId, RightsMask},
        object_relationships::{PACKAGE_LOCATOR_ROOT_OBJECT_ID, RelationshipKind},
        object_service::ObjectService,
        object_service_checkpoint::{
            read_object_service_candidate_checkpoint, read_object_service_checkpoint,
            reset_checkpoint_storage_for_test,
        },
        package_content_store::{ContentId, PackageContentTransaction},
        package_registry::PackageTransactionCommitV0,
        package_source::PackageSourceService,
        process_context::ActiveUserProcess,
        service_identity::ServiceId,
        shell_objects::{ObjectId, ObjectKind},
    };
    use pythos_shared::{
        init_bundle::{self, INIT_BUNDLE_HEADER_LEN, RECORD_ENTRY_LEN, TYPE_PACKAGE_SOURCE},
        package_abi::{
            MAX_PACKAGE_ARTIFACT_BYTES, PACKAGE_CONTENT_MAX_STAGED_RECORDS,
            PACKAGE_INSTALL_RESOURCE_ID, PACKAGE_INSTALL_RIGHT, PACKAGE_SOURCE_READ_RIGHT,
            PACKAGE_SOURCE_RESOURCE_ID, PackageStatus,
        },
        package_format::{CONTENT_ENTRY_V0_LEN, PACKAGE_ARTIFACT_HEADER_LEN},
        sha256::{Sha256, sha256},
    };
    use std::vec::Vec;
    use std::{boxed::Box, sync::Mutex, vec};

    const CALLER_SERVICE: ServiceId = ServiceId::from_raw(0x5059_504B_494E_5301);
    const SCHEMA_DESCRIPTOR: &[u8] = b"schema:seed.v0";
    const ORPHAN_CONTENT: &[u8] = b"orphan";
    static CHECKPOINT_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn package_service_recovery_selects_clean_committed_generation() {
        with_installed_package(|package_service| {
            package_service.rebuild_locator_mirrors().unwrap();

            assert_eq!(
                package_service.recover(),
                Ok(PackageRecoveryReport {
                    published_world_selected: true,
                    previous_published_world_selected: false,
                    unpublished_candidate_ignored: false,
                    candidate_content_reclaimable: false,
                    locator_mirrors_require_rebuild: false,
                })
            );
        });
    }

    #[test]
    fn package_recovery_rolls_back_transient_staged_content_without_candidate_report() {
        with_installed_package(|package_service| {
            package_service
                .content_store
                .stage_content(
                    &mut package_service.staged_content,
                    1,
                    1,
                    ORPHAN_CONTENT,
                    sha256(ORPHAN_CONTENT),
                )
                .unwrap();

            let report = package_service.recover().unwrap();

            assert!(!report.unpublished_candidate_ignored);
            assert!(!report.candidate_content_reclaimable);
            assert_eq!(package_service.staged_content.staged_count(), 0);
            assert!(
                package_service
                    .content_store
                    .staged_bitmap()
                    .iter()
                    .all(|word| *word == 0)
            );
        });
    }

    #[test]
    fn package_service_recovery_selects_older_committed_registry_after_newest_anchor_mismatch() {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let first = install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();
        let first_generation = package_service.registry_generation();
        let second = install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();

        assert_eq!(package_service.registry().package_count(), 2);
        assert_eq!(package_service.previous_registry.package_count(), 1);
        assert_ne!(package_service.registry_generation(), first_generation);
        package_service.object_checkpoint_identity.root_digest[0] ^= 0xFF;

        let report = package_service
            .recover_with_candidate_checkpoint(device)
            .unwrap();

        assert!(report.published_world_selected);
        assert!(report.previous_published_world_selected);
        assert_eq!(package_service.registry().package_count(), 1);
        assert_eq!(
            package_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            first.package_object_id
        );
        assert_ne!(
            package_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            second.package_object_id
        );
        assert!(
            PackageTransactionCommitV0::decode(
                &package_service.anchor_bytes,
                package_service.registry_generation(),
                package_service.object_checkpoint_identity,
            )
            .is_ok()
        );
    }

    #[test]
    fn package_service_recovery_normalizes_fallback_pair_before_a_later_install() {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let first = install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();
        install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();
        package_service.object_checkpoint_identity.root_digest[0] ^= 0xFF;
        package_service
            .recover_with_candidate_checkpoint(device)
            .unwrap();
        let recovered_snapshot = read_object_service_candidate_checkpoint(
            device,
            package_service.object_checkpoint_identity,
        )
        .unwrap();
        object_service = ObjectService::decode_snapshot_for_test(recovered_snapshot).unwrap();

        install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();
        package_service.object_checkpoint_identity.root_digest[0] ^= 0xFF;

        let report = package_service
            .recover_with_candidate_checkpoint(device)
            .unwrap();

        assert!(report.previous_published_world_selected);
        assert_eq!(package_service.registry().package_count(), 1);
        assert_eq!(
            package_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            first.package_object_id
        );
    }

    #[test]
    fn package_service_recovery_keeps_committed_snapshot_after_content_commit_failure() {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();
        let committed_snapshot = package_service.registry_snapshot;

        for filler in 0..(PACKAGE_CONTENT_MAX_STAGED_RECORDS - 1) {
            let mut transaction = PackageContentTransaction::new(
                0x5059_504B_4649_4C4C + filler as u64,
                [filler as u8; 32],
            );
            package_service
                .content_store
                .stage_content(
                    &mut transaction,
                    1,
                    1,
                    ORPHAN_CONTENT,
                    sha256(ORPHAN_CONTENT),
                )
                .unwrap();
            package_service
                .content_store
                .commit(&mut transaction)
                .unwrap();
        }

        assert_eq!(
            install_recovery_package_with_candidate_checkpoint_into(
                &mut package_service,
                device,
                &mut object_service,
            ),
            Err(PackageStatus::QuotaDenied)
        );
        assert_eq!(package_service.registry_snapshot, committed_snapshot);
        assert!(
            package_service
                .recover_with_candidate_checkpoint(device)
                .is_ok()
        );
    }

    #[test]
    fn package_service_recovery_reports_missing_locator_mirrors_without_publishing_them() {
        with_installed_package(|package_service| {
            assert!(!package_service.locator_mirror_visible_for_test());

            let report = package_service.recover().unwrap();

            assert!(report.locator_mirrors_require_rebuild);
            assert!(!package_service.locator_mirror_visible_for_test());
        });
    }

    #[test]
    fn package_service_recovery_denies_without_valid_anchor_and_leaves_object_recovery_unchanged() {
        let mut package_service = PackageService::new_empty_for_test();
        let object_service = ObjectService::new_for_test();
        let before = object_service.checkpoint_identity().unwrap();

        assert_eq!(
            package_service.recover(),
            Err(PackageStatus::RegistryRecoveryDenied)
        );
        assert_eq!(object_service.checkpoint_identity().unwrap(), before);
    }

    #[test]
    fn package_recovery_selects_anchor_published_candidate_world() {
        with_installed_package(|package_service| {
            let report = package_service.recover().unwrap();

            assert!(report.published_world_selected);
            assert!(!report.previous_published_world_selected);
            assert!(!report.unpublished_candidate_ignored);
            assert!(!report.candidate_content_reclaimable);
            assert_eq!(package_service.registry().package_count(), 1);
        });
    }

    #[test]
    fn package_service_candidate_checkpoint_anchor_references_persisted_candidate_identity() {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();

        let installed =
            install_recovery_package_with_candidate_checkpoint(&mut package_service, device)
                .unwrap();

        assert!(read_object_service_checkpoint(device).unwrap().is_none());
        assert_eq!(
            PackageTransactionCommitV0::decode(
                &installed.anchor_bytes,
                installed.registry_generation,
                installed.object_checkpoint_identity,
            )
            .unwrap(),
            installed.anchor
        );

        let candidate =
            read_object_service_candidate_checkpoint(device, installed.object_checkpoint_identity)
                .unwrap();
        assert_eq!(
            candidate.generation,
            installed.object_checkpoint_identity.generation
        );
        assert!(candidate.objects.iter().flatten().any(|record| {
            record.object.object_id() == ObjectId::new(installed.package_object_id)
        }));
    }

    #[test]
    fn package_recovery_reports_durable_unanchored_candidate_as_ignored_reclaimable() {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();

        let unanchored = package_service
            .write_validate_candidate_checkpoint(device, &object_service)
            .unwrap();

        assert!(read_object_service_candidate_checkpoint(device, unanchored).is_ok());
        assert!(read_object_service_checkpoint(device).unwrap().is_none());

        let report = package_service
            .recover_with_candidate_checkpoint(device)
            .unwrap();

        assert!(report.published_world_selected);
        assert!(report.unpublished_candidate_ignored);
        assert!(report.candidate_content_reclaimable);
        assert!(!report.previous_published_world_selected);
    }

    #[test]
    fn package_recovery_rejects_mismatched_anchor_for_durable_candidate_world() {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let first = install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();
        let second = install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();
        assert_eq!(package_service.registry().package_count(), 2);
        assert!(
            read_object_service_candidate_checkpoint(device, second.object_checkpoint_identity,)
                .is_ok()
        );
        package_service.object_checkpoint_identity.root_digest[0] ^= 0xFF;

        let report = package_service
            .recover_with_candidate_checkpoint(device)
            .unwrap();

        assert!(report.published_world_selected);
        assert!(report.previous_published_world_selected);
        assert_eq!(package_service.registry().package_count(), 1);
        assert_eq!(
            package_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            first.package_object_id
        );
    }

    #[test]
    fn package_install_without_candidate_checkpoint_device_denies_without_publication() {
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let before = object_service.checkpoint_identity().unwrap();

        assert_eq!(
            install_recovery_package_into(&mut package_service, &mut object_service),
            Err(PackageStatus::RegistryWriteDenied)
        );

        assert_eq!(package_service.registry().package_count(), 0);
        assert_eq!(object_service.checkpoint_identity().unwrap(), before);
    }

    #[test]
    fn package_install_with_candidate_checkpoint_advances_live_object_generation() {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();

        let first = install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();
        let second = install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();

        assert_ne!(
            first.object_checkpoint_identity,
            second.object_checkpoint_identity
        );
        assert_eq!(first.object_checkpoint_identity.generation, 1);
        assert_eq!(second.object_checkpoint_identity.generation, 2);
        assert!(
            read_object_service_candidate_checkpoint(device, first.object_checkpoint_identity)
                .is_ok()
        );
        assert!(
            read_object_service_candidate_checkpoint(device, second.object_checkpoint_identity)
                .is_ok()
        );
    }

    #[test]
    fn package_recovery_validates_candidate_checkpoint_storage_before_selection() {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();

        reset_checkpoint_storage_for_test();

        assert_eq!(
            package_service.recover_with_candidate_checkpoint(device),
            Err(PackageStatus::RegistryRecoveryDenied)
        );
    }

    #[test]
    fn package_recovery_ignores_unanchored_candidate_without_overwriting_anchor() {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let installed = install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();

        let unanchored = package_service
            .write_validate_candidate_checkpoint(device, &object_service)
            .unwrap();

        assert_ne!(installed.object_checkpoint_identity, unanchored);
        assert!(
            read_object_service_candidate_checkpoint(device, installed.object_checkpoint_identity)
                .is_ok()
        );
        assert!(read_object_service_candidate_checkpoint(device, unanchored).is_ok());

        let report = package_service
            .recover_with_candidate_checkpoint(device)
            .unwrap();

        assert!(report.published_world_selected);
        assert!(report.unpublished_candidate_ignored);
        assert!(report.candidate_content_reclaimable);
    }

    fn with_installed_package(f: impl FnOnce(&mut PackageService<'_>)) {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        install_recovery_package_with_candidate_checkpoint(&mut package_service, device).unwrap();
        f(&mut package_service);
    }

    fn install_recovery_package_into(
        package_service: &mut PackageService<'static>,
        object_service: &mut ObjectService,
    ) -> Result<PackageInstallResult, PackageStatus> {
        let artifact = build_schema_package_artifact();
        let source_record = build_source_record(0, b"phase13-recovery.pkg", &artifact);
        let bundle_bytes = build_bundle(&[(TYPE_PACKAGE_SOURCE, &source_record)]);
        let init_bundle = init_bundle::validate(&bundle_bytes).unwrap();
        let source_service = PackageSourceService::from_init_bundle(&init_bundle).unwrap();
        let source_handle = source_service.handle_at(0).unwrap();
        let caller = ActiveUserProcess::new(CALLER_SERVICE, 0x504B_5245_4356, 0x13);
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
        let artifact_buffer = Box::leak(vec![0u8; MAX_PACKAGE_ARTIFACT_BYTES].into_boxed_slice());

        package_service.install(
            PackageInstallRequest {
                caller,
                source_handle,
                source_read_capability,
                install_capability,
            },
            &source_service,
            &capabilities,
            object_service,
            artifact_buffer,
        )
    }

    fn install_recovery_package_with_candidate_checkpoint(
        package_service: &mut PackageService<'static>,
        device: BlockDeviceInfo,
    ) -> Result<PackageInstallResult, PackageStatus> {
        let mut object_service = ObjectService::new_for_test();
        install_recovery_package_with_candidate_checkpoint_into(
            package_service,
            device,
            &mut object_service,
        )
    }

    fn install_recovery_package_with_candidate_checkpoint_into(
        package_service: &mut PackageService<'static>,
        device: BlockDeviceInfo,
        object_service: &mut ObjectService,
    ) -> Result<PackageInstallResult, PackageStatus> {
        let artifact = build_schema_package_artifact();
        let source_record = build_source_record(0, b"phase13-recovery.pkg", &artifact);
        let bundle_bytes = build_bundle(&[(TYPE_PACKAGE_SOURCE, &source_record)]);
        let init_bundle = init_bundle::validate(&bundle_bytes).unwrap();
        let source_service = PackageSourceService::from_init_bundle(&init_bundle).unwrap();
        let source_handle = source_service.handle_at(0).unwrap();
        let caller = ActiveUserProcess::new(CALLER_SERVICE, 0x504B_5245_4356, 0x13);
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
        let artifact_buffer = Box::leak(vec![0u8; MAX_PACKAGE_ARTIFACT_BYTES].into_boxed_slice());

        package_service.install_with_candidate_checkpoint(
            device,
            PackageInstallRequest {
                caller,
                source_handle,
                source_read_capability,
                install_capability,
            },
            &source_service,
            &capabilities,
            object_service,
            artifact_buffer,
        )
    }

    #[test]
    fn package_install_transaction_commits_package_schema_content_registry_and_anchor_as_one_unit()
    {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
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
            .install_with_candidate_checkpoint(
                device,
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

    #[test]
    fn package_locator_mirrors_are_rebuilt_from_committed_registry_generation() {
        let _guard = CHECKPOINT_TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let artifact = build_schema_package_artifact();
        let source_record = build_source_record(0, b"phase13-mirror.pkg", &artifact);
        let bundle_bytes = build_bundle(&[(TYPE_PACKAGE_SOURCE, &source_record)]);
        let init_bundle = init_bundle::validate(&bundle_bytes).unwrap();
        let source_service = PackageSourceService::from_init_bundle(&init_bundle).unwrap();
        let source_handle = source_service.handle_at(0).unwrap();
        let caller = ActiveUserProcess::new(CALLER_SERVICE, 0x504B_4D49_5252, 0x13);
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
            .install_with_candidate_checkpoint(
                device,
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

        let mirrors = package_service.rebuild_locator_mirrors().unwrap();
        let relationships = mirrors.relationship_records();
        let root = crate::shell_objects::ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID);
        let package = crate::shell_objects::ObjectId::new(installed.package_object_id);
        let mut binding = None;
        for relationship in relationships.into_iter().flatten() {
            if relationship.source() == root && relationship.kind() == RelationshipKind::NameBinding
            {
                binding = Some(relationship.target());
            }
        }
        let binding = binding.unwrap();

        assert_eq!(mirrors.relationship_count(), 2);
        assert!(package_service.locator_mirror_visible_for_test());
        assert!(mirrors.has_object(root));
        assert!(mirrors.has_object(package));
        assert!(relationships.into_iter().flatten().any(|relationship| {
            relationship.source() == binding
                && relationship.kind() == RelationshipKind::BindingTarget
                && relationship.target() == package
        }));

        let mut empty_service = PackageService::new_empty_for_test();
        let empty_mirrors = empty_service.rebuild_locator_mirrors().unwrap();
        assert_eq!(empty_mirrors.relationship_count(), 0);
        assert!(!empty_service.locator_mirror_visible_for_test());
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
