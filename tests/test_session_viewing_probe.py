"""Independent pixel and live-session oracle specimens for ADR 0092."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from dataclasses import replace
from pathlib import Path
from unittest import mock
import tempfile


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "session_viewing_probe_harness", ROOT / "scripts/test-session-viewing-probe.py"
)
assert SPEC is not None and SPEC.loader is not None
HARNESS = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = HARNESS
SPEC.loader.exec_module(HARNESS)

# Literal specimen, not reconstructed from renderer or oracle geometry.
FOCUS_SPECIMEN = (
    "######.............######",
    "######.............######",
    "##.....................##",
    "##.....................##",
    "##.....................##",
    "##.....................##",
    ".........................",
    ".........................",
    ".........................",
    ".........................",
    ".........................",
    ".........................",
    ".........................",
    ".........................",
    ".........................",
    ".........................",
    ".........................",
    ".........................",
    ".........................",
    "##.....................##",
    "##.....................##",
    "##.....................##",
    "##.....................##",
    "######.............######",
    "######.............######",
)


def specimen(*positions: tuple[int, int], dot: tuple[int, int] | None = None) -> bytes:
    pixels = bytearray(640 * 480 * 3)
    for cx, cy in positions:
        for row, line in enumerate(FOCUS_SPECIMEN):
            for column, value in enumerate(line):
                x, y = cx + column - 12, cy + row - 12
                if value == "#" and 0 <= x < 640 and 0 <= y < 480:
                    offset = (y * 640 + x) * 3
                    pixels[offset : offset + 3] = b"\xff\x60\xd0"
    if dot is not None:
        offset = (dot[1] * 640 + dot[0]) * 3
        pixels[offset : offset + 3] = b"\xff\x60\xd0"
    return b"P6\n640 480\n255\n" + pixels


class ViewingPixelsTests(unittest.TestCase):
    def test_inactive_viewport_is_black(self):
        HARNESS.assert_viewing_pixels(specimen(), None)

    def test_exact_four_corner_activation(self):
        HARNESS.assert_viewing_pixels(specimen((320, 240)), (320, 240))

    def test_moved_frame_has_only_new_mark(self):
        HARNESS.assert_viewing_pixels(specimen((327, 233)), (327, 233))

    def test_old_mark_left_behind_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, "pixel"):
            HARNESS.assert_viewing_pixels(specimen((320, 240), (327, 233)), (327, 233))

    def test_center_dot_is_not_a_focus_mark(self):
        with self.assertRaisesRegex(AssertionError, "pixel"):
            HARNESS.assert_viewing_pixels(specimen((320, 240), dot=(320, 240)), (320, 240))

    def test_active_frame_is_not_inactive(self):
        with self.assertRaisesRegex(AssertionError, "pixel"):
            HARNESS.assert_viewing_pixels(specimen((320, 240)), None)

    def test_wrong_position_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, "pixel"):
            HARNESS.assert_viewing_pixels(specimen((319, 240)), (320, 240))

    def test_edge_clipping_retains_four_corner_geometry(self):
        HARNESS.assert_viewing_pixels(specimen((0, 0)), (0, 0))

    def test_truncated_pixels_are_rejected(self):
        with self.assertRaises(AssertionError):
            HARNESS.assert_viewing_pixels(specimen()[:-1], None)

    def test_too_small_viewport_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, "viewport"):
            HARNESS.assert_viewing_pixels(b"P6\n1 1\n255\n\0\0\0", None)


# Independent literal transcripts; never constructed from contract constants.
VALID_COM1 = """PYTHOS:CORE:SESSION_RUNTIME:COM2_READY
PYTHOS:CORE:SESSION_RUNTIME:AUTHORITY_CREATED
PYTHOS:CORE:SESSION_RUNTIME:IDENTITIES_VALID
PYTHOS:CORE:SESSION_RUNTIME:STREAM_BOUND
PYTHOS:CORE:SESSION_VIEWING:PRESENTATION_BOUND
PYTHOS:CORE:SESSION_VIEWING:FRAMEBUFFER_ISOLATED
PYTHOS:CORE:SESSION_RUNTIME:PS2_READY
PYTHOS:CORE:SESSION_RUNTIME:RING3_ENTER
PYTHOS:CORE:SESSION_VIEWING:DRAWN revision:0 active:0 x:0 y:0
PYTHOS:CORE:PS2:MOUSE_IRQ_FIRED
PYTHOS:CORE:SESSION_VIEWING:DRAWN revision:1 active:0 x:0 y:0
PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED
PYTHOS:CORE:SESSION_VIEWING:DRAWN revision:2 active:0 x:0 y:0
PYTHOS:CORE:SESSION_VIEWING:DRAWN revision:3 active:0 x:0 y:0
PYTHOS:CORE:SESSION_VIEWING:DRAWN revision:4 active:0 x:0 y:0
PYTHOS:CORE:SESSION_VIEWING:DRAWN revision:5 active:1 x:320 y:240
PYTHOS:CORE:SESSION_VIEWING:DRAWN revision:6 active:1 x:327 y:233
PYTHOS:CORE:SESSION_VIEWING:DRAWN revision:7 active:1 x:327 y:233
PYTHOS:CORE:SESSION_VIEWING:RING3_RETURN
PYTHOS:CORE:SESSION_VIEWING:INVOCATION_1_VALID
PYTHOS:CORE:SESSION_VIEWING:REINVOKE_VALID
PYTHOS:CORE:SESSION_VIEWING:INVOCATION_2_VALID
PYTHOS:CORE:SESSION_VIEWING:STATE_RETENTION_VALID
PYTHOS:CORE:SESSION_VIEWING:NO_DISK_WRITES
PYTHOS:CORE:SESSION_VIEWING:READY
"""
VALID_COM2 = """PYTHOS:USER:SESSION_VIEWING:BOOT_STATE_0
PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:0
PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:0
PYTHOS:USER:SESSION_VIEWING:MOTION:TRAVERSAL
PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:1
PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:1
PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:2
PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:2
PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:3
PYTHOS:USER:SESSION_VIEWING:INVOCATION:1:VALID
PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:3
PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:4
PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:4
PYTHOS:USER:SESSION_VIEWING:ACTIVATED
PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:5
PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:5
PYTHOS:USER:SESSION_VIEWING:MOTION:FOCUS_MARK
PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:6
PYTHOS:USER:SESSION_VIEWING:INVOCATION:2:VALID
PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:6
PYTHOS:USER:SESSION_VIEWING:DRAWN:REV:7
PYTHOS:USER:SESSION_VIEWING:COMPLETE
"""


def valid_boot():
    timeline = HARNESS.AcceptanceTimeline()
    core = iter(VALID_COM1.splitlines())
    user = iter(VALID_COM2.splitlines())
    for revision in range(8):
        for line in core:
            timeline.record("COM1", line)
            if ":DRAWN revision:" in line:
                break
        for line in user:
            timeline.record("COM2", line)
            if ":WAIT_EVENT:" in line or line.endswith(":COMPLETE"):
                break
        if revision in (0, 5, 6):
            timeline.record("HARNESS", f"SCREENSHOT:{revision}")
        if revision < 7:
            timeline.record("HARNESS", f"INPUT:{revision}:DISPATCH")
            timeline.record("HARNESS", f"INPUT:{revision}:SENT")
    for line in core:
        timeline.record("COM1", line)
    return HARNESS.BootEvidence(
        com1=VALID_COM1,
        com2=VALID_COM2,
        runner_output="QEMU_OUTCOME success\n",
        timeline=timeline,
        process_tree_reaped=True,
        screenshots=(specimen(), specimen((320, 240)), specimen((327, 233))),
    )


class ViewingAcceptanceTests(unittest.TestCase):
    def test_complete_live_session_is_accepted(self):
        HARNESS.assert_boot_acceptance(valid_boot())

    def test_success_after_timeout_is_not_success(self):
        with self.assertRaisesRegex(AssertionError, "outcome"):
            HARNESS.assert_boot_acceptance(replace(valid_boot(), runner_output="QEMU_OUTCOME timeout\nQEMU_OUTCOME success\n"))

    def test_duplicate_drawn_revision_is_rejected(self):
        boot = valid_boot()
        with self.assertRaises(AssertionError):
            HARNESS.assert_boot_acceptance(replace(boot, com1=boot.com1 + "PYTHOS:CORE:SESSION_VIEWING:DRAWN revision:7 active:1 x:327 y:233\n"))

    def test_unknown_owned_marker_is_rejected(self):
        boot = valid_boot()
        with self.assertRaises(AssertionError):
            HARNESS.assert_boot_acceptance(replace(boot, com2=boot.com2 + "PYTHOS:USER:SESSION_VIEWING:UNKNOWN\n"))

    def test_recovery_after_success_is_rejected(self):
        boot = valid_boot()
        with self.assertRaises(AssertionError):
            HARNESS.assert_boot_acceptance(replace(boot, com1=boot.com1 + "PYTHOS:CORE:RECOVERY_REQUESTED\n"))

    def test_missing_kernel_evidence_is_rejected(self):
        boot = valid_boot()
        with self.assertRaises(AssertionError):
            HARNESS.assert_boot_acceptance(replace(boot, com1=boot.com1.replace("PYTHOS:CORE:SESSION_VIEWING:FRAMEBUFFER_ISOLATED\n", "")))

    def test_terminal_only_screenshot_is_rejected(self):
        boot = valid_boot()
        boot.timeline.move_after("HARNESS", "SCREENSHOT:6", "COM1", "PYTHOS:CORE:SESSION_VIEWING:RING3_RETURN")
        with self.assertRaisesRegex(AssertionError, "order"):
            HARNESS.assert_boot_acceptance(boot)

    def test_input_before_ready_is_rejected(self):
        boot = valid_boot()
        boot.timeline.move_after("COM2", "PYTHOS:USER:SESSION_VIEWING:WAIT_EVENT:5", "HARNESS", "INPUT:5:SENT")
        with self.assertRaisesRegex(AssertionError, "order"):
            HARNESS.assert_boot_acceptance(boot)

    def test_surviving_process_tree_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, "process"):
            HARNESS.assert_boot_acceptance(replace(valid_boot(), process_tree_reaped=False))

    def test_final_frame_cannot_stand_in_for_inactive_frame(self):
        with self.assertRaises(AssertionError):
            HARNESS.assert_boot_acceptance(replace(valid_boot(), screenshots=(specimen((327, 233)),) * 3))

    def test_storage_change_between_boots_is_rejected(self):
        image = HARNESS.RUNTIME.ImageSnapshot(
            16777216,
            "080acf35a507ac9849cfcba47dc2ad83e01b75663a516279c8b9d243b719643e",
            HARNESS.RUNTIME.FileIdentity(1, 42), HARNESS.RUNTIME.FileIdentity(1, 42),
        )
        with self.assertRaisesRegex(AssertionError, "storage"):
            HARNESS.assert_two_boot_acceptance(
                (valid_boot(), valid_boot()), (image, replace(image, sha256="a" * 64), image)
            )

    def test_one_boot_cannot_prove_reboot_reset(self):
        with self.assertRaisesRegex(AssertionError, "two boots"):
            HARNESS.assert_two_boot_acceptance((valid_boot(),), ())


class ViewingRunnerTests(unittest.TestCase):
    def test_connect_failure_still_reaps_the_runner_and_preserves_evidence(self):
        runner = mock.Mock()
        tracker = mock.Mock()
        tracker.wait_reaped.return_value = True
        capture = mock.Mock()
        capture.finish.return_value = "QEMU_OUTCOME timeout\n"
        observer = mock.Mock()
        with tempfile.TemporaryDirectory() as directory:
            with (
                mock.patch.object(HARNESS, "spawn_runner_process", return_value=runner),
                mock.patch.object(HARNESS, "RunnerCapture", return_value=capture),
                mock.patch.object(HARNESS, "Com1Observer", return_value=observer),
                mock.patch.object(HARNESS, "connect_com2", side_effect=AssertionError("connection failed")),
                mock.patch.object(HARNESS.RUNTIME, "track_runner_tree", return_value=tracker),
                mock.patch.object(HARNESS, "cleanup_runner_process") as cleanup,
            ):
                with self.assertRaisesRegex(AssertionError, "connection failed"):
                    HARNESS.run_probe_boot(1, Path(directory), Path(directory) / "store.img")
                cleanup.assert_called_once_with(runner)
                observer.stop_join.assert_called_once()
                tracker.wait_reaped.assert_called_once_with(5.0)
                self.assertEqual((Path(directory) / "boot-1-runner.log").read_text(), "QEMU_OUTCOME timeout\n")

    def test_failed_input_injection_does_not_record_sent(self):
        timeline = HARNESS.AcceptanceTimeline()
        with mock.patch.object(HARNESS.launcher_click, "send_relative_mouse_motion", side_effect=RuntimeError("QMP failed")):
            with self.assertRaisesRegex(RuntimeError, "QMP failed"):
                HARNESS.send_input(0, timeline)
        self.assertEqual(timeline.events, [("HARNESS", "INPUT:0:DISPATCH")])


if __name__ == "__main__":
    unittest.main()
