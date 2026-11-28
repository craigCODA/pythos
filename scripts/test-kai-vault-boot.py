#!/usr/bin/env python
"""Prove disk-selected Kai startup and recovery from an identity-bound NVMe ISO."""

from __future__ import annotations

import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
from uuid import uuid4


ROOT = Path(__file__).resolve().parents[1]
PREFIX = "PYTHOS:VAULT:"
FIRST_RECORD = 8
PROGRAM_RECORD = FIRST_RECORD + 2
SLOT_RECORDS = (FIRST_RECORD + 3, FIRST_RECORD + 5)
TASK = 41


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(f"{name}.py"))
    script = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(script)
    return script


nvme = load("test-nvme-world")
vault = load("test-vault-pulse")
target = load("build-nvme-target")


def build(scratch, identity, *, emulated=False):
    source = scratch / "target.rs"
    source.write_text(target.target_source(identity, emulated=emulated), encoding="ascii")
    esp = vault.build_artifacts(
        scratch, "kai-vault-boot",
        build_dir=ROOT / "target" / "kai-vault-boot" / "build",
        core_environment=dict(os.environ, PYTHOS_NVME_WORLD_TARGET=str(source)),
        release=True,
    )
    builder = load("build-iso")
    files = {file.relative_to(esp).as_posix(): file.read_bytes() for file in esp.rglob("*") if file.is_file()}
    iso = scratch / "pythos-kai-vault-boot.iso"
    iso.write_bytes(builder.build_iso_bytes(builder.build_esp_image(files), files))
    return iso


def first_lba(record):
    return nvme.FIRST + nvme.STRIDE * record


def set_record(disk, record, data):
    assert len(data) <= nvme.SECTOR
    vault.write_sector(disk, first_lba(record), data)


def get_record(disk, record):
    return vault.read_sectors(disk, first_lba(record))


def root_bytes(task, program, event, interruption=0):
    return struct.pack("<8sQ32s24sB", b"PYKAIRT1", task, hashlib.sha256(program).digest(), event, interruption)


def make_disk(path, program, event, *, task=TASK, interruption=0, root=True):
    marker = b"PYWNTG01" + nvme.DISK_ID.bytes_le + nvme.WORLD_ID.bytes_le
    nvme.create_disk(path, marker=marker)
    # Deliberate object data in the reserved world range and beyond the vault.
    set_record(path, 1, b"retained world checkpoint")
    set_record(path, 15, b"outside the Kai records")
    if root:
        set_record(path, FIRST_RECORD, root_bytes(task, program, event, interruption))
        set_record(path, PROGRAM_RECORD, program)
    return path


def protected_hash(disk):
    image = disk.read_bytes()
    start = first_lba(SLOT_RECORDS[0]) * nvme.SECTOR
    end = first_lba(SLOT_RECORDS[1] + 2) * nvme.SECTOR
    return hashlib.sha256(image[:start] + image[end:]).digest()


def boot(iso, disk, label, expected, *, kill=None, reject=False):
    serial = disk.parent / (label + ".serial.log")
    result = subprocess.run([
        sys.executable, "-B", "scripts/run-qemu.py",
        "--iso", str(iso), "--nvme-world-image", str(disk), "--no-virtio-blk",
        "--serial-log", str(serial), "--timeout", "40", "--no-audio-device",
        "--success-marker", PREFIX + "BOOT_DONE", "--require-debug-exit",
        *(["--kill-after-marker", PREFIX + kill] if kill else []),
    ], cwd=ROOT, capture_output=True, text=True, errors="replace", timeout=55)
    runner = serial.with_suffix(".runner.log")
    runner.write_text(result.stdout + result.stderr, encoding="utf-8")
    outcome, code = ("reset", 21) if kill else ("panic", 20) if reject else ("success", 0)
    assert result.returncode == code and result.stdout.splitlines().count(f"QEMU_OUTCOME {outcome}") == 1, runner
    lines = serial.read_text(encoding="utf-8").splitlines()
    assert lines.index("PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK") < lines.index(PREFIX + "BOOT"), serial
    assert lines.count("PYTHOS:PANIC") == int(reject), serial
    assert lines.count(nvme.PREFIX + "QUIESCED") == int(not kill), serial
    transcript = [PREFIX + "BOOT"]
    for marker in expected:
        if marker.startswith("PULSE ") or marker == "KILL_BEFORE_COMMIT":
            transcript.append(vault.USER_RETURN)
        transcript.append(PREFIX + marker)
    assert [line for line in lines if line.startswith(PREFIX) or line == vault.USER_RETURN] == transcript, serial
    operations = [line for line in lines if line.startswith(
        (nvme.PREFIX + "WRITE_LBA=", nvme.PREFIX + "FLUSHED_LBA="))]
    writes = operations[::2]
    assert operations == [line for write in writes for line in (write, write.replace("WRITE_LBA", "FLUSHED_LBA"))], serial
    permitted = {nvme.field("WRITE_LBA", first_lba(record)) for record in range(SLOT_RECORDS[0], SLOT_RECORDS[1] + 2)}
    assert all(line in permitted for line in writes), serial
    print(f"{label} passed", flush=True)
    return writes


