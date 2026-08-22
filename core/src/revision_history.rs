//! Fixed revision history for ADR 0022 typed object records.
#![cfg_attr(test, allow(dead_code))]
// Some items are only used by the normal-boot object-service path (a
// `verify`-excluded set of modules), so they are legitimately unused under
// `--features verify`.
#![cfg_attr(feature = "verify", allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityError, ServiceIdentityTable};
use crate::shell_objects::{ObjectId, ObjectKind};
use crate::tasks::TaskId;
use crate::typed_object_format::{ObjectFormatError, TypedObjectField, TypedObjectRecord};
use pythos_shared::object_shell_abi::MAX_QUERY_RESULTS;

pub const MAX_CURRENT_OBJECTS: usize = 4;
pub const MAX_REVISIONS: usize = MAX_QUERY_RESULTS;
pub const OBJECT_SERVICE_CURRENT_OBJECTS: usize = crate::dynamic_object_store::MAX_DYNAMIC_OBJECTS;

pub type RevisionHistory = BoundedRevisionHistory<MAX_CURRENT_OBJECTS, MAX_REVISIONS>;
pub type ObjectServiceRevisionHistory =
    BoundedRevisionHistory<OBJECT_SERVICE_CURRENT_OBJECTS, MAX_REVISIONS>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevisionHistoryError {
    ObjectTableFull,
    RevisionTableFull,
    DuplicateObject,
    UnknownObject,
    NonMonotonicTimestamp,
    ObjectFormat(ObjectFormatError),
    Identity(ServiceIdentityError),
}

impl From<ObjectFormatError> for RevisionHistoryError {
    fn from(error: ObjectFormatError) -> Self {
        Self::ObjectFormat(error)
    }
}

