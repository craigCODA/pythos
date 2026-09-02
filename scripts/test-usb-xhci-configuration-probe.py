#!/usr/bin/env python
"""Acceptance test for the bounded USB/xHCI configuration descriptor probe."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "usb-xhci-configuration-probe-com1.log"
USB_BOOT_SIM_IMAGE = TARGET / "pythos-xhci-configuration-boot-sim.img"
SUCCESS_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE_READY"
SWAP_READY_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE:SWAP_READY"
IGNORED_CHANGE_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_CHANGE"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:USB_XHCI_PROBE:ENTER",
    "PYTHOS:CORE:USB_XHCI_PROBE:PCI_SCAN_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:USB_CONTROLLER_FOUND",
    "PYTHOS:CORE:USB_XHCI_PROBE:USB_KIND:XHCI",
    "PYTHOS:CORE:USB_XHCI_PROBE:SELECTED_XHCI",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_MMIO_MAPPED",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_REGISTERS_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_PORT_STATUS_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_SWAP_READY",
    SWAP_READY_MARKER,
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_START",
    IGNORED_CHANGE_MARKER,
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_ATTEMPT=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:FOUND",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:NUMBER=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:BEFORE_PORTSC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:AFTER_PORTSC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_CHANGED",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_START",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONTROLLER_RESET_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_COMMAND_RING_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_EVENT_RING_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_USBSTS=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_PORT_RESET_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_PORT=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_PORTSC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_NOOP_CC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_NOOP_COMMAND_COMPLETE",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENABLE_SLOT_CC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SLOT_ID=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENABLE_SLOT_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_CONTEXT_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_CONTEXT_SIZE=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_PORT_SPEED=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_MPS=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_CC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DEVICE_ADDRESS=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SLOT_STATE=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_EP0_STATE=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_TRANSFER_CC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_LENGTH=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_TYPE=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_USB_BCD=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_CLASS=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_SUBCLASS=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_PROTOCOL=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_MPS0=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_VENDOR=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_PRODUCT=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_DEVICE_BCD=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_CONFIG_COUNT=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_HEADER_TRANSFER_CC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_HEADER_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_TOTAL_LENGTH=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_VALUE=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_COUNT=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_TRANSFER_CC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_NUMBER=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_ALTERNATE_SETTING=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_ENDPOINT_COUNT=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_CLASS=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_SUBCLASS=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_PROTOCOL=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_ENDPOINT=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_ATTRIBUTES=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_MPS=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_INTERVAL=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES",
    SUCCESS_MARKER,
)
FORBIDDEN_MARKERS = (
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED",
    "PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY",
    "PYTHOS:CORE:OBJECT_STORE:PERSISTED",
    "PYTHOS:SHELL:RING3_ENTER",
    "PYTHOS:CORE:HARDWARE_PROBE:ENTER",
    "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_NON_SUCCESS",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_NON_SUCCESS",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SET_CONFIGURATION",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURE_ENDPOINT",
    "PYTHOS:CORE:USB_XHCI_PROBE:HID_REPORT",
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


def build_verified_user_shell() -> None:
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])


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
            "usb-xhci-configuration-probe",
        ]
    )
    build_verified_user_shell()
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
            raise AssertionError(f"configuration probe boot unexpectedly reached marker: {marker}")


def assert_configuration_result(serial: str) -> None:
    expected_values = (
        ("XHCI_ADDRESS_DEVICE_CC=", 1),
        ("XHCI_DESCRIPTOR_TRANSFER_CC=", 1),
        ("XHCI_DESCRIPTOR_CONFIG_COUNT=", 1),
        ("XHCI_CONFIGURATION_HEADER_TRANSFER_CC=", 1),
        ("XHCI_CONFIGURATION_TOTAL_LENGTH=", 34),
        ("XHCI_CONFIGURATION_VALUE=", 1),
        ("XHCI_CONFIGURATION_INTERFACE_COUNT=", 1),
        ("XHCI_CONFIGURATION_TRANSFER_CC=", 1),
        ("XHCI_CONFIGURATION_INTERFACE_NUMBER=", 0),
        ("XHCI_CONFIGURATION_ALTERNATE_SETTING=", 0),
        ("XHCI_CONFIGURATION_ENDPOINT_COUNT=", 1),
        ("XHCI_CONFIGURATION_INTERFACE_CLASS=", 3),
        ("XHCI_CONFIGURATION_INTERFACE_SUBCLASS=", 1),
        ("XHCI_CONFIGURATION_INTERFACE_PROTOCOL=", 2),
        ("XHCI_CONFIGURATION_INTERRUPT_IN_ENDPOINT=", 0x81),
        ("XHCI_CONFIGURATION_INTERRUPT_IN_ATTRIBUTES=", 0x03),
        ("XHCI_CONFIGURATION_INTERRUPT_IN_MPS=", 4),
        ("XHCI_CONFIGURATION_INTERRUPT_IN_INTERVAL=", 7),
    )
    prefix = "PYTHOS:CORE:USB_XHCI_PROBE:"
    for suffix, expected in expected_values:
        actual = extract_hex(serial, prefix + suffix)
        if actual != expected:
            raise AssertionError(
                f"expected {suffix} to be {expected:#x}, got {actual:#x}"
            )


def extract_hex(serial: str, marker: str) -> int:
    match = re.search(re.escape(marker) + r"0x([0-9A-Fa-f]+)", serial)
    if not match:
        raise AssertionError(f"missing numeric marker: {marker}")
    return int(match.group(1), 16)


def main() -> int:
    build_probe_image()
    serial = run_probe_boot()
    assert_required_markers(serial)
    assert_forbidden_markers_absent(serial)
    assert_configuration_result(serial)
    print("USB_XHCI_CONFIGURATION_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
