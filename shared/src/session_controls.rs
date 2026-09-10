use crate::input_types::{InputEvent, InputEventKind, KeyCode};

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
                self.reset();
                return Some(SessionControlCommand::ActivateCursorFeature);
            }
        } else {
            self.activation_index = u8::from(key == KeyCode::Space);
        }
        None
    }

    pub fn reset(&mut self) {
        self.activation_index = 0;
    }
}

impl Default for SessionControlInterpreter {
    fn default() -> Self {
        Self::new()
    }
}
