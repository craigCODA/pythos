//! Minimal Phase 4 Python service lifecycle manager.

#![cfg_attr(test, allow(dead_code))]

use crate::interpreter::{RuntimeInstance, RuntimeOperation};
#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityTable};
use crate::tasks::TaskId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceLifecycleError {
    MissingReadyOperation,
    UnknownService,
    InvalidTransition,
    ContainmentFailed,
    RestartFailed,
    EventDeliveryFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceEvent {
    NativeTick,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedServiceState {
    Starting,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ManagedService {
    identity: ServiceId,
    state: ManagedServiceState,
    generation: u32,
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
                generation: 0,
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

    pub fn contain_exception(
        &mut self,
        identity: ServiceId,
    ) -> Result<ManagedServiceState, ServiceLifecycleError> {
        if self.service.identity != identity {
            return Err(ServiceLifecycleError::UnknownService);
        }
        self.service.state = ManagedServiceState::Failed;
        #[cfg(not(test))]
        serial::write_line("PYTHOS:CORE:SERVICE:EXCEPTION");
        Ok(self.service.state)
    }

    pub fn restart_failed(
        &mut self,
        identity: ServiceId,
    ) -> Result<ManagedServiceState, ServiceLifecycleError> {
        if self.service.identity != identity {
            return Err(ServiceLifecycleError::UnknownService);
        }
        if self.service.state != ManagedServiceState::Failed {
            return Err(ServiceLifecycleError::InvalidTransition);
        }
        self.service.generation = self
            .service
            .generation
            .checked_add(1)
            .ok_or(ServiceLifecycleError::RestartFailed)?;
        self.service.state = ManagedServiceState::Starting;
        #[cfg(not(test))]
        serial::write_line("PYTHOS:CORE:SERVICE:RESTART");
        Ok(self.service.state)
    }

    pub const fn state(&self) -> ManagedServiceState {
        self.service.state
    }

    pub const fn generation(&self) -> u32 {
        self.service.generation
    }

    pub fn dispatch_event(
        &self,
        identity: ServiceId,
        event: ServiceEvent,
    ) -> Result<ServiceEvent, ServiceLifecycleError> {
        if self.service.identity != identity {
            return Err(ServiceLifecycleError::UnknownService);
        }
        if self.service.state != ManagedServiceState::Ready {
            return Err(ServiceLifecycleError::InvalidTransition);
        }
        #[cfg(not(test))]
        serial::write_line("PYTHOS:CORE:SERVICE:EVENT");
        Ok(event)
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

pub fn run_exception_containment_self_test() -> Result<(), ServiceLifecycleError> {
    let mut identities = ServiceIdentityTable::new();
    let primary = identities
        .register_task(TaskId::new(70))
        .map_err(|_| ServiceLifecycleError::ContainmentFailed)?;
    let failing = identities
        .register_task(TaskId::new(71))
        .map_err(|_| ServiceLifecycleError::ContainmentFailed)?;
    let mut primary_manager = ServiceManager::new(primary);
    let mut failing_manager = ServiceManager::new(failing);

    primary_manager.mark_ready(primary)?;
    failing_manager.contain_exception(failing)?;
    if failing_manager.state() != ManagedServiceState::Failed {
        return Err(ServiceLifecycleError::ContainmentFailed);
    }
    if primary_manager.state() != ManagedServiceState::Ready {
        return Err(ServiceLifecycleError::ContainmentFailed);
    }
    Ok(())
}

pub fn run_service_restart_self_test() -> Result<(), ServiceLifecycleError> {
    let mut identities = ServiceIdentityTable::new();
    let service = identities
        .register_task(TaskId::new(72))
        .map_err(|_| ServiceLifecycleError::RestartFailed)?;
    let mut manager = ServiceManager::new(service);

    manager.contain_exception(service)?;
    if manager.state() != ManagedServiceState::Failed {
        return Err(ServiceLifecycleError::RestartFailed);
    }
    manager.restart_failed(service)?;
    if manager.state() != ManagedServiceState::Starting || manager.generation() != 1 {
        return Err(ServiceLifecycleError::RestartFailed);
    }
    manager.mark_ready(service)?;
    if manager.state() != ManagedServiceState::Ready {
        return Err(ServiceLifecycleError::RestartFailed);
    }
    Ok(())
}

pub fn run_async_events_self_test() -> Result<(), ServiceLifecycleError> {
    let mut identities = ServiceIdentityTable::new();
    let ready_service = identities
        .register_task(TaskId::new(73))
        .map_err(|_| ServiceLifecycleError::EventDeliveryFailed)?;
    let failed_service = identities
        .register_task(TaskId::new(74))
        .map_err(|_| ServiceLifecycleError::EventDeliveryFailed)?;
    let mut ready_manager = ServiceManager::new(ready_service);
    let mut failed_manager = ServiceManager::new(failed_service);

    ready_manager.mark_ready(ready_service)?;
    failed_manager.contain_exception(failed_service)?;
    if failed_manager.dispatch_event(failed_service, ServiceEvent::NativeTick)
        != Err(ServiceLifecycleError::InvalidTransition)
    {
        return Err(ServiceLifecycleError::EventDeliveryFailed);
    }
    if ready_manager.dispatch_event(ready_service, ServiceEvent::NativeTick)
        != Ok(ServiceEvent::NativeTick)
    {
        return Err(ServiceLifecycleError::EventDeliveryFailed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{RUNTIME_TASK_ID, RuntimeProgram};
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

    #[test]
    fn unhandled_exception_marks_only_the_failing_service_failed() {
        let mut identities = ServiceIdentityTable::new();
        let healthy = identities
            .register_task(crate::tasks::TaskId::new(70))
            .unwrap();
        let failing = identities
            .register_task(crate::tasks::TaskId::new(71))
            .unwrap();
        let mut healthy_manager = ServiceManager::new(healthy);
        let mut failing_manager = ServiceManager::new(failing);

        assert_eq!(
            healthy_manager.mark_ready(healthy),
            Ok(ManagedServiceState::Ready)
        );
        assert_eq!(
            failing_manager.contain_exception(failing),
            Ok(ManagedServiceState::Failed)
        );
        assert_eq!(healthy_manager.state(), ManagedServiceState::Ready);
        assert_eq!(failing_manager.state(), ManagedServiceState::Failed);
    }

    #[test]
    fn failed_service_can_restart_to_fresh_starting_generation() {
        let mut identities = ServiceIdentityTable::new();
        let service = identities
            .register_task(crate::tasks::TaskId::new(72))
            .unwrap();
        let mut manager = ServiceManager::new(service);

        assert_eq!(
            manager.contain_exception(service),
            Ok(ManagedServiceState::Failed)
        );
        assert_eq!(
            manager.restart_failed(service),
            Ok(ManagedServiceState::Starting)
        );
        assert_eq!(manager.generation(), 1);
        assert_eq!(manager.mark_ready(service), Ok(ManagedServiceState::Ready));
    }

    #[test]
    fn async_events_dispatch_only_to_ready_services() {
        let mut identities = ServiceIdentityTable::new();
        let ready_service = identities
            .register_task(crate::tasks::TaskId::new(73))
            .unwrap();
        let failed_service = identities
            .register_task(crate::tasks::TaskId::new(74))
            .unwrap();
        let mut ready_manager = ServiceManager::new(ready_service);
        let mut failed_manager = ServiceManager::new(failed_service);

        assert_eq!(
            ready_manager.mark_ready(ready_service),
            Ok(ManagedServiceState::Ready)
        );
        assert_eq!(
            failed_manager.contain_exception(failed_service),
            Ok(ManagedServiceState::Failed)
        );
        assert_eq!(
            failed_manager.dispatch_event(failed_service, ServiceEvent::NativeTick),
            Err(ServiceLifecycleError::InvalidTransition)
        );
        assert_eq!(
            ready_manager.dispatch_event(ready_service, ServiceEvent::NativeTick),
            Ok(ServiceEvent::NativeTick)
        );
    }
}
