#!/usr/bin/env python
"""Strict two-boot oracle for the opt-in retained session runtime probe."""

from __future__ import annotations

import hashlib
import os
import sys
import subprocess
import time
import unittest
from dataclasses import dataclass, replace
from pathlib import Path
import re
import tempfile
from unittest import mock

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


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
TARGET_DIR = TARGET / "session-runtime-probe"
STORAGE_IMAGE = TARGET_DIR / "session-runtime-store.img"
COM1_LOGS = (
    TARGET_DIR / "session-runtime-boot-1-com1.log",
    TARGET_DIR / "session-runtime-boot-2-com1.log",
)
COM2_LOGS = (
    TARGET_DIR / "session-runtime-boot-1-com2.log",
    TARGET_DIR / "session-runtime-boot-2-com2.log",
)
SHELL_PORT = 4592
QEMU_TIMEOUT_SECONDS = 45.0
COM2_CONNECT_TIMEOUT_SECONDS = 20.0
COM2_READ_TIMEOUT_SECONDS = 15.0
STORAGE_IMAGE_SIZE = 16 * 1024 * 1024
ZEROED_STORAGE_SHA256 = "080acf35a507ac9849cfcba47dc2ad83e01b75663a516279c8b9d243b719643e"

COM1_MARKERS = (
    "PYTHOS:CORE:SESSION_RUNTIME:COM2_READY",
    "PYTHOS:CORE:SESSION_RUNTIME:AUTHORITY_CREATED",
    "PYTHOS:CORE:SESSION_RUNTIME:IDENTITIES_VALID",
    "PYTHOS:CORE:SESSION_RUNTIME:STREAM_BOUND",
    "PYTHOS:CORE:SESSION_RUNTIME:PS2_READY",
    "PYTHOS:CORE:SESSION_RUNTIME:RING3_ENTER",
    "PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED",
    "PYTHOS:CORE:PS2:MOUSE_IRQ_FIRED",
    "PYTHOS:CORE:SESSION_RUNTIME:RING3_RETURN",
    "PYTHOS:CORE:SESSION_RUNTIME:INVOCATION_1_VALID",
    "PYTHOS:CORE:SESSION_RUNTIME:REINVOKE_VALID",
    "PYTHOS:CORE:SESSION_RUNTIME:INVOCATION_2_VALID",
    "PYTHOS:CORE:SESSION_RUNTIME:STATE_RETENTION_VALID",
    "PYTHOS:CORE:SESSION_RUNTIME:NO_DISK_WRITES",
    "PYTHOS:CORE:SESSION_RUNTIME:READY",
)
COM2_MARKERS = (
    "PYTHOS:SESSION_RUNTIME:BOOT_STATE_0",
    "PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_1",
    "PYTHOS:SESSION_RUNTIME:EVENT_1_KEY_A_SEQUENCE_0",
    "PYTHOS:SESSION_RUNTIME:COMMAND_1_SLICE2_ONE",
    "PYTHOS:SESSION_RUNTIME:RESULT_1_SLICE2_ONE",
    "PYTHOS:SESSION_RUNTIME:INVOCATION_1_EXIT_OK",
    "PYTHOS:SESSION_RUNTIME:STATE_INPUTS_1_INVOCATIONS_1",
    "PYTHOS:SESSION_RUNTIME:INVOCATION_LOCAL_RESET",
    "PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_2",
    "PYTHOS:SESSION_RUNTIME:EVENT_2_RELATIVE_MOTION_DX_7_DY_NEG_7_SEQUENCE_1",
    "PYTHOS:SESSION_RUNTIME:COMMAND_2_SLICE2_TWO",
    "PYTHOS:SESSION_RUNTIME:RESULT_2_SLICE2_TWO",
    "PYTHOS:SESSION_RUNTIME:INVOCATION_2_EXIT_OK",
    "PYTHOS:SESSION_RUNTIME:STATE_INPUTS_2_INVOCATIONS_2",
    "PYTHOS:SESSION_RUNTIME:INPUT_CONTIGUOUS",
    "PYTHOS:SESSION_RUNTIME:SESSION_ID_STABLE",
    "PYTHOS:SESSION_RUNTIME:READY",
)


@dataclass(frozen=True)
class ImageSnapshot:
    size: int
    sha256: str


@dataclass(frozen=True)
class BootEvidence:
    com1: str
    com2: str
    runner_output: str
    timeline: AcceptanceTimeline
    process_tree_reaped: bool


