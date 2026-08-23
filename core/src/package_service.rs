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
        ObjectCheckpointIdentity, ObjectServiceSnapshot, object_candidate_checkpoint_identity,
        read_object_service_candidate_checkpoint, read_object_service_candidate_checkpoint_into,
        write_object_service_candidate_checkpoint,
    },
    package_candidate_store::{
        PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES, PackagePublicationAnchorSlot,
        read_candidate_registry_generation, read_candidate_registry_generation_into,
        read_publication_anchor_slot, write_candidate_registry_generation,
        write_publication_anchor,
    },
    package_content_store::{
        ContentId, PackageContentCommit, PackageContentStore, PackageContentTransaction,
    },
    package_registry::{
        PACKAGE_TRANSACTION_COMMIT_V0_LEN, PackageRegistry, PackageRegistryExportRecord,
        PackageRegistryGeneration, PackageRegistryPackageRecord, PackageRegistrySchemaRecord,
        PackageTransactionCommitV0,
    },
    package_source::PackageSourceService,
    process_context::ActiveUserProcess,
    pyth_runtime_launch::PackageLaunchGraphImportGrant,
    service_identity::ServiceId,
    shell_objects::{ObjectId, ObjectKind},
    typed_object_format::{ObjectFormatError, TypedObjectField, TypedObjectRecord},
};
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};
use pythos_shared::{
    object_shell_abi::PackedCapability,
    package_abi::{
        MAX_REQUIREMENT_RECORDS, OBJECT_KIND_PACKAGE, OBJECT_KIND_SCHEMA_DEFINITION,
        PACKAGE_INSTALL_RESOURCE_ID, PACKAGE_INSTALL_RIGHT, PackageRuntimeSchemaBindingV0,
        PackageStatus,
    },
    package_format::{PackageArtifactV0, PackageFormatError},
};

#[cfg_attr(not(test), allow(unused_imports))]
pub use crate::pyth_graph_loader::validate_package_export_graph;

const FIRST_PACKAGE_OBJECT_ID: u64 = 0x5059_504B_474F_0001;
const FIRST_SCHEMA_OBJECT_ID: u64 = 0x5059_5343_484F_0001;
const INSTALL_OPERATION: u16 = 1;

#[cfg(test)]
static TEST_CANDIDATE_SNAPSHOT_WRITE_OVERRIDE: std::sync::Mutex<Option<ObjectServiceSnapshot>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
static TEST_CANDIDATE_CONTENT_MATERIALIZATION_FAILURE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