impl From<ServiceIdentityError> for RevisionHistoryError {
    fn from(error: ServiceIdentityError) -> Self {
        Self::Identity(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RevisionRecord {
    object_id: ObjectId,
    revision: u64,
    timestamp_ticks: u64,
    writer: ServiceId,
    object: TypedObjectRecord,
}

impl RevisionRecord {
    pub const fn new(
        object_id: ObjectId,
        revision: u64,
        timestamp_ticks: u64,
        writer: ServiceId,
        object: TypedObjectRecord,
    ) -> Self {
        Self {
            object_id,
            revision,
            timestamp_ticks,
            writer,
            object,
        }
    }

    pub const fn object_id(self) -> ObjectId {
        self.object_id
    }

    pub const fn revision(self) -> u64 {
        self.revision
    }

    pub const fn timestamp_ticks(self) -> u64 {
        self.timestamp_ticks
    }

    pub const fn writer(self) -> ServiceId {
        self.writer
    }

    pub const fn object(self) -> TypedObjectRecord {
        self.object
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CurrentObject {
    revision: u64,
    timestamp_ticks: u64,
    writer: ServiceId,
    object: TypedObjectRecord,
}

impl CurrentObject {
    fn as_revision(self) -> RevisionRecord {
        RevisionRecord {
            object_id: self.object.object_id(),
            revision: self.revision,
            timestamp_ticks: self.timestamp_ticks,
            writer: self.writer,
            object: self.object,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundedRevisionHistory<const CURRENT_CAPACITY: usize, const REVISION_CAPACITY: usize> {
    current: [Option<CurrentObject>; CURRENT_CAPACITY],
    revisions: [Option<RevisionRecord>; REVISION_CAPACITY],
}

impl<const CURRENT_CAPACITY: usize, const REVISION_CAPACITY: usize>
    BoundedRevisionHistory<CURRENT_CAPACITY, REVISION_CAPACITY>
{
    pub const fn new() -> Self {
        Self {
            current: [None; CURRENT_CAPACITY],
            revisions: [None; REVISION_CAPACITY],
        }
    }

    pub fn create_object(
        &mut self,
        object: TypedObjectRecord,
        timestamp_ticks: u64,
        writer: ServiceId,
    ) -> Result<(), RevisionHistoryError> {
        if self.current_index(object.object_id()).is_some() {
            return Err(RevisionHistoryError::DuplicateObject);
        }
        let mut index = 0;
        while index < CURRENT_CAPACITY {
            if self.current[index].is_none() {
                self.current[index] = Some(CurrentObject {
                    revision: 1,
                    timestamp_ticks,
                    writer,
                    object,
                });
                return Ok(());
            }
            index += 1;
        }
        Err(RevisionHistoryError::ObjectTableFull)
    }

    pub fn update_object(
        &mut self,
        object: TypedObjectRecord,
        timestamp_ticks: u64,
        writer: ServiceId,
    ) -> Result<u64, RevisionHistoryError> {
        let index = self
            .current_index(object.object_id())
            .ok_or(RevisionHistoryError::UnknownObject)?;
        let current = self.current[index].ok_or(RevisionHistoryError::UnknownObject)?;
        if timestamp_ticks <= current.timestamp_ticks {
            return Err(RevisionHistoryError::NonMonotonicTimestamp);
        }
        self.push_revision(current.as_revision())?;
        let next_revision = current.revision + 1;
        self.current[index] = Some(CurrentObject {
            revision: next_revision,
            timestamp_ticks,
            writer,
            object,
        });
        Ok(next_revision)
    }

    pub fn current_revision(self, object_id: ObjectId) -> Option<RevisionRecord> {
        self.current_index(object_id)
            .and_then(|index| self.current[index].map(CurrentObject::as_revision))
    }

    pub fn prior_revision(self, object_id: ObjectId, revision: u64) -> Option<RevisionRecord> {
        let mut index = 0;
        while index < REVISION_CAPACITY {
            if let Some(record) = self.revisions[index]
                && record.object_id() == object_id
                && record.revision() == revision
            {
                return Some(record);
            }
            index += 1;
        }
        None
    }

    pub fn revision_count(self, object_id: ObjectId) -> usize {
        let mut count = 0;
        let mut index = 0;
        while index < REVISION_CAPACITY {
            if let Some(record) = self.revisions[index]
                && record.object_id() == object_id
            {
                count += 1;
            }
            index += 1;
        }
        count
    }

    pub fn current_records(self) -> [Option<RevisionRecord>; CURRENT_CAPACITY] {
        let mut records = [None; CURRENT_CAPACITY];
        let mut index = 0;
        while index < CURRENT_CAPACITY {
            records[index] = self.current[index].map(CurrentObject::as_revision);
            index += 1;
        }
        records
    }

    pub fn prior_records(self) -> [Option<RevisionRecord>; REVISION_CAPACITY] {
        self.revisions
    }

    pub fn current_record_at(&self, index: usize) -> Option<RevisionRecord> {
        if index >= CURRENT_CAPACITY {
            return None;
        }
        self.current[index].map(CurrentObject::as_revision)
    }

    pub fn prior_record_at(&self, index: usize) -> Option<RevisionRecord> {
        if index >= REVISION_CAPACITY {
            return None;
        }
        self.revisions[index]
    }

    pub fn restore_from_records(
        current: [Option<RevisionRecord>; CURRENT_CAPACITY],
        revisions: [Option<RevisionRecord>; REVISION_CAPACITY],
    ) -> Result<Self, RevisionHistoryError> {
        let mut restored = Self::new();
        let mut index = 0;
        while index < CURRENT_CAPACITY {
            if let Some(record) = current[index] {
                if record.object_id() != record.object().object_id() {
                    return Err(RevisionHistoryError::UnknownObject);
                }
                if restored.current_index(record.object_id()).is_some() {
                    return Err(RevisionHistoryError::DuplicateObject);
                }
                restored.current[index] = Some(CurrentObject {
                    revision: record.revision(),
                    timestamp_ticks: record.timestamp_ticks(),
                    writer: record.writer(),
                    object: record.object(),
                });
            }
            index += 1;
        }
        let mut revision_index = 0;
        while revision_index < REVISION_CAPACITY {
            if let Some(record) = revisions[revision_index]
                && restored.current_index(record.object_id()).is_none()
            {
                return Err(RevisionHistoryError::UnknownObject);
            }
            revision_index += 1;
        }
        restored.revisions = revisions;
        Ok(restored)
    }

    fn push_revision(&mut self, record: RevisionRecord) -> Result<(), RevisionHistoryError> {
        let mut index = 0;
        while index < REVISION_CAPACITY {
            if self.revisions[index].is_none() {
                self.revisions[index] = Some(record);
                return Ok(());
            }
            index += 1;
        }
        Err(RevisionHistoryError::RevisionTableFull)
    }

    fn current_index(self, object_id: ObjectId) -> Option<usize> {
        let mut index = 0;
        while index < CURRENT_CAPACITY {
            if let Some(current) = self.current[index]
                && current.object.object_id() == object_id
            {
                return Some(index);
            }
            index += 1;
        }
        None
    }
}

pub fn run_self_test() -> Result<(), RevisionHistoryError> {
    let mut identities = ServiceIdentityTable::new();
    let service_monitor = identities.register_task(TaskId::new(80))?;
    let python_console = identities.register_task(TaskId::new(81))?;
    let mut history = RevisionHistory::new();
    let object_id = ObjectId::new(0x7301);

    history.create_object(
        revision_record(object_id, 1, b"ready")?,
        100,
        service_monitor,
    )?;
    history.update_object(
        revision_record(object_id, 2, b"blocked")?,
        120,
        python_console,
    )?;
    history.update_object(
        revision_record(object_id, 3, b"ready2")?,
        140,
        service_monitor,
    )?;

    let current = history
        .current_revision(object_id)
        .ok_or(RevisionHistoryError::UnknownObject)?;
    if current.revision() != 3
        || current.object().schema_version() != 3
        || history.revision_count(object_id) != 2
    {
        return Err(RevisionHistoryError::UnknownObject);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT:REVISION_RETAINED");

    let first = history
        .prior_revision(object_id, 1)
        .ok_or(RevisionHistoryError::UnknownObject)?;
    let second = history
        .prior_revision(object_id, 2)
        .ok_or(RevisionHistoryError::UnknownObject)?;
    if first.timestamp_ticks() != 100
        || first.writer() != service_monitor
        || second.timestamp_ticks() != 120
        || second.writer() != python_console
        || current.timestamp_ticks() != 140
        || current.writer() != service_monitor
    {
        return Err(RevisionHistoryError::UnknownObject);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT:REVISION_PROVENANCE");
    Ok(())
}

fn revision_record(
    object_id: ObjectId,
    schema_version: u16,
    status: &[u8],
) -> Result<TypedObjectRecord, ObjectFormatError> {
    let mut record =
        TypedObjectRecord::new(object_id, ObjectKind::ServiceMonitorWindow, schema_version);
    record.push_field(TypedObjectField::new(1, schema_version, status)?)?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writer(task: u64) -> ServiceId {
        let mut identities = ServiceIdentityTable::new();
        identities.register_task(TaskId::new(task)).unwrap()
    }

    fn object(schema_version: u16, value: &[u8]) -> TypedObjectRecord {
        revision_record(ObjectId::new(42), schema_version, value).unwrap()
    }

    #[test]
    fn updates_retain_prior_versions_instead_of_overwriting() {
        let first_writer = writer(1);
        let second_writer = writer(2);
        let mut history = RevisionHistory::new();
        history
            .create_object(object(1, b"one"), 10, first_writer)
            .unwrap();

        assert_eq!(
            history.update_object(object(2, b"two"), 20, second_writer),
            Ok(2)
        );

        let prior = history.prior_revision(ObjectId::new(42), 1).unwrap();
        let current = history.current_revision(ObjectId::new(42)).unwrap();
        assert_eq!(prior.object().schema_version(), 1);
        assert_eq!(current.object().schema_version(), 2);
        assert_eq!(history.revision_count(ObjectId::new(42)), 1);
    }

    #[test]
    fn revisions_keep_timestamp_and_writer_identity() {
        let first_writer = writer(3);
        let second_writer = writer(4);
        let mut history = RevisionHistory::new();
        history
            .create_object(object(1, b"one"), 30, first_writer)
            .unwrap();
        history
            .update_object(object(2, b"two"), 40, second_writer)
            .unwrap();

        let prior = history.prior_revision(ObjectId::new(42), 1).unwrap();
        let current = history.current_revision(ObjectId::new(42)).unwrap();
        assert_eq!(prior.timestamp_ticks(), 30);
        assert_eq!(prior.writer(), first_writer);
        assert_eq!(current.timestamp_ticks(), 40);
        assert_eq!(current.writer(), second_writer);
    }

    #[test]
    fn stale_timestamp_and_unknown_object_are_rejected() {
        let writer = writer(5);
        let mut history = RevisionHistory::new();
        assert_eq!(
            history.update_object(object(2, b"two"), 40, writer),
            Err(RevisionHistoryError::UnknownObject)
        );
        history
            .create_object(object(1, b"one"), 50, writer)
            .unwrap();
        assert_eq!(
            history.update_object(object(2, b"two"), 50, writer),
            Err(RevisionHistoryError::NonMonotonicTimestamp)
        );
    }

    #[test]
    fn current_capacity_keeps_eight_shell_notes_plus_external_denial_fixture() {
        let writer = writer(6);
        let mut history = ObjectServiceRevisionHistory::new();
        history
            .create_object(
                TypedObjectRecord::new(ObjectId::new(2001), ObjectKind::Note, 1),
                1,
                writer,
            )
            .unwrap();

        for index in 0..pythos_shared::object_shell_abi::MAX_QUERY_RESULTS {
            history
                .create_object(
                    TypedObjectRecord::new(ObjectId::new(1042 + index as u64), ObjectKind::Note, 1),
                    2 + index as u64,
                    writer,
                )
                .unwrap();
        }

        assert!(history.current_revision(ObjectId::new(2001)).is_some());
        assert!(
            history
                .current_revision(ObjectId::new(
                    1042 + pythos_shared::object_shell_abi::MAX_QUERY_RESULTS as u64 - 1
                ))
                .is_some()
        );
    }
}
