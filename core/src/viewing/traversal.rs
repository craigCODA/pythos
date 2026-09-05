use crate::input_events::RelativeMotion;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraversalIntent {
    pub motion: RelativeMotion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraversalState {
    motion_count: u32,
    last_motion: Option<RelativeMotion>,
}

impl TraversalState {
    pub(super) const fn new() -> Self {
        Self {
            motion_count: 0,
            last_motion: None,
        }
    }

    pub(super) fn accept_relative_motion(&mut self, motion: RelativeMotion) -> TraversalIntent {
        self.motion_count = self.motion_count.saturating_add(1);
        self.last_motion = Some(motion);
        TraversalIntent { motion }
    }

    pub(super) const fn motion_count(&self) -> u32 {
        self.motion_count
    }
}
