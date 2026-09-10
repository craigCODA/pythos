use crate::{input_types::RelativeMotion, viewing::ViewingExtent};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocusMarkPosition {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CursorFeatureState {
    active: bool,
    position: FocusMarkPosition,
}

impl CursorFeatureState {
    pub(super) const fn new(extent: ViewingExtent) -> Self {
        Self {
            active: false,
            position: FocusMarkPosition {
                x: extent.width() / 2,
                y: extent.height() / 2,
            },
        }
    }

    pub(super) fn activate(&mut self) {
        self.active = true;
    }

    pub const fn is_active(&self) -> bool {
        self.active
    }

    pub(super) fn consume_relative_motion(
        &mut self,
        motion: RelativeMotion,
        extent: ViewingExtent,
    ) -> FocusMarkPosition {
        self.position.x = (i64::from(self.position.x) + i64::from(motion.dx))
            .clamp(0, i64::from(extent.width() - 1)) as u32;
        self.position.y = (i64::from(self.position.y) + i64::from(motion.dy))
            .clamp(0, i64::from(extent.height() - 1)) as u32;
        self.position
    }

    pub const fn position(&self) -> FocusMarkPosition {
        self.position
    }
}
