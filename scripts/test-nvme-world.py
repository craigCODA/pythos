#!/usr/bin/env python
"""Prove that the native world commits and recovers on one marked NVMe partition."""

from __future__ import annotations

import hashlib
import importlib.util
import os
from pathlib import Path
import socket
import struct
from uuid import UUID, uuid4
from zlib import crc32


ROOT = Path(__file__).resolve().parents[1]
PREFIX = "PYTHOS:CORE:NVME_WORLD:"
SECTOR = 512
BLOCKS = 32768
FIRST = 8192
LAST = 24575
STRIDE = 8
MARKER = b"PYWNVM01"
BASIC_DATA = UUID("ebd0a0a2-b9e5-4433-87c0-68b6b72699c7")
WORLD_ID = UUID("71dff308-92c4-4e88-b4a9-4b9400c87d66")
OTHER_ID = UUID("feb61f1c-0c7b-4ba1-b360-82cb10ae2488")
DISK_ID = UUID("a2920672-175b-4833-92c6-2d41a1464f77")
HIDDEN = 1 << 62


def load_world_harness():
    if "PYTHOS_QMP_PORT" not in os.environ:
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            os.environ["PYTHOS_QMP_PORT"] = str(listener.getsockname()[1])
    spec = importlib.util.spec_from_file_location("native_world", ROOT / "scripts/test-world-session.py")
    world = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(world)
    return world


def entry(identity, first, last, attributes, name):
    return struct.pack("<16s16sQQQ72s", BASIC_DATA.bytes_le, identity.bytes_le, first, last,
                       attributes, name.encode("utf-16le"))


def gpt_header(current, backup, entries_lba, table_crc, disk_id):
    header = bytearray(struct.pack(
        "<8sIIIIQQQQ16sQIII", b"EFI PART", 0x10000, 92, 0, 0,
        current, backup, 34, BLOCKS - 34, disk_id.bytes_le, entries_lba, 128, 128, table_crc,
    ))
    struct.pack_into("<I", header, 16, crc32(header))
    return header.ljust(SECTOR, b"\0")


def create_disk(path, *, identity=WORLD_ID, adjacent_last=FIRST - 1, last=LAST, attributes=HIDDEN,
                marker=MARKER, corrupt_backup=False, disk_id=DISK_ID,
                protective_blocks=BLOCKS - 1, hybrid=False):
    adjacent = entry(OTHER_ID, 2048, adjacent_last, 0, "Other volume")
    selected = entry(identity, FIRST, last, attributes, "PythOS world")
    table = (adjacent + selected).ljust(128 * 128, b"\0")
    table_crc = crc32(table)
    mbr = bytearray(SECTOR)
    struct.pack_into("<B3sB3sII", mbr, 446, 0, b"\0\x02\0", 0xEE, b"\xff" * 3, 1, protective_blocks)
    if hybrid:
        struct.pack_into("<B3sB3sII", mbr, 462, 0, bytes(3), 0x07, bytes(3), 2048, FIRST - 2048)
    mbr[-2:] = b"\x55\xaa"
    backup_header = gpt_header(BLOCKS - 1, 1, BLOCKS - 33, table_crc, disk_id)
    if corrupt_backup:
        backup_header[16] ^= 1
    with path.open("xb") as image:
        image.truncate(BLOCKS * SECTOR)
        image.write(mbr)
        image.write(gpt_header(1, BLOCKS - 1, 2, table_crc, disk_id))
        image.write(table)
        image.seek((FIRST - 1) * SECTOR)
        image.write(b"\xa5" * SECTOR)
        image.write(marker.ljust(SECTOR, b"\0"))
        image.seek((FIRST + 5 * STRIDE) * SECTOR)
        image.write(b"\x5a" * SECTOR)
        image.seek((BLOCKS - 33) * SECTOR)
        image.write(table)
        image.write(backup_header)


def arm(disk, revision):
    with disk.open("r+b") as image:
        image.seek(FIRST * SECTOR)
        assert image.read(8) == MARKER
        image.write(struct.pack("<Q", revision))


def protected_disk_hash(disk):
    data = disk.read_bytes()
    # Only the four 4 KiB records after the header are writable.
    protected = data[:8200 * SECTOR] + data[8232 * SECTOR:]
    return hashlib.sha256(protected).digest()


def field(name, value):
    return f"{PREFIX}{name}=0x{value:016X}"


def assert_writes(guest, addresses):
    lines = guest.tail.lines
    actual = [line for line in lines if line.startswith((PREFIX + "WRITE_LBA=", PREFIX + "FLUSHED_LBA="))]
    expected = [field(name, lba) for lba in addresses for name in ("WRITE_LBA", "FLUSHED_LBA")]
    assert actual == expected, guest.serial
    if addresses:
        assert lines.index(field("PARTITION_FIRST_LBA", FIRST)) < lines.index(actual[0]), guest.serial
        assert lines.index(field("PARTITION_LAST_LBA", LAST)) < lines.index(actual[0]), guest.serial


