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
DEFAULT_EMMC_IMAGE = ROOT / "target" / "pythos-emmc.img"
DEFAULT_XHCI_USB_STORAGE_IMAGE = ROOT / "target" / "pythos-xhci-usb-storage.img"
DEFAULT_STORAGE_SIZE_BYTES = 16 * 1024 * 1024
DEFAULT_EMMC_SIZE_BYTES = 32 * 1024 * 1024
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
    success_marker: str = SUCCESS_MARKER,
) -> QemuOutcome:
    if any(marker in serial for marker in PANIC_MARKERS):
        return QemuOutcome.PANIC
    if success_marker in serial:
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


def run_qmp_commands(commands: tuple[dict, ...]) -> None:
    with socket.create_connection(("127.0.0.1", QMP_PORT), timeout=5) as sock:
        sock_file = sock.makefile("rw", encoding="utf-8", newline="\n")
        read_qmp_message(sock_file)
        for command in ({"execute": "qmp_capabilities"},) + commands:
            sock_file.write(json.dumps(command) + "\n")
            sock_file.flush()
            reply = read_qmp_message(sock_file)
            if "error" in reply:
                raise RuntimeError(f"QMP error: {reply['error']}")


def request_screendump(path: Path) -> None:
    run_qmp_commands(
        (
            {
                "execute": "screendump",
                "arguments": {"filename": str(path), "format": "ppm"},
            },
        )
    )


def request_qmp_quit() -> None:
    run_qmp_commands(({"execute": "quit"},))


def request_usb_mouse_hotplug(port: str) -> None:
    run_qmp_commands(
        (
            {
                "execute": "device_add",
                "arguments": {
                    "driver": "usb-mouse",
                    "id": "pythos_hotplug_mouse",
                    "bus": "pythos_xhci.0",
                    "port": port,
                },
            },
        )
    )


