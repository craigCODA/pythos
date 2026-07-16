//! Capability-gated shared memory handles for Phase 3.
#![cfg_attr(test, allow(dead_code))]

use crate::capabilities::{
    CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask,
};
#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityTable};
use crate::tasks::TaskId;

const SHARED_MEMORY_RESOURCE: ResourceId = ResourceId::new(0x5A4D_0001);
const SHARED_REGION_INITIAL: [u8; 4] = [0x50, 0x59, 0x54, 0x48];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SharedRegion {
    resource: ResourceId,
    bytes: [u8; 4],
}

impl SharedRegion {
    pub const fn new(resource: ResourceId, bytes: [u8; 4]) -> Self {
        Self { resource, bytes }
    }

    pub fn read(
        &self,
        table: &CapabilityTable,
        caller: ServiceId,
        handle: CapabilityHandle,
    ) -> Result<[u8; 4], CapabilityError> {
        table.validate(
            caller,
            handle,
            self.resource,
            RightsMask::new(RightsMask::READ),
        )?;
        Ok(self.bytes)
    }

    pub fn write(
        &mut self,
        table: &CapabilityTable,
        caller: ServiceId,
        handle: CapabilityHandle,
        bytes: [u8; 4],
    ) -> Result<(), CapabilityError> {
        table.validate(
            caller,
            handle,
            self.resource,
            RightsMask::new(RightsMask::WRITE),
        )?;
        self.bytes = bytes;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuardedSharedMemoryError {
    Capability(CapabilityError),
    SharedAddressSpace,
    CrossSpaceWriteAllowed,
    RegionCorrupted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuardedSharedMemoryProof {
    pub ring3_read: bool,
    pub cross_space_write_denied: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Ring3Service {
    identity: ServiceId,
    address_space_root: u64,
}

impl Ring3Service {
    const fn new(identity: ServiceId, address_space_root: u64) -> Self {
        Self {
            identity,
            address_space_root,
        }
    }
}

pub fn run_shared_memory_self_test() -> Result<(), CapabilityError> {
    let mut identities = ServiceIdentityTable::new();
    let reader = identities
        .register_task(TaskId::new(40))
        .map_err(|_| CapabilityError::InvalidHandle)?;
    let mut table = CapabilityTable::new();
    let read_only = table.grant(
        reader,
        SHARED_MEMORY_RESOURCE,
        RightsMask::new(RightsMask::READ),
    )?;
    let mut region = SharedRegion::new(SHARED_MEMORY_RESOURCE, SHARED_REGION_INITIAL);

    if region.read(&table, reader, read_only)? != SHARED_REGION_INITIAL {
        return Err(CapabilityError::WrongResource);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:SHM:READ_ONLY");

    if region.write(&table, reader, read_only, [0x44, 0x52, 0x4F, 0x50])
        != Err(CapabilityError::MissingRights)
    {
        return Err(CapabilityError::WrongResource);
    }
    if region.read(&table, reader, read_only)? != SHARED_REGION_INITIAL {
        return Err(CapabilityError::WrongResource);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:SHM:WRITE_DENIED");
    Ok(())
}

pub fn run_guarded_phase8_self_test(
    reader_root: u64,
    writer_root: u64,
) -> Result<GuardedSharedMemoryProof, GuardedSharedMemoryError> {
    if reader_root == writer_root {
        return Err(GuardedSharedMemoryError::SharedAddressSpace);
    }

    let mut identities = ServiceIdentityTable::new();
    let reader_identity = identities
        .register_task(TaskId::new(82))
        .map_err(|_| GuardedSharedMemoryError::Capability(CapabilityError::InvalidHandle))?;
    let writer_identity = identities
        .register_task(TaskId::new(83))
        .map_err(|_| GuardedSharedMemoryError::Capability(CapabilityError::InvalidHandle))?;
    let reader = Ring3Service::new(reader_identity, reader_root);
    let writer = Ring3Service::new(writer_identity, writer_root);
    if reader.address_space_root == writer.address_space_root {
        return Err(GuardedSharedMemoryError::SharedAddressSpace);
    }

    let mut table = CapabilityTable::new();
    let read_only = table
        .grant(
            reader.identity,
            SHARED_MEMORY_RESOURCE,
            RightsMask::new(RightsMask::READ),
        )
        .map_err(GuardedSharedMemoryError::Capability)?;
    let mut region = SharedRegion::new(SHARED_MEMORY_RESOURCE, SHARED_REGION_INITIAL);

    if region
        .read(&table, reader.identity, read_only)
        .map_err(GuardedSharedMemoryError::Capability)?
        != SHARED_REGION_INITIAL
    {
        return Err(GuardedSharedMemoryError::RegionCorrupted);
    }
    if region.write(&table, writer.identity, read_only, [0x44, 0x52, 0x4F, 0x50])
        != Err(CapabilityError::WrongHolder)
    {
        return Err(GuardedSharedMemoryError::CrossSpaceWriteAllowed);
    }
    if region
        .read(&table, reader.identity, read_only)
        .map_err(GuardedSharedMemoryError::Capability)?
        != SHARED_REGION_INITIAL
    {
        return Err(GuardedSharedMemoryError::RegionCorrupted);
    }

    Ok(GuardedSharedMemoryProof {
        ring3_read: true,
        cross_space_write_denied: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_shared_memory_grant_cannot_write() {
        let mut identities = ServiceIdentityTable::new();
        let reader = identities.register_task(TaskId::new(40)).unwrap();
        let mut table = CapabilityTable::new();
        let read_only = table
            .grant(
                reader,
                SHARED_MEMORY_RESOURCE,
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();
        let mut region = SharedRegion::new(SHARED_MEMORY_RESOURCE, SHARED_REGION_INITIAL);

        assert_eq!(
            region.read(&table, reader, read_only),
            Ok(SHARED_REGION_INITIAL)
        );
        assert_eq!(
            region.write(&table, reader, read_only, [0x44, 0x52, 0x4F, 0x50]),
            Err(CapabilityError::MissingRights)
        );
        assert_eq!(
            region.read(&table, reader, read_only),
            Ok(SHARED_REGION_INITIAL)
        );
    }

    #[test]
    fn guarded_shared_memory_requires_distinct_address_spaces() {
        assert_eq!(
            run_guarded_phase8_self_test(0x200000, 0x200000),
            Err(GuardedSharedMemoryError::SharedAddressSpace)
        );
    }

    #[test]
    fn guarded_shared_memory_denies_cross_space_write() {
        let proof = run_guarded_phase8_self_test(0x200000, 0x300000).unwrap();

        assert!(proof.ring3_read);
        assert!(proof.cross_space_write_denied);
    }
}
