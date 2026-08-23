#![cfg_attr(test, allow(dead_code))]

use pythos_shared::{
    boot_protocol::PythBootInfo,
    init_bundle, init_pak,
    package_abi::{
        MAX_PACKAGE_ARTIFACT_BYTES, MAX_PACKAGE_SOURCE_LABEL_BYTES, MAX_PACKAGE_SOURCES,
        PACKAGE_INSTALL_RESOURCE_ID, PACKAGE_INSTALL_RIGHT, PACKAGE_SOURCE_READ_RIGHT,
        PACKAGE_SOURCE_RESOURCE_ID, PackageStatus,
    },
    package_format::PackageArtifactV0,
    sha256::sha256,
};

use crate::{
    block_device::BlockDeviceInfo,
    capabilities::{CapabilityHandle, CapabilityTable, ResourceId, RightsMask},
    object_relationships::PackageLocatorRelationshipStore,
    object_service::{ObjectService, ObjectServiceError},
    object_service_checkpoint::{
        ObjectCheckpointIdentity, ObjectServiceSnapshot, write_object_service_candidate_checkpoint,
    },
    package_candidate_store::{
        PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES, write_candidate_registry_generation_into,
        write_publication_anchor,
    },
    package_content_store::{PackageContentCommit, PackageContentStore, PackageContentTransaction},
    package_registry::{
        PackageRegistry, PackageRegistryGeneration, PackageRegistryPackageRecord,
        PackageRegistrySchemaRecord, PackageTransactionCommitV0,
    },
    package_service::{PackageInstallRequest, PackageInstallResult, PackageService},
    package_source::PackageSourceService,
    process_context::ActiveUserProcess,
    serial,
    service_identity::ServiceId,
    shell_objects::ObjectId,
};
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};

const PACKAGE_SOURCE_MAGIC: &[u8; 8] = b"PYPKGS01";
const PACKAGE_SOURCE_HEADER_LEN: usize = 64;
const PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER_BYTES: usize = 4096;
const FORMAT_FIXTURE_LABEL: &[u8] = b"phase13-format-fixture.pkg";
const INSTALL_SUCCESS_LABEL: &[u8] = b"phase13-install-success.pkg";
const INSTALL_SOURCE_DENIED_LABEL: &[u8] = b"phase13-install-source-denied.pkg";
const RESTORE_STACK_SMOKE_LABEL: &[u8] = b"phase13-restore-stack-smoke.pkg";
const PACKAGE_INSTALL_SERVICE_ID: ServiceId = ServiceId::from_raw(0x5059_504B_494E_5354);
const PACKAGE_INSTALL_PRINCIPAL_ID: u64 = 0x5059_504B_494E_5354;
const PACKAGE_INSTALL_PROGRAM_DIGEST: u64 = 0x5059_0013;
const RESTORE_SMOKE_TRANSACTION_ID: u64 = 1;
const RESTORE_SMOKE_PACKAGE_OBJECT_ID: u64 = 0x5059_504B_474F_0001;
const RESTORE_SMOKE_SCHEMA_OBJECT_ID: u64 = 0x5059_5343_484F_0001;
const INSTALL_OPERATION: u16 = 1;

static mut PACKAGE_ACCEPTANCE_OBJECT_SERVICE: MaybeUninit<ObjectService> = MaybeUninit::uninit();
static mut PACKAGE_ACCEPTANCE_PACKAGE_SERVICE: PackageService<'static> =
    PackageService::new_empty_for_test();
static mut PACKAGE_ACCEPTANCE_RESTORE_SERVICE: PackageService<'static> =
    PackageService::new_empty_for_test();
static mut PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER: [u8; PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER_BYTES] =
    [0; PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER_BYTES];
static mut PACKAGE_ACCEPTANCE_CAPABILITIES: CapabilityTable = CapabilityTable::new();
static mut PACKAGE_ACCEPTANCE_SOURCE_SERVICE: PackageSourceService<'static> =
    PackageSourceService::empty();
static mut PACKAGE_ACCEPTANCE_INSTALL_RESULT: PackageInstallResult = PackageInstallResult::empty();
static mut PACKAGE_ACCEPTANCE_LOCATOR_MIRRORS: PackageLocatorRelationshipStore =
    PackageLocatorRelationshipStore::new();
static mut PACKAGE_ACCEPTANCE_RESTORE_SEED: PackageRestoreSmokeSeed =
    PackageRestoreSmokeSeed::empty();
