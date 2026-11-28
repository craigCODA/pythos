#!/usr/bin/env python
"""Run the ADR 0110 vault continuation proof on disposable QEMU images."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import struct
import subprocess
import sys
from pathlib import Path
from uuid import uuid4


ROOT = Path(__file__).resolve().parents[1]
SECTOR = 512
TASK = 7
SOURCE = 13
PREFIX = "PYTHOS:VAULT:"
TERMINAL = PREFIX + "BOOT_DONE"
KILL_COMMITTED = PREFIX + "KILL_AFTER_COMMIT"
KILL_CANDIDATE = PREFIX + "KILL_BEFORE_COMMIT"
USER_RETURN = "PYTHOS:CORE:USER_MODE:RETURN"
VIEW = "PYTHOS:VAULT_VIEW:"


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def program_bytes(entry, limit, initial, goal, links):
    header = struct.pack("<8sHBBB3xQQ", b"PYVPRG01", 1, limit, entry, len(links), initial, goal)
    return (header + b"".join(bytes(link) for link in links)).ljust(64, b"\0")


def compile_counter(source):
    """Translate the operation 55/59 subset used by Raw Vault's counter."""
    if source is None:
        return program_bytes(0, 2, 0, 11, [(1, 0, 0, 0)]), 1
    document = json.loads(source.read_text(encoding="utf-8"))
    args = document["input"]
    if document["operation"] != 55 or set(args) != {"state", "entry", "max_steps", "event", "links", "c"}:
        raise ValueError("Expected the Raw Vault executable-links counter format.")
    goal, state, event = args["c"], args["state"], args["event"]
    if (set(goal) != {"path", "value"} or goal["path"] != "state.x"
            or set(state) != {"x"} or set(event) != {"kind", "step"} or event["kind"] != "task"):
        raise ValueError("The proof accepts a scalar counter and an equality goal.")
    names = sorted(args["links"])
    budget = args["max_steps"]
    values = (state["x"], goal["value"], event["step"])
    if (not 1 <= len(names) <= 8 or type(budget) is not int or not 1 <= budget <= 8
            or any(type(value) is not int or not 0 <= value < 2**64 for value in values)):
        raise ValueError("The counter exceeds the proof's typed limits.")
    links = []
    for name in names:
        definition = args["links"][name]
        target = definition["next"]
        expected = {
            "operation": 59,
            "input": {"state": {"$": "state"}, "values": {"step": {"$": "event.step"}}},
            "write": {"x": {"$": "result.value"}},
            "next": target,
        }
        if definition != expected:
            raise ValueError(f"Unsupported operation at {name!r}.")
        links.append((1, 0, names.index(target), 0))
    return program_bytes(names.index(args["entry"]), budget, state["x"], goal["value"], links), event["step"]


