//! Narrow Phase 4 `system.*` host-call surface.

#![cfg_attr(test, allow(dead_code))]

use crate::capabilities::{
    CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask,
};
use crate::interpreter::{RuntimeInstance, RuntimeOperation};
#[cfg(not(test))]
use crate::serial;
use crate::service_identity::ServiceId;
#[cfg(test)]
use crate::service_identity::ServiceIdentityTable;
#[cfg(test)]
use crate::tasks::TaskId;

const SYSTEM_LOG_RESOURCE: ResourceId = ResourceId::new(0x5159_5354_4C4F_4700);
const MAX_LOG_MESSAGE_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemApiError {
    Capability(CapabilityError),
    MissingLogOperation,
    EmptyMessage,
    MessageTooLong,
}

impl From<CapabilityError> for SystemApiError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

pub struct SystemApiHost {
    capabilities: CapabilityTable,
}

impl SystemApiHost {
    pub const fn new() -> Self {
        Self {
            capabilities: CapabilityTable::new(),
        }
    }

    pub fn grant_log(&mut self, service: ServiceId) -> Result<CapabilityHandle, SystemApiError> {
        self.capabilities
            .grant(
                service,
                SYSTEM_LOG_RESOURCE,
                RightsMask::new(RightsMask::LOG),
            )
            .map_err(SystemApiError::Capability)
    }

    pub fn log(
        &self,
        caller: ServiceId,
        handle: CapabilityHandle,
        message: &str,
    ) -> Result<(), SystemApiError> {
        self.capabilities.validate(
            caller,
            handle,
            SYSTEM_LOG_RESOURCE,
            RightsMask::new(RightsMask::LOG),
        )?;
        if message.is_empty() {
            return Err(SystemApiError::EmptyMessage);
        }
        if message.len() > MAX_LOG_MESSAGE_BYTES {
            return Err(SystemApiError::MessageTooLong);
        }
        #[cfg(not(test))]
        serial::write_line("PYTHOS:CORE:SYSTEM:LOG");
        Ok(())
    }
}

pub fn run_log_surface(instance: &RuntimeInstance<'_>) -> Result<(), SystemApiError> {
    let mut host = SystemApiHost::new();
    let log_handle = host.grant_log(instance.service_id)?;
    for operation in instance.program.operations {
        if let RuntimeOperation::SystemLog(message) = operation {
            return host.log(instance.service_id, log_handle, message);
        }
    }
    Err(SystemApiError::MissingLogOperation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{RUNTIME_TASK_ID, RuntimeProgram};

    fn service(task_id: TaskId) -> ServiceId {
        let mut identities = ServiceIdentityTable::new();
        identities.register_task(task_id).unwrap()
    }

    #[test]
    fn system_log_requires_explicit_log_capability() {
        let mut identities = ServiceIdentityTable::new();
        let runtime = identities.register_task(RUNTIME_TASK_ID).unwrap();
        let stranger = identities.register_task(TaskId::new(41)).unwrap();
        let mut host = SystemApiHost::new();
        let wrong_handle = host.grant_log(stranger).unwrap();

        assert_eq!(
            host.log(runtime, wrong_handle, "hello from Python"),
            Err(SystemApiError::Capability(CapabilityError::WrongHolder))
        );
    }

    #[test]
    fn system_log_accepts_bounded_message_with_log_capability() {
        let runtime = service(RUNTIME_TASK_ID);
        let mut host = SystemApiHost::new();
        let handle = host.grant_log(runtime).unwrap();

        assert_eq!(host.log(runtime, handle, "hello from Python"), Ok(()));
    }

    #[test]
    fn system_log_rejects_empty_or_oversized_messages() {
        let runtime = service(RUNTIME_TASK_ID);
        let mut host = SystemApiHost::new();
        let handle = host.grant_log(runtime).unwrap();
        let long_message = "x".repeat(MAX_LOG_MESSAGE_BYTES + 1);

        assert_eq!(
            host.log(runtime, handle, ""),
            Err(SystemApiError::EmptyMessage)
        );
        assert_eq!(
            host.log(runtime, handle, &long_message),
            Err(SystemApiError::MessageTooLong)
        );
    }

    #[test]
    fn runtime_plan_invokes_only_the_first_system_log_operation() {
        let runtime = service(RUNTIME_TASK_ID);
        let instance = RuntimeInstance {
            task_id: RUNTIME_TASK_ID,
            service_id: runtime,
            program: RuntimeProgram {
                service_name: "HelloService",
                entrypoint: "start",
                operations: [
                    RuntimeOperation::SystemLog("hello from Python"),
                    RuntimeOperation::Ready,
                ],
            },
        };

        assert_eq!(run_log_surface(&instance), Ok(()));
    }

    #[test]
    fn runtime_plan_without_log_operation_is_rejected() {
        let runtime = service(RUNTIME_TASK_ID);
        let instance = RuntimeInstance {
            task_id: RUNTIME_TASK_ID,
            service_id: runtime,
            program: RuntimeProgram {
                service_name: "HelloService",
                entrypoint: "start",
                operations: [RuntimeOperation::Ready, RuntimeOperation::Ready],
            },
        };

        assert_eq!(
            run_log_surface(&instance),
            Err(SystemApiError::MissingLogOperation)
        );
    }
}
