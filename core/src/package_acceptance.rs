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
    block_device::{BlockDeviceInfo, SECTOR_SIZE},
    capabilities::{CapabilityHandle, CapabilityTable, ResourceId, RightsMask},
    object_relationships::{
        PACKAGE_LOCATOR_ROOT_OBJECT_ID, PackageLocatorRelationshipStore, RelationshipKind,
    },
    object_service::{ObjectService, ObjectServiceError},
    object_service_checkpoint::{
        ObjectCheckpointIdentity, ObjectServiceSnapshot,
        read_object_service_candidate_checkpoint_into, write_object_service_candidate_checkpoint,
    },
    package_candidate_store::{
        PACKAGE_CANDIDATE_REGISTRY_SLOT_A_SECTOR, PACKAGE_CANDIDATE_REGISTRY_SLOT_B_SECTOR,
        PACKAGE_CANDIDATE_REGISTRY_SLOT_SECTORS, PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES,
        PackagePublicationAnchorSlot, read_candidate_registry_generation_into,
        read_package_candidate_sector, read_publication_anchor_slot,
        write_candidate_registry_generation_into, write_publication_anchor,
    },
    package_content_store::{
        ContentId, PackageContentCommit, PackageContentStore, PackageContentTransaction,
    },
    package_registry::{
        PackageRegistry, PackageRegistryGeneration, PackageRegistryPackageRecord,
        PackageRegistrySchemaRecord, PackageTransactionCommitV0,
    },
    package_service::{
        PackageInstallCandidate, PackageInstallRequest, PackageInstallResult, PackageService,
    },
    package_source::PackageSourceService,
    process_context::ActiveUserProcess,
    serial,
    service_identity::ServiceId,
    shell_objects::{ObjectId, ObjectKind},
};
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(test))]
use core::{arch::global_asm, cell::UnsafeCell};

const PACKAGE_SOURCE_MAGIC: &[u8; 8] = b"PYPKGS01";
const PACKAGE_SOURCE_HEADER_LEN: usize = 64;
const PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER_BYTES: usize = 4096;
const FORMAT_FIXTURE_LABEL: &[u8] = b"phase13-format-fixture.pkg";
const INSTALL_SUCCESS_LABEL: &[u8] = b"phase13-install-success.pkg";
const INSTALL_SOURCE_DENIED_LABEL: &[u8] = b"phase13-install-source-denied.pkg";
const RESTORE_STACK_SMOKE_LABEL: &[u8] = b"phase13-restore-stack-smoke.pkg";
const KILL_BEFORE_ANCHOR_LABEL: &[u8] = b"phase13-kill-before-anchor-a.pkg";
const KILL_AFTER_ANCHOR_BEFORE_MIRROR_LABEL: &[u8] =
    b"phase13-kill-after-anchor-before-mirror-a.pkg";
const PACKAGE_INSTALL_SERVICE_ID: ServiceId = ServiceId::from_raw(0x5059_504B_494E_5354);
const PACKAGE_INSTALL_PRINCIPAL_ID: u64 = 0x5059_504B_494E_5354;
const PACKAGE_INSTALL_PROGRAM_DIGEST: u64 = 0x5059_0013;
const RESTORE_SMOKE_TRANSACTION_ID: u64 = 1;
const RESTORE_SMOKE_PACKAGE_OBJECT_ID: u64 = 0x5059_504B_474F_0001;
const RESTORE_SMOKE_SCHEMA_OBJECT_ID: u64 = 0x5059_5343_484F_0001;
const INSTALL_OPERATION: u16 = 1;
#[cfg(not(test))]
const PACKAGE_ACCEPTANCE_STACK_BYTES: usize = 2 * 1024 * 1024;
#[cfg(not(test))]
const PACKAGE_STACK_MODE_INSTALL_SUCCESS: u8 = 1;
#[cfg(not(test))]
const PACKAGE_STACK_MODE_INSTALL_SOURCE_DENIED: u8 = 2;
#[cfg(not(test))]
const PACKAGE_STACK_MODE_PUBLICATION_BEFORE_ANCHOR: u8 = 3;
#[cfg(not(test))]
const PACKAGE_STACK_MODE_PUBLICATION_AFTER_ANCHOR: u8 = 4;

static mut PACKAGE_ACCEPTANCE_OBJECT_SERVICE: MaybeUninit<ObjectService> = MaybeUninit::uninit();
static mut PACKAGE_ACCEPTANCE_PACKAGE_SERVICE: PackageService<'static> =
    PackageService::new_empty_for_test();
static mut PACKAGE_ACCEPTANCE_RESTORE_SERVICE: PackageService<'static> =
    PackageService::new_empty_for_test();
