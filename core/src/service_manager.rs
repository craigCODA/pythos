//! Minimal Phase 4 Python service lifecycle manager.

#![cfg_attr(test, allow(dead_code))]

use crate::interpreter::{RuntimeInstance, RuntimeOperation};
#[cfg(not(test))]
use crate::serial;
use crate::service_identity::ServiceId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceLifecycleError {
    MissingReadyOperation,
    UnknownService,
    InvalidTransition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedServiceState {
    Starting,
    Ready,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ManagedService {
    identity: ServiceId,
    state: ManagedServiceState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceManager {
    service: ManagedService,
}

impl ServiceManager {
    pub const fn new(identity: ServiceId) -> Self {
        Self {
            service: ManagedService {
                identity,
                state: ManagedServiceState::Starting,
            },
        }
    }

    pub fn mark_ready(
        &mut self,
        identity: ServiceId,
    ) -> Result<ManagedServiceState, ServiceLifecycleError> {
        if self.service.identity != identity {
            return Err(ServiceLifecycleError::UnknownService);
        }
        if self.service.state != ManagedServiceState::Starting {
            return Err(ServiceLifecycleError::InvalidTransition);
        }
        self.service.state = ManagedServiceState::Ready;
        #[cfg(not(test))]
        serial::write_line("PYTHOS:CORE:SERVICE:READY");
        Ok(self.service.state)
    }

    pub const fn state(&self) -> ManagedServiceState {
        self.service.state
    }
}

pub fn run_self_test(instance: &RuntimeInstance<'_>) -> Result<(), ServiceLifecycleError> {
    let mut manager = ServiceManager::new(instance.service_id);
    for operation in instance.program.operations {
        if operation == RuntimeOperation::Ready {
            manager.mark_ready(instance.service_id)?;
            if manager.state() != ManagedServiceState::Ready {
                return Err(ServiceLifecycleError::InvalidTransition);
            }
            return Ok(());
        }
    }
    Err(ServiceLifecycleError::MissingReadyOperation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{RUNTIME_TASK_ID, RuntimeProgram};
    use crate::service_identity::ServiceIdentityTable;
    use crate::value_validation::UntrustedRuntimeValue;

    fn service_id() -> ServiceId {
        let mut identities = ServiceIdentityTable::new();
        identities.register_task(RUNTIME_TASK_ID).unwrap()
    }

    #[test]
    fn ready_operation_transitions_service_to_ready() {
        let service_id = service_id();
        let instance = RuntimeInstance {
            task_id: RUNTIME_TASK_ID,
            service_id,
            program: RuntimeProgram {
                service_name: "HelloService",
                entrypoint: "start",
                operations: [
                    RuntimeOperation::SystemLog(UntrustedRuntimeValue::StringBytes(
                        b"PythOS [HISS] We Are Woken",
                    )),
                    RuntimeOperation::Ready,
                ],
            },
        };

        assert_eq!(run_self_test(&instance), Ok(()));
    }

    #[test]
    fn ready_transition_requires_known_starting_service() {
        let mut identities = ServiceIdentityTable::new();
        let service_id = identities.register_task(RUNTIME_TASK_ID).unwrap();
        let stranger = identities
            .register_task(crate::tasks::TaskId::new(41))
            .unwrap();
        let mut manager = ServiceManager::new(service_id);

        assert_eq!(
            manager.mark_ready(stranger),
            Err(ServiceLifecycleError::UnknownService)
        );
        assert_eq!(
            manager.mark_ready(service_id),
            Ok(ManagedServiceState::Ready)
        );
        assert_eq!(
            manager.mark_ready(service_id),
            Err(ServiceLifecycleError::InvalidTransition)
        );
    }
}
