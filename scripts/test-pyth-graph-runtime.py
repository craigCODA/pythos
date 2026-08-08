#!/usr/bin/env python
"""Phase 2 PythTIG acceptance: graph admission, runtime, and containment."""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
ESP = ROOT / "image" / "esp"
PYTH_RUNTIME_ELF = (
    TARGET
    / "x86_64-unknown-none"
    / "debug"
    / "pythos-user-pyth-runtime"
)
SECTOR_SIZE = 512
STORAGE_SIZE_BYTES = 16 * 1024 * 1024
CONTROL_SECTOR = 96
CONTROL_MAGIC = b"PYTGCTL1"
CONTROL_LAUNCH_HELLO = 1
CONTROL_LAUNCH_INVALID = 2
CONTROL_LAUNCH_BUDGET = 3
CONTROL_LAUNCH_UNSUPPORTED = 4
CONTROL_LAUNCH_INVALID_STRING = 5
CONTROL_LAUNCH_PARAMETERIZED = 6
RUNTIME_ELF_ENTRY = 0x0000_0000_0050_0000

HELLO_EXIT_MARKER = "PYTHOS:PYTHTIG:RUNTIME_EXIT status:0"
BUDGET_EXIT_MARKER = "PYTHOS:PYTHTIG:RUNTIME_EXIT status:2"
RUNTIME_TERMINATED_MARKER = "PYTHOS:PYTHTIG:RUNTIME_TERMINATED"
HELLO_SUCCESS_MARKER = RUNTIME_TERMINATED_MARKER
BUDGET_SUCCESS_MARKER = RUNTIME_TERMINATED_MARKER
INVALID_SUCCESS_MARKER = "PYTHOS:PYTHTIG:PACKAGE_REJECTED"
UNSUPPORTED_SUCCESS_MARKER = "PYTHOS:PYTHTIG:PACKAGE_REJECTED"
INVALID_STRING_SUCCESS_MARKER = "PYTHOS:PYTHTIG:PACKAGE_REJECTED"
PARAMETERIZED_SUCCESS_MARKER = "PYTHOS:PYTHTIG:PACKAGE_REJECTED"
FAULT_SUCCESS_MARKER = "PYTHOS:PYTHTIG:RUNTIME_FAULT_SAFE_IDLE"

PACKAGE_VALID_RE = re.compile(
    r"^PYTHOS:PYTHTIG:PACKAGE_VALID package:([0-9A-F]{16}) nodes:(\d+) blocks:(\d+)$"
)
BOOTSTRAP_BOUND_RE = re.compile(
    r"^PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:([0-9A-F]{16}) imports:1$"
)
RUNTIME_ENTER_RE = re.compile(
    r"^PYTHOS:PYTHTIG:RUNTIME_ENTER package:([0-9A-F]{16})$"
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


def build_pyth_graph_artifacts() -> None:
    run([sys.executable, "scripts/build-pyth-runtime.py"])
    run([sys.executable, "scripts/verify-pyth-runtime-elf.py"])
    run([sys.executable, "scripts/build-pyth-graph.py"])


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
            "pythtig-phase2-test",
        ]
    )
    build_verified_user_shell()
    build_pyth_graph_artifacts()
    run([sys.executable, "scripts/build-image.py", "--with-pythtig"])


def rebuild_image_with_current_runtime() -> None:
    run([sys.executable, "scripts/verify-pyth-runtime-elf.py"])
    run([sys.executable, "scripts/build-image.py", "--with-pythtig"])


def prepare_graph_control_image(path: Path, mode: int) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    with path.open("wb") as image:
        image.truncate(STORAGE_SIZE_BYTES)
    sector = bytearray(SECTOR_SIZE)
    sector[0:8] = CONTROL_MAGIC
    sector[8:10] = mode.to_bytes(2, "little")
    with path.open("r+b") as image:
        image.seek(CONTROL_SECTOR * SECTOR_SIZE)
        image.write(sector)


def prepare_scenario_esp(scenario: str) -> Path:
    scenario_esp = TARGET / f"pyth-graph-runtime-{scenario}-esp"
    if scenario_esp.exists():
        shutil.rmtree(scenario_esp)
    shutil.copytree(ESP, scenario_esp)
    return scenario_esp


