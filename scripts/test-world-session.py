#!/usr/bin/env python
"""Boot the native editable world and verify committed edits across QEMU restarts."""

from __future__ import annotations

import argparse
from contextlib import contextmanager
import hashlib
import importlib.util
import os
from pathlib import Path
import struct
import subprocess
import sys
import time
from uuid import uuid4

import launcher_click
from qemu_probe_support import AcceptanceTimeline, SerialTail


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target" / "kai-world" / "native"
PREFIX = "PYTHOS:WORLD:"
SECTOR = 512
WORLD_LEN = 344


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build(scratch, *, nvme=False, nvme_target=None, features=()):
    feature = "world-session-nvme" if nvme else "world-session-probe"
    build_dir = TARGET / ("nvme-build" if nvme else "build")
    core_environment = None
    if nvme_target is not None:
        feature = "world-session-nvme-target"
        build_dir = scratch / "build"
        core_environment = dict(os.environ, PYTHOS_NVME_WORLD_TARGET=str(nvme_target.resolve()))
    return load_script("test-vault-pulse").build_artifacts(
        scratch, ",".join((feature, *features)), worker_package="pythos-user-world-session",
        worker_features=("native",), worker_name=b"world-session.elf", principal=0x5059574F524C0001,
        build_dir=build_dir, core_environment=core_environment,
    )


def create_disk(path, marker=b"PYWDSK01"):
    with path.open("xb") as image:
        image.truncate(16 * 1024 * 1024)
        image.write(marker.ljust(SECTOR, b"\0"))


def arm(path, revision):
    with path.open("r+b") as image:
        assert image.read(8) == b"PYWDSK01"
        image.write(struct.pack("<Q", revision))


def disk_state(path, *, first_lba=0, stride=1):
    with path.open("rb") as image:
        records = []
        for address in range(5):
            image.seek((first_lba + address * stride) * SECTOR)
            records.append(image.read(SECTOR))
    accepted = []
    for index, address in enumerate((1, 3)):
        body = records[address]
        marker = records[address + 1]
        revision, next_id, focus = struct.unpack_from("<QII", body, 8)
        expected = (b"PYWCMT01" + struct.pack("<Q", revision) + hashlib.sha256(body).digest()).ljust(SECTOR, b"\0")
        if not revision or marker != expected:
            continue
        assert revision % 2 == index and body[:8] == b"PYWSTA01" and not any(body[WORLD_LEN:])
        actor, target, edit, sequence = struct.unpack_from("<IIB3xQ", body, 36)
        objects = {}
        for offset in range(56, WORLD_LEN, 24):
            identity, kind, x, z, heading, binding = struct.unpack_from("<IB3xiiB3xI", body, offset)
            if identity:
                assert identity < next_id and identity not in objects
                objects[identity] = (kind, x, z, heading, binding)
        accepted.append((revision, focus, objects, (edit, sequence, actor, target)))
    return max(accepted, key=lambda entry: entry[0])


def runner_command(esp, image, serial, *, timeout="55", display="none", nvme=False, nvme_read_only=False):
    media = "--iso" if esp.suffix == ".iso" else "--esp"
    if nvme:
        option = "--nvme-read-only-image" if nvme_read_only else "--nvme-world-image"
        storage = ["--no-virtio-blk", option, str(image)]
    else:
        storage = ["--storage-image", str(image)]
    return [
        sys.executable, "-B", "scripts/run-qemu.py", media, str(esp), *storage,
        "--serial-log", str(serial), "--timeout", timeout, "--no-audio-device", "--display", display,
        "--success-marker", PREFIX + "EXIT", "--require-debug-exit",
    ]


def viewport(path):
    magic, size, intensity, pixels = path.read_bytes().split(b"\n", 3)
    width, height = map(int, size.split())
    assert magic == b"P6" and intensity == b"255", path
    assert width >= 600 and height >= 400 and len(pixels) == width * height * 3, path
    return width, pixels[84 * width * 3:(height - 98) * width * 3]


class Guest:
    def __init__(self, process, serial, result, interrupt, timeline):
        self.process = process
        self.serial = serial
        self.result = result
        self.interrupt_file = interrupt
        self.tail = SerialTail(serial, timeline)

    def wait(self, expected):
        marker = PREFIX + expected
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            self.tail.poll()
            if marker in self.tail.lines:
                return
            if self.process.poll() is not None:
                break
            time.sleep(.027)
        raise AssertionError(f"Expected {marker}. See {self.serial} and {self.result}")

    def keys(self, *keys):
        launcher_click.press_qcode_keys(list(keys))

    def frame(self, path):
        load_script("run-qemu").request_screendump(path)
        return viewport(path)

    def stop(self):
        self.interrupt_file.touch()
        self.finish(21, "reset")

    def finish(self, code, outcome):
        actual = self.process.wait(timeout=12)
        output = self.result.read_text(encoding="utf-8", errors="replace")
        assert actual == code and output.splitlines().count("QEMU_OUTCOME " + outcome) == 1, output
        self.tail.poll()
        lines = self.tail.lines
        assert lines.index("PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK") < lines.index(PREFIX + "BOOT"), lines
        assert ("PYTHOS:PANIC" in lines) == (outcome == "panic"), lines


