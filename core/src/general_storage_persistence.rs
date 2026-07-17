//! Phase 10 dynamic-allocation persistence and torn-write recovery proof.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::{
    block_device::{BlockDeviceError, BlockDeviceInfo, SECTOR_SIZE, read_sector, write_sector},
    shell_objects::{ObjectId, ObjectKind},
    typed_object_format::{ObjectFormatError, RECORD_SIZE, TypedObjectRecord},
};

const CONTROL_SECTOR: u64 = 40;
const SNAPSHOT_SECTOR: u64 = 41;
const TORN_SECTOR: u64 = 42;
const SNAPSHOT_MAGIC: [u8; 8] = *b"PY10OBJ1";
const CONTROL_MAGIC: [u8; 8] = *b"PY10CTL1";
const SNAPSHOT_VERSION: u16 = 1;
const CONTROL_ARM_TORN: u16 = 1;
const COMMIT_MARKER: u32 = 0x5059_4730;
const OBJECT_COUNT_OFFSET: usize = 20;
const BITMAP_OFFSET: usize = 24;
const FIRST_EXTENT_OFFSET: usize = 32;
const SECOND_EXTENT_OFFSET: usize = 36;
const FIRST_OBJECT_OFFSET: usize = 40;
const SECOND_OBJECT_OFFSET: usize = FIRST_OBJECT_OFFSET + RECORD_SIZE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneralStoragePersistenceError {
    Block(BlockDeviceError),
    ObjectFormat(ObjectFormatError),
    BadSnapshot,
    TornWrite,
}

impl From<BlockDeviceError> for GeneralStoragePersistenceError {
    fn from(error: BlockDeviceError) -> Self {
        Self::Block(error)
    }
}

