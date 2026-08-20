//! Phase 10 service-scoped storage quota proof.
#![cfg_attr(test, allow(dead_code))]

use crate::{
    service_identity::{ServiceId, ServiceIdentityError, ServiceIdentityTable},
    tasks::TaskId,
};

#[cfg(not(test))]
use crate::serial;

const MAX_STORAGE_QUOTAS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageQuotaError {
    TableFull,
    DuplicateService,
    UnknownService,
    StorageQuotaExceeded,
    CounterOverflow,
    Identity(ServiceIdentityError),
}

impl From<ServiceIdentityError> for StorageQuotaError {
    fn from(error: ServiceIdentityError) -> Self {
        Self::Identity(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StorageQuotaRecord {
    service_id: ServiceId,
    max_blocks: u64,
    used_blocks: u64,
    active: bool,
}

impl StorageQuotaRecord {
    const fn empty() -> Self {
        Self {
            service_id: ServiceId::invalid(),
            max_blocks: 0,
            used_blocks: 0,
            active: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageQuotaTable {
    records: [StorageQuotaRecord; MAX_STORAGE_QUOTAS],
}

impl StorageQuotaTable {
    pub const fn new() -> Self {
        Self {
            records: [StorageQuotaRecord::empty(); MAX_STORAGE_QUOTAS],
        }
    }

    pub fn register(
        &mut self,
        service_id: ServiceId,
        max_blocks: u64,
    ) -> Result<(), StorageQuotaError> {
        if self
            .records
            .iter()
            .any(|record| record.active && record.service_id == service_id)
        {
            return Err(StorageQuotaError::DuplicateService);
        }
        let record = self
            .records
            .iter_mut()
            .find(|record| !record.active)
            .ok_or(StorageQuotaError::TableFull)?;
        *record = StorageQuotaRecord {
            service_id,
            max_blocks,
            used_blocks: 0,
            active: true,
        };
        Ok(())
    }

    pub fn charge_blocks(
        &mut self,
        service_id: ServiceId,
        blocks: u64,
    ) -> Result<(), StorageQuotaError> {
        let record = self
            .records
            .iter_mut()
            .find(|record| record.active && record.service_id == service_id)
            .ok_or(StorageQuotaError::UnknownService)?;
        let next_used = record
            .used_blocks
            .checked_add(blocks)
            .ok_or(StorageQuotaError::CounterOverflow)?;
        if next_used > record.max_blocks {
            return Err(StorageQuotaError::StorageQuotaExceeded);
        }
        record.used_blocks = next_used;
        Ok(())
    }

    #[cfg_attr(all(not(test), feature = "verify"), allow(dead_code))]
    pub fn ensure_minimum_limit(
        &mut self,
        service_id: ServiceId,
        min_blocks: u64,
    ) -> Result<(), StorageQuotaError> {
        let record = self
            .records
            .iter_mut()
            .find(|record| record.active && record.service_id == service_id)
            .ok_or(StorageQuotaError::UnknownService)?;
        if record.max_blocks < min_blocks {
            record.max_blocks = min_blocks;
        }
        Ok(())
    }

    pub fn release_blocks(
        &mut self,
        service_id: ServiceId,
        blocks: u64,
    ) -> Result<(), StorageQuotaError> {
        let record = self
            .records
            .iter_mut()
            .find(|record| record.active && record.service_id == service_id)
            .ok_or(StorageQuotaError::UnknownService)?;
        record.used_blocks = record
            .used_blocks
            .checked_sub(blocks)
            .ok_or(StorageQuotaError::CounterOverflow)?;
        Ok(())
    }

    pub fn used_blocks(&self, service_id: ServiceId) -> Result<u64, StorageQuotaError> {
        self.records
            .iter()
            .find(|record| record.active && record.service_id == service_id)
            .map(|record| record.used_blocks)
            .ok_or(StorageQuotaError::UnknownService)
    }
}

pub fn run_self_test() -> Result<(), StorageQuotaError> {
    let mut identities = ServiceIdentityTable::new();
    let service_id = identities.register_task(TaskId::new(91))?;

    let mut quotas = StorageQuotaTable::new();
    quotas.register(service_id, 2)?;
    quotas.charge_blocks(service_id, 1)?;
    let used_after_grant = quotas.used_blocks(service_id)?;
    if used_after_grant != 1 {
        return Err(StorageQuotaError::UnknownService);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE_QUOTA:GRANTED");

    let denied =
        quotas.charge_blocks(service_id, 2) == Err(StorageQuotaError::StorageQuotaExceeded);
    if !denied || quotas.used_blocks(service_id)? != used_after_grant {
        return Err(StorageQuotaError::StorageQuotaExceeded);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE_QUOTA:DENIED");

    quotas.release_blocks(service_id, 1)?;
    if quotas.used_blocks(service_id)? != 0 {
        return Err(StorageQuotaError::CounterOverflow);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_quota_denies_over_limit_without_mutating_usage() {
        let mut identities = ServiceIdentityTable::new();
        let service_id = identities.register_task(TaskId::new(91)).unwrap();
        let mut quotas = StorageQuotaTable::new();

        quotas.register(service_id, 2).unwrap();
        quotas.charge_blocks(service_id, 1).unwrap();

        assert_eq!(
            quotas.charge_blocks(service_id, 2),
            Err(StorageQuotaError::StorageQuotaExceeded)
        );
        assert_eq!(quotas.used_blocks(service_id), Ok(1));
    }

    #[test]
    fn storage_quota_requires_registered_service() {
        let mut identities = ServiceIdentityTable::new();
        let unknown_service = identities.register_task(TaskId::new(92)).unwrap();
        let quotas = StorageQuotaTable::new();

        assert_eq!(
            quotas.used_blocks(unknown_service),
            Err(StorageQuotaError::UnknownService)
        );
    }

    #[test]
    fn storage_quota_release_reclaims_usage() {
        let mut identities = ServiceIdentityTable::new();
        let service_id = identities.register_task(TaskId::new(93)).unwrap();
        let mut quotas = StorageQuotaTable::new();

        quotas.register(service_id, 3).unwrap();
        quotas.charge_blocks(service_id, 2).unwrap();
        quotas.release_blocks(service_id, 1).unwrap();

        assert_eq!(quotas.used_blocks(service_id), Ok(1));
    }

    #[test]
    fn storage_quota_limit_can_be_raised_without_mutating_usage() {
        let mut identities = ServiceIdentityTable::new();
        let service_id = identities.register_task(TaskId::new(94)).unwrap();
        let mut quotas = StorageQuotaTable::new();

        quotas.register(service_id, 1).unwrap();
        quotas.charge_blocks(service_id, 1).unwrap();
        quotas.ensure_minimum_limit(service_id, 3).unwrap();

        assert_eq!(quotas.used_blocks(service_id), Ok(1));
        assert_eq!(quotas.charge_blocks(service_id, 2), Ok(()));
    }
}
