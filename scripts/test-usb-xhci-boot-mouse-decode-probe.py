#!/usr/bin/env python
"""Acceptance test for one decoded USB HID boot-mouse report."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "usb-xhci-boot-mouse-decode-probe-com1.log"


def load_interrupt_harness():
    path = ROOT / "scripts" / "test-usb-xhci-interrupt-transfer-probe.py"
    spec = importlib.util.spec_from_file_location("xhci_interrupt_harness", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


INTERRUPT_HARNESS = load_interrupt_harness()
INTERRUPT_HARNESS.SERIAL_LOG = SERIAL_LOG

PREFIX = "PYTHOS:CORE:USB_XHCI_PROBE:"
DECODE_MARKERS = (
    PREFIX + "XHCI_BOOT_MOUSE_BUTTONS=",
    PREFIX + "XHCI_BOOT_MOUSE_DX_I8=",
    PREFIX + "XHCI_BOOT_MOUSE_DY_I8=",
    PREFIX + "XHCI_BOOT_MOUSE_AUX_PRESENT=",
    PREFIX + "XHCI_BOOT_MOUSE_AUX=",
    PREFIX + "XHCI_BOOT_MOUSE_EVENT_READY",
    PREFIX + "XHCI_BOOT_MOUSE_DECODE_READY",
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
            "usb-xhci-boot-mouse-decode-probe",
        ]
    )
    INTERRUPT_HARNESS.ENDPOINT_HARNESS.CONFIGURATION_HARNESS.build_verified_user_shell()
    INTERRUPT_HARNESS.run([sys.executable, "scripts/build-image.py"])


def assert_decode_result(serial: str) -> None:
    expected = {
        PREFIX + "XHCI_BOOT_MOUSE_BUTTONS=": 0x00,
        PREFIX + "XHCI_BOOT_MOUSE_DX_I8=": 0x08,
        PREFIX + "XHCI_BOOT_MOUSE_DY_I8=": 0xFC,
        PREFIX + "XHCI_BOOT_MOUSE_AUX_PRESENT=": 0x01,
        PREFIX + "XHCI_BOOT_MOUSE_AUX=": 0x00,
    }
    for marker, value in expected.items():
        actual = INTERRUPT_HARNESS.extract_hex(serial, marker)
        if actual != value:
            raise AssertionError(f"expected {marker}{value:#x}, got {actual:#x}")

    cursor = serial.find(PREFIX + "XHCI_INTERRUPT_TRANSFER_READY")
    if cursor == -1:
        raise AssertionError("missing raw transfer boundary")
    for marker in DECODE_MARKERS:
        next_cursor = serial.find(marker, cursor)
        if next_cursor == -1:
            raise AssertionError(f"missing or out-of-order decode marker: {marker}")
        cursor = next_cursor + len(marker)
        if serial.count(marker) != 1:
            raise AssertionError(f"expected exactly one {marker}")

    framebuffer = serial.find(
        PREFIX + "FRAMEBUFFER_IDENTITY_READY",
        cursor,
    )
    if framebuffer == -1:
        raise AssertionError("decoded result was not rendered after decode")
    if PREFIX + "XHCI_BOOT_MOUSE_DECODE_INVALID" in serial:
        raise AssertionError("valid four-byte report reached decode failure")
    if "PYTHOS:CORE:POINTER_CURSOR_READY" in serial:
        raise AssertionError("decode-only boundary unexpectedly enabled a cursor")


def main() -> int:
    build_probe_image()
    serial = INTERRUPT_HARNESS.run_probe_boot()
    INTERRUPT_HARNESS.assert_required_markers(serial)
    INTERRUPT_HARNESS.ENDPOINT_HARNESS.assert_endpoint_configuration_result(serial)
    INTERRUPT_HARNESS.assert_transfer_result(serial)
    assert_decode_result(serial)
    print("USB_XHCI_BOOT_MOUSE_DECODE_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