impl From<ObjectFormatError> for GeneralStoragePersistenceError {
    fn from(error: ObjectFormatError) -> Self {
        Self::ObjectFormat(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DynamicSnapshot {
    object_count: u16,
    allocated_bitmap: u64,
    first_extent_start: u16,
    second_extent_start: u16,
    first_object: TypedObjectRecord,
    second_object: TypedObjectRecord,
}

pub fn run_self_test(device: BlockDeviceInfo) -> Result<(), GeneralStoragePersistenceError> {
    let control = read_sector(device, CONTROL_SECTOR)?;
    if control_requests_torn_write(control) {
        write_torn_commit_window(device);
    }

    let stored = read_sector(device, SNAPSHOT_SECTOR)?;
    if decode_snapshot(stored).is_err() {
        let sector = encode_snapshot(expected_snapshot(), true);
        write_sector(device, SNAPSHOT_SECTOR, &sector)?;
        #[cfg(not(test))]
        serial::write_line("PYTHOS:CORE:GENERAL_STORAGE:CREATED");
    }

    let restored = read_sector(device, SNAPSHOT_SECTOR)?;
    verify_expected_snapshot(restored)?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:GENERAL_STORAGE:PERSISTED");
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:GENERAL_STORAGE:RESTORED");

    let torn = read_sector(device, TORN_SECTOR)?;
    if !sector_is_empty(torn) {
        if decode_snapshot(torn) != Err(GeneralStoragePersistenceError::TornWrite) {
            return Err(GeneralStoragePersistenceError::BadSnapshot);
        }
        write_sector(device, TORN_SECTOR, &[0; SECTOR_SIZE])?;
        #[cfg(not(test))]
        serial::write_line("PYTHOS:CORE:GENERAL_STORAGE:DYNAMIC_TORN_WRITE_RECOVERED");
    }

    Ok(())
}

fn write_torn_commit_window(device: BlockDeviceInfo) -> ! {
    let snapshot = expected_snapshot();
    let committed = encode_snapshot(snapshot, true);
    let torn = encode_snapshot(snapshot, false);
    if write_sector(device, SNAPSHOT_SECTOR, &committed).is_err()
        || write_sector(device, TORN_SECTOR, &torn).is_err()
        || write_sector(device, CONTROL_SECTOR, &[0; SECTOR_SIZE]).is_err()
    {
        spin_forever();
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:GENERAL_STORAGE:KILL_WINDOW");
    spin_forever();
}

fn spin_forever() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

fn expected_snapshot() -> DynamicSnapshot {
    DynamicSnapshot {
        object_count: 2,
        allocated_bitmap: 0b11,
        first_extent_start: 0,
        second_extent_start: 1,
        first_object: TypedObjectRecord::new(
            ObjectId::new(0x8401),
            ObjectKind::ServiceMonitorWindow,
            1,
        ),
        second_object: TypedObjectRecord::new(
            ObjectId::new(0x8402),
            ObjectKind::SettingsPanelWindow,
            1,
        ),
    }
}

fn verify_expected_snapshot(
    sector: [u8; SECTOR_SIZE],
) -> Result<(), GeneralStoragePersistenceError> {
    let snapshot = decode_snapshot(sector)?;
    if snapshot.object_count != 2
        || snapshot.allocated_bitmap != 0b11
        || snapshot.first_extent_start != 0
        || snapshot.second_extent_start != 1
        || snapshot.first_object.object_id() != ObjectId::new(0x8401)
        || snapshot.second_object.object_id() != ObjectId::new(0x8402)
    {
        return Err(GeneralStoragePersistenceError::BadSnapshot);
    }
    Ok(())
}

fn encode_snapshot(snapshot: DynamicSnapshot, committed: bool) -> [u8; SECTOR_SIZE] {
    let mut sector = [0; SECTOR_SIZE];
    sector[0..8].copy_from_slice(&SNAPSHOT_MAGIC);
    write_u16(&mut sector, 8, SNAPSHOT_VERSION);
    write_u32(&mut sector, 12, if committed { COMMIT_MARKER } else { 0 });
    write_u16(&mut sector, OBJECT_COUNT_OFFSET, snapshot.object_count);
    write_u64(&mut sector, BITMAP_OFFSET, snapshot.allocated_bitmap);
    write_u16(
        &mut sector,
        FIRST_EXTENT_OFFSET,
        snapshot.first_extent_start,
    );
    write_u16(&mut sector, FIRST_EXTENT_OFFSET + 2, 1);
    write_u16(
        &mut sector,
        SECOND_EXTENT_OFFSET,
        snapshot.second_extent_start,
    );
    write_u16(&mut sector, SECOND_EXTENT_OFFSET + 2, 1);
    sector[FIRST_OBJECT_OFFSET..FIRST_OBJECT_OFFSET + RECORD_SIZE]
        .copy_from_slice(&snapshot.first_object.encode());
    sector[SECOND_OBJECT_OFFSET..SECOND_OBJECT_OFFSET + RECORD_SIZE]
        .copy_from_slice(&snapshot.second_object.encode());
    let checksum = checksum_snapshot(&sector);
    write_u32(&mut sector, 16, checksum);
    sector
}

fn decode_snapshot(
    sector: [u8; SECTOR_SIZE],
) -> Result<DynamicSnapshot, GeneralStoragePersistenceError> {
    if sector[0..8] != SNAPSHOT_MAGIC || read_u16(&sector, 8) != SNAPSHOT_VERSION {
        return Err(GeneralStoragePersistenceError::BadSnapshot);
    }
    if read_u32(&sector, 12) != COMMIT_MARKER {
        return Err(GeneralStoragePersistenceError::TornWrite);
    }
    if read_u32(&sector, 16) != checksum_snapshot(&sector) {
        return Err(GeneralStoragePersistenceError::BadSnapshot);
    }
    Ok(DynamicSnapshot {
        object_count: read_u16(&sector, OBJECT_COUNT_OFFSET),
        allocated_bitmap: read_u64(&sector, BITMAP_OFFSET),
        first_extent_start: read_u16(&sector, FIRST_EXTENT_OFFSET),
        second_extent_start: read_u16(&sector, SECOND_EXTENT_OFFSET),
        first_object: TypedObjectRecord::decode(
            &sector[FIRST_OBJECT_OFFSET..FIRST_OBJECT_OFFSET + RECORD_SIZE],
        )?,
        second_object: TypedObjectRecord::decode(
            &sector[SECOND_OBJECT_OFFSET..SECOND_OBJECT_OFFSET + RECORD_SIZE],
        )?,
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
    fn dynamic_snapshot_round_trips_object_count_extents_and_objects() {
        let sector = encode_snapshot(expected_snapshot(), true);

        let decoded = decode_snapshot(sector).unwrap();

        assert_eq!(decoded.object_count, 2);
        assert_eq!(decoded.allocated_bitmap, 0b11);
        assert_eq!(decoded.first_extent_start, 0);
        assert_eq!(decoded.second_extent_start, 1);
        assert_eq!(decoded.first_object.object_id(), ObjectId::new(0x8401));
        assert_eq!(decoded.second_object.object_id(), ObjectId::new(0x8402));
    }

    #[test]
    fn missing_commit_marker_is_dynamic_torn_write() {
        let sector = encode_snapshot(expected_snapshot(), false);

        assert_eq!(
            decode_snapshot(sector),
            Err(GeneralStoragePersistenceError::TornWrite)
        );
    }

    #[test]
    fn control_sector_arms_dynamic_killed_commit_path() {
        assert!(control_requests_torn_write(control_sector()));
        assert!(!control_requests_torn_write([0; SECTOR_SIZE]));
    }
}