static RESTORE_STACK_SMOKE_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageAcceptanceError {
    BadInitPak,
    BadInitBundle,
    MissingSource,
    TooManySources,
    BadSource,
    BadSourceDigest,
    BadPackageFormat,
    InvalidPackageWasAccepted,
    UnexpectedScenario,
    PackageOperation,
    ObjectService,
}

struct PackageRestoreSmokeSeed {
    content_store: PackageContentStore<'static>,
    content: PackageContentTransaction<'static>,
    registry: PackageRegistry,
    registry_snapshot: [u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES],
    decoded_registry: PackageRegistry,
    object_snapshot: ObjectServiceSnapshot,
    registry_generation: PackageRegistryGeneration,
    object_checkpoint: ObjectCheckpointIdentity,
    content_commit: PackageContentCommit,
}

impl PackageRestoreSmokeSeed {
    const fn empty() -> Self {
        Self {
            content_store: PackageContentStore::empty(),
            content: PackageContentTransaction::new(0, [0; 32]),
            registry: PackageRegistry::empty(),
            registry_snapshot: [0; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES],
            decoded_registry: PackageRegistry::empty(),
            object_snapshot: ObjectServiceSnapshot::empty(),
            registry_generation: PackageRegistryGeneration {
                generation: 0,
                root_digest: [0; 32],
            },
            object_checkpoint: ObjectCheckpointIdentity {
                generation: 0,
                root_digest: [0; 32],
            },
            content_commit: PackageContentCommit::empty(),
        }
    }
}

pub fn run_package_format_acceptance(
    boot_info: &PythBootInfo,
    block_device: BlockDeviceInfo,
) -> Result<(), PackageAcceptanceError> {
    let bytes = init_pak_bytes(boot_info)?;
    let payload = init_pak_payload(bytes)?;
    let bundle =
        init_bundle::validate(payload).map_err(|_| PackageAcceptanceError::BadInitBundle)?;
    let first_record = bundle
        .record_at(init_bundle::RecordType::PackageSource, 0)
        .ok_or(PackageAcceptanceError::MissingSource)?;
    let source = package_source_from_record(first_record.bytes(), 0)?;

    if source.label == FORMAT_FIXTURE_LABEL {
        validate_package_sources(bytes)?;
        return run_format_acceptance();
    }
    if source.label == INSTALL_SUCCESS_LABEL {
        return run_install_success_acceptance(block_device, &bundle);
    }
    if source.label == INSTALL_SOURCE_DENIED_LABEL {
        return run_install_source_denied_acceptance(block_device, &bundle);
    }
    if source.label == RESTORE_STACK_SMOKE_LABEL {
        return seed_restore_stack_smoke_world(block_device, &bundle);
    }

    Err(PackageAcceptanceError::UnexpectedScenario)
}

