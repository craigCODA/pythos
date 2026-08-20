#!/usr/bin/env python
"""Phase 6 PythTIG acceptance: native codegen and interpreter equivalence."""

from __future__ import annotations

from dataclasses import dataclass
import re
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
ESP = ROOT / "image" / "esp"
PYTH_TIG_DIR = TARGET / "pyth-tig"
PYTH_NATIVE_DIR = TARGET / "pyth-native"

SECTOR_SIZE = 512
STORAGE_SIZE_BYTES = 16 * 1024 * 1024
CONTROL_SECTOR = 95
CONTROL_MAGIC = b"PYTGCTL1"

CONTROL_LAUNCH_HELLO = 1
CONTROL_LAUNCH_BUDGET = 3
CONTROL_LAUNCH_OBJECT_CREATE = 7
CONTROL_LAUNCH_OBJECT_RESTORE = 8
CONTROL_LAUNCH_OBJECT_KNOWN_DENIED = 9
CONTROL_LAUNCH_OBJECT_FORGERY = 10
CONTROL_LAUNCH_TASK_STEWARD = 11
CONTROL_LAUNCH_NATIVE_HELLO = 12
CONTROL_LAUNCH_NATIVE_BUDGET = 13
CONTROL_LAUNCH_NATIVE_OBJECT_CREATE = 14
CONTROL_LAUNCH_NATIVE_OBJECT_RESTORE = 15
CONTROL_LAUNCH_NATIVE_OBJECT_KNOWN_DENIED = 16
CONTROL_LAUNCH_NATIVE_OBJECT_FORGERY = 17
CONTROL_LAUNCH_NATIVE_TASK_STEWARD = 18

RUNTIME_TERMINATED_MARKER = "PYTHOS:PYTHTIG:RUNTIME_TERMINATED"
RUNTIME_EXIT_PREFIX = "PYTHOS:PYTHTIG:RUNTIME_EXIT status:"
NATIVE_EXIT_PREFIX = "PYTHOS:PYTHTIG:NATIVE_EXIT status:"
NATIVE_ELF_VALID_PREFIX = "PYTHOS:PYTHTIG:NATIVE_ELF_VALID elf:"

PACKAGE_VALID_RE = re.compile(
    r"^PYTHOS:PYTHTIG:PACKAGE_VALID package:([0-9A-F]{16}) nodes:(\d+) blocks:(\d+)$"
)
BOOTSTRAP_BOUND_RE = re.compile(
    r"^PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:([0-9A-F]{16}) imports:(\d+)$"
)
ENTER_RE = re.compile(
    r"^PYTHOS:PYTHTIG:(?:RUNTIME|NATIVE)_ENTER package:([0-9A-F]{16})$"
)
EXIT_RE = re.compile(r"^PYTHOS:PYTHTIG:(?:RUNTIME|NATIVE)_EXIT status:(\d+)$")
TERMINATED_RE = re.compile(
    r"^PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:([0-9A-F]{16})$"
)
DIGEST_RE = re.compile(r"[0-9A-F]{16}")


@dataclass(frozen=True)
class Scenario:
    name: str
    graph: str
    interpreter_mode: int
    native_mode: int
    interpreter_image_flag: str
    expected_exit_status: int


SCENARIOS = [
    Scenario(
        "hello",
        "hello",
        CONTROL_LAUNCH_HELLO,
        CONTROL_LAUNCH_NATIVE_HELLO,
        "--with-pythtig",
        0,
    ),
    Scenario(
        "budget",
        "budget",
        CONTROL_LAUNCH_BUDGET,
        CONTROL_LAUNCH_NATIVE_BUDGET,
        "--with-pythtig",
        2,
    ),
    Scenario(
        "object-create",
        "object-create",
        CONTROL_LAUNCH_OBJECT_CREATE,
        CONTROL_LAUNCH_NATIVE_OBJECT_CREATE,
        "--with-pythtig-object-flow",
        0,
    ),
    Scenario(
        "object-restore",
        "object-restore",
        CONTROL_LAUNCH_OBJECT_RESTORE,
        CONTROL_LAUNCH_NATIVE_OBJECT_RESTORE,
        "--with-pythtig-object-flow",
        0,
    ),
    Scenario(
        "object-known-denied",
        "object-known-denied",
        CONTROL_LAUNCH_OBJECT_KNOWN_DENIED,
        CONTROL_LAUNCH_NATIVE_OBJECT_KNOWN_DENIED,
        "--with-pythtig-object-flow",
        0,
    ),
    Scenario(
        "object-forgery",
        "object-forgery",
        CONTROL_LAUNCH_OBJECT_FORGERY,
        CONTROL_LAUNCH_NATIVE_OBJECT_FORGERY,
        "--with-pythtig-object-flow",
        0,
    ),
    Scenario(
        "task-steward",
        "task-steward",
        CONTROL_LAUNCH_TASK_STEWARD,
        CONTROL_LAUNCH_NATIVE_TASK_STEWARD,
        "--with-pythtig-task-steward",
        0,
    ),
]


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


