#!/usr/bin/env python
"""Acceptance test for sixteen recurring USB HID boot-mouse reports."""

from __future__ import annotations

import importlib.util
import re
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "usb-xhci-boot-mouse-recurring-probe-com1.log"
USB_BOOT_SIM_IMAGE = TARGET / "pythos-xhci-boot-mouse-recurring-boot-sim.img"


def load_interrupt_harness():
    path = ROOT / "scripts" / "test-usb-xhci-interrupt-transfer-probe.py"
    spec = importlib.util.spec_from_file_location("xhci_interrupt_harness", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


INTERRUPT_HARNESS = load_interrupt_harness()
PREFIX = "PYTHOS:CORE:USB_XHCI_PROBE:"
SUCCESS_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE_READY"
SWAP_READY_MARKER = PREFIX + "SWAP_READY"
IGNORED_CHANGE_MARKER = PREFIX + "XHCI_SWAP_POLL_IGNORED_CHANGE"
TRANSFER_ARMED_MARKER = PREFIX + "XHCI_INTERRUPT_TRANSFER_ARMED"
ORDINAL_MARKER = PREFIX + "XHCI_BOOT_MOUSE_REPORT_ORDINAL="
REPORT_READY_MARKER = PREFIX + "XHCI_BOOT_MOUSE_REPORT_READY="
SEQUENCE_TARGET_MARKER = PREFIX + "XHCI_BOOT_MOUSE_SEQUENCE_TARGET="
RECURRING_READY_MARKER = PREFIX + "XHCI_BOOT_MOUSE_RECURRING_READY"
FRAMEBUFFER_READY_MARKER = PREFIX + "FRAMEBUFFER_IDENTITY_READY"
NO_WRITE_MARKER = PREFIX + "NO_DISK_WRITES"
TERMINAL_REPORT_COUNT = PREFIX + "XHCI_BOOT_MOUSE_REPORT_COUNT="

GROUP_MARKERS = (
    ORDINAL_MARKER,
    PREFIX + "XHCI_INTERRUPT_TRANSFER_TRB_INDEX=",
    PREFIX + "XHCI_INTERRUPT_TRANSFER_CYCLE=",
    PREFIX + "XHCI_INTERRUPT_TRANSFER_REQUESTED=",
    TRANSFER_ARMED_MARKER,
    PREFIX + "XHCI_INTERRUPT_TRANSFER_CC=",
    PREFIX + "XHCI_INTERRUPT_TRANSFER_ACTUAL=",
    PREFIX + "XHCI_INTERRUPT_TRANSFER_CAPTURED=",
    PREFIX + "XHCI_INTERRUPT_TRANSFER_RAW=",
    PREFIX + "XHCI_INTERRUPT_TRANSFER_READY",
    PREFIX + "XHCI_BOOT_MOUSE_BUTTONS=",
    PREFIX + "XHCI_BOOT_MOUSE_DX_I8=",
    PREFIX + "XHCI_BOOT_MOUSE_DY_I8=",
    PREFIX + "XHCI_BOOT_MOUSE_AUX_PRESENT=",
    PREFIX + "XHCI_BOOT_MOUSE_AUX=",
    PREFIX + "XHCI_BOOT_MOUSE_EVENT_READY",
    PREFIX + "XHCI_BOOT_MOUSE_DECODE_READY",
    REPORT_READY_MARKER,
)

TERMINAL_FIELDS = (
    TERMINAL_REPORT_COUNT,
    PREFIX + "XHCI_BOOT_MOUSE_DX_TOTAL_I32=",
    PREFIX + "XHCI_BOOT_MOUSE_DY_TOTAL_I32=",
    PREFIX + "XHCI_BOOT_MOUSE_BUTTONS_LAST=",
    PREFIX + "XHCI_BOOT_MOUSE_PRESSED_SEEN=",
    PREFIX + "XHCI_BOOT_MOUSE_RELEASED_AFTER_PRESSED=",
    PREFIX + "XHCI_BOOT_MOUSE_SEQUENCE_AUX_PRESENT=",
    PREFIX + "XHCI_BOOT_MOUSE_SEQUENCE_AUX_LAST=",
    PREFIX + "XHCI_INTERRUPT_TRANSFER_WRAP_COUNT=",
    PREFIX + "XHCI_EVENT_RING_WRAP_COUNT=",
)

FORBIDDEN_MARKERS = (
    PREFIX + "XHCI_DRIVER_ERROR:",
    PREFIX + "XHCI_BOOT_MOUSE_DECODE_INVALID",
    PREFIX + "XHCI_BOOT_MOUSE_TERMINAL_INVARIANT",
    PREFIX + "FRAMEBUFFER_IDENTITY_FAILED",
    "PYTHOS:CORE:POINTER_CURSOR_READY",
    "PYTHOS:CORE:INPUT_EVENT_SERVICE_READY",
    "PYTHOS:CORE:INPUT:EVENT",
    "PYTHOS:CORE:STORAGE:JOURNAL_APPEND",
    "PYTHOS:CORE:STORAGE:COMMIT_MARKER",
    "PYTHOS:CORE:OBJECT_STORE:PERSISTED",
    "PYTHOS:CORE:PANIC",
    "PANIC:",
)


def build_probe_image() -> None:
    INTERRUPT_HARNESS.run(
        ["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"]
    )
    INTERRUPT_HARNESS.run(
        [
            "cargo",
            "build",
            "-p",
            "pythos-core",
            "--target",
            "x86_64-unknown-none",
            "--features",
            "usb-xhci-boot-mouse-recurring-probe",
        ]
    )
    INTERRUPT_HARNESS.ENDPOINT_HARNESS.CONFIGURATION_HARNESS.build_verified_user_shell()
    INTERRUPT_HARNESS.run([sys.executable, "scripts/build-image.py"])


def run_probe_boot() -> tuple[str, str]:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    qemu_output = INTERRUPT_HARNESS.run(
        [
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
            SWAP_READY_MARKER,
            "--remove-usb-device-id",
            "pythos_boot_usb",
            "--hotplug-usb-mouse-after-marker",
            IGNORED_CHANGE_MARKER,
            "--hotplug-usb-mouse-delay",
            "2.0",
            "--sequence-usb-mouse-after-marker",
            TRANSFER_ARMED_MARKER,
            "--expect-outcome",
            "success",
        ]
    )
    if qemu_output.count("QEMU_OUTCOME success") != 1:
        raise AssertionError("recurring boot did not have exactly one success outcome")
    if "QEMU_OUTCOME timeout" in qemu_output or "usb mouse sequence incomplete" in qemu_output:
        raise AssertionError("timeout or incomplete input sequence was classified as success")
    return SERIAL_LOG.read_text(encoding="utf-8", errors="replace"), qemu_output


def extract_single_hex(text: str, marker: str) -> int:
    matches = re.findall(re.escape(marker) + r"0x([0-9A-Fa-f]+)", text)
    if len(matches) != 1:
        raise AssertionError(f"expected one numeric marker {marker}, got {len(matches)}")
    return int(matches[0], 16)


def assert_ordered_once(text: str, markers: tuple[str, ...]) -> None:
    cursor = 0
    for marker in markers:
        if text.count(marker) != 1:
            raise AssertionError(f"expected exactly one marker in group: {marker}")
        position = text.find(marker, cursor)
        if position == -1:
            raise AssertionError(f"missing or out-of-order marker in group: {marker}")
        cursor = position + len(marker)


def recurring_groups(serial: str) -> list[str]:
    ordinal_matches = list(
        re.finditer(re.escape(ORDINAL_MARKER) + r"0x([0-9A-Fa-f]+)", serial)
    )
    if len(ordinal_matches) != 16:
        raise AssertionError(f"expected 16 report groups, got {len(ordinal_matches)}")
    terminal_start = serial.find(TERMINAL_REPORT_COUNT, ordinal_matches[-1].start())
    if terminal_start == -1:
        raise AssertionError("missing recurring terminal aggregate fields")
    groups = []
    for index, match in enumerate(ordinal_matches):
        end = (
            ordinal_matches[index + 1].start()
            if index + 1 < len(ordinal_matches)
            else terminal_start
        )
        groups.append(serial[match.start() : end])
    return groups


def assert_base_marker_order(serial: str) -> None:
    requested_marker = PREFIX + "XHCI_INTERRUPT_TRANSFER_REQUESTED="
    requested_index = INTERRUPT_HARNESS.REQUIRED_MARKERS.index(requested_marker)
    cursor = 0
    for marker in INTERRUPT_HARNESS.REQUIRED_MARKERS[:requested_index]:
        position = serial.find(marker, cursor)
        if position == -1:
            raise AssertionError(f"missing or out-of-order prerequisite marker: {marker}")
        cursor = position + len(marker)
    if serial.count(SEQUENCE_TARGET_MARKER) != 1:
        raise AssertionError("expected exactly one recurring sequence target marker")
    if extract_single_hex(serial, SEQUENCE_TARGET_MARKER) != 16:
        raise AssertionError("recurring sequence target was not sixteen reports")
    target = re.search(
        re.escape(SEQUENCE_TARGET_MARKER) + r"0x([0-9A-Fa-f]+)", serial[cursor:]
    )
    if target is None:
        raise AssertionError("recurring sequence target did not follow endpoint setup")
    first_ordinal = re.search(
        re.escape(ORDINAL_MARKER) + r"0x([0-9A-Fa-f]+)", serial[cursor:]
    )
    if first_ordinal is None:
        raise AssertionError("first recurring report did not follow endpoint setup")
    target_position = cursor + target.start()
    target_end = cursor + target.end()
    first_ordinal_position = cursor + first_ordinal.start()
    if target_position > first_ordinal_position:
        raise AssertionError("recurring sequence target did not precede ordinal one")
    if int(first_ordinal.group(1), 16) != 1:
        raise AssertionError("first recurring report ordinal was not one")
    if serial[target_end:first_ordinal_position].strip():
        raise AssertionError("recurring sequence target was not immediately before ordinal one")


def assert_report_groups(serial: str) -> None:
    for marker in GROUP_MARKERS:
        count = serial.count(marker)
        if count != 16:
            raise AssertionError(
                f"expected exactly sixteen global occurrences of {marker}, got {count}"
            )
    groups = recurring_groups(serial)
    ordinals = []
    trb_indices = []
    trb_cycles = []
    raw_group_count = 0
    decode_group_count = 0
    for index, group in enumerate(groups):
        assert_ordered_once(group, GROUP_MARKERS)
        ordinals.append(extract_single_hex(group, ORDINAL_MARKER))
        trb_indices.append(
            extract_single_hex(group, PREFIX + "XHCI_INTERRUPT_TRANSFER_TRB_INDEX=")
        )
        trb_cycles.append(
            extract_single_hex(group, PREFIX + "XHCI_INTERRUPT_TRANSFER_CYCLE=")
        )
        if extract_single_hex(group, REPORT_READY_MARKER) != index + 1:
            raise AssertionError(f"report-ready ordinal mismatch in group {index + 1}")
        if extract_single_hex(group, PREFIX + "XHCI_INTERRUPT_TRANSFER_REQUESTED=") != 4:
            raise AssertionError("recurring capture did not request the four-byte endpoint size")
        completion = extract_single_hex(group, PREFIX + "XHCI_INTERRUPT_TRANSFER_CC=")
        if completion not in (1, 13):
            raise AssertionError(f"unexpected recurring transfer completion {completion:#x}")
        actual = extract_single_hex(group, PREFIX + "XHCI_INTERRUPT_TRANSFER_ACTUAL=")
        captured = extract_single_hex(group, PREFIX + "XHCI_INTERRUPT_TRANSFER_CAPTURED=")
        if actual != 4 or captured != 4:
            raise AssertionError(f"incomplete four-byte report group {index + 1}")
        expected_buttons = 1 if index == 14 else 0
        expected_dx = 8 if index < 14 else 0
        expected_dy = 0xFC if index < 14 else 0
        observed = (
            extract_single_hex(group, PREFIX + "XHCI_BOOT_MOUSE_BUTTONS="),
            extract_single_hex(group, PREFIX + "XHCI_BOOT_MOUSE_DX_I8="),
            extract_single_hex(group, PREFIX + "XHCI_BOOT_MOUSE_DY_I8="),
            extract_single_hex(group, PREFIX + "XHCI_BOOT_MOUSE_AUX_PRESENT="),
            extract_single_hex(group, PREFIX + "XHCI_BOOT_MOUSE_AUX="),
        )
        expected = (expected_buttons, expected_dx, expected_dy, 1, 0)
        if observed != expected:
            raise AssertionError(
                f"decoded report state {index + 1} was {observed}, expected {expected}"
            )
        raw_group_count += group.count(PREFIX + "XHCI_INTERRUPT_TRANSFER_RAW=")
        decode_group_count += group.count(PREFIX + "XHCI_BOOT_MOUSE_DECODE_READY")

    assert ordinals == list(range(1, 17))
    assert trb_indices == list(range(15)) + [0]
    assert trb_cycles == [1] * 15 + [0]
    assert raw_group_count == 16
    assert decode_group_count == 16
    if serial.count(TRANSFER_ARMED_MARKER) != 16:
        raise AssertionError("expected exactly sixteen arms and no seventeenth arm")
    wrap_marker = PREFIX + "XHCI_INTERRUPT_TRANSFER_RING_WRAP="
    transfer_wrap_count = serial.count(wrap_marker)
    assert transfer_wrap_count == 1
    if extract_single_hex(serial, wrap_marker) != 1:
        raise AssertionError("transfer ring wrap marker did not report wrap one")


def assert_terminal_summary(serial: str) -> None:
    terminal_values = {
        TERMINAL_REPORT_COUNT: 16,
        PREFIX + "XHCI_BOOT_MOUSE_DX_TOTAL_I32=": 0x00000070,
        PREFIX + "XHCI_BOOT_MOUSE_DY_TOTAL_I32=": 0xFFFFFFC8,
        PREFIX + "XHCI_BOOT_MOUSE_BUTTONS_LAST=": 0,
        PREFIX + "XHCI_BOOT_MOUSE_PRESSED_SEEN=": 1,
        PREFIX + "XHCI_BOOT_MOUSE_RELEASED_AFTER_PRESSED=": 1,
        PREFIX + "XHCI_BOOT_MOUSE_SEQUENCE_AUX_PRESENT=": 1,
        PREFIX + "XHCI_BOOT_MOUSE_SEQUENCE_AUX_LAST=": 0,
        PREFIX + "XHCI_INTERRUPT_TRANSFER_WRAP_COUNT=": 1,
    }
    for marker, expected in terminal_values.items():
        actual = extract_single_hex(serial, marker)
        if actual != expected:
            raise AssertionError(f"expected {marker}{expected:#x}, got {actual:#x}")
    event_wrap_count = extract_single_hex(serial, PREFIX + "XHCI_EVENT_RING_WRAP_COUNT=")
    if event_wrap_count < 1:
        raise AssertionError("shared event consumer did not cross an event-ring cycle wrap")

    cursor = serial.find(REPORT_READY_MARKER + "0x0000000000000010")
    if cursor == -1:
        raise AssertionError("missing final report-ready boundary")
    for marker in TERMINAL_FIELDS + (
        RECURRING_READY_MARKER,
        FRAMEBUFFER_READY_MARKER,
        NO_WRITE_MARKER,
        SUCCESS_MARKER,
    ):
        position = serial.find(marker, cursor)
        if position == -1:
            raise AssertionError(f"missing or out-of-order terminal marker: {marker}")
        cursor = position + len(marker)

    for marker in TERMINAL_FIELDS + (
        RECURRING_READY_MARKER,
        FRAMEBUFFER_READY_MARKER,
        NO_WRITE_MARKER,
        SUCCESS_MARKER,
    ):
        if serial.count(marker) != 1:
            raise AssertionError(f"duplicate or missing terminal marker: {marker}")


def assert_forbidden_markers_absent(serial: str) -> None:
    for marker in FORBIDDEN_MARKERS:
        if marker in serial:
            raise AssertionError(f"recurring boundary unexpectedly reached marker: {marker}")


class RecurringOracleRegressionTest(unittest.TestCase):
    @staticmethod
    def valid_group_serial() -> str:
        lines: list[str] = []
        for index in range(16):
            ordinal = index + 1
            buttons = 1 if index == 14 else 0
            dx = 8 if index < 14 else 0
            dy = 0xFC if index < 14 else 0
            values = (
                (ORDINAL_MARKER, ordinal),
                (PREFIX + "XHCI_INTERRUPT_TRANSFER_TRB_INDEX=", index if index < 15 else 0),
                (PREFIX + "XHCI_INTERRUPT_TRANSFER_CYCLE=", 1 if index < 15 else 0),
                (PREFIX + "XHCI_INTERRUPT_TRANSFER_REQUESTED=", 4),
            )
            lines.extend(f"{marker}0x{value:016X}" for marker, value in values)
            lines.append(TRANSFER_ARMED_MARKER)
            lines.extend(
                (
                    PREFIX + "XHCI_INTERRUPT_TRANSFER_CC=0x0000000000000001",
                    PREFIX + "XHCI_INTERRUPT_TRANSFER_ACTUAL=0x0000000000000004",
                    PREFIX + "XHCI_INTERRUPT_TRANSFER_CAPTURED=0x0000000000000004",
                    PREFIX + "XHCI_INTERRUPT_TRANSFER_RAW=0x0000000000000000",
                    PREFIX + "XHCI_INTERRUPT_TRANSFER_READY",
                    f"{PREFIX}XHCI_BOOT_MOUSE_BUTTONS=0x{buttons:016X}",
                    f"{PREFIX}XHCI_BOOT_MOUSE_DX_I8=0x{dx:016X}",
                    f"{PREFIX}XHCI_BOOT_MOUSE_DY_I8=0x{dy:016X}",
                    PREFIX + "XHCI_BOOT_MOUSE_AUX_PRESENT=0x0000000000000001",
                    PREFIX + "XHCI_BOOT_MOUSE_AUX=0x0000000000000000",
                    PREFIX + "XHCI_BOOT_MOUSE_EVENT_READY",
                    PREFIX + "XHCI_BOOT_MOUSE_DECODE_READY",
                    f"{REPORT_READY_MARKER}0x{ordinal:016X}",
                )
            )
            if ordinal == 15:
                lines.append(
                    PREFIX
                    + "XHCI_INTERRUPT_TRANSFER_RING_WRAP=0x0000000000000001"
                )
        lines.append(TERMINAL_REPORT_COUNT + "0x0000000000000010")
        return "\n".join(lines) + "\n"

    @staticmethod
    def prerequisite_serial() -> str:
        requested_marker = PREFIX + "XHCI_INTERRUPT_TRANSFER_REQUESTED="
        requested_index = INTERRUPT_HARNESS.REQUIRED_MARKERS.index(requested_marker)
        return "\n".join(
            marker + "0x0000000000000000"
            for marker in INTERRUPT_HARNESS.REQUIRED_MARKERS[:requested_index]
        )

    def test_sequence_target_is_required_exactly_once_before_ordinal_one(self) -> None:
        target = PREFIX + "XHCI_BOOT_MOUSE_SEQUENCE_TARGET=0x0000000000000010"
        ordinal = ORDINAL_MARKER + "0x0000000000000001"
        ordinal_two = ORDINAL_MARKER + "0x0000000000000002"
        prerequisite = self.prerequisite_serial()

        with self.assertRaises(AssertionError):
            assert_base_marker_order(prerequisite + "\n" + ordinal)
        with self.assertRaises(AssertionError):
            assert_base_marker_order(prerequisite + "\n" + target + "\n" + target + "\n" + ordinal)
        with self.assertRaises(AssertionError):
            assert_base_marker_order(prerequisite + "\n" + ordinal + "\n" + target)
        with self.assertRaises(AssertionError):
            assert_base_marker_order(
                prerequisite + "\n" + ordinal + "\n" + target + "\n" + ordinal_two
            )
        with self.assertRaises(AssertionError):
            assert_base_marker_order(
                prerequisite + "\n" + target + "\nPYTHOS:CORE:INTERVENING\n" + ordinal
            )

        assert_base_marker_order(prerequisite + "\n" + target + "\n" + ordinal)

    def test_every_repeated_marker_rejects_an_extra_before_ordinal_one(self) -> None:
        serial = self.valid_group_serial()
        for marker in GROUP_MARKERS:
            suffix = "0x00000000000000FF" if marker.endswith("=") else ""
            with self.subTest(marker=marker), self.assertRaises(AssertionError):
                assert_report_groups(marker + suffix + "\n" + serial)

    def test_every_repeated_marker_rejects_an_extra_after_terminal_fields(self) -> None:
        serial = self.valid_group_serial()
        for marker in GROUP_MARKERS:
            suffix = "0x00000000000000FF" if marker.endswith("=") else ""
            with self.subTest(marker=marker), self.assertRaises(AssertionError):
                assert_report_groups(serial + marker + suffix + "\n")

    def test_late_failures_with_overall_ready_cannot_be_accepted(self) -> None:
        failures = (
            PREFIX + "XHCI_DRIVER_ERROR:INTERRUPT_TRANSFER_TIMEOUT",
            PREFIX + "XHCI_BOOT_MOUSE_DECODE_INVALID",
            PREFIX + "XHCI_BOOT_MOUSE_TERMINAL_INVARIANT",
        )
        for failure in failures:
            with self.subTest(failure=failure), self.assertRaises(AssertionError):
                assert_forbidden_markers_absent(SUCCESS_MARKER + "\n" + failure)


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(
        RecurringOracleRegressionTest
    )
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


def main() -> int:
    build_probe_image()
    serial, _qemu_output = run_probe_boot()
    assert_base_marker_order(serial)
    INTERRUPT_HARNESS.ENDPOINT_HARNESS.assert_endpoint_configuration_result(serial)
    assert_report_groups(serial)
    assert_terminal_summary(serial)
    assert_forbidden_markers_absent(serial)
    print("USB_XHCI_BOOT_MOUSE_RECURRING_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    raise SystemExit(main())