def assert_session_runtime_acceptance(
    boots: tuple[BootEvidence, ...], images: tuple[ImageSnapshot, ...]
) -> None:
    """Accept only two complete boot proofs over one unchanged image."""
    if len(boots) != 2:
        raise AssertionError(f"expected exactly two fresh boots, got {len(boots)}")
    if len(images) != 3:
        raise AssertionError(f"expected initial/after-1/after-2 image snapshots, got {len(images)}")
    for label, snapshot in zip(("initial", "after boot 1", "after boot 2"), images):
        if snapshot.size != STORAGE_IMAGE_SIZE:
            raise AssertionError(
                f"{label}: expected {STORAGE_IMAGE_SIZE} image bytes, got {snapshot.size}"
            )
        if re.fullmatch(r"[0-9a-f]{64}", snapshot.sha256) is None:
            raise AssertionError(f"{label}: malformed lowercase SHA-256 {snapshot.sha256!r}")
    if images[0].sha256 != ZEROED_STORAGE_SHA256:
        raise AssertionError("initial image is not the fresh zeroed 16 MiB fixture")
    if images[0] != images[1] or images[0] != images[2]:
        raise AssertionError(f"storage image changed across boots: {images!r}")

    for ordinal, boot in enumerate(boots, start=1):
        assert_boot_acceptance(boot, ordinal)


def complete_lines(text: str) -> list[str]:
    return text.splitlines()


def assert_exact_ordered_markers(
    transcript: str,
    markers: tuple[str, ...],
    channel: str,
    owned_prefix: str,
) -> None:
    lines = complete_lines(transcript)
    previous = -1
    for marker in markers:
        count = lines.count(marker)
        if count != 1:
            raise AssertionError(
                f"{channel}: expected exactly one {marker!r}, found {count}"
            )
        position = lines.index(marker)
        if position <= previous:
            raise AssertionError(f"{channel}: marker order violation at {marker!r}")
        previous = position
    owned_lines = [line for line in lines if line.startswith(owned_prefix)]
    expected_owned_lines = [marker for marker in markers if marker.startswith(owned_prefix)]
    if owned_lines != expected_owned_lines:
        raise AssertionError(
            f"{channel}: owned marker contract contains extra or malformed lines: {owned_lines!r}"
        )


def assert_no_forbidden_evidence(transcript: str, channel: str) -> None:
    allowed_no_write = COM1_MARKERS[-2]
    for line in complete_lines(transcript):
        upper = line.upper()
        normalized = "_".join(filter(None, re.split(r"[^A-Z0-9]+", upper)))
        forbidden_semantics = (
            "VIEWING",
            "SESSION_CONTROL",
            "CURSOR",
            "FOCUS",
            "PRESENTATION",
        )
        if any(token in normalized for token in forbidden_semantics):
            raise AssertionError(f"{channel}: forbidden semantic evidence {line!r}")
        if line != allowed_no_write and (
            any(token in normalized for token in ("PANIC", "GAP", "ERROR", "FAULT", "FAILURE", "RECOVERY", "FALLBACK"))
            or "DISK_WRITE" in normalized
        ):
            raise AssertionError(f"{channel}: forbidden failure evidence {line!r}")


TIMELINE_EDGES = (
    ("COM1", COM1_MARKERS[5], "COM2", COM2_MARKERS[0]),
    ("COM2", COM2_MARKERS[0], "COM2", COM2_MARKERS[1]),
    ("COM2", COM2_MARKERS[1], "HARNESS", "QMP_A_SENT"),
    ("HARNESS", "QMP_A_SENT", "COM1", COM1_MARKERS[6]),
    ("COM1", COM1_MARKERS[6], "COM2", COM2_MARKERS[2]),
    ("COM2", COM2_MARKERS[2], "COM2", COM2_MARKERS[7]),
    ("COM2", COM2_MARKERS[7], "COM2", COM2_MARKERS[8]),
    ("COM2", COM2_MARKERS[8], "HARNESS", "QMP_MOUSE_SENT"),
    ("HARNESS", "QMP_MOUSE_SENT", "COM1", COM1_MARKERS[7]),
    ("COM1", COM1_MARKERS[7], "COM2", COM2_MARKERS[9]),
    ("COM2", COM2_MARKERS[9], "COM2", COM2_MARKERS[-1]),
    ("COM2", COM2_MARKERS[-1], "COM1", COM1_MARKERS[8]),
    ("COM1", COM1_MARKERS[8], "COM1", COM1_MARKERS[-1]),
    ("COM1", COM1_MARKERS[-1], "RUNNER", "QEMU_OUTCOME success"),
)


def assert_cross_channel_timeline(timeline: AcceptanceTimeline) -> None:
    for before_source, before, after_source, after in TIMELINE_EDGES:
        timeline.assert_before(before_source, before, after_source, after)


def assert_qemu_success(output: str) -> None:
    outcome_lines = [line for line in complete_lines(output) if "QEMU_OUTCOME" in line]
    if outcome_lines != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcome_lines!r}")


