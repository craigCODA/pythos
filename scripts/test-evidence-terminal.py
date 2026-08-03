#!/usr/bin/env python
"""Acceptance test for the opt-in physical evidence terminal."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
ISO = TARGET / "pythos.iso"
SERIAL_LOG = TARGET / "evidence-terminal.log"
SCREENDUMP = TARGET / "evidence-terminal.ppm"
EMMC_IMAGE = TARGET / "evidence-terminal-emmc-store.img"
STORAGE_SIZE_BYTES = 32 * 1024 * 1024
REQUIRED_MARKERS = (
    "PYTHOS:LOADER:ENTER",
    "PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK",
    "PYTHOS:CORE:BOOTINFO_VALID",
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC",
    "PYTHOS:CORE:OBJECT_STORE:PERSISTED",
    "PYTHOS:CORE:OBJECT_STORE:RESTORED",
    "PYTHOS:CORE:GENERAL_STORAGE:PERSISTED",
    "PYTHOS:CORE:GENERAL_STORAGE:RESTORED",
    "PYTHOS:CORE:PHASE_10_COMPLETE",
    "PYTHOS:CORE:FRAMEBUFFER_READY",
    "PYTHOS:CORE:MILESTONE_1_COMPLETE",
    "PYTHOS:CORE:EVIDENCE_TERMINAL_READY",
)
FORBIDDEN_MARKERS = (
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO",
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI",
    "PYTHOS:PANIC",
)


def run(command: list[str], expected_returncode: int = 0) -> str:
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
    if result.returncode != expected_returncode:
        raise AssertionError(
            f"expected return code {expected_returncode}, got {result.returncode}"
        )
    return result.stdout


def build_boot_iso() -> None:
    run(
        [
            "cargo",
            "build",
            "-p",
            "pythos-boot",
            "--target",
            "x86_64-unknown-uefi",
            "--features",
            "evidence-terminal",
        ]
    )
    run(
        [
            "cargo",
            "build",
            "-p",
            "pythos-core",
            "--target",
            "x86_64-unknown-none",
            "--features",
            "verify,sdhci-emmc-backend,evidence-terminal",
        ]
    )
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-iso.py"])


def prepare_fresh_image(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    with path.open("wb") as image:
        image.truncate(STORAGE_SIZE_BYTES)


def read_serial_log() -> str:
    return SERIAL_LOG.read_text(encoding="utf-8", errors="replace")


def run_evidence_terminal_boot() -> str:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    if SCREENDUMP.exists():
        SCREENDUMP.unlink()
    return run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--iso",
            str(ISO),
            "--serial-log",
            str(SERIAL_LOG),
            "--timeout",
            "75",
            "--screendump",
            str(SCREENDUMP),
            "--success-marker",
            "PYTHOS:CORE:EVIDENCE_TERMINAL_READY",
            "--no-virtio-blk",
            "--sdhci",
            "--emmc",
            "--emmc-image",
            str(EMMC_IMAGE),
            "--expect-outcome",
            "success",
        ]
    )


def assert_required_markers_ordered(serial: str) -> None:
    cursor = 0
    for marker in REQUIRED_MARKERS:
        position = serial.find(marker, cursor)
        if position < 0:
            raise AssertionError(f"missing ordered marker {marker}")
        cursor = position + len(marker)


def assert_forbidden_markers_absent(serial: str) -> None:
    for marker in FORBIDDEN_MARKERS:
        if marker in serial:
            raise AssertionError(f"unexpected fallback/panic marker: {marker}")
    if "PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED" in serial:
        raise AssertionError("evidence terminal dropped transcript lines")


def assert_screendump_written() -> None:
    if not SCREENDUMP.exists():
        raise AssertionError(f"missing screendump {SCREENDUMP}")
    if SCREENDUMP.stat().st_size == 0:
        raise AssertionError(f"empty screendump {SCREENDUMP}")


def main() -> int:
    build_boot_iso()
    prepare_fresh_image(EMMC_IMAGE)

    output = run_evidence_terminal_boot()
    if "QEMU_OUTCOME success" not in output:
        raise AssertionError("missing QEMU_OUTCOME success")
    serial = read_serial_log()
    assert_required_markers_ordered(serial)
    assert_forbidden_markers_absent(serial)
    assert_screendump_written()

    print("EVIDENCE_TERMINAL_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