pub fn run_restore_stack_smoke(
    block_device: BlockDeviceInfo,
) -> Result<(), PackageAcceptanceError> {
    let (expected_registry_generation, expected_object_checkpoint, expected_content_bitmap) = {
        let seed = restore_smoke_seed_for_acceptance();
        (
            seed.registry_generation,
            seed.object_checkpoint,
            seed.content_commit.committed_bitmap(),
        )
    };
    let restored_service = restore_service_for_acceptance();
    let report = restored_service
        .restore_from_storage(block_device)
        .map_err(map_package_status)?;
    if !report.published_world_selected
        || report.previous_published_world_selected
        || report
            .selected_object_checkpoint()
            .map(|(identity, _)| identity)
            != Some(expected_object_checkpoint)
    {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    if restored_service.registry().generation() != expected_registry_generation.generation
        || restored_service.registry().root_digest() != expected_registry_generation.root_digest
        || restored_service.registry().package_count() != 1
        || restored_service.registry().schema_count() != 1
        || restored_service.content_store().committed_bitmap() != expected_content_bitmap
    {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    serial::write_line("PYTHOS:CORE:PACKAGE_RESTORE_STACK_SAFE");
    Ok(())
}

pub fn restore_stack_smoke_requested() -> bool {
    RESTORE_STACK_SMOKE_REQUESTED.load(Ordering::Acquire)
}

fn run_format_acceptance() -> Result<(), PackageAcceptanceError> {
    serial::write_line("PYTHOS:CORE:PACKAGE_SOURCE_READY");
    serial::write_line("PYTHOS:CORE:PACKAGE_FORMAT:VALID");
    if PackageArtifactV0::parse(b"not-a-package").is_err() {
        serial::write_line("PYTHOS:CORE:PACKAGE_FORMAT:INVALID_DENIED");
    } else {
        return Err(PackageAcceptanceError::InvalidPackageWasAccepted);
    }
    serial::write_line("PYTHOS:CORE:PACKAGE_FORMAT_READY");
    Ok(())
}

fn run_install_success_acceptance(
    block_device: BlockDeviceInfo,
    bundle: &init_bundle::InitBundle<'_>,
) -> Result<(), PackageAcceptanceError> {
    let source_service = package_source_service_for_acceptance(bundle)?;
    let source_handle = source_service
        .handle_at(0)
        .ok_or(PackageAcceptanceError::MissingSource)?;
    let caller = ActiveUserProcess::new(
        PACKAGE_INSTALL_SERVICE_ID,
        PACKAGE_INSTALL_PRINCIPAL_ID,
        PACKAGE_INSTALL_PROGRAM_DIGEST,
    );
    let capabilities = capabilities_for_acceptance();
    let source_read_capability = capabilities
        .grant(
            caller.service_id(),
            ResourceId::new(PACKAGE_SOURCE_RESOURCE_ID),
            RightsMask::new(PACKAGE_SOURCE_READ_RIGHT as u32),
        )
        .map_err(|_| PackageAcceptanceError::PackageOperation)?;
    let install_capability = capabilities
        .grant(
            caller.service_id(),
            ResourceId::new(PACKAGE_INSTALL_RESOURCE_ID),
            RightsMask::new(PACKAGE_INSTALL_RIGHT as u32),
        )
        .map_err(|_| PackageAcceptanceError::PackageOperation)?;
    serial::write_line("PYTHOS:CORE:PACKAGE_SOURCE_AUTHORITY_READY");

    let object_service = object_service_for_acceptance(block_device)?;
    let package_service = package_service_for_acceptance();
    let artifact_buffer = acceptance_artifact_buffer();
    let installed = install_result_for_acceptance();
    package_service
        .install_into(
            PackageInstallRequest {
                caller,
                source_handle,
                source_read_capability,
                install_capability,
            },
            source_service,
            capabilities,
            object_service,
            artifact_buffer,
            installed,
        )
        .map_err(map_package_status)?;

    serial::write_line("PYTHOS:CORE:PACKAGE_INSTALL:STAGED");
    if installed.content_commit.record_count() == 0
        || package_service.registry().package_count() != 1
        || package_service.registry().schema_count() != 1
    {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    serial::write_line("PYTHOS:CORE:PACKAGE_INSTALL:COMMITTED");
    crate::package_registry::PackageTransactionCommitV0::decode(
        &installed.anchor_bytes,
        installed.registry_generation,
        installed.object_checkpoint_identity,
    )
    .map_err(map_package_status)?;
    serial::write_line("PYTHOS:CORE:PACKAGE_TRANSACTION_ANCHOR_READY");

    let mirrors = locator_mirrors_for_acceptance();
    package_service
        .rebuild_locator_mirrors_into(mirrors)
        .map_err(map_package_status)?;
    if mirrors.relationship_count() != 2 {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    serial::write_line("PYTHOS:CORE:PACKAGE_INSTALL_READY");
    Ok(())
}

fn seed_restore_stack_smoke_world(
    block_device: BlockDeviceInfo,
    bundle: &init_bundle::InitBundle<'_>,
) -> Result<(), PackageAcceptanceError> {
    let source_service = package_source_service_for_acceptance(bundle)?;
    let source_handle = source_service
        .handle_at(0)
        .ok_or(PackageAcceptanceError::MissingSource)?;
    let caller = ActiveUserProcess::new(
        PACKAGE_INSTALL_SERVICE_ID,
        PACKAGE_INSTALL_PRINCIPAL_ID,
        PACKAGE_INSTALL_PROGRAM_DIGEST,
    );
    let capabilities = capabilities_for_acceptance();
    let source_read_capability = capabilities
        .grant(
            caller.service_id(),
            ResourceId::new(PACKAGE_SOURCE_RESOURCE_ID),
            RightsMask::new(PACKAGE_SOURCE_READ_RIGHT as u32),
        )
        .map_err(|_| PackageAcceptanceError::PackageOperation)?;
    let object_service = object_service_for_acceptance(block_device)?;
    let artifact_buffer: &'static mut [u8] = acceptance_artifact_buffer();
    let artifact_len = source_service
        .read(
            caller.service_id(),
            capabilities,
            source_handle,
            source_read_capability,
            artifact_buffer,
        )
        .map_err(map_package_status)?;
    let artifact = PackageArtifactV0::parse(&artifact_buffer[..artifact_len])
        .map_err(|_| PackageAcceptanceError::BadPackageFormat)?;
    let descriptor_entry = artifact
        .content_entry(0)
        .ok_or(PackageAcceptanceError::PackageOperation)?;
    let descriptor_bytes = artifact
        .content_bytes(descriptor_entry)
        .map_err(|_| PackageAcceptanceError::PackageOperation)?;
    if descriptor_bytes.is_empty() {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    let seed = restore_smoke_seed_for_acceptance();
    seed.content_store.rollback(&mut seed.content);
    seed.content
        .reset(RESTORE_SMOKE_PACKAGE_OBJECT_ID, artifact.artifact_sha256());
    let mut descriptor_content_id = None;
    let mut entry_index = 0u16;
    while let Some(entry) = artifact.content_entry(entry_index) {
        let content_id = seed
            .content_store
            .stage_content(
                &mut seed.content,
                entry.role,
                entry.format,
                artifact
                    .content_bytes(entry)
                    .map_err(|_| PackageAcceptanceError::PackageOperation)?,
                entry.sha256,
            )
            .map_err(map_package_status)?;
        if entry_index == descriptor_entry.content_index {
            descriptor_content_id = Some(content_id);
        }
        entry_index = entry_index.wrapping_add(1);
    }
    let schema_descriptor_content_id =
        descriptor_content_id.ok_or(PackageAcceptanceError::PackageOperation)?;
    let package = object_service
        .create_package_object(
            caller,
            ObjectId::new(RESTORE_SMOKE_PACKAGE_OBJECT_ID),
            artifact.artifact_sha256(),
        )
        .map_err(map_object_error)?;
    let schema = object_service
        .create_schema_definition_object(
            caller,
            ObjectId::new(RESTORE_SMOKE_SCHEMA_OBJECT_ID),
            package.object_id,
            descriptor_entry.sha256,
        )
        .map_err(map_object_error)?;
    seed.registry.clear_to_empty();
    seed.registry
        .begin_candidate_generation(RESTORE_SMOKE_TRANSACTION_ID)
        .map_err(map_package_status)?;
    seed.registry
        .add_package_record(PackageRegistryPackageRecord::new(
            package.object_id.raw(),
            package.revision,
            artifact.artifact_sha256(),
            PackageStatus::Ok as u16,
        ))
        .map_err(map_package_status)?;
    seed.registry
        .add_schema_record(PackageRegistrySchemaRecord::new(
            schema.object_id.raw(),
            schema.revision,
            package.object_id.raw(),
            schema_descriptor_content_id.content_index,
            descriptor_entry.sha256,
        ))
        .map_err(map_package_status)?;
    seed.content_store
        .add_staged_records_to_registry(&seed.content, &mut seed.registry)
        .map_err(map_package_status)?;
    seed.content_commit = seed
        .content_store
        .write_candidate_content(block_device, &seed.content)
        .map_err(map_package_status)?;
    seed.registry_generation = write_candidate_registry_generation_into(
        block_device,
        &seed.registry,
        &mut seed.registry_snapshot,
        &mut seed.decoded_registry,
    )
    .map_err(map_package_status)?;
    object_service
        .encode_snapshot_into(&mut seed.object_snapshot)
        .map_err(map_object_error)?;
    seed.object_snapshot.generation = seed.object_snapshot.generation.wrapping_add(1);
    seed.object_checkpoint =
        write_object_service_candidate_checkpoint(block_device, &seed.object_snapshot)
            .map_err(|_| PackageAcceptanceError::PackageOperation)?
            .identity;
    let anchor = PackageTransactionCommitV0::new(
        RESTORE_SMOKE_TRANSACTION_ID,
        INSTALL_OPERATION,
        seed.registry_generation,
        seed.object_checkpoint,
        package.object_id.raw(),
        package.revision,
    );
    write_publication_anchor(block_device, anchor).map_err(map_package_status)?;
    if seed.content_commit.record_count() == 0
        || seed.registry_generation.generation == 0
        || seed.object_checkpoint.generation == 0
    {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    RESTORE_STACK_SMOKE_REQUESTED.store(true, Ordering::Release);
    serial::write_line("PYTHOS:CORE:PACKAGE_RESTORE_STACK_SEEDED");
    Ok(())
}

fn run_install_source_denied_acceptance(
    block_device: BlockDeviceInfo,
    bundle: &init_bundle::InitBundle<'_>,
) -> Result<(), PackageAcceptanceError> {
    let source_service = package_source_service_for_acceptance(bundle)?;
    let source_handle = source_service
        .handle_at(0)
        .ok_or(PackageAcceptanceError::MissingSource)?;
    let caller = ActiveUserProcess::new(
        PACKAGE_INSTALL_SERVICE_ID,
        PACKAGE_INSTALL_PRINCIPAL_ID,
        PACKAGE_INSTALL_PROGRAM_DIGEST,
    );
    let capabilities = capabilities_for_acceptance();
    let install_capability = capabilities
        .grant(
            caller.service_id(),
            ResourceId::new(PACKAGE_INSTALL_RESOURCE_ID),
            RightsMask::new(PACKAGE_INSTALL_RIGHT as u32),
        )
        .map_err(|_| PackageAcceptanceError::PackageOperation)?;
    let object_service = object_service_for_acceptance(block_device)?;
    let package_service = package_service_for_acceptance();
    let artifact_buffer = acceptance_artifact_buffer();
    let denied_result = install_result_for_acceptance();
    let denied = package_service.install_into(
        PackageInstallRequest {
            caller,
            source_handle,
            source_read_capability: CapabilityHandle::from_parts(31, 77),
            install_capability,
        },
        source_service,
        capabilities,
        object_service,
        artifact_buffer,
        denied_result,
    );

    if denied == Err(PackageStatus::SourceReadDenied) {
        serial::write_line("PYTHOS:CORE:PACKAGE_SOURCE:DENIED");
        Ok(())
    } else {
        Err(PackageAcceptanceError::PackageOperation)
    }
}

fn validate_package_sources(bytes: &[u8]) -> Result<(), PackageAcceptanceError> {
    let payload = init_pak_payload(bytes)?;
    let bundle =
        init_bundle::validate(payload).map_err(|_| PackageAcceptanceError::BadInitBundle)?;
    let mut ordinal = 0usize;
    while let Some(record) = bundle.record_at(init_bundle::RecordType::PackageSource, ordinal) {
        if ordinal >= MAX_PACKAGE_SOURCES {
            return Err(PackageAcceptanceError::TooManySources);
        }
        let source = package_source_from_record(record.bytes(), ordinal)?;
        PackageArtifactV0::parse(source.artifact)
            .map_err(|_| PackageAcceptanceError::BadPackageFormat)?;
        ordinal += 1;
    }
    if ordinal == 0 {
        return Err(PackageAcceptanceError::MissingSource);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PackageSourceView<'a> {
    label: &'a [u8],
    artifact: &'a [u8],
}

fn package_source_from_record(
    bytes: &[u8],
    ordinal: usize,
) -> Result<PackageSourceView<'_>, PackageAcceptanceError> {
    if bytes.len() < PACKAGE_SOURCE_HEADER_LEN {
        return Err(PackageAcceptanceError::BadSource);
    }
    if &bytes[..PACKAGE_SOURCE_MAGIC.len()] != PACKAGE_SOURCE_MAGIC {
        return Err(PackageAcceptanceError::BadSource);
    }
    let source_id = usize::from(read_u16(bytes, 8)?);
    if source_id != ordinal {
        return Err(PackageAcceptanceError::BadSource);
    }
    let label_len = usize::from(read_u16(bytes, 10)?);
    if label_len == 0 || label_len > MAX_PACKAGE_SOURCE_LABEL_BYTES {
        return Err(PackageAcceptanceError::BadSource);
    }
    if bytes[12..16].iter().any(|&byte| byte != 0) {
        return Err(PackageAcceptanceError::BadSource);
    }
    let artifact_offset =
        usize::try_from(read_u64(bytes, 16)?).map_err(|_| PackageAcceptanceError::BadSource)?;
    let artifact_len =
        usize::try_from(read_u64(bytes, 24)?).map_err(|_| PackageAcceptanceError::BadSource)?;
    if artifact_len == 0 || artifact_len > MAX_PACKAGE_ARTIFACT_BYTES {
        return Err(PackageAcceptanceError::BadSource);
    }
    if artifact_offset != PACKAGE_SOURCE_HEADER_LEN + label_len {
        return Err(PackageAcceptanceError::BadSource);
    }
    let artifact_end = artifact_offset
        .checked_add(artifact_len)
        .ok_or(PackageAcceptanceError::BadSource)?;
    if artifact_end != bytes.len() {
        return Err(PackageAcceptanceError::BadSource);
    }
    let label = bytes
        .get(PACKAGE_SOURCE_HEADER_LEN..artifact_offset)
        .ok_or(PackageAcceptanceError::BadSource)?;
    let artifact = bytes
        .get(artifact_offset..artifact_end)
        .ok_or(PackageAcceptanceError::BadSource)?;
    let mut expected_digest = [0u8; 32];
    expected_digest.copy_from_slice(&bytes[32..64]);
    if sha256(artifact) != expected_digest {
        return Err(PackageAcceptanceError::BadSourceDigest);
    }
    Ok(PackageSourceView { label, artifact })
}

fn object_service_for_acceptance(
    block_device: BlockDeviceInfo,
) -> Result<&'static mut ObjectService, PackageAcceptanceError> {
    // SAFETY:
    // 1. Invariant: this Phase 13 acceptance path runs once on one boot CPU and
    //    exits QEMU after emitting its terminal marker.
    // 2. Established by: `phase13-package-test` is a verify-only feature and
    //    `main.rs` calls this module from the single-threaded verify path.
    // 3. Lifetime: the static `MaybeUninit<ObjectService>` slot lives for the
    //    remainder of the boot and is not exposed outside package acceptance.
    // 4. Pointer ownership: no other mutable or shared reference is created for
    //    this slot while the acceptance scenario is running.
    // 5. Alignment: `MaybeUninit<ObjectService>` provides correct alignment.
    // 6. Mapped length: exactly one `ObjectService` value is initialized.
    // 7. Concurrency: no SMP or concurrent acceptance runner exists.
    // 8. Violation: reentry would create aliasing and must panic through the
    //    QEMU acceptance path rather than proceed.
    let slot = unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_OBJECT_SERVICE) };
    #[cfg(not(test))]
    ObjectService::restore_or_initialize_in_place(slot, block_device).map_err(map_object_error)?;
    #[cfg(test)]
    {
        let _ = block_device;
        slot.write(ObjectService::new_for_test());
    }
    // SAFETY:
    // 1. Invariant: `restore_or_initialize_in_place` initialized every field in
    //    `slot` or returned an error.
    // 2. Established by: the call immediately above succeeded.
    // 3. Lifetime: the returned reference is bounded by the one-shot QEMU
    //    acceptance path and the backing static lives long enough.
    // 4. Pointer ownership: package acceptance holds the only mutable reference.
    // 5. Alignment: inherited from the `MaybeUninit<ObjectService>` slot.
    // 6. Mapped length: one initialized object service value is referenced.
    // 7. Concurrency: single-core verify path.
    // 8. Violation: a failed initializer cannot reach this conversion.
    Ok(unsafe { &mut *slot.as_mut_ptr() })
}

fn acceptance_artifact_buffer() -> &'static mut [u8] {
    // SAFETY:
    // 1. Invariant: package acceptance is a one-shot verify path and uses the
    //    buffer for exactly one install attempt before QEMU exits.
    // 2. Established by: `phase13-package-test` dispatch is single-scenario and
    //    not callable by ring-3 code.
    // 3. Lifetime: the static buffer outlives `PackageService` content refs.
    // 4. Pointer ownership: this function is called once per boot scenario.
    // 5. Alignment: byte buffers require only `u8` alignment.
    // 6. Mapped length: the returned slice covers exactly the static buffer.
    // 7. Concurrency: no SMP or interrupt writer touches the buffer.
    // 8. Violation: reentry would alias mutable access and must not occur.
    unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER) }
}

