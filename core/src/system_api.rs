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
use crate::value_validation::{
    HostCallResult, UntrustedRuntimeValue, ValueValidationError, validate_system_string,
};

const SYSTEM_LOG_RESOURCE: ResourceId = ResourceId::new(0x5159_5354_4C4F_4700);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemApiError {
    Capability(CapabilityError),
    MissingLogOperation,
    Value(ValueValidationError),
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
        value: UntrustedRuntimeValue<'_>,
    ) -> Result<HostCallResult, SystemApiError> {
        self.capabilities.validate(
            caller,
            handle,
            SYSTEM_LOG_RESOURCE,
            RightsMask::new(RightsMask::LOG),
        )?;
        let message = match validate_system_string(value) {
            Ok(message) => message,
            Err(error) => return Ok(HostCallResult::rejected(error)),
        };
        if message.as_str() != "PythOS [HISS] We Are Woken" {
            return Ok(HostCallResult::rejected(
                ValueValidationError::UnsupportedType,
            ));
        }
        #[cfg(not(test))]
        serial::write_line("PYTHOS:CORE:SYSTEM:LOG");
        Ok(HostCallResult::returned())
    }
}

pub fn run_log_surface(instance: &RuntimeInstance<'_>) -> Result<(), SystemApiError> {
    let mut host = SystemApiHost::new();
    let log_handle = host.grant_log(instance.service_id)?;
    for operation in instance.program.operations {
        if let RuntimeOperation::SystemLog(value) = operation {
            return match host.log(instance.service_id, log_handle, value)? {
                HostCallResult::Returned => Ok(()),
                HostCallResult::Rejected(error) => Err(SystemApiError::Value(error)),
            };
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
            host.log(
                runtime,
                wrong_handle,
                UntrustedRuntimeValue::StringBytes(b"PythOS [HISS] We Are Woken")
            ),
            Err(SystemApiError::Capability(CapabilityError::WrongHolder))
        );
    }

    #[test]
    fn system_log_accepts_bounded_message_with_log_capability() {
        let runtime = service(RUNTIME_TASK_ID);
        let mut host = SystemApiHost::new();
        let handle = host.grant_log(runtime).unwrap();

        assert_eq!(
            host.log(
                runtime,
                handle,
                UntrustedRuntimeValue::StringBytes(b"PythOS [HISS] We Are Woken")
            ),
            Ok(HostCallResult::Returned)
        );
    }

    #[test]
    fn system_log_rejects_empty_or_oversized_messages() {
        let runtime = service(RUNTIME_TASK_ID);
        let mut host = SystemApiHost::new();
        let handle = host.grant_log(runtime).unwrap();
        let long_message = [b'x'; crate::value_validation::MAX_SYSTEM_STRING_BYTES + 1];

        assert_eq!(
            host.log(runtime, handle, UntrustedRuntimeValue::StringBytes(b"")),
            Ok(HostCallResult::Rejected(ValueValidationError::EmptyString))
        );
        assert_eq!(
            host.log(
                runtime,
                handle,
                UntrustedRuntimeValue::StringBytes(&long_message)
            ),
            Ok(HostCallResult::Rejected(
                ValueValidationError::StringTooLong
            ))
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
                    RuntimeOperation::SystemLog(UntrustedRuntimeValue::StringBytes(
                        b"PythOS [HISS] We Are Woken",
                    )),
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
