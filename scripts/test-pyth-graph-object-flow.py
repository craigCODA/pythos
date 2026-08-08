#!/usr/bin/env python
"""Phase 3 PythTIG acceptance: retained object service and capability flow."""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
ESP = ROOT / "image" / "esp"
SECTOR_SIZE = 512
STORAGE_SIZE_BYTES = 16 * 1024 * 1024
CONTROL_SECTOR = 95
CONTROL_MAGIC = b"PYTGCTL1"
CONTROL_LAUNCH_OBJECT_CREATE = 7
CONTROL_LAUNCH_OBJECT_RESTORE = 8
CONTROL_LAUNCH_OBJECT_KNOWN_DENIED = 9
CONTROL_LAUNCH_OBJECT_FORGERY = 10

STORAGE_IMAGE = TARGET / "pyth-graph-object-flow.img"
RUNTIME_EXIT_OK_MARKER = "PYTHOS:PYTHTIG:RUNTIME_EXIT status:0"
RUNTIME_TERMINATED_MARKER = "PYTHOS:PYTHTIG:RUNTIME_TERMINATED"
OBJECT_CREATED_MARKER = "PYTHOS:PYTHTIG:OBJECT_CREATED object:1042 revision:1"
OBJECT_REVISED_MARKER = "PYTHOS:PYTHTIG:OBJECT_REVISED object:1042 revision:2"
OBJECT_INSPECTED_MARKER = "PYTHOS:PYTHTIG:OBJECT_INSPECTED object:1042 revision:2"
OBJECT_REBOUND_MARKER = "PYTHOS:PYTHTIG:OBJECT_REBOUND object:1042"
OBJECT_HISTORY_MARKER = "PYTHOS:PYTHTIG:OBJECT_HISTORY object:1042 revisions:2"
OBJECT_KNOWN_DENIED_MARKER = "PYTHOS:PYTHTIG:OBJECT_KNOWN_DENIED object:2001"
CAPABILITY_FORGERY_DENIED_MARKER = "PYTHOS:PYTHTIG:CAPABILITY_FORGERY_DENIED"
OBJECT_FLOW_ACCEPTANCE_COMPLETE_MARKER = (
    "PYTHOS:PYTHTIG:OBJECT_FLOW_ACCEPTANCE_COMPLETE"
)

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
    run([sys.executable, "scripts/build-image.py", "--with-pythtig-object-flow"])


