#!/usr/bin/env python
"""Independent live acceptance for ADR 0093's default normal session."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import re
import struct
import subprocess
import sys
import tempfile
import time
import unittest
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, TypeVar


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import launcher_click
from qemu_probe_support import (
    AcceptanceTimeline,
    Com1Observer,
    Com2Collector,
    RunnerCapture,
    SerialTail,
    cleanup_runner_process,
    connect_com2,
    spawn_runner_process,
)


def load_script(name: str, filename: str):
    path = ROOT / "scripts" / filename
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {filename}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


RUNTIME = load_script("normal_session_runtime_support", "test-session-runtime-probe.py")
VIEWING = load_script("normal_session_pixel_support", "test-viewing-input-probe.py")
QEMU = VIEWING.QEMU_RUNNER

TARGET_DIR = ROOT / "target" / "normal-session-acceptance"
FAULT_TARGET_DIR = ROOT / "target" / "normal-session-fault-acceptance"
KERNEL = ROOT / "target/normal-session-kernel/x86_64-unknown-none/debug/pythcore"
NORMAL_ELF = ROOT / "target/normal-session/x86_64-unknown-none/debug/pythos-normal-session"
FAULT_ELF = ROOT / "target/normal-session-fault-test/x86_64-unknown-none/debug/pythos-normal-session"
GRAPH = ROOT / "target/normal-session/pyth-tig/session-manager.tig"
SHELL_PORT = 4594
QEMU_TIMEOUT = 90.0
WAIT_TIMEOUT = 20.0
READY = "PYTHOS:USER:NORMAL_SESSION:READY"
STATUS_PREFIX = "PYTHOS:USER:NORMAL_SESSION:STATUS "
COMMAND_REJECTED = "PYTHOS:USER:NORMAL_SESSION:COMMAND_REJECTED"
CORE_PREFIX = "PYTHOS:CORE:NORMAL_SESSION:"
DRAW_PREFIX = "PYTHOS:CORE:SESSION_VIEWING:DRAWN "
FAULT_CONTEXT_PREFIX = CORE_PREFIX + "FAULT_CONTAINED "
SHELL_READY = "PYTHOS:SHELL:READY"
EXPECTED_ZEROED_SHA256 = RUNTIME.ZEROED_STORAGE_SHA256

STATUS_PATTERN = re.compile(
    r"^PYTHOS:USER:NORMAL_SESSION:STATUS "
    r"e([0-9a-f]{16})c([0-9a-f]{16})r([0-9a-f]{16})"
    r"a([01])x([0-9a-f]{3})y([0-9a-f]{3})$"
)
FAULT_CONTEXT_PATTERN = re.compile(
    r"^PYTHOS:CORE:NORMAL_SESSION:FAULT_CONTAINED "
    r"principal:([0-9A-F]{16}) vector:([0-9]+) rip:([0-9A-F]{16}) "
    r"rsp:([0-9A-F]{16}) cr2:([0-9A-F]{16})$"
)


@dataclass(frozen=True)
class StatusRecord:
    event_count: int
    command_count: int
    revision: int
    active: bool
    x: int
    y: int


@dataclass(frozen=True)
class StorageSnapshot:
    path_identity: object
    retained_identity: object
    size: int
    sha256: str


@dataclass(frozen=True)
class ProfileIdentity:
    kernel_sha256: str
    graph_sha256: str
    user_elf_sha256: str
    fault: bool


@dataclass(frozen=True)
class BootEvidence:
    com1: str
    com2: str
    runner: str
    timeline: tuple[tuple[str, str], ...]
    screenshots: tuple[bytes, ...]
    process_tree_reaped: bool


class StatusOracle:
    """Independent make-event arithmetic for the fixed acceptance sequence."""

    def __init__(self) -> None:
        self.event_count = 0
        self.command_count = 0
        self.revision = 0
        self._activation: list[str] = []
        self.active = False
        self.x = 0
        self.y = 0

    def idle(self) -> None:
        # An unrelated wake changes no retained session value.
        return

    def accept_keys(self, keys: tuple[str, ...]) -> None:
        for key in keys:
            self.event_count += 1
            self.revision += 1
            if not self.active:
                self._activation.append(key)
                self._activation = self._activation[-4:]
                if self._activation == ["spc", "spc", "backspace", "backspace"]:
                    self.active = True
                    self.x, self.y = 0x140, 0x0F0

    def accept_motion(self, dx: int, dy: int) -> None:
        self.event_count += 1
        self.revision += 1
        if self.active:
            self.x = min(0x27F, max(0, self.x + dx))
            self.y = min(0x1DF, max(0, self.y + dy))

    def next_status_line(self) -> str:
        self.command_count += 1
        return (
            f"{STATUS_PREFIX}e{self.event_count:016x}c{self.command_count:016x}"
            f"r{self.revision:016x}a{int(self.active)}x{self.x:03x}y{self.y:03x}"
        )


def parse_status_line(line: str) -> StatusRecord:
    match = STATUS_PATTERN.fullmatch(line)
    if match is None:
        raise ValueError("not one complete exact normal-session status record")
    values = tuple(int(value, 16) for value in match.groups())
    event_count, command_count, revision, active, x, y = values
    if x >= 640 or y >= 480 or (active == 0 and (x != 0 or y != 0)):
        raise ValueError("status coordinates contradict the fixed viewport/state")
    return StatusRecord(event_count, command_count, revision, bool(active), x, y)


def complete_lines(text: str) -> list[str]:
    return [line.rstrip("\r") for line in text.splitlines()]


def assert_complete_transcript(
    transcript: str, statuses: tuple[str, ...], *, require_shell: bool
) -> None:
    lines = complete_lines(transcript)
    if lines.count(READY) != 1:
        raise AssertionError("normal READY must occur exactly once")
    if COMMAND_REJECTED in lines:
        raise AssertionError("normal command was rejected")
    observed = [line for line in lines if line.startswith(STATUS_PREFIX)]
    if observed != list(statuses):
        raise AssertionError(f"status transcript mismatch: {observed!r}")
    for line in observed:
        parse_status_line(line)
    if require_shell:
        for marker in (SHELL_READY, "query kind:note", "reboot"):
            if lines.count(marker) != 1:
                raise AssertionError(f"missing or duplicate shell response {marker!r}")
        ordered = [READY, *statuses, SHELL_READY, "query kind:note", "reboot"]
    else:
        if any(marker in lines for marker in (SHELL_READY, "query kind:note")):
            raise AssertionError("unexpected recovery shell response")
        ordered = [READY, *statuses]
    cursor = -1
    for marker in ordered:
        position = lines.index(marker)
        if position <= cursor:
            raise AssertionError(f"COM2 marker out of order: {marker!r}")
        cursor = position


def assert_live_barriers(
    timeline: tuple[tuple[str, str], ...],
    barriers: tuple[tuple[str, str, str], ...],
) -> None:
    for status, capture, next_input in barriers:
        required = (("COM2", status), ("HARNESS", capture), ("HARNESS", next_input))
        positions = []
        for event in required:
            if timeline.count(event) != 1:
                raise AssertionError(f"timeline requires exactly one {event!r}")
            positions.append(timeline.index(event))
        if positions != sorted(positions) or len(set(positions)) != 3:
            raise AssertionError(f"status/capture/input barrier violated: {required!r}")


def literal_focus_pixels(x: int, y: int) -> set[tuple[int, int]]:
    expected: set[tuple[int, int]] = set()
    for left, top, width, height in (
        (x - 12, y - 12, 6, 2), (x - 12, y - 12, 2, 6),
        (x + 7, y - 12, 6, 2), (x + 11, y - 12, 2, 6),
        (x - 12, y + 11, 6, 2), (x - 12, y + 7, 2, 6),
        (x + 7, y + 11, 6, 2), (x + 11, y + 7, 2, 6),
    ):
        for row in range(max(0, top), min(480, top + height)):
            for column in range(max(0, left), min(640, left + width)):
                expected.add((column, row))
    return expected


def make_test_ppm(width: int, height: int, colors: dict[tuple[int, int], bytes]) -> bytes:
    pixels = bytearray(width * height * 3)
    for (x, y), color in colors.items():
        offset = (y * width + x) * 3
        pixels[offset:offset + 3] = color
    return f"P6\n{width} {height}\n255\n".encode() + bytes(pixels)


def assert_normal_pixels(data: bytes, focus: tuple[int, int] | None) -> None:
    width, height, pixels = VIEWING.parse_ppm(data)
    if width < 640 or height < 480:
        raise AssertionError("screendump does not contain the fixed 640x480 viewport")
    expected = literal_focus_pixels(*focus) if focus is not None else set()
    for y in range(480):
        for x in range(640):
            offset = (y * width + x) * 3
            wanted = b"\xff\x60\xd0" if (x, y) in expected else b"\0\0\0"
            if pixels[offset:offset + 3] != wanted:
                raise AssertionError(f"viewport pixel mismatch at ({x},{y})")


def _slice(data: bytes, offset: int, length: int, label: str) -> bytes:
    end = offset + length
    if offset < 0 or length < 0 or end < offset or end > len(data):
        raise ValueError(f"{label} lies outside ELF")
    return data[offset:end]


def derive_fault_ud2_rip(data: bytes) -> int:
    """Derive the genuine acceptance UD2 RIP from one exact ELF symbol."""
    if len(data) < 64 or data[:7] != b"\x7fELF\x02\x01\x01":
        raise ValueError("fault image is not ELF64 little-endian version 1")
    try:
        (elf_type, machine, version, _entry, phoff, shoff, _flags, ehsize,
         phentsize, phnum, shentsize, shnum, shstrndx) = struct.unpack_from(
            "<HHIQQQIHHHHHH", data, 16
        )
    except struct.error as error:
        raise ValueError("truncated ELF header") from error
    if (elf_type, machine, version, ehsize, phentsize, shentsize) != (2, 0x3E, 1, 64, 56, 64):
        raise ValueError("unsupported fault ELF header")
    if phnum == 0 or shnum < 2 or shstrndx == 0xFFFF:
        raise ValueError("missing tables or extended section index")
    _slice(data, phoff, phnum * phentsize, "program table")
    _slice(data, shoff, shnum * shentsize, "section table")
    segments = []
    for index in range(phnum):
        fields = struct.unpack_from("<IIQQQQQQ", data, phoff + index * phentsize)
        p_type, p_flags, p_offset, p_vaddr, _paddr, p_filesz, p_memsz, _align = fields
        _slice(data, p_offset, p_filesz, "segment")
        if p_filesz > p_memsz:
            raise ValueError("segment file size exceeds memory size")
        if p_type == 1:
            segments.append((p_flags, p_offset, p_vaddr, p_filesz))
    sections = []
    for index in range(shnum):
        section = struct.unpack_from("<IIQQQQIIQQ", data, shoff + index * shentsize)
        _name, sh_type, sh_flags, sh_addr, sh_offset, sh_size, sh_link, _info, sh_align, sh_entsize = section
        if sh_type != 8:
            _slice(data, sh_offset, sh_size, "section")
        if sh_align not in (0, 1) and sh_offset % sh_align != 0:
            raise ValueError("misaligned section")
        sections.append((sh_type, sh_flags, sh_addr, sh_offset, sh_size, sh_link, sh_entsize))
    wanted = b"pythos_normal_session_fault_acceptance_ud2"
    matches = []
    for sh_type, _flags, _addr, sh_offset, sh_size, sh_link, sh_entsize in sections:
        if sh_type not in (2, 11):
            continue
        if sh_entsize != 24 or sh_size % sh_entsize or not (0 < sh_link < shnum):
            raise ValueError("malformed symbol table")
        string_section = sections[sh_link]
        if string_section[0] != 3:
            raise ValueError("symbol table link is not a string table")
        strings = _slice(data, string_section[3], string_section[4], "string table")
        for offset in range(0, sh_size, sh_entsize):
            st_name, st_info, _other, st_shndx, st_value, st_size = struct.unpack_from(
                "<IBBHQQ", data, sh_offset + offset
            )
            if st_name >= len(strings):
                raise ValueError("symbol name offset outside string table")
            terminator = strings.find(b"\0", st_name)
            if terminator < 0:
                raise ValueError("unterminated symbol name")
            if strings[st_name:terminator] == wanted:
                matches.append((st_info, st_shndx, st_value, st_size))
    if len(matches) != 1:
        raise ValueError("fault ELF must contain exactly one exact acceptance symbol")
    st_info, st_shndx, st_value, st_size = matches[0]
    if st_info & 0x0F != 2 or not (0 < st_shndx < 0xFF00) or st_shndx >= shnum:
        raise ValueError("fault symbol is not a defined STT_FUNC")
    if st_size != 5:
        raise ValueError("fault helper size changed")
    section = sections[st_shndx]
    _type, sh_flags, sh_addr, sh_offset, sh_size, _link, _entsize = section
    if sh_flags & 0x4 == 0 or sh_flags & 0x1 or not (sh_addr <= st_value and st_value + st_size <= sh_addr + sh_size):
        raise ValueError("fault symbol is not wholly in its executable nonwritable section")
    section_file_offset = sh_offset + (st_value - sh_addr)
    helper = _slice(data, section_file_offset, st_size, "fault helper")
    containing = [segment for segment in segments if (
        segment[0] & 0x1 and not segment[0] & 0x2
        and segment[2] <= st_value and st_value + st_size <= segment[2] + segment[3]
    )]
    if len(containing) != 1:
        raise ValueError("fault symbol is not in one executable nonwritable file-backed PT_LOAD")
    segment = containing[0]
    segment_file_offset = segment[1] + (st_value - segment[2])
    if segment_file_offset != section_file_offset or helper != b"\x50\x0f\x0b\x0f\x0b":
        raise ValueError("fault helper mapping or pinned bytes changed")
    return st_value + 1


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def assert_profile_pair(ordinary: ProfileIdentity, fault: ProfileIdentity) -> None:
    if ordinary.fault or not fault.fault:
        raise AssertionError("ordinary/fault profile labels are incorrect")
    if ordinary.kernel_sha256 != fault.kernel_sha256 or ordinary.graph_sha256 != fault.graph_sha256:
        raise AssertionError("ordinary and fault profiles must share the exact kernel and graph")
    if ordinary.user_elf_sha256 == fault.user_elf_sha256:
        raise AssertionError("fault profile must use its independently built fault ELF")


def assert_storage_unchanged(snapshots: tuple[StorageSnapshot, ...]) -> None:
    if len(snapshots) < 2:
        raise AssertionError("storage proof needs before and after snapshots")
    initial = snapshots[0]
    if initial.size != 16 * 1024 * 1024:
        raise AssertionError("storage fixture is not 16 MiB")
    for snapshot in snapshots:
        if snapshot.path_identity != snapshot.retained_identity:
            raise AssertionError("storage path was replaced during boot")
        if (snapshot.size, snapshot.sha256) != (initial.size, initial.sha256):
            raise AssertionError("storage bytes changed during normal session")


T = TypeVar("T")


def run_with_cleanup(action: Callable[[], T], cleanup: Callable[[], None]) -> T:
    try:
        return action()
    finally:
        cleanup()


def persist_capture(path: Path, captured: bytes) -> None:
    """Retain the serial stream byte-for-byte, including guest CRLF."""
    path.write_bytes(captured)


def run_checked(command: list[str]) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


def build_profiles(*, fault: bool) -> tuple[ProfileIdentity, ProfileIdentity | None]:
    commands = (
        ["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"],
        [sys.executable, "scripts/build-user-shell.py"],
        ["cargo", "build", "-p", "pythos-core", "--bin", "pythcore", "--target", "x86_64-unknown-none", "--target-dir", "target/normal-session-kernel"],
        [sys.executable, "scripts/build-session-runtime.py", "--features", "normal-session"],
        ["cargo", "run", "-p", "pythc", "--", "build", "programs/normal-session-manager/main.pyth", "-o", str(GRAPH.relative_to(ROOT))],
        ["cargo", "run", "-p", "pyth-tig-tool", "--", "verify", str(GRAPH.relative_to(ROOT))],
    )
    for command in commands:
        run_checked(command)
    ordinary = ProfileIdentity(sha256(KERNEL), sha256(GRAPH), sha256(NORMAL_ELF), False)
    selected = NORMAL_ELF
    fault_identity = None
    if fault:
        run_checked([sys.executable, "scripts/build-session-runtime.py", "--features", "normal-session-fault-test"])
        derive_fault_ud2_rip(FAULT_ELF.read_bytes())
        fault_identity = ProfileIdentity(sha256(KERNEL), sha256(GRAPH), sha256(FAULT_ELF), True)
        assert_profile_pair(ordinary, fault_identity)
        selected = FAULT_ELF
    run_checked([
        sys.executable, "scripts/build-image.py", "--kernel", str(KERNEL.relative_to(ROOT)),
        "--normal-session-elf", str(selected.relative_to(ROOT)),
        "--normal-session-graph", str(GRAPH.relative_to(ROOT)),
    ])
    return ordinary, fault_identity


def snapshot_storage(path: Path, retained) -> StorageSnapshot:
    observed = RUNTIME.snapshot_image(path, retained)
    return StorageSnapshot(observed.path_identity, observed.retained_identity, observed.size, observed.sha256)


def capture_live(path: Path, process, serial: SerialTail, timeline: AcceptanceTimeline) -> bytes:
    if path.exists():
        raise AssertionError(f"refusing to overwrite screenshot {path}")
    if process.poll() is not None or serial.contains_all((CORE_PREFIX + "RECOVERY reason=explicit",)):
        raise AssertionError("session stopped before live screenshot")
    QEMU.request_screendump(path.resolve())
    data = path.read_bytes()
    timeline.record("HARNESS", "CAPTURE:" + path.stem)
    return data


def request_status(sock, collector: Com2Collector, expected: str) -> None:
    sock.sendall(b"status\n")
    collector.read_until(expected.encode(), WAIT_TIMEOUT)


def assert_com1_common(com1: str) -> None:
    lines = complete_lines(com1)
    ordered = (
        "PYTHOS:CORE:BOOT_VISUAL:FRAME",
        "PYTHOS:CORE:BOOT_SYNC:AUDIO",
        "PYTHOS:CORE:NORMAL_BOOT:AUDIO_VISUAL_SYNC_READY",
        "PYTHOS:CORE:COM2_READY",
        CORE_PREFIX + "ENTER",
        DRAW_PREFIX + "revision:0 active:0 x:0 y:0",
    )
    cursor = -1
    for marker in ordered:
        if lines.count(marker) != 1:
            raise AssertionError(f"COM1 requires exactly one {marker!r}")
        position = lines.index(marker)
        if position <= cursor:
            raise AssertionError(f"COM1 marker out of order: {marker!r}")
        cursor = position
    if any("LAUNCHER_READY" in line for line in lines):
        raise AssertionError("normal session used the compatibility launcher")
    if any(":FATAL" in line or "PANIC" in line for line in lines):
        raise AssertionError("fatal or panic evidence in normal boot")


def assert_explicit_recovery(com1: str) -> None:
    lines = complete_lines(com1)
    required = (
        "PYTHOS:CORE:USER_MODE:RETURN",
        CORE_PREFIX + "RECOVERY reason=explicit",
        CORE_PREFIX + "CLEANUP_OK",
        "PYTHOS:SHELL:RING3_ENTER",
    )
    positions = []
    for marker in required:
        if lines.count(marker) != 1:
            raise AssertionError(f"explicit recovery requires exactly one {marker!r}")
        positions.append(lines.index(marker))
    if positions != sorted(positions):
        raise AssertionError("explicit recovery markers are out of order")


def assert_fault_recovery(com1: str, expected_rip: int) -> None:
    lines = complete_lines(com1)
    contexts = [line for line in lines if line.startswith(FAULT_CONTEXT_PREFIX)]
    if len(contexts) != 1:
        raise AssertionError("native fault requires one exact context record")
    match = FAULT_CONTEXT_PATTERN.fullmatch(contexts[0])
    if match is None:
        raise AssertionError("native fault context record is malformed")
    principal, vector, rip, rsp, cr2 = match.groups()
    if principal != "50595352544D0001" or int(vector) != 6 or int(rip, 16) != expected_rip:
        raise AssertionError("native fault identity/vector/RIP does not match the selected ELF")
    rsp_value = int(rsp, 16)
    canonical_rsp = rsp_value < (1 << 47) or rsp_value >= 0xFFFF800000000000
    if rsp_value == 0 or not canonical_rsp or rsp_value % 8 != 0 or int(cr2, 16) != 0:
        raise AssertionError("native fault stack/CR2 context is invalid")
    required = (
        contexts[0],
        CORE_PREFIX + "RECOVERY reason=native-fault",
        CORE_PREFIX + "CLEANUP_OK",
        "PYTHOS:SHELL:RING3_ENTER",
    )
    positions = [lines.index(marker) for marker in required]
    if positions != sorted(positions):
        raise AssertionError("native fault recovery markers are out of order")


def run_boot(
    ordinal: int,
    artifact_dir: Path,
    storage: Path,
    *,
    fault: bool,
    recover: bool,
    expected_fault_rip: int | None = None,
) -> BootEvidence:
    serial_path = artifact_dir / f"boot-{ordinal}-com1.log"
    com2_path = artifact_dir / f"boot-{ordinal}-com2.log"
    runner_path = artifact_dir / f"boot-{ordinal}-runner.log"
    timeline_path = artifact_dir / f"boot-{ordinal}-timeline.log"
    if any(path.exists() for path in (serial_path, com2_path, runner_path, timeline_path)):
        raise AssertionError("refusing to overwrite retained normal-session evidence")
    command = [
        sys.executable, "scripts/run-qemu.py", "--serial-log", str(serial_path),
        "--shell-port", str(SHELL_PORT), "--timeout", str(QEMU_TIMEOUT),
        "--storage-image", str(storage),
    ]
    print("+ " + " ".join(command), flush=True)
    kwargs: dict[str, object] = {"cwd": ROOT}
    if sys.platform != "win32":
        kwargs["start_new_session"] = True
    runner = spawn_runner_process(command, **kwargs)
    timeline = AcceptanceTimeline()
    serial = SerialTail(serial_path, timeline)
    observer = Com1Observer(serial)
    capture = RunnerCapture(runner.process, timeline)
    collector: Com2Collector | None = None
    screenshots: list[bytes] = []
    errors: list[BaseException] = []
    tree_reaped = False
    capture.start()
    observer.start()
    try:
        with connect_com2(SHELL_PORT, 25.0) as sock:
            collector = Com2Collector(sock, timeline)
            collector.read_until(READY.encode(), 35.0)
            observer.wait_for((CORE_PREFIX + "ENTER",), 35.0, runner.process, capture)
            oracle = StatusOracle()
            statuses: list[str] = []
            barriers: list[tuple[str, str, str]] = []
            if fault:
                launcher_click.press_qcode_keys(["spc", "spc"])
                oracle.accept_keys(("spc", "spc"))
                statuses.append(oracle.next_status_line())
                request_status(sock, collector, statuses[-1])
                launcher_click.press_qcode_keys(["backspace", "backspace"])
                launcher_click.send_relative_mouse_motion(7, -7)
                launcher_click.press_qcode_keys(["ret", "a", "b"])
                oracle.accept_keys(("backspace", "backspace"))
                oracle.accept_motion(7, -7)
                oracle.accept_keys(("ret", "a", "b"))
                statuses.append(oracle.next_status_line())
                request_status(sock, collector, statuses[-1])
                shot_path = artifact_dir / "fault-moved.ppm"
                screenshots.append(capture_live(shot_path, runner.process, serial, timeline))
                barriers.append((statuses[-1], "CAPTURE:" + shot_path.stem, "COMMAND:fault-status-3"))
                timeline.record("HARNESS", "COMMAND:fault-status-3")
                statuses.append(oracle.next_status_line())
                request_status(sock, collector, statuses[-1])
                observer.wait_for((CORE_PREFIX + "CLEANUP_OK", "PYTHOS:SHELL:RING3_ENTER"), WAIT_TIMEOUT, runner.process, capture)
                sock.sendall(b"help\n")
                collector.read_until(b"query kind:note", WAIT_TIMEOUT)
                collector.read_until(b"reboot", WAIT_TIMEOUT)
            else:
                statuses.append(oracle.next_status_line())
                request_status(sock, collector, statuses[-1])
                shot_path = artifact_dir / f"boot-{ordinal}-inactive.ppm"
                screenshots.append(capture_live(shot_path, runner.process, serial, timeline))
                barriers.append((statuses[-1], "CAPTURE:" + shot_path.stem, "INPUT:spaces"))
                timeline.record("HARNESS", "INPUT:spaces")
                launcher_click.press_qcode_keys(["spc", "spc"])
                oracle.accept_keys(("spc", "spc"))
                time.sleep(0.2)
                oracle.idle()
                statuses.append(oracle.next_status_line())
                request_status(sock, collector, statuses[-1])
                launcher_click.press_qcode_keys(["backspace", "backspace"])
                oracle.accept_keys(("backspace", "backspace"))
                statuses.append(oracle.next_status_line())
                request_status(sock, collector, statuses[-1])
                shot_path = artifact_dir / f"boot-{ordinal}-active.ppm"
                screenshots.append(capture_live(shot_path, runner.process, serial, timeline))
                barriers.append((statuses[-1], "CAPTURE:" + shot_path.stem, "INPUT:motion-enter-a-b"))
                timeline.record("HARNESS", "INPUT:motion-enter-a-b")
                time.sleep(0.2)
                oracle.idle()
                launcher_click.send_relative_mouse_motion(7, -7)
                launcher_click.press_qcode_keys(["ret", "a", "b"])
                oracle.accept_motion(7, -7)
                oracle.accept_keys(("ret", "a", "b"))
                statuses.append(oracle.next_status_line())
                request_status(sock, collector, statuses[-1])
                shot_path = artifact_dir / f"boot-{ordinal}-moved.ppm"
                screenshots.append(capture_live(shot_path, runner.process, serial, timeline))
                barriers.append((statuses[-1], "CAPTURE:" + shot_path.stem, "COMMAND:status-5"))
                timeline.record("HARNESS", "COMMAND:status-5")
                statuses.append(oracle.next_status_line())
                request_status(sock, collector, statuses[-1])
                if recover:
                    sock.sendall(b"recover\n")
                    observer.wait_for((CORE_PREFIX + "CLEANUP_OK", "PYTHOS:SHELL:RING3_ENTER"), WAIT_TIMEOUT, runner.process, capture)
                    collector.read_until(SHELL_READY.encode(), WAIT_TIMEOUT)
                    sock.sendall(b"help\n")
                    collector.read_until(b"query kind:note", WAIT_TIMEOUT)
                    collector.read_until(b"reboot", WAIT_TIMEOUT)
            assert_complete_transcript(
                bytes(collector.captured).decode("utf-8", errors="replace"),
                tuple(statuses),
                require_shell=fault or recover,
            )
            assert_live_barriers(tuple(timeline.events), tuple(barriers))
    except BaseException as error:
        errors.append(error)
    finally:
        try:
            observer.stop_join()
        except BaseException as error:
            errors.append(error)
        tracker = None
        try:
            tracker = RUNTIME.track_runner_tree(runner)
        except BaseException as error:
            errors.append(error)
        try:
            cleanup_runner_process(runner)
        except BaseException as error:
            errors.append(error)
        if tracker is not None:
            try:
                tree_reaped = tracker.wait_reaped(5.0)
            except BaseException as error:
                errors.append(error)
    output = capture.finish()
    com1 = serial.transcript()
    raw_com2 = bytes(collector.captured) if collector else b""
    com2 = raw_com2.decode("utf-8", errors="replace")
    persist_capture(com2_path, raw_com2)
    runner_path.write_text(output, encoding="utf-8")
    timeline_path.write_text("\n".join(f"{source} {value}" for source, value in timeline.events) + "\n", encoding="utf-8")
    if errors:
        raise AssertionError(
            f"boot {ordinal}: {'; '.join(str(error) for error in errors)}\nCOM1:\n{com1}\nCOM2:\n{com2}\nrunner:\n{output}"
        ) from errors[0]
    evidence = BootEvidence(com1, com2, output, tuple(timeline.events), tuple(screenshots), tree_reaped)
    assert_com1_common(com1)
    if not tree_reaped:
        raise AssertionError("runner/QEMU process tree survived cleanup")
    if fault:
        if expected_fault_rip is None:
            raise AssertionError("fault boot omitted exact selected-ELF RIP")
        assert_fault_recovery(com1, expected_fault_rip)
        assert_normal_pixels(screenshots[0], (0x147, 0x0E9))
    elif recover:
        assert_explicit_recovery(com1)
    else:
        forbidden = ("PYTHOS:CORE:USER_MODE:RETURN", CORE_PREFIX + "RECOVERY ", "PYTHOS:SHELL:RING3_ENTER")
        if any(marker in com1 for marker in forbidden):
            raise AssertionError("ordinary live boot returned before external cleanup")
    if not fault:
        for screenshot, focus in zip(screenshots, (None, (0x140, 0x0F0), (0x147, 0x0E9)), strict=True):
            assert_normal_pixels(screenshot, focus)
    return evidence


def run_ordinary() -> int:
    ordinary, _ = build_profiles(fault=False)
    TARGET_DIR.mkdir(parents=True, exist_ok=True)
    artifact_dir = Path(tempfile.mkdtemp(prefix="run-", dir=TARGET_DIR))
    print(f"NORMAL_SESSION_ARTIFACTS {artifact_dir}")
    print(f"NORMAL_SESSION_KERNEL_SHA256 {ordinary.kernel_sha256}")
    print(f"NORMAL_SESSION_GRAPH_SHA256 {ordinary.graph_sha256}")
    print(f"NORMAL_SESSION_USER_ELF_SHA256 {ordinary.user_elf_sha256}")
    storage = artifact_dir / "normal-session-store.img"
    RUNTIME.create_zeroed_storage_image(storage)
    with RUNTIME.open_retained_storage_image(storage) as retained:
        snapshots = [snapshot_storage(storage, retained)]
        boots = []
        for ordinal in (1, 2):
            boots.append(run_boot(ordinal, artifact_dir, storage, fault=False, recover=ordinal == 2))
            snapshots.append(snapshot_storage(storage, retained))
    assert_storage_unchanged(tuple(snapshots))
    if snapshots[0].sha256 != EXPECTED_ZEROED_SHA256:
        raise AssertionError("storage fixture was not fresh all-zero data")
    if sum(len(boot.screenshots) for boot in boots) != 6:
        raise AssertionError("ordinary acceptance did not retain exactly six screenshots")
    for index, snapshot in enumerate(snapshots):
        print(f"NORMAL_SESSION_STORAGE_{index}_SHA256 {snapshot.sha256}")
    print("NORMAL_SESSION_TWO_BOOT_ACCEPTANCE_OK")
    return 0


def run_fault() -> int:
    ordinary, fault = build_profiles(fault=True)
    if fault is None:
        raise AssertionError("fault build identity missing")
    assert_profile_pair(ordinary, fault)
    expected_rip = derive_fault_ud2_rip(FAULT_ELF.read_bytes())
    FAULT_TARGET_DIR.mkdir(parents=True, exist_ok=True)
    artifact_dir = Path(tempfile.mkdtemp(prefix="run-", dir=FAULT_TARGET_DIR))
    print(f"NORMAL_SESSION_FAULT_ARTIFACTS {artifact_dir}")
    print(f"NORMAL_SESSION_KERNEL_SHA256 {fault.kernel_sha256}")
    print(f"NORMAL_SESSION_GRAPH_SHA256 {fault.graph_sha256}")
    print(f"NORMAL_SESSION_FAULT_USER_ELF_SHA256 {fault.user_elf_sha256}")
    print(f"NORMAL_SESSION_FAULT_EXPECTED_RIP {expected_rip:016X}")
    storage = artifact_dir / "normal-session-fault-store.img"
    RUNTIME.create_zeroed_storage_image(storage)
    with RUNTIME.open_retained_storage_image(storage) as retained:
        before = snapshot_storage(storage, retained)
        run_boot(1, artifact_dir, storage, fault=True, recover=False, expected_fault_rip=expected_rip)
        after = snapshot_storage(storage, retained)
    assert_storage_unchanged((before, after))
    if before.sha256 != EXPECTED_ZEROED_SHA256:
        raise AssertionError("fault storage fixture was not fresh all-zero data")
    print(f"NORMAL_SESSION_FAULT_STORAGE_BEFORE_SHA256 {before.sha256}")
    print(f"NORMAL_SESSION_FAULT_STORAGE_AFTER_SHA256 {after.sha256}")
    print("NORMAL_SESSION_NATIVE_FAULT_ACCEPTANCE_OK")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fault", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.defaultTestLoader.discover(str(ROOT / "tests"), "test_normal_session.py")
        return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1
    return run_fault() if args.fault else run_ordinary()


if __name__ == "__main__":
    raise SystemExit(main())
