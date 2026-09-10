pub use pythos_shared::viewing::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{input_events::RelativeMotion, session_controls::SessionControlCommand};

    #[test]
    fn traversal_is_the_default_relative_motion_consumer() {
        let extent = ViewingExtent::new(100, 80).unwrap();
        let mut viewing = ViewingState::new(extent);
        let motion = RelativeMotion { dx: 7, dy: -3 };

        assert_eq!(
            viewing.route_relative_motion(motion),
            MotionRoute::Traversal(TraversalIntent { motion })
        );
        assert_eq!(viewing.traversal().motion_count(), 1);
        assert_eq!(viewing.snapshot().focus_mark, None);
    }

    #[test]
    fn activation_routes_later_motion_to_viewing_owned_cursor_feature() {
        let extent = ViewingExtent::new(100, 80).unwrap();
        let mut viewing = ViewingState::new(extent);
        viewing.apply_session_command(SessionControlCommand::ActivateCursorFeature);

        assert_eq!(
            viewing.route_relative_motion(RelativeMotion { dx: 7, dy: -3 }),
            MotionRoute::CursorFocus(FocusMarkPosition { x: 57, y: 37 })
        );
        assert_eq!(viewing.traversal().motion_count(), 0);
        assert!(viewing.cursor_feature().is_active());
    }

    #[test]
    fn repeated_activation_is_idempotent_and_never_deactivates() {
        let extent = ViewingExtent::new(100, 80).unwrap();
        let mut viewing = ViewingState::new(extent);
        viewing.apply_session_command(SessionControlCommand::ActivateCursorFeature);
        viewing.route_relative_motion(RelativeMotion { dx: 4, dy: 2 });
        let active_position = viewing.snapshot().focus_mark;

        viewing.apply_session_command(SessionControlCommand::ActivateCursorFeature);

        assert!(viewing.cursor_feature().is_active());
        assert_eq!(viewing.snapshot().focus_mark, active_position);
    }

    #[test]
    fn focus_position_clamps_to_viewing_extent() {
        let extent = ViewingExtent::new(4, 3).unwrap();
        let mut viewing = ViewingState::new(extent);
        viewing.apply_session_command(SessionControlCommand::ActivateCursorFeature);

        viewing.route_relative_motion(RelativeMotion { dx: 127, dy: 127 });
        assert_eq!(
            viewing.snapshot().focus_mark,
            Some(FocusMarkPosition { x: 3, y: 2 })
        );
        viewing.route_relative_motion(RelativeMotion { dx: -128, dy: -128 });
        assert_eq!(
            viewing.snapshot().focus_mark,
            Some(FocusMarkPosition { x: 0, y: 0 })
        );
    }

    #[test]
    fn zero_sized_viewing_extent_is_rejected() {
        assert_eq!(ViewingExtent::new(0, 10), Err(ViewingError::EmptyExtent));
        assert_eq!(ViewingExtent::new(10, 0), Err(ViewingError::EmptyExtent));
    }
}
