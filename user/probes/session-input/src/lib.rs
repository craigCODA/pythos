#![no_std]

use pythos_shared::session_input_abi::{
    KEY_BACKSPACE, KEY_SPACE, SESSION_INPUT_KIND_KEY_DOWN, SESSION_INPUT_KIND_RELATIVE_MOTION,
    SESSION_INPUT_SOURCE_KEYBOARD, SESSION_INPUT_SOURCE_MOUSE, SessionInputEventV1,
};

pub mod syscalls;

#[derive(Clone, Copy)]
enum ExpectedEvent {
    Key(u16),
    Motion(i32, i32),
}

impl ExpectedEvent {
    const fn key(key: u16) -> Self {
        Self::Key(key)
    }

    const fn motion(dx: i32, dy: i32) -> Self {
        Self::Motion(dx, dy)
    }
}

const EXPECTED: [ExpectedEvent; 5] = [
    ExpectedEvent::key(KEY_SPACE),
    ExpectedEvent::key(KEY_SPACE),
    ExpectedEvent::key(KEY_BACKSPACE),
    ExpectedEvent::key(KEY_BACKSPACE),
    ExpectedEvent::motion(7, -7),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventValidationError {
    Complete,
    Reserved,
    Flags,
    Sequence,
    Shape,
}

pub struct EventSequenceValidator {
    next_ordinal: usize,
    next_sequence: Option<u64>,
}

impl EventSequenceValidator {
    pub const fn new() -> Self {
        Self {
            next_ordinal: 0,
            next_sequence: None,
        }
    }

    pub const fn is_complete(&self) -> bool {
        self.next_ordinal == EXPECTED.len()
    }

    pub fn accept(&mut self, event: SessionInputEventV1) -> Result<usize, EventValidationError> {
        if self.is_complete() {
            return Err(EventValidationError::Complete);
        }
        if event.flags != 0 {
            return Err(EventValidationError::Flags);
        }
        if event.reserved0 != 0 || event.reserved1 != 0 {
            return Err(EventValidationError::Reserved);
        }
        if let Some(next_sequence) = self.next_sequence
            && event.sequence != next_sequence
        {
            return Err(EventValidationError::Sequence);
        }
        if !matches_expected(event, EXPECTED[self.next_ordinal]) {
            return Err(EventValidationError::Shape);
        }

        self.next_sequence = Some(event.sequence.wrapping_add(1));
        self.next_ordinal += 1;
        Ok(self.next_ordinal)
    }
}

impl Default for EventSequenceValidator {
    fn default() -> Self {
        Self::new()
    }
}

fn matches_expected(event: SessionInputEventV1, expected: ExpectedEvent) -> bool {
    match expected {
        ExpectedEvent::Key(key) => {
            event.kind == SESSION_INPUT_KIND_KEY_DOWN
                && event.source == SESSION_INPUT_SOURCE_KEYBOARD
                && event.value0 == i32::from(key)
                && event.value1 == 0
        }
        ExpectedEvent::Motion(dx, dy) => {
            event.kind == SESSION_INPUT_KIND_RELATIVE_MOTION
                && event.source == SESSION_INPUT_SOURCE_MOUSE
                && event.value0 == dx
                && event.value1 == dy
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::session_input_abi::{
        SESSION_INPUT_FLAG_GAP_BEFORE, SESSION_INPUT_KIND_MOUSE_BUTTON_STATE,
    };

    fn event(sequence: u64, expected: ExpectedEvent) -> SessionInputEventV1 {
        match expected {
            ExpectedEvent::Key(key) => SessionInputEventV1 {
                sequence,
                kind: SESSION_INPUT_KIND_KEY_DOWN,
                source: SESSION_INPUT_SOURCE_KEYBOARD,
                flags: 0,
                value0: i32::from(key),
                value1: 0,
                reserved0: 0,
                reserved1: 0,
            },
            ExpectedEvent::Motion(dx, dy) => SessionInputEventV1 {
                sequence,
                kind: SESSION_INPUT_KIND_RELATIVE_MOTION,
                source: SESSION_INPUT_SOURCE_MOUSE,
                flags: 0,
                value0: dx,
                value1: dy,
                reserved0: 0,
                reserved1: 0,
            },
        }
    }

    #[test]
    fn accepts_only_the_exact_five_event_delivery() {
        let mut validator = EventSequenceValidator::new();
        for (ordinal, expected) in EXPECTED.into_iter().enumerate() {
            assert_eq!(
                validator.accept(event(41 + ordinal as u64, expected)),
                Ok(ordinal + 1)
            );
        }
    }

    #[test]
    fn rejects_event_shape_mutations() {
        let mutations = [
            {
                let mut value = event(41, EXPECTED[0]);
                value.kind = SESSION_INPUT_KIND_MOUSE_BUTTON_STATE;
                value
            },
            {
                let mut value = event(41, EXPECTED[0]);
                value.reserved1 = 1;
                value
            },
            {
                let mut value = event(41, EXPECTED[0]);
                value.flags = SESSION_INPUT_FLAG_GAP_BEFORE;
                value
            },
            {
                let mut value = event(41, EXPECTED[0]);
                value.flags = 0x8000_0000;
                value
            },
        ];

        for mutation in mutations {
            assert!(EventSequenceValidator::new().accept(mutation).is_err());
        }

        for reserved0 in [true, false] {
            let mut reserved = event(41, EXPECTED[0]);
            if reserved0 {
                reserved.reserved0 = 1;
            } else {
                reserved.reserved1 = 1;
            }
            assert_eq!(
                EventSequenceValidator::new().accept(reserved),
                Err(EventValidationError::Reserved)
            );
        }

        let mut order = EventSequenceValidator::new();
        assert_eq!(order.accept(event(41, EXPECTED[0])), Ok(1));
        assert_eq!(
            order.accept(event(42, EXPECTED[2])),
            Err(EventValidationError::Shape)
        );
        assert_eq!(order.accept(event(42, EXPECTED[1])), Ok(2));

        for motion in [ExpectedEvent::motion(0, 0), ExpectedEvent::motion(7, 7)] {
            let mut validator = EventSequenceValidator::new();
            for (ordinal, expected) in EXPECTED[..4].iter().copied().enumerate() {
                assert_eq!(
                    validator.accept(event(41 + ordinal as u64, expected)),
                    Ok(ordinal + 1)
                );
            }
            assert_eq!(
                validator.accept(event(45, motion)),
                Err(EventValidationError::Shape)
            );
            assert_eq!(validator.accept(event(45, EXPECTED[4])), Ok(5));
        }
    }

    #[test]
    fn rejects_non_contiguous_and_sixth_events() {
        let mut duplicate = EventSequenceValidator::new();
        assert_eq!(duplicate.accept(event(41, EXPECTED[0])), Ok(1));
        assert!(duplicate.accept(event(41, EXPECTED[1])).is_err());

        let mut skip = EventSequenceValidator::new();
        assert_eq!(skip.accept(event(41, EXPECTED[0])), Ok(1));
        assert!(skip.accept(event(43, EXPECTED[1])).is_err());

        let mut completed = EventSequenceValidator::new();
        for (ordinal, expected) in EXPECTED.into_iter().enumerate() {
            assert!(
                completed
                    .accept(event(41 + ordinal as u64, expected))
                    .is_ok()
            );
        }
        assert!(completed.accept(event(46, EXPECTED[0])).is_err());
    }

    #[test]
    fn accepts_wrapping_contiguous_sequence_numbers() {
        let mut validator = EventSequenceValidator::new();
        for (ordinal, expected) in EXPECTED.into_iter().enumerate() {
            assert_eq!(
                validator.accept(event(
                    u64::MAX.wrapping_sub(3).wrapping_add(ordinal as u64),
                    expected
                )),
                Ok(ordinal + 1)
            );
        }
    }
}
