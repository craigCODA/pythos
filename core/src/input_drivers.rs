//! Phase 5 native keyboard and mouse driver proof.
//!
//! This is deliberately a fixed early-shell proof: raw device bytes are
//! decoded only through explicit input capabilities. It does not expose direct
//! driver access to services.

#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::{
    capabilities::{CapabilityError, CapabilityTable, ResourceId, RightsMask},
    service_identity::{ServiceId, ServiceIdentityTable},
    tasks::TaskId,
};

const KEYBOARD_RESOURCE: ResourceId = ResourceId::new(0x1A50_0001);
const MOUSE_RESOURCE: ResourceId = ResourceId::new(0x1A50_0002);
const INPUT_RIGHT: RightsMask = RightsMask::new(RightsMask::INPUT);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputDriverError {
    Capability(CapabilityError),
    UnknownScancode,
    BadMousePacket,
}

impl From<CapabilityError> for InputDriverError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawInputEvent {
    KeyPressed { scancode: u8, key: KeyCode },
    MouseMoved { dx: i8, dy: i8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyCode {
    A,
}

pub struct KeyboardDriver {
    owner: ServiceId,
    capability: crate::capabilities::CapabilityHandle,
}

impl KeyboardDriver {
    pub fn new(table: &mut CapabilityTable, owner: ServiceId) -> Result<Self, InputDriverError> {
        let capability = table.grant(owner, KEYBOARD_RESOURCE, INPUT_RIGHT)?;
        Ok(Self { owner, capability })
    }

    pub fn decode(
        &self,
        table: &CapabilityTable,
        caller: ServiceId,
        scancode: u8,
    ) -> Result<RawInputEvent, InputDriverError> {
        table.validate(caller, self.capability, KEYBOARD_RESOURCE, INPUT_RIGHT)?;
        if caller != self.owner {
            return Err(InputDriverError::Capability(CapabilityError::WrongHolder));
        }
        match scancode {
            0x1E => Ok(RawInputEvent::KeyPressed {
                scancode,
                key: KeyCode::A,
            }),
            _ => Err(InputDriverError::UnknownScancode),
        }
    }
}

pub struct MouseDriver {
    owner: ServiceId,
    capability: crate::capabilities::CapabilityHandle,
}

impl MouseDriver {
    pub fn new(table: &mut CapabilityTable, owner: ServiceId) -> Result<Self, InputDriverError> {
        let capability = table.grant(owner, MOUSE_RESOURCE, INPUT_RIGHT)?;
        Ok(Self { owner, capability })
    }

    pub fn decode(
        &self,
        table: &CapabilityTable,
        caller: ServiceId,
        packet: [u8; 3],
    ) -> Result<RawInputEvent, InputDriverError> {
        table.validate(caller, self.capability, MOUSE_RESOURCE, INPUT_RIGHT)?;
        if caller != self.owner {
            return Err(InputDriverError::Capability(CapabilityError::WrongHolder));
        }
        if packet[0] & 0x08 == 0 {
            return Err(InputDriverError::BadMousePacket);
        }
        Ok(RawInputEvent::MouseMoved {
            dx: packet[1] as i8,
            dy: packet[2] as i8,
        })
    }
}

pub fn run_self_test() -> Result<(), InputDriverError> {
    let mut identities = ServiceIdentityTable::new();
    let input_service = identities
        .register_task(TaskId::new(80))
        .map_err(|_| InputDriverError::Capability(CapabilityError::InvalidHandle))?;
    let stranger = identities
        .register_task(TaskId::new(81))
        .map_err(|_| InputDriverError::Capability(CapabilityError::InvalidHandle))?;
    let mut capabilities = CapabilityTable::new();
    let keyboard = KeyboardDriver::new(&mut capabilities, input_service)?;
    let mouse = MouseDriver::new(&mut capabilities, input_service)?;

    if keyboard.decode(&capabilities, stranger, 0x1E)
        != Err(InputDriverError::Capability(CapabilityError::WrongHolder))
    {
        return Err(InputDriverError::Capability(CapabilityError::WrongHolder));
    }
    if keyboard.decode(&capabilities, input_service, 0x1E)?
        != (RawInputEvent::KeyPressed {
            scancode: 0x1E,
            key: KeyCode::A,
        })
    {
        return Err(InputDriverError::UnknownScancode);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:INPUT:KEYBOARD");

    if mouse.decode(&capabilities, input_service, [0x08, 5, 253])?
        != (RawInputEvent::MouseMoved { dx: 5, dy: -3 })
    {
        return Err(InputDriverError::BadMousePacket);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:INPUT:MOUSE");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identities() -> (ServiceId, ServiceId) {
        let mut table = ServiceIdentityTable::new();
        let input = table.register_task(TaskId::new(80)).unwrap();
        let stranger = table.register_task(TaskId::new(81)).unwrap();
        (input, stranger)
    }

    #[test]
    fn keyboard_decode_requires_input_capability() {
        let (input, stranger) = identities();
        let mut capabilities = CapabilityTable::new();
        let keyboard = KeyboardDriver::new(&mut capabilities, input).unwrap();

        assert_eq!(
            keyboard.decode(&capabilities, stranger, 0x1E),
            Err(InputDriverError::Capability(CapabilityError::WrongHolder))
        );
        assert_eq!(
            keyboard.decode(&capabilities, input, 0x1E),
            Ok(RawInputEvent::KeyPressed {
                scancode: 0x1E,
                key: KeyCode::A,
            })
        );
    }

    #[test]
    fn mouse_decode_requires_input_capability_and_valid_packet() {
        let (input, stranger) = identities();
        let mut capabilities = CapabilityTable::new();
        let mouse = MouseDriver::new(&mut capabilities, input).unwrap();

        assert_eq!(
            mouse.decode(&capabilities, stranger, [0x08, 1, 1]),
            Err(InputDriverError::Capability(CapabilityError::WrongHolder))
        );
        assert_eq!(
            mouse.decode(&capabilities, input, [0x00, 1, 1]),
            Err(InputDriverError::BadMousePacket)
        );
        assert_eq!(
            mouse.decode(&capabilities, input, [0x08, 5, 253]),
            Ok(RawInputEvent::MouseMoved { dx: 5, dy: -3 })
        );
    }
}