static mut PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER: [u8; PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER_BYTES] =
    [0; PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER_BYTES];
static mut PACKAGE_ACCEPTANCE_SECOND_ARTIFACT_BUFFER: [u8;
    PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER_BYTES] = [0; PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER_BYTES];
static mut PACKAGE_ACCEPTANCE_CAPABILITIES: CapabilityTable = CapabilityTable::new();
static mut PACKAGE_ACCEPTANCE_SOURCE_SERVICE: PackageSourceService<'static> =
    PackageSourceService::empty();
static mut PACKAGE_ACCEPTANCE_INSTALL_RESULT: PackageInstallResult = PackageInstallResult::empty();
static mut PACKAGE_ACCEPTANCE_LOCATOR_MIRRORS: PackageLocatorRelationshipStore =
    PackageLocatorRelationshipStore::new();
static mut PACKAGE_ACCEPTANCE_DECODED_REGISTRY: PackageRegistry = PackageRegistry::empty();
static mut PACKAGE_ACCEPTANCE_REGISTRY_SNAPSHOT: [u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES] =
    [0; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES];
static mut PACKAGE_ACCEPTANCE_OBJECT_SNAPSHOT: ObjectServiceSnapshot =
    ObjectServiceSnapshot::empty();
static mut PACKAGE_ACCEPTANCE_CONTENT_READ_BUFFER: [u8; PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER_BYTES] =
    [0; PACKAGE_ACCEPTANCE_ARTIFACT_BUFFER_BYTES];
static mut PACKAGE_ACCEPTANCE_RESTORE_SEED: PackageRestoreSmokeSeed =
    PackageRestoreSmokeSeed::empty();
static RESTORE_STACK_SMOKE_REQUESTED: AtomicBool = AtomicBool::new(false);

#[cfg(not(test))]
#[repr(align(4096))]
struct PackageAcceptanceStack([u8; PACKAGE_ACCEPTANCE_STACK_BYTES]);

#[cfg(not(test))]
struct PackageAcceptanceStackStorage(UnsafeCell<PackageAcceptanceStack>);

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: the Task 2.13 acceptance stack is entered synchronously from
//    the single-core verify path and is not shared with interrupts or tasks.
// 2. Established by: `phase13-package-test` runs before QEMU exits and no SMP
//    or reentrant package acceptance entry exists.
// 3. Lifetime: the static stack lives for the whole kernel boot.
// 4. Pointer ownership: this module switches to the stack only through
//    `package_acceptance_call_on_stack_abi`.
// 5. Alignment: `repr(align(16))` satisfies the x86-64 C ABI stack alignment.
// 6. Mapped length: exactly `PACKAGE_ACCEPTANCE_STACK_BYTES` bytes are used.
// 7. Concurrency: single-core verify path; no concurrent user of this stack.
// 8. Violation: a reentrant switch would corrupt acceptance execution and must
//    fault rather than publish package state.
unsafe impl Sync for PackageAcceptanceStackStorage {}

#[cfg(not(test))]
static PACKAGE_ACCEPTANCE_STACK: PackageAcceptanceStackStorage = PackageAcceptanceStackStorage(
    UnsafeCell::new(PackageAcceptanceStack([0; PACKAGE_ACCEPTANCE_STACK_BYTES])),
);

#[cfg(not(test))]
#[repr(C)]
struct PackageStackAcceptanceContext {
    block_device: BlockDeviceInfo,
    bundle: *const init_bundle::InitBundle<'static>,
    mode: u8,
}

#[cfg(not(test))]
static mut PACKAGE_STACK_ACCEPTANCE_CONTEXT: MaybeUninit<PackageStackAcceptanceContext> =
    MaybeUninit::uninit();

#[cfg(not(test))]
unsafe extern "C" {
    fn package_acceptance_call_on_stack_abi(
        stack_top: u64,
        context: *mut core::ffi::c_void,
        entry: extern "C" fn(*mut core::ffi::c_void) -> u64,
    ) -> u64;
}

