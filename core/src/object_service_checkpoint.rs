//! ADR 0052 two-slot retained object-service checkpoint.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use core::cell::UnsafeCell;
#[cfg(test)]
use std::sync::Mutex;

#[cfg(not(test))]
use crate::block_device;
use crate::block_device::{BlockDeviceInfo, SECTOR_SIZE};
use crate::dynamic_object_store::MAX_DYNAMIC_OBJECTS;
use crate::general_storage_persistence::GeneralStoragePersistenceError;
use crate::revision_history::{MAX_REVISIONS, RevisionRecord};
use crate::service_identity::ServiceId;
use crate::shell_objects::ObjectId;
use crate::typed_object_format::{RECORD_SIZE, TypedObjectRecord};
use pythos_shared::package_abi::{PACKAGE_CONTENT_BASE_SECTOR, PACKAGE_CONTENT_MAX_BLOCKS};
use pythos_shared::sha256::Sha256;

pub const OBJECT_SERVICE_SLOT_A_HEADER_SECTOR: u64 = 192;
pub const OBJECT_SERVICE_SLOT_A_OBJECT_TABLE_SECTOR: u64 = 193;
pub const OBJECT_SERVICE_SLOT_A_RELATIONSHIP_TABLE_SECTOR: u64 = 201;
pub const OBJECT_SERVICE_SLOT_A_REVISION_TABLE_SECTOR: u64 = 205;
pub const OBJECT_SERVICE_SLOT_A_COMMIT_SECTOR: u64 = 217;
pub const OBJECT_SERVICE_SLOT_B_HEADER_SECTOR: u64 = 224;
pub const OBJECT_SERVICE_SLOT_B_OBJECT_TABLE_SECTOR: u64 = 225;
pub const OBJECT_SERVICE_SLOT_B_RELATIONSHIP_TABLE_SECTOR: u64 = 233;
pub const OBJECT_SERVICE_SLOT_B_REVISION_TABLE_SECTOR: u64 = 237;
pub const OBJECT_SERVICE_SLOT_B_COMMIT_SECTOR: u64 = 249;
pub const OBJECT_SERVICE_TORN_SECTOR: u64 = 250;
pub const OBJECT_SERVICE_OBJECT_TABLE_SECTORS: usize = 8;
pub const OBJECT_SERVICE_RELATIONSHIP_TABLE_SECTORS: usize = 4;
pub const OBJECT_SERVICE_REVISION_TABLE_SECTORS: usize = 12;
pub const OBJECT_SERVICE_CANDIDATE_BASE_SECTOR: u64 =
    PACKAGE_CONTENT_BASE_SECTOR + PACKAGE_CONTENT_MAX_BLOCKS as u64;
pub const OBJECT_SERVICE_CANDIDATE_SLOT_A_HEADER_SECTOR: u64 = OBJECT_SERVICE_CANDIDATE_BASE_SECTOR;
pub const OBJECT_SERVICE_CANDIDATE_SLOT_A_OBJECT_TABLE_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_A_HEADER_SECTOR + 1;
pub const OBJECT_SERVICE_CANDIDATE_SLOT_A_RELATIONSHIP_TABLE_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_A_OBJECT_TABLE_SECTOR
        + OBJECT_SERVICE_OBJECT_TABLE_SECTORS as u64;
pub const OBJECT_SERVICE_CANDIDATE_SLOT_A_REVISION_TABLE_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_A_RELATIONSHIP_TABLE_SECTOR
        + OBJECT_SERVICE_RELATIONSHIP_TABLE_SECTORS as u64;
pub const OBJECT_SERVICE_CANDIDATE_SLOT_A_COMMIT_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_A_REVISION_TABLE_SECTOR
        + OBJECT_SERVICE_REVISION_TABLE_SECTORS as u64;
pub const OBJECT_SERVICE_CANDIDATE_SLOT_B_HEADER_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_A_HEADER_SECTOR + OBJECT_SERVICE_SLOT_SECTOR_COUNT as u64;
pub const OBJECT_SERVICE_CANDIDATE_SLOT_B_OBJECT_TABLE_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_B_HEADER_SECTOR + 1;
pub const OBJECT_SERVICE_CANDIDATE_SLOT_B_RELATIONSHIP_TABLE_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_B_OBJECT_TABLE_SECTOR
        + OBJECT_SERVICE_OBJECT_TABLE_SECTORS as u64;
pub const OBJECT_SERVICE_CANDIDATE_SLOT_B_REVISION_TABLE_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_B_RELATIONSHIP_TABLE_SECTOR
        + OBJECT_SERVICE_RELATIONSHIP_TABLE_SECTORS as u64;
pub const OBJECT_SERVICE_CANDIDATE_SLOT_B_COMMIT_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_B_REVISION_TABLE_SECTOR
        + OBJECT_SERVICE_REVISION_TABLE_SECTORS as u64;
pub const OBJECT_SERVICE_CANDIDATE_HEADER_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_A_HEADER_SECTOR;
pub const OBJECT_SERVICE_CANDIDATE_OBJECT_TABLE_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_A_OBJECT_TABLE_SECTOR;
pub const OBJECT_SERVICE_CANDIDATE_RELATIONSHIP_TABLE_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_A_RELATIONSHIP_TABLE_SECTOR;
pub const OBJECT_SERVICE_CANDIDATE_REVISION_TABLE_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_A_REVISION_TABLE_SECTOR;
pub const OBJECT_SERVICE_CANDIDATE_COMMIT_SECTOR: u64 =
    OBJECT_SERVICE_CANDIDATE_SLOT_B_COMMIT_SECTOR;
pub const OBJECT_SERVICE_OBJECT_CAPACITY: usize = MAX_DYNAMIC_OBJECTS;
pub const OBJECT_SERVICE_WORKSPACE_RELATIONSHIP_CAPACITY: usize = MAX_DYNAMIC_OBJECTS;
pub const OBJECT_SERVICE_CURRENT_REVISION_CAPACITY: usize = MAX_DYNAMIC_OBJECTS;
pub const OBJECT_SERVICE_PRIOR_REVISION_CAPACITY: usize = MAX_REVISIONS;

const CHECKPOINT_MAGIC: [u8; 8] = *b"PY52OBJ1";
const COMMIT_MAGIC: [u8; 8] = *b"PY52DONE";
const CHECKPOINT_VERSION: u16 = 1;
const ACTIVE_RECORD: u16 = 1;
const HEADER_CHECKSUM_OFFSET: usize = 64;
const OBJECT_RECORD_SIZE: usize = 152;
const RELATIONSHIP_RECORD_SIZE: usize = 24;
const REVISION_RECORD_SIZE: usize = 160;
const HEADER_IMAGE_INDEX: usize = 0;
const OBJECT_IMAGE_INDEX: usize = 1;
const RELATIONSHIP_IMAGE_INDEX: usize = OBJECT_IMAGE_INDEX + OBJECT_SERVICE_OBJECT_TABLE_SECTORS;
const REVISION_IMAGE_INDEX: usize =
    RELATIONSHIP_IMAGE_INDEX + OBJECT_SERVICE_RELATIONSHIP_TABLE_SECTORS;
