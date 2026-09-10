#!/usr/bin/env python
"""Two-boot, live-snapshot acceptance for the opt-in retained Viewing profile."""

from __future__ import annotations

import importlib.util
import re
import socket
import subprocess
import sys
import tempfile
import time
import unittest
from dataclasses import dataclass
from pathlib import Path

import launcher_click
from qemu_probe_support import (
    AcceptanceTimeline, Com1Observer, Com2Collector, RunnerCapture, SerialTail,
    cleanup_runner_process, connect_com2, spawn_runner_process,
)


ROOT = Path(__file__).resolve().parents[1]
TARGET_DIR = ROOT / "target/session-viewing-probe"
SHELL_PORT = 4593
WAIT_TIMEOUT = 15.0
QEMU_TIMEOUT = 60.0
USER_PREFIX = "PYTHOS:USER:SESSION_VIEWING:"
CORE_PREFIX = "PYTHOS:CORE:SESSION_VIEWING:"
DRAWN = (
    CORE_PREFIX + "DRAWN revision:0 active:0 x:0 y:0",
    CORE_PREFIX + "DRAWN revision:1 active:0 x:0 y:0",
    CORE_PREFIX + "DRAWN revision:2 active:0 x:0 y:0",
    CORE_PREFIX + "DRAWN revision:3 active:0 x:0 y:0",
    CORE_PREFIX + "DRAWN revision:4 active:0 x:0 y:0",
    CORE_PREFIX + "DRAWN revision:5 active:1 x:320 y:240",
    CORE_PREFIX + "DRAWN revision:6 active:1 x:327 y:233",
    CORE_PREFIX + "DRAWN revision:7 active:1 x:327 y:233",
)
EXPECTED_COM1 = (
    "PYTHOS:CORE:SESSION_RUNTIME:COM2_READY",
    "PYTHOS:CORE:SESSION_RUNTIME:AUTHORITY_CREATED",
    "PYTHOS:CORE:SESSION_RUNTIME:IDENTITIES_VALID",
    "PYTHOS:CORE:SESSION_RUNTIME:STREAM_BOUND",
    CORE_PREFIX + "PRESENTATION_BOUND",
    CORE_PREFIX + "FRAMEBUFFER_ISOLATED",
    "PYTHOS:CORE:SESSION_RUNTIME:PS2_READY",
    "PYTHOS:CORE:SESSION_RUNTIME:RING3_ENTER",
    DRAWN[0],
    "PYTHOS:CORE:PS2:MOUSE_IRQ_FIRED",
    DRAWN[1],
    "PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED",
    *DRAWN[2:],
    CORE_PREFIX + "RING3_RETURN",
    CORE_PREFIX + "INVOCATION_1_VALID",
    CORE_PREFIX + "REINVOKE_VALID",
    CORE_PREFIX + "INVOCATION_2_VALID",
    CORE_PREFIX + "STATE_RETENTION_VALID",
    CORE_PREFIX + "NO_DISK_WRITES",
    CORE_PREFIX + "READY",
)
EXPECTED_COM2 = tuple(USER_PREFIX + suffix for suffix in (
    "BOOT_STATE_0", "DRAWN:REV:0", "WAIT_EVENT:0",
    "MOTION:TRAVERSAL", "DRAWN:REV:1", "WAIT_EVENT:1",
    "DRAWN:REV:2", "WAIT_EVENT:2",
    "DRAWN:REV:3", "INVOCATION:1:VALID", "WAIT_EVENT:3",
    "DRAWN:REV:4", "WAIT_EVENT:4",
    "ACTIVATED", "DRAWN:REV:5", "WAIT_EVENT:5",
    "MOTION:FOCUS_MARK", "DRAWN:REV:6", "INVOCATION:2:VALID", "WAIT_EVENT:6",
    "DRAWN:REV:7", "COMPLETE",
))


