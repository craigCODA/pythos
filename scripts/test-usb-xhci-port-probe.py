#!/usr/bin/env python
"""Acceptance test for the no-write USB/xHCI port-status probe boot."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "usb-xhci-port-probe-com1.log"
SUCCESS_MARKER = "PYTHOS:CORE:USB_XHCI_PROBE_READY"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:USB_XHCI_PROBE:ENTER",
    "PYTHOS:CORE:USB_XHCI_PROBE:PCI_SCAN_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:USB_CONTROLLER_FOUND",
    "PYTHOS:CORE:USB_XHCI_PROBE:USB_KIND:XHCI",
    "PYTHOS:CORE:USB_XHCI_PROBE:SELECTED_XHCI",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:MMIO_VIRT=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_MMIO_MAPPED",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:HCSPARAMS1=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:HCCPARAMS1=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:USBCMD=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:USBSTS=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_REGISTERS_READY",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:MAX_PORTS=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_REGISTER_BASE=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:EXT_CAP_DWORD_OFFSET=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:EXT_CAP_BYTE_OFFSET=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:COUNT=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:NUMBER=0x0000000000000001",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:PORTSC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:PORTPMSC=",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_PORT_STATUS_READY",
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
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_RESET",
    "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_COMMAND_RING",
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
            "usb-xhci-port-probe",
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
            "20",
            "--no-audio-device",
            "--no-virtio-blk",
            "--xhci",
            "--usb-mouse",
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
            raise AssertionError(f"port probe boot unexpectedly reached marker: {marker}")


def assert_numeric_port_facts(serial: str) -> None:
    max_ports = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:MAX_PORTS=")
    captured_ports = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:COUNT=")
    port_base = extract_hex(serial, "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_REGISTER_BASE=")
    if max_ports <= 0:
        raise AssertionError(f"expected at least one xHCI port, got {max_ports}")
    if captured_ports <= 0 or captured_ports > 8:
        raise AssertionError(f"expected 1..8 captured ports, got {captured_ports}")
    if captured_ports > max_ports:
        raise AssertionError(f"captured ports {captured_ports} exceeds max ports {max_ports}")
    if port_base < 0x400:
        raise AssertionError(f"unexpected xHCI port register base: 0x{port_base:X}")
    if (
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:LEGACY_CAP_PRESENT" not in serial
        and "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:LEGACY_CAP_ABSENT" not in serial
    ):
        raise AssertionError("missing xHCI legacy-support ownership result")


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
    assert_numeric_port_facts(serial)
    print("USB_XHCI_PORT_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