struct RetainedPackageServiceStorage(UnsafeCell<MaybeUninit<PackageService<'static>>>);

// SAFETY:
// 1. Invariant: retained package service storage is initialized at most once
//    before package-context syscalls read launch contexts.
// 2. Established by: initialize_retained_package_service_for_phase13 flips the
//    initialized flag with compare_exchange before writing the static slot.
// 3. Lifetime: the retained service lives in static storage for the boot or
//    until the test-only reset drops it.
// 4. Pointer ownership: with_retained_package_service_for_phase13 grants one
//    mutable borrow for one synchronous closure and stores no reference.
// 5. Alignment: MaybeUninit<PackageService> provides PackageService alignment.
// 6. Mapped length: exactly one PackageService value is accessed.
// 7. Concurrency: Phase 13 production and acceptance package dispatch are
//    single-core and non-reentrant in this slice.
// 8. Violation: overlapping borrows could expose package context for the wrong
//    launched process or corrupt package service state.
unsafe impl Sync for RetainedPackageServiceStorage {}

static RETAINED_PACKAGE_SERVICE: RetainedPackageServiceStorage =
    RetainedPackageServiceStorage(UnsafeCell::new(MaybeUninit::uninit()));

static RETAINED_PACKAGE_SERVICE_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
fn set_candidate_snapshot_write_override_for_test(snapshot: ObjectServiceSnapshot) {
    *TEST_CANDIDATE_SNAPSHOT_WRITE_OVERRIDE
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(snapshot);
}

#[cfg(test)]
fn fail_candidate_content_materialization_for_test() {
    TEST_CANDIDATE_CONTENT_MATERIALIZATION_FAILURE.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub(crate) fn initialize_retained_package_service_for_phase13(
    service: PackageService<'static>,
) -> bool {
    if RETAINED_PACKAGE_SERVICE_INITIALIZED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return false;
    }
    // SAFETY:
    // 1. Invariant: this is the only initialization write to the retained
    //    package service slot.
    // 2. Established by: compare_exchange above transitions false -> true for
    //    exactly one caller before this write.
    // 3. Lifetime: the moved PackageService lives in static storage until boot
    //    exit or test-only reset.
    // 4. Pointer ownership: RETAINED_PACKAGE_SERVICE owns the storage; the
    //    input service is moved and not used again by the caller.
    // 5. Alignment: MaybeUninit<PackageService> preserves PackageService
    //    alignment.
    // 6. Mapped length: exactly one PackageService value is written.
    // 7. Concurrency: Phase 13 production and acceptance initialization are
    //    single-core and non-reentrant.
    // 8. Violation: a second writer could replace launch contexts under an
    //    active package-context syscall.
    unsafe {
        (*RETAINED_PACKAGE_SERVICE.0.get()).write(service);
    }
    true
}

pub(crate) fn with_retained_package_service_for_phase13<R>(
    f: impl FnOnce(&mut PackageService<'static>) -> R,
) -> Option<R> {
    if !RETAINED_PACKAGE_SERVICE_INITIALIZED.load(Ordering::SeqCst) {
        return None;
    }
    // SAFETY:
    // 1. Invariant: the retained service slot contains one initialized
    //    PackageService while the initialized flag is true.
    // 2. Established by: initialize_retained_package_service_for_phase13 is
    //    non-reentrant in this single-core slice and writes the slot before
    //    returning to any caller that can issue package-context syscalls.
    // 3. Lifetime: the mutable borrow is limited to this synchronous closure.
    // 4. Pointer ownership: this module owns the static slot and lends one
    //    mutable reference without retaining it.
    // 5. Alignment: MaybeUninit<PackageService> provides PackageService
    //    alignment.
    // 6. Mapped length: exactly one initialized PackageService is borrowed.
    // 7. Concurrency: Phase 13 production and acceptance package dispatch are
    //    single-core and non-reentrant.
    // 8. Violation: reentrant mutable access could corrupt runtime context
    //    records or registry state.
    Some(f(unsafe {
        (*RETAINED_PACKAGE_SERVICE.0.get()).assume_init_mut()
    }))
}

#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
pub(crate) fn initialize_package_service_from_device(
    device: BlockDeviceInfo,
) -> Result<(), PackageStatus> {
    let mut service = PackageService::new_empty();
    service.restore_from_storage(device)?;
    if initialize_retained_package_service_for_phase13(service) {
        Ok(())
    } else {
        Err(PackageStatus::BadRequest)
    }
}

#[cfg(test)]
fn reset_retained_package_service_for_phase13_test() {
    if RETAINED_PACKAGE_SERVICE_INITIALIZED.swap(false, Ordering::SeqCst) {
        // SAFETY:
        // 1. Invariant: the initialized flag was true, so the slot contains
        //    one initialized PackageService.
        // 2. Established by: initialize_retained_package_service_for_phase13 is
        //    the only writer and sets the flag before tests can use the slot.
        // 3. Lifetime: this test-only reset ends the stored service lifetime.
        // 4. Pointer ownership: tests hold the retained-service test lock and
        //    no service borrow while resetting.
        // 5. Alignment: MaybeUninit<PackageService> preserves alignment.
        // 6. Mapped length: exactly one PackageService is dropped.
        // 7. Concurrency: tests serialize retained package service use with
        //    RETAINED_PACKAGE_SERVICE_TEST_LOCK.
        // 8. Violation: resetting while borrowed would invalidate a live
        //    mutable reference.
        unsafe {
            (*RETAINED_PACKAGE_SERVICE.0.get()).assume_init_drop();
        }
    }
}

#[cfg(test)]
pub(crate) struct RetainedPackageServicePhase13TestGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for RetainedPackageServicePhase13TestGuard {
    fn drop(&mut self) {
        reset_retained_package_service_for_phase13_test();
    }
}

#[cfg(test)]
static RETAINED_PACKAGE_SERVICE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn initialize_retained_package_service_for_phase13_test(
    service: PackageService<'static>,
) -> RetainedPackageServicePhase13TestGuard {
    let lock = RETAINED_PACKAGE_SERVICE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    reset_retained_package_service_for_phase13_test();
    assert!(initialize_retained_package_service_for_phase13(service));
    RetainedPackageServicePhase13TestGuard { _lock: lock }
}

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
pub struct PackageInstallCandidate {
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
    pub staged_registry_snapshot: [u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageLaunchRequirement {
    pub requirement_id: u16,
    pub graph_import_slot: u16,
    pub resource: ResourceId,
    pub rights: RightsMask,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageLaunchGrant {
    pub requirement_id: u16,
    pub capability: CapabilityHandle,
}

impl PackageLaunchGrant {
    pub const fn packed_capability(self) -> PackedCapability {
        PackedCapability::from_parts(self.capability.slot(), self.capability.generation())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageLaunchRequest<'a> {
    pub caller: ActiveUserProcess,
    pub namespace_root: ObjectId,
    pub locator: &'a str,
    pub supplied_grants: &'a [PackageLaunchGrant],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PackageLaunchRequirementRecord {
    export: PackageRegistryExportRecord,
    requirement: PackageLaunchRequirement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PackageLaunchResolvedGrant {
    supplied: PackageLaunchGrant,
    graph_import_slot: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PackageRuntimeContextRecord {
    service_id: ServiceId,
    principal_id: u64,
    program_digest: u64,
    export: PackageRegistryExportRecord,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageLaunchResult {
    pub export: PackageRegistryExportRecord,
    grants: [Option<PackageLaunchResolvedGrant>; MAX_REQUIREMENT_RECORDS],
    pub grant_count: usize,
}

impl PackageLaunchResult {
    const fn new(export: PackageRegistryExportRecord) -> Self {
        Self {
            export,
            grants: [None; MAX_REQUIREMENT_RECORDS],
            grant_count: 0,
        }
    }

    pub const fn grant(&self, index: usize) -> Option<PackageLaunchGrant> {
        if index >= self.grant_count {
            return None;
        }
        match self.grants[index] {
            Some(grant) => Some(grant.supplied),
            None => None,
        }
    }

    pub const fn graph_import_grant(&self, index: usize) -> Option<PackageLaunchGraphImportGrant> {
        if index >= self.grant_count {
            return None;
        }
        match self.grants[index] {
            Some(grant) => Some(PackageLaunchGraphImportGrant {
                import_slot: grant.graph_import_slot,
                capability: grant.supplied.packed_capability(),
            }),
            None => None,
        }
    }
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
pub struct PackageRecoveryReport<'snapshot> {
    pub published_world_selected: bool,
    pub previous_published_world_selected: bool,
    pub unpublished_candidate_ignored: bool,
    pub candidate_content_reclaimable: bool,
    pub locator_mirrors_require_rebuild: bool,
    selected_object_checkpoint_identity: Option<ObjectCheckpointIdentity>,
    selected_object_snapshot: Option<&'snapshot ObjectServiceSnapshot>,
}

impl PackageRecoveryReport<'_> {
    pub const fn selected_object_checkpoint(
        &self,
    ) -> Option<(ObjectCheckpointIdentity, &ObjectServiceSnapshot)> {
        match (
            self.selected_object_checkpoint_identity,
            self.selected_object_snapshot,
        ) {
            (Some(identity), Some(snapshot)) => Some((identity, snapshot)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageService<'a> {
    registry: PackageRegistry,
    content_store: PackageContentStore<'a>,
    staged_content: PackageContentTransaction<'a>,
    staged_registry: PackageRegistry,
    registry_snapshot: [u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES],
    object_checkpoint_identity: ObjectCheckpointIdentity,
    anchor_bytes: [u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
    anchored_generation_available: bool,
    previous_registry: PackageRegistry,
    previous_registry_snapshot: [u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES],
    previous_object_checkpoint_identity: ObjectCheckpointIdentity,
    previous_anchor_bytes: [u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
    previous_anchored_generation_available: bool,
    unpublished_candidate_checkpoint: Option<ObjectCheckpointIdentity>,
    prepared_candidate: Option<PackageInstallCandidate>,
    prepared_object_service: Option<ObjectService>,
    prepared_candidate_device: Option<BlockDeviceInfo>,
    prepared_live_object_checkpoint_identity: Option<ObjectCheckpointIdentity>,
    prepared_content_store: Option<PackageContentStore<'a>>,
    prepared_content: Option<PackageContentTransaction<'a>>,
    restored_object_snapshot: Option<ObjectServiceSnapshot>,
    next_transaction_id: u64,
    next_package_object_id: u64,
    next_schema_object_id: u64,
    locator_mirror_visible: bool,
    launch_requirements: [Option<PackageLaunchRequirementRecord>; MAX_REQUIREMENT_RECORDS],
    launch_requirement_count: usize,
    runtime_contexts: [Option<PackageRuntimeContextRecord>; MAX_REQUIREMENT_RECORDS],
    runtime_context_count: usize,
}

struct PackageRestoreWorld {
    registry: PackageRegistry,
    content_store: PackageContentStore<'static>,
    object_snapshot: ObjectServiceSnapshot,
}

impl PackageRestoreWorld {
    const fn empty() -> Self {
        Self {
            registry: PackageRegistry::empty(),
            content_store: PackageContentStore::empty(),
            object_snapshot: ObjectServiceSnapshot::empty(),
        }
    }

    fn copy_from(&mut self, source: &Self) {
        self.registry.copy_from_committed(&source.registry);
        self.content_store
            .copy_restored_state_from(&source.content_store);
        copy_object_snapshot(&mut self.object_snapshot, &source.object_snapshot);
    }
}

struct PackageRestoreScratch {
    selected_anchor: Option<PackageTransactionCommitV0>,
    selected: PackageRestoreWorld,
    candidate: PackageRestoreWorld,
    registry_snapshot: [u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES],
    anchor_bytes: [u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
}

impl PackageRestoreScratch {
    const fn empty() -> Self {
        Self {
            selected_anchor: None,
            selected: PackageRestoreWorld::empty(),
            candidate: PackageRestoreWorld::empty(),
            registry_snapshot: [0; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES],
            anchor_bytes: [0; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
        }
    }
}

#[cfg(not(test))]
struct PackageRestoreScratchStorage(UnsafeCell<PackageRestoreScratch>);

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: publication hydration borrows this workspace synchronously.
// 2. Established by: Phase 13 restore runs on one boot CPU and is non-reentrant.
// 3. Lifetime: the workspace is retained and mapped for the full boot.
// 4. Pointer ownership: this module creates the only mutable workspace borrow.
// 5. Alignment: `UnsafeCell<PackageRestoreScratch>` preserves alignment.
// 6. Mapped length: exactly one complete hydration workspace is accessed.
// 7. Concurrency: no SMP or concurrent package restore exists in this phase.
// 8. Violation: reentry could combine state from different publication worlds.
unsafe impl Sync for PackageRestoreScratchStorage {}

#[cfg(not(test))]
static PACKAGE_RESTORE_SCRATCH: PackageRestoreScratchStorage =
    PackageRestoreScratchStorage(UnsafeCell::new(PackageRestoreScratch::empty()));

#[cfg(not(test))]
fn with_package_restore_scratch<R>(f: impl FnOnce(&mut PackageRestoreScratch) -> R) -> R {
    // SAFETY:
    // 1. Invariant: the closure does not retain the mutable workspace reference.
    // 2. Established by: this helper accepts one synchronous `FnOnce`.
    // 3. Lifetime: the static workspace remains initialized for the whole boot.
    // 4. Pointer ownership: this module creates the only mutable reference.
    // 5. Alignment: `UnsafeCell` preserves the workspace alignment.
    // 6. Mapped length: exactly one complete workspace is borrowed.
    // 7. Concurrency: package restore is single-core and non-reentrant.
    // 8. Violation: overlapping borrows could select a mixed publication world.
    unsafe { f(&mut *PACKAGE_RESTORE_SCRATCH.0.get()) }
}

#[cfg(test)]
static PACKAGE_RESTORE_TEST_SCRATCH: std::sync::Mutex<PackageRestoreScratch> =
    std::sync::Mutex::new(PackageRestoreScratch::empty());

#[cfg(test)]
fn with_package_restore_scratch<R>(f: impl FnOnce(&mut PackageRestoreScratch) -> R) -> R {
    let mut scratch = PACKAGE_RESTORE_TEST_SCRATCH
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    f(&mut scratch)
}

impl<'a> PackageService<'a> {
    pub const fn new_empty() -> Self {
        Self {
            registry: PackageRegistry::empty(),
            content_store: PackageContentStore::empty(),
            staged_content: PackageContentTransaction::new(0, [0; 32]),
            staged_registry: PackageRegistry::empty(),
            registry_snapshot: [0; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES],
            object_checkpoint_identity: ObjectCheckpointIdentity {
                generation: 0,
                root_digest: [0; 32],
            },
            anchor_bytes: [0; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
            anchored_generation_available: false,
            previous_registry: PackageRegistry::empty(),
            previous_registry_snapshot: [0; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES],
            previous_object_checkpoint_identity: ObjectCheckpointIdentity {
                generation: 0,
                root_digest: [0; 32],
            },
            previous_anchor_bytes: [0; PACKAGE_TRANSACTION_COMMIT_V0_LEN],
            previous_anchored_generation_available: false,
            unpublished_candidate_checkpoint: None,
            prepared_candidate: None,
            prepared_object_service: None,
            prepared_candidate_device: None,
            prepared_live_object_checkpoint_identity: None,
            prepared_content_store: None,
            prepared_content: None,
            restored_object_snapshot: None,
            next_transaction_id: 1,
            next_package_object_id: FIRST_PACKAGE_OBJECT_ID,
            next_schema_object_id: FIRST_SCHEMA_OBJECT_ID,
            locator_mirror_visible: false,
            launch_requirements: [None; MAX_REQUIREMENT_RECORDS],
            launch_requirement_count: 0,
            runtime_contexts: [None; MAX_REQUIREMENT_RECORDS],
            runtime_context_count: 0,
        }
    }

    pub const fn new_empty_for_test() -> Self {
        Self::new_empty()
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
        let candidate = self.prepare_install_candidate(
            device,
            request,
            source_service,
            capabilities,
            object_service,
            artifact_buffer,
        )?;
        self.publish_install_candidate(candidate, object_service)
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
        *result = self.install_compatibility(
            request,
            source_service,
            capabilities,
            object_service,
            artifact_buffer,
        )?;
        Ok(())
    }

    fn install_compatibility(
        &mut self,
        request: PackageInstallRequest,
        source_service: &PackageSourceService<'_>,
        capabilities: &CapabilityTable,
        object_service: &mut ObjectService,
        artifact_buffer: &'a mut [u8],
    ) -> Result<PackageInstallResult, PackageStatus> {
        if self.prepared_candidate.is_some() {
            return Err(PackageStatus::BadRequest);
        }
        validate_install_authority(request.caller, capabilities, request.install_capability)?;
        let artifact_len = source_service.read(
            request.caller.service_id(),
            capabilities,
            request.source_handle,
            request.source_read_capability,
            artifact_buffer,
        )?;
        let artifact =
            PackageArtifactV0::parse(&artifact_buffer[..artifact_len]).map_err(map_format_error)?;
        let descriptor_entry = artifact
            .content_entry(0)
            .ok_or(PackageStatus::InvalidSchema)?;
        let descriptor_bytes = artifact
            .content_bytes(descriptor_entry)
            .map_err(map_format_error)?;
        let release_digest = artifact.artifact_sha256();
        let package_object_id = ObjectId::new(self.next_package_object_id);
        let schema_object_id = ObjectId::new(self.next_schema_object_id);

        self.content_store.rollback(&mut self.staged_content);
        self.staged_content
            .reset(package_object_id.raw(), release_digest);
        let installed = (|| -> Result<PackageInstallResult, PackageStatus> {
            let mut descriptor_content_id = None;
            let mut entry_index = 0u16;
            while let Some(entry) = artifact.content_entry(entry_index) {
                let content_id = self.content_store.stage_content(
                    &mut self.staged_content,
                    entry.role,
                    entry.format,
                    artifact.content_bytes(entry).map_err(map_format_error)?,
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
                return Err(PackageStatus::InvalidSchema);
            }

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
                .begin_candidate_generation(self.next_transaction_id)?;
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
            self.content_store
                .add_staged_records_to_registry(&self.staged_content, &mut self.staged_registry)?;

            self.content_store
                .validate_commit_capacity(&self.staged_content)?;
            let mut registry_snapshot = [0; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES];
            let registry_generation = self
                .staged_registry
                .encode_snapshot(&mut registry_snapshot)?;
            let candidate_snapshot = candidate_object_service
                .encode_candidate_snapshot()
                .map_err(map_object_error)?;
            let object_checkpoint_identity =
                object_candidate_checkpoint_identity(&candidate_snapshot);
            let anchor = PackageTransactionCommitV0::new(
                self.next_transaction_id,
                INSTALL_OPERATION,
                registry_generation,
                object_checkpoint_identity,
                package.object_id.raw(),
                package.revision,
            );
            let mut anchor_bytes = [0; PACKAGE_TRANSACTION_COMMIT_V0_LEN];
            anchor.encode(&mut anchor_bytes)?;

            let content_commit = self.content_store.commit(&mut self.staged_content)?;
            self.staged_registry
                .record_committed_generation(registry_generation);
            let previous_registry = self.registry.clone();
            let previous_registry_snapshot = self.registry_snapshot;
            let previous_object_checkpoint_identity = self.object_checkpoint_identity;
            let previous_anchor_bytes = self.anchor_bytes;
            let previous_anchored_generation_available = self.anchored_generation_available;
            self.registry.copy_from_committed(&self.staged_registry);
            self.registry_snapshot = registry_snapshot;
            self.previous_registry = previous_registry;
            self.previous_registry_snapshot = previous_registry_snapshot;
            self.previous_object_checkpoint_identity = previous_object_checkpoint_identity;
            self.previous_anchor_bytes = previous_anchor_bytes;
            self.previous_anchored_generation_available = previous_anchored_generation_available;
            self.object_checkpoint_identity = object_checkpoint_identity;
            self.anchor_bytes = anchor_bytes;
            self.anchored_generation_available = true;
            self.next_transaction_id = self.next_transaction_id.wrapping_add(1);
            self.next_package_object_id = self.next_package_object_id.wrapping_add(1);
            self.next_schema_object_id = self.next_schema_object_id.wrapping_add(1);
            self.locator_mirror_visible = false;
            candidate_object_service
                .publish_candidate_generation(object_checkpoint_identity.generation);
            *object_service = candidate_object_service;

            Ok(PackageInstallResult {
                transaction_id: anchor.transaction_id,
                package_object_id: package.object_id.raw(),
                package_revision: package.revision,
                schema_object_id: schema.object_id.raw(),
                schema_revision: schema.revision,
                release_digest,
                schema_descriptor_content_id,
                content_commit,
                registry_generation,
                object_checkpoint_identity,
                anchor,
                anchor_bytes,
            })
        })();
        if installed.is_err() {
            self.content_store.rollback(&mut self.staged_content);
        }
        installed
    }

    pub fn prepare_install_candidate(
        &mut self,
        device: BlockDeviceInfo,
        request: PackageInstallRequest,
        source_service: &PackageSourceService<'_>,
        capabilities: &CapabilityTable,
        object_service: &ObjectService,
        artifact_buffer: &'a mut [u8],
    ) -> Result<PackageInstallCandidate, PackageStatus> {
        self.prepare_install_candidate_inner(
            device,
            request,
            source_service,
            capabilities,
            object_service,
            artifact_buffer,
        )
    }

    fn prepare_install_candidate_inner(
        &mut self,
        device: BlockDeviceInfo,
        request: PackageInstallRequest,
        source_service: &PackageSourceService<'_>,
        capabilities: &CapabilityTable,
        object_service: &ObjectService,
        artifact_buffer: &'a mut [u8],
    ) -> Result<PackageInstallCandidate, PackageStatus> {
        if self.prepared_candidate.is_some() {
            return Err(PackageStatus::BadRequest);
        }
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

        self.content_store.rollback(&mut self.staged_content);
        self.staged_content
            .reset(package_object_id.raw(), release_digest);
        let prepared_content_store = self.content_store.clone();
        let prepared_live_object_checkpoint_identity = object_service
            .checkpoint_identity()
            .map_err(map_object_error)?;
        let prepared = (|| -> Result<PackageInstallCandidate, PackageStatus> {
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
                return Err(PackageStatus::InvalidSchema);
            }
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
                .begin_candidate_generation(self.next_transaction_id)?;
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
            self.content_store
                .add_staged_records_to_registry(&self.staged_content, &mut self.staged_registry)?;
            let candidate_snapshot = candidate_object_service
                .encode_candidate_snapshot()
                .map_err(map_object_error)?;
            ensure_candidate_backing_slots_available(
                device,
                self.staged_registry.generation(),
                candidate_snapshot.generation,
            )?;

            let content_commit = self
                .content_store
                .write_candidate_content(device, &self.staged_content)?;
            let registry_generation =
                write_candidate_registry_generation(device, &self.staged_registry)?;
            let decoded_registry = read_candidate_registry_generation(device, registry_generation)?;
            let decoded_registry_generation = PackageRegistryGeneration {
                generation: decoded_registry.generation(),
                root_digest: decoded_registry.root_digest(),
            };
            if decoded_registry_generation != registry_generation {
                return Err(PackageStatus::RegistryRecoveryDenied);
            }
            PackageContentStore::read_validate_candidate_content(device, &decoded_registry)?;
            let object_identity = self.persist_candidate_snapshot(device, &candidate_snapshot)?;
            let transaction_id = self.next_transaction_id;
            self.content_store
                .validate_commit_capacity(&self.staged_content)?;
            let mut staged_registry_snapshot = [0; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES];
            self.staged_registry
                .encode_snapshot(&mut staged_registry_snapshot)?;

            let candidate = PackageInstallCandidate {
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
                staged_registry_snapshot,
            };
            self.prepared_object_service = Some(candidate_object_service);
            self.prepared_candidate_device = Some(device);
            self.prepared_live_object_checkpoint_identity =
                Some(prepared_live_object_checkpoint_identity);
            self.prepared_content_store = Some(prepared_content_store);
            self.prepared_content = Some(self.staged_content.clone());
            self.prepared_candidate = Some(candidate.clone());
            Ok(candidate)
        })();

        match prepared {
            Ok(candidate) => Ok(candidate),
            Err(error) => {
                self.content_store.rollback(&mut self.staged_content);
                self.prepared_candidate_device = None;
                self.prepared_live_object_checkpoint_identity = None;
                self.prepared_content_store = None;
                self.prepared_content = None;
                Err(error)
            }
        }
    }

    pub fn publish_install_candidate(
        &mut self,
        candidate: PackageInstallCandidate,
        object_service: &mut ObjectService,
    ) -> Result<PackageInstallResult, PackageStatus> {
        if self.prepared_candidate.as_ref() != Some(&candidate)
            || self.unpublished_candidate_checkpoint != Some(candidate.object_checkpoint_identity)
        {
            return Err(PackageStatus::BadRequest);
        }
        if self.prepared_live_object_checkpoint_identity
            != Some(
                object_service
                    .checkpoint_identity()
                    .map_err(map_object_error)?,
            )
        {
            return Err(PackageStatus::BadRequest);
        }

        let device = self
            .prepared_candidate_device
            .ok_or(PackageStatus::RegistryWriteDenied)?;
        let mut persisted_registry_snapshot = [0u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES];
        let persisted_registry =
            read_candidate_registry_generation(device, candidate.registry_generation)
                .map_err(|_| PackageStatus::RegistryRecoveryDenied)?;
        let persisted_registry_generation =
            persisted_registry.encode_snapshot(&mut persisted_registry_snapshot)?;
        if persisted_registry_generation != candidate.registry_generation
            || persisted_registry_snapshot != candidate.staged_registry_snapshot
        {
            return Err(PackageStatus::RegistryRecoveryDenied);
        }
        let persisted_content =
            PackageContentStore::read_validate_candidate_content(device, &persisted_registry)
                .map_err(|_| PackageStatus::RegistryRecoveryDenied)?;
        let persisted_candidate_snapshot =
            read_object_service_candidate_checkpoint(device, candidate.object_checkpoint_identity)
                .map_err(map_storage_error)?;
        if object_candidate_checkpoint_identity(&persisted_candidate_snapshot)
            != candidate.object_checkpoint_identity
        {
            return Err(PackageStatus::RegistryRecoveryDenied);
        }
        let mut persisted_object_service = *object_service;
        persisted_object_service
            .apply_snapshot_preserving_runtime_authority(&persisted_candidate_snapshot)
            .map_err(map_object_error)?;
        #[cfg(test)]
        if TEST_CANDIDATE_CONTENT_MATERIALIZATION_FAILURE
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(PackageStatus::QuotaDenied);
        }
        let prepared_content_store = self
            .prepared_content_store
            .as_ref()
            .ok_or(PackageStatus::BadRequest)?;
        let prepared_content = self
            .prepared_content
            .as_ref()
            .ok_or(PackageStatus::BadRequest)?;
        let persisted_content_store = prepared_content_store
            .from_validated_candidate_registry(
                &persisted_registry,
                persisted_content,
                prepared_content,
            )
            .map_err(|_| PackageStatus::RegistryRecoveryDenied)?;

        let anchor = PackageTransactionCommitV0::new(
            candidate.transaction_id,
            INSTALL_OPERATION,
            candidate.registry_generation,
            candidate.object_checkpoint_identity,
            candidate.package_object_id,
            candidate.package_revision,
        );
        let mut anchor_bytes = [0u8; PACKAGE_TRANSACTION_COMMIT_V0_LEN];
        anchor.encode(&mut anchor_bytes)?;
        write_publication_anchor(device, anchor)?;

        let previous_registry = self.registry.clone();
        let previous_registry_snapshot = self.registry_snapshot;
        let previous_object_checkpoint_identity = self.object_checkpoint_identity;
        let previous_anchor_bytes = self.anchor_bytes;
        let previous_anchored_generation_available = self.anchored_generation_available;

        let mut published_registry = persisted_registry;
        published_registry.record_committed_generation(candidate.registry_generation);
        self.registry = published_registry;
        self.content_store = persisted_content_store;
        self.staged_content = PackageContentTransaction::new(0, [0; 32]);
        self.staged_registry.copy_from_committed(&self.registry);
        self.registry_snapshot = persisted_registry_snapshot;
        self.previous_registry = previous_registry;
        self.previous_registry_snapshot = previous_registry_snapshot;
        self.previous_object_checkpoint_identity = previous_object_checkpoint_identity;
        self.previous_anchor_bytes = previous_anchor_bytes;
        self.previous_anchored_generation_available = previous_anchored_generation_available;
        self.object_checkpoint_identity = candidate.object_checkpoint_identity;
        self.anchor_bytes = anchor_bytes;
        self.anchored_generation_available = true;
        self.unpublished_candidate_checkpoint = None;
        self.prepared_candidate = None;
        self.prepared_object_service = None;
        self.prepared_candidate_device = None;
        self.prepared_live_object_checkpoint_identity = None;
        self.prepared_content_store = None;
        self.prepared_content = None;
        self.next_transaction_id = self.next_transaction_id.wrapping_add(1);
        self.next_package_object_id = self.next_package_object_id.wrapping_add(1);
        self.next_schema_object_id = self.next_schema_object_id.wrapping_add(1);
        self.locator_mirror_visible = false;
        *object_service = persisted_object_service;

        Ok(PackageInstallResult {
            transaction_id: candidate.transaction_id,
            package_object_id: candidate.package_object_id,
            package_revision: candidate.package_revision,
            schema_object_id: candidate.schema_object_id,
            schema_revision: candidate.schema_revision,
            release_digest: candidate.release_digest,
            schema_descriptor_content_id: candidate.schema_descriptor_content_id,
            content_commit: candidate.content_commit,
            registry_generation: candidate.registry_generation,
            object_checkpoint_identity: candidate.object_checkpoint_identity,
            anchor,
            anchor_bytes,
        })
    }

    pub const fn registry(&self) -> &PackageRegistry {
        &self.registry
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn seed_launch_export_for_test(
        &mut self,
        namespace_root: ObjectId,
        package_locator: &[u8],
        export_name: &[u8],
        package_object_id: u64,
        package_revision: u64,
        release_digest: [u8; 32],
        schema_object_id: u64,
        schema_revision: u64,
        schema_descriptor_digest: [u8; 32],
    ) -> Result<PackageRegistryExportRecord, PackageStatus> {
        self.registry
            .add_package_record(PackageRegistryPackageRecord::new(
                package_object_id,
                package_revision,
                release_digest,
                PackageStatus::Ok as u16,
            ))?;
        self.registry
            .add_schema_record(PackageRegistrySchemaRecord::new(
                schema_object_id,
                schema_revision,
                package_object_id,
                0,
                schema_descriptor_digest,
            ))?;
        let export = PackageRegistryExportRecord::new(
            namespace_root.raw(),
            package_locator,
            export_name,
            package_object_id,
            package_revision,
            release_digest,
            1,
            0,
            0,
            schema_object_id,
            schema_revision,
            schema_descriptor_digest,
        )?;
        self.registry.add_export_record(export)?;
        Ok(export)
    }

    pub fn resolve_export(
        &self,
        namespace_root: ObjectId,
        locator: &str,
    ) -> Result<PackageRegistryExportRecord, PackageStatus> {
        self.registry.export_for_locator(namespace_root, locator)
    }

    pub fn launch(
        &mut self,
        request: PackageLaunchRequest<'_>,
        capabilities: &CapabilityTable,
    ) -> Result<PackageLaunchResult, PackageStatus> {
        if request.supplied_grants.len() > MAX_REQUIREMENT_RECORDS {
            return Err(PackageStatus::BadRequest);
        }
        let export = self.resolve_export(request.namespace_root, request.locator)?;
        let launch = validate_launch_authority(
            request.caller,
            request.supplied_grants,
            capabilities,
            export,
            &self.launch_requirements[..self.launch_requirement_count],
        )?;
        self.record_runtime_context(request.caller, launch.export)?;
        Ok(launch)
    }

    pub fn runtime_schema_binding(
        &self,
        process: ActiveUserProcess,
        schema_slot: u16,
    ) -> Result<PackageRuntimeSchemaBindingV0, PackageStatus> {
        let export = self
            .runtime_context_for_process(process)
            .ok_or(PackageStatus::Denied)?;
        if schema_slot != 0 {
            return Err(PackageStatus::NotFound);
        }
        Ok(PackageRuntimeSchemaBindingV0 {
            abi_major: 0,
            abi_minor: 1,
            schema_slot,
            reserved0: 0,
            package_object_id: export.package_object_id,
            package_revision: export.package_revision,
            schema_object_id: export.schema_object_id,
            schema_revision: export.schema_revision,
            schema_descriptor_sha256: export.schema_descriptor_digest,
            reserved1: [0; 16],
        })
    }

    #[cfg(test)]
    pub fn runtime_context_count_for_test(&self) -> usize {
        self.runtime_context_count
    }

    pub fn record_launch_requirement(
        &mut self,
        namespace_root: ObjectId,
        locator: &str,
        requirement: PackageLaunchRequirement,
    ) -> Result<(), PackageStatus> {
        let export = self.resolve_export(namespace_root, locator)?;
        self.record_launch_requirement_for_export(export, requirement)
    }

    fn record_launch_requirement_for_export(
        &mut self,
        export: PackageRegistryExportRecord,
        requirement: PackageLaunchRequirement,
    ) -> Result<(), PackageStatus> {
        if self.launch_requirement_count >= MAX_REQUIREMENT_RECORDS {
            return Err(PackageStatus::QuotaDenied);
        }
        if self.launch_requirements.iter().flatten().any(|record| {
            record.export == export
                && (record.requirement.requirement_id == requirement.requirement_id
                    || record.requirement.graph_import_slot == requirement.graph_import_slot)
        }) {
            return Err(PackageStatus::DuplicateStableName);
        }

        self.launch_requirements[self.launch_requirement_count] =
            Some(PackageLaunchRequirementRecord {
                export,
                requirement,
            });
        self.launch_requirement_count += 1;
        Ok(())
    }

    fn record_runtime_context(
        &mut self,
        process: ActiveUserProcess,
        export: PackageRegistryExportRecord,
    ) -> Result<(), PackageStatus> {
        let record = PackageRuntimeContextRecord {
            service_id: process.service_id(),
            principal_id: process.principal_id(),
            program_digest: process.program_digest(),
            export,
        };
        let mut index = 0usize;
        while index < self.runtime_context_count {
            if let Some(existing) = self.runtime_contexts[index]
                && same_runtime_context_identity(existing, record)
            {
                self.runtime_contexts[index] = Some(record);
                return Ok(());
            }
            index += 1;
        }
        if self.runtime_context_count >= MAX_REQUIREMENT_RECORDS {
            return Err(PackageStatus::QuotaDenied);
        }
        self.runtime_contexts[self.runtime_context_count] = Some(record);
        self.runtime_context_count += 1;
        Ok(())
    }

    fn runtime_context_for_process(
        &self,
        process: ActiveUserProcess,
    ) -> Option<PackageRegistryExportRecord> {
        let mut index = 0usize;
        while index < self.runtime_context_count {
            if let Some(record) = self.runtime_contexts[index]
                && runtime_context_matches_process(record, process)
            {
                return Some(record.export);
            }
            index += 1;
        }
        None
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

    pub fn recover(&mut self) -> Result<PackageRecoveryReport<'_>, PackageStatus> {
        self.recover_inner(None)
    }

    pub fn recover_with_candidate_checkpoint(
        &mut self,
        device: BlockDeviceInfo,
    ) -> Result<PackageRecoveryReport<'_>, PackageStatus> {
        self.recover_inner(Some(device))
    }

    pub fn restore_from_storage(
        &mut self,
        device: BlockDeviceInfo,
    ) -> Result<PackageRecoveryReport<'_>, PackageStatus> {
        with_package_restore_scratch(|scratch| {
            self.restore_from_storage_with_scratch(device, scratch)
        })
    }

    fn restore_from_storage_with_scratch(
        &mut self,
        device: BlockDeviceInfo,
        scratch: &mut PackageRestoreScratch,
    ) -> Result<PackageRecoveryReport<'_>, PackageStatus> {
        let mut newest_discovered_generation = None;
        scratch.selected_anchor = None;

        for slot in [
            PackagePublicationAnchorSlot::A,
            PackagePublicationAnchorSlot::B,
        ] {
            let Some(anchor) = read_publication_anchor_slot(device, slot)? else {
                continue;
            };
            newest_discovered_generation = Some(
                newest_discovered_generation
                    .map_or(anchor.package_registry_generation, |generation: u64| {
                        generation.max(anchor.package_registry_generation)
                    }),
            );

            let registry_generation = PackageRegistryGeneration {
                generation: anchor.package_registry_generation,
                root_digest: anchor.package_registry_root_digest,
            };
            let object_checkpoint_identity = ObjectCheckpointIdentity {
                generation: anchor.object_checkpoint_generation,
                root_digest: anchor.object_checkpoint_root_digest,
            };
            if read_candidate_registry_generation_into(
                device,
                registry_generation,
                &mut scratch.candidate.registry,
            )
            .is_err()
            {
                continue;
            }
            if read_object_service_candidate_checkpoint_into(
                device,
                object_checkpoint_identity,
                &mut scratch.candidate.object_snapshot,
            )
            .is_err()
            {
                continue;
            }
            if object_candidate_checkpoint_identity(&scratch.candidate.object_snapshot)
                != object_checkpoint_identity
                || !anchor.matches_pair(registry_generation, object_checkpoint_identity)
                || !registry_contains_anchor_package(&scratch.candidate.registry, anchor)
                || !registry_matches_object_snapshot(
                    &scratch.candidate.registry,
                    anchor,
                    &scratch.candidate.object_snapshot,
                )
            {
                continue;
            }
            let Ok(validated_content) = PackageContentStore::read_validate_candidate_content(
                device,
                &scratch.candidate.registry,
            ) else {
                continue;
            };
            if PackageContentStore::restore_from_validated_registry_into(
                device,
                &scratch.candidate.registry,
                validated_content,
                &mut scratch.candidate.content_store,
            )
            .is_err()
            {
                continue;
            }

            let candidate_order = (anchor.package_registry_generation, anchor.transaction_id);
            let replace_selected = match scratch.selected_anchor {
                Some(selected_anchor) => {
                    candidate_order
                        > (
                            selected_anchor.package_registry_generation,
                            selected_anchor.transaction_id,
                        )
                }
                None => true,
            };
            if replace_selected {
                scratch.selected_anchor = Some(anchor);
                scratch.selected.copy_from(&scratch.candidate);
            }
        }

        let Some(anchor) = scratch.selected_anchor else {
            return Ok(PackageRecoveryReport {
                published_world_selected: false,
                previous_published_world_selected: false,
                unpublished_candidate_ignored: false,
                candidate_content_reclaimable: false,
                locator_mirrors_require_rebuild: false,
                selected_object_checkpoint_identity: None,
                selected_object_snapshot: None,
            });
        };

        let registry_generation = scratch
            .selected
            .registry
            .encode_snapshot(&mut scratch.registry_snapshot)?;
        let object_checkpoint_identity = ObjectCheckpointIdentity {
            generation: anchor.object_checkpoint_generation,
            root_digest: anchor.object_checkpoint_root_digest,
        };
        anchor.encode(&mut scratch.anchor_bytes)?;

        self.commit_restored_publication(
            anchor,
            registry_generation,
            object_checkpoint_identity,
            newest_discovered_generation,
            scratch,
        )
    }

    fn commit_restored_publication(
        &mut self,
        anchor: PackageTransactionCommitV0,
        registry_generation: PackageRegistryGeneration,
        object_checkpoint_identity: ObjectCheckpointIdentity,
        newest_discovered_generation: Option<u64>,
        scratch: &PackageRestoreScratch,
    ) -> Result<PackageRecoveryReport<'_>, PackageStatus> {
        self.registry
            .copy_from_committed(&scratch.selected.registry);
        self.content_store
            .copy_restored_state_from(&scratch.selected.content_store);
        self.staged_content.reset_empty();
        self.staged_registry.copy_from_committed(&self.registry);
        self.registry_snapshot
            .copy_from_slice(&scratch.registry_snapshot);
        self.previous_registry.clear_to_empty();
        self.previous_registry_snapshot.fill(0);
        self.previous_object_checkpoint_identity = ObjectCheckpointIdentity {
            generation: 0,
            root_digest: [0; 32],
        };
        self.previous_anchor_bytes.fill(0);
        self.previous_anchored_generation_available = false;
        self.object_checkpoint_identity = object_checkpoint_identity;
        self.anchor_bytes.copy_from_slice(&scratch.anchor_bytes);
        self.anchored_generation_available = true;
        self.unpublished_candidate_checkpoint = None;
        clear_restore_option(&mut self.prepared_candidate);
        clear_restore_option(&mut self.prepared_object_service);
        self.prepared_candidate_device = None;
        self.prepared_live_object_checkpoint_identity = None;
        clear_restore_option(&mut self.prepared_content_store);
        clear_restore_option(&mut self.prepared_content);
        self.next_transaction_id = anchor.transaction_id.wrapping_add(1);
        self.next_package_object_id = next_package_object_id(&self.registry);
        self.next_schema_object_id = next_schema_object_id(&self.registry);
        self.locator_mirror_visible = false;

        store_object_snapshot(
            &mut self.restored_object_snapshot,
            &scratch.selected.object_snapshot,
        );
        Ok(PackageRecoveryReport {
            published_world_selected: true,
            previous_published_world_selected: newest_discovered_generation
                .is_some_and(|generation| generation > registry_generation.generation),
            unpublished_candidate_ignored: false,
            candidate_content_reclaimable: false,
            locator_mirrors_require_rebuild: self.registry.package_count() != 0,
            selected_object_checkpoint_identity: Some(object_checkpoint_identity),
            selected_object_snapshot: self.restored_object_snapshot.as_ref(),
        })
    }

    fn recover_inner(
        &mut self,
        candidate_device: Option<BlockDeviceInfo>,
    ) -> Result<PackageRecoveryReport<'_>, PackageStatus> {
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
            self.prepared_candidate = None;
            self.prepared_object_service = None;
            self.prepared_candidate_device = None;
            self.prepared_live_object_checkpoint_identity = None;
            self.prepared_content_store = None;
            self.prepared_content = None;
        }

        let selected_object_snapshot_available = match (candidate_device, selected_object_identity)
        {
            (Some(device), Some(identity)) => read_object_service_candidate_checkpoint_into(
                device,
                identity,
                object_snapshot_slot(&mut self.restored_object_snapshot),
            )
            .is_ok(),
            _ => {
                clear_restore_option(&mut self.restored_object_snapshot);
                false
            }
        };
        Ok(PackageRecoveryReport {
            published_world_selected: true,
            previous_published_world_selected: selected_previous,
            unpublished_candidate_ignored,
            candidate_content_reclaimable: unpublished_candidate_ignored,
            locator_mirrors_require_rebuild: self.registry.package_count() != 0
                && !self.locator_mirror_visible,
            selected_object_checkpoint_identity: selected_object_identity
                .filter(|_| selected_object_snapshot_available),
            selected_object_snapshot: if selected_object_snapshot_available {
                self.restored_object_snapshot.as_ref()
            } else {
                None
            },
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
        #[cfg(test)]
        let snapshot_to_write = TEST_CANDIDATE_SNAPSHOT_WRITE_OVERRIDE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
            .unwrap_or(*snapshot);
        #[cfg(not(test))]
        let snapshot_to_write = *snapshot;

        let candidate = write_object_service_candidate_checkpoint(device, &snapshot_to_write)
            .map_err(map_storage_error)?;
        let loaded = read_object_service_candidate_checkpoint(device, candidate.identity)
            .map_err(map_storage_error)?;
        let loaded_identity = object_candidate_checkpoint_identity(&loaded);
        if loaded_identity != candidate.identity || loaded != *snapshot {
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
        self.previous_registry_snapshot = [0; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES];
        self.previous_object_checkpoint_identity = ObjectCheckpointIdentity {
            generation: 0,
            root_digest: [0; 32],
        };
        self.previous_anchor_bytes = [0; PACKAGE_TRANSACTION_COMMIT_V0_LEN];
        self.previous_anchored_generation_available = false;
        self.locator_mirror_visible = false;
    }
}

fn copy_object_snapshot(target: &mut ObjectServiceSnapshot, source: &ObjectServiceSnapshot) {
    target.generation = source.generation;
    target.allocated_bitmap = source.allocated_bitmap;
    target.objects.copy_from_slice(&source.objects);
    target
        .workspace_relationships
        .copy_from_slice(&source.workspace_relationships);
    target
        .current_revisions
        .copy_from_slice(&source.current_revisions);
    target
        .prior_revisions
        .copy_from_slice(&source.prior_revisions);
}

fn object_snapshot_slot(slot: &mut Option<ObjectServiceSnapshot>) -> &mut ObjectServiceSnapshot {
    if slot.is_none() {
        *slot = Some(ObjectServiceSnapshot::empty());
    }
    slot.as_mut().expect("object snapshot slot initialized")
}

fn store_object_snapshot(slot: &mut Option<ObjectServiceSnapshot>, source: &ObjectServiceSnapshot) {
    copy_object_snapshot(object_snapshot_slot(slot), source);
}

fn clear_restore_option<T>(slot: &mut Option<T>) {
    *slot = None;
}

fn validate_launch_authority(
    caller: ActiveUserProcess,
    supplied_grants: &[PackageLaunchGrant],
    capabilities: &CapabilityTable,
    export: PackageRegistryExportRecord,
    launch_requirements: &[Option<PackageLaunchRequirementRecord>],
) -> Result<PackageLaunchResult, PackageStatus> {
    let mut launch = PackageLaunchResult::new(export);
    let mut index = 0usize;
    while index < launch_requirements.len() {
        if let Some(record) = launch_requirements[index]
            && record.export == export
        {
            let requirement = record.requirement;
            let supplied = find_supplied_launch_grant(requirement, supplied_grants)
                .ok_or(PackageStatus::RequiredGrantMissing)?;
            capabilities
                .validate(
                    caller.service_id(),
                    supplied.capability,
                    requirement.resource,
                    requirement.rights,
                )
                .map_err(|_| PackageStatus::FinalCapabilityDenied)?;
            launch.grants[launch.grant_count] = Some(PackageLaunchResolvedGrant {
                supplied,
                graph_import_slot: requirement.graph_import_slot,
            });
            launch.grant_count += 1;
        }
        index += 1;
    }

    Ok(launch)
}

fn same_runtime_context_identity(
    left: PackageRuntimeContextRecord,
    right: PackageRuntimeContextRecord,
) -> bool {
    left.service_id == right.service_id
        && left.principal_id == right.principal_id
        && left.program_digest == right.program_digest
}

fn runtime_context_matches_process(
    context: PackageRuntimeContextRecord,
    process: ActiveUserProcess,
) -> bool {
    context.service_id == process.service_id()
        && context.principal_id == process.principal_id()
        && context.program_digest == process.program_digest()
}

fn find_supplied_launch_grant(
    requirement: PackageLaunchRequirement,
    supplied_grants: &[PackageLaunchGrant],
) -> Option<PackageLaunchGrant> {
    let mut index = 0usize;
    while index < supplied_grants.len() {
        let supplied = supplied_grants[index];
        if supplied.requirement_id == requirement.requirement_id {
            return Some(supplied);
        }
        index += 1;
    }
    None
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

fn registry_contains_anchor_package(
    registry: &PackageRegistry,
    anchor: PackageTransactionCommitV0,
) -> bool {
    if anchor.operation != INSTALL_OPERATION {
        return false;
    }

    let mut index = 0usize;
    while let Some(record) = registry.package_record(index) {
        if record.package_object_id == anchor.package_object_id
            && record.installed_revision == anchor.package_installed_revision
        {
            return true;
        }
        index += 1;
    }
    false
}

fn ensure_candidate_backing_slots_available(
    device: BlockDeviceInfo,
    registry_generation: u64,
    object_checkpoint_generation: u64,
) -> Result<(), PackageStatus> {
    for slot in [
        PackagePublicationAnchorSlot::A,
        PackagePublicationAnchorSlot::B,
    ] {
        let Some(anchor) = read_publication_anchor_slot(device, slot)? else {
            continue;
        };
        if publication_anchor_is_complete(device, anchor)
            && ((anchor.package_registry_generation & 1) == (registry_generation & 1)
                || (anchor.object_checkpoint_generation & 1) == (object_checkpoint_generation & 1))
        {
            return Err(PackageStatus::RegistryWriteDenied);
        }
    }
    Ok(())
}

fn publication_anchor_is_complete(
    device: BlockDeviceInfo,
    anchor: PackageTransactionCommitV0,
) -> bool {
    if anchor.operation != INSTALL_OPERATION {
        return false;
    }
    let registry_generation = PackageRegistryGeneration {
        generation: anchor.package_registry_generation,
        root_digest: anchor.package_registry_root_digest,
    };
    let object_checkpoint_identity = ObjectCheckpointIdentity {
        generation: anchor.object_checkpoint_generation,
        root_digest: anchor.object_checkpoint_root_digest,
    };
    let Ok(registry) = read_candidate_registry_generation(device, registry_generation) else {
        return false;
    };
    let Ok(snapshot) = read_object_service_candidate_checkpoint(device, object_checkpoint_identity)
    else {
        return false;
    };
    anchor.matches_pair(registry_generation, object_checkpoint_identity)
        && registry_contains_anchor_package(&registry, anchor)
        && registry_matches_object_snapshot(&registry, anchor, &snapshot)
        && PackageContentStore::read_validate_candidate_content(device, &registry).is_ok()
}

fn registry_matches_object_snapshot(
    registry: &PackageRegistry,
    anchor: PackageTransactionCommitV0,
    snapshot: &ObjectServiceSnapshot,
) -> bool {
    let mut package_index = 0usize;
    let mut anchored_package = None;
    while let Some(record) = registry.package_record(package_index) {
        if record.package_object_id == anchor.package_object_id
            && record.installed_revision == anchor.package_installed_revision
        {
            anchored_package = Some(record);
            break;
        }
        package_index += 1;
    }
    let Some(package) = anchored_package else {
        return false;
    };
    if package.object_kind != OBJECT_KIND_PACKAGE
        || !snapshot_contains_current_revision(
            snapshot,
            package.package_object_id,
            package.installed_revision,
            ObjectKind::Package,
        )
    {
        return false;
    }

    let mut anchored_schema_count = 0usize;
    let mut index = 0usize;
    while let Some(schema) = registry.schema_record(index) {
        if schema.package_object_id == anchor.package_object_id {
            anchored_schema_count += 1;
            if schema.object_kind != OBJECT_KIND_SCHEMA_DEFINITION
                || !snapshot_contains_current_revision(
                    snapshot,
                    schema.schema_object_id,
                    schema.schema_revision,
                    ObjectKind::SchemaDefinition,
                )
            {
                return false;
            }
        }
        index += 1;
    }
    anchored_schema_count != 0
}

fn snapshot_contains_current_revision(
    snapshot: &ObjectServiceSnapshot,
    object_id: u64,
    revision: u64,
    kind: ObjectKind,
) -> bool {
    let object_present = snapshot.objects.iter().flatten().any(|record| {
        record.object.object_id().raw() == object_id && record.object.object_kind() == kind
    });
    object_present
        && snapshot.current_revisions.iter().flatten().any(|record| {
            record.object_id().raw() == object_id
                && record.revision() == revision
                && record.object().object_id().raw() == object_id
                && record.object().object_kind() == kind
        })
}

fn next_package_object_id(registry: &PackageRegistry) -> u64 {
    let mut next = FIRST_PACKAGE_OBJECT_ID;
    let mut index = 0usize;
    while let Some(record) = registry.package_record(index) {
        if record.package_object_id >= next {
            next = record.package_object_id.wrapping_add(1);
        }
        index += 1;
    }
    next
}

fn next_schema_object_id(registry: &PackageRegistry) -> u64 {
    let mut next = FIRST_SCHEMA_OBJECT_ID;
    let mut index = 0usize;
    while let Some(record) = registry.schema_record(index) {
        if record.schema_object_id >= next {
            next = record.schema_object_id.wrapping_add(1);
        }
        index += 1;
    }
    next
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
        PackageInstallCandidate, PackageInstallRequest, PackageInstallResult, PackageLaunchGrant,
        PackageLaunchRequest, PackageLaunchRequirement, PackageRecoveryReport, PackageService,
    };
    use crate::{
        block_device::BlockDeviceInfo,
        capabilities::{CapabilityTable, ResourceId, RightsMask},
        object_relationships::{PACKAGE_LOCATOR_ROOT_OBJECT_ID, RelationshipKind},
        object_service::{ObjectService, ObjectServiceError},
        object_service_checkpoint::{
            read_object_service_candidate_checkpoint, read_object_service_checkpoint,
            write_object_service_candidate_checkpoint,
        },
        package_candidate_store::{
            PACKAGE_CANDIDATE_STORAGE_TEST_LOCK, PackagePublicationAnchorSlot,
            read_candidate_registry_generation, read_publication_anchor_slot,
            reset_package_persistence_storage_for_test, write_candidate_registry_generation,
            write_publication_anchor,
        },
        package_content_store::{ContentId, PackageContentStore, PackageContentTransaction},
        package_registry::{
            PackageRegistry, PackageRegistryExportRecord, PackageRegistryPackageRecord,
            PackageTransactionCommitV0,
        },
        package_source::PackageSourceService,
        process_context::ActiveUserProcess,
        pyth_runtime_launch::prepare_package_launch_runtime_bootstrap,
        service_identity::ServiceId,
        shell_objects::{ObjectId, ObjectKind},
    };
    use pythos_shared::{
        init_bundle::{self, INIT_BUNDLE_HEADER_LEN, RECORD_ENTRY_LEN, TYPE_PACKAGE_SOURCE},
        package_abi::{
            MAX_PACKAGE_ARTIFACT_BYTES, PACKAGE_CONTENT_BITMAP_WORDS,
            PACKAGE_CONTENT_MAX_STAGED_RECORDS, PACKAGE_INSTALL_RESOURCE_ID, PACKAGE_INSTALL_RIGHT,
            PACKAGE_SOURCE_READ_RIGHT, PACKAGE_SOURCE_RESOURCE_ID, PackageStatus,
        },
        package_format::{CONTENT_ENTRY_V0_LEN, PACKAGE_ARTIFACT_HEADER_LEN},
        pyth_tig::{test_support, verify::verify_bytes},
        sha256::{Sha256, sha256},
    };
    use std::vec::Vec;
    use std::{boxed::Box, vec};

    const CALLER_SERVICE: ServiceId = ServiceId::from_raw(0x5059_504B_494E_5301);
    const SCHEMA_DESCRIPTOR: &[u8] = b"schema:seed.v0";
    const ADDITIONAL_CONTENT: &[u8] = b"payload:seed.v0";
    const ORPHAN_CONTENT: &[u8] = b"orphan";

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
                    selected_object_checkpoint_identity: None,
                    selected_object_snapshot: None,
                })
            );
        });
    }

    #[test]
    fn package_publication_anchor_persists_without_selecting_candidate() {
        static CONTENT: &[u8] = b"anchor-candidate-content";

        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut store = PackageContentStore::empty();
        let mut transaction = PackageContentTransaction::new(42, sha256(b"anchor-release"));
        store
            .stage_content(&mut transaction, 1, 1, CONTENT, sha256(CONTENT))
            .unwrap();
        let mut registry = PackageRegistry::empty();
        store
            .add_staged_records_to_registry(&transaction, &mut registry)
            .unwrap();
        store.write_candidate_content(device, &transaction).unwrap();
        let registry_generation = write_candidate_registry_generation(device, &registry).unwrap();
        let restored = read_candidate_registry_generation(device, registry_generation).unwrap();
        let object_identity = crate::object_service_checkpoint::ObjectCheckpointIdentity {
            generation: 9,
            root_digest: [9; 32],
        };
        let anchor =
            PackageTransactionCommitV0::new(7, 1, registry_generation, object_identity, 42, 1);

        write_publication_anchor(device, anchor).unwrap();
        let decoded = read_publication_anchor_slot(device, PackagePublicationAnchorSlot::B)
            .unwrap()
            .unwrap();
        let fresh = PackageService::new_empty_for_test();

        assert_eq!(decoded, anchor);
        assert_eq!(restored.package_count(), 0);
        assert_eq!(restored.content_count(), 1);
        assert_eq!(
            read_publication_anchor_slot(device, PackagePublicationAnchorSlot::A),
            Ok(None)
        );
        assert_eq!(fresh.registry().package_count(), 0);
        assert!(!fresh.locator_mirror_visible);
    }

    #[test]
    fn package_restore_from_storage_selects_published_candidate_without_boot1_ram() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let (installed, expected_checkpoint) = {
            let mut boot_one_service = PackageService::new_empty_for_test();
            let mut boot_one_objects = ObjectService::new_for_test();
            let candidate = prepare_recovery_package_install_candidate(
                &mut boot_one_service,
                device,
                &boot_one_objects,
            )
            .unwrap();
            let installed = boot_one_service
                .publish_install_candidate(candidate, &mut boot_one_objects)
                .unwrap();
            let expected_checkpoint = installed.object_checkpoint_identity;

            (installed, expected_checkpoint)
        };

        let mut boot_two_service = PackageService::new_empty_for_test();
        let report = boot_two_service.restore_from_storage(device).unwrap();

        assert!(report.published_world_selected);
        assert!(!report.previous_published_world_selected);
        assert!(report.locator_mirrors_require_rebuild);
        assert_eq!(boot_two_service.registry().package_count(), 1);
        assert_eq!(boot_two_service.registry().schema_count(), 1);
        assert_eq!(
            boot_two_service.content_store().committed_bitmap(),
            installed.content_commit.committed_bitmap()
        );
        assert!(read_object_service_candidate_checkpoint(device, expected_checkpoint).is_ok());
    }

    #[test]
    fn package_restore_from_storage_falls_back_when_no_publication_anchor_exists() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let candidate = {
            let mut boot_one_service = PackageService::new_empty_for_test();
            let boot_one_objects = ObjectService::new_for_test();
            prepare_recovery_package_install_candidate(
                &mut boot_one_service,
                device,
                &boot_one_objects,
            )
            .unwrap()
        };
        let candidate_registry =
            read_candidate_registry_generation(device, candidate.registry_generation).unwrap();
        let candidate_extent = candidate_registry.content_record(0).unwrap().extents[0];

        let mut boot_two_service = PackageService::new_empty_for_test();
        let report = boot_two_service.restore_from_storage(device).unwrap();

        assert!(!report.published_world_selected);
        assert_eq!(boot_two_service.registry().package_count(), 0);
        assert!(
            !PackageContentStore::extent_live_in_registry(
                boot_two_service.registry(),
                candidate_extent,
            )
            .unwrap()
        );
    }

    #[test]
    fn package_restore_from_storage_rejects_mismatched_publication_anchor() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let (first, second) = {
            let mut boot_one_service = PackageService::new_empty_for_test();
            let mut boot_one_objects = ObjectService::new_for_test();
            let first_candidate = prepare_recovery_package_install_candidate(
                &mut boot_one_service,
                device,
                &boot_one_objects,
            )
            .unwrap();
            let first = boot_one_service
                .publish_install_candidate(first_candidate, &mut boot_one_objects)
                .unwrap();
            let second_candidate = prepare_recovery_package_install_candidate(
                &mut boot_one_service,
                device,
                &boot_one_objects,
            )
            .unwrap();
            let second = boot_one_service
                .publish_install_candidate(second_candidate, &mut boot_one_objects)
                .unwrap();

            (first, second)
        };
        let mut mismatched = second.anchor;
        mismatched.object_checkpoint_root_digest[0] ^= 0x80;
        write_publication_anchor(device, mismatched).unwrap();

        let mut boot_two_service = PackageService::new_empty_for_test();
        let report = boot_two_service.restore_from_storage(device).unwrap();

        assert!(report.published_world_selected);
        assert!(report.previous_published_world_selected);
        assert_eq!(boot_two_service.registry().package_count(), 1);
        assert_eq!(
            boot_two_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            first.package_object_id
        );
        assert_ne!(
            boot_two_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            second.package_object_id
        );
    }

    #[test]
    fn package_restore_from_storage_preserves_previous_world_when_later_prepare_is_unpublished() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut boot_one_service = PackageService::new_empty_for_test();
        let mut boot_one_objects = ObjectService::new_for_test();
        let first_candidate = prepare_recovery_package_install_candidate(
            &mut boot_one_service,
            device,
            &boot_one_objects,
        )
        .unwrap();
        let first = boot_one_service
            .publish_install_candidate(first_candidate, &mut boot_one_objects)
            .unwrap();
        let second_candidate = prepare_recovery_package_install_candidate(
            &mut boot_one_service,
            device,
            &boot_one_objects,
        )
        .unwrap();
        let second = boot_one_service
            .publish_install_candidate(second_candidate, &mut boot_one_objects)
            .unwrap();

        assert_eq!(
            prepare_recovery_package_install_candidate(
                &mut boot_one_service,
                device,
                &boot_one_objects,
            ),
            Err(PackageStatus::RegistryWriteDenied)
        );
        assert!(read_candidate_registry_generation(device, first.registry_generation).is_ok());
        assert!(
            read_object_service_candidate_checkpoint(device, first.object_checkpoint_identity)
                .is_ok()
        );
        let mut corrupt_newest = second.anchor;
        corrupt_newest.object_checkpoint_root_digest[0] ^= 0x80;
        write_publication_anchor(device, corrupt_newest).unwrap();

        let mut boot_two_service = PackageService::new_empty_for_test();
        let report = boot_two_service.restore_from_storage(device).unwrap();

        assert!(report.published_world_selected);
        assert!(report.previous_published_world_selected);
        assert_eq!(boot_two_service.registry().package_count(), 1);
        assert_eq!(
            boot_two_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            first.package_object_id
        );
    }

    #[test]
    fn package_restore_from_storage_reads_selected_persisted_content() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let installed = {
            let mut boot_one_service = PackageService::new_empty_for_test();
            let mut boot_one_objects = ObjectService::new_for_test();
            let candidate = prepare_recovery_package_install_candidate(
                &mut boot_one_service,
                device,
                &boot_one_objects,
            )
            .unwrap();
            boot_one_service
                .publish_install_candidate(candidate, &mut boot_one_objects)
                .unwrap()
        };

        let mut boot_two_service = PackageService::new_empty_for_test();
        boot_two_service.restore_from_storage(device).unwrap();
        let mut restored = [0u8; SCHEMA_DESCRIPTOR.len()];
        let restored_len = boot_two_service
            .content_store()
            .read_published(
                ContentId::new(installed.package_object_id, installed.release_digest, 0),
                &mut restored,
            )
            .unwrap();

        assert_eq!(restored_len, SCHEMA_DESCRIPTOR.len());
        assert_eq!(&restored, SCHEMA_DESCRIPTOR);
    }

    #[test]
    fn package_restore_from_storage_reports_selected_object_checkpoint_facts() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let installed = {
            let mut boot_one_service = PackageService::new_empty_for_test();
            let mut boot_one_objects = ObjectService::new_for_test();
            let candidate = prepare_recovery_package_install_candidate(
                &mut boot_one_service,
                device,
                &boot_one_objects,
            )
            .unwrap();
            boot_one_service
                .publish_install_candidate(candidate, &mut boot_one_objects)
                .unwrap()
        };
        let expected_snapshot =
            read_object_service_candidate_checkpoint(device, installed.object_checkpoint_identity)
                .unwrap();

        let mut boot_two_service = PackageService::new_empty_for_test();
        let report = boot_two_service.restore_from_storage(device).unwrap();

        assert_eq!(
            report.selected_object_checkpoint(),
            Some((installed.object_checkpoint_identity, &expected_snapshot))
        );
    }

    #[test]
    fn package_restore_from_storage_rejects_semantically_mismatched_object_world() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut boot_one_service = PackageService::new_empty_for_test();
        let mut boot_one_objects = ObjectService::new_for_test();
        let first_candidate = prepare_recovery_package_install_candidate(
            &mut boot_one_service,
            device,
            &boot_one_objects,
        )
        .unwrap();
        let first = boot_one_service
            .publish_install_candidate(first_candidate, &mut boot_one_objects)
            .unwrap();
        let second = prepare_recovery_package_install_candidate(
            &mut boot_one_service,
            device,
            &boot_one_objects,
        )
        .unwrap();

        let mut mismatched_objects = ObjectService::new_for_test();
        let mismatched_caller = mismatched_objects.test_shell_caller();
        let mismatched_package_id = ObjectId::new(second.package_object_id.wrapping_add(100));
        mismatched_objects
            .create_package_object(mismatched_caller, mismatched_package_id, sha256(b"wrong"))
            .unwrap();
        mismatched_objects
            .create_schema_definition_object(
                mismatched_caller,
                ObjectId::new(second.schema_object_id.wrapping_add(100)),
                mismatched_package_id,
                sha256(b"wrong-schema"),
            )
            .unwrap();
        mismatched_objects.publish_candidate_generation(
            second.object_checkpoint_identity.generation.wrapping_sub(1),
        );
        let mismatched_snapshot = mismatched_objects.encode_candidate_snapshot().unwrap();
        let mismatched_checkpoint =
            write_object_service_candidate_checkpoint(device, &mismatched_snapshot).unwrap();
        let mismatched_anchor = PackageTransactionCommitV0::new(
            second.transaction_id,
            1,
            second.registry_generation,
            mismatched_checkpoint.identity,
            second.package_object_id,
            second.package_revision,
        );
        write_publication_anchor(device, mismatched_anchor).unwrap();

        let mut boot_two_service = PackageService::new_empty_for_test();
        let report = boot_two_service.restore_from_storage(device).unwrap();

        assert!(report.published_world_selected);
        assert!(report.previous_published_world_selected);
        assert_eq!(boot_two_service.registry().package_count(), 1);
        assert_eq!(
            boot_two_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            first.package_object_id
        );
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
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
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
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
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
        let mut corrupt_newest = second.anchor;
        corrupt_newest.object_checkpoint_root_digest[0] ^= 0xFF;
        write_publication_anchor(device, corrupt_newest).unwrap();
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
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
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
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
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
    fn package_prepare_install_candidate_writes_validated_candidate_without_publication() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let world_a = install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();
        let live_object_identity = object_service.checkpoint_identity().unwrap();

        let candidate = prepare_recovery_package_install_candidate(
            &mut package_service,
            device,
            &object_service,
        )
        .unwrap();

        assert_ne!(candidate.transaction_id, 0);
        assert_ne!(candidate.package_object_id, 0);
        assert_ne!(candidate.schema_object_id, 0);
        assert_ne!(candidate.content_commit.record_count(), 0);
        assert_ne!(candidate.registry_generation.generation, 0);
        assert_ne!(candidate.object_checkpoint_identity.generation, 0);
        let candidate_registry = PackageRegistry::decode_snapshot(
            &candidate.staged_registry_snapshot[..package_service.staged_registry.encoded_len()],
        )
        .unwrap();
        assert_eq!(
            candidate_registry.generation(),
            candidate.registry_generation.generation
        );
        assert_eq!(
            candidate_registry.root_digest(),
            candidate.registry_generation.root_digest
        );
        let persisted_registry =
            read_candidate_registry_generation(device, candidate.registry_generation).unwrap();
        let persisted_content =
            PackageContentStore::read_validate_candidate_content(device, &persisted_registry)
                .unwrap();
        assert!(persisted_content.record_count() >= candidate.content_commit.record_count());
        assert_eq!(
            read_publication_anchor_slot(
                device,
                if candidate.registry_generation.generation & 1 == 0 {
                    PackagePublicationAnchorSlot::A
                } else {
                    PackagePublicationAnchorSlot::B
                },
            ),
            Ok(None)
        );
        let persisted =
            read_object_service_candidate_checkpoint(device, candidate.object_checkpoint_identity)
                .unwrap();
        assert!(persisted.objects.iter().flatten().any(|record| {
            record.object.object_id() == ObjectId::new(candidate.package_object_id)
        }));
        assert!(persisted.objects.iter().flatten().any(|record| {
            record.object.object_id() == ObjectId::new(candidate.schema_object_id)
        }));

        assert_eq!(package_service.registry().package_count(), 1);
        assert_eq!(
            package_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            world_a.package_object_id
        );
        assert_eq!(
            object_service.checkpoint_identity().unwrap(),
            live_object_identity
        );
        assert!(!package_service.locator_mirror_visible_for_test());

        let report = package_service
            .recover_with_candidate_checkpoint(device)
            .unwrap();
        assert!(report.published_world_selected);
        assert!(report.unpublished_candidate_ignored);
        assert!(report.candidate_content_reclaimable);
        assert_eq!(package_service.registry().package_count(), 1);
        assert_eq!(
            package_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            world_a.package_object_id
        );
        assert_eq!(
            object_service.checkpoint_identity().unwrap(),
            live_object_identity
        );
        assert!(!package_service.locator_mirror_visible_for_test());
    }

    #[test]
    fn package_prepare_install_candidate_rejects_unintended_valid_checkpoint() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let object_service = ObjectService::new_for_test();

        super::set_candidate_snapshot_write_override_for_test(
            ObjectService::new_for_test()
                .encode_candidate_snapshot()
                .unwrap(),
        );

        assert_eq!(
            prepare_recovery_package_install_candidate(
                &mut package_service,
                device,
                &object_service
            ),
            Err(PackageStatus::RegistryRecoveryDenied)
        );
        assert_eq!(package_service.registry().package_count(), 0);
        assert!(package_service.prepared_candidate.is_none());
        assert!(package_service.prepared_object_service.is_none());
    }

    #[test]
    fn package_publish_install_candidate_materializes_before_writing_anchor() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let candidate = prepare_recovery_package_install_candidate(
            &mut package_service,
            device,
            &object_service,
        )
        .unwrap();

        super::fail_candidate_content_materialization_for_test();

        assert_eq!(
            package_service.publish_install_candidate(candidate.clone(), &mut object_service),
            Err(PackageStatus::QuotaDenied)
        );
        assert_eq!(
            read_publication_anchor_slot(
                device,
                if candidate.registry_generation.generation & 1 == 0 {
                    PackagePublicationAnchorSlot::A
                } else {
                    PackagePublicationAnchorSlot::B
                },
            ),
            Ok(None)
        );
        assert_eq!(package_service.registry().package_count(), 0);
    }

    #[test]
    fn package_publish_install_candidate_selects_persisted_world_not_mutated_ram() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let candidate = prepare_recovery_package_install_candidate(
            &mut package_service,
            device,
            &object_service,
        )
        .unwrap();
        let persisted_registry =
            read_candidate_registry_generation(device, candidate.registry_generation).unwrap();
        let mut persisted_registry_snapshot = [0; super::PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES];
        persisted_registry
            .encode_snapshot(&mut persisted_registry_snapshot)
            .unwrap();
        let persisted_content =
            PackageContentStore::read_validate_candidate_content(device, &persisted_registry)
                .unwrap();
        let persisted_object_snapshot =
            read_object_service_candidate_checkpoint(device, candidate.object_checkpoint_identity)
                .unwrap();

        package_service.staged_registry = PackageRegistry::empty();
        package_service
            .content_store
            .rollback(&mut package_service.staged_content);
        package_service.prepared_object_service = Some(ObjectService::new_for_test());

        let installed = package_service
            .publish_install_candidate(candidate.clone(), &mut object_service)
            .unwrap();

        assert_eq!(installed.registry_generation, candidate.registry_generation);
        let mut selected_registry = persisted_registry;
        selected_registry.record_committed_generation(candidate.registry_generation);
        assert_eq!(package_service.registry(), &selected_registry);
        assert_eq!(
            package_service.registry_snapshot,
            persisted_registry_snapshot
        );
        assert_eq!(
            package_service.content_store.committed_bitmap(),
            persisted_content.committed_bitmap()
        );
        assert_eq!(
            object_service.encode_snapshot().unwrap(),
            persisted_object_snapshot
        );
        assert_eq!(
            object_service
                .dynamic_object_for_test(ObjectId::new(candidate.package_object_id))
                .unwrap()
                .object_kind(),
            ObjectKind::Package
        );
        assert!(!package_service.locator_mirror_visible_for_test());
    }

    #[test]
    fn package_publish_install_candidate_denies_persisted_registry_mismatch() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let world_a = install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();
        let candidate = prepare_recovery_package_install_candidate(
            &mut package_service,
            device,
            &object_service,
        )
        .unwrap();

        let mut mismatched_registry = package_service.registry().clone();
        mismatched_registry
            .begin_candidate_generation(candidate.transaction_id)
            .unwrap();
        mismatched_registry
            .add_package_record(PackageRegistryPackageRecord::new(
                candidate.package_object_id,
                candidate.package_revision,
                candidate.release_digest,
                PackageStatus::Ok as u16,
            ))
            .unwrap();
        assert_eq!(
            write_candidate_registry_generation(device, &mismatched_registry)
                .unwrap()
                .generation,
            candidate.registry_generation.generation
        );

        assert_eq!(
            package_service.publish_install_candidate(candidate, &mut object_service),
            Err(PackageStatus::RegistryRecoveryDenied)
        );
        assert_eq!(package_service.registry().package_count(), 1);
        assert_eq!(
            package_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            world_a.package_object_id
        );
        assert!(!package_service.locator_mirror_visible_for_test());
    }

    #[test]
    fn package_publish_install_candidate_selects_prepared_world_once() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let candidate = prepare_recovery_package_install_candidate(
            &mut package_service,
            device,
            &object_service,
        )
        .unwrap();
        let replay = candidate.clone();

        let installed = package_service
            .publish_install_candidate(candidate.clone(), &mut object_service)
            .unwrap();

        assert_eq!(installed.transaction_id, candidate.transaction_id);
        assert_eq!(installed.package_object_id, candidate.package_object_id);
        assert_eq!(installed.package_revision, candidate.package_revision);
        assert_eq!(installed.schema_object_id, candidate.schema_object_id);
        assert_eq!(installed.schema_revision, candidate.schema_revision);
        assert_eq!(installed.registry_generation, candidate.registry_generation);
        assert_eq!(
            installed.object_checkpoint_identity,
            candidate.object_checkpoint_identity
        );
        assert_eq!(
            PackageTransactionCommitV0::decode(
                &installed.anchor_bytes,
                candidate.registry_generation,
                candidate.object_checkpoint_identity,
            )
            .unwrap(),
            installed.anchor
        );
        assert_eq!(package_service.registry().package_count(), 1);
        assert_eq!(package_service.registry().schema_count(), 1);
        assert_eq!(
            package_service
                .registry()
                .package_record(0)
                .unwrap()
                .package_object_id,
            candidate.package_object_id
        );
        assert_eq!(
            package_service
                .registry()
                .schema_record(0)
                .unwrap()
                .schema_object_id,
            candidate.schema_object_id
        );
        assert_eq!(
            object_service.checkpoint_identity().unwrap().generation,
            candidate.object_checkpoint_identity.generation
        );
        assert_eq!(
            object_service
                .dynamic_object_for_test(ObjectId::new(candidate.package_object_id))
                .unwrap()
                .object_kind(),
            ObjectKind::Package
        );
        assert_eq!(
            object_service
                .dynamic_object_for_test(ObjectId::new(candidate.schema_object_id))
                .unwrap()
                .object_kind(),
            ObjectKind::SchemaDefinition
        );
        assert!(!package_service.locator_mirror_visible_for_test());
        assert_eq!(
            package_service.publish_install_candidate(replay, &mut object_service),
            Err(PackageStatus::BadRequest)
        );
    }

    #[test]
    fn package_publish_install_candidate_preserves_revoked_live_capability() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let shell = object_service.test_shell_caller();
        let revoked = object_service.test_shell_workspace_capability();
        let candidate = prepare_recovery_package_install_candidate(
            &mut package_service,
            device,
            &object_service,
        )
        .unwrap();

        object_service
            .revoke_object_capability_for_test(shell, revoked)
            .unwrap();
        assert_eq!(
            object_service.query_objects(shell, revoked, ObjectKind::Note),
            Err(ObjectServiceError::Denied)
        );

        package_service
            .publish_install_candidate(candidate, &mut object_service)
            .unwrap();

        assert_eq!(
            object_service.query_objects(shell, revoked, ObjectKind::Note),
            Err(ObjectServiceError::Denied)
        );
    }

    #[test]
    fn package_publish_install_candidate_denies_stale_live_object_world_before_anchor() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let shell = object_service.test_shell_caller();
        let workspace = object_service.test_shell_workspace_capability();
        let candidate = prepare_recovery_package_install_candidate(
            &mut package_service,
            device,
            &object_service,
        )
        .unwrap();

        let created = object_service
            .create_object(shell, workspace, ObjectKind::Note)
            .unwrap();
        let live_identity = object_service.checkpoint_identity().unwrap();

        assert_eq!(
            package_service.publish_install_candidate(candidate.clone(), &mut object_service),
            Err(PackageStatus::BadRequest)
        );
        assert_eq!(
            read_publication_anchor_slot(
                device,
                if candidate.registry_generation.generation & 1 == 0 {
                    PackagePublicationAnchorSlot::A
                } else {
                    PackagePublicationAnchorSlot::B
                },
            ),
            Ok(None)
        );
        assert_eq!(object_service.checkpoint_identity().unwrap(), live_identity);
        assert!(object_service.object_exists_for_test(created.object_id));
        assert!(
            object_service
                .inspect_object(shell, created.object_capability, created.object_id)
                .is_ok()
        );
    }

    #[test]
    fn package_publish_install_candidate_keeps_durable_content_readable() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let candidate = prepare_recovery_package_install_candidate(
            &mut package_service,
            device,
            &object_service,
        )
        .unwrap();

        package_service
            .publish_install_candidate(candidate.clone(), &mut object_service)
            .unwrap();

        assert_eq!(
            package_service
                .content_store()
                .read_committed(ContentId::new(
                    candidate.package_object_id,
                    candidate.release_digest,
                    0,
                ))
                .unwrap(),
            SCHEMA_DESCRIPTOR
        );
    }

    #[test]
    fn package_install_into_denies_while_durable_candidate_is_pending() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let candidate = prepare_recovery_package_install_candidate(
            &mut package_service,
            device,
            &object_service,
        )
        .unwrap();
        let registry_before = package_service.registry().clone();
        let object_before = object_service.checkpoint_identity().unwrap();

        assert_eq!(
            install_recovery_package_into(&mut package_service, &mut object_service),
            Err(PackageStatus::BadRequest)
        );
        assert_eq!(package_service.registry(), &registry_before);
        assert_eq!(object_service.checkpoint_identity().unwrap(), object_before);
        assert_eq!(package_service.prepared_candidate, Some(candidate));
    }

    #[test]
    fn package_prepare_install_candidate_rolls_back_staged_content_after_registry_failure() {
        let mut package_service = PackageService::new_empty_for_test();
        let object_service = ObjectService::new_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);

        for index in 0..32 {
            package_service
                .registry
                .add_package_record(PackageRegistryPackageRecord::new(
                    0x5059_4649_4C4C_0000 + index,
                    1,
                    [index as u8; 32],
                    PackageStatus::Ok as u16,
                ))
                .unwrap();
        }

        assert_eq!(
            prepare_two_content_recovery_package_install_candidate(
                &mut package_service,
                device,
                &object_service,
            ),
            Err(PackageStatus::QuotaDenied)
        );
        assert_eq!(package_service.staged_content.staged_count(), 0);
        assert!(
            package_service
                .content_store
                .staged_bitmap()
                .iter()
                .all(|word| *word == 0)
        );
        assert_eq!(
            package_service.content_store.committed_bitmap(),
            [0; PACKAGE_CONTENT_BITMAP_WORDS]
        );
        assert!(package_service.prepared_candidate.is_none());
        assert!(package_service.prepared_object_service.is_none());
    }

    #[test]
    fn package_recovery_reports_durable_unanchored_candidate_as_ignored_reclaimable() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
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
    fn package_recovery_reuses_unanchored_candidate_content_extent() {
        static RETRY_CONTENT: &[u8] = b"reused-candidate-extent";

        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let world_a = install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();
        let candidate = prepare_recovery_package_install_candidate(
            &mut package_service,
            device,
            &object_service,
        )
        .unwrap();
        let candidate_only = bitmap_difference(
            candidate.content_commit.committed_bitmap(),
            world_a.content_commit.committed_bitmap(),
        );
        assert!(candidate_only.iter().any(|word| *word != 0));

        let report = package_service
            .recover_with_candidate_checkpoint(device)
            .unwrap();
        assert!(report.unpublished_candidate_ignored);
        let mut retry = PackageContentTransaction::new(0x9000, sha256(b"retry-release"));
        package_service
            .content_store
            .stage_content(&mut retry, 1, 1, RETRY_CONTENT, sha256(RETRY_CONTENT))
            .unwrap();

        assert!(
            package_service
                .content_store
                .staged_bitmap()
                .iter()
                .zip(candidate_only.iter())
                .any(|(staged, candidate)| (*staged & *candidate) != 0)
        );
    }

    #[test]
    fn package_recovery_rejects_mismatched_anchor_for_durable_candidate_world() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
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
    fn package_install_into_preserves_valid_compatibility_install() {
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        let installed =
            install_recovery_package_into(&mut package_service, &mut object_service).unwrap();

        assert_ne!(installed.content_commit.record_count(), 0);
        assert_eq!(package_service.registry().package_count(), 1);
        assert_eq!(package_service.registry().schema_count(), 1);
        assert_eq!(
            PackageTransactionCommitV0::decode(
                &installed.anchor_bytes,
                installed.registry_generation,
                installed.object_checkpoint_identity,
            )
            .unwrap(),
            installed.anchor
        );
    }

    #[test]
    fn package_install_with_candidate_checkpoint_advances_live_object_generation() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
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
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        let mut object_service = ObjectService::new_for_test();
        install_recovery_package_with_candidate_checkpoint_into(
            &mut package_service,
            device,
            &mut object_service,
        )
        .unwrap();

        reset_package_persistence_storage_for_test();

        assert_eq!(
            package_service.recover_with_candidate_checkpoint(device),
            Err(PackageStatus::RegistryRecoveryDenied)
        );
    }

    #[test]
    fn package_recovery_ignores_unanchored_candidate_without_overwriting_anchor() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
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
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut package_service = PackageService::new_empty_for_test();
        install_recovery_package_with_candidate_checkpoint(&mut package_service, device).unwrap();
        f(&mut package_service);
    }

    fn bitmap_difference(
        newer: [u64; PACKAGE_CONTENT_BITMAP_WORDS],
        older: [u64; PACKAGE_CONTENT_BITMAP_WORDS],
    ) -> [u64; PACKAGE_CONTENT_BITMAP_WORDS] {
        let mut difference = [0; PACKAGE_CONTENT_BITMAP_WORDS];
        let mut index = 0usize;
        while index < PACKAGE_CONTENT_BITMAP_WORDS {
            difference[index] = newer[index] & !older[index];
            index += 1;
        }
        difference
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

    fn prepare_recovery_package_install_candidate(
        package_service: &mut PackageService<'static>,
        device: BlockDeviceInfo,
        object_service: &ObjectService,
    ) -> Result<PackageInstallCandidate, PackageStatus> {
        let artifact = build_schema_package_artifact();
        let source_record = build_source_record(0, b"phase13-prepare.pkg", &artifact);
        let bundle_bytes = build_bundle(&[(TYPE_PACKAGE_SOURCE, &source_record)]);
        let init_bundle = init_bundle::validate(&bundle_bytes).unwrap();
        let source_service = PackageSourceService::from_init_bundle(&init_bundle).unwrap();
        let source_handle = source_service.handle_at(0).unwrap();
        let caller = ActiveUserProcess::new(CALLER_SERVICE, 0x504B_5052_4550, 0x13);
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

        package_service.prepare_install_candidate(
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

    fn prepare_two_content_recovery_package_install_candidate(
        package_service: &mut PackageService<'static>,
        device: BlockDeviceInfo,
        object_service: &ObjectService,
    ) -> Result<PackageInstallCandidate, PackageStatus> {
        let artifact = build_two_content_schema_package_artifact();
        let source_record = build_source_record(0, b"phase13-rollback.pkg", &artifact);
        let bundle_bytes = build_bundle(&[(TYPE_PACKAGE_SOURCE, &source_record)]);
        let init_bundle = init_bundle::validate(&bundle_bytes).unwrap();
        let source_service = PackageSourceService::from_init_bundle(&init_bundle).unwrap();
        let source_handle = source_service.handle_at(0).unwrap();
        let caller = ActiveUserProcess::new(CALLER_SERVICE, 0x504B_524F_4C4C, 0x13);
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

        package_service.prepare_install_candidate(
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
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
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
            PackageContentStore::read_validate_candidate_content(
                device,
                package_service.registry()
            )
            .unwrap()
            .record_count(),
            1
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
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_persistence_storage_for_test();
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

    #[test]
    fn package_export_resolution_resolves_export_from_explicit_namespace_root() {
        let mut package_service = PackageService::new_empty_for_test();
        let release_digest = sha256(b"seed-release");
        let schema_digest = sha256(b"schema:seed.v0");
        package_service
            .registry
            .add_package_record(PackageRegistryPackageRecord::new(
                0x5059_504B_474F_0001,
                7,
                release_digest,
                PackageStatus::Ok as u16,
            ))
            .unwrap();
        package_service
            .registry
            .add_export_record(
                PackageRegistryExportRecord::new(
                    PACKAGE_LOCATOR_ROOT_OBJECT_ID,
                    b"seed",
                    b"launch",
                    0x5059_504B_474F_0001,
                    7,
                    release_digest,
                    1,
                    0,
                    0,
                    0x5059_5343_484F_0001,
                    1,
                    schema_digest,
                )
                .unwrap(),
            )
            .unwrap();

        let resolved = package_service
            .resolve_export(ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID), "seed/launch")
            .unwrap();

        assert_eq!(resolved.package_object_id, 0x5059_504B_474F_0001);
        assert_eq!(resolved.package_revision, 7);
        assert_eq!(resolved.release_digest, release_digest);
        assert_eq!(resolved.schema_object_id, 0x5059_5343_484F_0001);
        assert_eq!(
            package_service.resolve_export(ObjectId::new(0xDEAD_BEEF), "seed/launch"),
            Err(PackageStatus::ExportMissing)
        );
    }

    #[test]
    fn package_export_resolution_missing_export_returns_export_missing() {
        let mut package_service = PackageService::new_empty_for_test();
        package_service
            .registry
            .add_package_record(PackageRegistryPackageRecord::new(
                0x5059_504B_474F_0001,
                1,
                sha256(b"seed-release"),
                PackageStatus::Ok as u16,
            ))
            .unwrap();

        assert_eq!(
            package_service.resolve_export(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                "seed/missing",
            ),
            Err(PackageStatus::ExportMissing)
        );
    }

    #[test]
    fn package_export_resolution_invalid_locator_syntax_is_denied_before_registry_lookup() {
        let package_service = PackageService::new_empty_for_test();

        assert_eq!(
            package_service.resolve_export(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                "seed/../launch",
            ),
            Err(PackageStatus::InvalidLocator)
        );
    }

    #[test]
    fn package_export_resolution_locator_text_is_not_package_object_id() {
        let mut package_service = PackageService::new_empty_for_test();
        let release_digest = sha256(b"seed-release");
        let schema_digest = sha256(b"schema:seed.v0");
        package_service
            .registry
            .add_package_record(PackageRegistryPackageRecord::new(
                0x5059_504B_474F_0001,
                3,
                release_digest,
                PackageStatus::Ok as u16,
            ))
            .unwrap();
        package_service
            .registry
            .add_export_record(
                PackageRegistryExportRecord::new(
                    PACKAGE_LOCATOR_ROOT_OBJECT_ID,
                    b"seed",
                    b"launch",
                    0x5059_504B_474F_0001,
                    3,
                    release_digest,
                    1,
                    0,
                    0,
                    0x5059_5343_484F_0001,
                    1,
                    schema_digest,
                )
                .unwrap(),
            )
            .unwrap();

        assert_eq!(
            package_service.resolve_export(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                "5059504b474f0001/launch",
            ),
            Err(PackageStatus::ExportMissing)
        );
        assert_eq!(
            package_service
                .resolve_export(ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID), "seed/launch",)
                .unwrap()
                .package_object_id,
            0x5059_504B_474F_0001
        );
    }

    #[test]
    fn package_launch_capability_missing_required_grant_returns_required_grant_missing() {
        let mut package_service = service_with_launch_export();
        let caller = ActiveUserProcess::new(CALLER_SERVICE, 0x504B_4C41_554E_4301, 0x13);
        let mut capabilities = CapabilityTable::new();
        let _ambient_create_capability = capabilities
            .grant(
                caller.service_id(),
                ResourceId::new(0x5059_4F42_4A43_0001),
                RightsMask::new(RightsMask::WRITE),
            )
            .unwrap();
        let requirements = [PackageLaunchRequirement {
            requirement_id: 7,
            graph_import_slot: 0,
            resource: ResourceId::new(0x5059_4F42_4A43_0001),
            rights: RightsMask::new(RightsMask::WRITE),
        }];
        package_service
            .record_launch_requirement(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                "seed/launch",
                requirements[0],
            )
            .unwrap();
        let supplied_grants = [];

        assert_eq!(
            package_service.launch(
                PackageLaunchRequest {
                    caller,
                    namespace_root: ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                    locator: "seed/launch",
                    supplied_grants: &supplied_grants,
                },
                &capabilities,
            ),
            Err(PackageStatus::RequiredGrantMissing)
        );
    }

    #[test]
    fn package_launch_capability_declared_requirement_omitted_grant_is_missing() {
        let mut package_service = service_with_launch_export();
        let caller = ActiveUserProcess::new(CALLER_SERVICE, 0x504B_4C41_554E_4304, 0x13);
        let requirement = PackageLaunchRequirement {
            requirement_id: 7,
            graph_import_slot: 0,
            resource: ResourceId::new(0x5059_4F42_4A43_0001),
            rights: RightsMask::new(RightsMask::WRITE),
        };
        package_service
            .record_launch_requirement(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                "seed/launch",
                requirement,
            )
            .unwrap();
        let supplied_grants = [];

        assert_eq!(
            package_service.launch(
                PackageLaunchRequest {
                    caller,
                    namespace_root: ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                    locator: "seed/launch",
                    supplied_grants: &supplied_grants,
                },
                &CapabilityTable::new(),
            ),
            Err(PackageStatus::RequiredGrantMissing)
        );
    }

    #[test]
    fn package_launch_capability_final_authority_denial_returns_final_capability_denied() {
        let mut package_service = service_with_launch_export();
        let caller = ActiveUserProcess::new(CALLER_SERVICE, 0x504B_4C41_554E_4302, 0x13);
        let other = ServiceId::from_raw(0x5059_504B_4F54_4852);
        let mut capabilities = CapabilityTable::new();
        let wrong_holder_capability = capabilities
            .grant(
                other,
                ResourceId::new(0x5059_4F42_4A43_0001),
                RightsMask::new(RightsMask::WRITE),
            )
            .unwrap();
        let requirements = [PackageLaunchRequirement {
            requirement_id: 7,
            graph_import_slot: 0,
            resource: ResourceId::new(0x5059_4F42_4A43_0001),
            rights: RightsMask::new(RightsMask::WRITE),
        }];
        package_service
            .record_launch_requirement(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                "seed/launch",
                requirements[0],
            )
            .unwrap();
        let supplied_grants = [PackageLaunchGrant {
            requirement_id: 7,
            capability: wrong_holder_capability,
        }];

        assert_eq!(
            package_service.launch(
                PackageLaunchRequest {
                    caller,
                    namespace_root: ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                    locator: "seed/launch",
                    supplied_grants: &supplied_grants,
                },
                &capabilities,
            ),
            Err(PackageStatus::FinalCapabilityDenied)
        );
    }

    #[test]
    fn package_launch_capability_valid_explicit_grants_produce_request_with_only_supplied_grants() {
        let mut package_service = service_with_launch_export();
        let caller = ActiveUserProcess::new(CALLER_SERVICE, 0x504B_4C41_554E_4303, 0x13);
        let mut capabilities = CapabilityTable::new();
        let supplied_capability = capabilities
            .grant(
                caller.service_id(),
                ResourceId::new(0x5059_4F42_4A43_0001),
                RightsMask::new(RightsMask::WRITE),
            )
            .unwrap();
        let _not_supplied_capability = capabilities
            .grant(
                caller.service_id(),
                ResourceId::new(0x5059_4F42_4A43_0002),
                RightsMask::new(RightsMask::WRITE),
            )
            .unwrap();
        let requirements = [PackageLaunchRequirement {
            requirement_id: 7,
            graph_import_slot: 0,
            resource: ResourceId::new(0x5059_4F42_4A43_0001),
            rights: RightsMask::new(RightsMask::WRITE),
        }];
        package_service
            .record_launch_requirement(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                "seed/launch",
                requirements[0],
            )
            .unwrap();
        let supplied_grants = [PackageLaunchGrant {
            requirement_id: 7,
            capability: supplied_capability,
        }];

        let launch = package_service
            .launch(
                PackageLaunchRequest {
                    caller,
                    namespace_root: ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                    locator: "seed/launch",
                    supplied_grants: &supplied_grants,
                },
                &capabilities,
            )
            .unwrap();

        assert_eq!(launch.export.package_object_id, 0x5059_504B_474F_0001);
        assert_eq!(launch.grant_count, 1);
        assert_eq!(launch.grant(0), Some(supplied_grants[0]));
        assert_eq!(launch.grant(1), None);
    }

    #[test]
    fn package_launch_requirement_registration_rejects_duplicate_graph_import_slot() {
        let mut package_service = service_with_launch_export();
        let first_requirement = PackageLaunchRequirement {
            requirement_id: 7,
            graph_import_slot: 0,
            resource: ResourceId::new(0x5059_4F42_4A43_0001),
            rights: RightsMask::new(RightsMask::WRITE),
        };
        let duplicate_slot_requirement = PackageLaunchRequirement {
            requirement_id: 8,
            graph_import_slot: 0,
            resource: ResourceId::new(0x5059_4F42_4A43_0002),
            rights: RightsMask::new(RightsMask::READ),
        };

        package_service
            .record_launch_requirement(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                "seed/launch",
                first_requirement,
            )
            .unwrap();

        assert_eq!(
            package_service.record_launch_requirement(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                "seed/launch",
                duplicate_slot_requirement,
            ),
            Err(PackageStatus::DuplicateStableName)
        );
    }

    #[test]
    fn package_launch_runtime_maps_requirement_id_to_explicit_graph_import_slot() {
        let mut package_service = service_with_launch_export();
        let caller = ActiveUserProcess::new(CALLER_SERVICE, 0x504B_4C41_554E_4305, 0x13);
        let mut capabilities = CapabilityTable::new();
        let supplied_capability = capabilities
            .grant(
                caller.service_id(),
                ResourceId::new(0x5059_4F42_4A43_0001),
                RightsMask::new(RightsMask::WRITE),
            )
            .unwrap();
        let requirement = PackageLaunchRequirement {
            requirement_id: 7,
            graph_import_slot: 0,
            resource: ResourceId::new(0x5059_4F42_4A43_0001),
            rights: RightsMask::new(RightsMask::WRITE),
        };
        package_service
            .record_launch_requirement(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                "seed/launch",
                requirement,
            )
            .unwrap();
        let supplied_grants = [PackageLaunchGrant {
            requirement_id: 7,
            capability: supplied_capability,
        }];

        let launch = package_service
            .launch(
                PackageLaunchRequest {
                    caller,
                    namespace_root: ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                    locator: "seed/launch",
                    supplied_grants: &supplied_grants,
                },
                &capabilities,
            )
            .unwrap();
        let graph_import_grants = [launch.graph_import_grant(0).unwrap()];
        let package = test_support::object_note_flow_package();
        let verified = verify_bytes(&package).unwrap();

        let bootstrap = prepare_package_launch_runtime_bootstrap(
            &verified,
            package.len() as u64,
            &graph_import_grants,
        )
        .unwrap();

        assert_eq!(launch.grant(0), Some(supplied_grants[0]));
        assert_eq!(bootstrap.imports[0].import_slot, 0);
        assert_eq!(
            bootstrap.imports[0].capability,
            supplied_grants[0].packed_capability()
        );
    }

    fn service_with_launch_export() -> PackageService<'static> {
        let mut package_service = PackageService::new_empty_for_test();
        let release_digest = sha256(b"seed-release");
        let schema_digest = sha256(b"schema:seed.v0");
        package_service
            .registry
            .add_package_record(PackageRegistryPackageRecord::new(
                0x5059_504B_474F_0001,
                7,
                release_digest,
                PackageStatus::Ok as u16,
            ))
            .unwrap();
        package_service
            .registry
            .add_export_record(
                PackageRegistryExportRecord::new(
                    PACKAGE_LOCATOR_ROOT_OBJECT_ID,
                    b"seed",
                    b"launch",
                    0x5059_504B_474F_0001,
                    7,
                    release_digest,
                    1,
                    0,
                    0,
                    0x5059_5343_484F_0001,
                    1,
                    schema_digest,
                )
                .unwrap(),
            )
            .unwrap();
        package_service
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

    fn build_two_content_schema_package_artifact() -> Vec<u8> {
        let mut manifest = b"PYTHMAN0".to_vec();
        manifest.extend_from_slice(&1u32.to_le_bytes());
        push_manifest_record(&mut manifest, 1, b"seed.v0", &0u16.to_le_bytes());

        let mut content_table = vec![0u8; 2 * CONTENT_ENTRY_V0_LEN];
        content_table[0..2].copy_from_slice(&0u16.to_le_bytes());
        content_table[2..4].copy_from_slice(&2u16.to_le_bytes());
        content_table[4..6].copy_from_slice(&1u16.to_le_bytes());
        content_table[6..8].copy_from_slice(&1u16.to_le_bytes());
        content_table[8..16].copy_from_slice(&0u64.to_le_bytes());
        content_table[16..24].copy_from_slice(&(SCHEMA_DESCRIPTOR.len() as u64).to_le_bytes());
        content_table[24..56].copy_from_slice(&sha256(SCHEMA_DESCRIPTOR));

        let second = CONTENT_ENTRY_V0_LEN;
        content_table[second..second + 2].copy_from_slice(&1u16.to_le_bytes());
        content_table[second + 2..second + 4].copy_from_slice(&2u16.to_le_bytes());
        content_table[second + 4..second + 6].copy_from_slice(&1u16.to_le_bytes());
        content_table[second + 6..second + 8].copy_from_slice(&1u16.to_le_bytes());
        content_table[second + 8..second + 16]
            .copy_from_slice(&(SCHEMA_DESCRIPTOR.len() as u64).to_le_bytes());
        content_table[second + 16..second + 24]
            .copy_from_slice(&(ADDITIONAL_CONTENT.len() as u64).to_le_bytes());
        content_table[second + 24..second + 56].copy_from_slice(&sha256(ADDITIONAL_CONTENT));

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
        header[56..64].copy_from_slice(
            &((SCHEMA_DESCRIPTOR.len() + ADDITIONAL_CONTENT.len()) as u64).to_le_bytes(),
        );
        header[64..96].copy_from_slice(&sha256(&manifest));

        let mut artifact = header;
        artifact.extend_from_slice(&manifest);
        artifact.extend_from_slice(&content_table);
        artifact.extend_from_slice(SCHEMA_DESCRIPTOR);
        artifact.extend_from_slice(ADDITIONAL_CONTENT);
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
