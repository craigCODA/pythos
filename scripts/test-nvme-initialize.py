#!/usr/bin/env python
"""Verify first-use initialization of an identity-bound NVMe world in QEMU."""

from __future__ import annotations

from contextlib import redirect_stdout
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch
from uuid import UUID, uuid4


def load_target_harness():
    path = Path(__file__).with_name("test-nvme-target.py")
    spec = importlib.util.spec_from_file_location("nvme_target", path)
    target = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(target)
    return target


def checkpoint_lbas(nvme):
    return [nvme.FIRST + record * nvme.STRIDE for record in range(1, 5)]


def exit_world(guest):
    guest.keys("0")
    guest.finish(0, "success")


def assert_initialized(world, nvme, guest):
    nvme.assert_writes(guest, checkpoint_lbas(nvme) + [nvme.FIRST])
    lines = guest.tail.lines
    initialized = nvme.PREFIX + "INITIALIZED"
    assert lines.count(initialized) == 1, guest.serial
    assert lines.index(nvme.field("FLUSHED_LBA", nvme.FIRST)) < lines.index(initialized), guest.serial
    assert lines.index(initialized) < lines.index(world.PREFIX + world.state("READY", 0, 0)), guest.serial


def verify_initialization(world, nvme, iso, scratch, disk, *, label="initialize"):
    before = disk.read_bytes()
    with world.guest(iso, disk, scratch, label, nvme=True) as guest:
        guest.wait(world.state("READY", 0, 0))
        exit_world(guest)
        assert_initialized(world, nvme, guest)

    start = nvme.FIRST * nvme.SECTOR
    end = start + 5 * nvme.STRIDE * nvme.SECTOR
    marker = b"PYWNTG01" + nvme.DISK_ID.bytes_le + nvme.WORLD_ID.bytes_le
    expected = before[:start] + marker.ljust(end - start, b"\0") + before[end:]
    assert disk.read_bytes() == expected, disk


def verify_persistence(world, nvme, iso, scratch, disk):
    with world.guest(iso, disk, scratch, "edit", nvme=True) as guest:
        guest.wait(world.state("READY", 0, 0))
        guest.keys("1")
        guest.wait(world.state("FRAME", 1, 1, selected=1))
        exit_world(guest)
        body, commit = checkpoint_lbas(nvme)[2:]
        nvme.assert_writes(guest, [commit, body, commit])
        assert nvme.PREFIX + "INITIALIZED" not in guest.tail.lines, guest.serial

    before = disk.read_bytes()
    with world.guest(iso, disk, scratch, "restore", nvme=True) as guest:
        guest.wait(world.state("RESTORED", 1, 1))
        exit_world(guest)
        nvme.assert_writes(guest, [])
        assert nvme.PREFIX + "INITIALIZED" not in guest.tail.lines, guest.serial
    assert disk.read_bytes() == before, disk
    assert world.disk_state(disk, first_lba=nvme.FIRST, stride=nvme.STRIDE)[0] == 1, disk


def verify_rejection(world, nvme, iso, scratch, label, reason, *, nonzero_offset=None,
                     serial=None, **fixture):
    disk = scratch / f"{label}.img"
    nvme.create_disk(disk, marker=b"", **fixture)
    if nonzero_offset is not None:
        with disk.open("r+b") as image:
            image.seek(nvme.FIRST * nvme.SECTOR + nonzero_offset)
            image.write(b"\xa5")
    before = disk.read_bytes()
    with world.guest(iso, disk, scratch, label, nvme=True, nvme_serial=serial) as guest:
        guest.wait("ERROR:" + reason)
        guest.finish(20, "panic")
        nvme.assert_writes(guest, [])
        assert nvme.PREFIX + "QUIESCED" in guest.tail.lines, guest.serial
        assert nvme.PREFIX + "INITIALIZED" not in guest.tail.lines, guest.serial
    assert disk.read_bytes() == before, disk


