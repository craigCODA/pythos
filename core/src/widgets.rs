//! Phase 5 minimal native widget proof.
//!
//! Widgets are typed shell objects. Button activation and text-field editing
//! preserve object identity and use fixed storage only.

#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::shell_objects::{DrawableObject, ObjectId, ObjectKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WidgetError {
    WrongObjectKind,
    EmptyTextBuffer,
    TextOverflow,
    UnsupportedInput,
    BadWidgetState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ButtonActivation {
    object_id: ObjectId,
}

impl ButtonActivation {
    pub const fn object_id(self) -> ObjectId {
        self.object_id
    }
}

pub struct ButtonWidget {
    object: DrawableObject,
    activations: u8,
}

impl ButtonWidget {
    pub fn new(object: DrawableObject) -> Result<Self, WidgetError> {
        if object.kind() != ObjectKind::ButtonWidget {
            return Err(WidgetError::WrongObjectKind);
        }
        Ok(Self {
            object,
            activations: 0,
        })
    }

    pub fn activate(&mut self) -> Result<ButtonActivation, WidgetError> {
        self.activations = self
            .activations
            .checked_add(1)
            .ok_or(WidgetError::BadWidgetState)?;
        Ok(ButtonActivation {
            object_id: self.object.id(),
        })
    }

    pub const fn object(&self) -> DrawableObject {
        self.object
    }

    pub const fn activations(&self) -> u8 {
        self.activations
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextInput {
    Ascii(u8),
    Backspace,
}

pub struct TextFieldWidget<const N: usize> {
    object: DrawableObject,
    buffer: [u8; N],
    len: usize,
}

impl<const N: usize> TextFieldWidget<N> {
    pub fn new(object: DrawableObject) -> Result<Self, WidgetError> {
        if object.kind() != ObjectKind::TextFieldWidget {
            return Err(WidgetError::WrongObjectKind);
        }
        if N == 0 {
            return Err(WidgetError::EmptyTextBuffer);
        }
        Ok(Self {
            object,
            buffer: [0; N],
            len: 0,
        })
    }

    pub fn apply(&mut self, input: TextInput) -> Result<(), WidgetError> {
        match input {
            TextInput::Ascii(byte) if byte.is_ascii_graphic() || byte == b' ' => {
                if self.len == N {
                    return Err(WidgetError::TextOverflow);
                }
                self.buffer[self.len] = byte;
                self.len += 1;
                Ok(())
            }
            TextInput::Ascii(_) => Err(WidgetError::UnsupportedInput),
            TextInput::Backspace => {
                if self.len > 0 {
                    self.len -= 1;
                    self.buffer[self.len] = 0;
                }
                Ok(())
            }
        }
    }

    pub const fn object(&self) -> DrawableObject {
        self.object
    }

    pub fn text(&self) -> &[u8] {
        &self.buffer[..self.len]
    }
}

pub fn run_self_test() -> Result<(), WidgetError> {
    let button_object = DrawableObject::new(ObjectId::new(0x7101), ObjectKind::ButtonWidget);
    let mut button = ButtonWidget::new(button_object)?;
    let activation = button.activate()?;
    if activation.object_id() != button_object.id()
        || button.object() != button_object
        || button.activations() != 1
    {
        return Err(WidgetError::BadWidgetState);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:WIDGET:BUTTON");

    let text_object = DrawableObject::new(ObjectId::new(0x7102), ObjectKind::TextFieldWidget);
    let mut text = TextFieldWidget::<4>::new(text_object)?;
    text.apply(TextInput::Ascii(b'A'))?;
    text.apply(TextInput::Ascii(b'A'))?;
    text.apply(TextInput::Backspace)?;
    text.apply(TextInput::Ascii(b'A'))?;
    if text.object() != text_object || text.text() != b"AA" {
        return Err(WidgetError::BadWidgetState);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:WIDGET:TEXT_FIELD");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_activation_preserves_widget_object_identity() {
        let object = DrawableObject::new(ObjectId::new(1), ObjectKind::ButtonWidget);
        let mut button = ButtonWidget::new(object).unwrap();

        let activation = button.activate().unwrap();

        assert_eq!(activation.object_id(), object.id());
        assert_eq!(button.object(), object);
        assert_eq!(button.activations(), 1);
    }

    #[test]
    fn text_field_edits_fixed_buffer() {
        let object = DrawableObject::new(ObjectId::new(2), ObjectKind::TextFieldWidget);
        let mut field = TextFieldWidget::<4>::new(object).unwrap();

        field.apply(TextInput::Ascii(b'A')).unwrap();
        field.apply(TextInput::Ascii(b'A')).unwrap();
        field.apply(TextInput::Backspace).unwrap();
        field.apply(TextInput::Ascii(b'A')).unwrap();

        assert_eq!(field.object(), object);
        assert_eq!(field.text(), b"AA");
    }

    #[test]
    fn text_field_rejects_overflow_and_unsupported_input() {
        let object = DrawableObject::new(ObjectId::new(2), ObjectKind::TextFieldWidget);
        let mut field = TextFieldWidget::<1>::new(object).unwrap();

        field.apply(TextInput::Ascii(b'A')).unwrap();

        assert_eq!(
            field.apply(TextInput::Ascii(b'B')),
            Err(WidgetError::TextOverflow)
        );
        assert_eq!(
            field.apply(TextInput::Ascii(0)),
            Err(WidgetError::UnsupportedInput)
        );
    }
}
