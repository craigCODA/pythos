#!/usr/bin/env python
"""Task 8/9/10 acceptance test: persistent ring-3 shell launch, a
repeatable capability-gated `reboot` that actually resets the machine, and
the typed object lifecycle (create/inspect/revise/history) with exact
response text asserted per operation, including durability: object state
created and revised before `reboot` is still present (inspect/history)
after the machine actually resets and the shell comes back up."""

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
SERIAL_LOG = TARGET / "object-shell-com1.log"
STORAGE_IMAGE = TARGET / "pythos-store.img"
SHELL_PORT = 4583


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


def wait_for_marker_count(path: Path, marker: str, count: int, timeout: float) -> str:
    deadline = time.monotonic() + timeout
    text = ""
    while time.monotonic() < deadline:
        if path.exists():
            text = path.read_text(encoding="utf-8", errors="replace")
            if text.count(marker) >= count:
                return text
        time.sleep(0.1)
    raise AssertionError(
        f"expected {marker!r} at least {count} time(s); saw {text.count(marker)}"
    )


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


def send_command(sock: socket.socket, line: bytes, timeout: float = 10) -> bytes:
    sock.sendall(line + b"\r\n")
    return read_until(sock, b"pyth> ", timeout)


def expect_line(transcript: bytes, expected: bytes, context: str) -> None:
    if expected not in transcript:
        raise AssertionError(f"{context}: expected {expected!r} in {transcript!r}")


def build_verified_user_shell() -> None:
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])


def build_boot_image() -> None:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none"])
    build_verified_user_shell()
    run([sys.executable, "scripts/build-image.py"])


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


def main() -> int:
    build_boot_image()
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    # Task 10's lifecycle asserts a specific deterministic object id
    # (1042, the first `next_shell_note_id`); a storage image left over
    # from an earlier run would already have consumed that id.
    if STORAGE_IMAGE.exists():
        STORAGE_IMAGE.unlink()
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
            "90",
            "--allow-reboot",
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
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:SHELL:RING3_ENTER", 30)
            initial = read_until(sock, b"pyth> ", 10)
            if b"PYTHOS:SHELL:READY" not in initial:
                raise AssertionError(f"missing shell ready banner: {initial!r}")
            sock.sendall(b"help\r\n")
            help_output = read_until(sock, b"reboot\r\npyth> ", 10)
            if b"query kind:note" not in help_output:
                raise AssertionError(f"missing help output: {help_output!r}")
            print("OBJECT_SHELL_TASK8_TEST_OK")

            # Task 10: the full typed object lifecycle, asserting the exact
            # response text ADR 0051 defines for each operation - not just
            # success markers. `next_shell_note_id` starts at 1042 on a fresh
            # boot, so the first created object is deterministically 1042.
            expect_line(
                send_command(sock, b"create kind:note"),
                b"CREATED object:1042 revision:1\r\n",
                "create",
            )
            expect_line(
                send_command(sock, b"inspect object:1042"),
                b'text="" revision:1\r\n',
                "inspect before revise",
            )
            expect_line(
                send_command(sock, b'revise object:1042 text="hello"'),
                b"COMMITTED revision:2\r\n",
                "revise",
            )
            expect_line(
                send_command(sock, b"inspect object:1042"),
                b'text="hello" revision:2\r\n',
                "inspect after revise, before reboot",
            )
            expect_line(
                send_command(sock, b"history object:1042"),
                b"history object:1042 revisions:2\r\n",
                "history before reboot",
            )
            # Two more revise cycles (generations 3 and 4) push persistence
            # through both ADR 0052 checkpoint slots twice each - regression
            # coverage for the block-device stack-overflow bug where only
            # repeated persist cycles within one boot reproduced.
            expect_line(
                send_command(sock, b'revise object:1042 text="hello again"'),
                b"COMMITTED revision:3\r\n",
                "second revise",
            )
            expect_line(
                send_command(sock, b'revise object:1042 text="final"'),
                b"COMMITTED revision:4\r\n",
                "third revise",
            )
            expect_line(
                send_command(sock, b"inspect object:1042"),
                b'text="final" revision:4\r\n',
                "inspect after repeated revises, before reboot",
            )
            print("OBJECT_SHELL_TASK10_LIFECYCLE_BEFORE_REBOOT_OK")

            # Task 9: `reboot` must actually reset the machine (a second real
            # boot, not a stub that only prints markers) and the shell must
            # come back up cleanly, proving the reset path is repeatable.
            sock.sendall(b"reboot\r\n")
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:SYSTEM:REBOOTING", 10)
            wait_for_marker_count(SERIAL_LOG, "PYTHOS:LOADER:ENTER", 2, 30)
            wait_for_marker_count(SERIAL_LOG, "PYTHOS:SHELL:RING3_ENTER", 2, 30)
            second_banner = read_until(sock, b"pyth> ", 30)
            if b"PYTHOS:SHELL:READY" not in second_banner:
                raise AssertionError(
                    f"missing post-reboot shell ready banner: {second_banner!r}"
                )
            print("OBJECT_SHELL_TASK9_REBOOT_TEST_OK")

            # Task 10 durability: the object created and revised before
            # `reboot` must still be there after the machine actually
            # resets, restored from the ADR 0052 checkpoint written by
            # `retained_services::persist_object_service` on create/revise.
            expect_line(
                send_command(sock, b"inspect object:1042"),
                b'text="final" revision:4\r\n',
                "inspect after reboot",
            )
            expect_line(
                send_command(sock, b"history object:1042"),
                b"history object:1042 revisions:4\r\n",
                "history after reboot",
            )
            print("OBJECT_SHELL_TASK10_PERSISTENCE_AFTER_REBOOT_OK")
        return 0
    finally:
        terminate_process_tree(process)


if __name__ == "__main__":
    raise SystemExit(main())