fn package_service_for_acceptance() -> &'static mut PackageService<'static> {
    // SAFETY:
    // 1. Invariant: the package service static is used only by one Phase 13
    //    QEMU acceptance scenario per boot.
    // 2. Established by: `phase13-package-test` dispatch exits QEMU after the
    //    scenario terminal marker.
    // 3. Lifetime: the static service and static artifact buffer both live for
    //    the remainder of the boot.
    // 4. Pointer ownership: this is the only mutable borrow of the service.
    // 5. Alignment: the static item has `PackageService` alignment.
    // 6. Mapped length: exactly one `PackageService` value is referenced.
    // 7. Concurrency: single-core verify path, no reentrant package acceptance.
    // 8. Violation: reentry would alias mutable state and must not occur.
    unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_PACKAGE_SERVICE) }
}

fn restore_service_for_acceptance() -> &'static mut PackageService<'static> {
    // SAFETY:
    // 1. Invariant: the restore smoke is a one-shot verify scenario and owns
    //    this separate empty service until QEMU exits.
    // 2. Established by: the dedicated package-source label dispatches once.
    // 3. Lifetime: the static service lives for the remainder of the boot.
    // 4. Pointer ownership: no other reference to this service is created.
    // 5. Alignment: the static item has `PackageService` alignment.
    // 6. Mapped length: exactly one `PackageService` value is referenced.
    // 7. Concurrency: the verify path is single-core and non-reentrant.
    // 8. Violation: reentry would alias mutable service state.
    unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_RESTORE_SERVICE) }
}