def assert_boot_acceptance(boot: BootEvidence, ordinal: int) -> None:
    assert_no_forbidden_evidence(boot.com1, f"boot {ordinal} COM1")
    assert_no_forbidden_evidence(boot.com2, f"boot {ordinal} COM2")
    assert_exact_ordered_markers(
        boot.com1,
        COM1_MARKERS,
        f"boot {ordinal} COM1",
        "PYTHOS:CORE:SESSION_RUNTIME:",
    )
    assert_exact_ordered_markers(
        boot.com2,
        COM2_MARKERS,
        f"boot {ordinal} COM2",
        "PYTHOS:SESSION_RUNTIME:",
    )
    assert_qemu_success(boot.runner_output)
    assert_cross_channel_timeline(boot.timeline)
    if not boot.process_tree_reaped:
        raise AssertionError(f"boot {ordinal}: runner or child process survived cleanup")


def run(command: list[str]) -> None:
    print("+ " + " ".join(command), flush=True)
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout, end="")
    if result.returncode != 0:
        raise AssertionError(
            f"build command failed ({result.returncode}): {' '.join(command)}"
        )


def build_boot_image() -> None:
    runtime = (
        TARGET_DIR
        / "x86_64-unknown-none"
        / "debug"
        / "pythos-user-session-runtime"
    ).resolve()
    kernel = (TARGET_DIR / "x86_64-unknown-none" / "debug" / "pythcore").resolve()
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(
        [
            "cargo",
            "build",
            "-p",
            "pythos-core",
            "--target",
            "x86_64-unknown-none",
            "--target-dir",
            str(TARGET_DIR),
            "--features",
            "session-runtime-probe",
        ]
    )
    run(
        [
            "cargo",
            "run",
            "-p",
            "pythc",
            "--",
            "build",
            "programs/session-manager/main.pyth",
            "-o",
            "target/pyth-tig/session-manager.tig",
        ]
    )
    run(
        [
            "cargo",
            "run",
            "-p",
            "pyth-tig-tool",
            "--",
            "verify",
            "target/pyth-tig/session-manager.tig",
        ]
    )
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run(
        [
            sys.executable,
            "scripts/build-session-runtime.py",
            "--target-dir",
            str(TARGET_DIR),
        ]
    )
    run([sys.executable, "scripts/verify-user-elf.py", "--elf", str(runtime)])
    run(
        [
            sys.executable,
            "scripts/build-image.py",
            "--kernel",
            str(kernel),
            "--session-runtime-elf",
            str(runtime),
        ]
    )


