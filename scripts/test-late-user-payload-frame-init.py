#!/usr/bin/env python
"""Smoke-test late-safe user payload frame initialization."""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target" / "late-user-payload-frame-init"
CORE_TARGET = ROOT / "target" / "late-user-payload-frame-init-core"
CORE_ELF = CORE_TARGET / "x86_64-unknown-none" / "debug" / "pythcore"
ESP = ROOT / "image" / "esp"
PYTH_RUNTIME_ELF = (
    ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-pyth-runtime"
)

SECTOR_SIZE = 512
STORAGE_SIZE_BYTES = 16 * 1024 * 1024
CONTROL_SECTOR = 95
CONTROL_MAGIC = b"PYTGCTL1"
CONTROL_LATE_PAYLOAD_INIT_HELLO = 19

LATE_READY = "PYTHOS:CORE:LATE_RUNTIME_PAYLOAD_INIT_READY"
RUNTIME_ENTER_RE = re.compile(r"^PYTHOS:PYTHTIG:RUNTIME_ENTER package:[0-9A-F]{16}$")
RUNTIME_EXIT = "PYTHOS:PYTHTIG:RUNTIME_EXIT status:0"
RUNTIME_TERMINATED = "PYTHOS:PYTHTIG:RUNTIME_TERMINATED"
FORBIDDEN = (
    "PYTHOS:LOADER:FAIL",
    "PYTHOS:PANIC",
    "PYTHOS:EXCEPTION",
    "vector=0x",
    "PYTHOS:PYTHTIG:RUNTIME_FAULT",
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


def build_boot_image() -> None:
    TARGET.mkdir(parents=True, exist_ok=True)
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
            "--no-default-features",
            "--features",
            "pythtig-phase2-test",
        ]
    )
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-pyth-runtime.py"])
    run([sys.executable, "scripts/verify-pyth-runtime-elf.py"])
    if not PYTH_RUNTIME_ELF.exists():
        raise AssertionError("missing built Pyth runtime ELF")
    run([sys.executable, "scripts/build-pyth-graph.py"])
    run(
        [
            sys.executable,
            "scripts/build-image.py",
            "--kernel",
            str(CORE_ELF),
            "--with-pythtig",
        ]
    )


def prepare_control_image(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    with path.open("wb") as image:
        image.truncate(STORAGE_SIZE_BYTES)
    sector = bytearray(SECTOR_SIZE)
    sector[0:8] = CONTROL_MAGIC
    sector[8:10] = CONTROL_LATE_PAYLOAD_INIT_HELLO.to_bytes(2, "little")
    with path.open("r+b") as image:
        image.seek(CONTROL_SECTOR * SECTOR_SIZE)
        image.write(sector)


def prepare_esp() -> Path:
    scenario_esp = TARGET / "esp"
    if scenario_esp.exists():
        shutil.rmtree(scenario_esp)
    shutil.copytree(ESP, scenario_esp)
    return scenario_esp


def run_qemu() -> str:
    serial_log = TARGET / "late-user-payload-frame-init.log"
    storage_image = TARGET / "late-user-payload-frame-init.img"
    if serial_log.exists():
        serial_log.unlink()
    prepare_control_image(storage_image)
    return run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--esp",
            str(prepare_esp()),
            "--serial-log",
            str(serial_log),
            "--storage-image",
            str(storage_image),
            "--timeout",
            "60",
            "--success-marker",
            RUNTIME_TERMINATED,
            "--expect-outcome",
            "success",
        ]
    )


def serial_lines(output: str) -> list[str]:
    return [line.strip() for line in output.splitlines() if line.strip()]


def require_single(lines: list[str], marker: str) -> int:
    indexes = [index for index, line in enumerate(lines) if line == marker]
    if len(indexes) != 1:
        raise AssertionError(f"expected one {marker!r}, saw {len(indexes)}")
    return indexes[0]


def require_single_containing(lines: list[str], marker: str) -> int:
    indexes = [index for index, line in enumerate(lines) if marker in line]
    if len(indexes) != 1:
        raise AssertionError(f"expected one line containing {marker!r}, saw {len(indexes)}")
    return indexes[0]


def require_single_match(lines: list[str], pattern: re.Pattern[str]) -> int:
    indexes = [index for index, line in enumerate(lines) if pattern.match(line)]
    if len(indexes) != 1:
        raise AssertionError(f"expected one match for {pattern.pattern!r}, saw {len(indexes)}")
    return indexes[0]


def assert_smoke(output: str) -> None:
    lines = serial_lines(output)
    for marker in FORBIDDEN:
        if any(marker in line for line in lines):
            raise AssertionError(f"forbidden marker {marker}")

    vm_ready = require_single(lines, "PYTHOS:CORE:NORMAL_INIT:MEMORY_VM_READY")
    substrate_ready = require_single(lines, "PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY")
    late_ready = require_single(lines, LATE_READY)
    runtime_enter = require_single_match(lines, RUNTIME_ENTER_RE)
    program_log = require_single(lines, "PYTHOS:PYTHTIG:PROGRAM_LOG")
    runtime_exit = require_single(lines, RUNTIME_EXIT)
    terminated = require_single_containing(lines, RUNTIME_TERMINATED)

    if not vm_ready < substrate_ready < late_ready < runtime_enter < program_log < runtime_exit < terminated:
        raise AssertionError("late payload frame smoke markers are out of order")
    if "QEMU_OUTCOME success" not in output:
        raise AssertionError("missing QEMU_OUTCOME success")


def main() -> int:
    build_boot_image()
    output = run_qemu()
    assert_smoke(output)
    print("LATE_USER_PAYLOAD_FRAME_INIT_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
