//! Kernel-owned capability handle table for Phase 3.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityTable};
use crate::tasks::TaskId;

const CAPABILITY_TABLE_SLOTS: usize = 8;
const PROOF_RESOURCE_ID: ResourceId = ResourceId::new(0xC0DA_0001);
const PROOF_RIGHTS: RightsMask = RightsMask::new(RightsMask::READ | RightsMask::WRITE);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityError {
    TableFull,
    InvalidHandle,
    Revoked,
    WrongHolder,
    WrongResource,
    MissingRights,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityHandle {
    slot: u32,
    generation: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceId(u64);

impl ResourceId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RightsMask(u32);

impl RightsMask {
    pub const READ: u32 = 1 << 0;
    pub const WRITE: u32 = 1 << 1;
    pub const SEND: u32 = 1 << 2;
    pub const LOG: u32 = 1 << 3;

    pub const fn new(bits: u32) -> Self {
        Self(bits)
    }

    fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityTable {
    entries: [Option<CapabilityEntry>; CAPABILITY_TABLE_SLOTS],
}

impl CapabilityTable {
    pub const fn new() -> Self {
        Self {
            entries: [None; CAPABILITY_TABLE_SLOTS],
        }
    }

    pub fn grant(
        &mut self,
        holder: ServiceId,
        resource: ResourceId,
        rights: RightsMask,
    ) -> Result<CapabilityHandle, CapabilityError> {
        let slot = self
            .entries
            .iter()
            .position(Option::is_none)
            .ok_or(CapabilityError::TableFull)?;
        let handle = CapabilityHandle {
            slot: slot as u32,
            generation: 1,
        };
        self.entries[slot] = Some(CapabilityEntry {
            holder,
            resource,
            rights,
            generation: handle.generation,
            state: CapabilityState::Active,
        });
        Ok(handle)
    }

    pub fn validate(
        &self,
        caller: ServiceId,
        handle: CapabilityHandle,
        resource: ResourceId,
        required_rights: RightsMask,
    ) -> Result<(), CapabilityError> {
        let entry = self
            .entries
            .get(handle.slot as usize)
            .and_then(Option::as_ref)
            .ok_or(CapabilityError::InvalidHandle)?;
        if entry.generation != handle.generation {
            return Err(CapabilityError::InvalidHandle);
        }
        if entry.state != CapabilityState::Active {
            return Err(CapabilityError::Revoked);
        }
        if entry.holder != caller {
            return Err(CapabilityError::WrongHolder);
        }
        if entry.resource != resource {
            return Err(CapabilityError::WrongResource);
        }
        if !entry.rights.contains(required_rights) {
            return Err(CapabilityError::MissingRights);
        }
        Ok(())
    }

    pub fn revoke(&mut self, handle: CapabilityHandle) -> Result<(), CapabilityError> {
        let entry = self
            .entries
            .get_mut(handle.slot as usize)
            .and_then(Option::as_mut)
            .ok_or(CapabilityError::InvalidHandle)?;
        if entry.generation != handle.generation {
            return Err(CapabilityError::InvalidHandle);
        }
        entry.state = CapabilityState::Revoked;
        entry.generation = entry.generation.wrapping_add(1);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CapabilityEntry {
    holder: ServiceId,
    resource: ResourceId,
    rights: RightsMask,
    generation: u32,
    state: CapabilityState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CapabilityState {
    Active,
    Revoked,
}

pub fn run_capability_handle_self_test() -> Result<(), CapabilityError> {
    let mut identities = ServiceIdentityTable::new();
    let holder = identities
        .register_task(TaskId::new(30))
        .map_err(|_| CapabilityError::InvalidHandle)?;
    let mut table = CapabilityTable::new();
    let handle = table.grant(holder, PROOF_RESOURCE_ID, PROOF_RIGHTS)?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:CAPABILITY:GRANT");

    table.validate(
        holder,
        handle,
        PROOF_RESOURCE_ID,
        RightsMask::new(RightsMask::READ),
    )?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:CAPABILITY:USE");
    Ok(())
}

pub fn run_revocation_self_test() -> Result<(), CapabilityError> {
    let mut identities = ServiceIdentityTable::new();
    let holder = identities
        .register_task(TaskId::new(32))
        .map_err(|_| CapabilityError::InvalidHandle)?;
    let mut table = CapabilityTable::new();
    let revoked = table.grant(holder, PROOF_RESOURCE_ID, PROOF_RIGHTS)?;
    let unaffected = table.grant(holder, ResourceId::new(0xC0DA_0002), PROOF_RIGHTS)?;

    table.revoke(revoked)?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:CAPABILITY:REVOKE");

    if table.validate(
        holder,
        revoked,
        PROOF_RESOURCE_ID,
        RightsMask::new(RightsMask::READ),
    ) != Err(CapabilityError::InvalidHandle)
    {
        return Err(CapabilityError::InvalidHandle);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:CAPABILITY:STALE_DENIED");

    table.validate(
        holder,
        unaffected,
        ResourceId::new(0xC0DA_0002),
        RightsMask::new(RightsMask::READ),
    )?;
    Ok(())
}

pub fn run_negative_authorization_self_test() -> Result<(), CapabilityError> {
    let mut identities = ServiceIdentityTable::new();
    let intruder = identities
        .register_task(TaskId::new(33))
        .map_err(|_| CapabilityError::InvalidHandle)?;
    let table = CapabilityTable::new();
    let forged = CapabilityHandle {
        slot: 0,
        generation: 1,
    };

    if table.validate(
        intruder,
        forged,
        PROOF_RESOURCE_ID,
        RightsMask::new(RightsMask::READ),
    ) != Err(CapabilityError::InvalidHandle)
    {
        return Err(CapabilityError::InvalidHandle);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:CAPABILITY:KNOWN_TARGET_DENIED");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granted_handle_validates_for_holder_resource_and_rights() {
        let mut identities = ServiceIdentityTable::new();
        let holder = identities.register_task(TaskId::new(30)).unwrap();
        let mut table = CapabilityTable::new();

        let handle = table
            .grant(holder, PROOF_RESOURCE_ID, PROOF_RIGHTS)
            .unwrap();

        assert_eq!(
            table.validate(
                holder,
                handle,
                PROOF_RESOURCE_ID,
                RightsMask::new(RightsMask::READ)
            ),
            Ok(())
        );
    }

    #[test]
    fn handle_value_does_not_authorize_wrong_holder() {
        let mut identities = ServiceIdentityTable::new();
        let holder = identities.register_task(TaskId::new(30)).unwrap();
        let stranger = identities.register_task(TaskId::new(31)).unwrap();
        let mut table = CapabilityTable::new();
        let handle = table
            .grant(holder, PROOF_RESOURCE_ID, PROOF_RIGHTS)
            .unwrap();

        assert_eq!(
            table.validate(
                stranger,
                handle,
                PROOF_RESOURCE_ID,
                RightsMask::new(RightsMask::READ)
            ),
            Err(CapabilityError::WrongHolder)
        );
    }

    #[test]
    fn revoked_handle_is_stale_but_other_handles_survive() {
        let mut identities = ServiceIdentityTable::new();
        let holder = identities.register_task(TaskId::new(32)).unwrap();
        let mut table = CapabilityTable::new();
        let revoked = table
            .grant(holder, PROOF_RESOURCE_ID, PROOF_RIGHTS)
            .unwrap();
        let unaffected = table
            .grant(holder, ResourceId::new(0xC0DA_0002), PROOF_RIGHTS)
            .unwrap();

        assert_eq!(table.revoke(revoked), Ok(()));
        assert_eq!(
            table.validate(
                holder,
                revoked,
                PROOF_RESOURCE_ID,
                RightsMask::new(RightsMask::READ)
            ),
            Err(CapabilityError::InvalidHandle)
        );
        assert_eq!(
            table.validate(
                holder,
                unaffected,
                ResourceId::new(0xC0DA_0002),
                RightsMask::new(RightsMask::READ)
            ),
            Ok(())
        );
    }

    #[test]
    fn knowing_resource_and_operation_without_handle_is_denied() {
        let mut identities = ServiceIdentityTable::new();
        let intruder = identities.register_task(TaskId::new(33)).unwrap();
        let table = CapabilityTable::new();
        let forged = CapabilityHandle {
            slot: 0,
            generation: 1,
        };

        assert_eq!(
            table.validate(
                intruder,
                forged,
                PROOF_RESOURCE_ID,
                RightsMask::new(RightsMask::READ)
            ),
            Err(CapabilityError::InvalidHandle)
        );
    }
}