def committed(disk):
    slots = []
    for slot in SLOT_RECORDS:
        body, marker = get_record(disk, slot), get_record(disk, slot + 1)
        if marker == bytes(nvme.SECTOR):
            continue
        generation, task, value, cursor, status, count = struct.unpack_from("<QQQBBB", body, 8)
        assert body[:8] == b"PYVCKP01" and marker[:16] == b"PYVCMT01" + body[8:16]
        assert marker[16:48] == hashlib.sha256(body).digest()
        slots.append((generation, task, value, cursor, status, count, body))
    return max(slots)


def assert_rejected(iso, disk, label, markers):
    digest = hashlib.sha256(disk.read_bytes()).digest()
    assert boot(iso, disk, label, markers, reject=True) == []
    assert hashlib.sha256(disk.read_bytes()).digest() == digest


def proof():
    nvme.load_world_harness()  # Give the runner a private test QMP port.
    scratch = ROOT / "target" / "kai-vault-boot" / f"proof-{uuid4().hex[:8]}"
    scratch.mkdir(parents=True)
    print(f"Kai boot evidence in {scratch}", flush=True)
    identity = {
        "vendor": 0x1B36, "device": 0x0010, "subsystem": None,
        "serial_hex": b"PYTHOS-NVME-WORLD   ".hex(), "namespace_blocks": nvme.BLOCKS,
        "disk_guid": str(nvme.DISK_ID), "partition_guid": str(nvme.WORLD_ID),
        "first_lba": nvme.FIRST, "last_lba": nvme.LAST,
    }
    iso = build(scratch, identity, emulated=True)
    program, step = vault.compile_counter(None)
    event = vault.event_bytes(1, 1, step)
    pulse, restored = vault.pulse, vault.restored
    disk = make_disk(scratch / "kai.img", program, event, interruption=2)
    root = f"KAI_ROOT task:{TASK}"
    protected = protected_hash(disk)
    boot(iso, disk, "cut-power", [root, "ADMITTED", pulse(2, 2), pulse(3, 4), "KILL_BEFORE_COMMIT"],
         kill="KILL_BEFORE_COMMIT")
    assert protected_hash(disk) == protected
    assert committed(disk)[:6] == (3, TASK, 4, 0, 1, 0)
    assert struct.unpack_from("<QQQ", get_record(disk, SLOT_RECORDS[1]), 8) == (4, TASK, 6)
    assert get_record(disk, SLOT_RECORDS[1] + 1) == bytes(nvme.SECTOR)

    set_record(disk, FIRST_RECORD, root_bytes(TASK, program, event))
    protected = protected_hash(disk)
    boot(iso, disk, "resume", [root, restored(3, 4), pulse(4, 6), pulse(5, 8), pulse(6, 10),
                              pulse(7, 11, status="finished", steps=1), "FINISHED", "BOOT_DONE"])
    assert protected_hash(disk) == protected
    assert committed(disk)[:6] == (7, TASK, 11, 0, 3, 1)
    assert committed(disk)[6][96:120] == event

    set_record(disk, FIRST_RECORD + 1, struct.pack("<8sQ", b"PYVMSG01", TASK) + event)
    before = disk.read_bytes()
    assert boot(iso, disk, "duplicate", [root, restored(7, 11),
                "DUPLICATE event:1 generation:7 x:11 cursor:0 status:finished", "FINISHED", "BOOT_DONE"]) == []
    assert disk.read_bytes() == before

    other = vault.program_bytes(0, 1, 3, 9, [(1, 0, 0, 0)])
    other_disk = make_disk(scratch / "other.img", other, vault.event_bytes(2, 1, 2), task=73)
    boot(iso, other_disk, "different-root", ["KAI_ROOT task:73", "ADMITTED", pulse(2, 5, steps=1),
         pulse(3, 7, steps=1), pulse(4, 9, status="finished", steps=1), "FINISHED", "BOOT_DONE"])
    assert committed(other_disk)[:6] == (4, 73, 9, 0, 3, 1)

    missing = make_disk(scratch / "missing.img", program, event, root=False)
    assert_rejected(iso, missing, "missing-root", ["ERROR:KAI_ROOT_REQUIRED"])
    damaged = scratch / "changed-root.img"
    shutil.copyfile(disk, damaged)
    set_record(damaged, PROGRAM_RECORD, other)
    assert_rejected(iso, damaged, "changed-program", ["ERROR:KAI_PROGRAM_REVISION"])
    set_record(damaged, PROGRAM_RECORD, program)
    set_record(damaged, FIRST_RECORD, root_bytes(73, program, event))
    assert_rejected(iso, damaged, "changed-task", ["KAI_ROOT task:73", "ERROR:KAI_CHECKPOINT_ROOT"])
    set_record(damaged, FIRST_RECORD, root_bytes(TASK, program, vault.event_bytes(3, 1, 2)))
    assert_rejected(iso, damaged, "changed-boot-event", [root, "ERROR:KAI_CHECKPOINT_ROOT"])
    print("PYTHOS_KAI_VAULT_BOOT_OK")


if __name__ == "__main__":
    proof()
