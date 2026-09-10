//! Retained ring-3 owner for ADR 0089 Viewing policy.

use pythos_shared::{
    input_types::{InputEvent, InputEventKind, InputSource, KeyCode, RelativeMotion},
    session_controls::{SessionControlCommand, SessionControlInterpreter},
    session_input_abi::{
        KEY_A, KEY_BACKSPACE, KEY_DIGIT0, KEY_DIGIT1, KEY_DIGIT2, KEY_DIGIT3, KEY_DIGIT4,
        KEY_DIGIT5, KEY_DIGIT6, KEY_DIGIT7, KEY_DIGIT8, KEY_DIGIT9, KEY_ENTER, KEY_ESCAPE,
        KEY_SPACE, KEY_Z, SESSION_INPUT_FLAG_GAP_BEFORE, SESSION_INPUT_KIND_KEY_DOWN,
        SESSION_INPUT_KIND_MOUSE_BUTTON_STATE, SESSION_INPUT_KIND_RELATIVE_MOTION,
        SESSION_INPUT_SOURCE_KEYBOARD, SESSION_INPUT_SOURCE_MOUSE, SessionInputEventV1,
    },
    viewing::{MotionRoute, ViewingExtent, ViewingSnapshot, ViewingState},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionViewingContinuity {
    Initial,
    Continuous,
    GapBefore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionViewingMalformedEvent {
    Reserved,
    Flags,
    SourceKind,
    Shape,
    MotionRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionViewingError {
    Malformed(SessionViewingMalformedEvent),
    UnflaggedDiscontinuity { expected: u64, actual: u64 },
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionViewingReceipt {
    pub sequence: u64,
    pub continuity: SessionViewingContinuity,
    pub command: Option<SessionControlCommand>,
    pub motion_route: Option<MotionRoute>,
    pub snapshot: ViewingSnapshot,
}

pub struct SessionViewing {
    controls: SessionControlInterpreter,
    viewing: ViewingState,
    expected_sequence: Option<u64>,
    poisoned: bool,
}

impl SessionViewing {
    pub fn new(extent: ViewingExtent) -> Self {
        Self {
            controls: SessionControlInterpreter::new(),
            viewing: ViewingState::new(extent),
            expected_sequence: None,
            poisoned: false,
        }
    }

    pub fn observe(
        &mut self,
        event: SessionInputEventV1,
    ) -> Result<SessionViewingReceipt, SessionViewingError> {
        if self.poisoned {
            return Err(SessionViewingError::RecoveryRequired);
        }

        let decoded = match decode(event) {
            Ok(decoded) => decoded,
            Err(error) => return Err(self.poison(error)),
        };
        let continuity = match self.continuity(event.sequence, decoded.gap_before) {
            Ok(continuity) => continuity,
            Err(error) => return Err(error),
        };

        if decoded.gap_before {
            self.controls.reset();
        }
        self.expected_sequence = Some(event.sequence.wrapping_add(1));

        let command = self.controls.observe(decoded.input);
        if let Some(command) = command {
            self.viewing.apply_session_command(command);
        }
        let motion_route = match decoded.input.kind {
            InputEventKind::RelativeMotion(motion) => {
                Some(self.viewing.route_relative_motion(motion))
            }
            InputEventKind::KeyDown(_) | InputEventKind::PointerButton { .. } => None,
        };

        Ok(SessionViewingReceipt {
            sequence: event.sequence,
            continuity,
            command,
            motion_route,
            snapshot: self.viewing.snapshot(),
        })
    }

    pub fn snapshot(&self) -> ViewingSnapshot {
        self.viewing.snapshot()
    }

    pub const fn traversal_intent_count(&self) -> u32 {
        self.viewing.traversal().motion_count()
    }

    fn continuity(
        &mut self,
        sequence: u64,
        gap_before: bool,
    ) -> Result<SessionViewingContinuity, SessionViewingError> {
        match self.expected_sequence {
            None if sequence == 0 && !gap_before => Ok(SessionViewingContinuity::Initial),
            None if gap_before => Ok(SessionViewingContinuity::GapBefore),
            None => Err(self.poison(SessionViewingError::UnflaggedDiscontinuity {
                expected: 0,
                actual: sequence,
            })),
            Some(expected) if sequence == expected && !gap_before => {
                Ok(SessionViewingContinuity::Continuous)
            }
            Some(_) if gap_before => Ok(SessionViewingContinuity::GapBefore),
            Some(expected) => Err(self.poison(SessionViewingError::UnflaggedDiscontinuity {
                expected,
                actual: sequence,
            })),
        }
    }

    fn poison(&mut self, error: SessionViewingError) -> SessionViewingError {
        self.controls.reset();
        self.poisoned = true;
        error
    }
}

#[derive(Clone, Copy)]
struct DecodedSessionInput {
    input: InputEvent,
    gap_before: bool,
}

fn decode(event: SessionInputEventV1) -> Result<DecodedSessionInput, SessionViewingError> {
    if event.reserved0 != 0 || event.reserved1 != 0 {
        return Err(SessionViewingError::Malformed(
            SessionViewingMalformedEvent::Reserved,
        ));
    }
    if event.flags & !SESSION_INPUT_FLAG_GAP_BEFORE != 0 {
        return Err(SessionViewingError::Malformed(
            SessionViewingMalformedEvent::Flags,
        ));
    }
    let gap_before = event.flags == SESSION_INPUT_FLAG_GAP_BEFORE;

    let input = match (event.source, event.kind) {
        (SESSION_INPUT_SOURCE_KEYBOARD, SESSION_INPUT_KIND_KEY_DOWN) => {
            if event.value1 != 0 {
                return Err(SessionViewingError::Malformed(
                    SessionViewingMalformedEvent::Shape,
                ));
            }
            let key = key_code(event.value0).ok_or(SessionViewingError::Malformed(
                SessionViewingMalformedEvent::Shape,
            ))?;
            InputEvent {
                source: InputSource::Keyboard,
                kind: InputEventKind::KeyDown(key),
            }
        }
        (SESSION_INPUT_SOURCE_MOUSE, SESSION_INPUT_KIND_RELATIVE_MOTION) => {
            let dx = i8::try_from(event.value0).map_err(|_| {
                SessionViewingError::Malformed(SessionViewingMalformedEvent::MotionRange)
            })?;
            let dy = i8::try_from(event.value1).map_err(|_| {
                SessionViewingError::Malformed(SessionViewingMalformedEvent::MotionRange)
            })?;
            InputEvent {
                source: InputSource::Mouse,
                kind: InputEventKind::RelativeMotion(RelativeMotion { dx, dy }),
            }
        }
        (SESSION_INPUT_SOURCE_MOUSE, SESSION_INPUT_KIND_MOUSE_BUTTON_STATE) => {
            let left = match (event.value0, event.value1) {
                (0, 0) => false,
                (1, 0) => true,
                _ => {
                    return Err(SessionViewingError::Malformed(
                        SessionViewingMalformedEvent::Shape,
                    ));
                }
            };
            InputEvent {
                source: InputSource::Mouse,
                kind: InputEventKind::PointerButton { left },
            }
        }
        _ => {
            return Err(SessionViewingError::Malformed(
                SessionViewingMalformedEvent::SourceKind,
            ));
        }
    };

    Ok(DecodedSessionInput { input, gap_before })
}

fn key_code(value: i32) -> Option<KeyCode> {
    match u16::try_from(value).ok()? {
        KEY_A => Some(KeyCode::A),
        0x0002 => Some(KeyCode::B),
        0x0003 => Some(KeyCode::C),
        0x0004 => Some(KeyCode::D),
        0x0005 => Some(KeyCode::E),
        0x0006 => Some(KeyCode::F),
        0x0007 => Some(KeyCode::G),
        0x0008 => Some(KeyCode::H),
        0x0009 => Some(KeyCode::I),
        0x000A => Some(KeyCode::J),
        0x000B => Some(KeyCode::K),
        0x000C => Some(KeyCode::L),
        0x000D => Some(KeyCode::M),
        0x000E => Some(KeyCode::N),
        0x000F => Some(KeyCode::O),
        0x0010 => Some(KeyCode::P),
        0x0011 => Some(KeyCode::Q),
        0x0012 => Some(KeyCode::R),
        0x0013 => Some(KeyCode::S),
        0x0014 => Some(KeyCode::T),
        0x0015 => Some(KeyCode::U),
        0x0016 => Some(KeyCode::V),
        0x0017 => Some(KeyCode::W),
        0x0018 => Some(KeyCode::X),
        0x0019 => Some(KeyCode::Y),
        KEY_Z => Some(KeyCode::Z),
        KEY_DIGIT0 => Some(KeyCode::Digit0),
        KEY_DIGIT1 => Some(KeyCode::Digit1),
        KEY_DIGIT2 => Some(KeyCode::Digit2),
        KEY_DIGIT3 => Some(KeyCode::Digit3),
        KEY_DIGIT4 => Some(KeyCode::Digit4),
        KEY_DIGIT5 => Some(KeyCode::Digit5),
        KEY_DIGIT6 => Some(KeyCode::Digit6),
        KEY_DIGIT7 => Some(KeyCode::Digit7),
        KEY_DIGIT8 => Some(KeyCode::Digit8),
        KEY_DIGIT9 => Some(KeyCode::Digit9),
        KEY_ENTER => Some(KeyCode::Enter),
        KEY_ESCAPE => Some(KeyCode::Escape),
        KEY_SPACE => Some(KeyCode::Space),
        KEY_BACKSPACE => Some(KeyCode::Backspace),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::{
        session_input_abi::{
            KEY_BACKSPACE, KEY_SPACE, SESSION_INPUT_KIND_KEY_DOWN,
            SESSION_INPUT_KIND_MOUSE_BUTTON_STATE, SESSION_INPUT_KIND_RELATIVE_MOTION,
            SESSION_INPUT_SOURCE_KEYBOARD, SESSION_INPUT_SOURCE_MOUSE,
        },
        viewing::{FocusMarkPosition, TraversalIntent},
    };

    fn key(sequence: u64, value0: u16) -> SessionInputEventV1 {
        SessionInputEventV1 {
            sequence,
            kind: SESSION_INPUT_KIND_KEY_DOWN,
            source: SESSION_INPUT_SOURCE_KEYBOARD,
            flags: 0,
            value0: i32::from(value0),
            value1: 0,
            reserved0: 0,
            reserved1: 0,
        }
    }

    fn motion(sequence: u64, dx: i32, dy: i32) -> SessionInputEventV1 {
        SessionInputEventV1 {
            sequence,
            kind: SESSION_INPUT_KIND_RELATIVE_MOTION,
            source: SESSION_INPUT_SOURCE_MOUSE,
            flags: 0,
            value0: dx,
            value1: dy,
            reserved0: 0,
            reserved1: 0,
        }
    }

    #[test]
    fn retained_owner_activates_once_and_routes_only_later_motion_to_focus() {
        // Catches graph-local or duplicate control ownership moving Traversal after activation.
        let mut viewing = SessionViewing::new(ViewingExtent::new(100, 80).unwrap());

        assert_eq!(
            viewing.observe(motion(0, 7, -3)).unwrap().motion_route,
            Some(MotionRoute::Traversal(TraversalIntent {
                motion: RelativeMotion { dx: 7, dy: -3 },
            }))
        );
        for (sequence, value0) in [(1, KEY_SPACE), (2, KEY_SPACE), (3, KEY_BACKSPACE)] {
            assert_eq!(
                viewing.observe(key(sequence, value0)).unwrap().command,
                None
            );
        }
        assert_eq!(
            viewing.observe(key(4, KEY_BACKSPACE)).unwrap().command,
            Some(SessionControlCommand::ActivateCursorFeature)
        );
        let moved = viewing.observe(motion(5, 7, -3)).unwrap();
        assert_eq!(
            moved.motion_route,
            Some(MotionRoute::CursorFocus(FocusMarkPosition { x: 57, y: 37 }))
        );
        assert_eq!(viewing.traversal_intent_count(), 1);

        for (sequence, value0) in [(6, KEY_SPACE), (7, KEY_SPACE), (8, KEY_BACKSPACE)] {
            viewing.observe(key(sequence, value0)).unwrap();
        }
        let repeated = viewing.observe(key(9, KEY_BACKSPACE)).unwrap();
        assert_eq!(
            repeated.command,
            Some(SessionControlCommand::ActivateCursorFeature)
        );
        assert_eq!(
            repeated.snapshot.focus_mark,
            Some(FocusMarkPosition { x: 57, y: 37 })
        );
    }

    #[test]
    fn flagged_gap_resets_partial_control_only_and_preserves_active_session_state() {
        // Catches a loss report carrying a stale activation prefix across a queue gap.
        let mut viewing = SessionViewing::new(ViewingExtent::new(4, 3).unwrap());
        viewing.observe(key(0, KEY_SPACE)).unwrap();
        let mut gap = key(9, KEY_BACKSPACE);
        gap.flags = SESSION_INPUT_FLAG_GAP_BEFORE;
        let receipt = viewing.observe(gap).unwrap();
        assert_eq!(receipt.continuity, SessionViewingContinuity::GapBefore);
        assert_eq!(receipt.command, None);

        for (sequence, value0) in [(10, KEY_SPACE), (11, KEY_SPACE), (12, KEY_BACKSPACE)] {
            viewing.observe(key(sequence, value0)).unwrap();
        }
        viewing.observe(key(13, KEY_BACKSPACE)).unwrap();
        viewing.observe(motion(14, 127, 127)).unwrap();
        assert_eq!(
            viewing.snapshot().focus_mark,
            Some(FocusMarkPosition { x: 3, y: 2 })
        );

        let mut active_gap = motion(22, -128, -128);
        active_gap.flags = SESSION_INPUT_FLAG_GAP_BEFORE;
        let moved = viewing.observe(active_gap).unwrap();
        assert_eq!(moved.continuity, SessionViewingContinuity::GapBefore);
        assert_eq!(
            moved.snapshot.focus_mark,
            Some(FocusMarkPosition { x: 0, y: 0 })
        );
        assert_eq!(viewing.traversal_intent_count(), 0);
    }

    #[test]
    fn malformed_or_unflagged_discontinuous_input_requires_recovery_before_any_later_dispatch() {
        // Catches accepting later input after malformed delivery can have bridged a gesture.
        let mut viewing = SessionViewing::new(ViewingExtent::new(10, 10).unwrap());
        viewing.observe(key(0, KEY_SPACE)).unwrap();
        let mut malformed = key(1, KEY_SPACE);
        malformed.reserved0 = 1;
        assert_eq!(
            viewing.observe(malformed),
            Err(SessionViewingError::Malformed(
                SessionViewingMalformedEvent::Reserved
            ))
        );
        assert_eq!(
            viewing.observe(key(2, KEY_SPACE)),
            Err(SessionViewingError::RecoveryRequired)
        );

        let mut discontinuous = SessionViewing::new(ViewingExtent::new(10, 10).unwrap());
        discontinuous.observe(key(0, KEY_SPACE)).unwrap();
        assert_eq!(
            discontinuous.observe(key(2, KEY_SPACE)),
            Err(SessionViewingError::UnflaggedDiscontinuity {
                expected: 1,
                actual: 2
            })
        );
        assert_eq!(
            discontinuous.observe(key(3, KEY_SPACE)),
            Err(SessionViewingError::RecoveryRequired)
        );
    }

    #[test]
    fn decoder_checks_tag_shapes_ranges_and_wrapping_continuity_without_button_semantics() {
        // Catches silently truncating wire motion or granting mouse buttons a Viewing action.
        let malformed = [
            SessionInputEventV1 {
                source: 9,
                ..key(0, KEY_SPACE)
            },
            SessionInputEventV1 {
                kind: 9,
                ..key(0, KEY_SPACE)
            },
            SessionInputEventV1 {
                value1: 1,
                ..key(0, KEY_SPACE)
            },
            SessionInputEventV1 {
                value0: 128,
                ..motion(0, 0, 0)
            },
            SessionInputEventV1 {
                flags: 2,
                ..key(0, KEY_SPACE)
            },
            SessionInputEventV1 {
                reserved1: 1,
                ..key(0, KEY_SPACE)
            },
        ];
        for event in malformed {
            let mut viewing = SessionViewing::new(ViewingExtent::new(10, 10).unwrap());
            assert!(matches!(
                viewing.observe(event),
                Err(SessionViewingError::Malformed(_))
            ));
            assert_eq!(
                viewing.observe(key(1, KEY_SPACE)),
                Err(SessionViewingError::RecoveryRequired)
            );
        }

        let mut wrapping = SessionViewing::new(ViewingExtent::new(10, 10).unwrap());
        let mut first = motion(u64::MAX, 1, 1);
        first.flags = SESSION_INPUT_FLAG_GAP_BEFORE;
        assert_eq!(
            wrapping.observe(first).unwrap().continuity,
            SessionViewingContinuity::GapBefore
        );
        let button = SessionInputEventV1 {
            sequence: 0,
            kind: SESSION_INPUT_KIND_MOUSE_BUTTON_STATE,
            source: SESSION_INPUT_SOURCE_MOUSE,
            flags: 0,
            value0: 1,
            value1: 0,
            reserved0: 0,
            reserved1: 0,
        };
        let receipt = wrapping.observe(button).unwrap();
        assert_eq!(receipt.continuity, SessionViewingContinuity::Continuous);
        assert_eq!(receipt.command, None);
        assert_eq!(receipt.motion_route, None);
        assert_eq!(wrapping.traversal_intent_count(), 1);
    }
}
