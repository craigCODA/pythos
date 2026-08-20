//! Phase 5 shell object identity primitives.
//!
//! Drawable meaning is represented by stable typed objects. Presentation state
//! is a separate binding so later shell work does not collapse window meaning
//! into framebuffer pixels.

#![cfg_attr(test, allow(dead_code))]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellObjectError {
    EmptyPresentation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectId(u64);

impl ObjectId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectKind {
    ApplicationLauncherWindow,
    BootIdentitySurface,
    ServiceMonitorWindow,
    PythonConsoleWindow,
    SettingsPanelWindow,
    WorkspaceSession,
    ObjectBrowserWindow,
    ButtonWidget,
    TextFieldWidget,
    Note,
    Task,
    TaskProposal,
    TaskEvent,
    TaskRelation,
    RelevanceAssertion,
    CapabilityRequest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrawableObject {
    id: ObjectId,
    kind: ObjectKind,
}

impl DrawableObject {
    pub const fn new(id: ObjectId, kind: ObjectKind) -> Self {
        Self { id, kind }
    }

    pub const fn id(self) -> ObjectId {
        self.id
    }

    pub const fn kind(self) -> ObjectKind {
        self.kind
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationBinding {
    object_id: ObjectId,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    z_order: u8,
}

impl PresentationBinding {
    pub fn new(
        object: DrawableObject,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        z_order: u8,
    ) -> Result<Self, ShellObjectError> {
        if width == 0 || height == 0 {
            return Err(ShellObjectError::EmptyPresentation);
        }
        Ok(Self {
            object_id: object.id(),
            x,
            y,
            width,
            height,
            z_order,
        })
    }

    pub fn move_to(self, x: usize, y: usize) -> Self {
        Self { x, y, ..self }
    }

    pub const fn object_id(self) -> ObjectId {
        self.object_id
    }

    pub const fn x(self) -> usize {
        self.x
    }

    pub const fn y(self) -> usize {
        self.y
    }

    pub const fn width(self) -> usize {
        self.width
    }

    pub const fn height(self) -> usize {
        self.height
    }

    pub const fn z_order(self) -> u8 {
        self.z_order
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_binding_moves_without_changing_object_identity() {
        let object = DrawableObject::new(ObjectId::new(42), ObjectKind::ServiceMonitorWindow);
        let binding = PresentationBinding::new(object, 1, 2, 4, 3, 7).unwrap();
        let moved = binding.move_to(9, 10);

        assert_eq!(moved.object_id(), object.id());
        assert_eq!(moved.x(), 9);
        assert_eq!(moved.y(), 10);
        assert_eq!(moved.width(), 4);
        assert_eq!(moved.height(), 3);
        assert_eq!(moved.z_order(), 7);
    }
}
