#!/usr/bin/env python
"""Acceptance test for Phase 7 reboot persistence and torn-write recovery."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "persistent-storage-serial.log"
REBOOT_IMAGE = TARGET / "persistent-object-store-reboot.img"
TORN_IMAGE = TARGET / "persistent-object-store-torn.img"
GENERAL_TORN_IMAGE = TARGET / "general-storage-torn.img"
SECTOR_SIZE = 512
STORAGE_SIZE_BYTES = 16 * 1024 * 1024
CONTROL_SECTOR = 30
CONTROL_MAGIC = b"PY7CTL01"
CONTROL_ARM_TORN = 1
GENERAL_CONTROL_SECTOR = 40
GENERAL_CONTROL_MAGIC = b"PY10CTL1"
KILL_WINDOW_MARKER = "PYTHOS:CORE:OBJECT_STORE:KILL_WINDOW"
GENERAL_KILL_WINDOW_MARKER = "PYTHOS:CORE:GENERAL_STORAGE:KILL_WINDOW"
CREATED_MARKER = "PYTHOS:CORE:OBJECT_STORE:CREATED"
PERSISTED_MARKER = "PYTHOS:CORE:OBJECT_STORE:PERSISTED"
RESTORED_MARKER = "PYTHOS:CORE:OBJECT_STORE:RESTORED"
TORN_RECOVERED_MARKER = "PYTHOS:CORE:OBJECT_STORE:TORN_WRITE_RECOVERED"
GENERAL_RESTORED_MARKER = "PYTHOS:CORE:GENERAL_STORAGE:RESTORED"
GENERAL_TORN_RECOVERED_MARKER = (
    "PYTHOS:CORE:GENERAL_STORAGE:DYNAMIC_TORN_WRITE_RECOVERED"
)
PHASE_7_COMPLETE_MARKER = "PYTHOS:CORE:PHASE_7_COMPLETE"
PHASE_10_COMPLETE_MARKER = "PYTHOS:CORE:PHASE_10_COMPLETE"
RESET_EXIT = 21


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


def run_qemu(storage_image: Path, *extra_args: str, expected_returncode: int = 0) -> str:
    return run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--storage-image",
            str(storage_image),
            "--timeout",
            "60",
            *extra_args,
        ],
        expected_returncode=expected_returncode,
    )


def assert_contains(output: str, marker: str) -> None:
    if marker not in output:
        raise AssertionError(f"missing marker {marker}")


def prepare_fresh_image(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    with path.open("wb") as image:
        image.truncate(STORAGE_SIZE_BYTES)


def seed_torn_control_image(path: Path) -> None:
    prepare_fresh_image(path)
    sector = bytearray(SECTOR_SIZE)
    sector[0:8] = CONTROL_MAGIC
    sector[8] = CONTROL_ARM_TORN
    sector[9] = 0
    with path.open("r+b") as image:
        image.seek(CONTROL_SECTOR * SECTOR_SIZE)
        image.write(sector)


def seed_general_torn_control_image(path: Path) -> None:
    prepare_fresh_image(path)
    sector = bytearray(SECTOR_SIZE)
    sector[0:8] = GENERAL_CONTROL_MAGIC
    sector[8] = CONTROL_ARM_TORN
    sector[9] = 0
    with path.open("r+b") as image:
        image.seek(GENERAL_CONTROL_SECTOR * SECTOR_SIZE)
        image.write(sector)


def test_reboot_restore() -> None:
    prepare_fresh_image(REBOOT_IMAGE)
    first = run_qemu(REBOOT_IMAGE, "--expect-outcome", "success")
    assert_contains(first, CREATED_MARKER)
    assert_contains(first, PERSISTED_MARKER)
    assert_contains(first, RESTORED_MARKER)
    assert_contains(first, PHASE_7_COMPLETE_MARKER)

    second = run_qemu(REBOOT_IMAGE, "--expect-outcome", "success")
    if CREATED_MARKER in second:
        raise AssertionError("second boot recreated the object store instead of restoring it")
    assert_contains(second, RESTORED_MARKER)
    assert_contains(second, PHASE_7_COMPLETE_MARKER)


def test_killed_mid_commit_recovery() -> None:
    seed_torn_control_image(TORN_IMAGE)
    killed = run_qemu(
        TORN_IMAGE,
        "--kill-after-marker",
        KILL_WINDOW_MARKER,
        expected_returncode=RESET_EXIT,
    )
    assert_contains(killed, KILL_WINDOW_MARKER)

    recovered = run_qemu(TORN_IMAGE, "--expect-outcome", "success")
    assert_contains(recovered, TORN_RECOVERED_MARKER)
    assert_contains(recovered, RESTORED_MARKER)
    assert_contains(recovered, PHASE_7_COMPLETE_MARKER)


def test_general_storage_killed_mid_commit_recovery() -> None:
    seed_general_torn_control_image(GENERAL_TORN_IMAGE)
    killed = run_qemu(
        GENERAL_TORN_IMAGE,
        "--kill-after-marker",
        GENERAL_KILL_WINDOW_MARKER,
        expected_returncode=RESET_EXIT,
    )
    assert_contains(killed, GENERAL_KILL_WINDOW_MARKER)

    recovered = run_qemu(GENERAL_TORN_IMAGE, "--expect-outcome", "success")
    assert_contains(recovered, GENERAL_TORN_RECOVERED_MARKER)
    assert_contains(recovered, GENERAL_RESTORED_MARKER)
    assert_contains(recovered, PHASE_10_COMPLETE_MARKER)


def main() -> int:
    build_boot_image()
    test_reboot_restore()
    test_killed_mid_commit_recovery()
    test_general_storage_killed_mid_commit_recovery()
    print("PERSISTENT_STORAGE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
