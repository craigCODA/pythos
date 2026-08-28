#!/usr/bin/env python
"""QEMU acceptance for physical keyboard ingress into the object shell."""

from __future__ import annotations

import os
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

import launcher_click

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "physical-keyboard-console-com1.log"
SHELL_PORT = 4582


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


def read_until(sock: socket.socket, needle: bytes, timeout: float) -> bytes:
    deadline = time.monotonic() + timeout
    buffer = bytearray()
    sock.settimeout(0.5)
    while time.monotonic() < deadline:
        try:
            chunk = sock.recv(256)
        except socket.timeout:
            continue
        if not chunk:
            raise AssertionError("COM2 shell connection closed")
        buffer.extend(chunk)
        if needle in buffer:
            return bytes(buffer)
    raise AssertionError(f"timed out waiting for {needle!r}; received {bytes(buffer)!r}")


def connect_shell(timeout: float) -> socket.socket:
    deadline = time.monotonic() + timeout
    last_error: OSError | None = None
    while time.monotonic() < deadline:
        try:
            return socket.create_connection(("127.0.0.1", SHELL_PORT), timeout=1)
        except OSError as error:
            last_error = error
            time.sleep(0.1)
    raise AssertionError(f"could not connect to COM2 shell: {last_error}")


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
            "physical-keyboard-console",
        ]
    )
    build_verified_user_shell()
    build_pyth_graph_artifacts()
    run([sys.executable, "scripts/build-image.py", "--with-pythtig-default-services"])


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
            "--shell-port",
            str(SHELL_PORT),
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
        with connect_shell(30) as sock:
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:COM2_READY", 30)
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY", 30)
            launcher_click.click_launcher_tile()
            wait_for_file_marker(
                SERIAL_LOG,
                "PYTHOS:CORE:PHYSICAL_KEYBOARD_CONSOLE:READY",
                30,
            )
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:SHELL:RING3_ENTER", 30)
            initial = read_until(sock, b"pyth> ", 10)
            if b"PYTHOS:SHELL:READY" not in initial:
                raise AssertionError(f"missing shell ready banner: {initial!r}")

            launcher_click.press_qcode_keys(["h", "e", "l", "p", "ret"])
            help_output = read_until(sock, b"reboot\r\npyth> ", 15)
            if b"query kind:note" not in help_output:
                raise AssertionError(f"missing help output: {help_output!r}")

        serial = SERIAL_LOG.read_text(encoding="utf-8", errors="replace")
        if serial.count("PYTHOS:CORE:PHYSICAL_KEYBOARD_CONSOLE:BYTE") < 5:
            raise AssertionError(f"missing physical keyboard byte markers:\n{serial}")
        print("PHYSICAL_KEYBOARD_CONSOLE_TEST_OK")
        return 0
    finally:
        terminate_process_tree(process)


if __name__ == "__main__":
    raise SystemExit(main())
