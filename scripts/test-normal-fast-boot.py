#!/usr/bin/env python
"""Acceptance test for the ADR 0052 normal/verify boot split."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "normal-fast-boot-com1.log"


def run(command: list[str], expected: int = 0) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout)
    if result.returncode != expected:
        raise AssertionError(f"{command} returned {result.returncode}, expected {expected}")
    return result.stdout


def main() -> int:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none"])
    run([sys.executable, "scripts/build-image.py"])
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    # run-qemu.py always exits with the outcome's dedicated code (see
    # SCRIPT_EXIT_CODES in scripts/run-qemu.py), even on an exact
    # --expect-outcome match; "timeout" is 22, not 0.
    run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--timeout",
            "20",
            "--expect-outcome",
            "timeout",
        ],
        expected=22,
    )
    serial = SERIAL_LOG.read_text(encoding="utf-8", errors="replace")
    required = [
        "PYTHOS:CORE:NORMAL_BOOT:FAST_PATH",
        "PYTHOS:CORE:NORMAL_INIT:MEMORY_VM_READY",
        "PYTHOS:CORE:NORMAL_INIT:INTERRUPTS_TIMER_READY",
        "PYTHOS:CORE:NORMAL_INIT:TASK_PROCESS_READY",
        "PYTHOS:CORE:NORMAL_INIT:RING3_READY",
        "PYTHOS:CORE:NORMAL_INIT:SYSCALL_READY",
        "PYTHOS:CORE:NORMAL_INIT:USER_STACKS_READY",
        "PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY",
        "PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY",
        "PYTHOS:CORE:NORMAL_SERVICES_READY",
        "PYTHOS:CORE:NORMAL_BOOT_ALIVE",
    ]
    for marker in required:
        if marker not in serial:
            raise AssertionError(f"missing {marker}")
    forbidden = [
        "PYTHOS:CORE:PROCESS_MODEL_ADVERSARIAL_READY",
        "PYTHOS:CORE:STORAGE_ADVERSARIAL_SUITE_READY",
        "PYTHOS:CORE:MILESTONE_1_COMPLETE",
    ]
    for marker in forbidden:
        if marker in serial:
            raise AssertionError(f"normal boot ran verification marker {marker}")
    print("NORMAL_FAST_BOOT_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