def run_qemu(scenario: str, mode: int, success_marker: str) -> str:
    serial_log = TARGET / f"pyth-graph-runtime-{scenario}.log"
    storage_image = TARGET / f"pyth-graph-runtime-{scenario}.img"
    scenario_esp = prepare_scenario_esp(scenario)
    prepare_graph_control_image(storage_image, mode)
    if serial_log.exists():
        serial_log.unlink()
    return run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--esp",
            str(scenario_esp),
            "--serial-log",
            str(serial_log),
            "--storage-image",
            str(storage_image),
            "--timeout",
            "60",
            "--success-marker",
            success_marker,
            "--expect-outcome",
            "success",
        ]
    )


def serial_lines(serial: str) -> list[str]:
    return [line.strip() for line in serial.splitlines() if line.strip()]


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


def require_single_match(lines: list[str], pattern: re.Pattern[str]) -> tuple[int, re.Match[str]]:
    matches = [
        (index, match)
        for index, line in enumerate(lines)
        if (match := pattern.match(line)) is not None
    ]
    if len(matches) != 1:
        raise AssertionError(f"expected one match for {pattern.pattern!r}, saw {len(matches)}")
    return matches[0]


def reject_forbidden(lines: list[str], forbidden: tuple[str, ...]) -> None:
    for marker in forbidden:
        if any(marker in line for line in lines):
            raise AssertionError(f"forbidden marker {marker}")


def assert_pyth_tig_success(serial: str) -> None:
    lines = serial_lines(serial)
    reject_forbidden(
        lines,
        (
            "PYTHOS:PANIC",
            "PYTHOS:PYTHTIG:PACKAGE_REJECTED",
            "PYTHOS:PYTHTIG:BUDGET_EXHAUSTED",
            "PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED",
            "PYTHOS:PYTHTIG:RUNTIME_TERMINATION_FAILED",
        ),
    )

    package_index, package_match = require_single_match(lines, PACKAGE_VALID_RE)
    bootstrap_index, bootstrap_match = require_single_match(lines, BOOTSTRAP_BOUND_RE)
    runtime_index, runtime_match = require_single_match(lines, RUNTIME_ENTER_RE)
    log_index = require_single(lines, "PYTHOS:PYTHTIG:PROGRAM_LOG")
    exit_index = require_single(lines, HELLO_EXIT_MARKER)
    terminated_index = require_single_containing(lines, RUNTIME_TERMINATED_MARKER)

    if (package_match.group(2), package_match.group(3)) != ("5", "1"):
        raise AssertionError("hello graph package shape changed")
    package_digest = package_match.group(1)
    runtime_digest = runtime_match.group(1)
    if package_digest != runtime_digest:
        raise AssertionError(
            f"package digest changed across launch: {package_digest} != {runtime_digest}"
        )
    if bootstrap_match.group(1) != "5059544847520001":
        raise AssertionError(f"unexpected graph principal {bootstrap_match.group(1)}")
    if not package_index < bootstrap_index < runtime_index < log_index < exit_index < terminated_index:
        raise AssertionError("PythTIG markers are out of order")


def assert_invalid_package_rejected(serial: str) -> None:
    lines = serial_lines(serial)
    reject_forbidden(
        lines,
        (
            "PYTHOS:PANIC",
            "PYTHOS:PYTHTIG:RUNTIME_ENTER",
            "PYTHOS:PYTHTIG:PROGRAM_LOG",
            "PYTHOS:PYTHTIG:RUNTIME_EXIT",
        ),
    )
    require_single(lines, "PYTHOS:PYTHTIG:PACKAGE_REJECTED error:VERIFY_EFFECT_FORK")


