#!/usr/bin/env python
"""Prove the native graph window through QEMU input, frames, and NVMe recovery."""

from __future__ import annotations

import hashlib
import importlib.util
import re
import statistics
import struct
import time
from pathlib import Path
from typing import NamedTuple
from uuid import uuid4

PREFIX = "PYTHOS:WORLD:GRAPH_WINDOW_"
PIT_HZ = 120
SAVE_DELAY_TICKS = 30
FIXTURE_MARKER = b"PYWGNV01" + struct.pack("<Q", SAVE_DELAY_TICKS)
ACCENT = bytes.fromhex("70F2CD")
ALTERNATE_ACCENT = bytes.fromhex("FC8D7B")


class Window(NamedTuple):
    revision: int
    x: int
    y: int
    z: int
    left: int
    top: int
    width: int
    height: int
    title: int

    @property
    def origin(self):
        return self.left, self.top


class Frame(NamedTuple):
    frame: int
    tick: int
    left: int
    top: int
    animation: int
    drag: int
    saving: int

    @property
    def origin(self):
        return self.left, self.top


class Drag(NamedTuple):
    sequence: int


class Pending(NamedTuple):
    revision: int
    sequence: int


class Placement(NamedTuple):
    revision: int
    sequence: int
    x: int
    y: int
    z: int
    left: int
    top: int


RECORDS = {
    "READY": Window,
    "FRAME": Frame,
    "DRAG_BEGIN": Drag,
    "SAVE_PENDING": Pending,
    "BODY_READY": Placement,
    "COMMITTED": Placement,
}
PATTERNS = {
    name: re.compile(PREFIX + name + "".join(rf" {field}:(-?\d+)" for field in record._fields))
    for name, record in RECORDS.items()
}


def markers(guest, name, *, start=0, end=None):
    pattern = PATTERNS[name]
    record = RECORDS[name]
    for index, line in enumerate(guest.tail.lines[start:end], start):
        match = pattern.fullmatch(line)
        if match:
            yield index, record(*map(int, match.groups()))


