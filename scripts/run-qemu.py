#!/usr/bin/env python
"""Run QEMU with OVMF and capture COM1 serial output."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ESP = ROOT / "image" / "esp"
DEFAULT_LOG = ROOT / "target" / "boot-serial.log"


def find_qemu(explicit: str | None) -> str:
    if explicit:
        return explicit
    found = shutil.which("qemu-system-x86_64")
    if found:
        return found
    candidates = [
        r"C:\Program Files\qemu\qemu-system-x86_64.exe",
        r"C:\Program Files (x86)\qemu\qemu-system-x86_64.exe",
    ]
    for candidate in candidates:
        if Path(candidate).exists():
            return candidate
    raise SystemExit("missing qemu-system-x86_64 on PATH")


def find_ovmf(explicit: str | None) -> str:
    if explicit:
        return explicit
    env_value = os.environ.get("PYTHOS_OVMF_CODE")
    if env_value:
        return env_value
    candidates = [
        r"C:\Program Files\qemu\share\edk2-x86_64-code.fd",
        r"C:\Program Files\qemu\share\edk2-x86_64-secure-code.fd",
        r"C:\Program Files\qemu\share\OVMF_CODE.fd",
        r"C:\Program Files (x86)\qemu\share\edk2-x86_64-code.fd",
    ]
    for candidate in candidates:
        if Path(candidate).exists():
            return candidate
    raise SystemExit("missing OVMF code firmware; set PYTHOS_OVMF_CODE")


QMP_PORT = 4488


def read_qmp_message(sock_file) -> dict:
    while True:
        line = sock_file.readline()
        if not line:
            raise ConnectionError("QMP connection closed")
        message = json.loads(line)
        if "event" not in message:
            return message


def request_screendump(path: Path) -> None:
    with socket.create_connection(("127.0.0.1", QMP_PORT), timeout=5) as sock:
        sock_file = sock.makefile("rw", encoding="utf-8", newline="\n")
        read_qmp_message(sock_file)
        for command in (
            {"execute": "qmp_capabilities"},
            {
                "execute": "screendump",
                "arguments": {"filename": str(path), "format": "png"},
            },
        ):
            sock_file.write(json.dumps(command) + "\n")
            sock_file.flush()
            reply = read_qmp_message(sock_file)
            if "error" in reply:
                raise RuntimeError(f"QMP error: {reply['error']}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--esp", type=Path, default=DEFAULT_ESP)
    parser.add_argument("--serial-log", type=Path, default=DEFAULT_LOG)
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--qemu")
    parser.add_argument("--ovmf-code")
    parser.add_argument("--screendump", type=Path)
    args = parser.parse_args()

    qemu = find_qemu(args.qemu)
    ovmf = find_ovmf(args.ovmf_code)
    args.serial_log.parent.mkdir(parents=True, exist_ok=True)
    if args.serial_log.exists():
        args.serial_log.unlink()

    command = [
        qemu,
        "-machine",
        "q35",
        "-cpu",
        "qemu64",
        "-smp",
        "1",
        "-m",
        "512M",
        "-drive",
        f"if=pflash,format=raw,readonly=on,file={ovmf}",
        "-drive",
        f"format=raw,file=fat:rw:{args.esp}",
        "-serial",
        f"file:{args.serial_log}",
        "-display",
        "none",
        "-no-reboot",
        "-no-shutdown",
    ]
    if args.screendump:
        args.screendump.parent.mkdir(parents=True, exist_ok=True)
        command += ["-qmp", f"tcp:127.0.0.1:{QMP_PORT},server=on,wait=off"]

    process = subprocess.Popen(command)
    deadline = time.monotonic() + args.timeout
    screendump_at = deadline - 2.0
    screendump_pending = args.screendump is not None
    try:
        while time.monotonic() < deadline:
            if process.poll() is not None:
                break
            if screendump_pending and time.monotonic() >= screendump_at:
                screendump_pending = False
                try:
                    request_screendump(args.screendump.resolve())
                except (OSError, RuntimeError, ConnectionError) as error:
                    print(f"screendump failed: {error}", file=sys.stderr)
            time.sleep(0.1)
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)

    print(args.serial_log.read_text(encoding="utf-8", errors="replace") if args.serial_log.exists() else "")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

