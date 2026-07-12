#!/usr/bin/env python
"""Build and verify PythOS boot serial markers."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SERIAL_LOG = ROOT / "target" / "boot-serial.log"
FAILURE_MARKERS = [
    "PYTHOS:LOADER:FAIL",
    "PYTHOS:PANIC",
    "PYTHOS:CORE:BOOTINFO_INVALID",
    "PYTHOS:CORE:MEMORY_INVALID",
]
SLICE_MARKERS = {
    "loader-enter": ["PYTHOS:LOADER:ENTER"],
    "gop-ready": [
        "PYTHOS:LOADER:ENTER",
        "PYTHOS:LOADER:GOP_READY",
    ],
    "kernel-loaded": [
        "PYTHOS:LOADER:ENTER",
        "PYTHOS:LOADER:GOP_READY",
        "PYTHOS:LOADER:KERNEL_LOADED",
    ],
    "memory-map-ready": [
        "PYTHOS:LOADER:ENTER",
        "PYTHOS:LOADER:GOP_READY",
        "PYTHOS:LOADER:KERNEL_LOADED",
        "PYTHOS:LOADER:MEMORY_MAP_READY",
    ],
    "milestone-1": [
        "PYTHOS:LOADER:ENTER",
        "PYTHOS:LOADER:GOP_READY",
        "PYTHOS:LOADER:KERNEL_LOADED",
        "PYTHOS:LOADER:MEMORY_MAP_READY",
        "PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK",
        "PYTHOS:CORE:ENTER",
        "PYTHOS:CORE:BOOTINFO_VALID",
        "PYTHOS:CORE:MEMORY_READY",
        "PYTHOS:CORE:GDT_READY",
        "PYTHOS:CORE:IDT_READY",
        "PYTHOS:CORE:FRAMEBUFFER_READY",
        "PYTHOS:CORE:MILESTONE_1_COMPLETE",
    ],
}


def run(command: list[str]) -> None:
    print("+ " + " ".join(command))
    subprocess.run(command, cwd=ROOT, check=True)


def assert_markers(serial: str, expected: list[str]) -> None:
    for failure in FAILURE_MARKERS:
        if failure in serial:
            raise AssertionError(f"failure marker present: {failure}")

    cursor = 0
    for marker in expected:
        index = serial.find(marker, cursor)
        if index == -1:
            raise AssertionError(f"missing or out-of-order marker: {marker}\nserial:\n{serial}")
        cursor = index + len(marker)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--slice", choices=sorted(SLICE_MARKERS), default="loader-enter")
    args = parser.parse_args()

    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none"])
    run([sys.executable, "scripts/build-image.py"])
    run([sys.executable, "scripts/run-qemu.py", "--serial-log", str(SERIAL_LOG)])

    serial = SERIAL_LOG.read_text(encoding="utf-8", errors="replace")
    assert_markers(serial, SLICE_MARKERS[args.slice])
    print("BOOT_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
