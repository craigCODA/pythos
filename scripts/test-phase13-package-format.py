#!/usr/bin/env python
"""Phase 13 package-format acceptance: source ingress and parser denial."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
CORE_TARGET = TARGET / "phase13-package-format-core"
CORE_ELF = CORE_TARGET / "x86_64-unknown-none" / "debug" / "pythcore"
SERIAL_LOG = TARGET / "phase13-package-format.log"
STORAGE_IMAGE = TARGET / "phase13-package-format.img"

REQUIRED_MARKERS = (
    "PYTHOS:CORE:PACKAGE_SOURCE_READY",
    "PYTHOS:CORE:PACKAGE_FORMAT:VALID",
    "PYTHOS:CORE:PACKAGE_FORMAT:INVALID_DENIED",
    "PYTHOS:CORE:PACKAGE_FORMAT_READY",
)
FORBIDDEN_MARKERS = (
    "PYTHOS:LOADER:FAIL",
    "PYTHOS:PANIC",
    "PYTHOS:CORE:PACKAGE_INSTALL:STAGED",
    "PYTHOS:CORE:PACKAGE_INSTALL:COMMITTED",
    "PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED",
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
        raise AssertionError(f"{command} returned {result.returncode}")
    return result.stdout


def build_verified_user_shell() -> None:
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])


def build_boot_image() -> None:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(
        [
            "cargo",
            "build",
            "-p",
            "pythos-core",
            "--target",
            "x86_64-unknown-none",
            "--target-dir",
            str(CORE_TARGET),
            "--features",
            "verify,phase13-package-test",
        ]
    )
    build_verified_user_shell()
    run(
        [
            sys.executable,
            "scripts/build-image.py",
            "--kernel",
            str(CORE_ELF),
            "--with-phase13-package-format-fixture",
        ]
    )


def run_qemu() -> str:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    return run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--storage-image",
            str(STORAGE_IMAGE),
            "--timeout",
            "60",
            "--success-marker",
            "PYTHOS:CORE:PACKAGE_FORMAT_READY",
            "--expect-outcome",
            "success",
        ]
    )


def serial_lines(output: str) -> list[str]:
    return [line.strip() for line in output.splitlines() if line.strip()]


def assert_ordered_markers(lines: list[str], markers: tuple[str, ...]) -> None:
    cursor = -1
    for marker in markers:
        matches = [index for index, line in enumerate(lines) if line == marker]
        if len(matches) != 1:
            raise AssertionError(f"expected one {marker!r}, saw {len(matches)}")
        if matches[0] <= cursor:
            raise AssertionError(f"out-of-order marker {marker!r}")
        cursor = matches[0]


def reject_forbidden(lines: list[str]) -> None:
    for marker in FORBIDDEN_MARKERS:
        if any(marker in line for line in lines):
            raise AssertionError(f"forbidden marker {marker}")


def main() -> int:
    build_boot_image()
    output = run_qemu()
    lines = serial_lines(output)
    reject_forbidden(lines)
    assert_ordered_markers(lines, REQUIRED_MARKERS)
    if "QEMU_OUTCOME success" not in output:
        raise AssertionError("missing QEMU_OUTCOME success")
    print("PHASE13_PACKAGE_FORMAT_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
