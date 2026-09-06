use crate::{
    input_drivers::{PhysicalKeyboardDecoder, UsbBootMouseReport},
    input_events::{self, InputEventKind, InputSource},
    session_controls::{SessionControlCommand, SessionControlInterpreter},
    viewing::{MotionRoute, ViewingExtent, ViewingSnapshot, ViewingState},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewingInputProbeKeyboardStep {
    Waiting,
    Activated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewingInputProbeError {
    EmptyExtent,
    InputNormalization,
    WrongRoute,
    MissingTraversalMotion,
    MissingCursorMotion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewingInputPresentationStatus {
    WaitingForTraversal,
    WaitingForActivation,
    Active,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewingInputIntegrationFailure {
    Probe(ViewingInputProbeError),
    KeyboardUnavailable,
    Presentation,
}

impl ViewingInputIntegrationFailure {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Probe(ViewingInputProbeError::EmptyExtent) => {
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:EMPTY_EXTENT"
            }
            Self::Probe(ViewingInputProbeError::InputNormalization) => {
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:INPUT_NORMALIZATION"
            }
            Self::Probe(ViewingInputProbeError::WrongRoute) => {
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:WRONG_ROUTE"
            }
            Self::Probe(ViewingInputProbeError::MissingTraversalMotion) => {
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:MISSING_TRAVERSAL_MOTION"
            }
            Self::Probe(ViewingInputProbeError::MissingCursorMotion) => {
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:MISSING_CURSOR_MOTION"
            }
            Self::KeyboardUnavailable => {
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:KEYBOARD_UNAVAILABLE"
            }
            Self::Presentation => "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:PRESENTATION",
        }
    }
}

impl From<ViewingInputProbeError> for ViewingInputIntegrationFailure {
    fn from(error: ViewingInputProbeError) -> Self {
        Self::Probe(error)
    }
}

pub const fn viewing_input_terminal_ready(
    traversal_routed: bool,
    cursor_activated: bool,
    cursor_routed: bool,
    focus_mark_rendered: bool,
) -> bool {
    traversal_routed && cursor_activated && cursor_routed && focus_mark_rendered
}

pub struct ViewingInputProbe {
    keyboard: PhysicalKeyboardDecoder,
    controls: SessionControlInterpreter,
    viewing: ViewingState,
    traversal_motion_seen: bool,
    cursor_motion_seen: bool,
}

impl ViewingInputProbe {
    pub fn new(width: u32, height: u32) -> Result<Self, ViewingInputProbeError> {
        let extent =
            ViewingExtent::new(width, height).map_err(|_| ViewingInputProbeError::EmptyExtent)?;
        Ok(Self {
            keyboard: PhysicalKeyboardDecoder::new(),
            controls: SessionControlInterpreter::new(),
            viewing: ViewingState::new(extent),
            traversal_motion_seen: false,
            cursor_motion_seen: false,
        })
    }

    pub fn observe_keyboard_byte(
        &mut self,
        byte: u8,
    ) -> Result<ViewingInputProbeKeyboardStep, ViewingInputProbeError> {
        let Some(raw) = self.keyboard.feed_raw_byte(byte) else {
            return Ok(ViewingInputProbeKeyboardStep::Waiting);
        };
        let event =
            input_events::normalize(raw).map_err(|_| ViewingInputProbeError::InputNormalization)?;
        if event.source != InputSource::Keyboard {
            return Err(ViewingInputProbeError::WrongRoute);
        }

        match self.controls.observe(event) {
            Some(SessionControlCommand::ActivateCursorFeature) => {
                self.viewing
                    .apply_session_command(SessionControlCommand::ActivateCursorFeature);
                Ok(ViewingInputProbeKeyboardStep::Activated)
            }
            None => Ok(ViewingInputProbeKeyboardStep::Waiting),
        }
    }

    pub fn observe_mouse_report(
        &mut self,
        report: UsbBootMouseReport,
    ) -> Result<Option<MotionRoute>, ViewingInputProbeError> {
        if report.dx == 0 && report.dy == 0 {
            return Ok(None);
        }

        let event = input_events::normalize(report.movement_event())
            .map_err(|_| ViewingInputProbeError::InputNormalization)?;
        let InputEventKind::RelativeMotion(motion) = event.kind else {
            return Err(ViewingInputProbeError::WrongRoute);
        };
        if event.source != InputSource::Mouse {
            return Err(ViewingInputProbeError::WrongRoute);
        }

        let route = self.viewing.route_relative_motion(motion);
        match route {
            MotionRoute::Traversal(_) => self.traversal_motion_seen = true,
            MotionRoute::CursorFocus(_) => self.cursor_motion_seen = true,
        }
        Ok(Some(route))
    }

    pub fn presentation_status(&self) -> ViewingInputPresentationStatus {
        if self.cursor_motion_seen {
            ViewingInputPresentationStatus::Complete
        } else if self.viewing.snapshot().focus_mark.is_some() {
            ViewingInputPresentationStatus::Active
        } else if self.traversal_motion_seen {
            ViewingInputPresentationStatus::WaitingForActivation
        } else {
            ViewingInputPresentationStatus::WaitingForTraversal
        }
    }

    pub fn finish(&self) -> Result<ViewingSnapshot, ViewingInputProbeError> {
        if !self.traversal_motion_seen {
            return Err(ViewingInputProbeError::MissingTraversalMotion);
        }
        if self.viewing.snapshot().focus_mark.is_none() || !self.cursor_motion_seen {
            return Err(ViewingInputProbeError::MissingCursorMotion);
        }
        Ok(self.snapshot())
    }

    pub fn snapshot(&self) -> ViewingSnapshot {
        self.viewing.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        input_drivers::decode_usb_boot_mouse_report,
        input_events::RelativeMotion,
        viewing::{MotionRoute, TraversalIntent},
    };

    fn activate_set1(probe: &mut ViewingInputProbe) {
        for byte in [0x39, 0x39, 0x0E] {
            assert_eq!(
                probe.observe_keyboard_byte(byte).unwrap(),
                ViewingInputProbeKeyboardStep::Waiting
            );
        }
        assert_eq!(
            probe.observe_keyboard_byte(0x0E).unwrap(),
            ViewingInputProbeKeyboardStep::Activated
        );
    }

    #[test]
    fn decoded_usb_motion_routes_to_traversal_then_focus_after_global_activation() {
        let mut probe = ViewingInputProbe::new(100, 80).unwrap();
        let first = decode_usb_boot_mouse_report(&[0, 8, 0xFC, 0]).unwrap();

        assert_eq!(
            probe.observe_mouse_report(first).unwrap(),
            Some(MotionRoute::Traversal(TraversalIntent {
                motion: RelativeMotion { dx: 8, dy: -4 },
            }))
        );
        assert_eq!(probe.snapshot().focus_mark, None);

        activate_set1(&mut probe);

        assert!(matches!(
            probe.observe_mouse_report(first).unwrap(),
            Some(MotionRoute::CursorFocus(_))
        ));
        assert!(probe.finish().is_ok());
    }

    #[test]
    fn repeated_activation_sequence_keeps_cursor_feature_active() {
        let mut probe = ViewingInputProbe::new(100, 80).unwrap();
        activate_set1(&mut probe);
        let active_position = probe.snapshot().focus_mark;

        activate_set1(&mut probe);

        assert_eq!(probe.snapshot().focus_mark, active_position);
        assert_eq!(
            probe.presentation_status(),
            ViewingInputPresentationStatus::Active
        );
    }

    #[test]
    fn zero_motion_report_ignores_buttons_and_preserves_viewing_state() {
        let mut probe = ViewingInputProbe::new(100, 80).unwrap();
        let zero_with_left_button = decode_usb_boot_mouse_report(&[1, 0, 0, 0x7F]).unwrap();

        assert_eq!(probe.observe_mouse_report(zero_with_left_button), Ok(None));
        assert_eq!(probe.snapshot().focus_mark, None);
        assert_eq!(
            probe.presentation_status(),
            ViewingInputPresentationStatus::WaitingForTraversal
        );

        activate_set1(&mut probe);
        let active_position = probe.snapshot().focus_mark;
        assert_eq!(probe.observe_mouse_report(zero_with_left_button), Ok(None));
        assert_eq!(probe.snapshot().focus_mark, active_position);
    }

    #[test]
    fn finish_requires_traversal_and_post_activation_cursor_motion() {
        let mut probe = ViewingInputProbe::new(100, 80).unwrap();
        let motion = decode_usb_boot_mouse_report(&[0, 1, 2]).unwrap();

        assert_eq!(
            probe.finish(),
            Err(ViewingInputProbeError::MissingTraversalMotion)
        );
        assert!(matches!(
            probe.observe_mouse_report(motion),
            Ok(Some(MotionRoute::Traversal(_)))
        ));
        assert_eq!(
            probe.presentation_status(),
            ViewingInputPresentationStatus::WaitingForActivation
        );
        assert_eq!(
            probe.finish(),
            Err(ViewingInputProbeError::MissingCursorMotion)
        );

        activate_set1(&mut probe);
        assert_eq!(
            probe.finish(),
            Err(ViewingInputProbeError::MissingCursorMotion)
        );
        assert!(matches!(
            probe.observe_mouse_report(motion),
            Ok(Some(MotionRoute::CursorFocus(_)))
        ));
        assert_eq!(
            probe.presentation_status(),
            ViewingInputPresentationStatus::Complete
        );
        assert!(probe.finish().is_ok());
    }

    #[test]
    fn set2_activation_sequence_reaches_the_shared_keyboard_decoder() {
        let mut probe = ViewingInputProbe::new(100, 80).unwrap();

        for byte in [0x29, 0x29, 0x66] {
            assert_eq!(
                probe.observe_keyboard_byte(byte).unwrap(),
                ViewingInputProbeKeyboardStep::Waiting
            );
        }
        assert_eq!(
            probe.observe_keyboard_byte(0x66).unwrap(),
            ViewingInputProbeKeyboardStep::Activated
        );
        assert_eq!(
            probe.presentation_status(),
            ViewingInputPresentationStatus::Active
        );
    }

    #[test]
    fn wrong_key_resets_activation_without_changing_viewing_state() {
        let mut probe = ViewingInputProbe::new(100, 80).unwrap();

        assert_eq!(
            probe.observe_keyboard_byte(0x39).unwrap(),
            ViewingInputProbeKeyboardStep::Waiting
        );
        assert_eq!(
            probe.observe_keyboard_byte(0x1E).unwrap(),
            ViewingInputProbeKeyboardStep::Waiting
        );
        assert_eq!(probe.snapshot().focus_mark, None);
        assert_eq!(
            probe.presentation_status(),
            ViewingInputPresentationStatus::WaitingForTraversal
        );

        activate_set1(&mut probe);
        assert_eq!(
            probe.presentation_status(),
            ViewingInputPresentationStatus::Active
        );
    }

    #[test]
    fn empty_extent_uses_the_pure_probe_failure_contract() {
        assert!(matches!(
            ViewingInputProbe::new(0, 80),
            Err(ViewingInputProbeError::EmptyExtent)
        ));
    }

    #[test]
    fn integration_failure_wraps_pure_probe_failure_without_transport_classification() {
        assert_eq!(
            ViewingInputIntegrationFailure::from(ViewingInputProbeError::MissingCursorMotion),
            ViewingInputIntegrationFailure::Probe(ViewingInputProbeError::MissingCursorMotion)
        );
    }

    #[test]
    fn integration_failures_emit_stable_non_driver_markers() {
        let cases = [
            (
                ViewingInputIntegrationFailure::Probe(ViewingInputProbeError::EmptyExtent),
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:EMPTY_EXTENT",
            ),
            (
                ViewingInputIntegrationFailure::Probe(ViewingInputProbeError::InputNormalization),
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:INPUT_NORMALIZATION",
            ),
            (
                ViewingInputIntegrationFailure::Probe(ViewingInputProbeError::WrongRoute),
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:WRONG_ROUTE",
            ),
            (
                ViewingInputIntegrationFailure::Probe(
                    ViewingInputProbeError::MissingTraversalMotion,
                ),
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:MISSING_TRAVERSAL_MOTION",
            ),
            (
                ViewingInputIntegrationFailure::Probe(ViewingInputProbeError::MissingCursorMotion),
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:MISSING_CURSOR_MOTION",
            ),
            (
                ViewingInputIntegrationFailure::KeyboardUnavailable,
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:KEYBOARD_UNAVAILABLE",
            ),
            (
                ViewingInputIntegrationFailure::Presentation,
                "PYTHOS:CORE:VIEWING_INPUT_PROBE_ERROR:PRESENTATION",
            ),
        ];

        for (failure, expected) in cases {
            assert_eq!(failure.marker(), expected);
            assert!(!failure.marker().contains("XHCI_DRIVER_ERROR"));
        }
    }

    #[test]
    fn terminal_readiness_requires_both_routes_activation_and_render() {
        assert!(viewing_input_terminal_ready(true, true, true, true));
        assert!(!viewing_input_terminal_ready(false, true, true, true));
        assert!(!viewing_input_terminal_ready(true, false, true, true));
        assert!(!viewing_input_terminal_ready(true, true, false, true));
        assert!(!viewing_input_terminal_ready(true, true, true, false));
    }
}
