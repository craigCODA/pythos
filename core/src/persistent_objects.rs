//! Phase 7 persistent object-store snapshot over the virtio block device.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::{
    block_device::{BlockDeviceError, BlockDeviceInfo, SECTOR_SIZE, read_sector, write_sector},
    object_relationships::{ObjectRelationship, RelationshipKind},
    revision_history::{RevisionHistory, RevisionHistoryError},
    service_identity::{ServiceIdentityError, ServiceIdentityTable},
    shell_apps::{ShellAppError, register_first_party_apps},
    shell_objects::{ObjectId, ObjectKind},
    tasks::TaskId,
    typed_object_format::{ObjectFormatError, RECORD_SIZE, TypedObjectRecord},
    workspace_objects::{
        WORKSPACE_SESSION_OBJECT_ID, WorkspaceObjectError, workspace_session_record,
    },
};

const CONTROL_SECTOR: u64 = 30;
const SNAPSHOT_SECTOR: u64 = 31;
const TORN_SECTOR: u64 = 32;
const SNAPSHOT_MAGIC: [u8; 8] = *b"PY7OBJ01";
const CONTROL_MAGIC: [u8; 8] = *b"PY7CTL01";
const SNAPSHOT_VERSION: u16 = 1;
const CONTROL_ARM_TORN: u16 = 1;
const COMMIT_MARKER: u32 = 0x5059_434D;
const OBJECT_OFFSET: usize = 24;
const REL_SOURCE_OFFSET: usize = OBJECT_OFFSET + RECORD_SIZE;
const REL_TARGET_OFFSET: usize = REL_SOURCE_OFFSET + 8;
const REL_KIND_OFFSET: usize = REL_TARGET_OFFSET + 8;
const REVISION_COUNT_OFFSET: usize = REL_KIND_OFFSET + 8;
const CURRENT_REVISION_OFFSET: usize = REVISION_COUNT_OFFSET + 8;
const CURRENT_TIMESTAMP_OFFSET: usize = CURRENT_REVISION_OFFSET + 8;
const WRITER_OFFSET: usize = CURRENT_TIMESTAMP_OFFSET + 8;
const EXPECTED_SCHEMA_VERSION: u16 = 1;
const EXPECTED_PRIOR_REVISIONS: u64 = 1;
const EXPECTED_CURRENT_REVISION: u64 = 2;
const EXPECTED_TIMESTAMP: u64 = 420;
const EXPECTED_WRITER_TASK: u64 = 96;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentObjectError {
    Block(BlockDeviceError),
    ObjectFormat(ObjectFormatError),
    RevisionHistory(RevisionHistoryError),
    ServiceIdentity(ServiceIdentityError),
    ShellApp(ShellAppError),
    Workspace(WorkspaceObjectError),
    BadSnapshot,
    TornWrite,
}

#[cfg(not(test))]
pub fn write_error(error: PersistentObjectError) {
    let marker = match error {
        PersistentObjectError::Block(BlockDeviceError::InvalidQueue) => {
            "PYTHOS:CORE:OBJECT_STORE:ERROR:INVALID_QUEUE"
        }
        PersistentObjectError::Block(BlockDeviceError::DmaAddress) => {
            "PYTHOS:CORE:OBJECT_STORE:ERROR:DMA_ADDRESS"
        }
        PersistentObjectError::Block(BlockDeviceError::RequestFailed) => {
            "PYTHOS:CORE:OBJECT_STORE:ERROR:REQUEST_FAILED"
        }
        PersistentObjectError::Block(BlockDeviceError::Timeout) => {
            "PYTHOS:CORE:OBJECT_STORE:ERROR:REQUEST_TIMEOUT"
        }
        PersistentObjectError::Block(_) => "PYTHOS:CORE:OBJECT_STORE:ERROR:BLOCK",
        PersistentObjectError::TornWrite => "PYTHOS:CORE:OBJECT_STORE:ERROR:TORN_WRITE",
        PersistentObjectError::BadSnapshot => "PYTHOS:CORE:OBJECT_STORE:ERROR:BAD_SNAPSHOT",
        _ => "PYTHOS:CORE:OBJECT_STORE:ERROR",
    };
    serial::write_line(marker);
}