@contextmanager
def guest(esp, image, scratch, label, *, kill=None, nvme=False, nvme_serial=None, nvme_read_only=False,
          runner_args=()):
    serial = scratch / f"{label}.serial.log"
    result = scratch / f"{label}.runner.log"
    interrupt = scratch / f"{label}.stop"
    command = runner_command(esp, image, serial, nvme=nvme, nvme_read_only=nvme_read_only)
    command += ["--interrupt-file", str(interrupt), *runner_args]
    if nvme_serial is not None:
        command += ["--nvme-serial", nvme_serial]
    if kill:
        command += ["--kill-after-marker", PREFIX + kill]
    with result.open("x", encoding="utf-8") as output:
        process = subprocess.Popen(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
        try:
            yield Guest(process, serial, result, interrupt, AcceptanceTimeline())
        finally:
            if process.poll() is None:
                interrupt.touch(exist_ok=True)
                process.wait(timeout=65)


def state(stage, revision, count, x=0, z=0, selected=0, target=0, focus=0):
    return (f"{stage} revision:{revision} objects:{count} x:{x} z:{z} heading:0 "
            f"selected:{selected} target:{target} focus:{focus}")


def proof():
    scratch = TARGET / f"proof-{uuid4().hex[:8]}"
    scratch.mkdir(parents=True)
    print(f"Native world evidence in {scratch}", flush=True)
    esp = build(scratch)
    disk = scratch / "world.img"
    create_disk(disk)

    with guest(esp, disk, scratch, "author") as first:
        first.wait(state("READY", 0, 0))
        width, blank = first.frame(scratch / "blank.ppm")
        sky_column = width // 3 * 3
        samples = [row * width * 3 + sky_column for row in range(0, 96, 8)]
        assert len({blank[offset:offset + 3] for offset in samples}) >= 6, "Expected graded sky"
        first.keys("w")
        first.wait(state("FRAME", 0, 0, z=-1000))
        first.keys("1", "d", "3", "d", "3", "c")
        first.wait(state("FRAME", 3, 3, x=2000, z=-1000, selected=1))
        first.keys("t")
        first.wait(state("FRAME", 3, 3, x=2000, z=-1000, selected=1, target=2))
        first.keys("b", "f")
        first.wait(state("FRAME", 5, 3, x=2000, z=-1000, selected=1, target=2, focus=2))
        first.keys("t", "b")
        first.wait(state("FRAME", 6, 3, x=2000, z=-1000, selected=1, target=3, focus=2))
        first.frame(scratch / "rebound.ppm")
        first.stop()

    expected_objects = {1: (1, 0, -4000, 0, 3), 2: (3, 1000, -4000, 0, 0), 3: (3, 2000, -4000, 0, 0)}
    assert disk_state(disk) == (6, 2, expected_objects, (5, 11, 1, 3))

    with guest(esp, disk, scratch, "resume") as resumed:
        resumed.wait(state("RESTORED", 6, 3, focus=2))
        resumed.wait(state("READY", 6, 3, focus=2))
        resumed.keys("c", "f")
        resumed.wait(state("FRAME", 7, 3, selected=1, target=3, focus=3))
        _, visible = resumed.frame(scratch / "recovered.ppm")
        selected, focused = bytes((246, 194, 120)), bytes((139, 242, 201))
        assert selected not in blank and focused not in blank
        assert selected in visible and focused in visible
        resumed.keys("0")
        resumed.finish(0, "success")
    assert disk_state(disk) == (7, 3, expected_objects, (6, 1, 1, 3))

    barrier = "BEFORE_COMMIT revision:8"
    arm(disk, 8)
    with guest(esp, disk, scratch, "torn", kill=barrier) as torn:
        torn.wait(state("READY", 7, 3, focus=3))
        torn.keys("c", "t", "b")
        torn.wait(barrier)
        torn.finish(21, "reset")
        assert not any(line.startswith(PREFIX + "EDIT revision:8") for line in torn.tail.lines)
    assert disk_state(disk) == (7, 3, expected_objects, (6, 1, 1, 3))
    arm(disk, 0)
    with guest(esp, disk, scratch, "recover-torn") as recovered:
        recovered.wait(state("RESTORED", 7, 3, focus=3))
        recovered.wait(state("READY", 7, 3, focus=3))
        recovered.keys("0")
        recovered.finish(0, "success")

    other = scratch / "counter-marker.img"
    create_disk(other, marker=b"PYVDSK01")
    before = hashlib.sha256(other.read_bytes()).digest()
    with guest(esp, other, scratch, "wrong-image") as rejected:
        rejected.wait("ERROR:WORLD_IMAGE")
        rejected.finish(20, "panic")
    assert hashlib.sha256(other.read_bytes()).digest() == before
    print(f"PYTHOS_NATIVE_WORLD_OK {scratch}")


def play(display):
    scratch = TARGET / "play"
    scratch.mkdir(parents=True, exist_ok=True)
    esp = build(scratch)
    disk = scratch / "world.img"
    if not disk.exists():
        create_disk(disk)
    print(f"Native world saves to {disk}. Press 0 to exit.", flush=True)
    return subprocess.call(
        runner_command(esp, disk, scratch / "play.serial.log", timeout="7200", display=display), cwd=ROOT,
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--play", action="store_true", help="open the editable native world in a QEMU window")
    parser.add_argument("--display", default="sdl")
    args = parser.parse_args()
    if args.play:
        raise SystemExit(play(args.display))
    proof()
