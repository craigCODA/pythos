#!/usr/bin/env python
"""Run QEMU with OVMF and capture COM1 serial output."""

from __future__ import annotations

import argparse
import os
import shutil
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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--esp", type=Path, default=DEFAULT_ESP)
    parser.add_argument("--serial-log", type=Path, default=DEFAULT_LOG)
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--qemu")
    parser.add_argument("--ovmf-code")
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

    process = subprocess.Popen(command)
    deadline = time.monotonic() + args.timeout
    try:
        while time.monotonic() < deadline:
            if process.poll() is not None:
                break
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

