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
    KeyPressed {
        scancode: u8,
        key: KeyCode,
    },
    MouseMoved {
        dx: i8,
        dy: i8,
    },
    /// A left mouse button state transition (ADR 0053). Only the button the
    /// interactive launcher screen needs; right/middle are not tracked.
    MouseButton {
        left: bool,
    },
}

/// Scancode-set-1 make codes for the keys the interactive launcher screen
/// (ADR 0053) and any future text entry need: A-Z, 0-9, Enter, Escape, Space,
/// Backspace. Not a complete keyboard layout — extend as further keys are
/// needed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyCode {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Enter,
    Escape,
    Space,
    Backspace,
}

/// Decode a scancode-set-1 make code to a [`KeyCode`], independent of any
/// capability check. `KeyboardDriver::decode` wraps this with the
/// capability-gated shape; `ps2::handle_keyboard_interrupt` (the real IRQ1
/// top half, ADR 0053) uses it directly since interrupt context has no
/// caller-identity capability to check against.
pub(crate) fn scancode_to_keycode(scancode: u8) -> Option<KeyCode> {
    match scancode {
        0x1E => Some(KeyCode::A),
        0x30 => Some(KeyCode::B),
        0x2E => Some(KeyCode::C),
        0x20 => Some(KeyCode::D),
        0x12 => Some(KeyCode::E),
        0x21 => Some(KeyCode::F),
        0x22 => Some(KeyCode::G),
        0x23 => Some(KeyCode::H),
        0x17 => Some(KeyCode::I),
        0x24 => Some(KeyCode::J),
        0x25 => Some(KeyCode::K),
        0x26 => Some(KeyCode::L),
        0x32 => Some(KeyCode::M),
        0x31 => Some(KeyCode::N),
        0x18 => Some(KeyCode::O),
        0x19 => Some(KeyCode::P),
        0x10 => Some(KeyCode::Q),
        0x13 => Some(KeyCode::R),
        0x1F => Some(KeyCode::S),
        0x14 => Some(KeyCode::T),
        0x16 => Some(KeyCode::U),
        0x2F => Some(KeyCode::V),
        0x11 => Some(KeyCode::W),
        0x2D => Some(KeyCode::X),
        0x15 => Some(KeyCode::Y),
        0x2C => Some(KeyCode::Z),
        0x0B => Some(KeyCode::Digit0),
        0x02 => Some(KeyCode::Digit1),
        0x03 => Some(KeyCode::Digit2),
        0x04 => Some(KeyCode::Digit3),
        0x05 => Some(KeyCode::Digit4),
        0x06 => Some(KeyCode::Digit5),
        0x07 => Some(KeyCode::Digit6),
        0x08 => Some(KeyCode::Digit7),
        0x09 => Some(KeyCode::Digit8),
        0x0A => Some(KeyCode::Digit9),
        0x1C => Some(KeyCode::Enter),
        0x01 => Some(KeyCode::Escape),
        0x39 => Some(KeyCode::Space),
        0x0E => Some(KeyCode::Backspace),
        _ => None,
    }
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
        match scancode_to_keycode(scancode) {
            Some(key) => Ok(RawInputEvent::KeyPressed { scancode, key }),
            None => Err(InputDriverError::UnknownScancode),
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
        if !mouse_byte0_is_valid(packet[0]) {
            return Err(InputDriverError::BadMousePacket);
        }
        Ok(RawInputEvent::MouseMoved {
            dx: packet[1] as i8,
            dy: packet[2] as i8,
        })
    }
}

/// Validate a PS/2 mouse packet's byte 0 against the protocol's "always 1"
/// bit 3 convention. Shared by [`MouseDriver::decode`] and
/// `ps2::MouseAssembler` (the real IRQ12 bottom half, ADR 0053) so the check
/// lives in exactly one place.
pub(crate) fn mouse_byte0_is_valid(byte0: u8) -> bool {
    byte0 & 0x08 != 0
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
