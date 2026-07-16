#!/usr/bin/env python
"""Build the first-slice EFI system partition directory."""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ESP = ROOT / "image" / "esp"
BOOT_EFI = ROOT / "target" / "x86_64-unknown-uefi" / "debug" / "bootx64.efi"
PYTHCORE_ELF = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"
BOOT_CFG = b"serial=true\nlog_level=trace\npanic=halt\nruntime_bundle=/PYTHOS/INIT.PAK\n"
INIT_PAK_MAGIC = b"PYTHOS_INIT_PAK_V0"
INIT_PAK_HEADER_LEN = 64
RUNTIME_PAYLOAD_MAGIC = b"PYTHOS_MINRT_V00"
RUNTIME_PAYLOAD_HEADER_LEN = 32
RUNTIME_SOURCE = (
    b"class HelloService(Service):\n"
    b"    async def start(self):\n"
    b"        system.log(\"hello from Python\")\n"
    b"        self.ready()\n"
)


def build_init_pak(payload: bytes = b"") -> bytes:
    checksum = sum(payload) & 0xFFFFFFFF
    total_len = INIT_PAK_HEADER_LEN + len(payload)
    header = bytearray(INIT_PAK_HEADER_LEN)
    header[: len(INIT_PAK_MAGIC)] = INIT_PAK_MAGIC
    header[18:20] = (0).to_bytes(2, "little")
    header[20:22] = (0).to_bytes(2, "little")
    header[22:26] = INIT_PAK_HEADER_LEN.to_bytes(4, "little")
    header[26:34] = total_len.to_bytes(8, "little")
    header[34:42] = len(payload).to_bytes(8, "little")
    header[42:46] = checksum.to_bytes(4, "little")
    return bytes(header) + payload


def build_runtime_payload(source: bytes = RUNTIME_SOURCE) -> bytes:
    checksum = sum(source) & 0xFFFFFFFF
    header = bytearray(RUNTIME_PAYLOAD_HEADER_LEN)
    header[: len(RUNTIME_PAYLOAD_MAGIC)] = RUNTIME_PAYLOAD_MAGIC
    header[16:18] = (0).to_bytes(2, "little")
    header[18:20] = (0).to_bytes(2, "little")
    header[20:24] = RUNTIME_PAYLOAD_HEADER_LEN.to_bytes(4, "little")
    header[24:28] = len(source).to_bytes(4, "little")
    header[28:32] = checksum.to_bytes(4, "little")
    return bytes(header) + source


INIT_PAK = build_init_pak(build_runtime_payload())


def write_binary_if_changed(path: Path, content: bytes) -> None:
    if path.exists() and path.read_bytes() == content:
        return
    path.write_bytes(content)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--loader", type=Path, default=BOOT_EFI)
    parser.add_argument("--kernel", type=Path, default=PYTHCORE_ELF)
    args = parser.parse_args()

    loader = args.loader
    if not loader.exists():
        raise SystemExit(f"missing loader: {loader}")
    kernel = args.kernel
    if not kernel.exists():
        raise SystemExit(f"missing kernel: {kernel}")

    boot_dir = ESP / "EFI" / "BOOT"
    pythos_dir = ESP / "PYTHOS"
    boot_dir.mkdir(parents=True, exist_ok=True)
    pythos_dir.mkdir(parents=True, exist_ok=True)

    shutil.copy2(loader, boot_dir / "BOOTX64.EFI")
    shutil.copy2(kernel, pythos_dir / "PYTHCORE.ELF")
    write_binary_if_changed(pythos_dir / "BOOT.CFG", BOOT_CFG)
    write_binary_if_changed(pythos_dir / "INIT.PAK", INIT_PAK)
    write_binary_if_changed(pythos_dir / "FONT.PSF", b"")

    print(f"ESP_READY {ESP}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
