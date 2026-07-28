//! Phase 5 native input-event service proof.
//!
//! The service owns normalization from raw driver events into typed events.
//! Other services subscribe through an explicit capability instead of reading
//! raw keyboard or mouse driver state directly.

#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::{
    capabilities::{CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask},
    input_drivers::{KeyCode, RawInputEvent},
    service_identity::{ServiceId, ServiceIdentityTable},
    tasks::TaskId,
};

const INPUT_EVENT_STREAM: ResourceId = ResourceId::new(0x1A50_0100);
const INPUT_RIGHT: RightsMask = RightsMask::new(RightsMask::INPUT);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputEventError {
    Capability(CapabilityError),
    BadNormalization,
}

impl From<CapabilityError> for InputEventError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputSource {
    Keyboard,
    Mouse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputEventKind {
    KeyDown(KeyCode),
    PointerDelta {
        dx: i8,
        dy: i8,
    },
    /// A left mouse button state transition (ADR 0053).
    PointerButton {
        left: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputEvent {
    pub source: InputSource,
    pub kind: InputEventKind,
}

pub struct InputEventService {
    subscriber: ServiceId,
    stream: CapabilityHandle,
}

impl InputEventService {
    pub fn new(
        table: &mut CapabilityTable,
        subscriber: ServiceId,
    ) -> Result<Self, InputEventError> {
        let stream = table.grant(subscriber, INPUT_EVENT_STREAM, INPUT_RIGHT)?;
        Ok(Self { subscriber, stream })
    }

    pub fn publish(
        &self,
        table: &CapabilityTable,
        subscriber: ServiceId,
        raw: RawInputEvent,
    ) -> Result<InputEvent, InputEventError> {
        table.validate(subscriber, self.stream, INPUT_EVENT_STREAM, INPUT_RIGHT)?;
        if subscriber != self.subscriber {
            return Err(InputEventError::Capability(CapabilityError::WrongHolder));
        }
        normalize(raw)
    }
}

pub fn normalize(raw: RawInputEvent) -> Result<InputEvent, InputEventError> {
    match raw {
        RawInputEvent::KeyPressed { key, .. } => Ok(InputEvent {
            source: InputSource::Keyboard,
            kind: InputEventKind::KeyDown(key),
        }),
        RawInputEvent::MouseMoved { dx, dy } => Ok(InputEvent {
            source: InputSource::Mouse,
            kind: InputEventKind::PointerDelta { dx, dy },
        }),
        RawInputEvent::MouseButton { left } => Ok(InputEvent {
            source: InputSource::Mouse,
            kind: InputEventKind::PointerButton { left },
        }),
    }
}

pub fn run_self_test() -> Result<(), InputEventError> {
    let mut identities = ServiceIdentityTable::new();
    let subscriber = identities
        .register_task(TaskId::new(82))
        .map_err(|_| InputEventError::Capability(CapabilityError::InvalidHandle))?;
    let stranger = identities
        .register_task(TaskId::new(83))
        .map_err(|_| InputEventError::Capability(CapabilityError::InvalidHandle))?;
    let mut capabilities = CapabilityTable::new();
    let service = InputEventService::new(&mut capabilities, subscriber)?;

    if service.publish(
        &capabilities,
        stranger,
        RawInputEvent::KeyPressed {
            scancode: 0x1E,
            key: KeyCode::A,
        },
    ) != Err(InputEventError::Capability(CapabilityError::WrongHolder))
    {
        return Err(InputEventError::Capability(CapabilityError::WrongHolder));
    }

    let key_event = service.publish(
        &capabilities,
        subscriber,
        RawInputEvent::KeyPressed {
            scancode: 0x1E,
            key: KeyCode::A,
        },
    )?;
    if key_event
        != (InputEvent {
            source: InputSource::Keyboard,
            kind: InputEventKind::KeyDown(KeyCode::A),
        })
    {
        return Err(InputEventError::BadNormalization);
    }

    let mouse_event = service.publish(
        &capabilities,
        subscriber,
        RawInputEvent::MouseMoved { dx: 5, dy: -3 },
    )?;
    if mouse_event
        != (InputEvent {
            source: InputSource::Mouse,
            kind: InputEventKind::PointerDelta { dx: 5, dy: -3 },
        })
    {
        return Err(InputEventError::BadNormalization);
    }

    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:INPUT:EVENT");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_driver_events_normalize_to_typed_input_events() {
        assert_eq!(
            normalize(RawInputEvent::KeyPressed {
                scancode: 0x1E,
                key: KeyCode::A,
            }),
            Ok(InputEvent {
                source: InputSource::Keyboard,
                kind: InputEventKind::KeyDown(KeyCode::A),
            })
        );
        assert_eq!(
            normalize(RawInputEvent::MouseMoved { dx: 5, dy: -3 }),
            Ok(InputEvent {
                source: InputSource::Mouse,
                kind: InputEventKind::PointerDelta { dx: 5, dy: -3 },
            })
        );
    }

    #[test]
    fn subscribers_need_input_stream_capability() {
        let mut identities = ServiceIdentityTable::new();
        let subscriber = identities.register_task(TaskId::new(82)).unwrap();
        let stranger = identities.register_task(TaskId::new(83)).unwrap();
        let mut capabilities = CapabilityTable::new();
        let service = InputEventService::new(&mut capabilities, subscriber).unwrap();

        assert_eq!(
            service.publish(
                &capabilities,
                stranger,
                RawInputEvent::MouseMoved { dx: 1, dy: 1 }
            ),
            Err(InputEventError::Capability(CapabilityError::WrongHolder))
        );
        assert_eq!(
            service.publish(
                &capabilities,
                subscriber,
                RawInputEvent::MouseMoved { dx: 1, dy: 1 }
            ),
            Ok(InputEvent {
                source: InputSource::Mouse,
                kind: InputEventKind::PointerDelta { dx: 1, dy: 1 },
            })
        );
    }
}
