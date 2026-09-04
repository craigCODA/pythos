#!/usr/bin/env python
"""Acceptance test for one bounded raw xHCI interrupt-IN report."""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "usb-xhci-interrupt-transfer-probe-com1.log"
USB_BOOT_SIM_IMAGE = TARGET / "pythos-xhci-interrupt-transfer-boot-sim.img"
SUCCESS_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE_READY"
SWAP_READY_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE:SWAP_READY"
IGNORED_CHANGE_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_CHANGE"
TRANSFER_ARMED_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_ARMED"


def load_endpoint_harness():
    path = ROOT / "scripts" / "test-usb-xhci-endpoint-configuration-probe.py"
    spec = importlib.util.spec_from_file_location("xhci_endpoint_harness", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ENDPOINT_HARNESS = load_endpoint_harness()
REQUIRED_MARKERS = ENDPOINT_HARNESS.REQUIRED_MARKERS[:-3] + (
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_REQUESTED=",
    TRANSFER_ARMED_MARKER,
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_CC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_ACTUAL=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_CAPTURED=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_RAW=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES",
    SUCCESS_MARKER,
)
FORBIDDEN_MARKERS = tuple(
    marker
    for marker in ENDPOINT_HARNESS.FORBIDDEN_MARKERS
    if marker != "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER"
) + (
    "PYTHOS:CORE:USB_XHCI_PROBE:HID_REPORT",
    "PYTHOS:CORE:POINTER_CURSOR_READY",
)


def run(command: list[str]) -> str:
    print("+ " + " ".join(command))
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout)
    if result.returncode != 0:
        raise AssertionError(f"{command} failed with {result.returncode}")
    return result.stdout


def build_probe_image() -> None:
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
            "usb-xhci-interrupt-transfer-probe",
        ]
    )
    ENDPOINT_HARNESS.CONFIGURATION_HARNESS.build_verified_user_shell()
    run([sys.executable, "scripts/build-image.py"])


def run_probe_boot() -> str:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    qemu_output = run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--success-marker",
            SUCCESS_MARKER,
            "--timeout",
            "30",
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
            "--move-usb-mouse-after-marker",
            TRANSFER_ARMED_MARKER,
            "--expect-outcome",
            "success",
        ]
    )
    if "QEMU_OUTCOME success" not in qemu_output:
        raise AssertionError("missing QEMU_OUTCOME success")
    return SERIAL_LOG.read_text(encoding="utf-8", errors="replace")


def assert_required_markers(serial: str) -> None:
    cursor = 0
    for marker in REQUIRED_MARKERS:
        index = serial.find(marker, cursor)
        if index == -1:
            raise AssertionError(f"missing or out-of-order marker: {marker}\nserial:\n{serial}")
        cursor = index + len(marker)


def extract_hex(serial: str, marker: str) -> int:
    match = re.search(re.escape(marker) + r"0x([0-9A-Fa-f]+)", serial)
    if not match:
        raise AssertionError(f"missing numeric marker: {marker}")
    return int(match.group(1), 16)


def assert_transfer_result(serial: str) -> None:
    prefix = "PYTHOS:CORE:USB_XHCI_PROBE:"
    requested = extract_hex(serial, prefix + "XHCI_INTERRUPT_TRANSFER_REQUESTED=")
    completion = extract_hex(serial, prefix + "XHCI_INTERRUPT_TRANSFER_CC=")
    actual = extract_hex(serial, prefix + "XHCI_INTERRUPT_TRANSFER_ACTUAL=")
    captured = extract_hex(serial, prefix + "XHCI_INTERRUPT_TRANSFER_CAPTURED=")
    raw = extract_hex(serial, prefix + "XHCI_INTERRUPT_TRANSFER_RAW=")
    if requested != 4:
        raise AssertionError(f"expected four-byte mouse request, got {requested}")
    if completion not in (1, 13):
        raise AssertionError(f"unexpected transfer completion code {completion:#x}")
    if not 0 < actual <= requested:
        raise AssertionError(f"invalid received length {actual}")
    if captured != actual:
        raise AssertionError(f"captured {captured} bytes from {actual}-byte report")
    if raw == 0:
        raise AssertionError("mouse movement produced an all-zero raw report")
    for marker in FORBIDDEN_MARKERS:
        if marker in serial:
            raise AssertionError(f"raw report boundary unexpectedly reached marker: {marker}")
    for marker in (TRANSFER_ARMED_MARKER, prefix + "XHCI_INTERRUPT_TRANSFER_READY"):
        if serial.count(marker) != 1:
            raise AssertionError(f"expected exactly one {marker}")


def main() -> int:
    build_probe_image()
    serial = run_probe_boot()
    assert_required_markers(serial)
    ENDPOINT_HARNESS.assert_endpoint_configuration_result(serial)
    assert_transfer_result(serial)
    print("USB_XHCI_INTERRUPT_TRANSFER_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
