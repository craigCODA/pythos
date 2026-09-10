mod focus_mark;
mod traversal;

use crate::{input_types::RelativeMotion, session_controls::SessionControlCommand};
use focus_mark::CursorFeatureState;
pub use focus_mark::FocusMarkPosition;
pub use traversal::{TraversalIntent, TraversalState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewingError {
    EmptyExtent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewingExtent {
    width: u32,
    height: u32,
}

impl ViewingExtent {
    pub fn new(width: u32, height: u32) -> Result<Self, ViewingError> {
        if width == 0 || height == 0 {
            return Err(ViewingError::EmptyExtent);
        }
        Ok(Self { width, height })
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionRoute {
    Traversal(TraversalIntent),
    CursorFocus(FocusMarkPosition),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewingSnapshot {
    pub extent: ViewingExtent,
    pub focus_mark: Option<FocusMarkPosition>,
}

pub struct ViewingState {
    extent: ViewingExtent,
    traversal: TraversalState,
    cursor_feature: CursorFeatureState,
}

impl ViewingState {
    pub fn new(extent: ViewingExtent) -> Self {
        Self {
            extent,
            traversal: TraversalState::new(),
            cursor_feature: CursorFeatureState::new(extent),
        }
    }

    pub fn apply_session_command(&mut self, command: SessionControlCommand) {
        match command {
            SessionControlCommand::ActivateCursorFeature => self.cursor_feature.activate(),
        }
    }

    pub fn route_relative_motion(&mut self, motion: RelativeMotion) -> MotionRoute {
        if self.cursor_feature.is_active() {
            MotionRoute::CursorFocus(
                self.cursor_feature
                    .consume_relative_motion(motion, self.extent),
            )
        } else {
            MotionRoute::Traversal(self.traversal.accept_relative_motion(motion))
        }
    }

    pub fn snapshot(&self) -> ViewingSnapshot {
        ViewingSnapshot {
            extent: self.extent,
            focus_mark: self
                .cursor_feature
                .is_active()
                .then(|| self.cursor_feature.position()),
        }
    }

    pub const fn traversal(&self) -> &TraversalState {
        &self.traversal
    }

    pub const fn cursor_feature(&self) -> &CursorFeatureState {
        &self.cursor_feature
    }
}
