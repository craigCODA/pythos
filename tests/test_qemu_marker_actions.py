from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_run_qemu_module():
    path = ROOT / "scripts" / "run-qemu.py"
    spec = importlib.util.spec_from_file_location("run_qemu", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load run-qemu.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class QemuMarkerActionTest(unittest.TestCase):
    def test_marker_delay_waits_until_the_deadline(self) -> None:
        run_qemu = load_run_qemu_module()

        ready, seen_at = run_qemu.marker_delay_ready(None, True, 2.0, 10.0)
        self.assertFalse(ready)
        self.assertEqual(seen_at, 10.0)

        ready, seen_at = run_qemu.marker_delay_ready(seen_at, True, 2.0, 11.9)
        self.assertFalse(ready)
        self.assertEqual(seen_at, 10.0)

        ready, seen_at = run_qemu.marker_delay_ready(seen_at, True, 2.0, 12.0)
        self.assertTrue(ready)
        self.assertEqual(seen_at, 10.0)

    def test_usb_mouse_sequence_is_fourteen_moves_then_left_press_release(self) -> None:
        run_qemu = load_run_qemu_module()
        move = [
            {"type": "rel", "data": {"axis": "x", "value": 8}},
            {"type": "rel", "data": {"axis": "y", "value": -4}},
        ]
        self.assertEqual(run_qemu.usb_mouse_sequence_events(0), move)
        self.assertEqual(run_qemu.usb_mouse_sequence_events(13), move)
        self.assertEqual(
            run_qemu.usb_mouse_sequence_events(14),
            [{"type": "btn", "data": {"button": "left", "down": True}}],
        )
        self.assertEqual(
            run_qemu.usb_mouse_sequence_events(15),
            [{"type": "btn", "data": {"button": "left", "down": False}}],
        )
        with self.assertRaises(ValueError):
            run_qemu.usb_mouse_sequence_events(16)

    def test_marker_sequence_sends_each_new_occurrence_once(self) -> None:
        run_qemu = load_run_qemu_module()
        dispatched_steps: list[int] = []

        def send_step(step: int) -> None:
            dispatched_steps.append(step)

        with self.assertRaises(ValueError):
            run_qemu.advance_marker_sequence_step(2, 0, 16, send_step)
        self.assertEqual(dispatched_steps, [])

        sent = 0
        for observed in range(1, 17):
            sent = run_qemu.advance_marker_sequence_step(
                observed,
                sent,
                16,
                send_step,
            )

        self.assertEqual(sent, 16)
        self.assertEqual(dispatched_steps, list(range(16)))
        self.assertEqual(
            run_qemu.advance_marker_sequence_step(16, sent, 16, send_step),
            16,
        )
        self.assertEqual(dispatched_steps, list(range(16)))
        with self.assertRaises(ValueError):
            run_qemu.advance_marker_sequence_step(17, sent, 16, send_step)
        self.assertEqual(dispatched_steps, list(range(16)))

    def test_sequence_step_sends_exact_input_send_event_envelope(self) -> None:
        run_qemu = load_run_qemu_module()
        commands_seen: list[tuple[dict, ...]] = []
        original_run_qmp_commands = run_qemu.run_qmp_commands
        try:
            run_qemu.run_qmp_commands = commands_seen.append
            run_qemu.request_usb_mouse_sequence_step(14)
        finally:
            run_qemu.run_qmp_commands = original_run_qmp_commands

        self.assertEqual(
            commands_seen,
            [
                (
                    {
                        "execute": "input-send-event",
                        "arguments": {
                            "events": [
                                {
                                    "type": "btn",
                                    "data": {"button": "left", "down": True},
                                }
                            ]
                        },
                    },
                )
            ],
        )

    def test_marker_sequence_increments_only_after_successful_qmp_send(self) -> None:
        run_qemu = load_run_qemu_module()
        sent_steps: list[int] = []

        def send_step(step: int) -> None:
            sent_steps.append(step)

        self.assertEqual(
            run_qemu.advance_marker_sequence_step(1, 0, 16, send_step),
            1,
        )
        self.assertEqual(sent_steps, [0])

        def failing_send_step(step: int) -> None:
            raise RuntimeError(f"failed step {step}")

        with self.assertRaisesRegex(RuntimeError, "failed step 1"):
            run_qemu.advance_marker_sequence_step(2, 1, 16, failing_send_step)

    def test_incomplete_sequence_overrides_success_outcome(self) -> None:
        run_qemu = load_run_qemu_module()
        self.assertEqual(
            run_qemu.sequence_completion_outcome(
                run_qemu.QemuOutcome.SUCCESS,
                True,
                15,
                16,
            ),
            run_qemu.QemuOutcome.RESET,
        )
        self.assertEqual(
            run_qemu.sequence_completion_outcome(
                run_qemu.QemuOutcome.SUCCESS,
                True,
                16,
                16,
            ),
            run_qemu.QemuOutcome.SUCCESS,
        )
        for outcome in (
            run_qemu.QemuOutcome.PANIC,
            run_qemu.QemuOutcome.TIMEOUT,
            run_qemu.QemuOutcome.MARKER_ORDER_VIOLATION,
            run_qemu.QemuOutcome.RESET,
        ):
            self.assertEqual(
                run_qemu.sequence_completion_outcome(outcome, True, 15, 16),
                outcome,
            )

    def test_sequence_option_requires_xhci(self) -> None:
        run_qemu = load_run_qemu_module()
        original_argv = sys.argv
        try:
            sys.argv = [
                "run-qemu.py",
                "--sequence-usb-mouse-after-marker",
                "PYTHOS:CORE:INPUT:MOUSE",
            ]
            with self.assertRaises(SystemExit) as error:
                run_qemu.main()
            self.assertIn("requires --xhci", str(error.exception))
        finally:
            sys.argv = original_argv

    def test_sequence_option_is_exclusive_with_one_shot_movement(self) -> None:
        run_qemu = load_run_qemu_module()
        original_argv = sys.argv
        try:
            sys.argv = [
                "run-qemu.py",
                "--xhci",
                "--move-usb-mouse-after-marker",
                "PYTHOS:CORE:INPUT:MOUSE",
                "--sequence-usb-mouse-after-marker",
                "PYTHOS:CORE:INPUT:MOUSE",
            ]
            with self.assertRaises(SystemExit) as error:
                run_qemu.main()
            self.assertIn("mutually exclusive", str(error.exception))
        finally:
            sys.argv = original_argv


if __name__ == "__main__":
    unittest.main()