def assert_unsupported_profile_rejected(serial: str) -> None:
    lines = serial_lines(serial)
    reject_forbidden(
        lines,
        (
            "PYTHOS:PANIC",
            "PYTHOS:PYTHTIG:PACKAGE_VALID",
            "PYTHOS:PYTHTIG:BOOTSTRAP_BOUND",
            "PYTHOS:PYTHTIG:RUNTIME_ENTER",
            "PYTHOS:PYTHTIG:PROGRAM_LOG",
            "PYTHOS:PYTHTIG:RUNTIME_EXIT",
        ),
    )
    require_single(
        lines,
        "PYTHOS:PYTHTIG:PACKAGE_REJECTED error:UNSUPPORTED_PHASE2_OPCODE",
    )


def assert_invalid_string_rejected(serial: str) -> None:
    lines = serial_lines(serial)
    reject_forbidden(
        lines,
        (
            "PYTHOS:PANIC",
            "PYTHOS:PYTHTIG:PACKAGE_VALID",
            "PYTHOS:PYTHTIG:BOOTSTRAP_BOUND",
            "PYTHOS:PYTHTIG:RUNTIME_ENTER",
            "PYTHOS:PYTHTIG:PROGRAM_LOG",
            "PYTHOS:PYTHTIG:RUNTIME_EXIT",
        ),
    )
    require_single(
        lines,
        "PYTHOS:PYTHTIG:PACKAGE_REJECTED error:VERIFY_NONCANONICAL_ENCODING",
    )


def assert_parameterized_jump_rejected(serial: str) -> None:
    lines = serial_lines(serial)
    reject_forbidden(
        lines,
        (
            "PYTHOS:PANIC",
            "PYTHOS:PYTHTIG:PACKAGE_VALID",
            "PYTHOS:PYTHTIG:BOOTSTRAP_BOUND",
            "PYTHOS:PYTHTIG:RUNTIME_ENTER",
            "PYTHOS:PYTHTIG:PROGRAM_LOG",
            "PYTHOS:PYTHTIG:RUNTIME_EXIT",
        ),
    )
    require_single(
        lines,
        "PYTHOS:PYTHTIG:PACKAGE_REJECTED error:UNSUPPORTED_PHASE2_CONTROL_FLOW",
    )


def assert_budget_exhaustion(serial: str) -> None:
    lines = serial_lines(serial)
    reject_forbidden(
        lines,
        (
            "PYTHOS:PANIC",
            "PYTHOS:PYTHTIG:PACKAGE_REJECTED",
            "PYTHOS:PYTHTIG:PROGRAM_LOG",
            "PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED",
            "PYTHOS:PYTHTIG:RUNTIME_TERMINATION_FAILED",
        ),
    )
    package_index, package_match = require_single_match(lines, PACKAGE_VALID_RE)
    bootstrap_index, bootstrap_match = require_single_match(lines, BOOTSTRAP_BOUND_RE)
    runtime_index, runtime_match = require_single_match(lines, RUNTIME_ENTER_RE)
    budget_index = require_single_containing(lines, "PYTHOS:PYTHTIG:BUDGET_EXHAUSTED node:")
    exit_index = require_single(lines, BUDGET_EXIT_MARKER)
    terminated_index = require_single_containing(lines, RUNTIME_TERMINATED_MARKER)

    if (package_match.group(2), package_match.group(3)) != ("2", "1"):
        raise AssertionError("budget graph package shape changed")
    if package_match.group(1) != runtime_match.group(1):
        raise AssertionError("budget package digest changed across launch")
    if bootstrap_match.group(1) != "5059544847520002":
        raise AssertionError(f"unexpected budget graph principal {bootstrap_match.group(1)}")
    if not package_index < bootstrap_index < runtime_index < budget_index < exit_index < terminated_index:
        raise AssertionError("budget markers are out of order")


def assert_fault_contained(serial: str) -> None:
    lines = serial_lines(serial)
    reject_forbidden(
        lines,
        (
            "PYTHOS:PANIC",
            "PYTHOS:PYTHTIG:PROGRAM_LOG",
            "PYTHOS:PYTHTIG:RUNTIME_EXIT",
            "PYTHOS:CORE:CRASH:PEER_ALIVE",
            "PYTHOS:PYTHTIG:RUNTIME_TERMINATION_FAILED",
        ),
    )
    runtime_index = require_single_match(lines, RUNTIME_ENTER_RE)[0]
    user_fault_index = require_single(lines, "PYTHOS:CORE:CRASH:USER_FAULT")
    contained_index = require_single_containing(
        lines,
        "PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED principal:5059544852540001",
    )
    safe_idle_index = require_single(lines, FAULT_SUCCESS_MARKER)
    if not runtime_index < user_fault_index < contained_index < safe_idle_index:
        raise AssertionError("runtime fault containment markers are out of order")