def build_artifacts(scratch, feature="vault-pulse-probe", *,
                    worker_package="pythos-user-vault-pulse-probe", worker_features=(),
                    worker_name=b"vault-pulse.elf", principal=0x50595650554C0001, build_dir=None,
                    core_environment=None, release=False, named_graph=None):
    build = build_dir if build_dir is not None else ROOT / "target" / "vault-pulse" / "build"
    core = ["cargo", "build", "-p", "pythos-core", "--no-default-features", "--features", feature,
            "--target", "x86_64-unknown-none", "--target-dir", str(build / "core")]
    boot = ["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi",
            "--target-dir", str(build / "boot")]
    user = ["cargo", "build", "-p", worker_package, "--target", "x86_64-unknown-none",
            "--target-dir", str(build / "user")]
    if worker_features:
        user.extend(["--features", ",".join(worker_features)])
    profile = "release" if release else "debug"
    if release:
        core.append("--release")
        user.append("--release")
    user_environment = os.environ.copy()
    linker = ROOT / "user/probes/session-input/linker.ld"
    user_environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(
        ("-C", "relocation-model=static", "-C", f"link-arg=-T{linker}", "-C", "link-arg=--no-pie"))
    build_log = scratch / "build.log"
    with build_log.open("w", encoding="utf-8") as log:
        for command, environment in ((core, core_environment), (boot, None), (user, user_environment)):
            print("+ " + " ".join(command), file=log, flush=True)
            result = subprocess.run(command, cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT)
            if result.returncode:
                raise AssertionError(f"Build failed. See {build_log}")

    module = load_script("build-image")
    worker = (build / "user/x86_64-unknown-none" / profile / worker_package).read_bytes()
    named = module.build_named_user_program(worker_name, principal, worker)
    records = [(module.INIT_BUNDLE_NAMED_USER_ELF_TYPE, named)]
    if named_graph is not None:
        graph_name, graph_bytes = named_graph
        records.append((module.INIT_BUNDLE_PYTH_GRAPH_TYPE,
                        module.build_named_pyth_graph(graph_name, principal, graph_bytes)))
    package = module.build_init_pak(module.build_init_bundle(records))
    esp = scratch / "esp"
    files = {
        "EFI/BOOT/BOOTX64.EFI": (build / "boot/x86_64-unknown-uefi/debug" / module.BOOT_EFI.name).read_bytes(),
        "PYTHOS/PYTHCORE.ELF": (build / "core/x86_64-unknown-none" / profile / "pythcore").read_bytes(),
        "PYTHOS/BOOT.CFG": module.BOOT_CFG,
        "PYTHOS/INIT.PAK": package,
        "PYTHOS/FONT.PSF": module.FONT_PSF,
    }
    for relative, payload in files.items():
        path = esp / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(payload)
    return esp


def event_bytes(event_id, kind, value):
    return struct.pack("<IB3xQQ", SOURCE, kind, event_id, value)


def write_sector(disk, address, data):
    assert len(data) <= SECTOR
    with disk.open("r+b") as image:
        image.seek(address * SECTOR)
        image.write(data.ljust(SECTOR, b"\0"))


def read_sectors(disk, address, count=1):
    with disk.open("rb") as image:
        image.seek(address * SECTOR)
        return image.read(SECTOR * count)


def deliver(disk, event=b"", program=b""):
    mailbox = struct.pack("<8sQ", b"PYVMSG01", TASK) + event + program if event else b""
    write_sector(disk, 1, mailbox)


def set_interrupt(disk, mode):
    write_sector(disk, 0, b"PYVDSK01" + bytes([mode]))


def create_disk(path, mode, event, program):
    with path.open("xb") as disk:
        disk.truncate(16 * 1024 * 1024)
    set_interrupt(path, mode)
    deliver(path, event, program)
    return path


def boot(media, disk, phase, expected, kill=None, screen=None):
    log = disk.parent / f"{phase}.log"
    command = [sys.executable, "scripts/run-qemu.py", "--esp" if media.is_dir() else "--iso", str(media),
               "--storage-image", str(disk),
               "--serial-log", str(log), "--timeout", "45", "--no-audio-device", "--success-marker", TERMINAL]
    command += ["--kill-after-marker", kill] if kill else ["--expect-outcome", "success"]
    if screen is not None:
        command += ["--screendump", str(screen)]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, errors="replace", timeout=75)
    output = result.stdout + result.stderr
    (log.parent / f"{phase}-runner.log").write_text(output, encoding="utf-8")
    expected_code = 21 if kill else 0
    assert result.returncode == expected_code, f"{phase} returned {result.returncode}. See {log}"
    outcome = "reset" if kill else "success"
    serial = output.splitlines()
    assert serial.count(f"QEMU_OUTCOME {outcome}") == 1, phase
    assert "PYTHOS:PANIC" not in output, phase
    assert "PYTHOS:VAULT_SCREEN:RENDER_FAILED" not in output, phase
    assert VIEW + "RENDER_FAILED" not in output, phase
    assert serial.index("PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK") < serial.index(PREFIX + "BOOT"), phase
    lines = [line for line in serial if line.startswith(PREFIX) or line == USER_RETURN]
    transcript = [PREFIX + "BOOT"]
    for line in expected:
        if line.startswith(("PULSE ", "REJECTED:ARITHMETIC ", "BLOCKED:ARITHMETIC")) or PREFIX + line == KILL_CANDIDATE:
            transcript.append(USER_RETURN)
        transcript.append(PREFIX + line)
    assert lines == transcript, (phase, lines)
    print(f"{phase} passed")
    return serial


