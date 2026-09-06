use crate::{
    input_drivers::KeyCode,
    input_events::{InputEvent, InputEventKind},
};

const CURSOR_ACTIVATION_KEYS: [KeyCode; 4] = [
    KeyCode::Space,
    KeyCode::Space,
    KeyCode::Backspace,
    KeyCode::Backspace,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionControlCommand {
    ActivateCursorFeature,
}

pub struct SessionControlInterpreter {
    activation_index: u8,
}

impl SessionControlInterpreter {
    pub const fn new() -> Self {
        Self {
            activation_index: 0,
        }
    }

    pub fn observe(&mut self, event: InputEvent) -> Option<SessionControlCommand> {
        let InputEventKind::KeyDown(key) = event.kind else {
            return None;
        };

        if key == CURSOR_ACTIVATION_KEYS[self.activation_index as usize] {
            self.activation_index += 1;
            if self.activation_index == CURSOR_ACTIVATION_KEYS.len() as u8 {
                self.activation_index = 0;
                return Some(SessionControlCommand::ActivateCursorFeature);
            }
        } else {
            self.activation_index = u8::from(key == KeyCode::Space);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        input_drivers::KeyCode,
        input_events::{InputEvent, InputEventKind, InputSource, RelativeMotion},
    };

    fn key_event(key: KeyCode) -> InputEvent {
        InputEvent {
            source: InputSource::Keyboard,
            kind: InputEventKind::KeyDown(key),
        }
    }

    #[test]
    fn exact_key_down_sequence_emits_global_cursor_activation() {
        let mut controls = SessionControlInterpreter::new();
        let keys = [
            KeyCode::Space,
            KeyCode::Space,
            KeyCode::Backspace,
            KeyCode::Backspace,
        ];
        for key in &keys[..3] {
            assert_eq!(controls.observe(key_event(*key)), None);
        }
        assert_eq!(
            controls.observe(key_event(keys[3])),
            Some(SessionControlCommand::ActivateCursorFeature)
        );
    }

    #[test]
    fn relative_motion_does_not_reset_activation_progress() {
        let mut controls = SessionControlInterpreter::new();
        assert_eq!(controls.observe(key_event(KeyCode::Space)), None);
        assert_eq!(
            controls.observe(InputEvent {
                source: InputSource::Mouse,
                kind: InputEventKind::RelativeMotion(RelativeMotion { dx: 1, dy: -1 }),
            }),
            None
        );
        assert_eq!(controls.observe(key_event(KeyCode::Space)), None);
        assert_eq!(controls.observe(key_event(KeyCode::Backspace)), None);
        assert_eq!(
            controls.observe(key_event(KeyCode::Backspace)),
            Some(SessionControlCommand::ActivateCursorFeature)
        );
    }

    #[test]
    fn unrelated_key_down_resets_but_leading_space_can_restart() {
        let mut controls = SessionControlInterpreter::new();
        controls.observe(key_event(KeyCode::Space));
        controls.observe(key_event(KeyCode::A));
        controls.observe(key_event(KeyCode::Space));
        controls.observe(key_event(KeyCode::Space));
        controls.observe(key_event(KeyCode::Backspace));
        assert_eq!(
            controls.observe(key_event(KeyCode::Backspace)),
            Some(SessionControlCommand::ActivateCursorFeature)
        );
    }

    #[test]
    fn repeated_sequences_emit_repeated_one_way_activation_commands() {
        let mut controls = SessionControlInterpreter::new();
        let keys = [
            KeyCode::Space,
            KeyCode::Space,
            KeyCode::Backspace,
            KeyCode::Backspace,
        ];

        for _ in 0..2 {
            for key in &keys[..3] {
                assert_eq!(controls.observe(key_event(*key)), None);
            }
            assert_eq!(
                controls.observe(key_event(keys[3])),
                Some(SessionControlCommand::ActivateCursorFeature)
            );
        }
    }
}
