//! Append-only journal intent path for Phase 7 storage writes.
#![cfg_attr(test, allow(dead_code))]

use crate::block_device::BlockDeviceInfo;
use crate::capabilities::{CapabilityError, CapabilityTable, RightsMask};
#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityError, ServiceIdentityTable};
use crate::storage_service::{
    STORAGE_RESOURCE_ID, StorageAccess, StorageOperation, StorageRequest, StorageService,
    StorageServiceError,
};
use crate::tasks::TaskId;

const JOURNAL_CAPACITY: usize = 8;
const COMMIT_MARKER: u32 = 0x5059_434D;
const EMPTY_RECORD: JournalRecord = JournalRecord {
    sequence: 0,
    operation: StorageOperation::Read,
    start_sector: 0,
    sector_count: 0,
    checksum: 0,
    commit_marker: 0,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalError {
    Full,
    ReadIsNotJournaled,
    MissingCommitMarker,
    ChecksumMismatch,
    Storage(StorageServiceError),
    Identity(ServiceIdentityError),
}

impl From<StorageServiceError> for JournalError {
    fn from(error: StorageServiceError) -> Self {
        Self::Storage(error)
    }
}

impl From<ServiceIdentityError> for JournalError {
    fn from(error: ServiceIdentityError) -> Self {
        Self::Identity(error)
    }
}

impl From<CapabilityError> for JournalError {
    fn from(error: CapabilityError) -> Self {
        Self::Storage(StorageServiceError::Capability(error))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalRecord {
    sequence: u64,
    operation: StorageOperation,
    start_sector: u64,
    sector_count: u16,
    checksum: u32,
    commit_marker: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryReport {
    replayed_records: usize,
    rolled_back_records: usize,
    last_sequence: u64,
}

impl RecoveryReport {
    pub const fn replayed_records(self) -> usize {
        self.replayed_records
    }

    pub const fn rolled_back_records(self) -> usize {
        self.rolled_back_records
    }

    pub const fn last_sequence(self) -> u64 {
        self.last_sequence
    }
}

impl JournalRecord {
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    pub const fn start_sector(self) -> u64 {
        self.start_sector
    }

    pub const fn sector_count(self) -> u16 {
        self.sector_count
    }

    pub const fn checksum(self) -> u32 {
        self.checksum
    }

    pub const fn is_committed(self) -> bool {
        self.commit_marker == COMMIT_MARKER
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppendOnlyJournal {
    records: [JournalRecord; JOURNAL_CAPACITY],
    len: usize,
    next_sequence: u64,
}

impl AppendOnlyJournal {
    pub const fn new() -> Self {
        Self {
            records: [EMPTY_RECORD; JOURNAL_CAPACITY],
            len: 0,
            next_sequence: 1,
        }
    }

    pub fn append_write(&mut self, access: StorageAccess) -> Result<JournalRecord, JournalError> {
        if access.operation() != StorageOperation::Write {
            return Err(JournalError::ReadIsNotJournaled);
        }
        if self.len == JOURNAL_CAPACITY {
            return Err(JournalError::Full);
        }
        let record = JournalRecord {
            sequence: self.next_sequence,
            operation: access.operation(),
            start_sector: access.start_sector(),
            sector_count: access.sector_count(),
            checksum: 0,
            commit_marker: 0,
        };
        self.records[self.len] = record;
        self.len += 1;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        Ok(record)
    }

    pub fn commit_last(&mut self) -> Result<JournalRecord, JournalError> {
        if self.len == 0 {
            return Err(JournalError::MissingCommitMarker);
        }
        let index = self.len - 1;
        let mut record = self.records[index];
        record.checksum = checksum_for(record);
        record.commit_marker = COMMIT_MARKER;
        self.records[index] = record;
        validate_committed(record)?;
        Ok(record)
    }

    pub const fn len(self) -> usize {
        self.len
    }

    pub fn last(self) -> Option<JournalRecord> {
        if self.len == 0 {
            None
        } else {
            Some(self.records[self.len - 1])
        }
    }
}

pub fn journal_first_write(
    journal: &mut AppendOnlyJournal,
    service: StorageService,
    table: &CapabilityTable,
    caller: ServiceId,
    handle: crate::capabilities::CapabilityHandle,
    request: StorageRequest,
) -> Result<JournalRecord, JournalError> {
    let access = service.authorize_request(table, caller, handle, request)?;
    journal.append_write(access)
}

pub fn validate_committed(record: JournalRecord) -> Result<(), JournalError> {
    if record.commit_marker != COMMIT_MARKER {
        return Err(JournalError::MissingCommitMarker);
    }
    if record.checksum != checksum_for(record) {
        return Err(JournalError::ChecksumMismatch);
    }
    Ok(())
}

pub fn recover_committed_prefix(journal: AppendOnlyJournal) -> RecoveryReport {
    let mut replayed_records = 0;
    let mut last_sequence = 0;
    while replayed_records < journal.len {
        let record = journal.records[replayed_records];
        if validate_committed(record).is_err() {
            return RecoveryReport {
                replayed_records,
                rolled_back_records: journal.len - replayed_records,
                last_sequence,
            };
        }
        replayed_records += 1;
        last_sequence = record.sequence;
    }
    RecoveryReport {
        replayed_records,
        rolled_back_records: 0,
        last_sequence,
    }
}

fn checksum_for(record: JournalRecord) -> u32 {
    let mut checksum = 0x811C_9DC5;
    checksum = checksum_mix(checksum, record.sequence);
    checksum = checksum_mix(checksum, u64::from(operation_code(record.operation)));
    checksum = checksum_mix(checksum, record.start_sector);
    checksum = checksum_mix(checksum, u64::from(record.sector_count));
    checksum
}

fn checksum_mix(mut checksum: u32, value: u64) -> u32 {
    let mut remaining = value;
    let mut byte_index = 0;
    while byte_index < 8 {
        checksum ^= (remaining as u8) as u32;
        checksum = checksum.wrapping_mul(0x0100_0193);
        remaining >>= 8;
        byte_index += 1;
    }
    checksum
}

fn operation_code(operation: StorageOperation) -> u8 {
    match operation {
        StorageOperation::Read => 1,
        StorageOperation::Write => 2,
    }
}

pub fn run_self_test(device: BlockDeviceInfo) -> Result<(), JournalError> {
    let service = StorageService::new(device);
    let mut identities = ServiceIdentityTable::new();
    let writer = identities.register_task(TaskId::new(72))?;
    let mut table = CapabilityTable::new();
    let handle = table.grant(
        writer,
        STORAGE_RESOURCE_ID,
        RightsMask::new(RightsMask::READ | RightsMask::WRITE),
    )?;
    let mut journal = AppendOnlyJournal::new();
    let record = journal_first_write(
        &mut journal,
        service,
        &table,
        writer,
        handle,
        StorageRequest::write(1, 1),
    )?;
    if journal.len() != 1
        || journal.last() != Some(record)
        || record.sequence() != 1
        || record.start_sector() != 1
        || record.sector_count() != 1
    {
        return Err(JournalError::Full);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE:JOURNAL_APPEND");
    Ok(())
}

pub fn run_commit_marker_self_test(device: BlockDeviceInfo) -> Result<(), JournalError> {
    let service = StorageService::new(device);
    let mut identities = ServiceIdentityTable::new();
    let writer = identities.register_task(TaskId::new(73))?;
    let mut table = CapabilityTable::new();
    let handle = table.grant(
        writer,
        STORAGE_RESOURCE_ID,
        RightsMask::new(RightsMask::READ | RightsMask::WRITE),
    )?;
    let mut journal = AppendOnlyJournal::new();
    journal_first_write(
        &mut journal,
        service,
        &table,
        writer,
        handle,
        StorageRequest::write(2, 1),
    )?;
    let committed = journal.commit_last()?;
    if !committed.is_committed()
        || committed.checksum() == 0
        || committed.sequence() != 1
        || committed.start_sector() != 2
        || committed.sector_count() != 1
    {
        return Err(JournalError::ChecksumMismatch);
    }
    let torn = JournalRecord {
        commit_marker: 0,
        ..committed
    };
    if validate_committed(torn) != Err(JournalError::MissingCommitMarker) {
        return Err(JournalError::MissingCommitMarker);
    }
    let corrupted = JournalRecord {
        checksum: committed.checksum().wrapping_add(1),
        ..committed
    };
    if validate_committed(corrupted) != Err(JournalError::ChecksumMismatch) {
        return Err(JournalError::ChecksumMismatch);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE:CHECKSUM_VALID");
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE:COMMIT_MARKER");
    Ok(())
}

pub fn run_crash_recovery_self_test(device: BlockDeviceInfo) -> Result<(), JournalError> {
    let service = StorageService::new(device);
    let mut identities = ServiceIdentityTable::new();
    let writer = identities.register_task(TaskId::new(74))?;
    let mut table = CapabilityTable::new();
    let handle = table.grant(
        writer,
        STORAGE_RESOURCE_ID,
        RightsMask::new(RightsMask::READ | RightsMask::WRITE),
    )?;
    let mut journal = AppendOnlyJournal::new();
    journal_first_write(
        &mut journal,
        service,
        &table,
        writer,
        handle,
        StorageRequest::write(3, 1),
    )?;
    journal.commit_last()?;
    journal_first_write(
        &mut journal,
        service,
        &table,
        writer,
        handle,
        StorageRequest::write(4, 1),
    )?;
    let report = recover_committed_prefix(journal);
    if report.replayed_records() != 1
        || report.rolled_back_records() != 1
        || report.last_sequence() != 1
    {
        return Err(JournalError::MissingCommitMarker);
    }

    let mut corrupted_journal = AppendOnlyJournal::new();
    journal_first_write(
        &mut corrupted_journal,
        service,
        &table,
        writer,
        handle,
        StorageRequest::write(5, 1),
    )?;
    corrupted_journal.commit_last()?;
    journal_first_write(
        &mut corrupted_journal,
        service,
        &table,
        writer,
        handle,
        StorageRequest::write(6, 1),
    )?;
    corrupted_journal.commit_last()?;
    corrupted_journal.records[1].checksum = corrupted_journal.records[1].checksum.wrapping_add(1);
    let corrupted_report = recover_committed_prefix(corrupted_journal);
    if corrupted_report.replayed_records() != 1 || corrupted_report.rolled_back_records() != 1 {
        return Err(JournalError::ChecksumMismatch);
    }

    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE:RECOVERY_REPLAY");
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE:RECOVERY_ROLLBACK");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authorized_writer() -> (
        StorageService,
        CapabilityTable,
        ServiceId,
        crate::capabilities::CapabilityHandle,
    ) {
        let service = StorageService::new(BlockDeviceInfo::new_for_test(16, 8));
        let mut identities = ServiceIdentityTable::new();
        let writer = identities.register_task(TaskId::new(72)).unwrap();
        let mut table = CapabilityTable::new();
        let handle = table
            .grant(
                writer,
                STORAGE_RESOURCE_ID,
                RightsMask::new(RightsMask::READ | RightsMask::WRITE),
            )
            .unwrap();
        (service, table, writer, handle)
    }

    #[test]
    fn journal_first_write_appends_before_write_completion() {
        let (service, table, writer, handle) = authorized_writer();
        let mut journal = AppendOnlyJournal::new();

        let record = journal_first_write(
            &mut journal,
            service,
            &table,
            writer,
            handle,
            StorageRequest::write(2, 3),
        )
        .unwrap();

        assert_eq!(journal.len(), 1);
        assert_eq!(journal.last(), Some(record));
        assert_eq!(record.sequence(), 1);
        assert_eq!(record.start_sector(), 2);
        assert_eq!(record.sector_count(), 3);
    }

    #[test]
    fn journal_is_append_only_and_monotonic() {
        let (service, table, writer, handle) = authorized_writer();
        let mut journal = AppendOnlyJournal::new();

        let first = journal_first_write(
            &mut journal,
            service,
            &table,
            writer,
            handle,
            StorageRequest::write(0, 1),
        )
        .unwrap();
        let second = journal_first_write(
            &mut journal,
            service,
            &table,
            writer,
            handle,
            StorageRequest::write(1, 1),
        )
        .unwrap();

        assert_eq!(first.sequence(), 1);
        assert_eq!(second.sequence(), 2);
        assert_eq!(journal.len(), 2);
        assert_eq!(journal.last(), Some(second));
    }

    #[test]
    fn read_requests_are_not_journaled_as_writes() {
        let (service, table, writer, handle) = authorized_writer();
        let mut journal = AppendOnlyJournal::new();

        assert_eq!(
            journal_first_write(
                &mut journal,
                service,
                &table,
                writer,
                handle,
                StorageRequest::read(0, 1),
            ),
            Err(JournalError::ReadIsNotJournaled)
        );
        assert_eq!(journal.len(), 0);
    }

    #[test]
    fn full_journal_rejects_new_writes_without_overwriting_old_records() {
        let (service, table, writer, handle) = authorized_writer();
        let mut journal = AppendOnlyJournal::new();

        for sector in 0..JOURNAL_CAPACITY {
            journal_first_write(
                &mut journal,
                service,
                &table,
                writer,
                handle,
                StorageRequest::write(sector as u64, 1),
            )
            .unwrap();
        }
        let last = journal.last();

        assert_eq!(
            journal_first_write(
                &mut journal,
                service,
                &table,
                writer,
                handle,
                StorageRequest::write(JOURNAL_CAPACITY as u64, 1),
            ),
            Err(JournalError::Full)
        );
        assert_eq!(journal.len(), JOURNAL_CAPACITY);
        assert_eq!(journal.last(), last);
    }

    #[test]
    fn committed_records_have_checksum_and_marker() {
        let (service, table, writer, handle) = authorized_writer();
        let mut journal = AppendOnlyJournal::new();

        journal_first_write(
            &mut journal,
            service,
            &table,
            writer,
            handle,
            StorageRequest::write(4, 1),
        )
        .unwrap();
        let committed = journal.commit_last().unwrap();

        assert!(committed.is_committed());
        assert_ne!(committed.checksum(), 0);
        assert_eq!(journal.last(), Some(committed));
        assert_eq!(validate_committed(committed), Ok(()));
    }

    #[test]
    fn missing_commit_marker_is_detected_as_torn_write() {
        let (service, table, writer, handle) = authorized_writer();
        let mut journal = AppendOnlyJournal::new();
        journal_first_write(
            &mut journal,
            service,
            &table,
            writer,
            handle,
            StorageRequest::write(5, 1),
        )
        .unwrap();
        let committed = journal.commit_last().unwrap();
        let torn = JournalRecord {
            commit_marker: 0,
            ..committed
        };

        assert_eq!(
            validate_committed(torn),
            Err(JournalError::MissingCommitMarker)
        );
    }

    #[test]
    fn checksum_mismatch_is_detected() {
        let (service, table, writer, handle) = authorized_writer();
        let mut journal = AppendOnlyJournal::new();
        journal_first_write(
            &mut journal,
            service,
            &table,
            writer,
            handle,
            StorageRequest::write(6, 1),
        )
        .unwrap();
        let committed = journal.commit_last().unwrap();
        let corrupted = JournalRecord {
            start_sector: committed.start_sector() + 1,
            ..committed
        };

        assert_eq!(
            validate_committed(corrupted),
            Err(JournalError::ChecksumMismatch)
        );
    }

    #[test]
    fn recovery_replays_all_committed_records() {
        let (service, table, writer, handle) = authorized_writer();
        let mut journal = AppendOnlyJournal::new();
        for sector in 0..3 {
            journal_first_write(
                &mut journal,
                service,
                &table,
                writer,
                handle,
                StorageRequest::write(sector, 1),
            )
            .unwrap();
            journal.commit_last().unwrap();
        }

        assert_eq!(
            recover_committed_prefix(journal),
            RecoveryReport {
                replayed_records: 3,
                rolled_back_records: 0,
                last_sequence: 3,
            }
        );
    }

    #[test]
    fn recovery_rolls_back_uncommitted_tail_after_interrupted_write() {
        let (service, table, writer, handle) = authorized_writer();
        let mut journal = AppendOnlyJournal::new();
        journal_first_write(
            &mut journal,
            service,
            &table,
            writer,
            handle,
            StorageRequest::write(0, 1),
        )
        .unwrap();
        journal.commit_last().unwrap();
        journal_first_write(
            &mut journal,
            service,
            &table,
            writer,
            handle,
            StorageRequest::write(1, 1),
        )
        .unwrap();

        assert_eq!(
            recover_committed_prefix(journal),
            RecoveryReport {
                replayed_records: 1,
                rolled_back_records: 1,
                last_sequence: 1,
            }
        );
    }

    #[test]
    fn recovery_rolls_back_from_first_checksum_mismatch() {
        let (service, table, writer, handle) = authorized_writer();
        let mut journal = AppendOnlyJournal::new();
        for sector in 0..3 {
            journal_first_write(
                &mut journal,
                service,
                &table,
                writer,
                handle,
                StorageRequest::write(sector, 1),
            )
            .unwrap();
            journal.commit_last().unwrap();
        }
        journal.records[1].checksum = journal.records[1].checksum.wrapping_add(1);

        assert_eq!(
            recover_committed_prefix(journal),
            RecoveryReport {
                replayed_records: 1,
                rolled_back_records: 2,
                last_sequence: 1,
            }
        );
    }
}
