//! Validation for values crossing the native/runtime boundary.

#![cfg_attr(test, allow(dead_code))]

use core::str;

use crate::interpreter::{RuntimeInstance, RuntimeOperation};

pub const MAX_SYSTEM_STRING_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueValidationError {
    UnsupportedType,
    EmptyString,
    StringTooLong,
    InvalidUtf8,
    RawPointerRejected,
    NativeStructRejected,
    MissingLogArgument,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UntrustedRuntimeValue<'a> {
    StringBytes(&'a [u8]),
    Integer(i64),
    RawPointerAddress(usize),
    NativeStructBytes(&'a [u8]),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeString<'a> {
    value: &'a str,
}

impl<'a> RuntimeString<'a> {
    pub const fn as_str(self) -> &'a str {
        self.value
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostCallResult {
    Returned,
    Rejected(ValueValidationError),
}

impl HostCallResult {
    pub const fn returned() -> Self {
        Self::Returned
    }

    pub const fn rejected(error: ValueValidationError) -> Self {
        Self::Rejected(error)
    }
}

pub fn validate_system_string(
    value: UntrustedRuntimeValue<'_>,
) -> Result<RuntimeString<'_>, ValueValidationError> {
    let bytes = match value {
        UntrustedRuntimeValue::StringBytes(bytes) => bytes,
        UntrustedRuntimeValue::Integer(_) => return Err(ValueValidationError::UnsupportedType),
        UntrustedRuntimeValue::RawPointerAddress(_) => {
            return Err(ValueValidationError::RawPointerRejected);
        }
        UntrustedRuntimeValue::NativeStructBytes(_) => {
            return Err(ValueValidationError::NativeStructRejected);
        }
    };
    if bytes.is_empty() {
        return Err(ValueValidationError::EmptyString);
    }
    if bytes.len() > MAX_SYSTEM_STRING_BYTES {
        return Err(ValueValidationError::StringTooLong);
    }
    let value = str::from_utf8(bytes).map_err(|_| ValueValidationError::InvalidUtf8)?;
    Ok(RuntimeString { value })
}

pub fn run_self_test(instance: &RuntimeInstance<'_>) -> Result<(), ValueValidationError> {
    let mut saw_log_argument = false;
    for operation in instance.program.operations {
        if let RuntimeOperation::SystemLog(value) = operation {
            saw_log_argument = true;
            let message = validate_system_string(value)?;
            if message.as_str() != "PythOS [HISS] We Are Woken" {
                return Err(ValueValidationError::UnsupportedType);
            }
        }
    }
    if !saw_log_argument {
        return Err(ValueValidationError::MissingLogArgument);
    }

    if validate_system_string(UntrustedRuntimeValue::Integer(1))
        != Err(ValueValidationError::UnsupportedType)
    {
        return Err(ValueValidationError::UnsupportedType);
    }
    if validate_system_string(UntrustedRuntimeValue::StringBytes(b""))
        != Err(ValueValidationError::EmptyString)
    {
        return Err(ValueValidationError::EmptyString);
    }
    let oversized = [b'x'; MAX_SYSTEM_STRING_BYTES + 1];
    if validate_system_string(UntrustedRuntimeValue::StringBytes(&oversized))
        != Err(ValueValidationError::StringTooLong)
    {
        return Err(ValueValidationError::StringTooLong);
    }
    let invalid_utf8 = [0xFF];
    if validate_system_string(UntrustedRuntimeValue::StringBytes(&invalid_utf8))
        != Err(ValueValidationError::InvalidUtf8)
    {
        return Err(ValueValidationError::InvalidUtf8);
    }
    if validate_system_string(UntrustedRuntimeValue::RawPointerAddress(0x1000))
        != Err(ValueValidationError::RawPointerRejected)
    {
        return Err(ValueValidationError::RawPointerRejected);
    }
    if validate_system_string(UntrustedRuntimeValue::NativeStructBytes(&[0, 1, 2, 3]))
        != Err(ValueValidationError::NativeStructRejected)
    {
        return Err(ValueValidationError::NativeStructRejected);
    }
    if HostCallResult::returned() != HostCallResult::Returned {
        return Err(ValueValidationError::UnsupportedType);
    }
    if HostCallResult::rejected(ValueValidationError::InvalidUtf8)
        != HostCallResult::Rejected(ValueValidationError::InvalidUtf8)
    {
        return Err(ValueValidationError::InvalidUtf8);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{RUNTIME_TASK_ID, RuntimeProgram};
    use crate::service_identity::{ServiceId, ServiceIdentityTable};

    fn service_id() -> ServiceId {
        let mut identities = ServiceIdentityTable::new();
        identities.register_task(RUNTIME_TASK_ID).unwrap()
    }

    #[test]
    fn validates_string_type_length_and_encoding() {
        assert_eq!(
            validate_system_string(UntrustedRuntimeValue::StringBytes(b"hello"))
                .unwrap()
                .as_str(),
            "hello"
        );
        assert_eq!(
            validate_system_string(UntrustedRuntimeValue::Integer(7)),
            Err(ValueValidationError::UnsupportedType)
        );
        assert_eq!(
            validate_system_string(UntrustedRuntimeValue::StringBytes(b"")),
            Err(ValueValidationError::EmptyString)
        );
        assert_eq!(
            validate_system_string(UntrustedRuntimeValue::StringBytes(&[0xFF])),
            Err(ValueValidationError::InvalidUtf8)
        );
        let oversized = [b'x'; MAX_SYSTEM_STRING_BYTES + 1];
        assert_eq!(
            validate_system_string(UntrustedRuntimeValue::StringBytes(&oversized)),
            Err(ValueValidationError::StringTooLong)
        );
    }

    #[test]
    fn rejects_raw_pointer_and_native_struct_shapes() {
        assert_eq!(
            validate_system_string(UntrustedRuntimeValue::RawPointerAddress(0x1000)),
            Err(ValueValidationError::RawPointerRejected)
        );
        assert_eq!(
            validate_system_string(UntrustedRuntimeValue::NativeStructBytes(&[1, 2, 3])),
            Err(ValueValidationError::NativeStructRejected)
        );
    }

    #[test]
    fn host_call_result_has_explicit_success_and_error_states() {
        assert_eq!(HostCallResult::returned(), HostCallResult::Returned);
        assert_eq!(
            HostCallResult::rejected(ValueValidationError::InvalidUtf8),
            HostCallResult::Rejected(ValueValidationError::InvalidUtf8)
        );
    }

    #[test]
    fn self_test_validates_runtime_program_values() {
        let instance = RuntimeInstance {
            task_id: RUNTIME_TASK_ID,
            service_id: service_id(),
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
}
