//! Phase 10 serialized storage write-gate proof.
#![cfg_attr(test, allow(dead_code))]

use crate::{
    capabilities::{CapabilityError, CapabilityHandle, CapabilityTable, RightsMask},
    service_identity::{ServiceId, ServiceIdentityError, ServiceIdentityTable},
    storage_service::STORAGE_RESOURCE_ID,
    tasks::TaskId,
};

#[cfg(not(test))]
use crate::serial;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageConcurrencyError {
    Capability(CapabilityError),
    Identity(ServiceIdentityError),
    Busy,
    StaleToken,
    CounterOverflow,
}

impl From<CapabilityError> for StorageConcurrencyError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

impl From<ServiceIdentityError> for StorageConcurrencyError {
    fn from(error: ServiceIdentityError) -> Self {
        Self::Identity(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriteToken {
    owner: ServiceId,
    generation: u64,
}

impl WriteToken {
    pub const fn owner(self) -> ServiceId {
        self.owner
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageWriteGate {
    owner: Option<ServiceId>,
    generation: u64,
}

impl StorageWriteGate {
    pub const fn new() -> Self {
        Self {
            owner: None,
            generation: 1,
        }
    }

    pub fn begin_write(
        &mut self,
        table: &CapabilityTable,
        caller: ServiceId,
        handle: CapabilityHandle,
    ) -> Result<WriteToken, StorageConcurrencyError> {
        table.validate(
            caller,
            handle,
            STORAGE_RESOURCE_ID,
            RightsMask::new(RightsMask::WRITE),
        )?;
        if self.owner.is_some() {
            return Err(StorageConcurrencyError::Busy);
        }
        let token = WriteToken {
            owner: caller,
            generation: self.generation,
        };
        self.owner = Some(caller);
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(StorageConcurrencyError::CounterOverflow)?;
        Ok(token)
    }

    pub fn commit_write(&mut self, token: WriteToken) -> Result<(), StorageConcurrencyError> {
        if self.owner != Some(token.owner) || self.generation != token.generation + 1 {
            return Err(StorageConcurrencyError::StaleToken);
        }
        self.owner = None;
        Ok(())
    }

    pub const fn owner(self) -> Option<ServiceId> {
        self.owner
    }
}

pub fn run_self_test() -> Result<(), StorageConcurrencyError> {
    let mut identities = ServiceIdentityTable::new();
    let writer_a = identities.register_task(TaskId::new(94))?;
    let writer_b = identities.register_task(TaskId::new(95))?;
    let mut table = CapabilityTable::new();
    let handle_a = table.grant(
        writer_a,
        STORAGE_RESOURCE_ID,
        RightsMask::new(RightsMask::READ | RightsMask::WRITE),
    )?;
    let handle_b = table.grant(
        writer_b,
        STORAGE_RESOURCE_ID,
        RightsMask::new(RightsMask::READ | RightsMask::WRITE),
    )?;

    let mut gate = StorageWriteGate::new();
    let token_a = gate.begin_write(&table, writer_a, handle_a)?;
    if gate.begin_write(&table, writer_b, handle_b) != Err(StorageConcurrencyError::Busy) {
        return Err(StorageConcurrencyError::Busy);
    }
    gate.commit_write(token_a)?;
    let token_b = gate.begin_write(&table, writer_b, handle_b)?;
    if token_b.owner() != writer_b {
        return Err(StorageConcurrencyError::StaleToken);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:CONCURRENT_WRITE:SERIALIZED");

    if gate.commit_write(token_a) != Err(StorageConcurrencyError::StaleToken)
        || gate.owner() != Some(writer_b)
    {
        return Err(StorageConcurrencyError::StaleToken);
    }
    gate.commit_write(token_b)?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:CONCURRENT_WRITE:CORRUPTION_DENIED");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writer_pair() -> (
        ServiceId,
        ServiceId,
        CapabilityTable,
        CapabilityHandle,
        CapabilityHandle,
    ) {
        let mut identities = ServiceIdentityTable::new();
        let writer_a = identities.register_task(TaskId::new(94)).unwrap();
        let writer_b = identities.register_task(TaskId::new(95)).unwrap();
        let mut table = CapabilityTable::new();
        let handle_a = table
            .grant(
                writer_a,
                STORAGE_RESOURCE_ID,
                RightsMask::new(RightsMask::READ | RightsMask::WRITE),
            )
            .unwrap();
        let handle_b = table
            .grant(
                writer_b,
                STORAGE_RESOURCE_ID,
                RightsMask::new(RightsMask::READ | RightsMask::WRITE),
            )
            .unwrap();
        (writer_a, writer_b, table, handle_a, handle_b)
    }

    #[test]
    fn concurrent_writer_is_denied_until_current_token_commits() {
        let (writer_a, writer_b, table, handle_a, handle_b) = writer_pair();
        let mut gate = StorageWriteGate::new();

        let token_a = gate.begin_write(&table, writer_a, handle_a).unwrap();

        assert_eq!(
            gate.begin_write(&table, writer_b, handle_b),
            Err(StorageConcurrencyError::Busy)
        );
        assert_eq!(gate.commit_write(token_a), Ok(()));
        assert!(gate.begin_write(&table, writer_b, handle_b).is_ok());
    }

    #[test]
    fn stale_token_cannot_release_active_writer() {
        let (writer_a, writer_b, table, handle_a, handle_b) = writer_pair();
        let mut gate = StorageWriteGate::new();

        let token_a = gate.begin_write(&table, writer_a, handle_a).unwrap();
        gate.commit_write(token_a).unwrap();
        let token_b = gate.begin_write(&table, writer_b, handle_b).unwrap();

        assert_eq!(
            gate.commit_write(token_a),
            Err(StorageConcurrencyError::StaleToken)
        );
        assert_eq!(gate.owner(), Some(writer_b));
        assert_eq!(gate.commit_write(token_b), Ok(()));
    }

    #[test]
    fn write_gate_requires_storage_write_capability() {
        let mut identities = ServiceIdentityTable::new();
        let writer = identities.register_task(TaskId::new(96)).unwrap();
        let mut table = CapabilityTable::new();
        let read_only = table
            .grant(
                writer,
                STORAGE_RESOURCE_ID,
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();
        let mut gate = StorageWriteGate::new();

        assert_eq!(
            gate.begin_write(&table, writer, read_only),
            Err(StorageConcurrencyError::Capability(
                CapabilityError::MissingRights
            ))
        );
    }
}