def create_zeroed_storage_image(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as image:
        image.truncate(STORAGE_IMAGE_SIZE)


def snapshot_image(path: Path) -> ImageSnapshot:
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as image:
        for chunk in iter(lambda: image.read(1024 * 1024), b""):
            size += len(chunk)
            digest.update(chunk)
    return ImageSnapshot(size=size, sha256=digest.hexdigest())


def probe_runner_command(boot_ordinal: int) -> list[str]:
    if boot_ordinal not in (1, 2):
        raise AssertionError(f"boot ordinal must be 1 or 2, got {boot_ordinal}")
    return [
        sys.executable,
        "scripts/run-qemu.py",
        "--serial-log",
        str(COM1_LOGS[boot_ordinal - 1]),
        "--shell-port",
        str(SHELL_PORT),
        "--timeout",
        str(QEMU_TIMEOUT_SECONDS),
        "--storage-image",
        str(STORAGE_IMAGE),
        "--success-marker",
        COM1_MARKERS[-1],
        "--expect-outcome",
        "success",
    ]


def runner_tree_is_reaped(process, process_group: int | None, job) -> bool:
    if process.poll() is None:
        return False
    if job is not None:
        return getattr(job, "_handle", None) is None
    if process_group is None or not hasattr(os, "killpg"):
        return True
    try:
        os.killpg(process_group, 0)
    except ProcessLookupError:
        return True
    except PermissionError:
        return False
    return False


def run_probe_boot(boot_ordinal: int) -> BootEvidence:
    serial_log = COM1_LOGS[boot_ordinal - 1]
    com2_log = COM2_LOGS[boot_ordinal - 1]
    serial_log.parent.mkdir(parents=True, exist_ok=True)
    for path in (serial_log, com2_log):
        if path.exists():
            path.unlink()

    popen_kwargs: dict[str, object] = {"cwd": ROOT}
    if sys.platform != "win32":
        popen_kwargs["start_new_session"] = True
    command = probe_runner_command(boot_ordinal)
    print("+ " + " ".join(command), flush=True)
    runner = spawn_runner_process(command, **popen_kwargs)
    process = runner.process
    timeline = AcceptanceTimeline()
    serial = SerialTail(serial_log, timeline)
    observer = Com1Observer(serial)
    capture = RunnerCapture(process, timeline)
    collector: Com2Collector | None = None
    captured_error: BaseException | None = None
    capture.start()
    observer.start()
    try:
        with connect_com2(SHELL_PORT, COM2_CONNECT_TIMEOUT_SECONDS) as com2:
            collector = Com2Collector(com2, timeline)
            observer.wait_for(
                (COM1_MARKERS[0], COM1_MARKERS[5]),
                COM2_CONNECT_TIMEOUT_SECONDS,
                process,
                capture,
            )
            collector.read_until(COM2_MARKERS[1].encode(), COM2_READ_TIMEOUT_SECONDS)

            timeline.record("HARNESS", "QMP_A_SENT")
            launcher_click.press_qcode_keys(["a"])
            observer.wait_for(
                (COM1_MARKERS[6],), COM2_READ_TIMEOUT_SECONDS, process, capture
            )
            collector.read_until(COM2_MARKERS[7].encode(), COM2_READ_TIMEOUT_SECONDS)
            collector.read_until(COM2_MARKERS[8].encode(), COM2_READ_TIMEOUT_SECONDS)

            timeline.record("HARNESS", "QMP_MOUSE_SENT")
            launcher_click.send_relative_mouse_motion(7, -7)
            observer.wait_for(
                (COM1_MARKERS[7],), COM2_READ_TIMEOUT_SECONDS, process, capture
            )
            collector.read_until(COM2_MARKERS[-1].encode(), COM2_READ_TIMEOUT_SECONDS)
            observer.wait_for(
                tuple(COM1_MARKERS[8:]), COM2_READ_TIMEOUT_SECONDS, process, capture
            )

        deadline = time.monotonic() + QEMU_TIMEOUT_SECONDS + 5.0
        while process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.05)
        if process.poll() is None:
            raise AssertionError("QEMU runner did not terminate after runtime readiness")
        if process.returncode != 0:
            raise AssertionError(f"QEMU runner failed with {process.returncode}")
    except BaseException as error:
        captured_error = error
    finally:
        try:
            observer.stop_join()
        except BaseException as error:
            if captured_error is None:
                captured_error = error
        try:
            cleanup_runner_process(runner)
        except BaseException as error:
            if captured_error is None:
                captured_error = error

    qemu_output = capture.finish()
    if qemu_output:
        print(qemu_output, end="")
    com1 = serial.transcript()
    com2 = (
        bytes(collector.captured).decode("utf-8", errors="replace")
        if collector is not None
        else ""
    )
    com2_log.write_text(com2, encoding="utf-8")
    tree_reaped = runner_tree_is_reaped(
        process, runner.process_group, runner.job
    )
    if captured_error is not None:
        raise AssertionError(
            f"boot {boot_ordinal}: {captured_error}\n"
            f"COM1:\n{com1}\nCOM2:\n{com2}\nrunner output:\n{qemu_output}"
        ) from captured_error

    evidence = BootEvidence(
        com1=com1,
        com2=com2,
        runner_output=qemu_output,
        timeline=timeline,
        process_tree_reaped=tree_reaped,
    )
    assert_boot_acceptance(evidence, boot_ordinal)
    return evidence


