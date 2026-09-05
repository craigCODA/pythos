#!/usr/bin/env python
"""Acceptance test for the USB/xHCI Address Device diagnostic."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "usb-xhci-address-probe-com1.log"
USB_BOOT_SIM_IMAGE = TARGET / "pythos-xhci-address-boot-sim.img"
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
            "usb-xhci-address-probe",
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
            raise AssertionError(f"address probe boot unexpectedly reached marker: {marker}")


def assert_address_driver_result(serial: str) -> None:
    port_number = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_PORT=")
    portsc = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_PORTSC=")
    noop_cc = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_NOOP_CC=")
    enable_slot_cc = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENABLE_SLOT_CC=")
    slot_id = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SLOT_ID=")
    context_size = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_CONTEXT_SIZE=")
    port_speed = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_PORT_SPEED=")
    max_packet_size = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_MPS=")
    address_cc = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_CC=")
    device_address = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DEVICE_ADDRESS=")
    slot_state = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SLOT_STATE=")
    ep0_state = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_EP0_STATE=")
    if port_number <= 0:
        raise AssertionError(f"expected nonzero driver port, got {port_number}")
    if portsc & 0x3 != 0x3:
        raise AssertionError(f"expected connected and enabled post-reset port, got 0x{portsc:X}")
    if noop_cc != 1:
        raise AssertionError(f"expected successful no-op completion, got {noop_cc}")
    if enable_slot_cc != 1:
        raise AssertionError(f"expected successful enable-slot completion, got {enable_slot_cc}")
    if slot_id <= 0:
        raise AssertionError(f"expected nonzero slot id, got {slot_id}")
    if context_size not in (32, 64):
        raise AssertionError(f"expected 32- or 64-byte contexts, got {context_size}")
    if port_speed <= 0:
        raise AssertionError(f"expected defined post-reset port speed, got {port_speed}")
    if max_packet_size not in (8, 64, 512):
        raise AssertionError(f"unexpected default control MPS: {max_packet_size}")
    if address_cc != 1:
        raise AssertionError(f"expected successful address-device completion, got {address_cc}")
    if device_address <= 0:
        raise AssertionError(f"expected assigned USB device address, got {device_address}")
    if slot_state != 2:
        raise AssertionError(f"expected addressed slot state 2, got {slot_state}")
    if ep0_state != 1:
        raise AssertionError(f"expected running EP0 state 1, got {ep0_state}")


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
    assert_address_driver_result(serial)
    print("USB_XHCI_ADDRESS_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
