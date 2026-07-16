//! Phase 5 pointer, focus, and movable-window proof.
//!
//! Interaction state is presentation only. Focus and movement operate on
//! presentation bindings while preserving the typed drawable object identity.

#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::shell_objects::{
    DrawableObject, ObjectId, ObjectKind, PresentationBinding, ShellObjectError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowInteractionError {
    ShellObject(ShellObjectError),
    EmptyPointerBounds,
    PointerOutOfBounds,
    RangeOverflow,
    NoWindowAtPointer,
    NoFocusedWindow,
    BadInteraction,
}

impl From<ShellObjectError> for WindowInteractionError {
    fn from(error: ShellObjectError) -> Self {
        Self::ShellObject(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerCursor {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl PointerCursor {
    pub fn new(width: usize, height: usize) -> Result<Self, WindowInteractionError> {
        if width == 0 || height == 0 {
            return Err(WindowInteractionError::EmptyPointerBounds);
        }
        Ok(Self {
            x: 0,
            y: 0,
            width,
            height,
        })
    }

    pub fn move_to(&mut self, x: usize, y: usize) -> Result<(), WindowInteractionError> {
        if x >= self.width || y >= self.height {
            return Err(WindowInteractionError::PointerOutOfBounds);
        }
        self.x = x;
        self.y = y;
        Ok(())
    }

    pub const fn x(self) -> usize {
        self.x
    }

    pub const fn y(self) -> usize {
        self.y
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowSlot {
    object: DrawableObject,
    binding: PresentationBinding,
}

impl WindowSlot {
    pub const fn new(object: DrawableObject, binding: PresentationBinding) -> Self {
        Self { object, binding }
    }

    pub const fn object(self) -> DrawableObject {
        self.object
    }

    pub const fn binding(self) -> PresentationBinding {
        self.binding
    }

    fn contains(self, x: usize, y: usize) -> Result<bool, WindowInteractionError> {
        let x_end = self
            .binding
            .x()
            .checked_add(self.binding.width())
            .ok_or(WindowInteractionError::RangeOverflow)?;
        let y_end = self
            .binding
            .y()
            .checked_add(self.binding.height())
            .ok_or(WindowInteractionError::RangeOverflow)?;
        Ok(x >= self.binding.x() && x < x_end && y >= self.binding.y() && y < y_end)
    }

    fn move_to(&mut self, x: usize, y: usize) {
        self.binding = self.binding.move_to(x, y);
    }
}

pub struct WindowInteractionState<const N: usize> {
    windows: [WindowSlot; N],
    focused: Option<ObjectId>,
}

impl<const N: usize> WindowInteractionState<N> {
    pub const fn new(windows: [WindowSlot; N]) -> Self {
        Self {
            windows,
            focused: None,
        }
    }

    pub fn focus_at(&mut self, x: usize, y: usize) -> Result<ObjectId, WindowInteractionError> {
        let mut focused: Option<WindowSlot> = None;
        for window in self.windows {
            if window.contains(x, y)? {
                focused = match focused {
                    Some(current) if current.binding().z_order() > window.binding().z_order() => {
                        Some(current)
                    }
                    _ => Some(window),
                };
            }
        }
        let window = focused.ok_or(WindowInteractionError::NoWindowAtPointer)?;
        let object_id = window.object().id();
        self.focused = Some(object_id);
        Ok(object_id)
    }

    pub fn move_focused_to(
        &mut self,
        x: usize,
        y: usize,
    ) -> Result<ObjectId, WindowInteractionError> {
        let focused = self
            .focused
            .ok_or(WindowInteractionError::NoFocusedWindow)?;
        for window in &mut self.windows {
            if window.object().id() == focused {
                let original = window.object();
                window.move_to(x, y);
                if window.object() != original || window.binding().object_id() != original.id() {
                    return Err(WindowInteractionError::BadInteraction);
                }
                return Ok(focused);
            }
        }
        Err(WindowInteractionError::NoFocusedWindow)
    }

    pub fn window_for(&self, object_id: ObjectId) -> Option<WindowSlot> {
        self.windows
            .iter()
            .copied()
            .find(|window| window.object().id() == object_id)
    }
}

pub fn run_self_test() -> Result<(), WindowInteractionError> {
    let mut cursor = PointerCursor::new(8, 8)?;
    cursor.move_to(3, 3)?;
    if cursor.x() != 3 || cursor.y() != 3 {
        return Err(WindowInteractionError::BadInteraction);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:POINTER_CURSOR_READY");

    let service_window =
        DrawableObject::new(ObjectId::new(0x6101), ObjectKind::ServiceMonitorWindow);
    let service_binding = PresentationBinding::new(service_window, 0, 0, 4, 4, 0)?;
    let console_window =
        DrawableObject::new(ObjectId::new(0x6102), ObjectKind::PythonConsoleWindow);
    let console_binding = PresentationBinding::new(console_window, 2, 2, 4, 4, 1)?;
    let mut windows = WindowInteractionState::new([
        WindowSlot::new(service_window, service_binding),
        WindowSlot::new(console_window, console_binding),
    ]);

    let focused = windows.focus_at(cursor.x(), cursor.y())?;
    if focused != console_window.id() {
        return Err(WindowInteractionError::BadInteraction);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:WINDOW_FOCUS_READY");

    let moved = windows.move_focused_to(4, 1)?;
    let moved_window = windows
        .window_for(moved)
        .ok_or(WindowInteractionError::NoFocusedWindow)?;
    if moved != console_window.id()
        || moved_window.object().id() != console_window.id()
        || moved_window.object().kind() != ObjectKind::PythonConsoleWindow
        || moved_window.binding().object_id() != console_window.id()
        || moved_window.binding().x() != 4
        || moved_window.binding().y() != 1
    {
        return Err(WindowInteractionError::BadInteraction);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:MOVABLE_WINDOWS_READY");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_cursor_rejects_out_of_bounds_position() {
        let mut cursor = PointerCursor::new(5, 4).unwrap();

        assert_eq!(cursor.move_to(4, 3), Ok(()));
        assert_eq!(cursor.x(), 4);
        assert_eq!(cursor.y(), 3);
        assert_eq!(
            cursor.move_to(5, 3),
            Err(WindowInteractionError::PointerOutOfBounds)
        );
    }

    #[test]
    fn focus_selects_topmost_window_at_pointer() {
        let service = DrawableObject::new(ObjectId::new(1), ObjectKind::ServiceMonitorWindow);
        let console = DrawableObject::new(ObjectId::new(2), ObjectKind::PythonConsoleWindow);
        let mut windows = WindowInteractionState::new([
            WindowSlot::new(
                service,
                PresentationBinding::new(service, 0, 0, 4, 4, 0).unwrap(),
            ),
            WindowSlot::new(
                console,
                PresentationBinding::new(console, 2, 2, 4, 4, 1).unwrap(),
            ),
        ]);

        assert_eq!(windows.focus_at(3, 3), Ok(console.id()));
    }

    #[test]
    fn moving_focused_window_preserves_object_identity() {
        let service = DrawableObject::new(ObjectId::new(1), ObjectKind::ServiceMonitorWindow);
        let console = DrawableObject::new(ObjectId::new(2), ObjectKind::PythonConsoleWindow);
        let mut windows = WindowInteractionState::new([
            WindowSlot::new(
                service,
                PresentationBinding::new(service, 0, 0, 4, 4, 0).unwrap(),
            ),
            WindowSlot::new(
                console,
                PresentationBinding::new(console, 2, 2, 4, 4, 1).unwrap(),
            ),
        ]);

        assert_eq!(windows.focus_at(3, 3), Ok(console.id()));
        assert_eq!(windows.move_focused_to(6, 1), Ok(console.id()));
        let moved = windows.window_for(console.id()).unwrap();

        assert_eq!(moved.object(), console);
        assert_eq!(moved.binding().object_id(), console.id());
        assert_eq!(moved.binding().x(), 6);
        assert_eq!(moved.binding().y(), 1);
    }
}