def build_base_artifacts() -> None:
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
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-pyth-runtime.py"])
    run([sys.executable, "scripts/verify-pyth-runtime-elf.py"])
    run([sys.executable, "scripts/build-pyth-graph.py"])


def build_native_artifacts() -> dict[str, Path]:
    native = {}
    for scenario in SCENARIOS:
        graph = PYTH_TIG_DIR / f"{scenario.graph}.tig"
        elf = PYTH_NATIVE_DIR / f"{scenario.graph}.elf"
        run(
            [
                "cargo",
                "run",
                "-q",
                "-p",
                "pyth-codegen-x86_64",
                "--",
                "build",
                str(graph),
                "-o",
                str(elf),
            ]
        )
        run([sys.executable, "scripts/verify-pyth-native-elf.py", str(elf)])
        native[scenario.name] = elf
    return native


def prepare_storage_image(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    with path.open("wb") as image:
        image.truncate(STORAGE_SIZE_BYTES)


def write_graph_control(path: Path, mode: int) -> None:
    if not path.exists():
        prepare_storage_image(path)
    sector = bytearray(SECTOR_SIZE)
    sector[0:8] = CONTROL_MAGIC
    sector[8:10] = mode.to_bytes(2, "little")
    with path.open("r+b") as image:
        image.seek(CONTROL_SECTOR * SECTOR_SIZE)
        image.write(sector)


def prepare_scenario_esp(label: str) -> Path:
    scenario_esp = TARGET / f"pyth-native-codegen-{label}-esp"
    if scenario_esp.exists():
        shutil.rmtree(scenario_esp)
    shutil.copytree(ESP, scenario_esp)
    return scenario_esp


def build_interpreter_image(flag: str) -> None:
    run([sys.executable, "scripts/build-image.py", flag])


def build_native_image(elf: Path) -> None:
    run([sys.executable, "scripts/build-image.py", "--pyth-native-elf", str(elf)])


def run_qemu(label: str, mode: int, storage_image: Path) -> str:
    serial_log = TARGET / f"pyth-native-codegen-{label}.log"
    scenario_esp = prepare_scenario_esp(label)
    write_graph_control(storage_image, mode)
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


def assert_interpreter_trace(serial: str, scenario: Scenario) -> list[str]:
    lines = serial_lines(serial)
    reject_forbidden(
        lines,
        (
            "PYTHOS:LOADER:FAIL",
            "PYTHOS:PANIC",
            "PYTHOS:PYTHTIG:PACKAGE_REJECTED",
            "PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED",
            "PYTHOS:PYTHTIG:RUNTIME_FAULT_SAFE_IDLE",
            "PYTHOS:PYTHTIG:RUNTIME_TERMINATION_FAILED",
            NATIVE_ELF_VALID_PREFIX,
            "PYTHOS:PYTHTIG:NATIVE_ENTER",
            NATIVE_EXIT_PREFIX,
        ),
    )
    package_index = require_single_match(lines, PACKAGE_VALID_RE)[0]
    bootstrap_index = require_single_match(lines, BOOTSTRAP_BOUND_RE)[0]
    enter_index = require_single_match(lines, ENTER_RE)[0]
    exit_index = require_single(lines, f"{RUNTIME_EXIT_PREFIX}{scenario.expected_exit_status}")
    terminated_index = require_single_containing(lines, RUNTIME_TERMINATED_MARKER)
    if not package_index < bootstrap_index < enter_index < exit_index < terminated_index:
        raise AssertionError(f"{scenario.name} interpreter markers are out of order")
    return lines


def assert_native_trace(serial: str, scenario: Scenario) -> list[str]:
    lines = serial_lines(serial)
    reject_forbidden(
        lines,
        (
            "PYTHOS:LOADER:FAIL",
            "PYTHOS:PANIC",
            "PYTHOS:PYTHTIG:PACKAGE_REJECTED",
            "PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED",
            "PYTHOS:PYTHTIG:RUNTIME_FAULT_SAFE_IDLE",
            "PYTHOS:PYTHTIG:RUNTIME_TERMINATION_FAILED",
            "PYTHOS:PYTHTIG:RUNTIME_ENTER",
            RUNTIME_EXIT_PREFIX,
        ),
    )
    native_valid_index = require_single_containing(lines, NATIVE_ELF_VALID_PREFIX)
    package_index = require_single_match(lines, PACKAGE_VALID_RE)[0]
    bootstrap_index = require_single_match(lines, BOOTSTRAP_BOUND_RE)[0]
    enter_index = require_single_match(lines, ENTER_RE)[0]
    exit_index = require_single(lines, f"{NATIVE_EXIT_PREFIX}{scenario.expected_exit_status}")
    terminated_index = require_single_containing(lines, RUNTIME_TERMINATED_MARKER)
    if not native_valid_index < package_index < bootstrap_index < enter_index < exit_index < terminated_index:
        raise AssertionError(f"{scenario.name} native markers are out of order")
    return lines


def normalize_pythtig_line(line: str) -> str | None:
    if not line.startswith("PYTHOS:PYTHTIG:"):
        return None
    if line.startswith(NATIVE_ELF_VALID_PREFIX):
        return None

    if match := PACKAGE_VALID_RE.match(line):
        return f"PACKAGE_VALID nodes:{match.group(2)} blocks:{match.group(3)}"
    if match := BOOTSTRAP_BOUND_RE.match(line):
        return f"BOOTSTRAP_BOUND principal:{match.group(1)} imports:{match.group(2)}"
    if match := ENTER_RE.match(line):
        return f"ENTER package:{match.group(1)}"
    if match := EXIT_RE.match(line):
        return f"EXIT status:{match.group(1)}"
    if match := TERMINATED_RE.match(line):
        return "TERMINATED"
    return DIGEST_RE.sub("<hex64>", line.removeprefix("PYTHOS:PYTHTIG:"))


def normalized_pythtig_trace(lines: list[str]) -> list[str]:
    return [
        normalized
        for line in lines
        if (normalized := normalize_pythtig_line(line)) is not None
    ]


def assert_equivalent_trace(
    scenario: Scenario,
    interpreter_lines: list[str],
    native_lines: list[str],
) -> None:
    interpreter_trace = normalized_pythtig_trace(interpreter_lines)
    native_trace = normalized_pythtig_trace(native_lines)
    if interpreter_trace != native_trace:
        raise AssertionError(
            f"{scenario.name} native/interpreter trace mismatch\n"
            f"interpreter: {interpreter_trace}\n"
            f"native:      {native_trace}"
        )


def run_interpreter_scenarios() -> dict[str, list[str]]:
    results: dict[str, list[str]] = {}

    build_interpreter_image("--with-pythtig")
    basic_storage = TARGET / "pyth-native-codegen-interpreter-basic.img"
    for scenario in [SCENARIOS[0], SCENARIOS[1]]:
        prepare_storage_image(basic_storage)
        serial = run_qemu(f"interpreter-{scenario.name}", scenario.interpreter_mode, basic_storage)
        results[scenario.name] = assert_interpreter_trace(serial, scenario)

    build_interpreter_image("--with-pythtig-object-flow")
    object_storage = TARGET / "pyth-native-codegen-interpreter-object-flow.img"
    prepare_storage_image(object_storage)
    for scenario in SCENARIOS[2:6]:
        serial = run_qemu(f"interpreter-{scenario.name}", scenario.interpreter_mode, object_storage)
        results[scenario.name] = assert_interpreter_trace(serial, scenario)

    build_interpreter_image("--with-pythtig-task-steward")
    task_storage = TARGET / "pyth-native-codegen-interpreter-task-steward.img"
    prepare_storage_image(task_storage)
    task_scenario = SCENARIOS[6]
    serial = run_qemu(
        f"interpreter-{task_scenario.name}",
        task_scenario.interpreter_mode,
        task_storage,
    )
    results[task_scenario.name] = assert_interpreter_trace(serial, task_scenario)

    return results


def run_native_scenarios(native_artifacts: dict[str, Path]) -> dict[str, list[str]]:
    results: dict[str, list[str]] = {}

    for scenario in SCENARIOS[:2]:
        build_native_image(native_artifacts[scenario.name])
        storage = TARGET / f"pyth-native-codegen-native-{scenario.name}.img"
        prepare_storage_image(storage)
        serial = run_qemu(f"native-{scenario.name}", scenario.native_mode, storage)
        results[scenario.name] = assert_native_trace(serial, scenario)

    object_storage = TARGET / "pyth-native-codegen-native-object-flow.img"
    prepare_storage_image(object_storage)
    for scenario in SCENARIOS[2:6]:
        build_native_image(native_artifacts[scenario.name])
        serial = run_qemu(f"native-{scenario.name}", scenario.native_mode, object_storage)
        results[scenario.name] = assert_native_trace(serial, scenario)

    task_scenario = SCENARIOS[6]
    build_native_image(native_artifacts[task_scenario.name])
    task_storage = TARGET / "pyth-native-codegen-native-task-steward.img"
    prepare_storage_image(task_storage)
    serial = run_qemu(f"native-{task_scenario.name}", task_scenario.native_mode, task_storage)
    results[task_scenario.name] = assert_native_trace(serial, task_scenario)

    return results


def main() -> int:
    build_base_artifacts()
    native_artifacts = build_native_artifacts()
    interpreter = run_interpreter_scenarios()
    native = run_native_scenarios(native_artifacts)
    for scenario in SCENARIOS:
        assert_equivalent_trace(scenario, interpreter[scenario.name], native[scenario.name])

    print("PYTH_NATIVE_CODEGEN_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
