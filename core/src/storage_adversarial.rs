//! Phase 10 general-purpose storage adversarial proofs.
#![cfg_attr(test, allow(dead_code))]

use crate::{
    dynamic_object_store::{DynamicObjectError, DynamicObjectStore},
    service_identity::{ServiceIdentityError, ServiceIdentityTable},
    shell_objects::{ObjectId, ObjectKind},
    storage_allocator::{
        AllocatorError, AllocatorJournal, BlockAllocator, recover_allocator_journal,
    },
    storage_quotas::{StorageQuotaError, StorageQuotaTable},
    tasks::TaskId,
    typed_object_format::TypedObjectRecord,
};

#[cfg(not(test))]
use crate::serial;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageAdversarialError {
    DynamicObject(DynamicObjectError),
    StorageQuota(StorageQuotaError),
    Allocator(AllocatorError),
    Identity(ServiceIdentityError),
    UnexpectedState,
}

impl From<DynamicObjectError> for StorageAdversarialError {
    fn from(error: DynamicObjectError) -> Self {
        Self::DynamicObject(error)
    }
}

impl From<StorageQuotaError> for StorageAdversarialError {
    fn from(error: StorageQuotaError) -> Self {
        Self::StorageQuota(error)
    }
}

impl From<AllocatorError> for StorageAdversarialError {
    fn from(error: AllocatorError) -> Self {
        Self::Allocator(error)
    }
}

impl From<ServiceIdentityError> for StorageAdversarialError {
    fn from(error: ServiceIdentityError) -> Self {
        Self::Identity(error)
    }
}

pub fn run_self_test() -> Result<(), StorageAdversarialError> {
    prove_create_delete_cycles()?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE_ADVERSARIAL:CREATE_DELETE_CYCLE");

    prove_out_of_quota_denial()?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE_ADVERSARIAL:OUT_OF_QUOTA_DENIED");

    prove_dynamic_torn_write_recovery()?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE_ADVERSARIAL:DYNAMIC_TORN_WRITE_RECOVERED");

    Ok(())
}

fn prove_create_delete_cycles() -> Result<(), StorageAdversarialError> {
    let mut store = DynamicObjectStore::new(144, 12)?;
    let mut round = 0;
    while round < 4 {
        let first_id = ObjectId::new(0x8200 + round);
        let second_id = ObjectId::new(0x8300 + round);
        let first_extent =
            store.create_object(record(first_id, ObjectKind::ServiceMonitorWindow))?;
        store.create_object(record(second_id, ObjectKind::PythonConsoleWindow))?;
        store.delete_object(first_id)?;
        let replacement_extent =
            store.create_object(record(first_id, ObjectKind::SettingsPanelWindow))?;
        if replacement_extent.start_block() != first_extent.start_block()
            || store.object_count() != ((round + 1) as usize * 2)
        {
            return Err(StorageAdversarialError::UnexpectedState);
        }
        round += 1;
    }
    if store.allocated_block_count() != 8 {
        return Err(StorageAdversarialError::UnexpectedState);
    }
    Ok(())
}

fn prove_out_of_quota_denial() -> Result<(), StorageAdversarialError> {
    let mut identities = ServiceIdentityTable::new();
    let writer = identities.register_task(TaskId::new(97))?;
    let mut quotas = StorageQuotaTable::new();
    quotas.register(writer, 2)?;
    quotas.charge_blocks(writer, 2)?;
    if quotas.charge_blocks(writer, 1) != Err(StorageQuotaError::StorageQuotaExceeded)
        || quotas.used_blocks(writer)? != 2
    {
        return Err(StorageAdversarialError::UnexpectedState);
    }
    Ok(())
}

fn prove_dynamic_torn_write_recovery() -> Result<(), StorageAdversarialError> {
    let mut allocator = BlockAllocator::new(160, 8)?;
    let mut journal = AllocatorJournal::new();
    allocator.allocate_journaled(2, &mut journal)?;
    allocator.commit_last(&mut journal)?;
    allocator.allocate_journaled(3, &mut journal)?;

    let recovery = recover_allocator_journal(journal)?;
    if recovery.replayed_records() != 1
        || recovery.rolled_back_records() != 1
        || recovery.committed_bitmap() != 0b11
    {
        return Err(StorageAdversarialError::UnexpectedState);
    }
    Ok(())
}

fn record(object_id: ObjectId, kind: ObjectKind) -> TypedObjectRecord {
    TypedObjectRecord::new(object_id, kind, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_create_delete_cycles_do_not_leak_freed_blocks() {
        assert_eq!(prove_create_delete_cycles(), Ok(()));
    }

    #[test]
    fn out_of_quota_write_is_denied_without_truncation() {
        assert_eq!(prove_out_of_quota_denial(), Ok(()));
    }

    #[test]
    fn dynamic_allocator_torn_write_recovers_committed_prefix() {
        assert_eq!(prove_dynamic_torn_write_recovery(), Ok(()));
    }
}
