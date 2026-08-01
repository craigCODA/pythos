#!/usr/bin/env python
"""Acceptance test for the polling SDHCI/eMMC block-device backend."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
ISO = TARGET / "pythos.iso"
SERIAL_LOG = TARGET / "sdhci-emmc-block-device.log"
EMMC_IMAGE = TARGET / "sdhci-emmc-store.img"
SECTOR_SIZE = 512
STORAGE_SIZE_BYTES = 32 * 1024 * 1024
OBJECT_SNAPSHOT_SECTOR = 31
GENERAL_SNAPSHOT_SECTOR = 41
OBJECT_SNAPSHOT_MAGIC = b"PY7OBJ01"
GENERAL_SNAPSHOT_MAGIC = b"PY10OBJ1"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:BLOCK:SDHCI_EMMC_CONTROLLER_FOUND",
    "PYTHOS:CORE:BLOCK:SDHCI_EMMC_CARD_READY",
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC",
    "PYTHOS:CORE:BLOCK_DEVICE_READY",
    "PYTHOS:CORE:OBJECT_STORE:PERSISTED",
    "PYTHOS:CORE:OBJECT_STORE:RESTORED",
    "PYTHOS:CORE:GENERAL_STORAGE:PERSISTED",
    "PYTHOS:CORE:GENERAL_STORAGE:RESTORED",
    "PYTHOS:CORE:PHASE_10_COMPLETE",
    "PYTHOS:CORE:MILESTONE_1_COMPLETE",
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


def build_verified_user_shell() -> None:
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])


def build_boot_iso() -> None:
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
            "verify,sdhci-emmc-backend",
        ]
    )
    build_verified_user_shell()
    run([sys.executable, "scripts/build-iso.py"])


def prepare_fresh_image(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    with path.open("wb") as image:
        image.truncate(STORAGE_SIZE_BYTES)


def run_sdhci_emmc_boot() -> str:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    return run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--iso",
            str(ISO),
            "--serial-log",
            str(SERIAL_LOG),
            "--timeout",
            "60",
            "--no-virtio-blk",
            "--sdhci",
            "--emmc",
            "--emmc-image",
            str(EMMC_IMAGE),
            "--expect-outcome",
            "success",
        ]
    )


def assert_contains(output: str, marker: str) -> None:
    if marker not in output:
        raise AssertionError(f"missing marker {marker}")


def assert_required_markers(output: str) -> None:
    for marker in REQUIRED_MARKERS:
        assert_contains(output, marker)


def assert_forbidden_markers_absent(output: str) -> None:
    for marker in FORBIDDEN_MARKERS:
        if marker in output:
            raise AssertionError(f"unexpected fallback/panic marker: {marker}")


def read_sector(path: Path, sector: int) -> bytes:
    with path.open("rb") as image:
        image.seek(sector * SECTOR_SIZE)
        return image.read(SECTOR_SIZE)


def assert_persistent_signatures_on_emmc_image() -> None:
    object_sector = read_sector(EMMC_IMAGE, OBJECT_SNAPSHOT_SECTOR)
    general_sector = read_sector(EMMC_IMAGE, GENERAL_SNAPSHOT_SECTOR)
    if not object_sector.startswith(OBJECT_SNAPSHOT_MAGIC):
        raise AssertionError(
            f"sector {OBJECT_SNAPSHOT_SECTOR} does not start with {OBJECT_SNAPSHOT_MAGIC!r}"
        )
    if not general_sector.startswith(GENERAL_SNAPSHOT_MAGIC):
        raise AssertionError(
            f"sector {GENERAL_SNAPSHOT_SECTOR} does not start with {GENERAL_SNAPSHOT_MAGIC!r}"
        )


def main() -> int:
    build_boot_iso()
    prepare_fresh_image(EMMC_IMAGE)

    first = run_sdhci_emmc_boot()
    assert_required_markers(first)
    assert_forbidden_markers_absent(first)

    second = run_sdhci_emmc_boot()
    assert_required_markers(second)
    assert_forbidden_markers_absent(second)
    if "PYTHOS:CORE:OBJECT_STORE:CREATED" in second:
        raise AssertionError("second eMMC boot recreated object store instead of restoring it")
    assert_contains(second, "PYTHOS:CORE:OBJECT_STORE:RESTORED")

    assert_persistent_signatures_on_emmc_image()
    print("SDHCI_EMMC_BLOCK_DEVICE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
