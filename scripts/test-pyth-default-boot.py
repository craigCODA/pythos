#!/usr/bin/env python
"""PythTIG Phase 7 default boot acceptance.

Boots the normal path with the default Pyth service graph bundle, proves the
Phase 7 service markers through COM1, then drives the typed object/task syscall
surfaces through the compatibility shell and proves reboot durability.
"""

from __future__ import annotations

import os
import re
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

import launcher_click

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "pyth-default-boot-com1.log"
STORAGE_IMAGE = TARGET / "pyth-default-boot-store.img"
RECOVERY_SERIAL_LOG = TARGET / "pyth-default-recovery-com1.log"
RECOVERY_STORAGE_IMAGE = TARGET / "pyth-default-recovery-store.img"
SHELL_PORT = 4591
SERVICE_PACKAGE_ADMITTED_RE = re.compile(
    r"^PYTHOS:PYTHTIG:SERVICE_PACKAGE_ADMITTED "
    r"service:(session-manager|task-steward) "
    r"package:([0-9A-F]{16}) principal:([0-9A-F]{16}) "
    r"nodes:(\d+) blocks:(\d+)$",
    re.MULTILINE,
)


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
        raise AssertionError(f"{command} failed with {result.returncode}")
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


def parse_task_id(transcript: bytes) -> int:
    match = re.search(rb"TASK_CREATED task:(\d+) active:\1\r\n", transcript)
    if not match:
        raise AssertionError(f"missing task creation response in {transcript!r}")
    return int(match.group(1))


def build_boot_image(core_features: list[str] | None = None) -> None:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    core_command = [
        "cargo",
        "build",
        "-p",
        "pythos-core",
        "--target",
        "x86_64-unknown-none",
    ]
    if core_features:
        core_command += ["--features", ",".join(core_features)]
    run(core_command)
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-pyth-runtime.py"])
    run([sys.executable, "scripts/verify-pyth-runtime-elf.py"])
    run([sys.executable, "scripts/build-pyth-graph.py"])
    run([sys.executable, "scripts/build-image.py", "--with-pythtig-default-services"])


def require_ordered_markers(serial: str, markers: list[str]) -> None:
    cursor = -1
    for marker in markers:
        index = serial.find(marker, cursor + 1)
        if index < 0:
            raise AssertionError(f"missing marker {marker}")
        cursor = index


def require_service_package_admission(
    serial: str, service: str, principal_id: str
) -> str:
    matches = [
        match
        for match in SERVICE_PACKAGE_ADMITTED_RE.finditer(serial)
        if match.group(1) == service
    ]
    if not matches:
        raise AssertionError(f"missing service package admission for {service}")
    for match in matches:
        nodes = int(match.group(4))
        blocks = int(match.group(5))
        if match.group(3) != principal_id:
            raise AssertionError(
                f"unexpected principal for {service}: {match.group(3)}"
            )
        if nodes <= 0 or blocks <= 0:
            raise AssertionError(
                f"admission for {service} did not prove package shape: {match.group(0)}"
            )
    return matches[0].group(2)


