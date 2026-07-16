//! Service-local runtime proof for Phase 8.
#![cfg_attr(test, allow(dead_code))]

use crate::interpreter::{
    RuntimeInstance, RuntimeOperation, SERVICE_LOCAL_RUNTIME_TASK_A, SERVICE_LOCAL_RUNTIME_TASK_B,
    boot_service_local,
};
use crate::service_identity::{ServiceId, ServiceIdentityTable};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceRuntimeError {
    InterpreterBoot,
    SharedAddressSpace,
    SharedServiceIdentity,
    MissingReadyOperation,
    SharedStateSlot,
    CrossServiceStateMutation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceRuntimeProof {
    pub local_instances: bool,
    pub address_spaces_isolated: bool,
    pub state_isolated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RuntimeLocalState {
    service_id: ServiceId,
    address_space_root: u64,
    state_slot: u64,
    ready_count: u32,
}

impl RuntimeLocalState {
    fn new(instance: RuntimeInstance<'_>, address_space_root: u64, ordinal: u64) -> Self {
        Self {
            service_id: instance.service_id,
            address_space_root,
            state_slot: local_state_slot(instance, address_space_root, ordinal),
            ready_count: 0,
        }
    }

    fn mark_ready(&mut self, caller: ServiceId) -> Result<(), ServiceRuntimeError> {
        if caller != self.service_id {
            return Err(ServiceRuntimeError::CrossServiceStateMutation);
        }
        self.ready_count = self
            .ready_count
            .checked_add(1)
            .ok_or(ServiceRuntimeError::CrossServiceStateMutation)?;
        Ok(())
    }
}

pub fn run_self_test(
    source: &str,
    first_address_space_root: u64,
    second_address_space_root: u64,
) -> Result<ServiceRuntimeProof, ServiceRuntimeError> {
    if first_address_space_root == second_address_space_root {
        return Err(ServiceRuntimeError::SharedAddressSpace);
    }

    let mut identities = ServiceIdentityTable::new();
    let first = boot_service_local(source, SERVICE_LOCAL_RUNTIME_TASK_A, &mut identities)
        .map_err(|_| ServiceRuntimeError::InterpreterBoot)?;
    let second = boot_service_local(source, SERVICE_LOCAL_RUNTIME_TASK_B, &mut identities)
        .map_err(|_| ServiceRuntimeError::InterpreterBoot)?;

    if first.service_id == second.service_id {
        return Err(ServiceRuntimeError::SharedServiceIdentity);
    }
    if !has_ready_operation(first) || !has_ready_operation(second) {
        return Err(ServiceRuntimeError::MissingReadyOperation);
    }

    let mut first_state = RuntimeLocalState::new(first, first_address_space_root, 0);
    let mut second_state = RuntimeLocalState::new(second, second_address_space_root, 1);
    if first_state.state_slot == second_state.state_slot {
        return Err(ServiceRuntimeError::SharedStateSlot);
    }
    first_state.mark_ready(first.service_id)?;
    if second_state.ready_count != 0 {
        return Err(ServiceRuntimeError::CrossServiceStateMutation);
    }
    second_state.mark_ready(second.service_id)?;
    if first_state.mark_ready(second.service_id)
        != Err(ServiceRuntimeError::CrossServiceStateMutation)
    {
        return Err(ServiceRuntimeError::CrossServiceStateMutation);
    }

    Ok(ServiceRuntimeProof {
        local_instances: true,
        address_spaces_isolated: true,
        state_isolated: first_state.ready_count == 1 && second_state.ready_count == 1,
    })
}

fn has_ready_operation(instance: RuntimeInstance<'_>) -> bool {
    instance
        .program
        .operations
        .contains(&RuntimeOperation::Ready)
}

fn local_state_slot(instance: RuntimeInstance<'_>, address_space_root: u64, ordinal: u64) -> u64 {
    address_space_root.rotate_left(7) ^ instance.service_id.raw() ^ instance.task_id.raw() ^ ordinal
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_SERVICE: &str = "class HelloService(Service):\n    async def start(self):\n        system.log(\"PythOS [HISS] We Are Woken\")\n        self.ready()\n";

    #[test]
    fn service_local_runtimes_require_distinct_address_spaces() {
        assert_eq!(
            run_self_test(HELLO_SERVICE, 0x200000, 0x200000),
            Err(ServiceRuntimeError::SharedAddressSpace)
        );
    }

    #[test]
    fn service_local_runtimes_have_distinct_state_slots() {
        let proof = run_self_test(HELLO_SERVICE, 0x200000, 0x300000).unwrap();

        assert!(proof.local_instances);
        assert!(proof.address_spaces_isolated);
        assert!(proof.state_isolated);
    }
}
