#!/usr/bin/env python
"""Acceptance test for the gated eMMC single-sector write probe."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "hardware-probe-emmc-write-com1.log"
EMMC_WRITE_IMAGE = TARGET / "hardware-probe-emmc-write.img"
EMMC_WRITE_IMAGE_SIZE_BYTES = 32 * 1024 * 1024
EMMC_WRITE_TEST_LBA = 2048
EMMC_WRITE_TEST_BLOCK_LEN = 512
EMMC_WRITE_TEST_MAGIC = b"PYTHOS_EMMC_WR00"
SUCCESS_MARKER = "PYTHOS:CORE:HARDWARE_PROBE_READY"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:HARDWARE_PROBE:ENTER",
    "PYTHOS:CORE:HARDWARE_PROBE:DISK_WRITE_TEST_ARMED",
    "PYTHOS:CORE:HARDWARE_PROBE:PCI_SCAN_READY",
    "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_CONTROLLER_FOUND",
    "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:VIRTIO_LEGACY_BLOCK",
    "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:SDHCI_EMMC_CANDIDATE",
    "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_REGISTERS_READY",
    "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT_READY",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC:OCR=",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC:RCA=",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_IDENTIFICATION_READY",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:LBA=0x0000000000000000",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ONLY_BLOCK_READY",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE:LBA=0x0000000000000800",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE:BLOCK_LEN=0x0000000000000200",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE:FIRST_DWORD=0x0000000048545950",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE:CHECKSUM=0x000000000000FBD8",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_READBACK:FIRST_DWORD=0x0000000048545950",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_READBACK:CHECKSUM=0x000000000000FBD8",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_READBACK:NONZERO_BYTES=0x00000000000001FE",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_READBACK_MATCH_READY",
    "PYTHOS:CORE:HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY",
    SUCCESS_MARKER,
)
FORBIDDEN_MARKERS = (
    "PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES",
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED",
    "PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY",
    "PYTHOS:CORE:OBJECT_STORE:PERSISTED",
    "PYTHOS:SHELL:RING3_ENTER",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:",
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
            "hardware-probe-emmc-write",
        ]
    )
    build_verified_user_shell()
    run([sys.executable, "scripts/build-image.py"])


def emmc_write_test_pattern() -> bytes:
    pattern = bytearray(EMMC_WRITE_TEST_BLOCK_LEN)
    for index in range(EMMC_WRITE_TEST_BLOCK_LEN):
        if index < len(EMMC_WRITE_TEST_MAGIC):
            pattern[index] = EMMC_WRITE_TEST_MAGIC[index]
        else:
            pattern[index] = ((index & 0xFF) * 37 + 0x5A) & 0xFF
    return bytes(pattern)


def prepare_emmc_write_image() -> None:
    TARGET.mkdir(parents=True, exist_ok=True)
    with EMMC_WRITE_IMAGE.open("wb") as image:
        image.truncate(EMMC_WRITE_IMAGE_SIZE_BYTES)


def run_probe_boot() -> str:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    prepare_emmc_write_image()
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
            "--emmc-image",
            str(EMMC_WRITE_IMAGE),
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


def emmc_ocr(serial: str) -> int:
    marker = "PYTHOS:CORE:HARDWARE_PROBE:EMMC:OCR=0x"
    start = serial.find(marker)
    if start == -1:
        raise AssertionError("missing eMMC OCR marker")
    start += len(marker)
    return int(serial[start : start + 16], 16)


def emmc_image_offset_for_command_address(serial: str) -> int:
    ocr = emmc_ocr(serial)
    high_capacity_or_sector_mode = (ocr & (1 << 30)) != 0
    if high_capacity_or_sector_mode:
        return EMMC_WRITE_TEST_LBA * EMMC_WRITE_TEST_BLOCK_LEN
    return EMMC_WRITE_TEST_LBA


def assert_image_contains_write_pattern(serial: str) -> None:
    expected = emmc_write_test_pattern()
    offset = emmc_image_offset_for_command_address(serial)
    with EMMC_WRITE_IMAGE.open("rb") as image:
        image.seek(offset)
        actual = image.read(EMMC_WRITE_TEST_BLOCK_LEN)
    if actual != expected:
        for index, (actual_byte, expected_byte) in enumerate(zip(actual, expected)):
            if actual_byte != expected_byte:
                raise AssertionError(
                    "eMMC image mismatch at "
                    f"LBA {EMMC_WRITE_TEST_LBA} byte {index}: "
                    f"got 0x{actual_byte:02X}, expected 0x{expected_byte:02X}"
                )
        raise AssertionError(
            f"eMMC image offset {offset} length mismatch: "
            f"got {len(actual)}, expected {len(expected)}"
        )


def main() -> int:
    build_probe_image()
    serial = run_probe_boot()
    assert_required_markers(serial)
    assert_forbidden_markers_absent(serial)
    assert_image_contains_write_pattern(serial)
    print("EMMC_WRITE_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