#[cfg(not(test))]
global_asm!(
    r#"
    .global package_acceptance_call_on_stack_abi
    package_acceptance_call_on_stack_abi:
        push rbx
        mov rbx, rsp
        mov rsp, rdi
        and rsp, -16
        mov rdi, rsi
        call rdx
        mov rsp, rbx
        pop rbx
        ret
    "#
);

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
    if source.label == KILL_BEFORE_ANCHOR_LABEL {
        return run_publication_boundary_acceptance(block_device, &bundle, false);
    }
    if source.label == KILL_AFTER_ANCHOR_BEFORE_MIRROR_LABEL {
        return run_publication_boundary_acceptance(block_device, &bundle, true);
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

fn run_publication_boundary_acceptance(
    block_device: BlockDeviceInfo,
    bundle: &init_bundle::InitBundle<'_>,
    publish_candidate_anchor: bool,
) -> Result<(), PackageAcceptanceError> {
    #[cfg(not(test))]
    {
        let mode = if publish_candidate_anchor {
            PACKAGE_STACK_MODE_PUBLICATION_AFTER_ANCHOR
        } else {
            PACKAGE_STACK_MODE_PUBLICATION_BEFORE_ANCHOR
        };
        return run_acceptance_on_static_stack(block_device, bundle, mode);
    }

    #[cfg(test)]
    {
        run_publication_boundary_acceptance_inner(block_device, bundle, publish_candidate_anchor)
    }
}

#[cfg(not(test))]
fn run_acceptance_on_static_stack(
    block_device: BlockDeviceInfo,
    bundle: &init_bundle::InitBundle<'_>,
    mode: u8,
) -> Result<(), PackageAcceptanceError> {
    // SAFETY:
    // 1. Invariant: package-source records in `bundle` point into the
    //    loader-retained INIT.PAK bytes.
    // 2. Established by: `run_package_format_acceptance` validated INIT.PAK and
    //    its InitBundle before dispatching this scenario.
    // 3. Lifetime: INIT.PAK is mapped and retained for the full verify boot.
    // 4. Pointer ownership: the bytes are immutable and owned by PythCore.
    // 5. Alignment: only byte slices inside the bundle are accessed.
    // 6. Mapped length: bundle validation checked record bounds.
    // 7. Concurrency: single-core acceptance path, no writer exists.
    // 8. Violation: stale bundle bytes would fail package-source validation or
    //    fault before any publication marker is emitted.
    let static_bundle: &init_bundle::InitBundle<'static> = unsafe { core::mem::transmute(bundle) };
    // SAFETY:
    // 1. Invariant: the context static is written for exactly one synchronous
    //    static-stack call and is not read afterward.
    // 2. Established by: `phase13-package-test` runs one scenario per boot and
    //    exits or waits for harness power cut.
    // 3. Lifetime: the context slot is static and the referenced bundle is
    //    loader-retained for the whole boot.
    // 4. Pointer ownership: this helper creates the only mutable context
    //    reference before passing it to the trampoline.
    // 5. Alignment: `MaybeUninit<PackageStackAcceptanceContext>`
    //    provides correct alignment.
    // 6. Mapped length: exactly one initialized context is written/read.
    // 7. Concurrency: no SMP or reentrant package acceptance path exists.
    // 8. Violation: overlapping context writes could run the wrong scenario and
    //    must not publish package state.
    let context_slot = unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_STACK_ACCEPTANCE_CONTEXT) };
    let context = context_slot.write(PackageStackAcceptanceContext {
        block_device,
        bundle: static_bundle as *const init_bundle::InitBundle<'static>,
        mode,
    });
    // SAFETY:
    // 1. Invariant: `PACKAGE_ACCEPTANCE_STACK` names a private static stack and
    //    `context` points to an initialized context for this call.
    // 2. Established by: the static stack declaration above and the context
    //    write immediately before this call.
    // 3. Lifetime: both static allocations live for the entire kernel boot.
    // 4. Pointer ownership: the assembly shim owns `RSP` only during the
    //    synchronous call and restores the caller stack before returning.
    // 5. Alignment: the stack wrapper is 16-byte aligned; the shim aligns the
    //    top before issuing `call`.
    // 6. Mapped length: the stack top is computed from exactly
    //    `PACKAGE_ACCEPTANCE_STACK_BYTES` bytes of static storage.
    // 7. Concurrency: single-core verify path; no interrupt or task uses this
    //    acceptance stack.
    // 8. Violation: a bad stack/context would fault before acceptance success.
    let result = unsafe {
        let stack_base = PACKAGE_ACCEPTANCE_STACK.0.get() as u64;
        let stack_top = stack_base + PACKAGE_ACCEPTANCE_STACK_BYTES as u64;
        package_acceptance_call_on_stack_abi(
            stack_top,
            context as *mut PackageStackAcceptanceContext as *mut core::ffi::c_void,
            package_acceptance_stack_entry,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(PackageAcceptanceError::PackageOperation)
    }
}

