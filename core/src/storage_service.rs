//! Capability-gated Phase 7 storage service facade.
#![cfg_attr(test, allow(dead_code))]

use crate::block_device::BlockDeviceInfo;
use crate::capabilities::{
    CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask,
};
#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityError, ServiceIdentityTable};
use crate::tasks::TaskId;

pub const STORAGE_RESOURCE_ID: ResourceId = ResourceId::new(0x570A_0001);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageServiceError {
    Capability(CapabilityError),
    Identity(ServiceIdentityError),
    InvalidRange,
}

impl From<CapabilityError> for StorageServiceError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

impl From<ServiceIdentityError> for StorageServiceError {
    fn from(error: ServiceIdentityError) -> Self {
        Self::Identity(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageOperation {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageRequest {
    operation: StorageOperation,
    start_sector: u64,
    sector_count: u16,
}

impl StorageRequest {
    pub const fn read(start_sector: u64, sector_count: u16) -> Self {
        Self {
            operation: StorageOperation::Read,
            start_sector,
            sector_count,
        }
    }

    pub const fn write(start_sector: u64, sector_count: u16) -> Self {
        Self {
            operation: StorageOperation::Write,
            start_sector,
            sector_count,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageAccess {
    operation: StorageOperation,
    start_sector: u64,
    sector_count: u16,
}

impl StorageAccess {
    pub const fn operation(self) -> StorageOperation {
        self.operation
    }

    pub const fn start_sector(self) -> u64 {
        self.start_sector
    }

    pub const fn sector_count(self) -> u16 {
        self.sector_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageService {
    device: BlockDeviceInfo,
    resource: ResourceId,
}

impl StorageService {
    pub const fn new(device: BlockDeviceInfo) -> Self {
        Self {
            device,
            resource: STORAGE_RESOURCE_ID,
        }
    }

    pub fn authorize_request(
        self,
        table: &CapabilityTable,
        caller: ServiceId,
        handle: CapabilityHandle,
        request: StorageRequest,
    ) -> Result<StorageAccess, StorageServiceError> {
        table.validate(caller, handle, self.resource, required_rights(request))?;
        self.validate_range(request)?;
        Ok(StorageAccess {
            operation: request.operation,
            start_sector: request.start_sector,
            sector_count: request.sector_count,
        })
    }

    fn validate_range(self, request: StorageRequest) -> Result<(), StorageServiceError> {
        if request.sector_count == 0 {
            return Err(StorageServiceError::InvalidRange);
        }
        let end = request
            .start_sector
            .checked_add(u64::from(request.sector_count))
            .ok_or(StorageServiceError::InvalidRange)?;
        if end > self.device.capacity_sectors() {
            return Err(StorageServiceError::InvalidRange);
        }
        Ok(())
    }
}

fn required_rights(request: StorageRequest) -> RightsMask {
    match request.operation {
        StorageOperation::Read => RightsMask::new(RightsMask::READ),
        StorageOperation::Write => RightsMask::new(RightsMask::WRITE),
    }
}

pub fn run_self_test(device: BlockDeviceInfo) -> Result<(), StorageServiceError> {
    let service = StorageService::new(device);
    let mut identities = ServiceIdentityTable::new();
    let writer = identities.register_task(TaskId::new(70))?;
    let intruder = identities.register_task(TaskId::new(71))?;
    let mut table = CapabilityTable::new();
    let write_handle = table.grant(
        writer,
        STORAGE_RESOURCE_ID,
        RightsMask::new(RightsMask::READ | RightsMask::WRITE),
    )?;
    let read_only = table.grant(
        writer,
        STORAGE_RESOURCE_ID,
        RightsMask::new(RightsMask::READ),
    )?;
    if service.device.queue_size() == 0 {
        return Err(StorageServiceError::InvalidRange);
    }
    service.authorize_request(&table, writer, read_only, StorageRequest::read(0, 1))?;
    let write_request = StorageRequest::write(0, 1);

    service.authorize_request(&table, writer, write_handle, write_request)?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE:ACCESS_GRANTED");

    if service.authorize_request(&table, intruder, write_handle, write_request)
        != Err(StorageServiceError::Capability(
            CapabilityError::WrongHolder,
        ))
    {
        return Err(StorageServiceError::Capability(
            CapabilityError::WrongHolder,
        ));
    }
    if service.authorize_request(&table, writer, read_only, write_request)
        != Err(StorageServiceError::Capability(
            CapabilityError::MissingRights,
        ))
    {
        return Err(StorageServiceError::Capability(
            CapabilityError::MissingRights,
        ));
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:STORAGE:ACCESS_DENIED");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_service() -> StorageService {
        StorageService::new(BlockDeviceInfo::new_for_test(16, 8))
    }

    #[test]
    fn write_request_requires_write_capability() {
        let service = test_service();
        let mut identities = ServiceIdentityTable::new();
        let caller = identities.register_task(TaskId::new(70)).unwrap();
        let mut table = CapabilityTable::new();
        let handle = table
            .grant(
                caller,
                STORAGE_RESOURCE_ID,
                RightsMask::new(RightsMask::READ | RightsMask::WRITE),
            )
            .unwrap();

        assert_eq!(
            service.authorize_request(&table, caller, handle, StorageRequest::write(4, 2)),
            Ok(StorageAccess {
                operation: StorageOperation::Write,
                start_sector: 4,
                sector_count: 2,
            })
        );
    }

    #[test]
    fn read_only_capability_cannot_authorize_writes() {
        let service = test_service();
        let mut identities = ServiceIdentityTable::new();
        let caller = identities.register_task(TaskId::new(70)).unwrap();
        let mut table = CapabilityTable::new();
        let handle = table
            .grant(
                caller,
                STORAGE_RESOURCE_ID,
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();

        assert_eq!(
            service.authorize_request(&table, caller, handle, StorageRequest::write(0, 1)),
            Err(StorageServiceError::Capability(
                CapabilityError::MissingRights
            ))
        );
    }

    #[test]
    fn wrong_service_identity_cannot_reuse_storage_handle() {
        let service = test_service();
        let mut identities = ServiceIdentityTable::new();
        let owner = identities.register_task(TaskId::new(70)).unwrap();
        let intruder = identities.register_task(TaskId::new(71)).unwrap();
        let mut table = CapabilityTable::new();
        let handle = table
            .grant(
                owner,
                STORAGE_RESOURCE_ID,
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();

        assert_eq!(
            service.authorize_request(&table, intruder, handle, StorageRequest::read(0, 1)),
            Err(StorageServiceError::Capability(
                CapabilityError::WrongHolder
            ))
        );
    }

    #[test]
    fn sector_range_must_fit_inside_selected_device() {
        let service = test_service();
        let mut identities = ServiceIdentityTable::new();
        let caller = identities.register_task(TaskId::new(70)).unwrap();
        let mut table = CapabilityTable::new();
        let handle = table
            .grant(
                caller,
                STORAGE_RESOURCE_ID,
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();

        assert_eq!(
            service.authorize_request(&table, caller, handle, StorageRequest::read(15, 2)),
            Err(StorageServiceError::InvalidRange)
        );
        assert_eq!(
            service.authorize_request(&table, caller, handle, StorageRequest::read(0, 0)),
            Err(StorageServiceError::InvalidRange)
        );
    }
}