def verify_retry(target, world, nvme, iso, scratch):
    disk = scratch / "header-write-failure.img"
    nvme.create_disk(disk, marker=b"")
    before = disk.read_bytes()
    serial = scratch / "header-write-failure.serial.log"
    result = scratch / "header-write-failure.runner.log"
    argv = world.runner_command(iso, disk, serial, nvme=True)[2:]
    runner = target.load_script("run-qemu")
    popen = subprocess.Popen

    with result.open("x", encoding="utf-8") as log:
        def launch(command, **kwargs):
            # Fail just the header write, after all four checkpoint flushes.
            index = command.index("-blockdev") + 1
            block = json.loads(command[index])
            assert block["node-name"] == "pythos_nvme_image" and Path(block["file"]["filename"]) == disk
            block["file"] = {
                "driver": "blkdebug", "image": block["file"],
                "inject-error": [{"event": "write_aio", "iotype": "write", "sector": nvme.FIRST}],
            }
            command[index] = json.dumps(block)
            return popen(command, stdout=log, stderr=subprocess.STDOUT, **kwargs)

        with redirect_stdout(log), patch.object(sys, "argv", argv), patch.object(runner.subprocess, "Popen", launch):
            code = runner.main()

    output = result.read_text(encoding="utf-8", errors="replace").splitlines()
    assert code == 20 and output.count("QEMU_OUTCOME panic") == 1, result
    tail = world.SerialTail(serial, world.AcceptanceTimeline())
    tail.poll()
    expected = [nvme.field(name, lba) for lba in checkpoint_lbas(nvme) for name in ("WRITE_LBA", "FLUSHED_LBA")]
    actual = [line for line in tail.lines if line.startswith((nvme.PREFIX + "WRITE_LBA=", nvme.PREFIX + "FLUSHED_LBA="))]
    assert actual == expected, serial
    assert nvme.PREFIX + "QUIESCED" in tail.lines, serial
    assert world.PREFIX + "ERROR:COMMAND_STATUS" in tail.lines, serial
    assert nvme.PREFIX + "INITIALIZED" not in tail.lines, serial
    assert disk.read_bytes() == before, disk

    verify_initialization(world, nvme, iso, scratch, disk, label="retry")


def main():
    target = load_target_harness()
    nvme = target.load_script("test-nvme-world")
    world = nvme.load_world_harness()
    scratch = world.TARGET / f"nvme-initialize-proof-{uuid4().hex[:8]}"
    scratch.mkdir(parents=True)
    print(f"NVMe initialization evidence in {scratch}", flush=True)
    iso = target.build_iso(nvme, scratch, features=("world-session-nvme-initialize",))

    disk = scratch / "blank.img"
    nvme.create_disk(disk, marker=b"", protective_blocks=0xFFFFFFFF)
    verify_initialization(world, nvme, iso, scratch, disk)
    verify_persistence(world, nvme, iso, scratch, disk)
    verify_retry(target, world, nvme, iso, scratch)

    verify_rejection(world, nvme, iso, scratch, "header-padding", "NVME_TARGET_INIT_HEADER_NOT_BLANK",
                     nonzero_offset=nvme.SECTOR + 3)
    verify_rejection(world, nvme, iso, scratch, "checkpoint-padding", "NVME_TARGET_INIT_CHECKPOINT_NOT_BLANK",
                     nonzero_offset=5 * nvme.STRIDE * nvme.SECTOR - 1)
    verify_rejection(world, nvme, iso, scratch, "wrong-controller", "CONTROLLER_IDENTITY",
                     serial="PYTHOS-NVME-OTHER")
    verify_rejection(world, nvme, iso, scratch, "wrong-disk", "GPT_TARGET_DISK",
                     disk_id=UUID(int=nvme.DISK_ID.int + 1))
    verify_rejection(world, nvme, iso, scratch, "wrong-partition", "GPT_WORLD_ABSENT",
                     identity=UUID(int=nvme.WORLD_ID.int + 1))
    verify_rejection(world, nvme, iso, scratch, "changed-boundary", "GPT_TARGET_GEOMETRY",
                     last=nvme.LAST - nvme.STRIDE)
    print("PYTHOS_NVME_INITIALIZE_OK", flush=True)


if __name__ == "__main__":
    main()