#[cfg(not(test))]
extern "C" fn package_acceptance_stack_entry(context: *mut core::ffi::c_void) -> u64 {
    // SAFETY:
    // 1. Invariant: `context` is the initialized static context written by
    //    `run_acceptance_on_static_stack`.
    // 2. Established by: the only caller is the assembly shim invoked above.
    // 3. Lifetime: the static context remains valid for the duration of this
    //    synchronous call.
    // 4. Pointer ownership: this entry reads and copies fields; it does not
    //    retain references after returning.
    // 5. Alignment: the pointer comes from `MaybeUninit::write`.
    // 6. Mapped length: exactly one context object is readable.
    // 7. Concurrency: no other acceptance call can mutate the context.
    // 8. Violation: an invalid context pointer faults before publication-ready.
    let context = unsafe { &*(context.cast::<PackageStackAcceptanceContext>()) };
    // SAFETY:
    // 1. Invariant: `bundle` points to the validated loader-retained INIT.PAK
    //    bundle supplied by the outer acceptance function.
    // 2. Established by: `run_acceptance_on_static_stack`
    //    writes only a pointer derived from a validated InitBundle.
    // 3. Lifetime: INIT.PAK remains mapped for the full boot.
    // 4. Pointer ownership: the bundle is read-only.
    // 5. Alignment: `InitBundle` is referenced through the original Rust
    //    reference's alignment.
    // 6. Mapped length: bundle validation checked record bounds.
    // 7. Concurrency: no writer mutates INIT.PAK bytes.
    // 8. Violation: an invalid pointer faults before success markers.
    let bundle = unsafe { &*context.bundle };
    let result = match context.mode {
        PACKAGE_STACK_MODE_INSTALL_SUCCESS => {
            run_install_success_acceptance_inner(context.block_device, bundle)
        }
        PACKAGE_STACK_MODE_INSTALL_SOURCE_DENIED => {
            run_install_source_denied_acceptance_inner(context.block_device, bundle)
        }
        PACKAGE_STACK_MODE_PUBLICATION_BEFORE_ANCHOR => {
            run_publication_boundary_acceptance_inner(context.block_device, bundle, false)
        }
        PACKAGE_STACK_MODE_PUBLICATION_AFTER_ANCHOR => {
            run_publication_boundary_acceptance_inner(context.block_device, bundle, true)
        }
        _ => Err(PackageAcceptanceError::UnexpectedScenario),
    };
    match result {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

fn run_publication_boundary_acceptance_inner(
    block_device: BlockDeviceInfo,
    bundle: &init_bundle::InitBundle<'_>,
    publish_candidate_anchor: bool,
) -> Result<(), PackageAcceptanceError> {
    if !publication_anchor_exists(block_device)? {
        let source_service = package_source_service_for_acceptance(bundle)?;
        return run_publication_boundary_boot_one(
            block_device,
            source_service,
            publish_candidate_anchor,
        );
    }

    let restored_service = restore_service_for_acceptance();
    {
        let report = restored_service
            .restore_from_storage(block_device)
            .map_err(map_package_status)?;
        if publish_candidate_anchor {
            validate_published_restore_report(&report)?;
        } else {
            validate_previous_restore_report(&report)?;
        }
    }

    if publish_candidate_anchor {
        validate_published_boundary_after_reboot(restored_service)?;
    } else {
        validate_previous_boundary_after_reboot(block_device, restored_service)?;
    }
    serial::write_line("PYTHOS:CORE:PACKAGE_PUBLICATION_BOUNDARY_READY");
    Ok(())
}

fn run_publication_boundary_boot_one(
    block_device: BlockDeviceInfo,
    source_service: &PackageSourceService<'_>,
    publish_candidate_anchor: bool,
) -> Result<(), PackageAcceptanceError> {
    let source_handle_a = source_service
        .handle_at(0)
        .ok_or(PackageAcceptanceError::MissingSource)?;
    let source_handle_b = source_service
        .handle_at(1)
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

    let object_service = object_service_for_acceptance(block_device)?;
    let package_service = package_service_for_acceptance();
    {
        let candidate_a = package_service
            .prepare_install_candidate(
                block_device,
                PackageInstallRequest {
                    caller,
                    source_handle: source_handle_a,
                    source_read_capability,
                    install_capability,
                },
                source_service,
                capabilities,
                object_service,
                acceptance_artifact_buffer(),
            )
            .map_err(map_package_status)?;
        package_service
            .publish_install_candidate(candidate_a, object_service)
            .map_err(map_package_status)?;
    }

    let candidate_b = package_service
        .prepare_install_candidate(
            block_device,
            PackageInstallRequest {
                caller,
                source_handle: source_handle_b,
                source_read_capability,
                install_capability,
            },
            source_service,
            capabilities,
            object_service,
            second_acceptance_artifact_buffer(),
        )
        .map_err(map_package_status)?;
    serial::write_line("PYTHOS:CORE:PACKAGE_CANDIDATE_READY");
    validate_durable_candidate(block_device, &candidate_b)?;
    serial::write_line("PYTHOS:CORE:PACKAGE_CANDIDATE_VALIDATED");

    if publish_candidate_anchor {
        let installed_b = package_service
            .publish_install_candidate(candidate_b, object_service)
            .map_err(map_package_status)?;
        PackageTransactionCommitV0::decode(
            &installed_b.anchor_bytes,
            installed_b.registry_generation,
            installed_b.object_checkpoint_identity,
        )
        .map_err(map_package_status)?;
        serial::write_line("PYTHOS:CORE:PACKAGE_ANCHOR_PUBLISHED");
    }

    wait_for_power_cut();
}

fn validate_durable_candidate(
    block_device: BlockDeviceInfo,
    candidate: &PackageInstallCandidate,
) -> Result<(), PackageAcceptanceError> {
    let registry = decoded_registry_for_acceptance();
    read_candidate_registry_generation_into(block_device, candidate.registry_generation, registry)
        .map_err(map_package_status)?;
    if registry.package_count() == 0
        || registry.schema_count() == 0
        || registry.content_count() == 0
    {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    PackageContentStore::read_validate_candidate_content(block_device, registry)
        .map_err(map_package_status)?;
    let snapshot = object_snapshot_for_acceptance();
    read_object_service_candidate_checkpoint_into(
        block_device,
        candidate.object_checkpoint_identity,
        snapshot,
    )
    .map_err(|_| PackageAcceptanceError::PackageOperation)?;
    if !snapshot_contains_current_object(
        snapshot,
        candidate.package_object_id,
        candidate.package_revision,
        ObjectKind::Package,
    ) || !snapshot_contains_current_object(
        snapshot,
        candidate.schema_object_id,
        candidate.schema_revision,
        ObjectKind::SchemaDefinition,
    ) {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    Ok(())
}

fn validate_previous_restore_report(
    report: &crate::package_service::PackageRecoveryReport<'_>,
) -> Result<(), PackageAcceptanceError> {
    if !report.published_world_selected
        || report.previous_published_world_selected
        || report.selected_object_checkpoint().is_none()
    {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    Ok(())
}

fn validate_previous_boundary_after_reboot(
    block_device: BlockDeviceInfo,
    restored_service: &mut PackageService<'_>,
) -> Result<(), PackageAcceptanceError> {
    let selected = restored_service.registry();
    if selected.package_count() != 1 || selected.schema_count() != 1 {
        return Err(PackageAcceptanceError::PackageOperation);
    }

    let candidate = decoded_registry_for_acceptance();
    read_unpublished_candidate_registry_into(block_device, selected, candidate)?;
    let b_package = package_record_absent_from_selected(selected, candidate)
        .ok_or(PackageAcceptanceError::PackageOperation)?;
    if schema_record_for_package(selected, b_package.package_object_id).is_some()
        || content_record_for_package(selected, b_package.package_object_id).is_some()
    {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    let b_schema = schema_record_for_package(candidate, b_package.package_object_id)
        .ok_or(PackageAcceptanceError::PackageOperation)?;
    if registry_contains_schema(selected, b_schema.schema_object_id) {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    let b_content = content_record_for_package(candidate, b_package.package_object_id)
        .ok_or(PackageAcceptanceError::PackageOperation)?;
    if !candidate_content_is_reclaimable(selected, b_content)? {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    if restored_service
        .content_store()
        .read_published(
            ContentId::new(
                b_content.package_object_id,
                b_content.release_digest,
                b_content.content_index,
            ),
            content_read_buffer_for_acceptance(),
        )
        .is_ok()
    {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    let anchor_slot = anchor_slot_for_generation(candidate.generation());
    if read_publication_anchor_slot(block_device, anchor_slot)
        .map_err(map_package_status)?
        .is_some()
    {
        return Err(PackageAcceptanceError::PackageOperation);
    }

    let mirrors = locator_mirrors_for_acceptance();
    restored_service
        .rebuild_locator_mirrors_into(mirrors)
        .map_err(map_package_status)?;
    if mirrors.has_object(ObjectId::new(b_package.package_object_id)) {
        serial::write_line("PYTHOS:CORE:PACKAGE_LOCATOR:VISIBLE");
        return Err(PackageAcceptanceError::PackageOperation);
    }

    serial::write_line("PYTHOS:CORE:PACKAGE_WORLD_SELECTED:PREVIOUS");
    serial::write_line("PYTHOS:CORE:PACKAGE_CANDIDATE:IGNORED_RECLAIMABLE");
    Ok(())
}

fn validate_published_restore_report(
    report: &crate::package_service::PackageRecoveryReport<'_>,
) -> Result<(), PackageAcceptanceError> {
    if !report.published_world_selected
        || report.previous_published_world_selected
        || !report.locator_mirrors_require_rebuild
        || report.selected_object_checkpoint().is_none()
    {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    serial::write_line("PYTHOS:CORE:PACKAGE_WORLD_SELECTED:PUBLISHED");
    Ok(())
}

fn validate_published_boundary_after_reboot(
    restored_service: &mut PackageService<'_>,
) -> Result<(), PackageAcceptanceError> {
    let registry = restored_service.registry();
    if registry.package_count() < 2 || registry.schema_count() < 2 {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    let b_package =
        newest_package_record(registry).ok_or(PackageAcceptanceError::PackageOperation)?;
    if schema_record_for_package(registry, b_package.package_object_id).is_none() {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    let b_content = content_record_for_package(registry, b_package.package_object_id)
        .ok_or(PackageAcceptanceError::PackageOperation)?;
    if !candidate_content_is_live(registry, b_content)? {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    let out = content_read_buffer_for_acceptance();
    let restored_len = restored_service
        .content_store()
        .read_published(
            ContentId::new(
                b_content.package_object_id,
                b_content.release_digest,
                b_content.content_index,
            ),
            out,
        )
        .map_err(map_package_status)?;
    let expected_len = usize::try_from(b_content.byte_len)
        .map_err(|_| PackageAcceptanceError::PackageOperation)?;
    if restored_len == 0
        || restored_len != expected_len
        || sha256(&out[..restored_len]) != b_content.digest
    {
        return Err(PackageAcceptanceError::PackageOperation);
    }

    let mirrors = locator_mirrors_for_acceptance();
    restored_service
        .rebuild_locator_mirrors_into(mirrors)
        .map_err(map_package_status)?;
    if !locator_resolves_package(mirrors, b_package.package_object_id) {
        return Err(PackageAcceptanceError::PackageOperation);
    }
    serial::write_line("PYTHOS:CORE:PACKAGE_MIRRORS_REBUILT");
    Ok(())
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
    #[cfg(not(test))]
    {
        return run_acceptance_on_static_stack(
            block_device,
            bundle,
            PACKAGE_STACK_MODE_INSTALL_SUCCESS,
        );
    }

    #[cfg(test)]
    {
        run_install_success_acceptance_inner(block_device, bundle)
    }
}

fn run_install_success_acceptance_inner(
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
    #[cfg(not(test))]
    {
        return run_acceptance_on_static_stack(
            block_device,
            bundle,
            PACKAGE_STACK_MODE_INSTALL_SOURCE_DENIED,
        );
    }

    #[cfg(test)]
    {
        run_install_source_denied_acceptance_inner(block_device, bundle)
    }
}

fn run_install_source_denied_acceptance_inner(
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
    {
        ObjectService::restore_or_initialize_in_place(slot, block_device)
            .map_err(map_object_error)?;
    }
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

fn second_acceptance_artifact_buffer() -> &'static mut [u8] {
    // SAFETY:
    // 1. Invariant: Task 2.13 owns this second package-source copy buffer for
    //    the candidate B install path in one QEMU acceptance boot.
    // 2. Established by: the two-boot harness runs one scenario per guest and
    //    kills Boot 1 after the requested marker.
    // 3. Lifetime: the static bytes outlive package content references kept by
    //    `PackageService` until Boot 1 is killed.
    // 4. Pointer ownership: this helper returns the only mutable slice for the
    //    B-source buffer.
    // 5. Alignment: byte buffers require only `u8` alignment.
    // 6. Mapped length: the returned slice covers exactly the static buffer.
    // 7. Concurrency: no SMP or reentrant package acceptance path exists.
    // 8. Violation: reusing the buffer concurrently could corrupt candidate
    //    content validation and must fail acceptance rather than publish.
    unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_SECOND_ARTIFACT_BUFFER) }
}

fn decoded_registry_for_acceptance() -> &'static mut PackageRegistry {
    // SAFETY:
    // 1. Invariant: this scratch registry is used synchronously by one
    //    acceptance validation path.
    // 2. Established by: `phase13-package-test` exits or waits for harness
    //    power cut after the active scenario.
    // 3. Lifetime: the static registry lives for the whole boot.
    // 4. Pointer ownership: this helper creates the only mutable reference.
    // 5. Alignment: the static item has `PackageRegistry` alignment.
    // 6. Mapped length: exactly one registry value is referenced.
    // 7. Concurrency: single-core verify path, no package acceptance reentry.
    // 8. Violation: overlapping decodes could mix selected and candidate
    //    worlds, invalidating the publication-boundary proof.
    unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_DECODED_REGISTRY) }
}

fn registry_snapshot_scratch_for_acceptance()
-> &'static mut [u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES] {
    // SAFETY:
    // 1. Invariant: raw candidate-slot scans borrow this byte buffer only
    //    during one synchronous decode.
    // 2. Established by: Task 2.13 acceptance performs one restore validation
    //    on one boot CPU before QEMU exits.
    // 3. Lifetime: the static bytes live for the whole boot.
    // 4. Pointer ownership: no other helper returns this buffer concurrently.
    // 5. Alignment: byte buffers require only `u8` alignment.
    // 6. Mapped length: exactly `PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES` bytes.
    // 7. Concurrency: no SMP or interrupt writer touches package scratch.
    // 8. Violation: mixed slot bytes would decode to a bad registry and fail.
    unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_REGISTRY_SNAPSHOT) }
}

fn object_snapshot_for_acceptance() -> &'static mut ObjectServiceSnapshot {
    // SAFETY:
    // 1. Invariant: this scratch snapshot is used by one candidate validation
    //    read at a time.
    // 2. Established by: package acceptance is a one-shot verify path.
    // 3. Lifetime: the static snapshot lives for the whole boot.
    // 4. Pointer ownership: this helper creates the only mutable reference.
    // 5. Alignment: the static item has `ObjectServiceSnapshot` alignment.
    // 6. Mapped length: exactly one snapshot value is referenced.
    // 7. Concurrency: single-core verify path.
    // 8. Violation: overlapping reads would mix object-checkpoint evidence.
    unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_OBJECT_SNAPSHOT) }
}

fn content_read_buffer_for_acceptance() -> &'static mut [u8] {
    // SAFETY:
    // 1. Invariant: this buffer is used for one `read_published` assertion at
    //    a time in the acceptance path.
    // 2. Established by: the QEMU verification scenario is single-threaded.
    // 3. Lifetime: static storage remains valid for the whole boot.
    // 4. Pointer ownership: no other mutable borrow of the buffer exists while
    //    a read is in progress.
    // 5. Alignment: byte buffers require only `u8` alignment.
    // 6. Mapped length: the returned slice covers exactly the static buffer.
    // 7. Concurrency: no SMP or interrupt writer touches the buffer.
    // 8. Violation: corrupted readback bytes fail digest validation.
    unsafe { &mut *core::ptr::addr_of_mut!(PACKAGE_ACCEPTANCE_CONTENT_READ_BUFFER) }
}

fn read_unpublished_candidate_registry_into(
    block_device: BlockDeviceInfo,
    selected: &PackageRegistry,
    out: &mut PackageRegistry,
) -> Result<(), PackageAcceptanceError> {
    for first_sector in [
        PACKAGE_CANDIDATE_REGISTRY_SLOT_A_SECTOR,
        PACKAGE_CANDIDATE_REGISTRY_SLOT_B_SECTOR,
    ] {
        if read_candidate_registry_slot_into(block_device, first_sector, out).is_ok()
            && out.generation() > selected.generation()
            && out.package_count() > selected.package_count()
        {
            return Ok(());
        }
    }
    Err(PackageAcceptanceError::PackageOperation)
}

fn publication_anchor_exists(
    block_device: BlockDeviceInfo,
) -> Result<bool, PackageAcceptanceError> {
    for slot in [
        PackagePublicationAnchorSlot::A,
        PackagePublicationAnchorSlot::B,
    ] {
        if read_publication_anchor_slot(block_device, slot)
            .map_err(map_package_status)?
            .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn read_candidate_registry_slot_into(
    block_device: BlockDeviceInfo,
    first_sector: u64,
    registry: &mut PackageRegistry,
) -> Result<(), PackageAcceptanceError> {
    let bytes = registry_snapshot_scratch_for_acceptance();
    bytes.fill(0);
    let mut sector_index = 0usize;
    while sector_index < PACKAGE_CANDIDATE_REGISTRY_SLOT_SECTORS {
        let sector =
            read_package_candidate_sector(block_device, first_sector + sector_index as u64)
                .map_err(|_| PackageAcceptanceError::PackageOperation)?;
        let start = sector_index * SECTOR_SIZE;
        bytes[start..start + SECTOR_SIZE].copy_from_slice(&sector);
        sector_index += 1;
    }
    let encoded_len =
        PackageRegistry::encoded_len_from_snapshot_header(bytes).map_err(map_package_status)?;
    PackageRegistry::decode_snapshot_into(&bytes[..encoded_len], registry)
        .map_err(map_package_status)
}

fn package_record_absent_from_selected(
    selected: &PackageRegistry,
    candidate: &PackageRegistry,
) -> Option<PackageRegistryPackageRecord> {
    let mut index = 0usize;
    while let Some(record) = candidate.package_record(index) {
        if !registry_contains_package(selected, record.package_object_id) {
            return Some(record);
        }
        index += 1;
    }
    None
}

fn newest_package_record(registry: &PackageRegistry) -> Option<PackageRegistryPackageRecord> {
    let mut newest = None;
    let mut index = 0usize;
    while let Some(record) = registry.package_record(index) {
        if newest
            .map(|current: PackageRegistryPackageRecord| {
                record.package_object_id > current.package_object_id
            })
            .unwrap_or(true)
        {
            newest = Some(record);
        }
        index += 1;
    }
    newest
}

fn schema_record_for_package(
    registry: &PackageRegistry,
    package_object_id: u64,
) -> Option<PackageRegistrySchemaRecord> {
    let mut index = 0usize;
    while let Some(record) = registry.schema_record(index) {
        if record.package_object_id == package_object_id {
            return Some(record);
        }
        index += 1;
    }
    None
}

fn content_record_for_package(
    registry: &PackageRegistry,
    package_object_id: u64,
) -> Option<crate::package_registry::PackageRegistryContentRecord> {
    let mut index = 0usize;
    while let Some(record) = registry.content_record(index) {
        if record.package_object_id == package_object_id {
            return Some(record);
        }
        index += 1;
    }
    None
}

fn registry_contains_package(registry: &PackageRegistry, package_object_id: u64) -> bool {
    let mut index = 0usize;
    while let Some(record) = registry.package_record(index) {
        if record.package_object_id == package_object_id {
            return true;
        }
        index += 1;
    }
    false
}

fn registry_contains_schema(registry: &PackageRegistry, schema_object_id: u64) -> bool {
    let mut index = 0usize;
    while let Some(record) = registry.schema_record(index) {
        if record.schema_object_id == schema_object_id {
            return true;
        }
        index += 1;
    }
    false
}

fn candidate_content_is_reclaimable(
    selected: &PackageRegistry,
    content: crate::package_registry::PackageRegistryContentRecord,
) -> Result<bool, PackageAcceptanceError> {
    if content.extent_count == 0 {
        return Ok(false);
    }
    let mut index = 0usize;
    while index < content.extent_count as usize {
        if PackageContentStore::extent_live_in_registry(selected, content.extents[index])
            .map_err(map_package_status)?
        {
            return Ok(false);
        }
        index += 1;
    }
    Ok(true)
}

fn candidate_content_is_live(
    selected: &PackageRegistry,
    content: crate::package_registry::PackageRegistryContentRecord,
) -> Result<bool, PackageAcceptanceError> {
    if content.extent_count == 0 {
        return Ok(false);
    }
    let mut index = 0usize;
    while index < content.extent_count as usize {
        if !PackageContentStore::extent_live_in_registry(selected, content.extents[index])
            .map_err(map_package_status)?
        {
            return Ok(false);
        }
        index += 1;
    }
    Ok(true)
}

fn snapshot_contains_current_object(
    snapshot: &ObjectServiceSnapshot,
    object_id: u64,
    revision: u64,
    kind: ObjectKind,
) -> bool {
    let object_id = ObjectId::new(object_id);
    let object_present = snapshot.objects.iter().flatten().any(|record| {
        record.object.object_id() == object_id && record.object.object_kind() == kind
    });
    let revision_present = snapshot.current_revisions.iter().flatten().any(|record| {
        record.object_id() == object_id
            && record.revision() == revision
            && record.object().object_id() == object_id
            && record.object().object_kind() == kind
    });
    object_present && revision_present
}

fn locator_resolves_package(
    mirrors: &PackageLocatorRelationshipStore,
    package_object_id: u64,
) -> bool {
    let package = ObjectId::new(package_object_id);
    let relationships = mirrors.relationship_records();
    let mut index = 0usize;
    while index < relationships.len() {
        if let Some(target_relationship) = relationships[index]
            && target_relationship.kind() == RelationshipKind::BindingTarget
            && target_relationship.target() == package
        {
            let binding = target_relationship.source();
            let mut root_index = 0usize;
            while root_index < relationships.len() {
                if let Some(root_relationship) = relationships[root_index]
                    && root_relationship.source() == ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID)
                    && root_relationship.kind() == RelationshipKind::NameBinding
                    && root_relationship.target() == binding
                {
                    return mirrors.has_object(package) && mirrors.has_object(binding);
                }
                root_index += 1;
            }
        }
        index += 1;
    }
    false
}

fn anchor_slot_for_generation(generation: u64) -> PackagePublicationAnchorSlot {
    if generation & 1 == 0 {
        PackagePublicationAnchorSlot::A
    } else {
        PackagePublicationAnchorSlot::B
    }
}

fn wait_for_power_cut() -> ! {
    loop {
        core::hint::spin_loop();
    }
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
