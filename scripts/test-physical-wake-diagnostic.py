#!/usr/bin/env python
"""QEMU acceptance for the opt-in physical wake diagnostic."""

from __future__ import annotations

import os
import signal
import subprocess
import sys
import time
from pathlib import Path

import launcher_click

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "physical-wake-diagnostic-com1.log"
READY_MARKER = "PYTHOS:CORE:PHYSICAL_WAKE:READY"
ACCEPTED_MARKER = "PYTHOS:CORE:PHYSICAL_WAKE:ACCEPTED"


def run(command: list[str]) -> str:
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


def wait_for_file_marker(path: Path, marker: str, timeout: float) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            text = path.read_text(encoding="utf-8", errors="replace")
            if marker in text:
                return text
        time.sleep(0.1)
    raise AssertionError(f"missing marker {marker}")


def terminate_process_tree(process: subprocess.Popen[str]) -> None:
    if sys.platform == "win32":
        subprocess.run(
            ["taskkill", "/F", "/T", "/PID", str(process.pid)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    else:
        try:
            os.killpg(os.getpgid(process.pid), signal.SIGTERM)
        except ProcessLookupError:
            pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        if sys.platform != "win32":
            try:
                os.killpg(os.getpgid(process.pid), signal.SIGKILL)
            except ProcessLookupError:
                pass
        else:
            process.kill()
        process.wait(timeout=5)


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
            "verify,physical-wake-diagnostic",
        ]
    )
    build_verified_user_shell()
    build_pyth_graph_artifacts()
    run([sys.executable, "scripts/build-image.py"])


def assert_ordered(serial: str, markers: list[str]) -> None:
    cursor = 0
    for marker in markers:
        index = serial.find(marker, cursor)
        if index == -1:
            raise AssertionError(f"missing or out-of-order marker: {marker}\nserial:\n{serial}")
        cursor = index + len(marker)


def main() -> int:
    build_boot_image()
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()

    popen_kwargs: dict[str, object] = {}
    if sys.platform != "win32":
        popen_kwargs["start_new_session"] = True
    process = subprocess.Popen(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--timeout",
            "60",
            "--success-marker",
            ACCEPTED_MARKER,
            "--expect-outcome",
            "success",
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        **popen_kwargs,
    )

    output = ""
    try:
        wait_for_file_marker(SERIAL_LOG, READY_MARKER, 45)
        launcher_click.type_wake()
        output, _ = process.communicate(timeout=20)
        print(output)
        if process.returncode != 0:
            raise AssertionError(f"run-qemu.py returned {process.returncode}")
    finally:
        terminate_process_tree(process)

    serial = SERIAL_LOG.read_text(encoding="utf-8", errors="replace")
    for failure in ("PYTHOS:LOADER:FAIL", "PYTHOS:PANIC", "PYTHOS:CORE:MEMORY_INVALID"):
        if failure in serial:
            raise AssertionError(f"failure marker present: {failure}")
    assert_ordered(
        serial,
        [
            "PYTHOS:CORE:BOOT_SYNC:AUDIO",
            "PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY",
            "PYTHOS:CORE:PHYSICAL_WAKE:ENTER",
            READY_MARKER,
            ACCEPTED_MARKER,
        ],
    )
    print("PHYSICAL_WAKE_DIAGNOSTIC_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
