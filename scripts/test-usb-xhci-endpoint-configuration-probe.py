#!/usr/bin/env python
"""Acceptance test for bounded xHCI endpoint plus USB configuration."""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "usb-xhci-endpoint-configuration-probe-com1.log"
USB_BOOT_SIM_IMAGE = TARGET / "pythos-xhci-endpoint-configuration-boot-sim.img"
SUCCESS_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE_READY"
SWAP_READY_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE:SWAP_READY"
IGNORED_CHANGE_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_CHANGE"


def load_configuration_harness():
    path = ROOT / "scripts" / "test-usb-xhci-configuration-probe.py"
    spec = importlib.util.spec_from_file_location("xhci_configuration_harness", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CONFIGURATION_HARNESS = load_configuration_harness()
REQUIRED_MARKERS = CONFIGURATION_HARNESS.REQUIRED_MARKERS[:-3] + (
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENDPOINT_CONTEXT_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENDPOINT_ID=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENDPOINT_CONTEXT_INTERVAL=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURE_ENDPOINT_CC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURE_ENDPOINT_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SET_CONFIGURATION_CC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SET_CONFIGURATION_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURED_SLOT_STATE=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURED_ENDPOINT_STATE=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENDPOINT_CONFIGURATION_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES",
    SUCCESS_MARKER,
)
FORBIDDEN_MARKERS = tuple(
    marker
    for marker in CONFIGURATION_HARNESS.FORBIDDEN_MARKERS
    if marker
    not in (
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SET_CONFIGURATION",
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURE_ENDPOINT",
    )
) + (
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER",
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
            "usb-xhci-endpoint-configuration-probe",
        ]
    )
    CONFIGURATION_HARNESS.build_verified_user_shell()
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


def assert_forbidden_markers_absent(serial: str) -> None:
    for marker in FORBIDDEN_MARKERS:
        if marker in serial:
            raise AssertionError(
                f"endpoint configuration boot unexpectedly reached marker: {marker}"
            )


def extract_hex(serial: str, marker: str) -> int:
    match = re.search(re.escape(marker) + r"0x([0-9A-Fa-f]+)", serial)
    if not match:
        raise AssertionError(f"missing numeric marker: {marker}")
    return int(match.group(1), 16)


def assert_endpoint_configuration_result(serial: str) -> None:
    CONFIGURATION_HARNESS.assert_configuration_result(serial)
    expected_values = (
        ("XHCI_ENDPOINT_ID=", 3),
        ("XHCI_ENDPOINT_CONTEXT_INTERVAL=", 6),
        ("XHCI_CONFIGURE_ENDPOINT_CC=", 1),
        ("XHCI_SET_CONFIGURATION_CC=", 1),
        ("XHCI_CONFIGURED_SLOT_STATE=", 3),
        ("XHCI_CONFIGURED_ENDPOINT_STATE=", 1),
    )
    prefix = "PYTHOS:CORE:USB_XHCI_PROBE:"
    for suffix, expected in expected_values:
        actual = extract_hex(serial, prefix + suffix)
        if actual != expected:
            raise AssertionError(
                f"expected {suffix} to be {expected:#x}, got {actual:#x}"
            )


def main() -> int:
    build_probe_image()
    serial = run_probe_boot()
    assert_required_markers(serial)
    assert_forbidden_markers_absent(serial)
    assert_endpoint_configuration_result(serial)
    print("USB_XHCI_ENDPOINT_CONFIGURATION_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
