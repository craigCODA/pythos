#!/usr/bin/env python
"""Acceptance test for the USB/xHCI device descriptor diagnostic."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "usb-xhci-descriptor-probe-com1.log"
USB_BOOT_SIM_IMAGE = TARGET / "pythos-xhci-descriptor-boot-sim.img"
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
            "usb-xhci-descriptor-probe",
        ]
    )
    build_verified_user_shell()
    run([sys.executable, "scripts/build-image.py"])


def run_probe_boot() -> str:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    run(
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
            raise AssertionError(f"descriptor probe boot unexpectedly reached marker: {marker}")


def assert_descriptor_result(serial: str) -> None:
    address_cc = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_CC=")
    descriptor_cc = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_TRANSFER_CC=")
    length = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_LENGTH=")
    descriptor_type = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_TYPE=")
    usb_bcd = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_USB_BCD=")
    mps0 = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_MPS0=")
    vendor = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_VENDOR=")
    product = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_PRODUCT=")
    config_count = extract_hex(
        serial,
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_CONFIG_COUNT=",
    )
    if address_cc != 1:
        raise AssertionError(f"expected successful Address Device completion, got {address_cc}")
    if descriptor_cc != 1:
        raise AssertionError(f"expected successful descriptor transfer, got {descriptor_cc}")
    if length != 18:
        raise AssertionError(f"expected 18-byte device descriptor, got {length}")
    if descriptor_type != 1:
        raise AssertionError(f"expected device descriptor type 1, got {descriptor_type}")
    if usb_bcd == 0:
        raise AssertionError("expected nonzero USB BCD")
    if mps0 not in (8, 16, 32, 64):
        raise AssertionError(f"unexpected endpoint-zero max packet size: {mps0}")
    if vendor == 0 or product == 0:
        raise AssertionError(f"expected nonzero VID/PID, got {vendor:04x}:{product:04x}")
    if config_count == 0:
        raise AssertionError("expected at least one USB configuration")


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
    assert_descriptor_result(serial)
    print("USB_XHCI_DESCRIPTOR_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