def load_script(name: str, filename: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {filename}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


# Reuse characterized transport, PPM parsing, image identity and tree-reaping
# mechanisms. The new profile owns its own independent acceptance contract.
RUNTIME = load_script("viewing_runtime_harness_support", "test-session-runtime-probe.py")
VIEWING = load_script("viewing_pixel_harness_support", "test-viewing-input-probe.py")
QEMU = VIEWING.QEMU_RUNNER


@dataclass(frozen=True)
class BootEvidence:
    com1: str
    com2: str
    runner_output: str
    timeline: AcceptanceTimeline
    process_tree_reaped: bool
    screenshots: tuple[bytes, ...]


def assert_boot_acceptance(boot: BootEvidence) -> None:
    RUNTIME.assert_exact_ordered_markers(
        boot.com1, EXPECTED_COM1, "COM1",
        (CORE_PREFIX, "PYTHOS:CORE:SESSION_RUNTIME:", "PYTHOS:CORE:PS2:"),
    )
    RUNTIME.assert_exact_ordered_markers(
        boot.com2, EXPECTED_COM2, "COM2", ("PYTHOS:",),
    )
    for text in (boot.com1, boot.com2, boot.runner_output):
        for line in text.splitlines():
            if line in ("PYTHOS:CORE:EXPECTED_PAGE_FAULT", CORE_PREFIX + "NO_DISK_WRITES"):
                continue
            normalized = "_".join(filter(None, re.split(r"[^A-Z0-9]+", line.upper())))
            if any(token in normalized for token in (
                "PANIC", "GAP", "ERROR", "FAULT", "FAILURE", "RECOVERY", "FALLBACK", "DISK_WRITE",
            )):
                raise AssertionError(f"forbidden failure evidence: {line!r}")
    RUNTIME.assert_qemu_success(boot.runner_output)
    if not boot.process_tree_reaped:
        raise AssertionError("runner or child process survived cleanup")
    timeline = boot.timeline
    for revision in range(7):
        dispatch = f"INPUT:{revision}:DISPATCH"
        timeline.assert_before("COM1", DRAWN[revision], "HARNESS", dispatch)
        timeline.assert_before("COM2", USER_PREFIX + f"WAIT_EVENT:{revision}", "HARNESS", dispatch)
        timeline.assert_before("HARNESS", dispatch, "HARNESS", f"INPUT:{revision}:SENT")
        timeline.assert_before("HARNESS", dispatch, "COM1", DRAWN[revision + 1])
        timeline.assert_before("HARNESS", f"INPUT:{revision}:SENT", "COM2", USER_PREFIX + f"DRAWN:REV:{revision + 1}")
    timeline.assert_before("HARNESS", "INPUT:0:DISPATCH", "COM1", "PYTHOS:CORE:PS2:MOUSE_IRQ_FIRED")
    timeline.assert_before("HARNESS", "INPUT:1:DISPATCH", "COM1", "PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED")
    for revision in (0, 5, 6):
        shot = f"SCREENSHOT:{revision}"
        timeline.assert_before("COM1", DRAWN[revision], "HARNESS", shot)
        timeline.assert_before("COM2", USER_PREFIX + f"WAIT_EVENT:{revision}", "HARNESS", shot)
        timeline.assert_before("HARNESS", shot, "HARNESS", f"INPUT:{revision}:DISPATCH")
        timeline.assert_before("HARNESS", shot, "COM1", CORE_PREFIX + "RING3_RETURN")
    if len(boot.screenshots) != 3:
        raise AssertionError("expected exactly three live screenshots")
    for pixels, focus in zip(boot.screenshots, (None, (320, 240), (327, 233)), strict=True):
        assert_viewing_pixels(pixels, focus)


def assert_two_boot_acceptance(boots: tuple[BootEvidence, ...], images: tuple) -> None:
    if len(boots) != 2 or len(images) != 3:
        raise AssertionError("expected two boots and three storage snapshots")
    initial = images[0]
    if initial.size != 16 * 1024 * 1024 or initial.sha256 != RUNTIME.ZEROED_STORAGE_SHA256:
        raise AssertionError("initial image is not the fresh zeroed 16MiB fixture")
    for image in images:
        if image.path_identity != image.retained_identity or image != initial:
            raise AssertionError("storage image or its identity changed")
    for boot in boots:
        assert_boot_acceptance(boot)


def assert_viewing_pixels(data: bytes, focus: tuple[int, int] | None) -> None:
    """Require every pixel in the owned viewport, including cleared old marks."""
    width, height, pixels = VIEWING.parse_ppm(data)
    if width < 640 or height < 480:
        raise AssertionError("screendump cannot contain the 640x480 viewport")
    expected: set[tuple[int, int]] = set()
    if focus is not None:
        x, y = focus
        if not (0 <= x < 640 and 0 <= y < 480):
            raise AssertionError("focus lies outside the viewport")
        for left, top, w, h in (
            (x - 12, y - 12, 6, 2), (x - 12, y - 12, 2, 6),
            (x + 7, y - 12, 6, 2), (x + 11, y - 12, 2, 6),
            (x - 12, y + 11, 6, 2), (x - 12, y + 7, 2, 6),
            (x + 7, y + 11, 6, 2), (x + 11, y + 7, 2, 6),
        ):
            expected.update(VIEWING.clipped_rectangle_pixels(left, top, w, h, 640, 480))
    for y in range(480):
        for x in range(640):
            offset = (y * width + x) * 3
            wanted = b"\xff\x60\xd0" if (x, y) in expected else b"\0\0\0"
            if pixels[offset : offset + 3] != wanted:
                raise AssertionError(f"viewport pixel mismatch at ({x},{y})")


def build_boot_image() -> None:
    runtime = TARGET_DIR / "x86_64-unknown-none/debug/pythos-user-session-runtime"
    kernel = TARGET_DIR / "x86_64-unknown-none/debug/pythcore"
    commands = (
        ["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"],
        ["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none",
         "--target-dir", str(TARGET_DIR), "--features", "session-viewing-probe"],
        ["cargo", "run", "-p", "pythc", "--", "build", "programs/session-manager/main.pyth",
         "-o", "target/pyth-tig/session-manager.tig"],
        ["cargo", "run", "-p", "pyth-tig-tool", "--", "verify", "target/pyth-tig/session-manager.tig"],
        [sys.executable, "scripts/build-user-shell.py"],
        [sys.executable, "scripts/verify-user-elf.py"],
        [sys.executable, "scripts/build-session-runtime.py", "--target-dir", str(TARGET_DIR),
         "--features", "session-viewing"],
        [sys.executable, "scripts/verify-user-elf.py", "--elf", str(runtime)],
        [sys.executable, "scripts/build-image.py", "--kernel", str(kernel),
         "--session-runtime-elf", str(runtime)],
    )
    for command in commands:
        RUNTIME.run(command)


def send_input(index: int, timeline: AcceptanceTimeline) -> None:
    if not 0 <= index <= 6:
        raise AssertionError("input index outside seven-event proof")
    timeline.record("HARNESS", f"INPUT:{index}:DISPATCH")
    if index in (0, 5):
        launcher_click.send_relative_mouse_motion(7, -7)
    else:
        key = {1: "spc", 2: "spc", 3: "backspace", 4: "backspace", 6: "ret"}[index]
        launcher_click.press_qcode_keys([key])
    timeline.record("HARNESS", f"INPUT:{index}:SENT")


def drain_com2(collector: Com2Collector) -> None:
    """Do not accept an unobserved trailing recovery/error after COMPLETE."""
    deadline = time.monotonic() + 5.0
    while time.monotonic() < deadline:
        try:
            chunk = collector.sock.recv(512)
        except socket.timeout:
            continue
        if not chunk:
            if collector.remainder:
                raise AssertionError("truncated trailing COM2 line")
            return
        collector.captured.extend(chunk)
        collector._record_complete_lines(chunk)
    raise AssertionError("COM2 did not close after runner exit")


def run_probe_boot(ordinal: int, artifact_dir: Path, storage_image: Path) -> BootEvidence:
    if ordinal not in (1, 2):
        raise AssertionError("boot ordinal must be 1 or 2")
    serial_log = artifact_dir / f"boot-{ordinal}-com1.log"
    com2_log = artifact_dir / f"boot-{ordinal}-com2.log"
    runner_log = artifact_dir / f"boot-{ordinal}-runner.log"
    if any(path.exists() for path in (serial_log, com2_log, runner_log)):
        raise AssertionError("refusing to overwrite retained boot evidence")
    command = [
        sys.executable, "scripts/run-qemu.py", "--serial-log", str(serial_log),
        "--shell-port", str(SHELL_PORT), "--timeout", str(QEMU_TIMEOUT),
        "--storage-image", str(storage_image), "--success-marker", CORE_PREFIX + "READY",
        "--expect-outcome", "success",
    ]
    kwargs: dict[str, object] = {"cwd": ROOT}
    if sys.platform != "win32":
        kwargs["start_new_session"] = True
    print("+ " + " ".join(command), flush=True)
    runner = spawn_runner_process(command, **kwargs)
    process = runner.process
    timeline = AcceptanceTimeline()
    serial = SerialTail(serial_log, timeline)
    observer = Com1Observer(serial)
    capture = RunnerCapture(process, timeline)
    collector: Com2Collector | None = None
    screenshots: list[bytes] = []
    errors: list[BaseException] = []
    tree_reaped = False
    try:
        capture.start()
        observer.start()
        with connect_com2(SHELL_PORT, 20.0) as com2:
            collector = Com2Collector(com2, timeline)
            observer.wait_for((EXPECTED_COM1[7],), 20.0, process, capture)
            for index in range(7):
                collector.read_until((USER_PREFIX + f"WAIT_EVENT:{index}").encode(), WAIT_TIMEOUT)
                observer.wait_for((DRAWN[index],), WAIT_TIMEOUT, process, capture)
                if index in (0, 5, 6):
                    # The next real input is still withheld. Runtime cannot finish
                    # its seven-event script while this screenshot is requested.
                    if process.poll() is not None or serial.contains_all((CORE_PREFIX + "RING3_RETURN",)):
                        raise AssertionError("session exited before live screenshot")
                    path = artifact_dir / f"boot-{ordinal}-revision-{index}.ppm"
                    QEMU.request_screendump(path.resolve())
                    screenshots.append(path.read_bytes())
                    timeline.record("HARNESS", f"SCREENSHOT:{index}")
                send_input(index, timeline)
            collector.read_until((USER_PREFIX + "COMPLETE").encode(), WAIT_TIMEOUT)
            observer.wait_for(tuple(EXPECTED_COM1[-7:]), WAIT_TIMEOUT, process, capture)
            try:
                process.wait(timeout=15.0)
            except subprocess.TimeoutExpired as error:
                raise AssertionError("runner did not exit after readiness") from error
            if process.returncode != 0:
                raise AssertionError(f"QEMU runner failed with {process.returncode}")
            drain_com2(collector)
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
    com2 = bytes(collector.captured).decode("utf-8", errors="replace") if collector else ""
    com2_log.write_text(com2, encoding="utf-8")
    runner_log.write_text(output, encoding="utf-8")
    # Observer-owned ordering is retained alongside serial and pixel evidence.
    (artifact_dir / f"boot-{ordinal}-timeline.log").write_text(
        "\n".join(f"{source} {value}" for source, value in timeline.events) + "\n", encoding="utf-8"
    )
    if output:
        print(output, end="", flush=True)
    if errors:
        raise AssertionError(
            f"boot {ordinal}: {'; '.join(str(error) for error in errors)}\n"
            f"COM1:\n{com1}\nCOM2:\n{com2}\nrunner:\n{output}"
        ) from errors[0]
    evidence = BootEvidence(com1, com2, output, timeline, tree_reaped, tuple(screenshots))
    assert_boot_acceptance(evidence)
    return evidence


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        suite = unittest.defaultTestLoader.discover(str(ROOT / "tests"), "test_session_viewing_probe.py")
        return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1
    if sys.argv[1:]:
        raise SystemExit("usage: test-session-viewing-probe.py [--self-test]")
    build_boot_image()
    TARGET_DIR.mkdir(parents=True, exist_ok=True)
    artifact_dir = Path(tempfile.mkdtemp(prefix="acceptance-", dir=TARGET_DIR))
    print(f"SESSION_VIEWING_ARTIFACTS {artifact_dir}", flush=True)
    storage = artifact_dir / "session-viewing-store.img"
    RUNTIME.create_zeroed_storage_image(storage)
    with RUNTIME.open_retained_storage_image(storage) as retained:
        images = [RUNTIME.snapshot_image(storage, retained)]
        boots = []
        for ordinal in (1, 2):
            boots.append(run_probe_boot(ordinal, artifact_dir, storage))
            images.append(RUNTIME.snapshot_image(storage, retained))
    assert_two_boot_acceptance(tuple(boots), tuple(images))
    print("SESSION_VIEWING_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