def prepare_fresh_storage_image(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    with path.open("wb") as image:
        image.truncate(STORAGE_SIZE_BYTES)


def write_graph_control(path: Path, mode: int) -> None:
    if not path.exists():
        raise AssertionError(f"missing persistent storage image {path}")
    sector = bytearray(SECTOR_SIZE)
    sector[0:8] = CONTROL_MAGIC
    sector[8:10] = mode.to_bytes(2, "little")
    with path.open("r+b") as image:
        image.seek(CONTROL_SECTOR * SECTOR_SIZE)
        image.write(sector)


def prepare_scenario_esp(scenario: str) -> Path:
    scenario_esp = TARGET / f"pyth-graph-object-flow-{scenario}-esp"
    if scenario_esp.exists():
        shutil.rmtree(scenario_esp)
    shutil.copytree(ESP, scenario_esp)
    return scenario_esp


def run_qemu(scenario: str, mode: int) -> str:
    serial_log = TARGET / f"pyth-graph-object-flow-{scenario}.log"
    scenario_esp = prepare_scenario_esp(scenario)
    write_graph_control(STORAGE_IMAGE, mode)
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
            str(STORAGE_IMAGE),
            "--timeout",
            "60",
            "--success-marker",
            RUNTIME_TERMINATED_MARKER,
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


def assert_common_graph_execution(
    serial: str,
    expected_principal: str,
    expected_shape: tuple[str, str],
) -> tuple[list[str], int, int, int, int]:
    lines = serial_lines(serial)
    reject_forbidden(
        lines,
        (
            "PYTHOS:LOADER:FAIL",
            "PYTHOS:PANIC",
            "PYTHOS:PYTHTIG:PACKAGE_REJECTED",
            "PYTHOS:PYTHTIG:BUDGET_EXHAUSTED",
            "PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED",
            "PYTHOS:PYTHTIG:RUNTIME_FAULT_SAFE_IDLE",
            "PYTHOS:PYTHTIG:RUNTIME_TERMINATION_FAILED",
        ),
    )
    require_single(lines, "PYTHOS:LOADER:ENTER")
    package_index, package_match = require_single_match(lines, PACKAGE_VALID_RE)
    bootstrap_index, bootstrap_match = require_single_match(lines, BOOTSTRAP_BOUND_RE)
    runtime_index, runtime_match = require_single_match(lines, RUNTIME_ENTER_RE)
    exit_index = require_single(lines, RUNTIME_EXIT_OK_MARKER)
    terminated_index = require_single_containing(lines, RUNTIME_TERMINATED_MARKER)

    if (package_match.group(2), package_match.group(3)) != expected_shape:
        raise AssertionError(
            f"unexpected graph package shape: nodes={package_match.group(2)} "
            f"blocks={package_match.group(3)}"
        )
    if bootstrap_match.group(1) != expected_principal:
        raise AssertionError(f"unexpected graph principal {bootstrap_match.group(1)}")
    if package_match.group(1) != runtime_match.group(1):
        raise AssertionError("package digest changed across launch")
    if not package_index < bootstrap_index < runtime_index < exit_index < terminated_index:
        raise AssertionError("common PythTIG runtime markers are out of order")
    return lines, package_index, bootstrap_index, runtime_index, terminated_index


def assert_object_create_flow(serial: str) -> None:
    lines, _, _, runtime_index, terminated_index = assert_common_graph_execution(
        serial, "5059544847520006", ("11", "1")
    )
    created_index = require_single(lines, OBJECT_CREATED_MARKER)
    revised_index = require_single(lines, OBJECT_REVISED_MARKER)
    inspected_index = require_single(lines, OBJECT_INSPECTED_MARKER)
    exit_index = require_single(lines, RUNTIME_EXIT_OK_MARKER)
    reject_forbidden(
        lines,
        (
            OBJECT_REBOUND_MARKER,
            OBJECT_HISTORY_MARKER,
            OBJECT_KNOWN_DENIED_MARKER,
            CAPABILITY_FORGERY_DENIED_MARKER,
            OBJECT_FLOW_ACCEPTANCE_COMPLETE_MARKER,
        ),
    )
    if not runtime_index < created_index < revised_index < inspected_index < exit_index < terminated_index:
        raise AssertionError("object create/revise/inspect markers are out of order")


def assert_object_restore_flow(serial: str) -> None:
    lines, _, _, runtime_index, terminated_index = assert_common_graph_execution(
        serial, "5059544847520007", ("10", "1")
    )
    rebound_index = require_single(lines, OBJECT_REBOUND_MARKER)
    inspected_index = require_single(lines, OBJECT_INSPECTED_MARKER)
    history_index = require_single(lines, OBJECT_HISTORY_MARKER)
    exit_index = require_single(lines, RUNTIME_EXIT_OK_MARKER)
    reject_forbidden(
        lines,
        (
            OBJECT_CREATED_MARKER,
            OBJECT_REVISED_MARKER,
            OBJECT_KNOWN_DENIED_MARKER,
            CAPABILITY_FORGERY_DENIED_MARKER,
            OBJECT_FLOW_ACCEPTANCE_COMPLETE_MARKER,
        ),
    )
    if not runtime_index < rebound_index < inspected_index < history_index < exit_index < terminated_index:
        raise AssertionError("object restore/query/history markers are out of order")


def assert_known_object_denied(serial: str) -> None:
    lines, _, _, runtime_index, terminated_index = assert_common_graph_execution(
        serial, "5059544847520008", ("9", "1")
    )
    rebound_index = require_single(lines, OBJECT_REBOUND_MARKER)
    denied_index = require_single(lines, OBJECT_KNOWN_DENIED_MARKER)
    exit_index = require_single(lines, RUNTIME_EXIT_OK_MARKER)
    reject_forbidden(
        lines,
        (
            OBJECT_CREATED_MARKER,
            OBJECT_REVISED_MARKER,
            OBJECT_INSPECTED_MARKER,
            OBJECT_HISTORY_MARKER,
            CAPABILITY_FORGERY_DENIED_MARKER,
            OBJECT_FLOW_ACCEPTANCE_COMPLETE_MARKER,
        ),
    )
    if not runtime_index < rebound_index < denied_index < exit_index < terminated_index:
        raise AssertionError("known-object denial markers are out of order")


def assert_capability_forgery_denied(serial: str) -> None:
    lines, _, _, runtime_index, terminated_index = assert_common_graph_execution(
        serial, "5059544847520009", ("6", "1")
    )
    denied_index = require_single(lines, CAPABILITY_FORGERY_DENIED_MARKER)
    exit_index = require_single(lines, RUNTIME_EXIT_OK_MARKER)
    complete_index = require_single(lines, OBJECT_FLOW_ACCEPTANCE_COMPLETE_MARKER)
    reject_forbidden(
        lines,
        (
            OBJECT_CREATED_MARKER,
            OBJECT_REVISED_MARKER,
            OBJECT_INSPECTED_MARKER,
            OBJECT_REBOUND_MARKER,
            OBJECT_HISTORY_MARKER,
            OBJECT_KNOWN_DENIED_MARKER,
        ),
    )
    if not runtime_index < denied_index < exit_index < complete_index < terminated_index:
        raise AssertionError("capability-forgery denial markers are out of order")


def main() -> int:
    build_boot_image()
    prepare_fresh_storage_image(STORAGE_IMAGE)

    create_serial = run_qemu("create", CONTROL_LAUNCH_OBJECT_CREATE)
    assert_object_create_flow(create_serial)

    restore_serial = run_qemu("restore", CONTROL_LAUNCH_OBJECT_RESTORE)
    assert_object_restore_flow(restore_serial)

    known_serial = run_qemu("known-denied", CONTROL_LAUNCH_OBJECT_KNOWN_DENIED)
    assert_known_object_denied(known_serial)

    forgery_serial = run_qemu("forgery", CONTROL_LAUNCH_OBJECT_FORGERY)
    assert_capability_forgery_denied(forgery_serial)

    print("PYTH_GRAPH_OBJECT_FLOW_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