def wait_for(guest, description, read, *, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        guest.tail.poll()
        value = read()
        if value is not None:
            return value
        if guest.process.poll() is not None or "PYTHOS:PANIC" in guest.tail.lines:
            break
        time.sleep(0.01)
    raise AssertionError(f"Expected {description}. See {guest.serial} and {guest.result}")


def wait_marker(guest, name, *, start=0, matches=lambda record: True):
    return wait_for(guest, PREFIX + name, lambda: next(
        ((index, record) for index, record in markers(guest, name, start=start) if matches(record)), None,
    ))


def read_frames(guest, *, start=0, end=None):
    return [frame for _, frame in markers(guest, "FRAME", start=start, end=end)]


def wait_frames(guest, count, start):
    def enough_frames():
        frames = read_frames(guest, start=start)
        return frames[:count] if len(frames) >= count else None

    return wait_for(guest, f"{count} frames after serial line {start}", enough_frames, timeout=5)


def verify_cadence(label, frames):
    assert len(frames) >= 8, (label, len(frames))
    assert all(right.frame > left.frame for left, right in zip(frames, frames[1:])), label
    intervals = [right.tick - left.tick for left, right in zip(frames, frames[1:])]
    median = statistics.median(intervals)
    assert 0 < min(intervals) and median <= 2 and max(intervals) <= 12, (label, intervals)
    assert sum(interval <= 3 for interval in intervals) * 4 >= len(intervals) * 3, (label, intervals)
    assert len({frame.animation for frame in frames}) >= 3, (label, "animation did not advance")
    fps = len(intervals) * PIT_HZ / sum(intervals)
    print(f"{label}: {len(frames)} frames, {fps:.1f} fps by PIT, "
          f"tick intervals {min(intervals)}/{median:g}/{max(intervals)} min/median/max", flush=True)


class Picture(NamedTuple):
    width: int
    height: int
    pixels: bytes

    def rows(self, left, top, width, height):
        assert 0 <= left < left + width <= self.width
        assert 0 <= top < top + height <= self.height
        for y in range(top, top + height):
            start = (y * self.width + left) * 3
            yield self.pixels[start:start + width * 3]


def screenshot(qemu, path: Path):
    qemu.request_screendump(path.resolve())
    magic, size, intensity, pixels = path.read_bytes().split(b"\n", 3)
    width, height = map(int, size.split())
    assert magic == b"P6" and intensity == b"255" and len(pixels) == 3 * width * height, path
    return Picture(width, height, pixels)


def title_pixels(picture, window):
    # Sample the static left side of the title bar, clear of the centered pointer.
    return b"".join(picture.rows(window.left + 8, window.top + 4, window.width // 3, window.title - 8))


def accent_bounds(picture, window, color):
    # Match horizontal runs in the content, excluding the title and its text.
    run = color * 8
    spans = []
    for y, row in enumerate(picture.rows(window.left, window.top + window.title,
                                         window.width, window.height - window.title)):
        first = row.find(run)
        if first >= 0 and first % 3 == 0:
            spans.append((first // 3, y, row.rfind(run) // 3 + 8))
    if not spans:
        return None
    return min(left for left, _, _ in spans), spans[0][1], max(right for _, _, right in spans), spans[-1][1]


def button(qemu, down):
    qemu.run_qmp_commands(({
        "execute": "input-send-event",
        "arguments": {"events": [{"type": "btn", "data": {"down": down, "button": "left"}}]},
    },))


def exit_guest(qemu, guest):
    # QEMU releases the key locally even if the optimized guest exits first.
    qemu.run_qmp_commands(({
        "execute": "send-key",
        "arguments": {"keys": [{"type": "qcode", "data": "0"}], "hold-time": 50},
    },))
    guest.finish(0, "success")


def graph_digest(scratch):
    return hashlib.sha256((scratch / "graph-window.tig").read_bytes()).digest()


def verify_graph(guest, ready_index, digest):
    lines = guest.tail.lines[:ready_index]
    execution = [index for index, line in enumerate(lines) if line.startswith(PREFIX + "GRAPH ")]
    assert len(execution) == 1, "Expected one execution receipt from the packaged graph"
    index = execution[0]
    assert re.fullmatch(rf"{PREFIX}GRAPH executed:[1-9]\d* sha256:{digest.hex().upper()}", lines[index])
    assert lines.index("PYTHOS:CORE:USER_MODE:RETURN") < index


def verify_scene(guest, qemu, scratch):
    ready_index, window = wait_marker(guest, "READY")
    verify_graph(guest, ready_index, graph_digest(scratch))
    assert (window.revision, window.z, window.width, window.height, window.title) == (0, -3000, 352, 208, 36)
    idle = wait_frames(guest, 20, ready_index + 1)
    assert all(frame.origin == window.origin and (frame.drag, frame.saving) == (0, 0) for frame in idle)
    verify_cadence("Idle animation", idle)

    initial = screenshot(qemu, scratch / "idle-first.ppm")
    first_accent = accent_bounds(initial, window, ACCENT)
    assert first_accent is not None, "The packaged graph's rectangle is absent"
    for sample in range(2, 4):
        wait_frames(guest, 10, len(guest.tail.lines))
        later = screenshot(qemu, scratch / f"idle-{sample}.ppm")
        later_accent = accent_bounds(later, window, ACCENT)
        assert later_accent is not None, "The graph's rectangle disappeared during animation"
        if later_accent != first_accent:
            break
    else:
        raise AssertionError("The graph's rectangle did not move between idle captures")
    return window, initial


def verify_content_does_not_drag(guest, qemu, move, window):
    # Move from the title center to the content, then click and move while held.
    offset = window.title // 2 + 16
    start = len(guest.tail.lines)
    move(0, offset)
    wait_frames(guest, 6, start)
    button(qemu, True)
    wait_frames(guest, 6, len(guest.tail.lines))
    move(28, 0)
    wait_frames(guest, 6, len(guest.tail.lines))
    button(qemu, False)
    wait_frames(guest, 6, len(guest.tail.lines))
    move(-28, -offset)
    wait_frames(guest, 6, len(guest.tail.lines))
    assert all(frame.origin == window.origin and (frame.drag, frame.saving) == (0, 0)
               for frame in read_frames(guest, start=start)), "Content input moved or saved the window"
    for name in ("DRAG_BEGIN", "SAVE_PENDING", "BODY_READY", "COMMITTED"):
        assert not list(markers(guest, name, start=start)), f"Unexpected {name} after clicking the content"


def drag_to(guest, qemu, move, origin, steps):
    start = len(guest.tail.lines)
    button(qemu, True)
    begin_index, begin = wait_marker(guest, "DRAG_BEGIN", start=start)
    left, top = origin
    for dx, dy in steps:
        left, top = left + dx, top + dy
        start = len(guest.tail.lines)
        move(dx, dy)
        wait_marker(guest, "FRAME", start=start,
                    matches=lambda frame: frame.drag == 1 and frame.origin == (left, top))
    return begin_index, begin, (left, top)


def release_drag(guest, qemu, begin_index, begin, revision):
    button(qemu, False)
    index, pending = wait_marker(guest, "SAVE_PENDING", start=begin_index + 1)
    assert pending.revision == revision and pending.sequence > begin.sequence, pending
    return index, pending


def verify_placement(placement, pending, origin):
    assert (placement.revision, placement.sequence, placement.z, placement.left, placement.top) == (
        pending.revision, pending.sequence, -3000, *origin,
    ), placement


def verify_checkpoint(nvme, image, placement, digest, *, committed):
    # Odd revisions use records 3 and 4. Even revisions use records 1 and 2.
    record = 3 if placement.revision % 2 else 1
    page_bytes = nvme.STRIDE * nvme.SECTOR
    body_start = nvme.FIRST * nvme.SECTOR + record * page_bytes
    marker_start = body_start + page_bytes
    body = struct.pack("<8sQQiii32s", b"PYGWPS01", placement.revision, placement.sequence,
                       placement.x, placement.y, placement.z, digest).ljust(nvme.SECTOR, b"\0")
    assert image[body_start:marker_start] == body.ljust(page_bytes, b"\0"), f"Wrong body in record {record}"
    marker = b""
    if committed:
        marker = b"PYGWCM01" + struct.pack("<Q", placement.revision) + hashlib.sha256(body).digest()
    assert image[marker_start:marker_start + page_bytes] == marker.ljust(page_bytes, b"\0"), (
        f"Wrong commit marker in record {record + 1}"
    )


def verify_exit_after_drop(guest, qemu, move, origin, step):
    begin_index, begin, moved = drag_to(guest, qemu, move, origin, (step,))
    pending_index, pending = release_drag(guest, qemu, begin_index, begin, revision=2)
    exit_guest(qemu, guest)
    body_index, body = wait_marker(guest, "BODY_READY", start=pending_index + 1)
    commit_index, commit = wait_marker(guest, "COMMITTED", start=body_index + 1)
    verify_placement(commit, pending, moved)
    assert body == commit
    lines = guest.tail.lines
    key_index = lines.index("PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED")
    assert pending_index < key_index < commit_index < lines.index("PYTHOS:WORLD:EXIT"), (
        "The shutdown request did not arrive while the save was pending"
    )
    assert len(list(markers(guest, "COMMITTED"))) == 1
    return commit


def verify_drag_and_recovery(world, nvme, qemu, esp, scratch):
    disk = scratch / "world.img"
    nvme.create_disk(disk, marker=FIXTURE_MARKER)
    protected = nvme.protected_disk_hash(disk)
    move = world.launcher_click.send_relative_mouse_motion

    with world.guest(esp, disk, scratch, "author", nvme=True,
                     kill="GRAPH_WINDOW_BODY_READY revision:2 ") as guest:
        window, initial = verify_scene(guest, qemu, scratch)
        verify_content_does_not_drag(guest, qemu, move, window)

        dx = 1 if window.left + window.width + 60 < initial.width else -1
        dy = 1 if window.top + window.height + 42 < initial.height else -1
        steps = ((32 * dx, 24 * dy), (28 * dx, 18 * dy))
        begin_index, begin, moved = drag_to(guest, qemu, move, window.origin, steps)
        held = wait_frames(guest, 12, len(guest.tail.lines))
        assert all(frame.origin == moved and (frame.drag, frame.saving) == (1, 0) for frame in held)
        verify_cadence("Held drag", read_frames(guest, start=begin_index + 1))
        pending_index, pending = release_drag(guest, qemu, begin_index, begin, revision=1)
        move(34 * dx, 0)
        body_index, body = wait_marker(guest, "BODY_READY", start=pending_index + 1)
        commit_index, commit = wait_marker(guest, "COMMITTED", start=body_index + 1)
        verify_placement(commit, pending, moved)
        assert body == commit
        assert (commit.x, commit.y) != (window.x, window.y), "The drop did not change the world pose"
        saving = read_frames(guest, start=pending_index + 1, end=commit_index)
        assert all(frame.origin == moved and (frame.drag, frame.saving) == (0, 1) for frame in saving)
        after_release = wait_frames(guest, 8, commit_index + 1)
        assert all(frame.origin == moved and (frame.drag, frame.saving) == (0, 0) for frame in after_release)
        before_save = read_frames(guest, start=begin_index + 1, end=pending_index)[-1]
        verify_cadence("Pending save", [before_save, *saving, after_release[0]])
        awaiting_marker = read_frames(guest, start=body_index + 1, end=commit_index)
        assert len(awaiting_marker) >= 8 and awaiting_marker[-1].tick - awaiting_marker[0].tick >= SAVE_DELAY_TICKS - 6, (
            "Frames did not cover the pause between body readback and marker write"
        )

        moved_window = window._replace(left=moved[0], top=moved[1])
        placed = screenshot(qemu, scratch / "committed.ppm")
        placed_title = title_pixels(placed, moved_window)
        assert placed_title == title_pixels(initial, window), "Title pixels did not move to the committed bounds"
        assert placed_title != title_pixels(placed, window), "The old title was left on screen"
        assert accent_bounds(placed, moved_window, ACCENT) is not None

        move(-34 * dx, 0)
        wait_frames(guest, 6, len(guest.tail.lines))
        next_index, next_drag, torn_origin = drag_to(guest, qemu, move, moved, ((-30 * dx, -25 * dy),))
        pending_index, pending = release_drag(guest, qemu, next_index, next_drag, revision=2)
        _, interrupted = wait_marker(guest, "BODY_READY", start=pending_index + 1)
        verify_placement(interrupted, pending, torn_origin)
        guest.finish(21, "reset")
        assert [record for _, record in markers(guest, "COMMITTED")] == [commit]
        assert [record for _, record in markers(guest, "BODY_READY")] == [commit, interrupted]
        assert len(list(markers(guest, "READY"))) == 1
        assert len(list(markers(guest, "DRAG_BEGIN"))) == 2
        assert len(list(markers(guest, "SAVE_PENDING"))) == 2

    assert nvme.protected_disk_hash(disk) == protected, "Writes escaped the four checkpoint pages"
    after_kill = disk.read_bytes()
    digest = graph_digest(scratch)
    verify_checkpoint(nvme, after_kill, commit, digest, committed=True)
    verify_checkpoint(nvme, after_kill, interrupted, digest, committed=False)
    with world.guest(esp, disk, scratch, "recovered", nvme=True) as guest:
        ready_index, restored = wait_marker(guest, "READY")
        verify_graph(guest, ready_index, digest)
        assert restored == moved_window._replace(revision=1, x=commit.x, y=commit.y), restored
        wait_frames(guest, 8, ready_index + 1)
        recovered = screenshot(qemu, scratch / "recovered.ppm")
        assert title_pixels(recovered, restored) == placed_title
        assert accent_bounds(recovered, restored, ACCENT) is not None
        shutdown_commit = verify_exit_after_drop(guest, qemu, move, restored.origin, (-24 * dx, -22 * dy))
        assert len(list(markers(guest, "READY"))) == 1
    after_exit = disk.read_bytes()
    assert nvme.protected_disk_hash(disk) == protected, "Shutdown wrote outside the four checkpoint pages"
    verify_checkpoint(nvme, after_exit, commit, digest, committed=True)
    verify_checkpoint(nvme, after_exit, shutdown_commit, digest, committed=True)


def verify_alternate_graph(world, nvme, qemu, builder, scratch):
    alternate = scratch / "alternate"
    alternate.mkdir()
    original = world.ROOT / "user/probes/graph-window/window.pyth"
    source = alternate / "window.pyth"
    graph = original.read_text(encoding="utf-8")
    assert graph.count("70F2CD") == 1, "Expected one graph rectangle color to replace"
    source.write_text(graph.replace("70F2CD", "FC8D7B"), encoding="utf-8")
    esp = builder.build(alternate, source=source)
    disk = alternate / "world.img"
    nvme.create_disk(disk, marker=FIXTURE_MARKER)
    original_disk = disk.read_bytes()
    with world.guest(esp, disk, alternate, "alternate", nvme=True) as guest:
        ready_index, window = wait_marker(guest, "READY")
        digest = graph_digest(alternate)
        assert digest != graph_digest(scratch), "Alternate source produced the same packaged graph"
        verify_graph(guest, ready_index, digest)
        assert window.revision == 0
        wait_frames(guest, 8, ready_index + 1)
        picture = screenshot(qemu, alternate / "recolored.ppm")
        assert accent_bounds(picture, window, ALTERNATE_ACCENT) is not None, "New graph color did not reach the window"
        assert accent_bounds(picture, window, ACCENT) is None, "The old graph color was still rendered"
        exit_guest(qemu, guest)
    assert disk.read_bytes() == original_disk


def main():
    spec = importlib.util.spec_from_file_location("nvme_world", Path(__file__).with_name("test-nvme-world.py"))
    nvme = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(nvme)
    world = nvme.load_world_harness()
    qemu = world.load_script("run-qemu")
    builder = world.load_script("build-graph-window")
    scratch = world.TARGET / f"graph-window-{uuid4().hex[:8]}"
    scratch.mkdir(parents=True)
    print(f"Native graph window evidence in {scratch}", flush=True)
    esp = builder.build(scratch)
    verify_drag_and_recovery(world, nvme, qemu, esp, scratch)
    verify_alternate_graph(world, nvme, qemu, builder, scratch)
    print("PYTHOS_GRAPH_WINDOW_OK", flush=True)


if __name__ == "__main__":
    main()