impl From<BlockDeviceError> for PersistentObjectError {
    fn from(error: BlockDeviceError) -> Self {
        Self::Block(error)
    }
}

impl From<ObjectFormatError> for PersistentObjectError {
    fn from(error: ObjectFormatError) -> Self {
        Self::ObjectFormat(error)
    }
}

impl From<RevisionHistoryError> for PersistentObjectError {
    fn from(error: RevisionHistoryError) -> Self {
        Self::RevisionHistory(error)
    }
}

impl From<ServiceIdentityError> for PersistentObjectError {
    fn from(error: ServiceIdentityError) -> Self {
        Self::ServiceIdentity(error)
    }
}

impl From<ShellAppError> for PersistentObjectError {
    fn from(error: ShellAppError) -> Self {
        Self::ShellApp(error)
    }
}

impl From<WorkspaceObjectError> for PersistentObjectError {
    fn from(error: WorkspaceObjectError) -> Self {
        Self::Workspace(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    object: TypedObjectRecord,
    relationship: ObjectRelationship,
    prior_revision_count: u64,
    current_revision: u64,
    current_timestamp: u64,
    writer_raw: u64,
}

pub fn run_self_test(device: BlockDeviceInfo) -> Result<(), PersistentObjectError> {
    let control = read_sector(device, CONTROL_SECTOR)?;
    if control_requests_torn_write(control) {
        write_torn_commit_window(device);
    }

    let stored = read_sector(device, SNAPSHOT_SECTOR)?;
    if decode_snapshot(stored).is_err() {
        let snapshot = expected_snapshot()?;
        let sector = encode_snapshot(snapshot, true);
        write_sector(device, SNAPSHOT_SECTOR, &sector)?;
        #[cfg(not(test))]
        serial::write_line("PYTHOS:CORE:OBJECT_STORE:CREATED");
    }

    let restored = read_sector(device, SNAPSHOT_SECTOR)?;
    verify_expected_snapshot(restored)?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT_STORE:PERSISTED");
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT_STORE:RESTORED");

    let torn = read_sector(device, TORN_SECTOR)?;
    if !sector_is_empty(torn) {
        if decode_snapshot(torn) != Err(PersistentObjectError::TornWrite) {
            return Err(PersistentObjectError::BadSnapshot);
        }
        write_sector(device, TORN_SECTOR, &[0; SECTOR_SIZE])?;
        #[cfg(not(test))]
        serial::write_line("PYTHOS:CORE:OBJECT_STORE:TORN_WRITE_RECOVERED");
    }
    Ok(())
}

fn write_torn_commit_window(device: BlockDeviceInfo) -> ! {
    let snapshot = match expected_snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => spin_forever(),
    };
    let committed = encode_snapshot(snapshot, true);
    let torn = encode_snapshot(snapshot, false);
    if write_sector(device, SNAPSHOT_SECTOR, &committed).is_err()
        || write_sector(device, TORN_SECTOR, &torn).is_err()
        || write_sector(device, CONTROL_SECTOR, &[0; SECTOR_SIZE]).is_err()
    {
        spin_forever();
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT_STORE:KILL_WINDOW");
    spin_forever();
}

fn spin_forever() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

fn expected_snapshot() -> Result<Snapshot, PersistentObjectError> {
    let apps = register_first_party_apps()?;
    let workspace = workspace_session_record(&apps)?;
    let settings_panel =
        TypedObjectRecord::new(ObjectId::new(0x7204), ObjectKind::SettingsPanelWindow, 1);
    let relationship = ObjectRelationship::new(
        workspace.object_id(),
        RelationshipKind::DependsOn,
        settings_panel.object_id(),
    );
    let mut identities = ServiceIdentityTable::new();
    let writer = identities.register_task(TaskId::new(EXPECTED_WRITER_TASK))?;
    let mut history = RevisionHistory::new();
    history.create_object(workspace, 400, writer)?;
    history.update_object(workspace, EXPECTED_TIMESTAMP, writer)?;
    let current = history
        .current_revision(WORKSPACE_SESSION_OBJECT_ID)
        .ok_or(PersistentObjectError::BadSnapshot)?;
    Ok(Snapshot {
        object: current.object(),
        relationship,
        prior_revision_count: history.revision_count(WORKSPACE_SESSION_OBJECT_ID) as u64,
        current_revision: current.revision(),
        current_timestamp: current.timestamp_ticks(),
        writer_raw: current.writer().raw(),
    })
}

fn verify_expected_snapshot(sector: [u8; SECTOR_SIZE]) -> Result<(), PersistentObjectError> {
    let snapshot = decode_snapshot(sector)?;
    if snapshot.object.object_id() != WORKSPACE_SESSION_OBJECT_ID
        || snapshot.object.object_kind() != ObjectKind::WorkspaceSession
        || snapshot.object.schema_version() != EXPECTED_SCHEMA_VERSION
        || snapshot.relationship.source() != WORKSPACE_SESSION_OBJECT_ID
        || snapshot.relationship.kind() != RelationshipKind::DependsOn
        || snapshot.relationship.target() != ObjectId::new(0x7204)
        || snapshot.prior_revision_count != EXPECTED_PRIOR_REVISIONS
        || snapshot.current_revision != EXPECTED_CURRENT_REVISION
        || snapshot.current_timestamp != EXPECTED_TIMESTAMP
        || snapshot.writer_raw != 1
    {
        return Err(PersistentObjectError::BadSnapshot);
    }
    Ok(())
}

fn encode_snapshot(snapshot: Snapshot, committed: bool) -> [u8; SECTOR_SIZE] {
    let mut sector = [0; SECTOR_SIZE];
    sector[0..8].copy_from_slice(&SNAPSHOT_MAGIC);
    write_u16(&mut sector, 8, SNAPSHOT_VERSION);
    write_u32(&mut sector, 12, if committed { COMMIT_MARKER } else { 0 });
    sector[OBJECT_OFFSET..OBJECT_OFFSET + RECORD_SIZE].copy_from_slice(&snapshot.object.encode());
    write_u64(
        &mut sector,
        REL_SOURCE_OFFSET,
        snapshot.relationship.source().raw(),
    );
    write_u64(
        &mut sector,
        REL_TARGET_OFFSET,
        snapshot.relationship.target().raw(),
    );
    write_u16(
        &mut sector,
        REL_KIND_OFFSET,
        relationship_kind_code(snapshot.relationship.kind()),
    );
    write_u64(
        &mut sector,
        REVISION_COUNT_OFFSET,
        snapshot.prior_revision_count,
    );
    write_u64(
        &mut sector,
        CURRENT_REVISION_OFFSET,
        snapshot.current_revision,
    );
    write_u64(
        &mut sector,
        CURRENT_TIMESTAMP_OFFSET,
        snapshot.current_timestamp,
    );
    write_u64(&mut sector, WRITER_OFFSET, snapshot.writer_raw);
    let checksum = checksum_snapshot(&sector);
    write_u32(&mut sector, 16, checksum);
    sector
}

fn decode_snapshot(sector: [u8; SECTOR_SIZE]) -> Result<Snapshot, PersistentObjectError> {
    if sector[0..8] != SNAPSHOT_MAGIC || read_u16(&sector, 8) != SNAPSHOT_VERSION {
        return Err(PersistentObjectError::BadSnapshot);
    }
    if read_u32(&sector, 12) != COMMIT_MARKER {
        return Err(PersistentObjectError::TornWrite);
    }
    if read_u32(&sector, 16) != checksum_snapshot(&sector) {
        return Err(PersistentObjectError::BadSnapshot);
    }
    let object = TypedObjectRecord::decode(&sector[OBJECT_OFFSET..OBJECT_OFFSET + RECORD_SIZE])?;
    let relationship = ObjectRelationship::new(
        ObjectId::new(read_u64(&sector, REL_SOURCE_OFFSET)),
        relationship_kind_from_code(read_u16(&sector, REL_KIND_OFFSET))?,
        ObjectId::new(read_u64(&sector, REL_TARGET_OFFSET)),
    );
    Ok(Snapshot {
        object,
        relationship,
        prior_revision_count: read_u64(&sector, REVISION_COUNT_OFFSET),
        current_revision: read_u64(&sector, CURRENT_REVISION_OFFSET),
        current_timestamp: read_u64(&sector, CURRENT_TIMESTAMP_OFFSET),
        writer_raw: read_u64(&sector, WRITER_OFFSET),
    })
}

fn control_requests_torn_write(sector: [u8; SECTOR_SIZE]) -> bool {
    sector[0..8] == CONTROL_MAGIC && read_u16(&sector, 8) == CONTROL_ARM_TORN
}

fn sector_is_empty(sector: [u8; SECTOR_SIZE]) -> bool {
    let mut index = 0;
    while index < SECTOR_SIZE {
        if sector[index] != 0 {
            return false;
        }
        index += 1;
    }
    true
}

fn relationship_kind_code(kind: RelationshipKind) -> u16 {
    match kind {
        RelationshipKind::Blocks => 1,
        RelationshipKind::CreatedBy => 2,
        RelationshipKind::DependsOn => 3,
        RelationshipKind::BelongsTo => 4,
    }
}

fn relationship_kind_from_code(code: u16) -> Result<RelationshipKind, PersistentObjectError> {
    match code {
        1 => Ok(RelationshipKind::Blocks),
        2 => Ok(RelationshipKind::CreatedBy),
        3 => Ok(RelationshipKind::DependsOn),
        4 => Ok(RelationshipKind::BelongsTo),
        _ => Err(PersistentObjectError::BadSnapshot),
    }
}

fn checksum_snapshot(sector: &[u8; SECTOR_SIZE]) -> u32 {
    let mut checksum = 0x811C_9DC5u32;
    let mut index = 0;
    while index < SECTOR_SIZE {
        if !(16..20).contains(&index) {
            checksum ^= u32::from(sector[index]);
            checksum = checksum.wrapping_mul(0x0100_0193);
        }
        index += 1;
    }
    checksum
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset] = value as u8;
    bytes[offset + 1] = (value >> 8) as u8;
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    let mut remaining = value;
    let mut index = 0;
    while index < 4 {
        bytes[offset + index] = remaining as u8;
        remaining >>= 8;
        index += 1;
    }
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    let mut remaining = value;
    let mut index = 0;
    while index < 8 {
        bytes[offset + index] = remaining as u8;
        remaining >>= 8;
        index += 1;
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from(bytes[offset]) | (u16::from(bytes[offset + 1]) << 8)
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from(bytes[offset])
        | (u32::from(bytes[offset + 1]) << 8)
        | (u32::from(bytes[offset + 2]) << 16)
        | (u32::from(bytes[offset + 3]) << 24)
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    let mut value = 0u64;
    let mut index = 0;
    while index < 8 {
        value |= u64::from(bytes[offset + index]) << (index * 8);
        index += 1;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control_sector() -> [u8; SECTOR_SIZE] {
        let mut sector = [0; SECTOR_SIZE];
        sector[0..8].copy_from_slice(&CONTROL_MAGIC);
        write_u16(&mut sector, 8, CONTROL_ARM_TORN);
        sector
    }

    #[test]
    fn snapshot_sector_round_trips_object_relationship_and_revision_metadata() {
        let snapshot = expected_snapshot().unwrap();
        let sector = encode_snapshot(snapshot, true);

        let decoded = decode_snapshot(sector).unwrap();

        assert_eq!(decoded.object.object_id(), WORKSPACE_SESSION_OBJECT_ID);
        assert_eq!(decoded.object.object_kind(), ObjectKind::WorkspaceSession);
        assert_eq!(decoded.relationship.kind(), RelationshipKind::DependsOn);
        assert_eq!(decoded.prior_revision_count, EXPECTED_PRIOR_REVISIONS);
        assert_eq!(decoded.current_revision, EXPECTED_CURRENT_REVISION);
        assert_eq!(decoded.writer_raw, 1);
    }

    #[test]
    fn missing_commit_marker_is_torn_write_not_snapshot_success() {
        let snapshot = expected_snapshot().unwrap();
        let sector = encode_snapshot(snapshot, false);

        assert_eq!(
            decode_snapshot(sector),
            Err(PersistentObjectError::TornWrite)
        );
    }

    #[test]
    fn control_sector_arms_killed_commit_path() {
        assert!(control_requests_torn_write(control_sector()));
        assert!(!control_requests_torn_write([0; SECTOR_SIZE]));
    }
}