def drive_boot_and_reboot() -> None:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
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
            "--storage-image",
            str(STORAGE_IMAGE),
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
        with connect_shell(30) as sock:
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:COM2_READY", 30)
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY", 30)
            serial = wait_for_file_marker(
                SERIAL_LOG, "PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY", 30
            )
            require_ordered_markers(
                serial,
                [
                    "PYTHOS:CORE:NORMAL_BOOT:FAST_PATH",
                    "PYTHOS:PYTHTIG:SERVICE_PACKAGE_ADMITTED service:session-manager",
                    "PYTHOS:PYTHTIG:SESSION_MANAGER_READY",
                    "PYTHOS:PYTHTIG:SERVICE_PACKAGE_ADMITTED service:task-steward",
                    "PYTHOS:PYTHTIG:TASK_STEWARD_READY",
                    "PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY",
                    "PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY",
                ],
            )
            require_service_package_admission(
                serial, "session-manager", "50595448534D0001"
            )
            require_service_package_admission(serial, "task-steward", "5059544853540001")
            launcher_click.click_launcher_tile()
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:SHELL:RING3_ENTER", 30)
            initial = read_until(sock, b"pyth> ", 10)
            if b"PYTHOS:SHELL:READY" not in initial:
                raise AssertionError(f"missing shell ready banner: {initial!r}")

            expect_line(
                send_command(sock, b"create kind:note"),
                b"CREATED object:1042 revision:1\r\n",
                "create note",
            )
            expect_line(
                send_command(sock, b'revise object:1042 text="phase7"'),
                b"COMMITTED revision:2\r\n",
                "revise note",
            )
            task_created = send_command(sock, b'task new "Phase 7 default boot"')
            task_id = parse_task_id(task_created)
            expect_line(
                send_command(sock, b"task active"),
                f"TASK_ACTIVE task:{task_id}\r\n".encode(),
                "active task before reboot",
            )

            sock.sendall(b"reboot\r\n")
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:SYSTEM:REBOOTING", 10)
            wait_for_marker_count(SERIAL_LOG, "PYTHOS:LOADER:ENTER", 2, 30)
            wait_for_marker_count(
                SERIAL_LOG, "PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY", 2, 30
            )
            wait_for_marker_count(
                SERIAL_LOG,
                "PYTHOS:PYTHTIG:SERVICE_PACKAGE_ADMITTED service:session-manager",
                2,
                30,
            )
            wait_for_marker_count(
                SERIAL_LOG,
                "PYTHOS:PYTHTIG:SERVICE_PACKAGE_ADMITTED service:task-steward",
                2,
                30,
            )
            wait_for_marker_count(
                SERIAL_LOG, "PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY", 2, 30
            )
            launcher_click.click_launcher_tile()
            wait_for_marker_count(SERIAL_LOG, "PYTHOS:SHELL:RING3_ENTER", 2, 30)
            second_banner = read_until(sock, b"pyth> ", 30)
            if b"PYTHOS:SHELL:READY" not in second_banner:
                raise AssertionError(
                    f"missing post-reboot shell ready banner: {second_banner!r}"
                )

            expect_line(
                send_command(sock, b"inspect object:1042"),
                b'text="phase7" revision:2\r\n',
                "inspect note after reboot",
            )
            expect_line(
                send_command(sock, b"task active"),
                f"TASK_ACTIVE task:{task_id}\r\n".encode(),
                "active task after reboot",
            )
            print("PYTH_DEFAULT_BOOT_REBOOT_TEST_OK")
    finally:
        terminate_process_tree(process)


def drive_recovery_boot() -> None:
    if RECOVERY_SERIAL_LOG.exists():
        RECOVERY_SERIAL_LOG.unlink()
    if RECOVERY_STORAGE_IMAGE.exists():
        RECOVERY_STORAGE_IMAGE.unlink()
    popen_kwargs: dict[str, object] = {}
    if sys.platform != "win32":
        popen_kwargs["start_new_session"] = True
    process = subprocess.Popen(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(RECOVERY_SERIAL_LOG),
            "--shell-port",
            str(SHELL_PORT),
            "--storage-image",
            str(RECOVERY_STORAGE_IMAGE),
            "--timeout",
            "45",
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
            wait_for_file_marker(RECOVERY_SERIAL_LOG, "PYTHOS:CORE:COM2_READY", 30)
            wait_for_file_marker(
                RECOVERY_SERIAL_LOG, "PYTHOS:PYTHTIG:RECOVERY_SHELL_ENTER", 30
            )
            serial = wait_for_file_marker(
                RECOVERY_SERIAL_LOG, "PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY", 30
            )
            require_ordered_markers(
                serial,
                [
                    "PYTHOS:CORE:NORMAL_BOOT:FAST_PATH",
                    "PYTHOS:PYTHTIG:SERVICE_PACKAGE_ADMITTED service:session-manager",
                    "PYTHOS:CORE:CRASH:USER_FAULT",
                    "PYTHOS:PYTHTIG:SERVICE_FAULT_CONTAINED service:session-manager",
                    "PYTHOS:PYTHTIG:RECOVERY_SHELL_ENTER",
                    "PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY",
                ],
            )
            require_service_package_admission(
                serial, "session-manager", "50595448534D0001"
            )
            launcher_click.click_launcher_tile()
            wait_for_file_marker(RECOVERY_SERIAL_LOG, "PYTHOS:SHELL:RING3_ENTER", 30)
            banner = read_until(sock, b"pyth> ", 10)
            if b"PYTHOS:SHELL:READY" not in banner:
                raise AssertionError(f"missing recovery shell ready banner: {banner!r}")
            print("PYTH_DEFAULT_RECOVERY_TEST_OK")
    finally:
        terminate_process_tree(process)


def main() -> int:
    build_boot_image()
    drive_boot_and_reboot()
    build_boot_image(["pyth-tig-session-manager-fault-test"])
    drive_recovery_boot()
    print("PYTH_DEFAULT_BOOT_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
