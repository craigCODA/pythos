#!/usr/bin/env python
"""Prove first-boot world initialization, native USB controls and restart recovery."""

from __future__ import annotations

import importlib.util
from pathlib import Path
from uuid import uuid4


ROOT = Path(__file__).resolve().parents[1]
RUNNER_ARGS = ("--xhci", "--usb-keyboard")


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def press(guest, sequence, key, value):
    guest.keys(key)
    guest.wait(f"INPUT sequence:{sequence} kind:1 value:{value} flags:0")


def main():
    nvme = load_script("test-nvme-world")
    world = nvme.load_world_harness()
    target = load_script("test-nvme-target")
    scratch = world.TARGET / f"nvme-usb-world-{uuid4().hex[:8]}"
    scratch.mkdir(parents=True)
    print(f"Native USB world evidence in {scratch}", flush=True)
    iso = target.build_iso(
        nvme, scratch, features=("world-session-nvme-initialize", "world-session-nvme-usb")
    )
    disk = scratch / "world.img"
    nvme.create_disk(disk, marker=b"")
    original = disk.read_bytes()

    state = world.state
    with world.guest(iso, disk, scratch, "first-boot", nvme=True, runner_args=RUNNER_ARGS) as first:
        first.wait("USB_KEYBOARD_READY")
        first.wait(state("READY", 0, 0))
        # More than two wraps of both hardware rings, including key release reports.
        for sequence, (key, value) in enumerate((("w", 23), ("s", 19)) * 18):
            press(first, sequence, key, value)
        actions = (("w", 23), ("1", 0x21), ("d", 4), ("3", 0x23), ("c", 3),
                   ("t", 20), ("b", 2), ("f", 6), ("h", 8))
        for sequence, (key, value) in enumerate(actions, 36):
            press(first, sequence, key, value)
        first.wait(state("FRAME", 5, 2, x=1000, z=-1000, selected=1, target=2, focus=2))
        first.stop()
        assert first.tail.lines.count(nvme.PREFIX + "INITIALIZED") == 1, first.serial
        assert first.tail.lines.count(world.PREFIX + "USB_RING_WRAP") >= 2, first.serial
    current = disk.read_bytes()
    start = nvme.FIRST * nvme.SECTOR
    end = start + 5 * nvme.STRIDE * nvme.SECTOR
    assert current[:start] == original[:start] and current[end:] == original[end:]
    revision, focus, objects, _ = world.disk_state(disk, first_lba=nvme.FIRST, stride=nvme.STRIDE)
    assert revision == 5 and focus == 2 and objects[1][-1] == 2

    with world.guest(iso, disk, scratch, "restart", nvme=True, runner_args=RUNNER_ARGS) as restored:
        restored.wait("USB_KEYBOARD_READY")
        restored.wait(state("RESTORED", 5, 2, x=1000, z=-1000, focus=2))
        restored.keys("w")
        restored.wait(state("FRAME", 5, 2, x=1000, z=-2000, focus=2))
        restored.keys("0")
        restored.finish(0, "success")
        assert nvme.PREFIX + "INITIALIZED" not in restored.tail.lines
        nvme.assert_writes(restored, [])
    assert disk.read_bytes() == current
    print("PYTHOS_NVME_USB_WORLD_OK", flush=True)


if __name__ == "__main__":
    main()
