#!/usr/bin/env python
"""COM2 transport smoke test for the normal shell path."""

from __future__ import annotations

import socket
import subprocess
import sys
import time
from pathlib import Path

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


def main() -> int:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none"])
    run([sys.executable, "scripts/build-image.py"])
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    # stdout is discarded (not piped): run-qemu.py mostly prints only at exit,
    # but nothing drains a PIPE while we poll below, and Windows can deadlock
    # a child on a full pipe buffer. We only need the COM1 log file and the
    # COM2 socket, not this process's stdout.
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
    )
    try:
        wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:COM2_READY", 20)
        with socket.create_connection(("127.0.0.1", SHELL_PORT), timeout=5) as sock:
            sock.sendall(b"\n")
        print("COM2_TRANSPORT_TEST_OK")
        return 0
    finally:
        # On Windows, Popen.terminate()/kill() only signal the direct child
        # (this run-qemu.py Python process); TerminateProcess bypasses its own
        # `finally` cleanup, so its qemu-system-x86_64.exe grandchild is
        # orphaned rather than reaped. Kill the whole process tree instead.
        if sys.platform == "win32":
            subprocess.run(
                ["taskkill", "/F", "/T", "/PID", str(process.pid)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
        else:
            process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


if __name__ == "__main__":
    raise SystemExit(main())
