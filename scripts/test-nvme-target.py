#!/usr/bin/env python
"""Boot an identity-bound NVMe ISO against a disposable QEMU disk."""

from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
from uuid import UUID, uuid4


ROOT = Path(__file__).resolve().parents[1]


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build_iso(nvme, scratch, *, features=()):
    identity = {
        "vendor": 0x1B36,
        "device": 0x0010,
        "subsystem": None,
        "serial_hex": b"PYTHOS-NVME-WORLD   ".hex(),
        "namespace_blocks": nvme.BLOCKS,
        "disk_guid": str(nvme.DISK_ID),
        "partition_guid": str(nvme.WORLD_ID),
        "first_lba": nvme.FIRST,
        "last_lba": nvme.LAST,
    }
    return load_script("build-nvme-target").build_iso(identity, scratch, emulated=True, features=features)


def unchanged_boot(world, nvme, iso, scratch, label, expected_marker, outcome, **fixture):
    disk = scratch / f"{label}.img"
    nvme.create_disk(disk, **fixture)
    before = hashlib.sha256(disk.read_bytes()).digest()
    with world.guest(iso, disk, scratch, label, nvme=True) as guest:
        guest.wait(expected_marker)
        guest.finish(0 if outcome == "success" else 20, outcome)
        nvme.assert_writes(guest, [])
        assert nvme.PREFIX + "QUIESCED" in guest.tail.lines
    assert hashlib.sha256(disk.read_bytes()).digest() == before, label


def verify_marked_world(world, nvme, iso, scratch):
    disk = scratch / "marked.img"
    marker = b"PYWNTG01" + nvme.DISK_ID.bytes_le + nvme.WORLD_ID.bytes_le
    nvme.create_disk(disk, marker=marker, protective_blocks=0xFFFFFFFF)
    protected = nvme.protected_disk_hash(disk)
    with world.guest(iso, disk, scratch, "write", nvme=True) as guest:
        guest.wait(world.state("READY", 0, 0))
        guest.keys("1")
        guest.wait(world.state("FRAME", 1, 1, selected=1))
        guest.stop()
        nvme.assert_writes(guest, [8224, 8216, 8224])
    assert nvme.protected_disk_hash(disk) == protected

    with world.guest(iso, disk, scratch, "restored", nvme=True) as guest:
        guest.wait(world.state("RESTORED", 1, 1))
        guest.keys("0")
        guest.finish(0, "success")
        nvme.assert_writes(guest, [])
    assert world.disk_state(disk, first_lba=nvme.FIRST, stride=nvme.STRIDE)[0] == 1
    assert nvme.protected_disk_hash(disk) == protected


def main():
    nvme = load_script("test-nvme-world")
    world = nvme.load_world_harness()
    scratch = world.TARGET / f"nvme-target-proof-{uuid4().hex[:8]}"
    scratch.mkdir(parents=True)
    print(f"Identity-bound NVMe evidence in {scratch}", flush=True)
    iso = build_iso(nvme, scratch)
    unchanged_boot(world, nvme, iso, scratch, "windows-protective-span",
                   "NVME_TARGET_VERIFIED", "success", marker=b"", protective_blocks=0xFFFFFFFF)
    unchanged_boot(world, nvme, iso, scratch, "wrong-protective-span",
                   "ERROR:GPT_PROTECTIVE_MBR", "panic", protective_blocks=0xFFFFFFFE)
    unchanged_boot(world, nvme, iso, scratch, "hybrid-mbr",
                   "ERROR:GPT_PROTECTIVE_MBR", "panic", protective_blocks=0xFFFFFFFF, hybrid=True)
    unchanged_boot(world, nvme, iso, scratch, "unprovisioned",
                   "NVME_TARGET_VERIFIED", "success", marker=b"")
    unchanged_boot(world, nvme, iso, scratch, "wrong-header-identity",
                   "NVME_TARGET_VERIFIED", "success",
                   marker=b"PYWNTG01" + nvme.DISK_ID.bytes_le + bytes(16))
    unchanged_boot(world, nvme, iso, scratch, "wrong-disk",
                   "ERROR:GPT_TARGET_DISK", "panic", disk_id=UUID(int=nvme.DISK_ID.int + 1))
    unchanged_boot(world, nvme, iso, scratch, "changed-boundary",
                   "ERROR:GPT_TARGET_GEOMETRY", "panic", last=nvme.LAST - 8)
    verify_marked_world(world, nvme, iso, scratch)
    print("PYTHOS_NVME_TARGET_OK", flush=True)


if __name__ == "__main__":
    main()
