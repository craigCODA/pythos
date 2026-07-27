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
        wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:COM2_READY", 20)
        with socket.create_connection(("127.0.0.1", SHELL_PORT), timeout=5) as sock:
            # Prove COM2 actually reads and writes, not just that init_com2()
            # ran: normal_boot's idle loop echoes every byte it reads back
            # (Task 2 temporary proof; Task 8 replaces it with the real shell).
            probe = b"X"
            sock.sendall(probe)
            sock.settimeout(5)
            echoed = sock.recv(1)
            if echoed != probe:
                raise AssertionError(f"COM2 echo mismatch: sent {probe!r}, got {echoed!r}")
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
