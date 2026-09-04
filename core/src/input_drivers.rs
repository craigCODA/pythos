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
    #[cfg(any(test, feature = "usb-xhci-boot-mouse-decode-probe"))]
    BadUsbBootMouseReport,
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

/// The bounded, protocol-level meaning of one USB HID boot-mouse report.
///
/// Only the three standard boot-protocol bytes are assigned semantics here.
/// A fourth byte is retained as auxiliary evidence because ADR 0086 proved
/// four-byte transfers on the target mouse without proving that byte is a
/// wheel report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(test, feature = "usb-xhci-boot-mouse-decode-probe"))]
pub struct UsbBootMouseReport {
    pub buttons: u8,
    pub dx: i8,
    pub dy: i8,
    pub auxiliary: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(test, feature = "usb-xhci-boot-mouse-decode-probe"))]
pub struct UsbBootMouseSequenceSummary {
    pub report_count: u8,
    pub dx_total: i32,
    pub dy_total: i32,
    pub last_report: Option<UsbBootMouseReport>,
    pub pressed_seen: u8,
    pub released_after_pressed: u8,
    pub auxiliary_seen: bool,
    pub latest_auxiliary: Option<u8>,
}

#[cfg(any(test, feature = "usb-xhci-boot-mouse-decode-probe"))]
impl UsbBootMouseSequenceSummary {
    pub const fn new() -> Self {
        Self {
            report_count: 0,
            dx_total: 0,
            dy_total: 0,
            last_report: None,
            pressed_seen: 0,
            released_after_pressed: 0,
            auxiliary_seen: false,
            latest_auxiliary: None,
        }
    }

    pub fn observe(&mut self, report: UsbBootMouseReport) {
        let prior_buttons = self.last_report.map_or(0, |prior| prior.buttons);
        self.released_after_pressed |= prior_buttons & !report.buttons & 0x07;
        self.pressed_seen |= report.buttons & 0x07;
        self.dx_total += i32::from(report.dx);
        self.dy_total += i32::from(report.dy);
        if let Some(auxiliary) = report.auxiliary {
            self.auxiliary_seen = true;
            self.latest_auxiliary = Some(auxiliary);
        }
        self.last_report = Some(report);
        self.report_count += 1;
    }
}

#[cfg(any(test, feature = "usb-xhci-boot-mouse-decode-probe"))]
impl UsbBootMouseReport {
    pub const fn left_pressed(self) -> bool {
        self.buttons & 0x01 != 0
    }

    pub const fn right_pressed(self) -> bool {
        self.buttons & 0x02 != 0
    }

    pub const fn middle_pressed(self) -> bool {
        self.buttons & 0x04 != 0
    }

    pub const fn movement_event(self) -> RawInputEvent {
        RawInputEvent::MouseMoved {
            dx: self.dx,
            dy: self.dy,
        }
    }
}

/// Decode exactly one three- or four-byte USB HID boot-mouse report.
///
/// This deliberately does not use the PS/2 packet validator: USB boot reports
/// do not carry the PS/2 byte-zero "always 1" synchronization bit.
#[cfg(any(test, feature = "usb-xhci-boot-mouse-decode-probe"))]
pub fn decode_usb_boot_mouse_report(report: &[u8]) -> Result<UsbBootMouseReport, InputDriverError> {
    if report.len() != 3 && report.len() != 4 {
        return Err(InputDriverError::BadUsbBootMouseReport);
    }

    Ok(UsbBootMouseReport {
        buttons: report[0] & 0x07,
        dx: report[1] as i8,
        dy: report[2] as i8,
        auxiliary: if report.len() == 4 {
            Some(report[3])
        } else {
            None
        },
    })
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

    #[test]
    fn usb_boot_mouse_decodes_the_physically_captured_report() {
        let decoded = decode_usb_boot_mouse_report(&[0x00, 0xFE, 0x00, 0x00]).unwrap();

        assert_eq!(decoded.buttons, 0);
        assert_eq!(decoded.dx, -2);
        assert_eq!(decoded.dy, 0);
        assert_eq!(decoded.auxiliary, Some(0));
        assert_eq!(
            decoded.movement_event(),
            RawInputEvent::MouseMoved { dx: -2, dy: 0 }
        );
    }

    #[test]
    fn usb_boot_mouse_decodes_buttons_signed_axes_and_optional_auxiliary_byte() {
        let decoded = decode_usb_boot_mouse_report(&[0x05, 0x7F, 0x80]).unwrap();

        assert_eq!(decoded.buttons, 0x05);
        assert!(decoded.left_pressed());
        assert!(!decoded.right_pressed());
        assert!(decoded.middle_pressed());
        assert_eq!(decoded.dx, 127);
        assert_eq!(decoded.dy, -128);
        assert_eq!(decoded.auxiliary, None);
    }

    #[test]
    fn usb_boot_mouse_rejects_reports_outside_the_bounded_three_or_four_bytes() {
        assert_eq!(
            decode_usb_boot_mouse_report(&[0x00, 0x01]),
            Err(InputDriverError::BadUsbBootMouseReport)
        );
        assert_eq!(
            decode_usb_boot_mouse_report(&[0x00, 0x01, 0x02, 0x03, 0x04]),
            Err(InputDriverError::BadUsbBootMouseReport)
        );
    }

    #[test]
    fn usb_boot_mouse_sequence_accumulates_signed_motion_and_latest_report() {
        let mut summary = UsbBootMouseSequenceSummary::new();
        summary.observe(UsbBootMouseReport {
            buttons: 0,
            dx: 8,
            dy: -4,
            auxiliary: Some(0),
        });
        summary.observe(UsbBootMouseReport {
            buttons: 0,
            dx: -7,
            dy: -7,
            auxiliary: None,
        });

        assert_eq!(summary.report_count, 2);
        assert_eq!(summary.dx_total, 1);
        assert_eq!(summary.dy_total, -11);
        assert_eq!(summary.last_report.unwrap().dx, -7);
        assert!(summary.auxiliary_seen);
        assert_eq!(summary.latest_auxiliary, Some(0));
    }

    #[test]
    fn usb_boot_mouse_sequence_records_only_observed_press_then_release() {
        let mut summary = UsbBootMouseSequenceSummary::new();
        summary.observe(UsbBootMouseReport {
            buttons: 0,
            dx: 0,
            dy: 0,
            auxiliary: None,
        });
        assert_eq!(summary.released_after_pressed, 0);
        summary.observe(UsbBootMouseReport {
            buttons: 1,
            dx: 0,
            dy: 0,
            auxiliary: None,
        });
        summary.observe(UsbBootMouseReport {
            buttons: 0,
            dx: 0,
            dy: 0,
            auxiliary: None,
        });
        assert_eq!(summary.pressed_seen, 1);
        assert_eq!(summary.released_after_pressed, 1);
    }
}
