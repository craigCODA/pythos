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


def write_text_if_changed(path: Path, text: str) -> None:
    if path.exists() and path.read_text(encoding="utf-8") == text:
        return
    path.write_text(text, encoding="utf-8")


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
    write_text_if_changed(
        pythos_dir / "BOOT.CFG",
        "serial=true\nlog_level=trace\npanic=halt\nruntime_bundle=/PYTHOS/INIT.PAK\n",
    )
    write_text_if_changed(pythos_dir / "INIT.PAK", "PYTHOS_INIT_PAK_V0\n")
    (pythos_dir / "FONT.PSF").touch()

    print(f"ESP_READY {ESP}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