fn restore_smoke_seed_for_acceptance() -> &'static mut PackageRestoreSmokeSeed {
    // SAFETY:
    // 1. Invariant: the restore-stack smoke seeds exactly one publication world.
    // 2. Established by: its dedicated source label runs once before restore.
    // 3. Lifetime: the retained seed state lives until QEMU exits.
    // 4. Pointer ownership: this helper creates the only mutable seed reference.
    // 5. Alignment: the static item has `PackageRestoreSmokeSeed` alignment.
    // 6. Mapped length: exactly one complete seed workspace is referenced.
    // 7. Concurrency: the verify path is single-core and non-reentrant.
    // 8. Violation: reentry could combine facts from different seed worlds.
    unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_RESTORE_SEED) }
}

fn capabilities_for_acceptance() -> &'static mut CapabilityTable {
    // SAFETY:
    // 1. Invariant: Phase 13 package acceptance runs one scenario per boot and
    //    owns this capability table until QEMU exits.
    // 2. Established by: `phase13-package-test` dispatch is verify-only,
    //    single-threaded, and does not expose the table to ring-3 code.
    // 3. Lifetime: the static table lives for the remainder of the boot.
    // 4. Pointer ownership: this helper returns the only mutable reference for
    //    the active acceptance scenario.
    // 5. Alignment: the static item has `CapabilityTable` alignment.
    // 6. Mapped length: exactly one capability table is referenced.
    // 7. Concurrency: no SMP or reentrant package acceptance path exists.
    // 8. Violation: reentry would alias mutable capability state.
    let table = unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_CAPABILITIES) };
    table.clear();
    table
}

