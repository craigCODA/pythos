#!/usr/bin/env python
"""QEMU acceptance for Viewing input routing and the FocusMark overlay."""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
import time
import unittest
from pathlib import Path

from launcher_click import type_cursor_activation_sequence


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "viewing-input-probe-com1.log"
SCREENDUMP = TARGET / "viewing-input-probe.ppm"
USB_BOOT_SIM_IMAGE = TARGET / "pythos-viewing-input-probe-boot-sim.img"


def load_recurring_harness():
    path = ROOT / "scripts" / "test-usb-xhci-boot-mouse-recurring-probe.py"
    spec = importlib.util.spec_from_file_location("viewing_recurring_harness", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


RECURRING_HARNESS = load_recurring_harness()


PREFIX = "PYTHOS:CORE:"
TRAVERSAL_MARKER = PREFIX + "VIEWING:TRAVERSAL_RELATIVE_MOTION"
ACTIVATION_READY_MARKER = PREFIX + "SESSION_CONTROL:CURSOR_ACTIVATION_READY"
ACTIVATED_MARKER = PREFIX + "SESSION_CONTROL:CURSOR_ACTIVATED"
DEACTIVATED_MARKER = PREFIX + "SESSION_CONTROL:CURSOR_DEACTIVATED"
CURSOR_MARKER = PREFIX + "VIEWING:CURSOR_RELATIVE_MOTION"
FOCUS_X_MARKER = PREFIX + "VIEWING:FOCUS_MARK_X="
FOCUS_Y_MARKER = PREFIX + "VIEWING:FOCUS_MARK_Y="
FOCUS_READY_MARKER = PREFIX + "VIEWING:FOCUS_MARK_READY"
VIEWING_READY_MARKER = PREFIX + "VIEWING_INPUT_PROBE_READY"
RECURRING_READY_MARKER = PREFIX + "USB_XHCI_PROBE:XHCI_BOOT_MOUSE_RECURRING_READY"
NO_WRITE_MARKER = PREFIX + "USB_XHCI_PROBE:NO_DISK_WRITES"
SUCCESS_MARKER = PREFIX + "USB_XHCI_PROBE_READY"

FOCUS_COLOR = (255, 96, 208)
BACKGROUND_COLOR = (0, 0, 0)
FOCUS_HALF_SPAN = 12
FOCUS_ARM_LENGTH = 6
FOCUS_THICKNESS = 2


def assert_viewing_serial(serial: str) -> tuple[int, int]:
    """Validate the higher-level Viewing markers and return the focus position."""
    lines = serial.splitlines()
    singleton_markers = (
        TRAVERSAL_MARKER,
        ACTIVATION_READY_MARKER,
        ACTIVATED_MARKER,
        RECURRING_READY_MARKER,
        FOCUS_READY_MARKER,
        PREFIX + "USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY",
        NO_WRITE_MARKER,
        VIEWING_READY_MARKER,
        SUCCESS_MARKER,
    )
    for marker in singleton_markers:
        count = lines.count(marker)
        if count != 1:
            raise AssertionError(f"expected exactly one {marker}, got {count}")
    if lines.count(CURSOR_MARKER) != 13:
        raise AssertionError(
            f"expected thirteen cursor motion routes, got {lines.count(CURSOR_MARKER)}"
        )
    if DEACTIVATED_MARKER in lines:
        raise AssertionError("repeated cursor activation produced a deactivation marker")
    if PREFIX + "VIEWING_INPUT_PROBE_ERROR:" in serial:
        raise AssertionError("Viewing input probe emitted an error marker")

    focus_values: list[int] = []
    for marker in (FOCUS_X_MARKER, FOCUS_Y_MARKER):
        matches = [
            re.fullmatch(re.escape(marker) + r"([0-9]+)", line)
            for line in lines
        ]
        values = [match for match in matches if match is not None]
        if len(values) != 1:
            raise AssertionError(f"expected one decimal focus marker {marker}")
        focus_values.append(int(values[0].group(1), 10))

    traversal_index = lines.index(TRAVERSAL_MARKER)
    activation_ready_index = lines.index(ACTIVATION_READY_MARKER)
    activated_index = lines.index(ACTIVATED_MARKER)
    recurring_ready_index = lines.index(RECURRING_READY_MARKER)
    focus_x_index = next(
        index for index, line in enumerate(lines) if line.startswith(FOCUS_X_MARKER)
    )
    focus_y_index = next(
        index for index, line in enumerate(lines) if line.startswith(FOCUS_Y_MARKER)
    )
    terminal_markers = (
        FOCUS_READY_MARKER,
        PREFIX + "USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY",
        NO_WRITE_MARKER,
        VIEWING_READY_MARKER,
        SUCCESS_MARKER,
    )
    terminal_indices = [lines.index(marker) for marker in terminal_markers]
    if not (
        traversal_index
        < activation_ready_index
        < activated_index
        < recurring_ready_index
        < focus_x_index
        < focus_y_index
        < terminal_indices[0]
        < terminal_indices[1]
        < terminal_indices[2]
        < terminal_indices[3]
        < terminal_indices[4]
    ):
        raise AssertionError("Viewing activation or terminal markers are out of order")
    cursor_indices = [
        index for index, line in enumerate(lines) if line == CURSOR_MARKER
    ]
    if any(index <= activated_index for index in cursor_indices):
        raise AssertionError("cursor motion routed before CURSOR_ACTIVATED")
    if any(index >= recurring_ready_index for index in cursor_indices):
        raise AssertionError("cursor motion routed after recurring report completion")
    if traversal_index >= activated_index:
        raise AssertionError("Traversal motion routed after CURSOR_ACTIVATED")
    return focus_values[0], focus_values[1]


def read_ppm_token(data: bytes, offset: int) -> tuple[bytes, int]:
    while offset < len(data):
        if data[offset] in b" \t\r\n":
            offset += 1
            continue
        if data[offset] == ord("#"):
            newline = data.find(b"\n", offset)
            if newline == -1:
                raise AssertionError("unterminated PPM comment")
            offset = newline + 1
            continue
        break
    start = offset
    while offset < len(data) and data[offset] not in b" \t\r\n#":
        offset += 1
    if start == offset:
        raise AssertionError("truncated PPM header")
    return data[start:offset], offset


def parse_ppm(data: bytes) -> tuple[int, int, bytes]:
    magic, offset = read_ppm_token(data, 0)
    width_token, offset = read_ppm_token(data, offset)
    height_token, offset = read_ppm_token(data, offset)
    max_value_token, offset = read_ppm_token(data, offset)
    if magic != b"P6":
        raise AssertionError("screendump is not a binary PPM (P6)")
    try:
        width = int(width_token)
        height = int(height_token)
        max_value = int(max_value_token)
    except ValueError as error:
        raise AssertionError("invalid numeric PPM header") from error
    if width <= 0 or height <= 0:
        raise AssertionError("PPM dimensions must be positive")
    if max_value != 255:
        raise AssertionError(f"unsupported PPM max value {max_value}")
    if offset >= len(data) or data[offset] not in b" \t\r\n":
        raise AssertionError("PPM header missing pixel-data separator")
    if data[offset : offset + 2] == b"\r\n":
        offset += 2
    else:
        offset += 1
    pixels = data[offset:]
    expected_length = width * height * 3
    if len(pixels) != expected_length:
        raise AssertionError(
            f"PPM pixel data length {len(pixels)} does not match {expected_length}"
        )
    return width, height, pixels


def clipped_rectangle_pixels(
    left: int,
    top: int,
    rectangle_width: int,
    rectangle_height: int,
    image_width: int,
    image_height: int,
) -> set[tuple[int, int]]:
    return {
        (x, y)
        for y in range(max(0, top), min(image_height, top + rectangle_height))
        for x in range(max(0, left), min(image_width, left + rectangle_width))
    }


def assert_focus_mark_ppm(data: bytes, focus_x: int, focus_y: int) -> None:
    """Require exactly four separated, clipped L corners at the focus position."""
    width, height, pixels = parse_ppm(data)
    if not (0 <= focus_x < width and 0 <= focus_y < height):
        raise AssertionError("reported focus position is outside the screendump")
    left = focus_x - FOCUS_HALF_SPAN
    top = focus_y - FOCUS_HALF_SPAN
    right = focus_x + FOCUS_HALF_SPAN
    bottom = focus_y + FOCUS_HALF_SPAN
    right_arm_start = right + 1 - FOCUS_ARM_LENGTH
    bottom_arm_start = bottom + 1 - FOCUS_ARM_LENGTH
    right_edge_start = right + 1 - FOCUS_THICKNESS
    bottom_edge_start = bottom + 1 - FOCUS_THICKNESS
    rectangles = (
        (left, top, FOCUS_ARM_LENGTH, FOCUS_THICKNESS),
        (left, top, FOCUS_THICKNESS, FOCUS_ARM_LENGTH),
        (right_arm_start, top, FOCUS_ARM_LENGTH, FOCUS_THICKNESS),
        (right_edge_start, top, FOCUS_THICKNESS, FOCUS_ARM_LENGTH),
        (left, bottom_edge_start, FOCUS_ARM_LENGTH, FOCUS_THICKNESS),
        (left, bottom_arm_start, FOCUS_THICKNESS, FOCUS_ARM_LENGTH),
        (right_arm_start, bottom_edge_start, FOCUS_ARM_LENGTH, FOCUS_THICKNESS),
        (right_edge_start, bottom_arm_start, FOCUS_THICKNESS, FOCUS_ARM_LENGTH),
    )
    expected_focus_pixels: set[tuple[int, int]] = set()
    for rectangle in rectangles:
        expected_focus_pixels.update(
            clipped_rectangle_pixels(*rectangle, width, height)
        )
    observed_focus_pixels = {
        (x, y)
        for y in range(height)
        for x in range(width)
        if tuple(pixels[(y * width + x) * 3 : (y * width + x) * 3 + 3])
        == FOCUS_COLOR
    }
    if observed_focus_pixels != expected_focus_pixels:
        missing = len(expected_focus_pixels - observed_focus_pixels)
        extra = len(observed_focus_pixels - expected_focus_pixels)
        raise AssertionError(
            f"FocusMark pixel mask mismatch: {missing} missing, {extra} extra"
        )

    def pixel_at(x: int, y: int) -> tuple[int, int, int]:
        offset = (y * width + x) * 3
        return tuple(pixels[offset : offset + 3])

    if pixel_at(focus_x, focus_y) != BACKGROUND_COLOR:
        raise AssertionError("FocusMark center is not background")
    horizontal_gap = range(max(0, left), min(width, right + 1))
    vertical_gap = range(max(0, top), min(height, bottom + 1))
    if any(pixel_at(x, focus_y) != BACKGROUND_COLOR for x in horizontal_gap):
        raise AssertionError("FocusMark horizontal axis gap is not background")
    if any(pixel_at(focus_x, y) != BACKGROUND_COLOR for y in vertical_gap):
        raise AssertionError("FocusMark vertical axis gap is not background")


def assert_viewing_report_routes(serial: str) -> None:
    """Tie the fourteen motion routes to their exact ADR 0088 report groups."""
    groups = RECURRING_HARNESS.recurring_groups(serial)
    for index, group in enumerate(groups):
        ordinal = index + 1
        expected_traversal = 1 if ordinal == 1 else 0
        expected_cursor = 1 if 2 <= ordinal <= 14 else 0
        traversal_count = group.count(TRAVERSAL_MARKER)
        cursor_count = group.count(CURSOR_MARKER)
        if traversal_count != expected_traversal or cursor_count != expected_cursor:
            raise AssertionError(
                f"report {ordinal} routes were traversal={traversal_count}, "
                f"cursor={cursor_count}; expected {expected_traversal}, {expected_cursor}"
            )
        if ordinal == 1:
            route = group.find(TRAVERSAL_MARKER)
            ready = group.find(ACTIVATION_READY_MARKER)
            activated = group.find(ACTIVATED_MARKER)
            report_ready = group.find(RECURRING_HARNESS.REPORT_READY_MARKER)
            if not 0 <= route < ready < activated < report_ready:
                raise AssertionError("report one activation sequence is out of order")


def assert_adr0088_serial(serial: str) -> None:
    """Reuse the complete accepted ADR 0088 oracle without weakening it."""
    RECURRING_HARNESS.assert_base_marker_order(serial)
    RECURRING_HARNESS.INTERRUPT_HARNESS.ENDPOINT_HARNESS.assert_endpoint_configuration_result(
        serial
    )
    RECURRING_HARNESS.assert_report_groups(serial)
    RECURRING_HARNESS.assert_terminal_summary(serial)
    RECURRING_HARNESS.assert_forbidden_markers_absent(serial)


def build_probe_image() -> None:
    run = RECURRING_HARNESS.INTERRUPT_HARNESS.run
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(
        [
            "cargo",
            "build",
            "-p",
            "pythos-core",
            "--target",
            "x86_64-unknown-none",
            "--features",
            "viewing-input-probe",
        ]
    )
    RECURRING_HARNESS.INTERRUPT_HARNESS.ENDPOINT_HARNESS.CONFIGURATION_HARNESS.build_verified_user_shell()
    run([sys.executable, "scripts/build-image.py"])


def read_serial_log() -> str:
    if not SERIAL_LOG.exists():
        return ""
    return SERIAL_LOG.read_text(encoding="utf-8", errors="replace")


def run_probe_boot() -> tuple[str, str]:
    for artifact in (SERIAL_LOG, SCREENDUMP):
        if artifact.exists():
            artifact.unlink()
    command = [
        sys.executable,
        "scripts/run-qemu.py",
        "--serial-log",
        str(SERIAL_LOG),
        "--success-marker",
        SUCCESS_MARKER,
        "--timeout",
        "45",
        "--no-audio-device",
        "--no-virtio-blk",
        "--xhci",
        "--xhci-usb-storage",
        "--xhci-usb-storage-image",
        str(USB_BOOT_SIM_IMAGE),
        "--remove-usb-device-after-marker",
        RECURRING_HARNESS.SWAP_READY_MARKER,
        "--remove-usb-device-id",
        "pythos_boot_usb",
        "--hotplug-usb-mouse-after-marker",
        RECURRING_HARNESS.IGNORED_CHANGE_MARKER,
        "--hotplug-usb-mouse-delay",
        "2.0",
        "--sequence-usb-mouse-after-marker",
        RECURRING_HARNESS.TRANSFER_ARMED_MARKER,
        "--screendump",
        str(SCREENDUMP),
        "--expect-outcome",
        "success",
    ]
    print("+ " + " ".join(command), flush=True)
    process = subprocess.Popen(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    activation_deadline = time.monotonic() + 50.0
    while ACTIVATION_READY_MARKER not in read_serial_log():
        if process.poll() is not None:
            output, _ = process.communicate()
            print(output)
            raise AssertionError(
                "QEMU runner exited before SESSION_CONTROL:CURSOR_ACTIVATION_READY"
            )
        if time.monotonic() >= activation_deadline:
            try:
                output, _ = process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                process.terminate()
                output, _ = process.communicate(timeout=5)
            print(output)
            raise AssertionError("timed out waiting for cursor activation readiness")
        time.sleep(0.05)

    type_cursor_activation_sequence()
    try:
        output, _ = process.communicate(timeout=55)
    except subprocess.TimeoutExpired as error:
        process.terminate()
        output, _ = process.communicate(timeout=5)
        print(output)
        raise AssertionError("QEMU runner did not terminate after activation") from error
    print(output)
    if process.returncode != 0:
        raise AssertionError(f"QEMU runner failed with {process.returncode}")
    if output.count("QEMU_OUTCOME success") != 1:
        raise AssertionError("QEMU runner did not report exactly one success outcome")
    if "QEMU_OUTCOME timeout" in output or "usb mouse sequence incomplete" in output:
        raise AssertionError("timeout or incomplete mouse sequence was classified as success")
    if not SERIAL_LOG.exists():
        raise AssertionError("QEMU runner did not create the COM1 serial log")
    if not SCREENDUMP.exists():
        raise AssertionError("QEMU runner did not create the FocusMark screendump")
    return read_serial_log(), output


def main() -> int:
    build_probe_image()
    serial, _qemu_output = run_probe_boot()
    assert_adr0088_serial(serial)
    assert_viewing_report_routes(serial)
    focus_x, focus_y = assert_viewing_serial(serial)
    assert_focus_mark_ppm(SCREENDUMP.read_bytes(), focus_x, focus_y)
    print("VIEWING_INPUT_PROBE_TEST_OK")
    return 0


class ViewingInputOracleSelfTest(unittest.TestCase):
    @staticmethod
    def valid_serial() -> str:
        lines = [
            TRAVERSAL_MARKER,
            ACTIVATION_READY_MARKER,
            ACTIVATED_MARKER,
            *([CURSOR_MARKER] * 13),
            RECURRING_READY_MARKER,
            FOCUS_X_MARKER + "20",
            FOCUS_Y_MARKER + "15",
            FOCUS_READY_MARKER,
            PREFIX + "USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY",
            NO_WRITE_MARKER,
            VIEWING_READY_MARKER,
            SUCCESS_MARKER,
        ]
        return "\n".join(lines) + "\n"

    @staticmethod
    def valid_focus_pixels() -> set[tuple[int, int]]:
        pixels: set[tuple[int, int]] = set()
        rectangles = (
            (8, 3, 6, 2),
            (8, 3, 2, 6),
            (27, 3, 6, 2),
            (31, 3, 2, 6),
            (8, 26, 6, 2),
            (8, 22, 2, 6),
            (27, 26, 6, 2),
            (31, 22, 2, 6),
        )
        for left, top, width, height in rectangles:
            for y in range(top, top + height):
                for x in range(left, left + width):
                    pixels.add((x, y))
        return pixels

    @staticmethod
    def valid_report_route_serial() -> str:
        import importlib.util
        from pathlib import Path

        recurring_path = (
            Path(__file__).resolve().parent
            / "test-usb-xhci-boot-mouse-recurring-probe.py"
        )
        spec = importlib.util.spec_from_file_location(
            "viewing_self_test_recurring_harness", recurring_path
        )
        if spec is None or spec.loader is None:
            raise RuntimeError(f"failed to load {recurring_path}")
        recurring = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(recurring)
        serial = recurring.RecurringOracleRegressionTest.valid_group_serial()
        for ordinal in range(1, 15):
            ready = (
                recurring.REPORT_READY_MARKER
                + f"0x{ordinal:016X}"
            )
            route_lines = [
                TRAVERSAL_MARKER if ordinal == 1 else CURSOR_MARKER,
            ]
            if ordinal == 1:
                route_lines.extend((ACTIVATION_READY_MARKER, ACTIVATED_MARKER))
            serial = serial.replace(
                ready,
                "\n".join((*route_lines, ready)),
                1,
            )
        return serial

    @staticmethod
    def ppm_with_focus_pixels(focus_pixels: set[tuple[int, int]]) -> bytes:
        width = 41
        height = 31
        pixels = bytearray(BACKGROUND_COLOR * (width * height))
        for x, y in focus_pixels:
            offset = (y * width + x) * 3
            pixels[offset : offset + 3] = bytes(FOCUS_COLOR)
        return f"P6\n{width} {height}\n255\n".encode("ascii") + bytes(pixels)

    def test_valid_serial_semantics_pass(self) -> None:
        self.assertEqual(assert_viewing_serial(self.valid_serial()), (20, 15))

    def test_missing_or_out_of_order_activation_markers_fail(self) -> None:
        serial = self.valid_serial()
        for marker in (ACTIVATION_READY_MARKER, ACTIVATED_MARKER):
            with self.subTest(missing=marker), self.assertRaises(AssertionError):
                assert_viewing_serial(serial.replace(marker + "\n", "", 1))
        out_of_order = serial.replace(
            ACTIVATION_READY_MARKER + "\n" + ACTIVATED_MARKER,
            ACTIVATED_MARKER + "\n" + ACTIVATION_READY_MARKER,
        )
        with self.assertRaises(AssertionError):
            assert_viewing_serial(out_of_order)

    def test_cursor_route_before_activation_fails(self) -> None:
        serial = self.valid_serial().replace(CURSOR_MARKER + "\n", "", 1)
        serial = serial.replace(
            ACTIVATION_READY_MARKER,
            CURSOR_MARKER + "\n" + ACTIVATION_READY_MARKER,
            1,
        )
        with self.assertRaises(AssertionError):
            assert_viewing_serial(serial)

    def test_traversal_route_after_activation_fails(self) -> None:
        serial = self.valid_serial().replace(TRAVERSAL_MARKER + "\n", "", 1)
        serial = serial.replace(
            ACTIVATED_MARKER,
            ACTIVATED_MARKER + "\n" + TRAVERSAL_MARKER,
            1,
        )
        with self.assertRaises(AssertionError):
            assert_viewing_serial(serial)

    def test_repeated_activation_does_not_deactivate_cursor(self) -> None:
        serial = self.valid_serial().replace(
            ACTIVATED_MARKER,
            ACTIVATED_MARKER + "\n" + DEACTIVATED_MARKER,
            1,
        )
        with self.assertRaises(AssertionError):
            assert_viewing_serial(serial)

    def test_missing_adr0088_terminal_markers_fail(self) -> None:
        serial = self.valid_serial()
        for marker in (RECURRING_READY_MARKER, NO_WRITE_MARKER, SUCCESS_MARKER):
            with self.subTest(missing=marker), self.assertRaises(AssertionError):
                assert_viewing_serial(serial.replace(marker + "\n", "", 1))

    def test_valid_four_separated_l_corners_pass(self) -> None:
        ppm = self.ppm_with_focus_pixels(self.valid_focus_pixels())
        assert_focus_mark_ppm(ppm, 20, 15)

    def test_non_focus_mark_shapes_fail(self) -> None:
        valid = self.valid_focus_pixels()
        arrow = {
            (20, 15),
            (20, 16),
            (21, 16),
            (20, 17),
            (21, 17),
            (22, 17),
            (20, 18),
        }
        filled_center = valid | {
            (x, y) for y in range(13, 18) for x in range(18, 23)
        }
        dot = {(20, 15)}
        crosshair = valid | {(x, 15) for x in range(8, 33)} | {
            (20, y) for y in range(3, 28)
        }
        joined = valid | {(x, y) for x in range(8, 33) for y in (3, 27)} | {
            (x, y) for x in (8, 32) for y in range(3, 28)
        }
        cases = {
            "arrow": arrow,
            "filled center": filled_center,
            "dot": dot,
            "crosshair": crosshair,
            "joined corners": joined,
        }
        for label, pixels in cases.items():
            with self.subTest(shape=label), self.assertRaises(AssertionError):
                assert_focus_mark_ppm(self.ppm_with_focus_pixels(pixels), 20, 15)

    def test_focus_mark_at_wrong_reported_position_fails(self) -> None:
        ppm = self.ppm_with_focus_pixels(self.valid_focus_pixels())
        with self.assertRaises(AssertionError):
            assert_focus_mark_ppm(ppm, 21, 15)

    def test_routes_are_tied_to_reports_one_through_fourteen(self) -> None:
        assert_viewing_report_routes(self.valid_report_route_serial())

    def test_wrong_or_button_report_motion_routes_fail(self) -> None:
        serial = self.valid_report_route_serial()
        cases = {
            "report one cursor": serial.replace(TRAVERSAL_MARKER, CURSOR_MARKER, 1),
            "report two traversal": serial.replace(CURSOR_MARKER, TRAVERSAL_MARKER, 1),
            "report fifteen motion": serial.replace(
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_REPORT_READY="
                "0x000000000000000F",
                CURSOR_MARKER
                + "\nPYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_REPORT_READY="
                "0x000000000000000F",
                1,
            ),
        }
        for label, malformed in cases.items():
            with self.subTest(case=label), self.assertRaises(AssertionError):
                assert_viewing_report_routes(malformed)


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(
        ViewingInputOracleSelfTest
    )
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.wasSuccessful():
        print("VIEWING_INPUT_PROBE_SELF_TEST_OK")
        return 0
    return 1


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-viewing-input-probe.py [--self-test]")
    raise SystemExit(main())
