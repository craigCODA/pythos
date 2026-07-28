#!/usr/bin/env python
"""COM2 transport smoke test for the normal shell path."""

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
SERIAL_LOG = TARGET / "com2-transport-com1.log"
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
    # stdout is discarded (not piped): run-qemu.py mostly prints only at exit,
    # but nothing drains a PIPE while we poll below, and Windows can deadlock
    # a child on a full pipe buffer. We only need the COM1 log file and the
    # COM2 socket, not this process's stdout.
    #
    # start_new_session=True (POSIX only; harmless no-op-ish on Windows, but
    # we use taskkill there anyway) puts run-qemu.py and everything it spawns
    # in a new process group, so cleanup can signal the whole group instead of
    # just the direct child.
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
        # Connect before watching COM1 markers. With QEMU's wait=off TCP
        # serial backend, bytes written before a client attaches are discarded;
        # normal boot can reach shell entry faster than the log poll interval.
        with connect_shell(30) as sock:
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:COM2_READY", 30)
            # ADR 0053: normal boot now blocks on a real click before
            # launching the shell - inject one over QMP.
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY", 30)
            launcher_click.click_launcher_tile()
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:SHELL:RING3_ENTER", 30)
            initial = read_until(sock, b"pyth> ", 10)
            if b"PYTHOS:SHELL:READY" not in initial:
                raise AssertionError(f"missing shell ready banner: {initial!r}")
            sock.sendall(b"help\r\n")
            help_output = read_until(sock, b"reboot\r\npyth> ", 10)
            if b"query kind:note" not in help_output:
                raise AssertionError(f"missing help output: {help_output!r}")
        print("COM2_TRANSPORT_TEST_OK")
        return 0
    finally:
        # On Windows, Popen.terminate()/kill() only signal the direct child
        # (this run-qemu.py Python process); TerminateProcess bypasses its own
        # `finally` cleanup, so its qemu-system-x86_64.exe grandchild is
        # orphaned rather than reaped. Kill the whole process tree instead:
        # taskkill /T on Windows, the process group (via start_new_session
        # above) on POSIX.
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


if __name__ == "__main__":
    raise SystemExit(main())