def build_fault_runtime_elf() -> bytes:
    text = b"\x0F\x0B\xF4"
    text_offset = 0x1000
    elf = bytearray(text_offset + len(text))
    elf[0:4] = b"\x7fELF"
    elf[4] = 2
    elf[5] = 1
    elf[6] = 1
    elf[16:18] = (2).to_bytes(2, "little")
    elf[18:20] = (0x3E).to_bytes(2, "little")
    elf[20:24] = (1).to_bytes(4, "little")
    elf[24:32] = RUNTIME_ELF_ENTRY.to_bytes(8, "little")
    elf[32:40] = (64).to_bytes(8, "little")
    elf[52:54] = (64).to_bytes(2, "little")
    elf[54:56] = (56).to_bytes(2, "little")
    elf[56:58] = (1).to_bytes(2, "little")
    entry = 64
    elf[entry : entry + 4] = (1).to_bytes(4, "little")
    elf[entry + 4 : entry + 8] = (0x5).to_bytes(4, "little")
    elf[entry + 8 : entry + 16] = text_offset.to_bytes(8, "little")
    elf[entry + 16 : entry + 24] = RUNTIME_ELF_ENTRY.to_bytes(8, "little")
    elf[entry + 24 : entry + 32] = RUNTIME_ELF_ENTRY.to_bytes(8, "little")
    elf[entry + 32 : entry + 40] = len(text).to_bytes(8, "little")
    elf[entry + 40 : entry + 48] = len(text).to_bytes(8, "little")
    elf[entry + 48 : entry + 56] = (0x1000).to_bytes(8, "little")
    elf[text_offset : text_offset + len(text)] = text
    return bytes(elf)


def run_fault_runtime_scenario(original_runtime: bytes) -> None:
    try:
        PYTH_RUNTIME_ELF.write_bytes(build_fault_runtime_elf())
        rebuild_image_with_current_runtime()
        fault_serial = run_qemu("fault", CONTROL_LAUNCH_HELLO, FAULT_SUCCESS_MARKER)
        assert_fault_contained(fault_serial)
    finally:
        PYTH_RUNTIME_ELF.write_bytes(original_runtime)
        rebuild_image_with_current_runtime()


def main() -> int:
    build_boot_image()
    original_runtime = PYTH_RUNTIME_ELF.read_bytes()

    success_serial = run_qemu("success", CONTROL_LAUNCH_HELLO, HELLO_SUCCESS_MARKER)
    assert_pyth_tig_success(success_serial)

    invalid_serial = run_qemu("invalid", CONTROL_LAUNCH_INVALID, INVALID_SUCCESS_MARKER)
    assert_invalid_package_rejected(invalid_serial)

    unsupported_serial = run_qemu(
        "unsupported", CONTROL_LAUNCH_UNSUPPORTED, UNSUPPORTED_SUCCESS_MARKER
    )
    assert_unsupported_profile_rejected(unsupported_serial)

    invalid_string_serial = run_qemu(
        "invalid-string", CONTROL_LAUNCH_INVALID_STRING, INVALID_STRING_SUCCESS_MARKER
    )
    assert_invalid_string_rejected(invalid_string_serial)

    parameterized_serial = run_qemu(
        "parameterized", CONTROL_LAUNCH_PARAMETERIZED, PARAMETERIZED_SUCCESS_MARKER
    )
    assert_parameterized_jump_rejected(parameterized_serial)

    budget_serial = run_qemu("budget", CONTROL_LAUNCH_BUDGET, BUDGET_SUCCESS_MARKER)
    assert_budget_exhaustion(budget_serial)

    run_fault_runtime_scenario(original_runtime)

    print("PYTH_GRAPH_RUNTIME_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