def verify_recovery(world, esp, disk, scratch):
    protected_before = protected_disk_hash(disk)
    state = world.state
    with world.guest(esp, disk, scratch, "first", nvme=True) as first:
        first.wait(state("READY", 0, 0))
        first.keys("w", "1", "d", "3", "c", "t", "b")
        first.wait(state("FRAME", 3, 2, x=1000, z=-1000, selected=1, target=2))
        first.stop()
        assert_writes(first, [8224, 8216, 8224, 8208, 8200, 8208, 8224, 8216, 8224])
    assert protected_disk_hash(disk) == protected_before

    objects = {1: (1, 0, -4000, 0, 2), 2: (3, 1000, -4000, 0, 0)}
    expected = (3, 0, objects, (5, 6, 1, 2))
    assert world.disk_state(disk, first_lba=FIRST, stride=STRIDE) == expected

    arm(disk, 4)
    protected_before = protected_disk_hash(disk)
    barrier = "BEFORE_COMMIT revision:4"
    with world.guest(esp, disk, scratch, "interrupted", kill=barrier, nvme=True) as interrupted:
        interrupted.wait(state("RESTORED", 3, 2))
        interrupted.keys("c", "f")
        interrupted.wait(barrier)
        interrupted.finish(21, "reset")
        assert_writes(interrupted, [8208, 8200])
        assert interrupted.tail.lines.index(field("FLUSHED_LBA", 8200)) < interrupted.tail.lines.index(world.PREFIX + barrier)
        assert not any(line.startswith(world.PREFIX + "EDIT revision:4") for line in interrupted.tail.lines)
    assert protected_disk_hash(disk) == protected_before
    assert world.disk_state(disk, first_lba=FIRST, stride=STRIDE) == expected
    with disk.open("rb") as image:
        image.seek(8200 * SECTOR)
        assert image.read(16) == b"PYWSTA01" + struct.pack("<Q", 4)
        image.seek(8208 * SECTOR)
        assert image.read(SECTOR) == bytes(SECTOR)

    arm(disk, 0)
    protected_before = protected_disk_hash(disk)
    with world.guest(esp, disk, scratch, "recovered", nvme=True) as recovered:
        recovered.wait(state("RESTORED", 3, 2))
        recovered.wait(state("READY", 3, 2))
        recovered.keys("c", "f")
        recovered.wait(state("FRAME", 4, 2, selected=1, target=2, focus=2))
        recovered.keys("0")
        recovered.finish(0, "success")
        assert_writes(recovered, [8208, 8200, 8208])
        lines = recovered.tail.lines
        edit = next(index for index, line in enumerate(lines) if line.startswith(world.PREFIX + "EDIT revision:4"))
        last_flush = max(index for index, line in enumerate(lines) if line == field("FLUSHED_LBA", 8208))
        assert last_flush < edit < lines.index(PREFIX + "QUIESCED") < lines.index(world.PREFIX + "EXIT")
    assert world.disk_state(disk, first_lba=FIRST, stride=STRIDE) == (4, 2, objects, (6, 1, 1, 2))
    assert protected_disk_hash(disk) == protected_before
    print("NVMe committed edit, interrupted commit, and restart recovery passed", flush=True)


def verify_rejections(world, esp, scratch):
    cases = [
        ("wrong-partition", {"identity": UUID(int=WORLD_ID.int + 1)}, "GPT_WORLD_ABSENT", {}),
        ("overlapping-partitions", {"adjacent_last": FIRST}, "GPT_OVERLAP", {}),
        ("short-partition", {"last": FIRST + STRIDE * 4 - 1}, "WORLD_DEVICE", {}),
        ("read-only-partition", {"attributes": HIDDEN | (1 << 60)}, "GPT_WORLD_ATTRIBUTES", {}),
        # Keep OVMF from repairing the backup before PythCore can validate it.
        ("corrupt-backup", {"corrupt_backup": True}, "GPT_HEADER",
         {"nvme_read_only": True, "nvme_serial": "PYTHOS-NVME-WORLD"}),
        ("wrong-marker", {"marker": b"PYWDSK01"}, "WORLD_IMAGE", {}),
        ("wrong-controller", {}, "CONTROLLER_IDENTITY", {"nvme_serial": "UNRELATED-TEST-DISK"}),
    ]
    for label, options, error, guest_options in cases:
        disk = scratch / f"{label}.img"
        create_disk(disk, **options)
        before = hashlib.sha256(disk.read_bytes()).digest()
        with world.guest(esp, disk, scratch, label, nvme=True, **guest_options) as rejected:
            rejected.wait("ERROR:" + error)
            rejected.finish(20, "panic")
            assert_writes(rejected, [])
            assert PREFIX + "QUIESCED" in rejected.tail.lines
        assert hashlib.sha256(disk.read_bytes()).digest() == before, label
        print(f"{label} rejected with disk bytes preserved", flush=True)


def main():
    world = load_world_harness()
    scratch = world.TARGET / f"nvme-proof-{uuid4().hex[:8]}"
    scratch.mkdir(parents=True)
    print(f"NVMe world evidence in {scratch}", flush=True)
    esp = world.build(scratch, nvme=True)
    disk = scratch / "world-nvme.img"
    create_disk(disk)
    verify_recovery(world, esp, disk, scratch)
    verify_rejections(world, esp, scratch)
    print("PYTHOS_NVME_WORLD_OK", flush=True)


if __name__ == "__main__":
    main()