fn package_source_service_for_acceptance(
    bundle: &init_bundle::InitBundle<'_>,
) -> Result<&'static PackageSourceService<'static>, PackageAcceptanceError> {
    // SAFETY:
    // 1. Invariant: `bundle` contains records whose bytes are slices into the
    //    loader-retained INIT.PAK payload for the full acceptance boot.
    // 2. Established by: `init_pak_bytes` reads `boot_info.init_bundle_phys`,
    //    which is retained and mapped for the whole kernel lifetime.
    // 3. Lifetime: the returned source service is used only until QEMU exits
    //    this single acceptance scenario.
    // 4. Pointer ownership: no writer mutates the INIT.PAK bytes.
    // 5. Alignment: only byte slices are stored.
    // 6. Mapped length: package-source validation bounds-checks every slice.
    // 7. Concurrency: no SMP or reentrant package-source loader exists.
    // 8. Violation: if INIT.PAK were not retained, source reads would fault
    //    or digest validation would fail rather than grant authority.
    let static_bundle: &init_bundle::InitBundle<'static> = unsafe { core::mem::transmute(bundle) };
    // SAFETY:
    // 1. Invariant: Phase 13 package acceptance owns this static service for
    //    one scenario per boot.
    // 2. Established by: the verify-only dispatcher exits QEMU after the
    //    scenario terminal marker.
    // 3. Lifetime: the static service outlives the acceptance path.
    // 4. Pointer ownership: this helper returns the only reference for the
    //    active scenario after loading completes.
    // 5. Alignment: the static item has `PackageSourceService` alignment.
    // 6. Mapped length: exactly one source-service value is referenced.
    // 7. Concurrency: single-core verify path.
    // 8. Violation: reentry would alias mutable package-source state.
    let service = unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_SOURCE_SERVICE) };
    service
        .load_from_init_bundle(static_bundle)
        .map_err(map_package_status)?;
    Ok(service)
}

