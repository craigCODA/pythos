#![cfg_attr(test, allow(dead_code))]

use crate::capabilities::{
    CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask,
};
use crate::service_identity::{ServiceId, ServiceIdentityError, ServiceIdentityTable};
use crate::tasks::TaskId;

pub const MAX_DYNAMIC_PROCESSES: usize = 4;
const MAX_PROCESS_CAPABILITIES: usize = 4;
const PROOF_RESOURCE_ID: ResourceId = ResourceId::new(0x4459_4E43_4150_0001);
const PROOF_RIGHTS: RightsMask = RightsMask::new(RightsMask::READ | RightsMask::SEND);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicCapabilityError {
    ProcessTableFull,
    DuplicateTask,
    ProcessIdExhausted,
    CapabilityInventoryFull,
    ServiceIdentity(ServiceIdentityError),
    Capability(CapabilityError),
    NoGrant,
    ProofFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicCapabilityGrantProof {
    pub process_created: bool,
    pub zero_default: bool,
    pub no_grant_denied: bool,
    pub grant_issued: bool,
    pub granted_use: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitialGrant {
    resource: ResourceId,
    rights: RightsMask,
}

impl InitialGrant {
    pub const fn new(resource: ResourceId, rights: RightsMask) -> Self {
        Self { resource, rights }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreatorGrantPolicy {
    grants: [Option<InitialGrant>; MAX_PROCESS_CAPABILITIES],
    grant_count: usize,
}

impl CreatorGrantPolicy {
    pub const fn empty() -> Self {
        Self {
            grants: [None; MAX_PROCESS_CAPABILITIES],
            grant_count: 0,
        }
    }

    pub const fn single(grant: InitialGrant) -> Self {
        let mut policy = Self::empty();
        policy.grants[0] = Some(grant);
        policy.grant_count = 1;
        policy
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicProcessId(u64);

impl DynamicProcessId {
    const fn new(raw: u64) -> Self {
        Self(raw)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessCapability {
    handle: CapabilityHandle,
    resource: ResourceId,
    rights: RightsMask,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicProcess {
    id: DynamicProcessId,
    task_id: TaskId,
    service_id: ServiceId,
    capabilities: [Option<ProcessCapability>; MAX_PROCESS_CAPABILITIES],
    capability_count: usize,
}

impl DynamicProcess {
    const fn new(id: DynamicProcessId, task_id: TaskId, service_id: ServiceId) -> Self {
        Self {
            id,
            task_id,
            service_id,
            capabilities: [None; MAX_PROCESS_CAPABILITIES],
            capability_count: 0,
        }
    }

    pub const fn capability_count(self) -> usize {
        self.capability_count
    }

    pub const fn task_id(self) -> TaskId {
        self.task_id
    }

    pub const fn service_id(self) -> ServiceId {
        self.service_id
    }

    pub fn use_capability(
        self,
        table: &CapabilityTable,
        resource: ResourceId,
        required_rights: RightsMask,
    ) -> Result<CapabilityHandle, DynamicCapabilityError> {
        let mut saw_resource = false;
        let mut index = 0;
        while index < self.capability_count {
            if let Some(process_capability) = self.capabilities[index]
                && process_capability.resource == resource
            {
                saw_resource = true;
                match table.validate(
                    self.service_id,
                    process_capability.handle,
                    resource,
                    required_rights,
                ) {
                    Ok(()) => return Ok(process_capability.handle),
                    Err(CapabilityError::MissingRights) => {}
                    Err(error) => return Err(DynamicCapabilityError::Capability(error)),
                }
            }
            index += 1;
        }

        if saw_resource {
            Err(DynamicCapabilityError::Capability(
                CapabilityError::MissingRights,
            ))
        } else {
            Err(DynamicCapabilityError::NoGrant)
        }
    }

    fn add_initial_grant(
        &mut self,
        table: &mut CapabilityTable,
        grant: InitialGrant,
    ) -> Result<(), DynamicCapabilityError> {
        if self.capability_count >= MAX_PROCESS_CAPABILITIES {
            return Err(DynamicCapabilityError::CapabilityInventoryFull);
        }
        let handle = table
            .grant(self.service_id, grant.resource, grant.rights)
            .map_err(DynamicCapabilityError::Capability)?;
        self.capabilities[self.capability_count] = Some(ProcessCapability {
            handle,
            resource: grant.resource,
            rights: grant.rights,
        });
        self.capability_count += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicProcessTable {
    processes: [Option<DynamicProcess>; MAX_DYNAMIC_PROCESSES],
    next_id: u64,
}

impl DynamicProcessTable {
    pub const fn new() -> Self {
        Self {
            processes: [None; MAX_DYNAMIC_PROCESSES],
            next_id: 1,
        }
    }

    pub fn spawn(
        &mut self,
        identities: &mut ServiceIdentityTable,
        capabilities: &mut CapabilityTable,
        task_id: TaskId,
        grant_policy: CreatorGrantPolicy,
    ) -> Result<DynamicProcess, DynamicCapabilityError> {
        if self
            .processes
            .iter()
            .flatten()
            .any(|process| process.task_id == task_id)
        {
            return Err(DynamicCapabilityError::DuplicateTask);
        }

        let slot = self
            .processes
            .iter()
            .position(Option::is_none)
            .ok_or(DynamicCapabilityError::ProcessTableFull)?;
        let service_id = identities
            .register_task(task_id)
            .map_err(DynamicCapabilityError::ServiceIdentity)?;
        let process_id = DynamicProcessId::new(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(DynamicCapabilityError::ProcessIdExhausted)?;
        let mut process = DynamicProcess::new(process_id, task_id, service_id);

        let mut grant_index = 0;
        while grant_index < grant_policy.grant_count {
            let grant = grant_policy.grants[grant_index].ok_or(DynamicCapabilityError::NoGrant)?;
            process.add_initial_grant(capabilities, grant)?;
            grant_index += 1;
        }

        self.processes[slot] = Some(process);
        Ok(process)
    }
}

pub fn run_self_test() -> Result<DynamicCapabilityGrantProof, DynamicCapabilityError> {
    let mut identities = ServiceIdentityTable::new();
    let mut capabilities = CapabilityTable::new();
    let mut processes = DynamicProcessTable::new();

    let empty_process = processes.spawn(
        &mut identities,
        &mut capabilities,
        TaskId::new(120),
        CreatorGrantPolicy::empty(),
    )?;
    let process_created = identities.task_for(empty_process.service_id()) == Some(TaskId::new(120))
        && empty_process.task_id() == TaskId::new(120);
    let zero_default = empty_process.capability_count() == 0;
    let no_grant_denied = empty_process.use_capability(
        &capabilities,
        PROOF_RESOURCE_ID,
        RightsMask::new(RightsMask::READ),
    ) == Err(DynamicCapabilityError::NoGrant);

    let granted_process = processes.spawn(
        &mut identities,
        &mut capabilities,
        TaskId::new(121),
        CreatorGrantPolicy::single(InitialGrant::new(PROOF_RESOURCE_ID, PROOF_RIGHTS)),
    )?;
    let grant_issued = granted_process.capability_count() == 1;
    let granted_use = granted_process
        .use_capability(
            &capabilities,
            PROOF_RESOURCE_ID,
            RightsMask::new(RightsMask::READ),
        )
        .is_ok();

    if !process_created || !zero_default || !no_grant_denied || !grant_issued || !granted_use {
        return Err(DynamicCapabilityError::ProofFailed);
    }

    Ok(DynamicCapabilityGrantProof {
        process_created,
        zero_default,
        no_grant_denied,
        grant_issued,
        granted_use,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::{CapabilityTable, ResourceId, RightsMask};
    use crate::service_identity::ServiceIdentityTable;
    use crate::tasks::TaskId;

    const TEST_RESOURCE: ResourceId = ResourceId::new(0x4459_4E43_4150_0001);
    const TEST_RIGHTS: RightsMask = RightsMask::new(RightsMask::READ | RightsMask::SEND);

    #[test]
    fn newly_created_process_has_zero_capabilities_by_default() {
        let mut identities = ServiceIdentityTable::new();
        let mut capabilities = CapabilityTable::new();
        let mut processes = DynamicProcessTable::new();

        let process = processes
            .spawn(
                &mut identities,
                &mut capabilities,
                TaskId::new(120),
                CreatorGrantPolicy::empty(),
            )
            .unwrap();

        assert_eq!(process.capability_count(), 0);
    }

    #[test]
    fn process_without_initial_grant_cannot_use_known_resource() {
        let mut identities = ServiceIdentityTable::new();
        let mut capabilities = CapabilityTable::new();
        let mut processes = DynamicProcessTable::new();
        let process = processes
            .spawn(
                &mut identities,
                &mut capabilities,
                TaskId::new(121),
                CreatorGrantPolicy::empty(),
            )
            .unwrap();

        assert_eq!(
            process.use_capability(
                &capabilities,
                TEST_RESOURCE,
                RightsMask::new(RightsMask::READ)
            ),
            Err(DynamicCapabilityError::NoGrant)
        );
    }

    #[test]
    fn explicit_creator_grant_allows_capability_use() {
        let mut identities = ServiceIdentityTable::new();
        let mut capabilities = CapabilityTable::new();
        let mut processes = DynamicProcessTable::new();
        let policy = CreatorGrantPolicy::single(InitialGrant::new(TEST_RESOURCE, TEST_RIGHTS));

        let process = processes
            .spawn(&mut identities, &mut capabilities, TaskId::new(122), policy)
            .unwrap();

        assert_eq!(process.capability_count(), 1);
        assert!(
            process
                .use_capability(
                    &capabilities,
                    TEST_RESOURCE,
                    RightsMask::new(RightsMask::READ)
                )
                .is_ok()
        );
    }

    #[test]
    fn process_table_capacity_is_bounded() {
        let mut identities = ServiceIdentityTable::new();
        let mut capabilities = CapabilityTable::new();
        let mut processes = DynamicProcessTable::new();

        for index in 0..MAX_DYNAMIC_PROCESSES {
            processes
                .spawn(
                    &mut identities,
                    &mut capabilities,
                    TaskId::new(130 + index as u64),
                    CreatorGrantPolicy::empty(),
                )
                .unwrap();
        }

        assert_eq!(
            processes.spawn(
                &mut identities,
                &mut capabilities,
                TaskId::new(140),
                CreatorGrantPolicy::empty(),
            ),
            Err(DynamicCapabilityError::ProcessTableFull)
        );
    }

    #[test]
    fn self_test_proves_required_dynamic_grant_cases() {
        let proof = run_self_test().unwrap();

        assert!(proof.process_created);
        assert!(proof.zero_default);
        assert!(proof.no_grant_denied);
        assert!(proof.grant_issued);
        assert!(proof.granted_use);
    }
}
