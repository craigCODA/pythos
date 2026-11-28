#!/usr/bin/env python
"""Build a PythOS world ISO for a locally identified WD SN550."""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
from uuid import UUID


ROOT = Path(__file__).resolve().parents[1]
WD_PCI = (0x15B7, 0x5009, 0x500915B7)


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def target_source(identity, *, emulated=False):
    vendor, device, subsystem = (identity[field] for field in ("vendor", "device", "subsystem"))
    if not emulated and (vendor, device, subsystem) != WD_PCI:
        raise ValueError("expected the verified WD SN550 PCI identity")
    serial = bytes.fromhex(identity["serial_hex"])
    disk_guid = UUID(identity["disk_guid"])
    partition_guid = UUID(identity["partition_guid"])
    blocks = identity["namespace_blocks"]
    first = identity["first_lba"]
    last = identity["last_lba"]
    if len(serial) != 20 or not serial.strip() or any(not 32 <= byte <= 126 for byte in serial):
        raise ValueError("expected the original 20-byte NVMe controller serial")
    if disk_guid.int == 0 or partition_guid.int == 0:
        raise ValueError("expected nonzero GPT identities")
    if first < 34 or first % 8 or last - first + 1 < 5 * 8 or last > blocks - 34 or blocks >= 2**64:
        raise ValueError("the world range must hold five aligned pages within the GPT usable area")
    subsystem_rs = "None" if subsystem is None else f"Some({subsystem:#010x})"
    return f"""Target {{
    vendor: {vendor:#06x},
    device: {device:#06x},
    subsystem: {subsystem_rs},
    serial: {list(serial)},
    namespace_blocks: {blocks:d},
    disk_guid: {list(disk_guid.bytes_le)},
    partition_guid: {list(partition_guid.bytes_le)},
    first_lba: {first:d},
    last_lba: {last:d},
    emulated: {str(emulated).lower()},
}}
"""


def build_iso(identity, directory: Path, *, emulated=False, features=()) -> Path:
    target = directory / "target.rs"
    target.write_text(target_source(identity, emulated=emulated), encoding="ascii")
    world = load_script("test-world-session")
    esp = world.build(directory, nvme_target=target, features=features)
    builder = load_script("build-iso")
    files = {path.relative_to(esp).as_posix(): path.read_bytes() for path in esp.rglob("*") if path.is_file()}
    name = "pythos-nvme-target-test.iso" if emulated else "pythos-wd-sn550-world.iso"
    iso = directory / name
    iso.write_bytes(builder.build_iso_bytes(builder.build_esp_image(files), files))
    return iso


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("identity", type=Path, help="local JSON from the WD controller and GPT inventory")
    parser.add_argument("output_directory", type=Path, help="new directory for the build and ISO")
    parser.add_argument("--boot-world", action="store_true", help="prepare the blank store and enter the editable world")
    args = parser.parse_args()
    identity = json.loads(args.identity.read_text(encoding="utf-8-sig"))
    args.output_directory.mkdir(parents=True)
    features = ("world-session-nvme-initialize", "world-session-nvme-usb") if args.boot_world else ()
    iso = build_iso(identity, args.output_directory, features=features)
    print(iso.resolve())


if __name__ == "__main__":
    main()