def request_device_removal(device_id: str) -> None:
    run_qmp_commands(
        (
            {
                "execute": "device_del",
                "arguments": {"id": device_id},
            },
        )
    )


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
    parser.add_argument(
        "--success-marker",
        default=SUCCESS_MARKER,
        help="serial marker that lets the runner classify the boot as success",
    )
    parser.add_argument("--no-audio-device", action="store_true")
    parser.add_argument("--audio-wav", type=Path)
    parser.add_argument("--hda", action="store_true", help="add an Intel HDA controller")
    parser.add_argument(
        "--display",
        default="none",
        help="QEMU -display value (e.g. gtk, sdl); default none for headless CI",
    )
    parser.add_argument(
        "--audio-backend",
        default="none",
        help="QEMU audiodev backend (e.g. dsound to hear it on Windows speakers); default none",
    )
    parser.add_argument(
        "--shell-port",
        type=int,
        help="add a second serial backend (COM2) as a TCP server on this port",
    )
    parser.add_argument("--storage-image", type=Path, default=DEFAULT_STORAGE_IMAGE)
    parser.add_argument(
        "--no-virtio-blk",
        action="store_true",
        help="do not attach the default legacy virtio-blk storage device",
    )
    parser.add_argument(
        "--ahci",
        action="store_true",
        help="attach a polling-test SATA disk behind an explicit AHCI controller",
    )
    parser.add_argument(
        "--ahci-storage-image",
        type=Path,
        help="storage image for --ahci; defaults to --storage-image when omitted",
    )
    parser.add_argument(
        "--sdhci",
        action="store_true",
        help="attach an SDHCI PCI controller",
    )
    parser.add_argument(
        "--emmc",
        action="store_true",
        help="attach a QEMU eMMC card behind --sdhci",
    )
    parser.add_argument(
        "--emmc-image",
        type=Path,
        default=DEFAULT_EMMC_IMAGE,
        help="storage image for --emmc; created if missing",
    )
    parser.add_argument(
        "--xhci",
        action="store_true",
        help="attach a qemu-xhci PCI USB controller",
    )
    parser.add_argument(
        "--usb-mouse",
        action="store_true",
        help="attach a USB mouse to the qemu-xhci controller",
    )
    parser.add_argument(
        "--xhci-usb-storage",
        action="store_true",
        help="attach a removable USB storage device to the qemu-xhci controller",
    )
    parser.add_argument(
        "--xhci-usb-storage-image",
        type=Path,
        default=DEFAULT_XHCI_USB_STORAGE_IMAGE,
        help="storage image for --xhci-usb-storage",
    )
    parser.add_argument(
        "--xhci-usb-storage-port",
        default="1",
        help="qemu-xhci port for --xhci-usb-storage",
    )
    parser.add_argument(
        "--remove-usb-device-after-marker",
        help="QMP-remove the USB device id after this serial marker appears",
    )
    parser.add_argument(
        "--remove-usb-device-id",
        default="pythos_boot_usb",
        help="QMP device id to remove for --remove-usb-device-after-marker",
    )
    parser.add_argument(
        "--hotplug-usb-mouse-after-marker",
        help="QMP hotplug a USB mouse after this serial marker appears",
    )
    parser.add_argument(
        "--hotplug-usb-mouse-port",
        default="1",
        help="qemu-xhci port for --hotplug-usb-mouse-after-marker",
    )
    parser.add_argument("--kill-after-marker")
    parser.add_argument(
        "--allow-reboot",
        action="store_true",
        help="drop -no-reboot so a guest-triggered reset (e.g. i8042 pulse "
        "reset) actually restarts the VM instead of QEMU exiting; only the "
        "reboot acceptance test opts into this",
    )
    args = parser.parse_args()

    qemu = find_qemu(args.qemu)
    ovmf = find_ovmf(args.ovmf_code)
    if args.iso and args.esp != DEFAULT_ESP:
        raise SystemExit("--esp and --iso are mutually exclusive")
    if args.ahci_storage_image and not args.ahci:
        raise SystemExit("--ahci-storage-image requires --ahci")
    if args.emmc and not args.sdhci:
        raise SystemExit("--emmc requires --sdhci")
    if args.usb_mouse and not args.xhci:
        raise SystemExit("--usb-mouse requires --xhci")
    if args.xhci_usb_storage and not args.xhci:
        raise SystemExit("--xhci-usb-storage requires --xhci")
    if args.remove_usb_device_after_marker and not args.xhci:
        raise SystemExit("--remove-usb-device-after-marker requires --xhci")
    if args.hotplug_usb_mouse_after_marker and not args.xhci:
        raise SystemExit("--hotplug-usb-mouse-after-marker requires --xhci")
    if args.usb_mouse and args.hotplug_usb_mouse_after_marker:
        raise SystemExit("--usb-mouse and --hotplug-usb-mouse-after-marker are mutually exclusive")
    args.serial_log.parent.mkdir(parents=True, exist_ok=True)
    if args.serial_log.exists():
        args.serial_log.unlink()

    command = [qemu]
    command += [
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
        args.display,
        "-no-shutdown",
        "-device",
        "isa-debug-exit,iobase=0x501,iosize=0x04",
    ]
    if not args.allow_reboot:
        command.append("-no-reboot")
    if args.shell_port is not None:
        # QEMU assigns successive -serial flags to COM1, COM2, ... in order;
        # this becomes COM2, the interactive object-shell transport (ADR 0052).
        command += [
            "-serial",
            f"tcp:127.0.0.1:{args.shell_port},server=on,wait=off",
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
                f"{args.audio_backend},id=pythos_audio",
            ]
        command += [
            "-device",
            "AC97,audiodev=pythos_audio",
        ]
        if args.hda:
            command += [
                "-device",
                "intel-hda",
                "-device",
                "hda-output,audiodev=pythos_audio",
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
    if args.ahci:
        ahci_storage_image = args.ahci_storage_image or args.storage_image
        ensure_storage_image(ahci_storage_image)
        command += [
            "-device",
            "ich9-ahci,id=pythos_ahci,bus=pcie.0,addr=0x5",
            "-drive",
            (
                "if=none,id=pythos_ahci_store,format=raw,cache=writethrough,"
                f"file={ahci_storage_image}"
            ),
            "-device",
            "ide-hd,drive=pythos_ahci_store,bus=pythos_ahci.0,bootindex=-1",
        ]
    if args.sdhci:
        command += [
            "-device",
            "sdhci-pci,id=pythos_sdhci,bus=pcie.0,addr=0x6",
        ]
        if args.emmc:
            ensure_storage_image(args.emmc_image, DEFAULT_EMMC_SIZE_BYTES)
            command += [
                "-drive",
                f"if=none,id=pythos_emmc,format=raw,cache=writethrough,file={args.emmc_image}",
                "-device",
                "emmc,drive=pythos_emmc,bus=sd-bus",
            ]
    if args.xhci:
        command += [
            "-device",
            "qemu-xhci,id=pythos_xhci,bus=pcie.0,addr=0x7",
        ]
        if args.usb_mouse:
            command += [
                "-device",
                "usb-mouse,bus=pythos_xhci.0,port=1",
            ]
        if args.xhci_usb_storage:
            ensure_storage_image(args.xhci_usb_storage_image)
            command += [
                "-drive",
                (
                    "if=none,id=pythos_xhci_usb_store,format=raw,readonly=on,"
                    f"file={args.xhci_usb_storage_image}"
                ),
                "-device",
                (
                    "usb-storage,id=pythos_boot_usb,drive=pythos_xhci_usb_store,"
                    f"bus=pythos_xhci.0,port={args.xhci_usb_storage_port},bootindex=-1"
                ),
            ]
    if not args.no_virtio_blk:
        ensure_storage_image(args.storage_image)
        command += [
            "-drive",
            f"if=none,id=pythos_store,format=raw,cache=writethrough,file={args.storage_image}",
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
    hotplug_done = False
    hotplug_error = None
    remove_done = False
    remove_error = None
    try:
        while time.monotonic() < deadline:
            if process.poll() is not None:
                break
            serial = read_serial_log(args.serial_log)
            if (
                args.remove_usb_device_after_marker
                and not remove_done
                and args.remove_usb_device_after_marker in serial
            ):
                try:
                    request_device_removal(args.remove_usb_device_id)
                except (OSError, RuntimeError, ConnectionError) as error:
                    remove_error = error
                    print(f"usb device removal failed: {error}", file=sys.stderr)
                    process.terminate()
                    break
                remove_done = True
            if (
                args.hotplug_usb_mouse_after_marker
                and not hotplug_done
                and args.hotplug_usb_mouse_after_marker in serial
            ):
                try:
                    request_usb_mouse_hotplug(args.hotplug_usb_mouse_port)
                except (OSError, RuntimeError, ConnectionError) as error:
                    hotplug_error = error
                    print(f"usb mouse hotplug failed: {error}", file=sys.stderr)
                    process.terminate()
                    break
                hotplug_done = True
            if args.kill_after_marker and args.kill_after_marker in serial:
                process.kill()
                process.wait(timeout=2)
                break
            if not requested_quit:
                terminal_outcome = None
                if any(marker in serial for marker in PANIC_MARKERS):
                    terminal_outcome = QemuOutcome.PANIC
                elif args.success_marker in serial:
                    terminal_outcome = QemuOutcome.SUCCESS
                if terminal_outcome is not None:
                    if args.screendump:
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
    outcome = classify_qemu_exit(
        process.returncode,
        serial,
        timed_out,
        success_marker=args.success_marker,
    )
    print(f"QEMU_OUTCOME {outcome.value}")
    if hotplug_error is not None or remove_error is not None:
        return SCRIPT_EXIT_CODES[QemuOutcome.RESET.value]
    if args.expect_outcome and outcome.value != args.expect_outcome:
        print(
            f"expected QEMU outcome {args.expect_outcome}, got {outcome.value}",
            file=sys.stderr,
        )
        return SCRIPT_EXIT_CODES[outcome.value]
    return SCRIPT_EXIT_CODES[outcome.value]


if __name__ == "__main__":
    raise SystemExit(main())