fn install_result_for_acceptance() -> &'static mut PackageInstallResult {
    // SAFETY:
    // 1. Invariant: this statically initialized slot contains a valid empty
    //    `PackageInstallResult` before any mutable reference is formed.
    // 2. Established by: `PackageInstallResult::empty()` is a const initializer.
    //    `install_into` may overwrite it on success; denial scenarios retain
    //    the initialized empty value and do not inspect success-only fields.
    // 3. Lifetime: the static result slot lives for the remainder of the boot.
    // 4. Pointer ownership: package acceptance holds the only mutable
    //    reference for the active scenario.
    // 5. Alignment: the static item has `PackageInstallResult` alignment.
    // 6. Mapped length: exactly one result object is referenced.
    // 7. Concurrency: no SMP or reentrant package acceptance path exists.
    // 8. Violation: reentry would alias mutable access and must not occur.
    unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_INSTALL_RESULT) }
}

fn locator_mirrors_for_acceptance() -> &'static mut PackageLocatorRelationshipStore {
    // SAFETY:
    // 1. Invariant: Phase 13 package acceptance owns this mirror store for one
    //    scenario and rebuilds it before reading the relationship count.
    // 2. Established by: `phase13-package-test` is a one-shot QEMU path.
    // 3. Lifetime: the static mirror store lives for the whole boot.
    // 4. Pointer ownership: this helper returns the only mutable reference.
    // 5. Alignment: the static item has `PackageLocatorRelationshipStore`
    //    alignment.
    // 6. Mapped length: exactly one mirror store is referenced.
    // 7. Concurrency: single-core verify path.
    // 8. Violation: reentry would alias mutable mirror state.
    unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_LOCATOR_MIRRORS) }
}

