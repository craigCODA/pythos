#!/usr/bin/env python
"""ADR 0053 acceptance test: prove the real, QEMU-emulated PS/2 IRQ path
drives the interactive launcher's click-to-launch flow - not just that the
decode logic is correct on synthetic bytes (which `input_drivers.rs`'s
existing self-test already covers), but that a QMP-injected keystroke and
mouse click actually reach `ps2.rs`'s real interrupt handlers via real
hardware IRQ1/IRQ12 delivery, and that the shell launches as a result."""

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
SERIAL_LOG = TARGET / "normal-boot-interactive-com1.log"


def run(command: list[str]) -> None:
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
        raise AssertionError(f"{command} failed with {result.returncode}")


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


def build_boot_image() -> None:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none"])
    build_verified_user_shell()
    run([sys.executable, "scripts/build-image.py"])


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
            "30",
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
        wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY", 30)

        # Prove IRQ1 (keyboard) fires for real before touching the mouse -
        # the launcher itself ignores key presses, so this is purely a
        # hardware-path proof, independent of the click flow below.
        launcher_click.press_a_key()
        wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED", 10)

        # Prove IRQ12 (mouse) fires for real and drives the actual
        # click-to-launch state machine through to a real shell launch.
        launcher_click.click_launcher_tile()
        wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:PS2:MOUSE_IRQ_FIRED", 10)
        wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:LAUNCHER:CLICK_CONFIRMED", 10)
        wait_for_file_marker(SERIAL_LOG, "PYTHOS:SHELL:RING3_ENTER", 10)

        returncode = process.wait(timeout=30)
        if returncode != 22:
            raise AssertionError(f"run-qemu.py returned {returncode}, expected 22")
        print("INTERACTIVE_LAUNCHER_TEST_OK")
        return 0
    finally:
        terminate_process_tree(process)


if __name__ == "__main__":
    raise SystemExit(main())
