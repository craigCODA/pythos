//! Kernel-assigned service identities for Phase 3.
#![cfg_attr(test, allow(dead_code))]

use crate::tasks::{MAX_TASKS, TaskId};

const MAX_SERVICES: usize = MAX_TASKS;
const FIRST_SERVICE_ID: u64 = 1;
const INVALID_SERVICE_ID: u64 = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceId(u64);

impl ServiceId {
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceIdentityError {
    TableFull,
    UnknownService,
    ExhaustedIds,
    StaleIdentityStillActive,
    ReusedIdentity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ServiceRecord {
    service_id: ServiceId,
    task_id: TaskId,
    active: bool,
}

impl ServiceRecord {
    const fn empty() -> Self {
        Self {
            service_id: ServiceId(INVALID_SERVICE_ID),
            task_id: TaskId::new(0),
            active: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceIdentityTable {
    records: [ServiceRecord; MAX_SERVICES],
    next_id: u64,
}

impl ServiceIdentityTable {
    pub const fn new() -> Self {
        Self {
            records: [ServiceRecord::empty(); MAX_SERVICES],
            next_id: FIRST_SERVICE_ID,
        }
    }

    pub fn register_task(&mut self, task_id: TaskId) -> Result<ServiceId, ServiceIdentityError> {
        let slot = self
            .records
            .iter()
            .position(|record| !record.active)
            .ok_or(ServiceIdentityError::TableFull)?;
        let service_id = ServiceId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(ServiceIdentityError::ExhaustedIds)?;
        self.records[slot] = ServiceRecord {
            service_id,
            task_id,
            active: true,
        };
        Ok(service_id)
    }

    pub fn unregister(&mut self, service_id: ServiceId) -> Result<(), ServiceIdentityError> {
        let record = self
            .records
            .iter_mut()
            .find(|record| record.active && record.service_id == service_id)
            .ok_or(ServiceIdentityError::UnknownService)?;
        record.active = false;
        Ok(())
    }

    pub fn task_for(&self, service_id: ServiceId) -> Option<TaskId> {
        self.records
            .iter()
            .find(|record| record.active && record.service_id == service_id)
            .map(|record| record.task_id)
    }
}

pub fn run_self_test() -> Result<(), ServiceIdentityError> {
    let mut table = ServiceIdentityTable::new();
    let old_service = table.register_task(TaskId::new(10))?;
    table.unregister(old_service)?;
    let new_service = table.register_task(TaskId::new(11))?;

    if old_service == new_service {
        return Err(ServiceIdentityError::ReusedIdentity);
    }
    if table.task_for(old_service).is_some() {
        return Err(ServiceIdentityError::StaleIdentityStillActive);
    }
    if table.task_for(new_service) != Some(TaskId::new(11)) {
        return Err(ServiceIdentityError::UnknownService);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_identity_is_distinct_from_task_id() {
        let mut table = ServiceIdentityTable::new();

        let service = table.register_task(TaskId::new(42)).unwrap();

        assert_ne!(service.raw(), 42);
        assert_eq!(table.task_for(service), Some(TaskId::new(42)));
    }

    #[test]
    fn slot_reuse_gets_fresh_service_identity() {
        let mut table = ServiceIdentityTable::new();

        let old_service = table.register_task(TaskId::new(10)).unwrap();
        assert_eq!(table.unregister(old_service), Ok(()));
        let new_service = table.register_task(TaskId::new(11)).unwrap();

        assert_ne!(old_service, new_service);
        assert_eq!(table.task_for(old_service), None);
        assert_eq!(table.task_for(new_service), Some(TaskId::new(11)));
    }
}
