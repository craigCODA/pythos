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
const EMPTY_RECORD: JournalRecord = JournalRecord {
    sequence: 0,
    operation: StorageOperation::Read,
    start_sector: 0,
    sector_count: 0,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalError {
    Full,
    ReadIsNotJournaled,
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
        };
        self.records[self.len] = record;
        self.len += 1;
        self.next_sequence = self.next_sequence.wrapping_add(1);
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
}
