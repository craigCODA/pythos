#!/usr/bin/env python
"""Acceptance test for the probe-only real-hardware storage discovery boot."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "hardware-probe-com1.log"
SUCCESS_MARKER = "PYTHOS:CORE:HARDWARE_PROBE_READY"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:HARDWARE_PROBE:ENTER",
    "PYTHOS:CORE:HARDWARE_PROBE:PCI_SCAN_READY",
    "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_CONTROLLER_FOUND",
    "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:VIRTIO_LEGACY_BLOCK",
    "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:SDHCI_EMMC_CANDIDATE",
    "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_REGISTERS_READY",
    "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT_READY",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC:OCR=",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC:RCA=",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC:CID0=",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC:CSD0=",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_IDENTIFICATION_READY",
    "PYTHOS:CORE:HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY",
    "PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES",
    SUCCESS_MARKER,
)
FORBIDDEN_MARKERS = (
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED",
    "PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY",
    "PYTHOS:CORE:OBJECT_STORE:PERSISTED",
    "PYTHOS:SHELL:RING3_ENTER",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:",
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
            "hardware-probe",
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
            "--sdhci",
            "--emmc",
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
            raise AssertionError(f"probe boot unexpectedly reached marker: {marker}")


def main() -> int:
    build_probe_image()
    serial = run_probe_boot()
    assert_required_markers(serial)
    assert_forbidden_markers_absent(serial)
    print("HARDWARE_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