class SessionRuntimeOracleSelfTest(unittest.TestCase):
    def valid_timeline(self) -> AcceptanceTimeline:
        timeline = AcceptanceTimeline()
        timeline.record("COM1", COM1_MARKERS[5])
        timeline.record("COM2", COM2_MARKERS[0])
        timeline.record("COM2", COM2_MARKERS[1])
        timeline.record("HARNESS", "QMP_A_SENT")
        timeline.record("COM1", COM1_MARKERS[6])
        for marker in COM2_MARKERS[2:8]:
            timeline.record("COM2", marker)
        timeline.record("COM2", COM2_MARKERS[8])
        timeline.record("HARNESS", "QMP_MOUSE_SENT")
        timeline.record("COM1", COM1_MARKERS[7])
        for marker in COM2_MARKERS[9:]:
            timeline.record("COM2", marker)
        for marker in COM1_MARKERS[8:]:
            timeline.record("COM1", marker)
        timeline.record("RUNNER", "QEMU_OUTCOME success")
        return timeline

    def valid_boot(self) -> BootEvidence:
        return BootEvidence(
            com1="\n".join(COM1_MARKERS),
            com2="\n".join(COM2_MARKERS),
            runner_output="QEMU_OUTCOME success\n",
            timeline=self.valid_timeline(),
            process_tree_reaped=True,
        )

    def valid_evidence(
        self,
    ) -> tuple[tuple[BootEvidence, ...], tuple[ImageSnapshot, ...]]:
        snapshot = ImageSnapshot(STORAGE_IMAGE_SIZE, ZEROED_STORAGE_SHA256)
        return (self.valid_boot(), self.valid_boot()), (snapshot, snapshot, snapshot)

    def assert_rejected(
        self,
        boots: tuple[BootEvidence, ...],
        images: tuple[ImageSnapshot, ...] | None = None,
    ) -> None:
        if images is None:
            images = self.valid_evidence()[1]
        with self.assertRaises(AssertionError):
            assert_session_runtime_acceptance(boots, images)

    @staticmethod
    def replace_boot(
        boots: tuple[BootEvidence, ...], ordinal: int, **changes: object
    ) -> tuple[BootEvidence, ...]:
        updated = list(boots)
        updated[ordinal] = replace(updated[ordinal], **changes)
        return tuple(updated)

    @staticmethod
    def swap_markers(markers: tuple[str, ...], left: int, right: int) -> str:
        reordered = list(markers)
        reordered[left], reordered[right] = reordered[right], reordered[left]
        return "\n".join(reordered)

    def test_valid_two_boot_transcript_passes(self) -> None:
        assert_session_runtime_acceptance(*self.valid_evidence())

    def test_requires_exactly_two_boots_and_three_image_snapshots(self) -> None:
        boots, images = self.valid_evidence()
        for mutation in (boots[:1], boots + (self.valid_boot(),)):
            with self.subTest(boot_count=len(mutation)):
                self.assert_rejected(mutation, images)
        for mutation in (images[:2], images + (images[0],)):
            with self.subTest(snapshot_count=len(mutation)):
                self.assert_rejected(boots, mutation)

    def test_every_com1_marker_is_required_exact_once_well_formed_and_ordered(self) -> None:
        boots, images = self.valid_evidence()
        for boot_ordinal in range(2):
            for marker_ordinal, marker in enumerate(COM1_MARKERS):
                with self.subTest(boot=boot_ordinal + 1, mutation="missing", marker=marker):
                    transcript = "\n".join(
                        candidate for candidate in COM1_MARKERS if candidate != marker
                    )
                    self.assert_rejected(
                        self.replace_boot(boots, boot_ordinal, com1=transcript), images
                    )
                with self.subTest(boot=boot_ordinal + 1, mutation="duplicate", marker=marker):
                    self.assert_rejected(
                        self.replace_boot(
                            boots,
                            boot_ordinal,
                            com1="\n".join(COM1_MARKERS) + "\n" + marker,
                        ),
                        images,
                    )
                with self.subTest(boot=boot_ordinal + 1, mutation="malformed", marker=marker):
                    self.assert_rejected(
                        self.replace_boot(
                            boots,
                            boot_ordinal,
                            com1="\n".join(COM1_MARKERS).replace(marker, marker + "_MALFORMED", 1),
                        ),
                        images,
                    )
                if marker_ordinal:
                    with self.subTest(boot=boot_ordinal + 1, mutation="reordered", marker=marker):
                        self.assert_rejected(
                            self.replace_boot(
                                boots,
                                boot_ordinal,
                                com1=self.swap_markers(
                                    COM1_MARKERS, marker_ordinal - 1, marker_ordinal
                                ),
                            ),
                            images,
                        )

    def test_every_com2_marker_is_required_exact_once_well_formed_and_ordered(self) -> None:
        boots, images = self.valid_evidence()
        for boot_ordinal in range(2):
            for marker_ordinal, marker in enumerate(COM2_MARKERS):
                with self.subTest(boot=boot_ordinal + 1, mutation="missing", marker=marker):
                    transcript = "\n".join(
                        candidate for candidate in COM2_MARKERS if candidate != marker
                    )
                    self.assert_rejected(
                        self.replace_boot(boots, boot_ordinal, com2=transcript), images
                    )
                with self.subTest(boot=boot_ordinal + 1, mutation="duplicate", marker=marker):
                    self.assert_rejected(
                        self.replace_boot(
                            boots,
                            boot_ordinal,
                            com2="\n".join(COM2_MARKERS) + "\n" + marker,
                        ),
                        images,
                    )
                with self.subTest(boot=boot_ordinal + 1, mutation="malformed", marker=marker):
                    self.assert_rejected(
                        self.replace_boot(
                            boots,
                            boot_ordinal,
                            com2="\n".join(COM2_MARKERS).replace(marker, marker + "_MALFORMED", 1),
                        ),
                        images,
                    )
                if marker_ordinal:
                    with self.subTest(boot=boot_ordinal + 1, mutation="reordered", marker=marker):
                        self.assert_rejected(
                            self.replace_boot(
                                boots,
                                boot_ordinal,
                                com2=self.swap_markers(
                                    COM2_MARKERS, marker_ordinal - 1, marker_ordinal
                                ),
                            ),
                            images,
                        )

    def test_cross_channel_timeline_rejects_every_reversed_edge(self) -> None:
        edges = (
            ("COM1", COM1_MARKERS[5], "COM2", COM2_MARKERS[0]),
            ("COM2", COM2_MARKERS[0], "COM2", COM2_MARKERS[1]),
            ("COM2", COM2_MARKERS[1], "HARNESS", "QMP_A_SENT"),
            ("HARNESS", "QMP_A_SENT", "COM1", COM1_MARKERS[6]),
            ("COM1", COM1_MARKERS[6], "COM2", COM2_MARKERS[2]),
            ("COM2", COM2_MARKERS[2], "COM2", COM2_MARKERS[7]),
            ("COM2", COM2_MARKERS[7], "COM2", COM2_MARKERS[8]),
            ("COM2", COM2_MARKERS[8], "HARNESS", "QMP_MOUSE_SENT"),
            ("HARNESS", "QMP_MOUSE_SENT", "COM1", COM1_MARKERS[7]),
            ("COM1", COM1_MARKERS[7], "COM2", COM2_MARKERS[9]),
            ("COM2", COM2_MARKERS[9], "COM2", COM2_MARKERS[-1]),
            ("COM2", COM2_MARKERS[-1], "COM1", COM1_MARKERS[8]),
            ("COM1", COM1_MARKERS[8], "COM1", COM1_MARKERS[-1]),
            ("COM1", COM1_MARKERS[-1], "RUNNER", "QEMU_OUTCOME success"),
        )
        _, images = self.valid_evidence()
        for boot_ordinal in range(2):
            for before_source, before, after_source, after in edges:
                with self.subTest(
                    boot=boot_ordinal + 1,
                    before=(before_source, before),
                    after=(after_source, after),
                ):
                    timeline = self.valid_timeline()
                    timeline.move_after(before_source, before, after_source, after)
                    boots = (self.valid_boot(), self.valid_boot())
                    self.assert_rejected(
                        self.replace_boot(boots, boot_ordinal, timeline=timeline), images
                    )

    def test_identity_state_reset_stale_reuse_and_event_mutations_fail(self) -> None:
        boots, images = self.valid_evidence()
        mutations = {
            "changed session identity": (1, COM2_MARKERS[15], "PYTHOS:SESSION_RUNTIME:SESSION_ID_CHANGED"),
            "boot 2 starts above zero": (1, COM2_MARKERS[0], "PYTHOS:SESSION_RUNTIME:BOOT_STATE_1"),
            "state resets between invocations": (
                0,
                COM2_MARKERS[13],
                "PYTHOS:SESSION_RUNTIME:STATE_INPUTS_1_INVOCATIONS_1",
            ),
            "stale command reused": (0, COM2_MARKERS[10], "PYTHOS:SESSION_RUNTIME:COMMAND_2_SLICE2_ONE"),
            "stale result reused": (0, COM2_MARKERS[11], "PYTHOS:SESSION_RUNTIME:RESULT_2_SLICE2_ONE"),
            "wrong event kind": (0, COM2_MARKERS[2], "PYTHOS:SESSION_RUNTIME:EVENT_1_KEY_B_SEQUENCE_0"),
            "wrong event value": (
                0,
                COM2_MARKERS[9],
                "PYTHOS:SESSION_RUNTIME:EVENT_2_RELATIVE_MOTION_DX_8_DY_NEG_7_SEQUENCE_1",
            ),
            "wrong event continuity": (
                0,
                COM2_MARKERS[9],
                "PYTHOS:SESSION_RUNTIME:EVENT_2_RELATIVE_MOTION_DX_7_DY_NEG_7_SEQUENCE_2",
            ),
        }
        for name, (boot_ordinal, original, mutation) in mutations.items():
            with self.subTest(mutation=name):
                transcript = boots[boot_ordinal].com2.replace(original, mutation, 1)
                self.assert_rejected(
                    self.replace_boot(boots, boot_ordinal, com2=transcript), images
                )

        event_before_reset = list(COM2_MARKERS)
        event = event_before_reset.pop(9)
        event_before_reset.insert(7, event)
        self.assert_rejected(
            self.replace_boot(boots, 0, com2="\n".join(event_before_reset)), images
        )

    def test_gap_failure_reinvocation_and_second_binding_fail(self) -> None:
        boots, images = self.valid_evidence()
        mutations = (
            ("com2", "PYTHOS:SESSION_RUNTIME:GAP_BEFORE\n" + boots[0].com2),
            (
                "com2",
                boots[0].com2.replace(
                    COM2_MARKERS[5],
                    "PYTHOS:SESSION_RUNTIME:INVOCATION_1_EXIT_RUNTIME_ERROR\n"
                    + COM2_MARKERS[5],
                ),
            ),
            ("com1", boots[0].com1 + "\n" + COM1_MARKERS[3]),
        )
        for channel, transcript in mutations:
            with self.subTest(channel=channel, extra=transcript.splitlines()[-1]):
                self.assert_rejected(
                    self.replace_boot(boots, 0, **{channel: transcript}), images
                )

    def test_one_channel_only_and_runner_replay_cannot_supply_com1(self) -> None:
        boots, images = self.valid_evidence()
        self.assert_rejected(self.replace_boot(boots, 0, com1=""), images)
        self.assert_rejected(self.replace_boot(boots, 0, com2=""), images)
        self.assert_rejected(
            self.replace_boot(
                boots,
                0,
                com1="",
                runner_output="\n".join(COM1_MARKERS) + "\nQEMU_OUTCOME success\n",
            ),
            images,
        )

    def test_viewing_cursor_focus_and_presentation_evidence_fail(self) -> None:
        boots, images = self.valid_evidence()
        forbidden = (
            "PYTHOS:CORE:VIEWING:TRAVERSAL_RELATIVE_MOTION",
            "PYTHOS:CORE:SESSION_CONTROL:CURSOR_ACTIVATED",
            "PYTHOS:CORE:POINTER_CURSOR_READY",
            "PYTHOS:CORE:VIEWING:FOCUS_MARK_READY",
            "PYTHOS:CORE:PRESENTATION:FRAME_READY",
        )
        for marker in forbidden:
            for channel in ("com1", "com2"):
                with self.subTest(marker=marker, channel=channel):
                    transcript = getattr(boots[0], channel) + "\n" + marker
                    self.assert_rejected(
                        self.replace_boot(boots, 0, **{channel: transcript}), images
                    )

    def test_panic_timeout_and_malformed_or_duplicate_outcomes_fail(self) -> None:
        boots, images = self.valid_evidence()
        for channel in ("com1", "com2"):
            transcript = getattr(boots[0], channel) + "\nPYTHOS:PANIC"
            self.assert_rejected(
                self.replace_boot(boots, 0, **{channel: transcript}), images
            )
        for outcome in (
            "",
            "QEMU_OUTCOME timeout\n",
            "QEMU_OUTCOME reset\n",
            "QEMU_OUTCOME success\nQEMU_OUTCOME success\n",
            "prefix QEMU_OUTCOME success suffix\n",
            "QEMU_OUTCOME success suffix\n",
        ):
            with self.subTest(outcome=outcome):
                self.assert_rejected(
                    self.replace_boot(boots, 0, runner_output=outcome), images
                )

    def test_zeroed_image_mutation_truncation_and_replacement_after_either_boot_fail(self) -> None:
        boots, images = self.valid_evidence()
        changed = ImageSnapshot(STORAGE_IMAGE_SIZE, "1" * 64)
        truncated = ImageSnapshot(STORAGE_IMAGE_SIZE - 1, ZEROED_STORAGE_SHA256)
        nonzero_initial = (changed, images[1], images[2])
        mutations = (
            nonzero_initial,
            (images[0], changed, images[2]),
            (images[0], truncated, images[2]),
            (images[0], images[1], changed),
            (images[0], images[1], truncated),
        )
        for mutation in mutations:
            with self.subTest(snapshots=mutation):
                self.assert_rejected(boots, mutation)

    def test_surviving_runner_or_child_process_after_either_boot_fails(self) -> None:
        boots, images = self.valid_evidence()
        for boot_ordinal in range(2):
            with self.subTest(boot=boot_ordinal + 1):
                self.assert_rejected(
                    self.replace_boot(
                        boots, boot_ordinal, process_tree_reaped=False
                    ),
                    images,
                )

    def test_build_order_uses_the_same_absolute_verified_runtime_for_packaging(self) -> None:
        self.assertIn("build_boot_image", globals(), "build_boot_image is not implemented")
        commands: list[list[str]] = []
        module = sys.modules[__name__]
        with mock.patch.object(module, "run", side_effect=commands.append):
            build_boot_image()

        target_dir = ROOT / "target" / "session-runtime-probe"
        runtime = (
            target_dir
            / "x86_64-unknown-none"
            / "debug"
            / "pythos-user-session-runtime"
        ).resolve()
        kernel = (target_dir / "x86_64-unknown-none" / "debug" / "pythcore").resolve()
        self.assertEqual(
            commands,
            [
                ["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"],
                [
                    "cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none",
                    "--target-dir", str(target_dir), "--features", "session-runtime-probe",
                ],
                [
                    "cargo", "run", "-p", "pythc", "--", "build",
                    "programs/session-manager/main.pyth", "-o", "target/pyth-tig/session-manager.tig",
                ],
                [
                    "cargo", "run", "-p", "pyth-tig-tool", "--", "verify",
                    "target/pyth-tig/session-manager.tig",
                ],
                [sys.executable, "scripts/build-user-shell.py"],
                [sys.executable, "scripts/verify-user-elf.py"],
                [
                    sys.executable, "scripts/build-session-runtime.py", "--target-dir", str(target_dir),
                ],
                [sys.executable, "scripts/verify-user-elf.py", "--elf", str(runtime)],
                [
                    sys.executable, "scripts/build-image.py", "--kernel", str(kernel),
                    "--session-runtime-elf", str(runtime),
                ],
            ],
        )

    def test_storage_fixture_is_fresh_zeroed_and_hashed_in_chunks(self) -> None:
        self.assertIn("create_zeroed_storage_image", globals(), "storage creator is not implemented")
        self.assertIn("snapshot_image", globals(), "chunked hashing is not implemented")
        with tempfile.TemporaryDirectory() as directory:
            image = Path(directory) / "session-runtime-store.img"
            image.write_bytes(b"stale")
            create_zeroed_storage_image(image)
            self.assertEqual(
                snapshot_image(image),
                ImageSnapshot(STORAGE_IMAGE_SIZE, ZEROED_STORAGE_SHA256),
            )

    def test_two_runner_commands_use_distinct_com1_logs_and_the_same_storage_image(self) -> None:
        self.assertIn("probe_runner_command", globals(), "probe runner command is not implemented")
        first = probe_runner_command(1)
        second = probe_runner_command(2)
        first_log = first[first.index("--serial-log") + 1]
        second_log = second[second.index("--serial-log") + 1]
        self.assertNotEqual(first_log, second_log)
        self.assertEqual(
            first[first.index("--storage-image") + 1],
            second[second.index("--storage-image") + 1],
        )
        self.assertEqual(first[-2:], ["--expect-outcome", "success"])
        self.assertEqual(second[-2:], ["--expect-outcome", "success"])

    def test_cleanup_verifier_rejects_a_live_runner(self) -> None:
        self.assertIn("runner_tree_is_reaped", globals(), "cleanup verifier is not implemented")

        class LiveProcess:
            @staticmethod
            def poll() -> None:
                return None

        self.assertFalse(runner_tree_is_reaped(LiveProcess(), None, None))

    def test_shared_cleanup_reaps_runner_and_child_process_tree(self) -> None:
        child_code = "import time; time.sleep(60)"
        parent_code = (
            "import subprocess,sys,time; "
            f"child=subprocess.Popen([sys.executable, '-u', '-c', {child_code!r}]); "
            "print('child pid=' + str(child.pid), flush=True); time.sleep(60)"
        )
        popen_kwargs: dict[str, object] = {}
        if sys.platform != "win32":
            popen_kwargs["start_new_session"] = True
        runner = spawn_runner_process(
            [sys.executable, "-u", "-c", parent_code], **popen_kwargs
        )
        capture = RunnerCapture(runner.process, AcceptanceTimeline())
        capture.start()
        try:
            capture.wait_for("child pid=", 2)
            child_pid = int(capture.text().split("child pid=", 1)[1].splitlines()[0])
            cleanup_runner_process(runner, terminate_timeout=0.2)
            self.assertTrue(
                runner_tree_is_reaped(
                    runner.process, runner.process_group, runner.job
                )
            )
            deadline = time.monotonic() + 2.0
            while self.process_alive(child_pid) and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertFalse(self.process_alive(child_pid), f"surviving child {child_pid}")
            self.assertIn("child pid=", capture.finish(timeout=2))
        finally:
            cleanup_runner_process(runner, terminate_timeout=0.2)

    @staticmethod
    def process_alive(pid: int) -> bool:
        if sys.platform == "win32":
            result = subprocess.run(
                ["tasklist", "/FI", f"PID eq {pid}", "/NH"],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            return str(pid) in result.stdout
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        return True


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(SessionRuntimeOracleSelfTest)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.wasSuccessful():
        print("SESSION_RUNTIME_ORACLE_SELF_TEST_OK")
        return 0
    return 1


def main() -> int:
    build_boot_image()
    create_zeroed_storage_image(STORAGE_IMAGE)
    snapshots = [snapshot_image(STORAGE_IMAGE)]
    print(
        "SESSION_RUNTIME_IMAGE_INITIAL "
        f"size={snapshots[0].size} sha256={snapshots[0].sha256}"
    )

    boots: list[BootEvidence] = []
    for boot_ordinal in (1, 2):
        boots.append(run_probe_boot(boot_ordinal))
        snapshot = snapshot_image(STORAGE_IMAGE)
        snapshots.append(snapshot)
        print(
            f"SESSION_RUNTIME_IMAGE_AFTER_BOOT_{boot_ordinal} "
            f"size={snapshot.size} sha256={snapshot.sha256}"
        )
        if snapshot != snapshots[0]:
            raise AssertionError(
                f"storage image changed after boot {boot_ordinal}: "
                f"initial={snapshots[0]!r}, current={snapshot!r}"
            )
        print(f"SESSION_RUNTIME_BOOT_{boot_ordinal}_PROCESS_TREE_REAPED")

    assert_session_runtime_acceptance(tuple(boots), tuple(snapshots))
    print("SESSION_RUNTIME_PROBE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-session-runtime-probe.py [--self-test]")
    raise SystemExit(main())
