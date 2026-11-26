//! USB HID boot keyboard input for the native world.

use crate::input_drivers::{KeyCode, RawInputEvent};

/// Tracks the six non-modifier usages from the last valid boot report.
pub(crate) struct BootKeyboard {
    held: [u8; 6],
}

impl BootKeyboard {
    pub(crate) const fn new() -> Self {
        Self { held: [0; 6] }
    }

    /// Emits key-downs for newly pressed usages. A release removes the usage
    /// from the held set so a later press emits a fresh event.
    pub(crate) fn update(&mut self, report: &[u8], mut emit: impl FnMut(RawInputEvent)) {
        // The modifier and reserved bytes precede the six usage slots (HID 1.11, Appendix B.1).
        let Ok([_, _, usages @ ..]) = <&[u8; 8]>::try_from(report) else {
            return;
        };
        // ErrorRollOver, POSTFail, and ErrorUndefined invalidate the key snapshot.
        if usages.iter().any(|&usage| matches!(usage, 0x01..=0x03)) {
            return;
        }

        for (slot, &usage) in usages.iter().enumerate() {
            if self.held.contains(&usage) || usages[..slot].contains(&usage) {
                continue;
            }
            if let Some(key) = usage_to_key_code(usage) {
                emit(RawInputEvent::KeyPressed {
                    scancode: usage,
                    key,
                });
            }
        }
        self.held = *usages;
    }
}

fn usage_to_key_code(usage: u8) -> Option<KeyCode> {
    Some(match usage {
        0x04 => KeyCode::A,
        0x05 => KeyCode::B,
        0x06 => KeyCode::C,
        0x07 => KeyCode::D,
        0x08 => KeyCode::E,
        0x09 => KeyCode::F,
        0x0a => KeyCode::G,
        0x0b => KeyCode::H,
        0x10 => KeyCode::M,
        0x14 => KeyCode::Q,
        0x16 => KeyCode::S,
        0x17 => KeyCode::T,
        0x1a => KeyCode::W,
        0x1b => KeyCode::X,
        0x1e => KeyCode::Digit1,
        0x1f => KeyCode::Digit2,
        0x20 => KeyCode::Digit3,
        0x21 => KeyCode::Digit4,
        0x27 => KeyCode::Digit0,
        0x29 => KeyCode::Escape,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::BootKeyboard;
    use crate::input_drivers::{KeyCode, RawInputEvent};

    fn events(keyboard: &mut BootKeyboard, report: &[u8]) -> Vec<RawInputEvent> {
        let mut events = Vec::new();
        keyboard.update(report, |event| events.push(event));
        events
    }

    fn key_down(usage: u8, key: KeyCode) -> RawInputEvent {
        RawInputEvent::KeyPressed {
            scancode: usage,
            key,
        }
    }

    #[test]
    fn emits_one_key_down_per_press() {
        let mut keyboard = BootKeyboard::new();
        let pressed = [0, 0, 0x1a, 0, 0, 0, 0, 0];
        let w = key_down(0x1a, KeyCode::W);

        assert_eq!(events(&mut keyboard, &pressed), [w]);
        assert!(events(&mut keyboard, &pressed).is_empty());
        assert!(events(&mut keyboard, &[0; 8]).is_empty());
        assert_eq!(events(&mut keyboard, &pressed), [w]);
    }

    #[test]
    fn treats_the_six_key_slots_as_a_set() {
        let mut keyboard = BootKeyboard::new();
        let w = key_down(0x1a, KeyCode::W);
        let c = key_down(0x06, KeyCode::C);
        let f = key_down(0x09, KeyCode::F);

        assert_eq!(
            events(&mut keyboard, &[0, 0, 0x1a, 0x06, 0x1a, 0, 0, 0]),
            [w, c]
        );
        assert!(events(&mut keyboard, &[1, 0xff, 0x06, 0x1a, 0, 0, 0, 0]).is_empty());
        assert_eq!(
            events(&mut keyboard, &[2, 0xff, 0x06, 0x3a, 0, 0, 0, 0x09]),
            [f]
        );
        assert_eq!(
            events(&mut keyboard, &[0, 0, 0x06, 0x09, 0x1a, 0, 0, 0]),
            [w]
        );
    }

    #[test]
    fn invalid_reports_preserve_the_previous_keys() {
        let mut keyboard = BootKeyboard::new();
        let pressed = [0, 0, 0x10, 0, 0, 0, 0, 0];
        let m = key_down(0x10, KeyCode::M);
        let invalid: [&[u8]; 6] = [
            &[0, 0, 0x07, 0, 0, 0, 0],
            &[0, 0, 0x07, 0, 0, 0, 0, 0, 0],
            &[0, 0, 1, 1, 1, 1, 1, 1],
            &[0, 0, 0x07, 0, 0, 0, 0, 1],
            &[0, 0, 0x07, 0, 0, 0, 0, 2],
            &[0, 0, 0x07, 0, 0, 0, 0, 3],
        ];

        assert_eq!(events(&mut keyboard, &pressed), [m]);
        for report in invalid {
            assert!(events(&mut keyboard, report).is_empty(), "{report:?}");
            assert!(events(&mut keyboard, &pressed).is_empty(), "{report:?}");
        }
        assert!(events(&mut keyboard, &[0; 8]).is_empty());
        assert_eq!(events(&mut keyboard, &pressed), [m]);
    }

    #[test]
    fn maps_native_world_controls() {
        let mut keyboard = BootKeyboard::new();
        for (usage, key) in [
            (0x1a, KeyCode::W),
            (0x04, KeyCode::A),
            (0x16, KeyCode::S),
            (0x07, KeyCode::D),
            (0x14, KeyCode::Q),
            (0x08, KeyCode::E),
            (0x06, KeyCode::C),
            (0x17, KeyCode::T),
            (0x29, KeyCode::Escape),
            (0x0a, KeyCode::G),
            (0x27, KeyCode::Digit0),
            (0x1e, KeyCode::Digit1),
            (0x1f, KeyCode::Digit2),
            (0x20, KeyCode::Digit3),
            (0x21, KeyCode::Digit4),
            (0x10, KeyCode::M),
            (0x1b, KeyCode::X),
            (0x05, KeyCode::B),
            (0x09, KeyCode::F),
            (0x0b, KeyCode::H),
        ] {
            assert_eq!(
                events(&mut keyboard, &[0, 0, usage, 0, 0, 0, 0, 0]),
                [key_down(usage, key)]
            );
        }
    }
}