fn map_package_status(_status: PackageStatus) -> PackageAcceptanceError {
    PackageAcceptanceError::PackageOperation
}

fn map_object_error(_error: ObjectServiceError) -> PackageAcceptanceError {
    PackageAcceptanceError::ObjectService
}

fn init_pak_payload(bytes: &[u8]) -> Result<&[u8], PackageAcceptanceError> {
    let header = init_pak::validate(bytes).map_err(|_| PackageAcceptanceError::BadInitPak)?;
    let payload_start =
        usize::try_from(header.header_len).map_err(|_| PackageAcceptanceError::BadInitPak)?;
    let payload_len =
        usize::try_from(header.payload_len).map_err(|_| PackageAcceptanceError::BadInitPak)?;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or(PackageAcceptanceError::BadInitPak)?;
    bytes
        .get(payload_start..payload_end)
        .ok_or(PackageAcceptanceError::BadInitPak)
}

fn init_pak_bytes(boot_info: &PythBootInfo) -> Result<&[u8], PackageAcceptanceError> {
    let len = usize::try_from(boot_info.init_bundle_len)
        .map_err(|_| PackageAcceptanceError::BadInitPak)?;
    if boot_info.init_bundle_phys == 0 || len == 0 {
        return Err(PackageAcceptanceError::BadInitPak);
    }
    // SAFETY:
    // 1. Invariant: `init_bundle_phys` points to the loader-retained INIT.PAK
    //    byte range already mapped readable by the active verify-path page
    //    tables.
    // 2. Established by: boot metadata validation and the VM switch completed
    //    before Phase 13 package-format acceptance runs.
    // 3. Lifetime: the loader allocation is retained for the whole boot.
    // 4. Pointer ownership: PythCore owns the allocation and reads it
    //    immutably here.
    // 5. Alignment: byte-slice reads require only `u8` alignment.
    // 6. Mapped length: `init_bundle_len` bytes were mapped into the kernel
    //    address space.
    // 7. Concurrency: single-core verify path; no writer exists.
    // 8. Violation: an invalid mapping faults or fails package-source checks.
    Ok(unsafe { core::slice::from_raw_parts(boot_info.init_bundle_phys as *const u8, len) })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, PackageAcceptanceError> {
    let raw = bytes
        .get(offset..offset + 2)
        .ok_or(PackageAcceptanceError::BadSource)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, PackageAcceptanceError> {
    let raw = bytes
        .get(offset..offset + 8)
        .ok_or(PackageAcceptanceError::BadSource)?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}
