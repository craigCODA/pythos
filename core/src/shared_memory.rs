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
}