const COMMIT_IMAGE_INDEX: usize = REVISION_IMAGE_INDEX + OBJECT_SERVICE_REVISION_TABLE_SECTORS;
const OBJECT_SERVICE_SLOT_SECTOR_COUNT: usize = COMMIT_IMAGE_INDEX + 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectServiceCheckpointHeader {
    pub magic: [u8; 8],
    pub version: u16,
    pub slot: u16,
    pub object_count: u16,
    pub relationship_count: u16,
    pub revision_count: u16,
    pub reserved0: u16,
    pub generation: u64,
    pub object_table_sector: u64,
    pub relationship_table_sector: u64,
    pub revision_table_sector: u64,
    pub commit_sector: u64,
    pub checksum: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectExtentRecord {
    pub extent_start: u64,
    pub extent_len: u16,
    pub object: TypedObjectRecord,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceRelationshipRecord {
    pub object_id: ObjectId,
    pub workspace_id: ObjectId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectServiceSnapshot {
    pub generation: u64,
    pub allocated_bitmap: u64,
    pub objects: [Option<ObjectExtentRecord>; OBJECT_SERVICE_OBJECT_CAPACITY],
    pub workspace_relationships:
        [Option<WorkspaceRelationshipRecord>; OBJECT_SERVICE_WORKSPACE_RELATIONSHIP_CAPACITY],
    pub current_revisions: [Option<RevisionRecord>; OBJECT_SERVICE_CURRENT_REVISION_CAPACITY],
    pub prior_revisions: [Option<RevisionRecord>; OBJECT_SERVICE_PRIOR_REVISION_CAPACITY],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectCheckpointIdentity {
    pub generation: u64,
    pub root_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectServiceCandidateCheckpoint {
    pub identity: ObjectCheckpointIdentity,
    pub generation: u64,
}

impl ObjectServiceSnapshot {
    pub const fn empty() -> Self {
        Self {
            generation: 0,
            allocated_bitmap: 0,
            objects: [None; OBJECT_SERVICE_OBJECT_CAPACITY],
            workspace_relationships: [None; OBJECT_SERVICE_WORKSPACE_RELATIONSHIP_CAPACITY],
            current_revisions: [None; OBJECT_SERVICE_CURRENT_REVISION_CAPACITY],
            prior_revisions: [None; OBJECT_SERVICE_PRIOR_REVISION_CAPACITY],
        }
    }

    #[cfg(test)]
    pub fn contains_runtime_handle_for_test(
        &self,
        handle: pythos_shared::object_shell_abi::PackedCapability,
    ) -> bool {
        let raw = handle.raw();
        let mut index = 0;
        while index < OBJECT_SERVICE_OBJECT_CAPACITY {
            if let Some(record) = self.objects[index]
                && (record.extent_start == raw || record.object.object_id().raw() == raw)
            {
                return true;
            }
            index += 1;
        }
        let mut relationship_index = 0;
        while relationship_index < OBJECT_SERVICE_WORKSPACE_RELATIONSHIP_CAPACITY {
            if let Some(record) = self.workspace_relationships[relationship_index]
                && (record.object_id.raw() == raw || record.workspace_id.raw() == raw)
            {
                return true;
            }
            relationship_index += 1;
        }
        false
    }
}

pub fn object_checkpoint_identity(snapshot: &ObjectServiceSnapshot) -> ObjectCheckpointIdentity {
    let slot = slot_for_generation(snapshot.generation);
    object_checkpoint_identity_for_slot(slot, snapshot)
}

pub fn object_candidate_checkpoint_identity(
    snapshot: &ObjectServiceSnapshot,
) -> ObjectCheckpointIdentity {
    object_checkpoint_identity_for_slot(
        candidate_slot_for_generation(snapshot.generation),
        snapshot,
    )
}

fn object_checkpoint_identity_for_slot(
    slot: CheckpointSlot,
    snapshot: &ObjectServiceSnapshot,
) -> ObjectCheckpointIdentity {
    #[cfg(test)]
    {
        let image = encode_slot(slot, snapshot, true);
        ObjectCheckpointIdentity {
            generation: snapshot.generation,
            root_digest: slot_image_digest(&image),
        }
    }

    #[cfg(not(test))]
    {
        with_slot_scratch(|image| {
            encode_slot_into(image, slot, snapshot, true);
            ObjectCheckpointIdentity {
                generation: snapshot.generation,
                root_digest: slot_image_digest(image),
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckpointSlot {
    A,
    B,
    CandidateA,
    CandidateB,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectServiceSlotImage {
    sectors: [[u8; SECTOR_SIZE]; OBJECT_SERVICE_SLOT_SECTOR_COUNT],
}

#[cfg(not(test))]
struct SlotScratch(UnsafeCell<ObjectServiceSlotImage>);

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: the scratch slot image is borrowed only through this module's
//    synchronous checkpoint read/write helpers.
// 2. Established by: ADR 0051 normal boot is single-core and no checkpoint
//    helper re-enters another helper while the scratch borrow is live.
// 3. Lifetime: the storage is static and mapped for the full boot.
// 4. Pointer ownership: this module exclusively owns the scratch image.
// 5. Alignment: `UnsafeCell<ObjectServiceSlotImage>` preserves the image's
//    alignment.
// 6. Mapped length: exactly one `ObjectServiceSlotImage` is accessed.
// 7. Concurrency: no SMP or concurrent object-service checkpoint access exists
//    in this slice.
// 8. Violation: concurrent use would mix sectors from different checkpoint
//    operations and corrupt recovery decisions.
unsafe impl Sync for SlotScratch {}

#[cfg(not(test))]
static SLOT_SCRATCH: SlotScratch = SlotScratch(UnsafeCell::new(ObjectServiceSlotImage {
    sectors: [[0; SECTOR_SIZE]; OBJECT_SERVICE_SLOT_SECTOR_COUNT],
}));

#[cfg(not(test))]
struct SnapshotScratch(UnsafeCell<[Option<ObjectServiceSnapshot>; 2]>);

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: restored snapshot scratch slots are borrowed only through one
//    synchronous checkpoint recovery operation.
// 2. Established by: ADR 0051 normal boot is single-core and does not re-enter
//    object-service checkpoint recovery.
// 3. Lifetime: scratch storage is static for the whole boot.
// 4. Pointer ownership: this module exclusively writes and reads the scratch
//    snapshot slots.
// 5. Alignment: `UnsafeCell<[Option<ObjectServiceSnapshot>; 2]>` preserves the
//    array alignment.
// 6. Mapped length: exactly two optional object-service snapshots are accessed.
// 7. Concurrency: no SMP or concurrent checkpoint recovery exists in this
//    slice.
// 8. Violation: concurrent use could expose a mixed generation selection.
unsafe impl Sync for SnapshotScratch {}

#[cfg(not(test))]
static SNAPSHOT_SCRATCH: SnapshotScratch = SnapshotScratch(UnsafeCell::new([None; 2]));

#[cfg(not(test))]
struct IdentitySnapshotScratch(UnsafeCell<ObjectServiceSnapshot>);

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: the identity snapshot scratch is borrowed only through one
//    synchronous checkpoint-root identity computation.
// 2. Established by: ADR 0051 normal boot is single-core and Phase 13 package
//    install computes one transaction anchor at a time.
// 3. Lifetime: scratch storage is static for the whole boot.
// 4. Pointer ownership: this module exclusively owns the scratch snapshot.
// 5. Alignment: `UnsafeCell<ObjectServiceSnapshot>` preserves alignment.
// 6. Mapped length: exactly one object-service snapshot is borrowed.
// 7. Concurrency: no SMP or concurrent package install exists in this slice.
// 8. Violation: concurrent use could mix object state from separate anchors.
unsafe impl Sync for IdentitySnapshotScratch {}

#[cfg(not(test))]
static IDENTITY_SNAPSHOT_SCRATCH: IdentitySnapshotScratch =
    IdentitySnapshotScratch(UnsafeCell::new(ObjectServiceSnapshot::empty()));

#[cfg(test)]
const TEST_CHECKPOINT_SECTOR_COUNT: usize =
    OBJECT_SERVICE_CANDIDATE_SLOT_B_COMMIT_SECTOR as usize + 1;

#[cfg(test)]
static TEST_CHECKPOINT_SECTORS: Mutex<[[u8; SECTOR_SIZE]; TEST_CHECKPOINT_SECTOR_COUNT]> =
    Mutex::new([[0; SECTOR_SIZE]; TEST_CHECKPOINT_SECTOR_COUNT]);

#[cfg(not(test))]
pub fn with_object_identity_snapshot_scratch<R>(
    f: impl FnOnce(&mut ObjectServiceSnapshot) -> R,
) -> R {
    // SAFETY:
    // 1. Invariant: callers borrow the scratch snapshot for one synchronous
    //    identity computation and do not retain references beyond the closure.
    // 2. Established by: this helper takes a non-storing closure and Phase 13
    //    package install is non-reentrant in the current single-core kernel.
    // 3. Lifetime: `IDENTITY_SNAPSHOT_SCRATCH` is initialized for the whole
    //    boot.
    // 4. Pointer ownership: this module owns the scratch snapshot exclusively.
    // 5. Alignment: `UnsafeCell<ObjectServiceSnapshot>` preserves alignment.
    // 6. Mapped length: exactly one `ObjectServiceSnapshot` is accessed.
    // 7. Concurrency: no SMP or concurrent package install path exists.
    // 8. Violation: concurrent mutation would corrupt transaction anchor
    //    identity input.
    unsafe { f(&mut *IDENTITY_SNAPSHOT_SCRATCH.0.get()) }
}

pub fn write_object_service_checkpoint(
    device: BlockDeviceInfo,
    snapshot: &ObjectServiceSnapshot,
) -> Result<(), GeneralStoragePersistenceError> {
    #[cfg(not(test))]
    {
        let next_generation = with_restored_object_service_checkpoint(device, |snapshot| {
            snapshot.map_or(1, |snapshot| snapshot.generation + 1)
        })?;
        let slot = slot_for_generation(next_generation);
        return with_slot_scratch(|image| {
            encode_slot_into_with_generation(image, slot, snapshot, next_generation, true);
            write_slot_image(device, slot, image)?;
            read_slot_image_into(device, slot, image)?;
            decode_slot(image)?;
            Ok(())
        });
    }

    #[cfg(test)]
    {
        let current = read_object_service_checkpoint(device)?;
        let mut next = *snapshot;
        next.generation = current.map_or(1, |snapshot| snapshot.generation + 1);
        let slot = slot_for_generation(next.generation);
        let image = encode_slot(slot, &next, true);
        write_slot_image(device, slot, &image)?;

        let written = read_slot_image(device, slot)?;
        decode_slot(&written)?;
        Ok(())
    }
}

pub fn write_object_service_candidate_checkpoint(
    device: BlockDeviceInfo,
    snapshot: &ObjectServiceSnapshot,
) -> Result<ObjectServiceCandidateCheckpoint, GeneralStoragePersistenceError> {
    #[cfg(not(test))]
    {
        return with_slot_scratch(|image| {
            let slot = candidate_slot_for_generation(snapshot.generation);
            encode_slot_into_with_generation(image, slot, snapshot, snapshot.generation, true);
            write_slot_image(device, slot, image)?;
            read_slot_image_into(device, slot, image)?;
            let identity = object_candidate_checkpoint_identity_from_image(image)?;
            Ok(ObjectServiceCandidateCheckpoint {
                identity,
                generation: identity.generation,
            })
        });
    }

    #[cfg(test)]
    {
        let slot = candidate_slot_for_generation(snapshot.generation);
        let image = encode_slot(slot, snapshot, true);
        write_slot_image(device, slot, &image)?;

        let written = read_slot_image(device, slot)?;
        let identity = object_candidate_checkpoint_identity_from_image(&written)?;
        Ok(ObjectServiceCandidateCheckpoint {
            identity,
            generation: identity.generation,
        })
    }
}

pub fn read_object_service_candidate_checkpoint(
    device: BlockDeviceInfo,
    expected: ObjectCheckpointIdentity,
) -> Result<ObjectServiceSnapshot, GeneralStoragePersistenceError> {
    let mut snapshot = ObjectServiceSnapshot::empty();
    read_object_service_candidate_checkpoint_into(device, expected, &mut snapshot)?;
    Ok(snapshot)
}

pub fn read_object_service_candidate_checkpoint_into(
    device: BlockDeviceInfo,
    expected: ObjectCheckpointIdentity,
    snapshot: &mut ObjectServiceSnapshot,
) -> Result<(), GeneralStoragePersistenceError> {
    #[cfg(not(test))]
    {
        return with_slot_scratch(|image| {
            read_slot_image_into(
                device,
                candidate_slot_for_generation(expected.generation),
                image,
            )?;
            if !is_candidate_slot(slot_from_code(read_u16(
                &image.sectors[HEADER_IMAGE_INDEX],
                10,
            ))?) {
                return Err(GeneralStoragePersistenceError::BadSnapshot);
            }
            decode_slot_into(image, snapshot)?;
            let identity = ObjectCheckpointIdentity {
                generation: snapshot.generation,
                root_digest: slot_image_digest(image),
            };
            if identity != expected {
                return Err(GeneralStoragePersistenceError::BadSnapshot);
            }
            Ok(())
        });
    }

    #[cfg(test)]
    {
        let image = read_slot_image(device, candidate_slot_for_generation(expected.generation))?;
        if !is_candidate_slot(slot_from_code(read_u16(
            &image.sectors[HEADER_IMAGE_INDEX],
            10,
        ))?) {
            return Err(GeneralStoragePersistenceError::BadSnapshot);
        }
        decode_slot_into(&image, snapshot)?;
        let identity = ObjectCheckpointIdentity {
            generation: snapshot.generation,
            root_digest: slot_image_digest(&image),
        };
        if identity != expected {
            return Err(GeneralStoragePersistenceError::BadSnapshot);
        }
        Ok(())
    }
}

#[cfg(test)]
pub fn read_object_service_checkpoint(
    device: BlockDeviceInfo,
) -> Result<Option<ObjectServiceSnapshot>, GeneralStoragePersistenceError> {
    let first = read_committed_slot_image(device, CheckpointSlot::A)?
        .and_then(|image| decode_ordinary_slot(&image).ok());
    let second = read_committed_slot_image(device, CheckpointSlot::B)?
        .and_then(|image| decode_ordinary_slot(&image).ok());
    Ok(select_highest_generation(first, second))
}

#[cfg(not(test))]
pub fn with_restored_object_service_checkpoint<R>(
    device: BlockDeviceInfo,
    f: impl FnOnce(Option<&ObjectServiceSnapshot>) -> R,
) -> Result<R, GeneralStoragePersistenceError> {
    with_checkpoint_scratch(|image, snapshots| {
        snapshots[0] = None;
        snapshots[1] = None;
        read_committed_slot_snapshot_into(device, CheckpointSlot::A, image, &mut snapshots[0])?;
        read_committed_slot_snapshot_into(device, CheckpointSlot::B, image, &mut snapshots[1])?;
        let selected = select_highest_generation_ref(&snapshots[0], &snapshots[1]);
        Ok(f(selected))
    })
}

fn recover_from_slot_images(
    first: &ObjectServiceSlotImage,
    second: &ObjectServiceSlotImage,
) -> Result<Option<ObjectServiceSnapshot>, GeneralStoragePersistenceError> {
    let first = decode_ordinary_slot(first).ok();
    let second = decode_ordinary_slot(second).ok();
    Ok(select_highest_generation(first, second))
}

fn select_highest_generation(
    first: Option<ObjectServiceSnapshot>,
    second: Option<ObjectServiceSnapshot>,
) -> Option<ObjectServiceSnapshot> {
    match (first, second) {
        (Some(first), Some(second)) if second.generation > first.generation => Some(second),
        (Some(first), Some(_)) => Some(first),
        (Some(first), None) => Some(first),
        (None, Some(second)) => Some(second),
        (None, None) => None,
    }
}

#[cfg(not(test))]
fn select_highest_generation_ref<'a>(
    first: &'a Option<ObjectServiceSnapshot>,
    second: &'a Option<ObjectServiceSnapshot>,
) -> Option<&'a ObjectServiceSnapshot> {
    match (first.as_ref(), second.as_ref()) {
        (Some(first), Some(second)) if second.generation > first.generation => Some(second),
        (Some(first), Some(_)) => Some(first),
        (Some(first), None) => Some(first),
        (None, Some(second)) => Some(second),
        (None, None) => None,
    }
}

#[cfg(test)]
fn read_committed_slot_image(
    device: BlockDeviceInfo,
    slot: CheckpointSlot,
) -> Result<Option<ObjectServiceSlotImage>, GeneralStoragePersistenceError> {
    let header = read_checkpoint_sector(device, sector_for_image_index(slot, HEADER_IMAGE_INDEX))?;
    let commit = read_checkpoint_sector(device, sector_for_image_index(slot, COMMIT_IMAGE_INDEX))?;
    if !slot_probe_is_committed(&header, &commit) {
        return Ok(None);
    }

    let mut image = ObjectServiceSlotImage {
        sectors: [[0; SECTOR_SIZE]; OBJECT_SERVICE_SLOT_SECTOR_COUNT],
    };
    image.sectors[HEADER_IMAGE_INDEX] = header;
    image.sectors[COMMIT_IMAGE_INDEX] = commit;
    let mut index = OBJECT_IMAGE_INDEX;
    while index < COMMIT_IMAGE_INDEX {
        image.sectors[index] = read_checkpoint_sector(device, sector_for_image_index(slot, index))?;
        index += 1;
    }
    Ok(Some(image))
}

#[cfg(not(test))]
fn read_committed_slot_snapshot_into(
    device: BlockDeviceInfo,
    slot: CheckpointSlot,
    image: &mut ObjectServiceSlotImage,
    snapshot: &mut Option<ObjectServiceSnapshot>,
) -> Result<(), GeneralStoragePersistenceError> {
    *snapshot = None;
    let header = read_checkpoint_sector(device, sector_for_image_index(slot, HEADER_IMAGE_INDEX))?;
    let commit = read_checkpoint_sector(device, sector_for_image_index(slot, COMMIT_IMAGE_INDEX))?;
    if !slot_probe_is_committed(&header, &commit) {
        return Ok(());
    }

    image.sectors[HEADER_IMAGE_INDEX] = header;
    image.sectors[COMMIT_IMAGE_INDEX] = commit;
    let mut index = OBJECT_IMAGE_INDEX;
    while index < COMMIT_IMAGE_INDEX {
        image.sectors[index] = read_checkpoint_sector(device, sector_for_image_index(slot, index))?;
        index += 1;
    }
    *snapshot = decode_ordinary_slot(image).ok();
    Ok(())
}

fn slot_probe_is_committed(header: &[u8; SECTOR_SIZE], commit: &[u8; SECTOR_SIZE]) -> bool {
    header[0..8] == CHECKPOINT_MAGIC
        && commit[0..8] == COMMIT_MAGIC
        && read_u64(header, 24) == read_u64(commit, 8)
}

fn read_slot_image(
    device: BlockDeviceInfo,
    slot: CheckpointSlot,
) -> Result<ObjectServiceSlotImage, GeneralStoragePersistenceError> {
    let mut image = ObjectServiceSlotImage {
        sectors: [[0; SECTOR_SIZE]; OBJECT_SERVICE_SLOT_SECTOR_COUNT],
    };
    read_slot_image_into(device, slot, &mut image)?;
    Ok(image)
}

fn read_slot_image_into(
    device: BlockDeviceInfo,
    slot: CheckpointSlot,
    image: &mut ObjectServiceSlotImage,
) -> Result<(), GeneralStoragePersistenceError> {
    let mut index = 0;
    while index < OBJECT_SERVICE_SLOT_SECTOR_COUNT {
        image.sectors[index] = read_checkpoint_sector(device, sector_for_image_index(slot, index))?;
        index += 1;
    }
    Ok(())
}

fn write_slot_image(
    device: BlockDeviceInfo,
    slot: CheckpointSlot,
    image: &ObjectServiceSlotImage,
) -> Result<(), GeneralStoragePersistenceError> {
    write_checkpoint_sector(
        device,
        sector_for_image_index(slot, COMMIT_IMAGE_INDEX),
        &[0; SECTOR_SIZE],
    )?;

    let mut index = OBJECT_IMAGE_INDEX;
    while index < COMMIT_IMAGE_INDEX {
        write_checkpoint_sector(
            device,
            sector_for_image_index(slot, index),
            &image.sectors[index],
        )?;
        index += 1;
    }
    write_checkpoint_sector(
        device,
        sector_for_image_index(slot, HEADER_IMAGE_INDEX),
        &image.sectors[HEADER_IMAGE_INDEX],
    )?;
    write_checkpoint_sector(
        device,
        sector_for_image_index(slot, COMMIT_IMAGE_INDEX),
        &image.sectors[COMMIT_IMAGE_INDEX],
    )?;
    Ok(())
}

fn encode_slot(
    slot: CheckpointSlot,
    snapshot: &ObjectServiceSnapshot,
    committed: bool,
) -> ObjectServiceSlotImage {
    let mut image = ObjectServiceSlotImage {
        sectors: [[0; SECTOR_SIZE]; OBJECT_SERVICE_SLOT_SECTOR_COUNT],
    };
    encode_slot_into(&mut image, slot, snapshot, committed);
    image
}

fn encode_slot_into(
    image: &mut ObjectServiceSlotImage,
    slot: CheckpointSlot,
    snapshot: &ObjectServiceSnapshot,
    committed: bool,
) {
    encode_slot_into_with_generation(image, slot, snapshot, snapshot.generation, committed);
}

fn encode_slot_into_with_generation(
    image: &mut ObjectServiceSlotImage,
    slot: CheckpointSlot,
    snapshot: &ObjectServiceSnapshot,
    generation: u64,
    committed: bool,
) {
    *image = ObjectServiceSlotImage {
        sectors: [[0; SECTOR_SIZE]; OBJECT_SERVICE_SLOT_SECTOR_COUNT],
    };
    encode_objects(image, snapshot);
    encode_relationships(image, snapshot);
    encode_revisions(image, snapshot);
    encode_header(image, slot, snapshot, generation, 0);
    let checksum = checksum_image(image);
    encode_header(image, slot, snapshot, generation, checksum);
    if committed {
        image.sectors[COMMIT_IMAGE_INDEX][0..8].copy_from_slice(&COMMIT_MAGIC);
        write_u64(&mut image.sectors[COMMIT_IMAGE_INDEX], 8, generation);
    }
}

fn slot_image_digest(image: &ObjectServiceSlotImage) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for sector in image.sectors.iter() {
        hasher.update(sector);
    }
    hasher.finalize()
}

fn object_candidate_checkpoint_identity_from_image(
    image: &ObjectServiceSlotImage,
) -> Result<ObjectCheckpointIdentity, GeneralStoragePersistenceError> {
    let snapshot = decode_slot(image)?;
    if !is_candidate_slot(slot_from_code(read_u16(
        &image.sectors[HEADER_IMAGE_INDEX],
        10,
    ))?) {
        return Err(GeneralStoragePersistenceError::BadSnapshot);
    }
    Ok(ObjectCheckpointIdentity {
        generation: snapshot.generation,
        root_digest: slot_image_digest(image),
    })
}

#[cfg(not(test))]
fn with_slot_scratch<R>(f: impl FnOnce(&mut ObjectServiceSlotImage) -> R) -> R {
    // SAFETY:
    // 1. Invariant: callers borrow the scratch image for one synchronous
    //    checkpoint operation and do not retain the mutable reference.
    // 2. Established by: this function takes a non-storing closure and all
    //    checkpoint helpers are non-reentrant in ADR 0051 normal boot.
    // 3. Lifetime: `SLOT_SCRATCH` is static and remains initialized forever.
    // 4. Pointer ownership: this module owns the scratch image exclusively.
    // 5. Alignment: `UnsafeCell<ObjectServiceSlotImage>` preserves alignment.
    // 6. Mapped length: exactly one object-service slot image is borrowed.
    // 7. Concurrency: single-core boot and syscall dispatch prevent concurrent
    //    checkpoint access in this slice.
    // 8. Violation: a concurrent borrow could mix checkpoint sector contents.
    unsafe { f(&mut *SLOT_SCRATCH.0.get()) }
}

#[cfg(not(test))]
fn with_snapshot_scratch<R>(f: impl FnOnce(&mut [Option<ObjectServiceSnapshot>; 2]) -> R) -> R {
    // SAFETY:
    // 1. Invariant: callers borrow the snapshot scratch array for one
    //    synchronous checkpoint recovery operation and do not store references
    //    beyond the closure.
    // 2. Established by: this function takes a non-storing closure and all
    //    checkpoint recovery helpers are non-reentrant in ADR 0051 normal boot.
    // 3. Lifetime: `SNAPSHOT_SCRATCH` is static and remains initialized.
    // 4. Pointer ownership: this module owns both scratch snapshot slots.
    // 5. Alignment: `UnsafeCell` preserves the array alignment.
    // 6. Mapped length: exactly two optional snapshots are borrowed.
    // 7. Concurrency: single-core boot and syscall dispatch prevent concurrent
    //    checkpoint recovery in this slice.
    // 8. Violation: a concurrent borrow could select or decode the wrong
    //    generation.
    unsafe { f(&mut *SNAPSHOT_SCRATCH.0.get()) }
}

#[cfg(not(test))]
fn with_checkpoint_scratch<R>(
    f: impl FnOnce(&mut ObjectServiceSlotImage, &mut [Option<ObjectServiceSnapshot>; 2]) -> R,
) -> R {
    with_slot_scratch(|image| with_snapshot_scratch(|snapshots| f(image, snapshots)))
}

fn decode_slot(
    image: &ObjectServiceSlotImage,
) -> Result<ObjectServiceSnapshot, GeneralStoragePersistenceError> {
    let mut snapshot = ObjectServiceSnapshot::empty();
    decode_slot_into(image, &mut snapshot)?;
    Ok(snapshot)
}

fn decode_slot_into(
    image: &ObjectServiceSlotImage,
    snapshot: &mut ObjectServiceSnapshot,
) -> Result<(), GeneralStoragePersistenceError> {
    if image.sectors[COMMIT_IMAGE_INDEX][0..8] != COMMIT_MAGIC {
        return Err(GeneralStoragePersistenceError::TornWrite);
    }
    let header = decode_header(&image.sectors[HEADER_IMAGE_INDEX])?;
    let commit_generation = read_u64(&image.sectors[COMMIT_IMAGE_INDEX], 8);
    let expected_layout = slot_layout(slot_from_code(header.slot)?);
    if header.generation != commit_generation
        || header.object_table_sector != expected_layout.object_table_sector
        || header.relationship_table_sector != expected_layout.relationship_table_sector
        || header.revision_table_sector != expected_layout.revision_table_sector
        || header.commit_sector != expected_layout.commit_sector
        || header.checksum != checksum_image(image)
    {
        return Err(GeneralStoragePersistenceError::BadSnapshot);
    }
    snapshot.generation = header.generation;
    snapshot.allocated_bitmap = read_u64(&image.sectors[HEADER_IMAGE_INDEX], 72);
    decode_objects_into(image, &mut snapshot.objects)?;
    decode_relationships_into(image, &mut snapshot.workspace_relationships)?;
    decode_current_revisions_into(image, &mut snapshot.current_revisions)?;
    decode_prior_revisions_into(image, &mut snapshot.prior_revisions)?;
    if header.object_count as usize != count_options(&snapshot.objects)
        || header.relationship_count as usize != count_options(&snapshot.workspace_relationships)
        || header.revision_count as usize
            != count_options(&snapshot.current_revisions) + count_options(&snapshot.prior_revisions)
    {
        return Err(GeneralStoragePersistenceError::BadSnapshot);
    }
    Ok(())
}

fn decode_ordinary_slot(
    image: &ObjectServiceSlotImage,
) -> Result<ObjectServiceSnapshot, GeneralStoragePersistenceError> {
    if is_candidate_slot(slot_from_code(read_u16(
        &image.sectors[HEADER_IMAGE_INDEX],
        10,
    ))?) {
        return Err(GeneralStoragePersistenceError::BadSnapshot);
    }
    decode_slot(image)
}

fn encode_header(
    image: &mut ObjectServiceSlotImage,
    slot: CheckpointSlot,
    snapshot: &ObjectServiceSnapshot,
    generation: u64,
    checksum: u64,
) {
    let layout = slot_layout(slot);
    let sector = &mut image.sectors[HEADER_IMAGE_INDEX];
    sector[0..8].copy_from_slice(&CHECKPOINT_MAGIC);
    write_u16(sector, 8, CHECKPOINT_VERSION);
    write_u16(sector, 10, slot_code(slot));
    write_u16(sector, 12, count_options(&snapshot.objects) as u16);
    write_u16(
        sector,
        14,
        count_options(&snapshot.workspace_relationships) as u16,
    );
    write_u16(
        sector,
        16,
        (count_options(&snapshot.current_revisions) + count_options(&snapshot.prior_revisions))
            as u16,
    );
    write_u16(sector, 18, 0);
    write_u64(sector, 24, generation);
    write_u64(sector, 32, layout.object_table_sector);
    write_u64(sector, 40, layout.relationship_table_sector);
    write_u64(sector, 48, layout.revision_table_sector);
    write_u64(sector, 56, layout.commit_sector);
    write_u64(sector, HEADER_CHECKSUM_OFFSET, checksum);
    write_u64(sector, 72, snapshot.allocated_bitmap);
}

fn decode_header(
    sector: &[u8; SECTOR_SIZE],
) -> Result<ObjectServiceCheckpointHeader, GeneralStoragePersistenceError> {
    if sector[0..8] != CHECKPOINT_MAGIC || read_u16(sector, 8) != CHECKPOINT_VERSION {
        return Err(GeneralStoragePersistenceError::BadSnapshot);
    }
    if read_u16(sector, 18) != 0 {
        return Err(GeneralStoragePersistenceError::BadSnapshot);
    }
    Ok(ObjectServiceCheckpointHeader {
        magic: CHECKPOINT_MAGIC,
        version: CHECKPOINT_VERSION,
        slot: read_u16(sector, 10),
        object_count: read_u16(sector, 12),
        relationship_count: read_u16(sector, 14),
        revision_count: read_u16(sector, 16),
        reserved0: 0,
        generation: read_u64(sector, 24),
        object_table_sector: read_u64(sector, 32),
        relationship_table_sector: read_u64(sector, 40),
        revision_table_sector: read_u64(sector, 48),
        commit_sector: read_u64(sector, 56),
        checksum: read_u64(sector, HEADER_CHECKSUM_OFFSET),
    })
}

fn encode_objects(image: &mut ObjectServiceSlotImage, snapshot: &ObjectServiceSnapshot) {
    let mut index = 0;
    while index < OBJECT_SERVICE_OBJECT_CAPACITY {
        if let Some(record) = snapshot.objects[index] {
            let offset = index * OBJECT_RECORD_SIZE;
            table_write_u16(
                image,
                OBJECT_IMAGE_INDEX,
                OBJECT_SERVICE_OBJECT_TABLE_SECTORS,
                offset,
                ACTIVE_RECORD,
            );
            table_write_u64(
                image,
                OBJECT_IMAGE_INDEX,
                OBJECT_SERVICE_OBJECT_TABLE_SECTORS,
                offset + 8,
                record.extent_start,
            );
            table_write_u16(
                image,
                OBJECT_IMAGE_INDEX,
                OBJECT_SERVICE_OBJECT_TABLE_SECTORS,
                offset + 16,
                record.extent_len,
            );
            table_write_bytes(
                image,
                OBJECT_IMAGE_INDEX,
                OBJECT_SERVICE_OBJECT_TABLE_SECTORS,
                offset + 24,
                &record.object.encode(),
            );
        }
        index += 1;
    }
}

fn decode_objects_into(
    image: &ObjectServiceSlotImage,
    records: &mut [Option<ObjectExtentRecord>; OBJECT_SERVICE_OBJECT_CAPACITY],
) -> Result<(), GeneralStoragePersistenceError> {
    let mut index = 0;
    while index < OBJECT_SERVICE_OBJECT_CAPACITY {
        records[index] = None;
        let offset = index * OBJECT_RECORD_SIZE;
        let active = table_read_u16(image, OBJECT_IMAGE_INDEX, offset);
        if active == ACTIVE_RECORD {
            let object_bytes =
                table_read_bytes::<RECORD_SIZE>(image, OBJECT_IMAGE_INDEX, offset + 24);
            records[index] = Some(ObjectExtentRecord {
                extent_start: table_read_u64(image, OBJECT_IMAGE_INDEX, offset + 8),
                extent_len: table_read_u16(image, OBJECT_IMAGE_INDEX, offset + 16),
                object: TypedObjectRecord::decode(&object_bytes)?,
            });
        } else if active != 0 {
            return Err(GeneralStoragePersistenceError::BadSnapshot);
        }
        index += 1;
    }
    Ok(())
}

fn encode_relationships(image: &mut ObjectServiceSlotImage, snapshot: &ObjectServiceSnapshot) {
    let mut index = 0;
    while index < OBJECT_SERVICE_WORKSPACE_RELATIONSHIP_CAPACITY {
        if let Some(record) = snapshot.workspace_relationships[index] {
            let offset = index * RELATIONSHIP_RECORD_SIZE;
            table_write_u16(
                image,
                RELATIONSHIP_IMAGE_INDEX,
                OBJECT_SERVICE_RELATIONSHIP_TABLE_SECTORS,
                offset,
                ACTIVE_RECORD,
            );
            table_write_u64(
                image,
                RELATIONSHIP_IMAGE_INDEX,
                OBJECT_SERVICE_RELATIONSHIP_TABLE_SECTORS,
                offset + 8,
                record.object_id.raw(),
            );
            table_write_u64(
                image,
                RELATIONSHIP_IMAGE_INDEX,
                OBJECT_SERVICE_RELATIONSHIP_TABLE_SECTORS,
                offset + 16,
                record.workspace_id.raw(),
            );
        }
        index += 1;
    }
}

fn decode_relationships_into(
    image: &ObjectServiceSlotImage,
    records: &mut [Option<WorkspaceRelationshipRecord>;
             OBJECT_SERVICE_WORKSPACE_RELATIONSHIP_CAPACITY],
) -> Result<(), GeneralStoragePersistenceError> {
    let mut index = 0;
    while index < OBJECT_SERVICE_WORKSPACE_RELATIONSHIP_CAPACITY {
        records[index] = None;
        let offset = index * RELATIONSHIP_RECORD_SIZE;
        let active = table_read_u16(image, RELATIONSHIP_IMAGE_INDEX, offset);
        if active == ACTIVE_RECORD {
            records[index] = Some(WorkspaceRelationshipRecord {
                object_id: ObjectId::new(table_read_u64(
                    image,
                    RELATIONSHIP_IMAGE_INDEX,
                    offset + 8,
                )),
                workspace_id: ObjectId::new(table_read_u64(
                    image,
                    RELATIONSHIP_IMAGE_INDEX,
                    offset + 16,
                )),
            });
        } else if active != 0 {
            return Err(GeneralStoragePersistenceError::BadSnapshot);
        }
        index += 1;
    }
    Ok(())
}

fn encode_revisions(image: &mut ObjectServiceSlotImage, snapshot: &ObjectServiceSnapshot) {
    let mut index = 0;
    while index < OBJECT_SERVICE_CURRENT_REVISION_CAPACITY {
        if let Some(record) = snapshot.current_revisions[index] {
            encode_revision(image, index, record);
        }
        index += 1;
    }
    let mut prior_index = 0;
    while prior_index < OBJECT_SERVICE_PRIOR_REVISION_CAPACITY {
        if let Some(record) = snapshot.prior_revisions[prior_index] {
            encode_revision(
                image,
                OBJECT_SERVICE_CURRENT_REVISION_CAPACITY + prior_index,
                record,
            );
        }
        prior_index += 1;
    }
}

fn encode_revision(image: &mut ObjectServiceSlotImage, index: usize, record: RevisionRecord) {
    let offset = index * REVISION_RECORD_SIZE;
    table_write_u16(
        image,
        REVISION_IMAGE_INDEX,
        OBJECT_SERVICE_REVISION_TABLE_SECTORS,
        offset,
        ACTIVE_RECORD,
    );
    table_write_u64(
        image,
        REVISION_IMAGE_INDEX,
        OBJECT_SERVICE_REVISION_TABLE_SECTORS,
        offset + 8,
        record.object_id().raw(),
    );
    table_write_u64(
        image,
        REVISION_IMAGE_INDEX,
        OBJECT_SERVICE_REVISION_TABLE_SECTORS,
        offset + 16,
        record.revision(),
    );
    table_write_u64(
        image,
        REVISION_IMAGE_INDEX,
        OBJECT_SERVICE_REVISION_TABLE_SECTORS,
        offset + 24,
        record.timestamp_ticks(),
    );
    table_write_u64(
        image,
        REVISION_IMAGE_INDEX,
        OBJECT_SERVICE_REVISION_TABLE_SECTORS,
        offset + 32,
        record.writer().raw(),
    );
    table_write_bytes(
        image,
        REVISION_IMAGE_INDEX,
        OBJECT_SERVICE_REVISION_TABLE_SECTORS,
        offset + 40,
        &record.object().encode(),
    );
}

fn decode_current_revisions_into(
    image: &ObjectServiceSlotImage,
    records: &mut [Option<RevisionRecord>; OBJECT_SERVICE_CURRENT_REVISION_CAPACITY],
) -> Result<(), GeneralStoragePersistenceError> {
    let mut index = 0;
    while index < OBJECT_SERVICE_CURRENT_REVISION_CAPACITY {
        records[index] = None;
        records[index] = decode_revision(image, index)?;
        index += 1;
    }
    Ok(())
}

fn decode_prior_revisions_into(
    image: &ObjectServiceSlotImage,
    records: &mut [Option<RevisionRecord>; OBJECT_SERVICE_PRIOR_REVISION_CAPACITY],
) -> Result<(), GeneralStoragePersistenceError> {
    let mut index = 0;
    while index < OBJECT_SERVICE_PRIOR_REVISION_CAPACITY {
        records[index] = None;
        records[index] = decode_revision(image, OBJECT_SERVICE_CURRENT_REVISION_CAPACITY + index)?;
        index += 1;
    }
    Ok(())
}

fn decode_revision(
    image: &ObjectServiceSlotImage,
    index: usize,
) -> Result<Option<RevisionRecord>, GeneralStoragePersistenceError> {
    let offset = index * REVISION_RECORD_SIZE;
    let active = table_read_u16(image, REVISION_IMAGE_INDEX, offset);
    if active == 0 {
        return Ok(None);
    }
    if active != ACTIVE_RECORD {
        return Err(GeneralStoragePersistenceError::BadSnapshot);
    }
    let object_bytes = table_read_bytes::<RECORD_SIZE>(image, REVISION_IMAGE_INDEX, offset + 40);
    Ok(Some(RevisionRecord::new(
        ObjectId::new(table_read_u64(image, REVISION_IMAGE_INDEX, offset + 8)),
        table_read_u64(image, REVISION_IMAGE_INDEX, offset + 16),
        table_read_u64(image, REVISION_IMAGE_INDEX, offset + 24),
        ServiceId::from_raw(table_read_u64(image, REVISION_IMAGE_INDEX, offset + 32)),
        TypedObjectRecord::decode(&object_bytes)?,
    )))
}

fn table_write_bytes(
    image: &mut ObjectServiceSlotImage,
    first_sector: usize,
    sector_count: usize,
    offset: usize,
    bytes: &[u8],
) {
    let mut index = 0;
    while index < bytes.len() {
        let absolute = offset + index;
        if absolute < sector_count * SECTOR_SIZE {
            let sector = first_sector + absolute / SECTOR_SIZE;
            let sector_offset = absolute % SECTOR_SIZE;
            image.sectors[sector][sector_offset] = bytes[index];
        }
        index += 1;
    }
}

fn table_read_bytes<const N: usize>(
    image: &ObjectServiceSlotImage,
    first_sector: usize,
    offset: usize,
) -> [u8; N] {
    let mut bytes = [0; N];
    let mut index = 0;
    while index < N {
        let absolute = offset + index;
        let sector = first_sector + absolute / SECTOR_SIZE;
        let sector_offset = absolute % SECTOR_SIZE;
        bytes[index] = image.sectors[sector][sector_offset];
        index += 1;
    }
    bytes
}

fn table_write_u16(
    image: &mut ObjectServiceSlotImage,
    first_sector: usize,
    sector_count: usize,
    offset: usize,
    value: u16,
) {
    table_write_bytes(
        image,
        first_sector,
        sector_count,
        offset,
        &value.to_le_bytes(),
    );
}

fn table_write_u64(
    image: &mut ObjectServiceSlotImage,
    first_sector: usize,
    sector_count: usize,
    offset: usize,
    value: u64,
) {
    table_write_bytes(
        image,
        first_sector,
        sector_count,
        offset,
        &value.to_le_bytes(),
    );
}

fn table_read_u16(image: &ObjectServiceSlotImage, first_sector: usize, offset: usize) -> u16 {
    u16::from_le_bytes(table_read_bytes::<2>(image, first_sector, offset))
}

fn table_read_u64(image: &ObjectServiceSlotImage, first_sector: usize, offset: usize) -> u64 {
    u64::from_le_bytes(table_read_bytes::<8>(image, first_sector, offset))
}

fn checksum_image(image: &ObjectServiceSlotImage) -> u64 {
    let mut checksum = 0xcbf2_9ce4_8422_2325u64;
    let mut sector_index = 0;
    while sector_index < COMMIT_IMAGE_INDEX {
        let mut byte_index = 0;
        while byte_index < SECTOR_SIZE {
            if !(sector_index == HEADER_IMAGE_INDEX
                && (HEADER_CHECKSUM_OFFSET..HEADER_CHECKSUM_OFFSET + 8).contains(&byte_index))
            {
                checksum ^= u64::from(image.sectors[sector_index][byte_index]);
                checksum = checksum.wrapping_mul(0x0000_0100_0000_01B3);
            }
            byte_index += 1;
        }
        sector_index += 1;
    }
    checksum
}

fn count_options<T, const N: usize>(records: &[Option<T>; N]) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < N {
        if records[index].is_some() {
            count += 1;
        }
        index += 1;
    }
    count
}

#[derive(Clone, Copy)]
struct SlotLayout {
    object_table_sector: u64,
    relationship_table_sector: u64,
    revision_table_sector: u64,
    commit_sector: u64,
}

fn slot_layout(slot: CheckpointSlot) -> SlotLayout {
    match slot {
        CheckpointSlot::A => SlotLayout {
            object_table_sector: OBJECT_SERVICE_SLOT_A_OBJECT_TABLE_SECTOR,
            relationship_table_sector: OBJECT_SERVICE_SLOT_A_RELATIONSHIP_TABLE_SECTOR,
            revision_table_sector: OBJECT_SERVICE_SLOT_A_REVISION_TABLE_SECTOR,
            commit_sector: OBJECT_SERVICE_SLOT_A_COMMIT_SECTOR,
        },
        CheckpointSlot::B => SlotLayout {
            object_table_sector: OBJECT_SERVICE_SLOT_B_OBJECT_TABLE_SECTOR,
            relationship_table_sector: OBJECT_SERVICE_SLOT_B_RELATIONSHIP_TABLE_SECTOR,
            revision_table_sector: OBJECT_SERVICE_SLOT_B_REVISION_TABLE_SECTOR,
            commit_sector: OBJECT_SERVICE_SLOT_B_COMMIT_SECTOR,
        },
        CheckpointSlot::CandidateA => SlotLayout {
            object_table_sector: OBJECT_SERVICE_CANDIDATE_SLOT_A_OBJECT_TABLE_SECTOR,
            relationship_table_sector: OBJECT_SERVICE_CANDIDATE_SLOT_A_RELATIONSHIP_TABLE_SECTOR,
            revision_table_sector: OBJECT_SERVICE_CANDIDATE_SLOT_A_REVISION_TABLE_SECTOR,
            commit_sector: OBJECT_SERVICE_CANDIDATE_SLOT_A_COMMIT_SECTOR,
        },
        CheckpointSlot::CandidateB => SlotLayout {
            object_table_sector: OBJECT_SERVICE_CANDIDATE_SLOT_B_OBJECT_TABLE_SECTOR,
            relationship_table_sector: OBJECT_SERVICE_CANDIDATE_SLOT_B_RELATIONSHIP_TABLE_SECTOR,
            revision_table_sector: OBJECT_SERVICE_CANDIDATE_SLOT_B_REVISION_TABLE_SECTOR,
            commit_sector: OBJECT_SERVICE_CANDIDATE_SLOT_B_COMMIT_SECTOR,
        },
    }
}

fn slot_code(slot: CheckpointSlot) -> u16 {
    match slot {
        CheckpointSlot::A => 0,
        CheckpointSlot::B => 1,
        CheckpointSlot::CandidateA => 2,
        CheckpointSlot::CandidateB => 3,
    }
}

fn slot_for_generation(generation: u64) -> CheckpointSlot {
    if generation % 2 == 1 {
        CheckpointSlot::A
    } else {
        CheckpointSlot::B
    }
}

fn candidate_slot_for_generation(generation: u64) -> CheckpointSlot {
    if generation % 2 == 1 {
        CheckpointSlot::CandidateA
    } else {
        CheckpointSlot::CandidateB
    }
}

fn is_candidate_slot(slot: CheckpointSlot) -> bool {
    matches!(
        slot,
        CheckpointSlot::CandidateA | CheckpointSlot::CandidateB
    )
}

fn slot_from_code(code: u16) -> Result<CheckpointSlot, GeneralStoragePersistenceError> {
    match code {
        0 => Ok(CheckpointSlot::A),
        1 => Ok(CheckpointSlot::B),
        2 => Ok(CheckpointSlot::CandidateA),
        3 => Ok(CheckpointSlot::CandidateB),
        _ => Err(GeneralStoragePersistenceError::BadSnapshot),
    }
}

fn sector_for_image_index(slot: CheckpointSlot, image_index: usize) -> u64 {
    match image_index {
        HEADER_IMAGE_INDEX => match slot {
            CheckpointSlot::A => OBJECT_SERVICE_SLOT_A_HEADER_SECTOR,
            CheckpointSlot::B => OBJECT_SERVICE_SLOT_B_HEADER_SECTOR,
            CheckpointSlot::CandidateA => OBJECT_SERVICE_CANDIDATE_SLOT_A_HEADER_SECTOR,
            CheckpointSlot::CandidateB => OBJECT_SERVICE_CANDIDATE_SLOT_B_HEADER_SECTOR,
        },
        index if (OBJECT_IMAGE_INDEX..RELATIONSHIP_IMAGE_INDEX).contains(&index) => {
            slot_layout(slot).object_table_sector + (index - OBJECT_IMAGE_INDEX) as u64
        }
        index if (RELATIONSHIP_IMAGE_INDEX..REVISION_IMAGE_INDEX).contains(&index) => {
            slot_layout(slot).relationship_table_sector + (index - RELATIONSHIP_IMAGE_INDEX) as u64
        }
        index if (REVISION_IMAGE_INDEX..COMMIT_IMAGE_INDEX).contains(&index) => {
            slot_layout(slot).revision_table_sector + (index - REVISION_IMAGE_INDEX) as u64
        }
        COMMIT_IMAGE_INDEX => slot_layout(slot).commit_sector,
        _ => OBJECT_SERVICE_TORN_SECTOR,
    }
}

fn read_checkpoint_sector(
    device: BlockDeviceInfo,
    sector: u64,
) -> Result<[u8; SECTOR_SIZE], GeneralStoragePersistenceError> {
    #[cfg(not(test))]
    {
        block_device::read_sector(device, sector).map_err(GeneralStoragePersistenceError::from)
    }

    #[cfg(test)]
    {
        let _ = device;
        let index = sector as usize;
        if index >= TEST_CHECKPOINT_SECTOR_COUNT {
            return Err(GeneralStoragePersistenceError::BadSnapshot);
        }
        Ok(TEST_CHECKPOINT_SECTORS.lock().unwrap()[index])
    }
}

fn write_checkpoint_sector(
    device: BlockDeviceInfo,
    sector: u64,
    bytes: &[u8; SECTOR_SIZE],
) -> Result<(), GeneralStoragePersistenceError> {
    #[cfg(not(test))]
    {
        block_device::write_sector(device, sector, bytes)
            .map_err(GeneralStoragePersistenceError::from)
    }

    #[cfg(test)]
    {
        let _ = device;
        let index = sector as usize;
        if index >= TEST_CHECKPOINT_SECTOR_COUNT {
            return Err(GeneralStoragePersistenceError::BadSnapshot);
        }
        TEST_CHECKPOINT_SECTORS.lock().unwrap()[index] = *bytes;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn reset_checkpoint_storage_for_test() {
    *TEST_CHECKPOINT_SECTORS.lock().unwrap() = [[0; SECTOR_SIZE]; TEST_CHECKPOINT_SECTOR_COUNT];
}

fn write_u16(bytes: &mut [u8; SECTOR_SIZE], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8; SECTOR_SIZE], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u16(bytes: &[u8; SECTOR_SIZE], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn read_u64(bytes: &[u8; SECTOR_SIZE], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[cfg(test)]
fn encode_slot_for_test(
    slot: CheckpointSlot,
    snapshot: &ObjectServiceSnapshot,
    committed: bool,
) -> ObjectServiceSlotImage {
    encode_slot(slot, snapshot, committed)
}

#[cfg(test)]
fn decode_slot_for_test(
    image: &ObjectServiceSlotImage,
) -> Result<ObjectServiceSnapshot, GeneralStoragePersistenceError> {
    decode_slot(image)
}

#[cfg(test)]
fn recover_from_slot_images_for_test(
    first: &ObjectServiceSlotImage,
    second: &ObjectServiceSlotImage,
) -> Result<Option<ObjectServiceSnapshot>, GeneralStoragePersistenceError> {
    recover_from_slot_images(first, second)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_device::BlockDeviceInfo;
    use crate::general_storage_persistence::GeneralStoragePersistenceError;
    use crate::object_relationships::{EXTERNAL_WORKSPACE_OBJECT_ID, SHELL_WORKSPACE_OBJECT_ID};
    use crate::revision_history::RevisionRecord;
    use crate::service_identity::ServiceIdentityTable;
    use crate::shell_objects::{ObjectId, ObjectKind};
    use crate::tasks::TaskId;
    use crate::typed_object_format::{TypedObjectField, TypedObjectRecord};
    use pythos_shared::package_abi::{PACKAGE_CONTENT_BASE_SECTOR, PACKAGE_CONTENT_MAX_BLOCKS};
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn note(object_id: u64, text: &[u8]) -> TypedObjectRecord {
        let mut record = TypedObjectRecord::new(ObjectId::new(object_id), ObjectKind::Note, 1);
        record
            .push_field(TypedObjectField::new(1, 1, text).unwrap())
            .unwrap();
        record
    }

    fn snapshot(generation: u64, object_id: u64, workspace_id: u64) -> ObjectServiceSnapshot {
        let mut identities = ServiceIdentityTable::new();
        let writer = identities.register_task(TaskId::new(190)).unwrap();
        let object = note(object_id, b"hello");
        let mut objects = [None; OBJECT_SERVICE_OBJECT_CAPACITY];
        objects[0] = Some(ObjectExtentRecord {
            extent_start: 3,
            extent_len: 1,
            object,
        });
        let mut relationships = [None; OBJECT_SERVICE_WORKSPACE_RELATIONSHIP_CAPACITY];
        relationships[0] = Some(WorkspaceRelationshipRecord {
            object_id: ObjectId::new(object_id),
            workspace_id: ObjectId::new(workspace_id),
        });
        let mut current_revisions = [None; OBJECT_SERVICE_CURRENT_REVISION_CAPACITY];
        current_revisions[0] = Some(RevisionRecord::new(
            ObjectId::new(object_id),
            1,
            generation,
            writer,
            object,
        ));

        ObjectServiceSnapshot {
            generation,
            allocated_bitmap: 1 << 3,
            objects,
            workspace_relationships: relationships,
            current_revisions,
            prior_revisions: [None; OBJECT_SERVICE_PRIOR_REVISION_CAPACITY],
        }
    }

    #[test]
    fn slot_encoding_retains_extents_relationships_and_revisions() {
        let snapshot = snapshot(7, 1042, SHELL_WORKSPACE_OBJECT_ID);
        let image = encode_slot_for_test(CheckpointSlot::A, &snapshot, true);

        let decoded = decode_slot_for_test(&image).unwrap();

        assert_eq!(decoded.generation, 7);
        assert_eq!(decoded.allocated_bitmap, 1 << 3);
        assert_eq!(decoded.objects[0].unwrap().extent_start, 3);
        assert_eq!(
            decoded.objects[0].unwrap().object.object_id(),
            ObjectId::new(1042)
        );
        assert_eq!(
            decoded.workspace_relationships[0].unwrap().workspace_id,
            ObjectId::new(SHELL_WORKSPACE_OBJECT_ID)
        );
        assert_eq!(decoded.current_revisions[0].unwrap().revision(), 1);
    }

    #[test]
    fn object_checkpoint_identity_identical_snapshots_have_identical_root_digest() {
        let first = snapshot(8, 1042, SHELL_WORKSPACE_OBJECT_ID);
        let second = snapshot(8, 1042, SHELL_WORKSPACE_OBJECT_ID);

        assert_eq!(
            object_checkpoint_identity(&first).root_digest,
            object_checkpoint_identity(&second).root_digest
        );
    }

    #[test]
    fn object_checkpoint_identity_changes_when_object_record_changes() {
        let first = snapshot(8, 1042, SHELL_WORKSPACE_OBJECT_ID);
        let mut second = snapshot(8, 1042, SHELL_WORKSPACE_OBJECT_ID);
        second.objects[0].as_mut().unwrap().object = note(1042, b"changed");

        assert_ne!(
            object_checkpoint_identity(&first).root_digest,
            object_checkpoint_identity(&second).root_digest
        );
    }

    #[test]
    fn object_checkpoint_identity_preserves_generation() {
        let snapshot = snapshot(13, 1042, SHELL_WORKSPACE_OBJECT_ID);
        let identity = object_checkpoint_identity(&snapshot);

        assert_eq!(identity.generation, 13);
        assert_ne!(identity.root_digest, [0; 32]);
    }

    #[test]
    fn recovery_selects_highest_committed_generation() {
        let first = encode_slot_for_test(
            CheckpointSlot::A,
            &snapshot(1, 1042, SHELL_WORKSPACE_OBJECT_ID),
            true,
        );
        let second = encode_slot_for_test(
            CheckpointSlot::B,
            &snapshot(2, 2001, EXTERNAL_WORKSPACE_OBJECT_ID),
            true,
        );

        let recovered = recover_from_slot_images_for_test(&first, &second)
            .unwrap()
            .unwrap();

        assert_eq!(recovered.generation, 2);
        assert_eq!(
            recovered.workspace_relationships[0].unwrap().workspace_id,
            ObjectId::new(EXTERNAL_WORKSPACE_OBJECT_ID)
        );
    }

    #[test]
    fn torn_inactive_slot_keeps_previous_committed_checkpoint() {
        let committed = encode_slot_for_test(
            CheckpointSlot::A,
            &snapshot(3, 1042, SHELL_WORKSPACE_OBJECT_ID),
            true,
        );
        let torn = encode_slot_for_test(
            CheckpointSlot::B,
            &snapshot(4, 1043, SHELL_WORKSPACE_OBJECT_ID),
            false,
        );

        let recovered = recover_from_slot_images_for_test(&committed, &torn)
            .unwrap()
            .unwrap();

        assert_eq!(recovered.generation, 3);
        assert_eq!(
            recovered.objects[0].unwrap().object.object_id(),
            ObjectId::new(1042)
        );
    }

    #[test]
    fn reused_slot_torn_rewrite_with_old_commit_marker_is_rejected() {
        let committed = encode_slot_for_test(
            CheckpointSlot::A,
            &snapshot(5, 1042, SHELL_WORKSPACE_OBJECT_ID),
            true,
        );
        let mut torn_reuse = encode_slot_for_test(
            CheckpointSlot::A,
            &snapshot(7, 1043, SHELL_WORKSPACE_OBJECT_ID),
            false,
        );
        torn_reuse.sectors[COMMIT_IMAGE_INDEX] = committed.sectors[COMMIT_IMAGE_INDEX];

        let recovered = recover_from_slot_images_for_test(&torn_reuse, &committed)
            .unwrap()
            .unwrap();

        assert_eq!(recovered.generation, 5);
        assert_eq!(
            recovered.objects[0].unwrap().object.object_id(),
            ObjectId::new(1042)
        );
    }

    #[test]
    fn slot_probe_requires_header_and_commit_magic_before_full_decode() {
        let committed = encode_slot_for_test(
            CheckpointSlot::A,
            &snapshot(5, 1042, SHELL_WORKSPACE_OBJECT_ID),
            true,
        );
        let blank = [0; SECTOR_SIZE];

        assert!(slot_probe_is_committed(
            &committed.sectors[HEADER_IMAGE_INDEX],
            &committed.sectors[COMMIT_IMAGE_INDEX]
        ));
        assert!(!slot_probe_is_committed(
            &blank,
            &committed.sectors[COMMIT_IMAGE_INDEX]
        ));
        assert!(!slot_probe_is_committed(
            &committed.sectors[HEADER_IMAGE_INDEX],
            &blank
        ));
    }

    #[test]
    fn package_candidate_checkpoint_storage_does_not_overlap_package_content() {
        let package_content_first = PACKAGE_CONTENT_BASE_SECTOR;
        let package_content_last =
            PACKAGE_CONTENT_BASE_SECTOR + u64::from(PACKAGE_CONTENT_MAX_BLOCKS) - 1;
        let candidate_first = OBJECT_SERVICE_CANDIDATE_HEADER_SECTOR;
        let candidate_last = OBJECT_SERVICE_CANDIDATE_SLOT_B_COMMIT_SECTOR;

        assert_eq!(
            candidate_first,
            PACKAGE_CONTENT_BASE_SECTOR + u64::from(PACKAGE_CONTENT_MAX_BLOCKS)
        );
        assert!(
            candidate_last < package_content_first || candidate_first > package_content_last,
            "candidate checkpoint sectors {candidate_first}..={candidate_last} overlap package content sectors {package_content_first}..={package_content_last}"
        );
    }

    #[test]
    fn package_candidate_checkpoint_is_durable_but_not_ordinary_recovery_eligible() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(512, 8);
        let ordinary = snapshot(1, 1042, SHELL_WORKSPACE_OBJECT_ID);
        let candidate_world = snapshot(2, 2001, EXTERNAL_WORKSPACE_OBJECT_ID);

        write_object_service_checkpoint(device, &ordinary).unwrap();
        let candidate =
            write_object_service_candidate_checkpoint(device, &candidate_world).unwrap();

        let ordinary_recovered = read_object_service_checkpoint(device).unwrap().unwrap();
        assert_eq!(ordinary_recovered.generation, 1);
        assert_eq!(
            ordinary_recovered.objects[0].unwrap().object.object_id(),
            ObjectId::new(1042)
        );

        let candidate_recovered =
            read_object_service_candidate_checkpoint(device, candidate.identity).unwrap();
        assert_eq!(candidate_recovered.generation, candidate.generation);
        assert_eq!(
            candidate_recovered.objects[0].unwrap().object.object_id(),
            ObjectId::new(2001)
        );
    }

    #[test]
    fn package_candidate_checkpoint_root_mismatch_is_denied() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(512, 8);
        let candidate_world = snapshot(2, 2001, EXTERNAL_WORKSPACE_OBJECT_ID);
        let candidate =
            write_object_service_candidate_checkpoint(device, &candidate_world).unwrap();
        let mut wrong_identity = candidate.identity;
        wrong_identity.root_digest[0] ^= 0x80;

        assert_eq!(
            read_object_service_candidate_checkpoint(device, wrong_identity),
            Err(GeneralStoragePersistenceError::BadSnapshot)
        );
    }

    #[test]
    fn package_candidate_checkpoint_writes_snapshot_generation() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(512, 8);
        let candidate_world = snapshot(41, 2001, EXTERNAL_WORKSPACE_OBJECT_ID);

        let candidate =
            write_object_service_candidate_checkpoint(device, &candidate_world).unwrap();

        assert_eq!(candidate.generation, 41);
        assert_eq!(candidate.identity.generation, 41);
        assert_eq!(
            read_object_service_candidate_checkpoint(device, candidate.identity)
                .unwrap()
                .generation,
            41
        );
    }

    #[test]
    fn package_candidate_checkpoint_keeps_two_generation_selected_slots() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_checkpoint_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(512, 8);
        let anchored_world = snapshot(1, 1042, SHELL_WORKSPACE_OBJECT_ID);
        let unanchored_world = snapshot(2, 2001, EXTERNAL_WORKSPACE_OBJECT_ID);

        let anchored = write_object_service_candidate_checkpoint(device, &anchored_world).unwrap();
        let unanchored =
            write_object_service_candidate_checkpoint(device, &unanchored_world).unwrap();

        assert_ne!(anchored.identity, unanchored.identity);
        let anchored_recovered =
            read_object_service_candidate_checkpoint(device, anchored.identity).unwrap();
        let unanchored_recovered =
            read_object_service_candidate_checkpoint(device, unanchored.identity).unwrap();
        assert_eq!(
            anchored_recovered.objects[0].unwrap().object.object_id(),
            ObjectId::new(1042)
        );
        assert_eq!(
            unanchored_recovered.objects[0].unwrap().object.object_id(),
            ObjectId::new(2001)
        );
    }
}
