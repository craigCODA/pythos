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
from enum import Enum
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ESP = ROOT / "image" / "esp"
DEFAULT_ISO = ROOT / "target" / "pythos.iso"
DEFAULT_LOG = ROOT / "target" / "boot-serial.log"
DEFAULT_STORAGE_IMAGE = ROOT / "target" / "pythos-store.img"
DEFAULT_STORAGE_SIZE_BYTES = 16 * 1024 * 1024
DEFAULT_TIMEOUT_SECONDS = 20.0
SUCCESS_MARKER = "PYTHOS:CORE:MILESTONE_1_COMPLETE"
DEBUG_EXIT_CODES = {
    "success": 0x10,
    "panic": 0x11,
}
SCRIPT_EXIT_CODES = {
    "success": 0,
    "panic": 20,
    "reset": 21,
    "timeout": 22,
    "marker-order-violation": 23,
}
PANIC_MARKERS = (
    "PYTHOS:LOADER:FAIL",
    "PYTHOS:PANIC",
    "PYTHOS:CORE:BOOTINFO_INVALID",
    "PYTHOS:CORE:MEMORY_INVALID",
)


class QemuOutcome(str, Enum):
    SUCCESS = "success"
    PANIC = "panic"
    RESET = "reset"
    TIMEOUT = "timeout"
    MARKER_ORDER_VIOLATION = "marker-order-violation"


def debug_exit_status(outcome: str) -> int:
    return (DEBUG_EXIT_CODES[outcome] << 1) | 1


def classify_qemu_exit(
    returncode: int | None,
    serial: str,
    timed_out: bool = False,
) -> QemuOutcome:
    if any(marker in serial for marker in PANIC_MARKERS):
        return QemuOutcome.PANIC
    if SUCCESS_MARKER in serial:
        return QemuOutcome.SUCCESS
    if returncode == debug_exit_status("success"):
        return QemuOutcome.SUCCESS
    if returncode == debug_exit_status("panic"):
        return QemuOutcome.PANIC
    if timed_out or returncode is None:
        return QemuOutcome.TIMEOUT
    return QemuOutcome.RESET


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


def request_qmp_quit() -> None:
    with socket.create_connection(("127.0.0.1", QMP_PORT), timeout=5) as sock:
        sock_file = sock.makefile("rw", encoding="utf-8", newline="\n")
        read_qmp_message(sock_file)
        for command in (
            {"execute": "qmp_capabilities"},
            {"execute": "quit"},
        ):
            sock_file.write(json.dumps(command) + "\n")
            sock_file.flush()
            reply = read_qmp_message(sock_file)
            if "error" in reply:
                raise RuntimeError(f"QMP error: {reply['error']}")


def read_serial_log(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace") if path.exists() else ""


def ensure_storage_image(path: Path, size_bytes: int = DEFAULT_STORAGE_SIZE_BYTES) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        with path.open("wb") as image:
            image.truncate(size_bytes)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--esp", type=Path, default=DEFAULT_ESP)
    parser.add_argument("--iso", type=Path)
    parser.add_argument("--serial-log", type=Path, default=DEFAULT_LOG)
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT_SECONDS)
    parser.add_argument("--qemu")
    parser.add_argument("--ovmf-code")
    parser.add_argument("--screendump", type=Path)
    parser.add_argument("--expect-outcome", choices=[outcome.value for outcome in QemuOutcome])
    parser.add_argument("--no-audio-device", action="store_true")
    parser.add_argument("--audio-wav", type=Path)
    parser.add_argument("--storage-image", type=Path, default=DEFAULT_STORAGE_IMAGE)
    args = parser.parse_args()

    qemu = find_qemu(args.qemu)
    ovmf = find_ovmf(args.ovmf_code)
    if args.iso and args.esp != DEFAULT_ESP:
        raise SystemExit("--esp and --iso are mutually exclusive")
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
        "-serial",
        f"file:{args.serial_log}",
        "-display",
        "none",
        "-no-reboot",
        "-no-shutdown",
        "-device",
        "isa-debug-exit,iobase=0x501,iosize=0x04",
    ]
    if args.audio_wav and args.no_audio_device:
        raise SystemExit("--audio-wav requires an audio device")
    if not args.no_audio_device:
        if args.audio_wav:
            args.audio_wav.parent.mkdir(parents=True, exist_ok=True)
            if args.audio_wav.exists():
                args.audio_wav.unlink()
            command += [
                "-audiodev",
                f"wav,id=pythos_audio,path={args.audio_wav}",
            ]
        else:
            command += [
                "-audiodev",
                "none,id=pythos_audio",
            ]
        command += [
            "-device",
            "AC97,audiodev=pythos_audio",
        ]
    if args.iso:
        command += [
            "-drive",
            f"if=ide,media=cdrom,readonly=on,file={args.iso}",
            "-boot",
            "order=d",
        ]
    else:
        command += [
            "-drive",
            f"if=none,id=pythos_esp,format=raw,file=fat:rw:{args.esp}",
            "-device",
            "ide-hd,drive=pythos_esp,bootindex=1",
        ]
    ensure_storage_image(args.storage_image)
    command += [
        "-drive",
        f"if=none,id=pythos_store,format=raw,file={args.storage_image}",
        "-device",
        "virtio-blk-pci,drive=pythos_store,disable-modern=on,disable-legacy=off,bootindex=-1",
    ]
    if args.screendump:
        args.screendump.parent.mkdir(parents=True, exist_ok=True)
    command += ["-qmp", f"tcp:127.0.0.1:{QMP_PORT},server=on,wait=off"]

    process = subprocess.Popen(command)
    deadline = time.monotonic() + args.timeout
    screendump_at = deadline - 2.0
    screendump_pending = args.screendump is not None
    timed_out = False
    requested_quit = False
    try:
        while time.monotonic() < deadline:
            if process.poll() is not None:
                break
            serial = read_serial_log(args.serial_log)
            if not requested_quit:
                terminal_outcome = None
                if any(marker in serial for marker in PANIC_MARKERS):
                    terminal_outcome = QemuOutcome.PANIC
                elif SUCCESS_MARKER in serial:
                    terminal_outcome = QemuOutcome.SUCCESS
                if terminal_outcome is not None:
                    if screendump_pending:
                        screendump_pending = False
                        try:
                            request_screendump(args.screendump.resolve())
                        except (OSError, RuntimeError, ConnectionError) as error:
                            print(f"screendump failed: {error}", file=sys.stderr)
                    requested_quit = True
                    try:
                        request_qmp_quit()
                    except (OSError, RuntimeError, ConnectionError) as error:
                        print(f"qmp quit failed: {error}", file=sys.stderr)
                        process.terminate()
            if screendump_pending and time.monotonic() >= screendump_at:
                screendump_pending = False
                try:
                    request_screendump(args.screendump.resolve())
                except (OSError, RuntimeError, ConnectionError) as error:
                    print(f"screendump failed: {error}", file=sys.stderr)
            time.sleep(0.1)
    finally:
        if process.poll() is None:
            timed_out = True
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)

    serial = read_serial_log(args.serial_log)
    print(serial)
    outcome = classify_qemu_exit(process.returncode, serial, timed_out)
    print(f"QEMU_OUTCOME {outcome.value}")
    if args.expect_outcome and outcome.value != args.expect_outcome:
        print(
            f"expected QEMU outcome {args.expect_outcome}, got {outcome.value}",
            file=sys.stderr,
        )
        return SCRIPT_EXIT_CODES[outcome.value]
    return SCRIPT_EXIT_CODES[outcome.value]


if __name__ == "__main__":
    raise SystemExit(main())
