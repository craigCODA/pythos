#!/usr/bin/env python
"""Phase 13 independent-package end-to-end QEMU acceptance."""

from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target" / "phase13-independent-package"
CORE_TARGET = ROOT / "target" / "phase13-independent-package-core"
CORE_ELF = CORE_TARGET / "x86_64-unknown-none" / "debug" / "pythcore"
PACKAGE_ARTIFACT = TARGET / "independent-seed.pkg"
PACKAGE_LABEL = "phase13-independent-seed.pkg"
SERIAL_LOG = TARGET / "independent-package.log"
STORAGE_IMAGE = TARGET / "independent-package.img"

COMMON_FORBIDDEN = (
    "PYTHOS:LOADER:FAIL",
    "PYTHOS:PANIC",
    "PYTHOS:EXCEPTION",
    "vector=0x",
    "PYTHOS:PYTHTIG:RUNTIME_FAULT",
    "PYTHOS:CORE:INDEPENDENT_PACKAGE:INSTANCE_LOST",
    "PYTHOS:CORE:INDEPENDENT_PACKAGE:SCHEMA_LOST",
    "PYTHOS:CORE:PACKAGE_SESSION_RUNTIME_READY",
    "PYTHOS:CORE:WAKE_CONTEXT_READY",
    "PYTHOS:CORE:KAI_READY",
)

REQUIRED_MARKERS = (
    "PYTHOS:CORE:INDEPENDENT_PACKAGE:INSTALLED",
    "PYTHOS:CORE:INDEPENDENT_PACKAGE:LAUNCHED",
    "PYTHOS:PYTHTIG:RUNTIME_ENTER package:",
    "PYTHOS:PYTHTIG:OBJECT_FLOW_ACCEPTANCE_COMPLETE",
    "PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:",
    "PYTHOS:CORE:INDEPENDENT_PACKAGE:OBJECT_CREATED",
    "PYTHOS:CORE:INDEPENDENT_PACKAGE:UNINSTALLED",
    "PYTHOS:CORE:INDEPENDENT_PACKAGE:INSTANCE_RESTORED",
    "PYTHOS:CORE:INDEPENDENT_PACKAGE_READY",
    "PYTHOS:CORE:PACKAGE_SCHEMA_EXTENSIBILITY_READY",
    "PYTHOS:CORE:PHASE_13_COMPLETE",
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
    run([sys.executable, "scripts/build-pyth-graph.py"])


def build_pyth_runtime_artifacts() -> None:
    run([sys.executable, "scripts/build-pyth-runtime.py"])
    run([sys.executable, "scripts/verify-pyth-runtime-elf.py"])


def build_independent_package_artifact() -> None:
    TARGET.mkdir(parents=True, exist_ok=True)
    run(
        [
            sys.executable,
            "scripts/build-phase13-package-fixture.py",
            "--fixture",
            "independent-seed",
            "--output",
            str(PACKAGE_ARTIFACT),
        ]
    )


def build_boot_image() -> None:
    build_independent_package_artifact()
    build_pyth_graph_artifacts()
    build_pyth_runtime_artifacts()
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
            "--features",
            "verify,phase13-package-test",
        ]
    )
    build_verified_user_shell()
    source_spec = f"{PACKAGE_ARTIFACT.relative_to(ROOT).as_posix()}:{PACKAGE_LABEL}"
    run(
        [
            sys.executable,
            "scripts/build-image.py",
            "--kernel",
            str(CORE_ELF),
            "--with-pythtig",
            "--phase13-package-source",
            source_spec,
        ]
    )


def wait_for_file_marker(serial_log: Path, marker: str, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if serial_log.exists() and marker in serial_log.read_text(errors="replace"):
            return
        time.sleep(0.1)
    observed = serial_log.read_text(errors="replace") if serial_log.exists() else ""
    raise AssertionError(f"timed out waiting for {marker!r}:\n{observed}")


def terminate_process_tree(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    if sys.platform == "win32":
        subprocess.run(
            ["taskkill", "/F", "/T", "/PID", str(process.pid)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    else:
        os.killpg(os.getpgid(process.pid), 15)
    process.wait(timeout=10)


def run_three_boot_qemu() -> str:
    for path in (SERIAL_LOG, STORAGE_IMAGE):
        if path.exists():
            path.unlink()

    popen_kwargs: dict[str, object] = {}
    if sys.platform != "win32":
        popen_kwargs["start_new_session"] = True
    process = subprocess.Popen(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--storage-image",
            str(STORAGE_IMAGE),
            "--timeout",
            "60",
            "--expect-outcome",
            "timeout",
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        **popen_kwargs,
    )
    try:
        wait_for_file_marker(
            SERIAL_LOG, "PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:", 45
        )
    finally:
        terminate_process_tree(process)

    boot_one = SERIAL_LOG.read_text(errors="replace")
    boot_two = run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--storage-image",
            str(STORAGE_IMAGE),
            "--timeout",
            "60",
            "--success-marker",
            "PYTHOS:CORE:INDEPENDENT_PACKAGE:UNINSTALLED",
            "--expect-outcome",
            "success",
        ]
    )
    boot_three = run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--storage-image",
            str(STORAGE_IMAGE),
            "--timeout",
            "60",
            "--success-marker",
            "PYTHOS:CORE:PHASE_13_COMPLETE",
            "--expect-outcome",
            "success",
        ]
    )
    return boot_one + "\n" + boot_two + "\n" + boot_three


def serial_lines(output: str) -> list[str]:
    return [line.strip() for line in output.splitlines() if line.strip()]


def assert_ordered_markers(lines: list[str], markers: tuple[str, ...]) -> None:
    cursor = -1
    for marker in markers:
        matches = [
            index
            for index, line in enumerate(lines)
            if line == marker or (marker.endswith(":") and line.startswith(marker))
        ]
        if len(matches) != 1:
            raise AssertionError(f"expected one {marker!r}, saw {len(matches)}")
        if matches[0] <= cursor:
            raise AssertionError(f"out-of-order marker {marker!r}")
        cursor = matches[0]


def reject_forbidden(lines: list[str]) -> None:
    for marker in COMMON_FORBIDDEN:
        if any(marker in line for line in lines):
            raise AssertionError(f"forbidden marker {marker}")


def main() -> int:
    build_boot_image()
    output = run_three_boot_qemu()
    lines = serial_lines(output)
    reject_forbidden(lines)
    assert_ordered_markers(lines, REQUIRED_MARKERS)
    if "QEMU_OUTCOME success" not in output:
        raise AssertionError("missing QEMU_OUTCOME success")
    print("PHASE13_INDEPENDENT_PACKAGE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
