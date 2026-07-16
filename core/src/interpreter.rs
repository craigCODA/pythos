//! Custom-minimal Phase 4 interpreter bootstrap.

#![cfg_attr(test, allow(dead_code))]

use crate::capabilities::{
    CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask,
};
use crate::service_identity::{ServiceId, ServiceIdentityError, ServiceIdentityTable};
use crate::tasks::TaskId;
use crate::value_validation::UntrustedRuntimeValue;

pub const RUNTIME_TASK_ID: TaskId = TaskId::new(40);
const RUNTIME_BOOT_RESOURCE: ResourceId = ResourceId::new(0x5059_5448_5255_4E54);
const HELLO_SERVICE_SOURCE: &str = "class HelloService(Service):\n    async def start(self):\n        system.log(\"PythOS [HISS] We Are Woken\")\n        self.ready()\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterpreterError {
    UnsupportedSource,
    ServiceIdentity,
    BootCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeOperation<'a> {
    SystemLog(UntrustedRuntimeValue<'a>),
    Ready,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeProgram<'a> {
    pub service_name: &'a str,
    pub entrypoint: &'a str,
    pub operations: [RuntimeOperation<'a>; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeInstance<'a> {
    pub task_id: TaskId,
    pub service_id: ServiceId,
    pub program: RuntimeProgram<'a>,
}

pub fn boot(source: &str) -> Result<RuntimeInstance<'_>, InterpreterError> {
    let mut identities = ServiceIdentityTable::new();
    let service_id = identities
        .register_task(RUNTIME_TASK_ID)
        .map_err(map_identity_error)?;
    let mut capabilities = CapabilityTable::new();
    let handle = capabilities
        .grant(
            service_id,
            RUNTIME_BOOT_RESOURCE,
            RightsMask::new(RightsMask::READ),
        )
        .map_err(map_capability_error)?;
    boot_authorized(source, service_id, &capabilities, handle)
}

fn boot_authorized<'a>(
    source: &'a str,
    service_id: ServiceId,
    capabilities: &CapabilityTable,
    handle: CapabilityHandle,
) -> Result<RuntimeInstance<'a>, InterpreterError> {
    capabilities
        .validate(
            service_id,
            handle,
            RUNTIME_BOOT_RESOURCE,
            RightsMask::new(RightsMask::READ),
        )
        .map_err(map_capability_error)?;
    let program = parse(source)?;
    Ok(RuntimeInstance {
        task_id: RUNTIME_TASK_ID,
        service_id,
        program,
    })
}

fn parse(source: &str) -> Result<RuntimeProgram<'_>, InterpreterError> {
    if source != HELLO_SERVICE_SOURCE {
        return Err(InterpreterError::UnsupportedSource);
    }
    Ok(RuntimeProgram {
        service_name: "HelloService",
        entrypoint: "start",
        operations: [
            RuntimeOperation::SystemLog(UntrustedRuntimeValue::StringBytes(
                b"PythOS [HISS] We Are Woken",
            )),
            RuntimeOperation::Ready,
        ],
    })
}

fn map_identity_error(_: ServiceIdentityError) -> InterpreterError {
    InterpreterError::ServiceIdentity
}

fn map_capability_error(_: CapabilityError) -> InterpreterError {
    InterpreterError::BootCapability
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_recognizes_hello_service_as_fixed_instruction_plan() {
        let instance = boot(HELLO_SERVICE_SOURCE).unwrap();

        assert_eq!(instance.task_id, RUNTIME_TASK_ID);
        assert_eq!(instance.program.service_name, "HelloService");
        assert_eq!(instance.program.entrypoint, "start");
        assert_eq!(
            instance.program.operations,
            [
                RuntimeOperation::SystemLog(UntrustedRuntimeValue::StringBytes(
                    b"PythOS [HISS] We Are Woken"
                )),
                RuntimeOperation::Ready,
            ]
        );
    }

    #[test]
    fn unsupported_source_is_rejected_before_runtime_start() {
        assert_eq!(
            boot("class Other(Service):\n    pass\n"),
            Err(InterpreterError::UnsupportedSource)
        );
    }

    #[test]
    fn runtime_boot_requires_explicit_capability_for_service() {
        let mut identities = ServiceIdentityTable::new();
        let runtime_service = identities.register_task(RUNTIME_TASK_ID).unwrap();
        let wrong_holder = identities.register_task(TaskId::new(41)).unwrap();
        let mut capabilities = CapabilityTable::new();
        let wrong_handle = capabilities
            .grant(
                wrong_holder,
                RUNTIME_BOOT_RESOURCE,
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();

        assert_eq!(
            boot_authorized(
                HELLO_SERVICE_SOURCE,
                runtime_service,
                &capabilities,
                wrong_handle
            ),
            Err(InterpreterError::BootCapability)
        );
    }
}