def verify_missing_disk(media, scratch, boot_view=False):
    log = scratch / "no-virtio.log"
    screen = scratch / "no-virtio.ppm"
    command = [
        sys.executable, "scripts/run-qemu.py", "--esp" if media.is_dir() else "--iso", str(media),
        "--serial-log", str(log),
        "--no-virtio-blk", "--no-audio-device", "--expect-outcome", "panic",
        "--screendump", str(screen), "--timeout", "30",
    ]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, errors="replace", timeout=60)
    output = result.stdout + result.stderr
    (scratch / "no-virtio-runner.log").write_text(output, encoding="utf-8")
    serial = output.splitlines()
    assert result.returncode == 20 and serial.count("QEMU_OUTCOME panic") == 1, log
    assert "PYTHOS:VAULT_SCREEN:RENDER_FAILED" not in output, log
    assert [line for line in serial if line.startswith(PREFIX)] == [
        PREFIX + "BOOT", PREFIX + "ERROR:VIRTIO_BLOCK",
    ], log
    assert serial.index("PYTHOS:CORE:IDT_READY") < serial.index(PREFIX + "ERROR:VIRTIO_BLOCK")
    assert screen.is_file(), "The missing-disk path exited before a diagnostic screen was visible."

    width, height, pixels = load_script("test-evidence-terminal").parse_ppm(screen.read_bytes())
    assert width >= 480 and height >= 124
    if boot_view:
        expect_view(serial, ["NODE CORE", "NODE WORKER", "NODE MEMORY", "NODE STORAGE", "BOOT_LINKS",
                             "STOP VIRTIO_BLOCK"])
        graph_pixels = pixels[height // 5 * width * 3:height * 4 // 5 * width * 3]
        assert graph_pixels.count(bytes((255, 180, 100))) > 80, "The unavailable storage node was not drawn."
        print("missing-disk graph passed")
        return
    assert pixels[:3] == bytes((4, 12, 10)), "Expected the boot diagnostic background."
    for top, rows, color in ((32, 24, (80, 255, 150)), (80, 16, (230, 245, 235)), (108, 16, (230, 245, 235))):
        matching = sum(
            pixels[offset:offset + 3] == bytes(color)
            for y in range(top, top + rows)
            for offset in range((y * width + 32) * 3, (y * width + 480) * 3, 3)
        )
        assert matching > 80, f"The diagnostic text row at {top} is missing."
    print("missing-disk screen passed")


def checkpoint(disk):
    slots = []
    for slot in (3, 5):
        body, marker = read_sectors(disk, slot), read_sectors(disk, slot + 1)
        if marker[:8] != b"PYVCMT01" or marker[16:48] != hashlib.sha256(body).digest():
            continue
        generation, task, x, cursor, status, count = struct.unpack_from("<QQQBBB", body, 8)
        assert body[:8] == b"PYVCKP01" and marker[8:16] == body[8:16]
        assert task == TASK and count <= 4
        receipts = []
        for offset in range(96, 96 + count * 48, 48):
            event = body[offset:offset + 24]
            receipts.append((event, *struct.unpack_from("<QQBB", body, offset + 24)))
        slots.append((generation, x, cursor, status, body[72:96], receipts))
    return max(slots)


def pulse(generation, x, cursor=0, status="active", steps=2):
    return f"PULSE generation:{generation} x:{x} cursor:{cursor} status:{status} steps:{steps}"


def restored(generation, x, cursor=0):
    return f"RESTORED generation:{generation} x:{x} cursor:{cursor}"


def verify_counter(esp, scratch, program, step):
    event = event_bytes(1, 1, step)
    first_pulses = ["ADMITTED", pulse(2, 2), pulse(3, 4)]
    last_pulses = [pulse(4, 6), pulse(5, 8), pulse(6, 10), pulse(7, 11, status="finished", steps=1),
                   "FINISHED", "BOOT_DONE"]
    for mode, barrier, label in ((1, KILL_COMMITTED, "committed"), (2, KILL_CANDIDATE, "torn")):
        disk = create_disk(scratch / f"{label}.img", mode, event, program)
        boot(esp, disk, f"kill-{label}", [*first_pulses, barrier.removeprefix(PREFIX)], kill=barrier)
        assert checkpoint(disk) == (3, 4, 0, 1, event, [])
        assert read_sectors(disk, 2)[:64] == program
        if mode == 2:
            body, marker = read_sectors(disk, 5), read_sectors(disk, 6)
            assert struct.unpack_from("<QQQ", body, 8) == (4, TASK, 6)
            assert marker == bytes(SECTOR)
        deliver(disk)
        intake = []
        if mode == 2:
            write_sector(disk, 1, b"invalid mailbox")
            intake = ["REJECTED:MAILBOX"]
        set_interrupt(disk, 0)
        boot(esp, disk, f"resume-{label}", [restored(3, 4), *intake, *last_pulses])
        assert checkpoint(disk) == (7, 11, 0, 3, bytes(24), [(event, 7, 11, 0, 3)])

    stable = read_sectors(disk, 2, 5)
    deliver(disk, event)
    boot(esp, disk, "duplicate", [restored(7, 11), "DUPLICATE event:1 generation:7 x:11 cursor:0 status:finished",
                                  "FINISHED", "BOOT_DONE"])
    assert read_sectors(disk, 2, 5) == stable
    deliver(disk, event_bytes(1, 1, step + 1))
    boot(esp, disk, "conflict", [restored(7, 11), "CONFLICT event:1", "FINISHED", "BOOT_DONE"])
    assert read_sectors(disk, 2, 5) == stable


def verify_wait(esp, scratch):
    program = program_bytes(0, 2, 0, 1, [(2, 1, 1, 3), (3, 2, 2, 0), (1, 0, 2, 0), (3, 3, 3, 0)])
    first, second = event_bytes(1, 1, 1), event_bytes(3, 2, 1)
    disk = create_disk(scratch / "wait.img", 0, first, program)
    wait = ["WAITING kind:2", "BOOT_DONE"]
    waiting = boot(esp, disk, "wait", ["ADMITTED", pulse(2, 0, 1, "waiting", 1), *wait])
    receipt = (first, 2, 0, 1, 2)
    assert checkpoint(disk) == (2, 0, 1, 2, bytes(24), [receipt])
    stable = read_sectors(disk, 2, 5)
    deliver(disk, event_bytes(2, 3, 1))
    boot(esp, disk, "unmatched", [restored(2, 0, 1), "UNMATCHED event:2", *wait])
    assert read_sectors(disk, 2, 5) == stable
    deliver(disk, event_bytes(2, 2, 2))
    boot(esp, disk, "invalid-arithmetic", [restored(2, 0, 1), "REJECTED:ARITHMETIC event:2", *wait])
    assert read_sectors(disk, 2, 5) == stable
    deliver(disk, second)
    woken = boot(esp, disk, "wake", [restored(2, 0, 1), pulse(3, 1, 2, "finished"), "ACCEPTED event:3",
                                   "FINISHED", "BOOT_DONE"])
    assert checkpoint(disk) == (3, 1, 2, 3, bytes(24), [receipt, (second, 3, 1, 2, 3)])
    stable = read_sectors(disk, 2, 5)
    deliver(disk, first)
    boot(esp, disk, "replay-wait", [restored(3, 1, 2), "DUPLICATE event:1 generation:2 x:0 cursor:1 status:waiting",
                                    "FINISHED", "BOOT_DONE"])
    assert read_sectors(disk, 2, 5) == stable
    return waiting, woken


def verify_queued_wake(esp, scratch):
    program = program_bytes(0, 1, 0, 3, [(1, 0, 1, 0), (1, 0, 2, 0), (3, 2, 3, 0), (1, 0, 3, 0)])
    initial, queued = event_bytes(1, 1, 1), event_bytes(2, 2, 1)
    disk = create_disk(scratch / "queued-wake.img", 1, initial, program)
    boot(esp, disk, "before-queued-wake",
         ["ADMITTED", pulse(2, 1, 1, steps=1), pulse(3, 2, 2, steps=1), "KILL_AFTER_COMMIT"],
         kill=KILL_COMMITTED)
    deliver(disk, queued)
    set_interrupt(disk, 0)
    boot(esp, disk, "queued-wake", [restored(3, 2, 2), pulse(4, 2, 2, "waiting", 0),
                                    pulse(5, 2, 3, steps=1), "ACCEPTED event:2",
                                    pulse(6, 3, 3, "finished", 1), "FINISHED", "BOOT_DONE"])
    assert checkpoint(disk) == (6, 3, 3, 3, bytes(24), [(initial, 4, 2, 2, 2), (queued, 6, 3, 3, 3)])


def verify_unavailable_revision(esp, scratch, program, step):
    event = event_bytes(1, 1, step)
    disk = create_disk(scratch / "revision.img", 1, event, program)
    boot(esp, disk, "revision-start", ["ADMITTED", pulse(2, 2), pulse(3, 4), "KILL_AFTER_COMMIT"],
         kill=KILL_COMMITTED)
    saved = read_sectors(disk, 3, 4)
    deliver(disk)
    set_interrupt(disk, 0)
    different = program_bytes(0, 2, 0, 12, [(1, 0, 0, 0)])
    for label, content in (("missing", b""), ("mismatched", different)):
        write_sector(disk, 2, content)
        boot(esp, disk, f"revision-{label}", [restored(3, 4), "BLOCKED:PROGRAM_REVISION", "BOOT_DONE"])
        assert read_sectors(disk, 3, 4) == saved
    write_sector(disk, 2, program)
    boot(esp, disk, "revision-restored", [restored(3, 4), pulse(4, 6), pulse(5, 8), pulse(6, 10),
                                         pulse(7, 11, status="finished", steps=1), "FINISHED", "BOOT_DONE"])
    assert checkpoint(disk) == (7, 11, 0, 3, bytes(24), [(event, 7, 11, 0, 3)])


def expect_view(serial, expected):
    actual = [line.removeprefix(VIEW) for line in serial if line.startswith(VIEW)]
    assert actual == expected, actual


def verify_boot_view(iso, scratch):
    verify_missing_disk(iso, scratch, boot_view=True)
    program = program_bytes(0, 2, 0, 9, [(2, 1, 1, 4), (1, 0, 2, 0), (1, 0, 3, 0),
                                          (1, 0, 1, 0), (3, 2, 1, 0)])
    event = event_bytes(1, 1, 1)
    disk = create_disk(scratch / "view.img", 1, event, program)
    graph = ["NODE CORE", "NODE WORKER", "NODE MEMORY", "NODE VAULT", "BOOT_LINKS",
             "ADDRESS 0", "ADDRESS 1", "ADDRESS 2", "ADDRESS 3", "ADDRESS 4",
             "LINK 0 1", "LINK 0 4", "LINK 1 2", "LINK 2 3", "LINK 3 1", "LINK 4 1"]
    serial = boot(iso, disk, "view-interrupted",
                  ["ADMITTED", pulse(2, 1, 2), pulse(3, 3, 1), "KILL_AFTER_COMMIT"], kill=KILL_COMMITTED)
    expect_view(serial, [*graph, "STATE admitted generation:1 x:0 cursor:0 status:active",
                         "STATE pulse generation:2 x:1 cursor:2 status:active",
                         "STATE pulse generation:3 x:3 cursor:1 status:active"])
    assert checkpoint(disk) == (3, 3, 1, 1, event, [])

    deliver(disk)
    set_interrupt(disk, 0)
    screen = scratch / "view-resumed.ppm"
    serial = boot(iso, disk, "view-resumed",
                  [restored(3, 3, 1), pulse(4, 5, 3), pulse(5, 7, 2), pulse(6, 9, 1, "finished"),
                   "FINISHED", "BOOT_DONE"], screen=screen)
    finished = "generation:6 x:9 cursor:1 status:finished"
    expect_view(serial, [*graph, "STATE restored generation:3 x:3 cursor:1 status:active",
                         "STATE pulse generation:4 x:5 cursor:3 status:active",
                         "STATE pulse generation:5 x:7 cursor:2 status:active", f"STATE pulse {finished}"])
    assert serial.index(PREFIX + pulse(6, 9, 1, "finished")) < serial.index(VIEW + f"STATE pulse {finished}")
    assert checkpoint(disk) == (6, 9, 1, 3, bytes(24), [(event, 6, 9, 1, 3)])
    width, height, pixels = load_script("test-evidence-terminal").parse_ppm(screen.read_bytes())
    graph_pixels = pixels[height // 5 * width * 3:height * 4 // 5 * width * 3]
    assert graph_pixels.count(bytes((109, 255, 192))) > 80, "The current instruction node was not drawn."
    assert graph_pixels.count(bytes((163, 132, 233))) > 20, "The alternate link was not drawn."

    stable = read_sectors(disk, 2, 5)
    deliver(disk, event)
    serial = boot(iso, disk, "view-replayed", [restored(6, 9, 1), "DUPLICATE event:1 " + finished,
                                               "FINISHED", "BOOT_DONE"])
    expect_view(serial, [*graph, f"STATE restored {finished}"])
    assert read_sectors(disk, 2, 5) == stable

    waiting, woken = verify_wait(iso, scratch)
    assert VIEW + "STATE pulse generation:2 x:0 cursor:1 status:waiting" in waiting
    assert VIEW + "STATE restored generation:2 x:0 cursor:1 status:waiting" in woken
    assert VIEW + "STATE pulse generation:3 x:1 cursor:2 status:finished" in woken


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--program", type=Path, help="Raw Vault operation 55 counter to use in the proof")
    mode.add_argument("--boot-view", action="store_true", help="Build and test the native Kai boot-view ISO")
    args = parser.parse_args()
    program, step = compile_counter(args.program)
    if (struct.unpack("<8sHBBB3xQQ", program[:32])[3:] != (0, 1, 0, 11)
            or program[10] != 2 or program[32:36] != bytes([1, 0, 0, 0]) or step != 1):
        parser.error("The QEMU acceptance oracle requires the x=11, step=1 counter with a two-step budget.")
    scratch = ROOT / "target" / "vault-pulse" / f"run-{uuid4().hex[:10]}"
    scratch.mkdir(parents=True)
    print(f"QEMU evidence in {scratch}", flush=True)
    feature = "vault-pulse-view" if args.boot_view else "vault-pulse-probe"
    esp = build_artifacts(scratch, feature)
    if args.boot_view:
        builder = load_script("build-iso")
        files = {path.relative_to(esp).as_posix(): path.read_bytes() for path in esp.rglob("*") if path.is_file()}
        iso = scratch / "pythos-kai-boot-view.iso"
        iso.write_bytes(builder.build_iso_bytes(builder.build_esp_image(files), files))
        verify_boot_view(iso, scratch)
        print(f"BOOT_VIEW_ISO {iso}")
        print("VAULT_BOOT_VIEW_TEST_OK")
        return
    verify_missing_disk(esp, scratch)
    verify_counter(esp, scratch, program, step)
    verify_wait(esp, scratch)
    verify_queued_wake(esp, scratch)
    verify_unavailable_revision(esp, scratch, program, step)
    print("VAULT_PULSE_TEST_OK")


if __name__ == "__main__":
    main()
