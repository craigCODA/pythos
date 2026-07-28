#!/usr/bin/env python
"""Acceptance test for the polling AHCI block-device backend."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "ahci-block-device-serial.log"
AHCI_IMAGE = TARGET / "ahci-store.img"
STORAGE_SIZE_BYTES = 16 * 1024 * 1024


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
            "--features",
            "verify",
        ]
    )
    build_verified_user_shell()
    run([sys.executable, "scripts/build-image.py"])


def prepare_fresh_image(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    with path.open("wb") as image:
        image.truncate(STORAGE_SIZE_BYTES)


def run_ahci_boot() -> str:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    return run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--timeout",
            "60",
            "--no-virtio-blk",
            "--ahci",
            "--ahci-storage-image",
            str(AHCI_IMAGE),
            "--expect-outcome",
            "success",
        ]
    )


def assert_contains(output: str, marker: str) -> None:
    if marker not in output:
        raise AssertionError(f"missing marker {marker}")


def assert_ahci_selected(output: str) -> None:
    assert_contains(output, "PYTHOS:CORE:BLOCK:AHCI_CONTROLLER_FOUND")
    assert_contains(output, "PYTHOS:CORE:BLOCK:DEVICE_SELECTED")
    assert_contains(output, "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI")
    assert_contains(output, "PYTHOS:CORE:BLOCK_DEVICE_READY")
    if "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO" in output:
        raise AssertionError("AHCI test selected virtio despite --no-virtio-blk")


def main() -> int:
    build_boot_image()
    prepare_fresh_image(AHCI_IMAGE)

    first = run_ahci_boot()
    assert_ahci_selected(first)
    assert_contains(first, "PYTHOS:CORE:OBJECT_STORE:PERSISTED")
    assert_contains(first, "PYTHOS:CORE:OBJECT_STORE:RESTORED")
    assert_contains(first, "PYTHOS:CORE:GENERAL_STORAGE:PERSISTED")
    assert_contains(first, "PYTHOS:CORE:GENERAL_STORAGE:RESTORED")
    assert_contains(first, "PYTHOS:CORE:PHASE_10_COMPLETE")
    assert_contains(first, "PYTHOS:CORE:MILESTONE_1_COMPLETE")

    second = run_ahci_boot()
    assert_ahci_selected(second)
    if "PYTHOS:CORE:OBJECT_STORE:CREATED" in second:
        raise AssertionError("second AHCI boot recreated object store instead of restoring it")
    assert_contains(second, "PYTHOS:CORE:OBJECT_STORE:RESTORED")
    assert_contains(second, "PYTHOS:CORE:GENERAL_STORAGE:RESTORED")
    assert_contains(second, "PYTHOS:CORE:MILESTONE_1_COMPLETE")

    print("AHCI_BLOCK_DEVICE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
