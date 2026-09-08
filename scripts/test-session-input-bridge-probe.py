#!/usr/bin/env python
"""Strict dual-channel acceptance oracle for the opt-in session-input probe."""

from __future__ import annotations

import os
import re
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

import launcher_click
import qemu_probe_support as qemu_support
from qemu_probe_support import (
    AcceptanceTimeline,
    Com1Observer,
    Com2Collector,
    POSIX_SIGKILL,
    RunnerCapture,
    RunnerHandle,
    SerialTail,
    WindowsJob,
    cleanup_posix_process_group,
    cleanup_runner_process,
    connect_com2,
    spawn_runner_process,
)

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "session-input-bridge-probe-com1.log"
SHELL_PORT = 4592
QEMU_TIMEOUT_SECONDS = 45.0
COM2_CONNECT_TIMEOUT_SECONDS = 20.0
COM2_READ_TIMEOUT_SECONDS = 15.0
DRAIN_SECONDS = 0.25

COM1_MARKERS = (
    "PYTHOS:CORE:SESSION_INPUT_BRIDGE:COM2_READY",
    "PYTHOS:CORE:SESSION_INPUT_BRIDGE:STREAM_BOUND",
    "PYTHOS:CORE:SESSION_INPUT_BRIDGE:PS2_READY",
    "PYTHOS:CORE:SESSION_INPUT_BRIDGE:RING3_ENTER",
    "PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED",
    "PYTHOS:CORE:PS2:MOUSE_IRQ_FIRED",
    "PYTHOS:CORE:SESSION_INPUT_BRIDGE:RING3_RETURN",
    "PYTHOS:CORE:SESSION_INPUT_BRIDGE:NO_DISK_WRITES",
    "PYTHOS:CORE:SESSION_INPUT_BRIDGE:READY",
)
COM2_MARKERS = (
    "PYTHOS:SESSION_INPUT_PROBE:READY_FOR_INPUT",
    "PYTHOS:SESSION_INPUT_PROBE:FORGED_DENIED_OUTPUT_UNCHANGED",
    "PYTHOS:SESSION_INPUT_PROBE:EVENT_1_SPACE",
    "PYTHOS:SESSION_INPUT_PROBE:EVENT_2_SPACE",
    "PYTHOS:SESSION_INPUT_PROBE:EVENT_3_BACKSPACE",
    "PYTHOS:SESSION_INPUT_PROBE:EVENT_4_BACKSPACE",
    "PYTHOS:SESSION_INPUT_PROBE:EVENT_5_RELATIVE_MOTION_DX_7_DY_NEG_7",
    "PYTHOS:SESSION_INPUT_PROBE:CONTIGUOUS",
    "PYTHOS:SESSION_INPUT_PROBE:READY",
)

def complete_lines(text: str) -> list[str]:
    return text.splitlines()


def assert_cross_channel_timeline(timeline: AcceptanceTimeline) -> None:
    required_edges = (
        ("COM1", COM1_MARKERS[3], "COM2", COM2_MARKERS[0]),
        ("COM2", COM2_MARKERS[0], "HARNESS", "QMP_INJECTION_STARTED"),
        ("HARNESS", "QMP_INJECTION_STARTED", "COM1", COM1_MARKERS[4]),
        ("HARNESS", "QMP_INJECTION_STARTED", "COM1", COM1_MARKERS[5]),
        ("COM1", COM1_MARKERS[4], "HARNESS", "G_SENT"),
        ("COM1", COM1_MARKERS[5], "HARNESS", "G_SENT"),
        ("HARNESS", "G_SENT", "COM2", COM2_MARKERS[1]),
        ("COM2", COM2_MARKERS[-1], "COM1", COM1_MARKERS[6]),
        ("COM1", COM1_MARKERS[6], "COM1", COM1_MARKERS[-1]),
        ("COM1", COM1_MARKERS[-1], "RUNNER", "QEMU_OUTCOME success"),
        ("COM2", COM2_MARKERS[-1], "RUNNER", "QEMU_OUTCOME success"),
    )
    for before_source, before, after_source, after in required_edges:
        timeline.assert_before(before_source, before, after_source, after)


def assert_exact_ordered_markers(transcript: str, markers: tuple[str, ...], channel: str) -> None:
    lines = complete_lines(transcript)
    previous = -1
    for marker in markers:
        count = lines.count(marker)
        if count != 1:
            raise AssertionError(f"{channel}: expected exactly one {marker!r}, found {count}")
        position = lines.index(marker)
        if position <= previous:
            raise AssertionError(f"{channel}: marker order violation at {marker!r}")
        previous = position


def assert_no_failure_markers(transcript: str, channel: str) -> None:
    for line in complete_lines(transcript):
        if line == COM1_MARKERS[7]:
            continue
        normalized = "_".join(filter(None, re.split(r"[^A-Z0-9]+", line.upper())))
        if any(marker in line for marker in ("GAP", "ERROR", "PYTHOS:PANIC")) or "DISK_WRITE" in normalized:
            raise AssertionError(f"{channel}: forbidden evidence line {line!r}")


def assert_com1_transcript(transcript: str) -> None:
    assert_no_failure_markers(transcript, "COM1")
    assert_exact_ordered_markers(transcript, COM1_MARKERS, "COM1")


def assert_com2_transcript(transcript: str) -> None:
    assert_no_failure_markers(transcript, "COM2")
    assert_exact_ordered_markers(transcript, COM2_MARKERS, "COM2")


def assert_qemu_success(output: str) -> None:
    outcome_lines = [line for line in complete_lines(output) if "QEMU_OUTCOME" in line]
    if outcome_lines != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcome_lines!r}")


def assert_session_input_acceptance(
    com1: str, com2: str, qemu_output: str, timeline: AcceptanceTimeline
) -> None:
    """Accept only the complete, ordered, non-error Slice 1 transcript."""
    assert_com1_transcript(com1)
    assert_com2_transcript(com2)
    assert_qemu_success(qemu_output)
    assert_cross_channel_timeline(timeline)


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
        raise AssertionError(f"build command failed ({result.returncode}): {' '.join(command)}")


def build_boot_image() -> None:
    target_dir = TARGET / "session-input-bridge-probe"
    probe_elf = target_dir / "x86_64-unknown-none" / "debug" / "pythos-user-session-input-probe"
    kernel = target_dir / "x86_64-unknown-none" / "debug" / "pythcore"
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run([
        "cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none",
        "--target-dir", str(target_dir), "--features", "session-input-bridge-probe",
    ])
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-session-input-probe.py", "--target-dir", str(target_dir)])
    run([sys.executable, "scripts/verify-user-elf.py", "--elf", str(probe_elf.resolve())])
    run([
        sys.executable, "scripts/build-image.py", "--kernel", str(kernel.resolve()),
        "--session-input-probe-elf", str(probe_elf.resolve()),
    ])


def probe_runner_command() -> list[str]:
    return [
        sys.executable, "scripts/run-qemu.py", "--serial-log", str(SERIAL_LOG),
        "--shell-port", str(SHELL_PORT), "--timeout", str(QEMU_TIMEOUT_SECONDS),
        "--success-marker", COM1_MARKERS[-1],
        "--expect-outcome", "success",
    ]


def run_probe_boot() -> tuple[str, str, str, AcceptanceTimeline]:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    popen_kwargs: dict[str, object] = {}
    if sys.platform != "win32":
        popen_kwargs["start_new_session"] = True
    command = probe_runner_command()
    print("+ " + " ".join(command), flush=True)
    popen_kwargs["cwd"] = ROOT
    runner = spawn_runner_process(command, **popen_kwargs)
    process = runner.process
    timeline = AcceptanceTimeline()
    serial = SerialTail(SERIAL_LOG, timeline)
    observer = Com1Observer(serial)
    capture = RunnerCapture(process, timeline)
    capture.start()
    observer.start()
    captured_error: BaseException | None = None
    result: tuple[str, str, str, AcceptanceTimeline] | None = None
    try:
        # The wait=off COM2 backend drops writes before a client connects.
        with connect_com2(SHELL_PORT, COM2_CONNECT_TIMEOUT_SECONDS) as com2:
            collector = Com2Collector(com2, timeline)
            observer.wait_for(
                (COM1_MARKERS[0], COM1_MARKERS[3]),
                COM2_CONNECT_TIMEOUT_SECONDS,
                process,
                capture,
            )
            before_input = collector.read_until(
                COM2_MARKERS[0].encode(), COM2_READ_TIMEOUT_SECONDS
            )
            timeline.record("HARNESS", "QMP_INJECTION_STARTED")
            launcher_click.type_session_input_bridge_sequence()
            drain_deadline = time.monotonic() + DRAIN_SECONDS
            while time.monotonic() < drain_deadline:
                if process.poll() is not None:
                    raise AssertionError(f"QEMU exited during input injection: {capture.text()}")
                time.sleep(0.01)
            observer.wait_for(
                (COM1_MARKERS[4], COM1_MARKERS[5]),
                COM2_READ_TIMEOUT_SECONDS,
                process,
                capture,
            )
            timeline.record("HARNESS", "G_SENT")
            com2.sendall(b"G")
            after_input = collector.read_until(
                COM2_MARKERS[-1].encode(), COM2_READ_TIMEOUT_SECONDS
            )
            observer.wait_for(
                (COM1_MARKERS[6], COM1_MARKERS[7], COM1_MARKERS[8]),
                COM2_READ_TIMEOUT_SECONDS,
                process,
                capture,
            )
        runner_deadline = time.monotonic() + QEMU_TIMEOUT_SECONDS + 5
        while process.poll() is None and time.monotonic() < runner_deadline:
            time.sleep(0.05)
        if process.poll() is None:
            raise AssertionError("QEMU runner did not terminate after probe acknowledgement")
        qemu_output = capture.finish()
        if process.returncode != 0:
            raise AssertionError(f"QEMU runner failed with {process.returncode}")
        assert_qemu_success(qemu_output)
        observer.stop_join()
        result = (
            serial.transcript(),
            bytes(collector.captured).decode("utf-8", errors="replace"),
            qemu_output,
            timeline,
        )
    except BaseException as error:
        captured_error = error
    finally:
        try:
            observer.stop_join()
        finally:
            cleanup_runner_process(runner)
    qemu_output = capture.finish()
    if qemu_output:
        print(qemu_output, end="")
    if captured_error is not None:
        raise AssertionError(f"{captured_error}\nrunner output:\n{qemu_output}") from captured_error
    if result is None:
        raise AssertionError("runner ended without an acceptance result")
    return result


class SessionInputBridgeOracleSelfTest(unittest.TestCase):
    def valid_com1(self) -> str:
        return "\n".join(COM1_MARKERS)

    def valid_com2(self) -> str:
        return "\n".join(COM2_MARKERS)

    def valid_timeline(self) -> AcceptanceTimeline:
        timeline = AcceptanceTimeline()
        timeline.record("COM1", COM1_MARKERS[3])
        timeline.record("COM2", COM2_MARKERS[0])
        timeline.record("HARNESS", "QMP_INJECTION_STARTED")
        timeline.record("COM1", COM1_MARKERS[4])
        timeline.record("COM1", COM1_MARKERS[5])
        timeline.record("HARNESS", "G_SENT")
        for marker in COM2_MARKERS[1:]:
            timeline.record("COM2", marker)
        timeline.record("COM1", COM1_MARKERS[6])
        timeline.record("COM1", COM1_MARKERS[7])
        timeline.record("COM1", COM1_MARKERS[8])
        timeline.record("RUNNER", "QEMU_OUTCOME success")
        return timeline

    @staticmethod
    def spawn_runner(code: str) -> tuple[RunnerHandle, RunnerCapture]:
        popen_kwargs: dict[str, object] = {}
        if sys.platform != "win32":
            popen_kwargs["start_new_session"] = True
        runner = spawn_runner_process([sys.executable, "-u", "-c", code], **popen_kwargs)
        capture = RunnerCapture(runner.process, AcceptanceTimeline())
        capture.start()
        return runner, capture

    @staticmethod
    def process_alive(pid: int) -> bool:
        if sys.platform == "win32":
            tasklist = subprocess.run(
                ["tasklist", "/FI", f"PID eq {pid}", "/NH"],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            return str(pid) in tasklist.stdout
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        return True

    def assert_stopped(self, process: subprocess.Popen[str]) -> None:
        deadline = time.monotonic() + 2.0
        while process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertIsNotNone(process.poll())

    @staticmethod
    def move_marker_before(markers: tuple[str, ...], marker: str, before: str) -> str:
        reordered = list(markers)
        reordered.remove(marker)
        reordered.insert(reordered.index(before), marker)
        return "\n".join(reordered)

    def assert_rejected(self, com1: str | None = None, com2: str | None = None, outcome: str = "QEMU_OUTCOME success") -> None:
        with self.assertRaises(AssertionError):
            assert_session_input_acceptance(
                self.valid_com1() if com1 is None else com1,
                self.valid_com2() if com2 is None else com2,
                outcome,
                self.valid_timeline(),
            )

    def test_valid_dual_channel_transcript_passes(self) -> None:
        assert_session_input_acceptance(
            self.valid_com1(), self.valid_com2(), "QEMU_OUTCOME success", self.valid_timeline()
        )

    def test_strict_acceptance_requires_a_timeline(self) -> None:
        with self.assertRaises(TypeError):
            assert_session_input_acceptance(
                self.valid_com1(), self.valid_com2(), "QEMU_OUTCOME success"  # type: ignore[call-arg]
            )

    def test_com2_split_lines_return_only_new_deltas_and_runner_replay_is_not_com1(self) -> None:
        class ChunkSocket:
            def __init__(self, chunks: list[bytes]) -> None:
                self.chunks = chunks
                self.receives = 0

            def settimeout(self, _timeout: float) -> None:
                pass

            def recv(self, _size: int) -> bytes:
                self.receives += 1
                return self.chunks.pop(0)

        timeline = self.valid_timeline()
        # A runner replay is retained as diagnostics, never a second COM1 marker source.
        for marker in COM1_MARKERS:
            timeline.record("RUNNER_STDOUT", marker)
        socket_chunks = [
            COM2_MARKERS[0].encode() + b"\r\n",
            b"\r\n".join(marker.encode() for marker in COM2_MARKERS[1:]) + b"\r\n",
        ]
        socket = ChunkSocket(socket_chunks)
        collector = Com2Collector(socket, AcceptanceTimeline())
        first = collector.read_until(COM2_MARKERS[0].encode(), 1)
        second = collector.read_until(COM2_MARKERS[-1].encode(), 1)
        self.assertEqual(socket.receives, 2)
        self.assertEqual(first, COM2_MARKERS[0].encode() + b"\r\n")
        self.assertNotIn(COM2_MARKERS[0].encode(), second)
        self.assertEqual(bytes(collector.captured).count(COM2_MARKERS[0].encode()), 1)
        assert_session_input_acceptance(
            self.valid_com1(), bytes(collector.captured).decode(), "QEMU_OUTCOME success", timeline
        )

    def test_com2_does_not_accept_a_split_marker_before_its_newline(self) -> None:
        class ChunkSocket:
            def __init__(self) -> None:
                self.chunks = [COM2_MARKERS[0].encode(), b"\r\n"]
                self.receives = 0

            def settimeout(self, _timeout: float) -> None:
                pass

            def recv(self, _size: int) -> bytes:
                self.receives += 1
                return self.chunks.pop(0)

        socket = ChunkSocket()
        collector = Com2Collector(socket, AcceptanceTimeline())
        self.assertEqual(
            collector.read_until(COM2_MARKERS[0].encode(), 1),
            COM2_MARKERS[0].encode() + b"\r\n",
        )
        self.assertEqual(socket.receives, 2)

    def test_com1_event_during_com2_receive_cannot_be_observed_after_terminal_ready(self) -> None:
        timeline = AcceptanceTimeline()
        for source, marker in (
            ("COM1", COM1_MARKERS[3]),
            ("COM2", COM2_MARKERS[0]),
            ("HARNESS", "QMP_INJECTION_STARTED"),
            ("COM1", COM1_MARKERS[4]),
            ("COM1", COM1_MARKERS[5]),
            ("HARNESS", "G_SENT"),
        ):
            timeline.record(source, marker)

        with tempfile.TemporaryDirectory() as directory:
            serial_path = Path(directory) / "com1.log"
            serial_path.write_text("")
            observer = Com1Observer(SerialTail(serial_path, timeline), poll_interval=0.001)
            observer.start()
            try:
                class ReceiveRaceSocket:
                    def __init__(self) -> None:
                        self.reads = 0

                    def settimeout(self, _timeout: float) -> None:
                        pass

                    def recv(self, _size: int) -> bytes:
                        self.reads += 1
                        if self.reads == 1:
                            return COM2_MARKERS[0].encode() + b"\r\n"
                        # This COM1 line becomes readable while recv is blocked.  The
                        # independent observer must record it before recv can return.
                        with serial_path.open("a", encoding="utf-8") as serial_file:
                            serial_file.write(COM1_MARKERS[6] + "\n")
                        observer.wait_for((COM1_MARKERS[6],), 1)
                        return b"\r\n".join(marker.encode() for marker in COM2_MARKERS[1:]) + b"\r\n"

                collector = Com2Collector(ReceiveRaceSocket(), timeline)
                collector.read_until(COM2_MARKERS[0].encode(), 1)
                collector.read_until(COM2_MARKERS[-1].encode(), 1)
                with serial_path.open("a", encoding="utf-8") as serial_file:
                    serial_file.write(COM1_MARKERS[7] + "\n" + COM1_MARKERS[8] + "\n")
                observer.wait_for((COM1_MARKERS[6], COM1_MARKERS[7], COM1_MARKERS[8]), 1)
            finally:
                observer.stop_join()
        timeline.record("RUNNER", "QEMU_OUTCOME success")
        self.assertLess(
            timeline.index("COM1", COM1_MARKERS[6]),
            timeline.index("COM2", COM2_MARKERS[-1]),
        )
        with self.assertRaises(AssertionError):
            assert_session_input_acceptance(
                self.valid_com1(), bytes(collector.captured).decode(), "QEMU_OUTCOME success", timeline
            )

    def test_run_probe_boot_includes_late_complete_com1_failure_before_final_snapshot(self) -> None:
        late_failure = "PYTHOS:PANIC late failure"

        with tempfile.TemporaryDirectory() as directory:
            serial_path = Path(directory) / "com1.log"
            observer_ref: list[Com1Observer] = []

            def append_lines(*lines: str) -> None:
                with serial_path.open("a", encoding="utf-8") as serial_file:
                    serial_file.write("\n".join(lines) + "\n")

            class ExitingProcess:
                def __init__(self) -> None:
                    self.returncode: int | None = None
                    self.ready_to_exit = False
                    self.late_failure_written = False
                    self.timeline: AcceptanceTimeline | None = None

                def poll(self) -> int | None:
                    if not self.ready_to_exit:
                        return None
                    if not self.late_failure_written:
                        self.returncode = 0
                        append_lines(late_failure)
                        self.late_failure_written = True
                    return self.returncode

                def wait(self, timeout: float | None = None) -> int:
                    del timeout
                    self.returncode = 0
                    return 0

            process = ExitingProcess()

            class CompletedRunnerCapture:
                def __init__(self, captured_process: ExitingProcess, timeline: AcceptanceTimeline) -> None:
                    self.process = captured_process
                    self.timeline = timeline
                    self.recorded_outcome = False
                    captured_process.timeline = timeline

                def start(self) -> None:
                    pass

                def text(self) -> str:
                    return "QEMU_OUTCOME success\n"

                def finish(self, timeout: float = 5.0) -> str:
                    del timeout
                    if not self.recorded_outcome:
                        self.timeline.record("RUNNER", "QEMU_OUTCOME success")
                        self.recorded_outcome = True
                    return self.text()

            class ProbeSocket:
                def __init__(self) -> None:
                    self.receives = 0
                    self.writer: threading.Thread | None = None

                def __enter__(self) -> ProbeSocket:
                    return self

                def __exit__(self, *_args: object) -> None:
                    if self.writer is not None:
                        self.writer.join(timeout=1)

                def settimeout(self, _timeout: float) -> None:
                    pass

                def sendall(self, data: bytes) -> None:
                    if data != b"G":
                        raise AssertionError(f"unexpected COM2 acknowledgement {data!r}")

                def recv(self, _size: int) -> bytes:
                    self.receives += 1
                    if self.receives == 1:
                        return COM2_MARKERS[0].encode() + b"\r\n"
                    if self.receives != 2:
                        raise AssertionError("unexpected extra COM2 receive")

                    def write_terminal_com1_after_com2_ready() -> None:
                        assert process.timeline is not None
                        deadline = time.monotonic() + 1
                        while process.timeline.count("COM2", COM2_MARKERS[-1]) != 1:
                            if time.monotonic() >= deadline:
                                raise AssertionError("COM2 terminal marker was not recorded")
                            time.sleep(0.001)
                        append_lines(COM1_MARKERS[6], COM1_MARKERS[7], COM1_MARKERS[8])
                        observer_ref[0].serial.poll()
                        process.ready_to_exit = True

                    self.writer = threading.Thread(target=write_terminal_com1_after_com2_ready)
                    self.writer.start()
                    return b"\r\n".join(marker.encode() for marker in COM2_MARKERS[1:]) + b"\r\n"

            original_observer = Com1Observer

            def create_slow_observer(serial: SerialTail) -> Com1Observer:
                observer = original_observer(serial, poll_interval=60)
                observer_ref.append(observer)
                return observer

            def spawn_completed_runner(_command: list[str], **_kwargs: object) -> RunnerHandle:
                serial_path.write_text("\n".join(COM1_MARKERS[:4]) + "\n", encoding="utf-8")
                return RunnerHandle(process, None, None)  # type: ignore[arg-type]

            def inject_irq_evidence() -> None:
                append_lines(COM1_MARKERS[4], COM1_MARKERS[5])
                observer_ref[0].serial.poll()

            module = sys.modules[__name__]
            with (
                mock.patch.object(module, "SERIAL_LOG", serial_path),
                mock.patch.object(module, "DRAIN_SECONDS", 0),
                mock.patch.object(module, "Com1Observer", create_slow_observer),
                mock.patch.object(module, "RunnerCapture", CompletedRunnerCapture),
                mock.patch.object(module, "spawn_runner_process", spawn_completed_runner),
                mock.patch.object(module, "connect_com2", return_value=ProbeSocket()),
                mock.patch.object(
                    launcher_click,
                    "type_session_input_bridge_sequence",
                    side_effect=inject_irq_evidence,
                ),
            ):
                com1, com2, qemu_output, timeline = run_probe_boot()

        self.assertTrue(process.late_failure_written)
        self.assertIn(late_failure, com1)
        with self.assertRaisesRegex(AssertionError, "forbidden evidence"):
            assert_session_input_acceptance(com1, com2, qemu_output, timeline)

    def test_cross_channel_timeline_edges_reject_every_reversal(self) -> None:
        required_edges = (
            ("COM1", COM1_MARKERS[3], "COM2", COM2_MARKERS[0]),
            ("COM2", COM2_MARKERS[0], "HARNESS", "QMP_INJECTION_STARTED"),
            ("HARNESS", "QMP_INJECTION_STARTED", "COM1", COM1_MARKERS[4]),
            ("HARNESS", "QMP_INJECTION_STARTED", "COM1", COM1_MARKERS[5]),
            ("COM1", COM1_MARKERS[4], "HARNESS", "G_SENT"),
            ("COM1", COM1_MARKERS[5], "HARNESS", "G_SENT"),
            ("HARNESS", "G_SENT", "COM2", COM2_MARKERS[1]),
            ("COM2", COM2_MARKERS[-1], "COM1", COM1_MARKERS[6]),
            ("COM1", COM1_MARKERS[6], "COM1", COM1_MARKERS[-1]),
            ("COM1", COM1_MARKERS[-1], "RUNNER", "QEMU_OUTCOME success"),
            ("COM2", COM2_MARKERS[-1], "RUNNER", "QEMU_OUTCOME success"),
        )
        for before_source, before, after_source, after in required_edges:
            with self.subTest(before=(before_source, before), after=(after_source, after)):
                timeline = self.valid_timeline()
                timeline.move_after(before_source, before, after_source, after)
                with self.assertRaises(AssertionError):
                    assert_session_input_acceptance(
                        self.valid_com1(), self.valid_com2(), "QEMU_OUTCOME success", timeline
                    )

    def test_missing_duplicate_and_out_of_order_markers_fail(self) -> None:
        self.assert_rejected(com1=self.valid_com1().replace(COM1_MARKERS[2] + "\n", "", 1))
        self.assert_rejected(com2=self.valid_com2() + "\n" + COM2_MARKERS[3])
        out_of_order = list(COM1_MARKERS)
        out_of_order[3], out_of_order[4] = out_of_order[4], out_of_order[3]
        self.assert_rejected(com1="\n".join(out_of_order))

    def test_one_channel_only_fails(self) -> None:
        self.assert_rejected(com2="")
        self.assert_rejected(com1="")

    def test_wrong_input_values_and_missing_probe_evidence_fail(self) -> None:
        self.assert_rejected(com2=self.valid_com2().replace("EVENT_3_BACKSPACE", "EVENT_3_SPACE"))
        self.assert_rejected(com2=self.valid_com2().replace("DX_7_DY_NEG_7", "DX_8_DY_NEG_7"))
        self.assert_rejected(com2=self.valid_com2().replace(COM2_MARKERS[1] + "\n", ""))
        self.assert_rejected(com2=self.valid_com2().replace(COM2_MARKERS[-2] + "\n", ""))

    def test_forbidden_markers_and_outcome_mutations_fail(self) -> None:
        for marker in (
            "PYTHOS:SESSION_INPUT_PROBE:GAP",
            "PYTHOS:SESSION_INPUT_PROBE:ERROR",
            "PYTHOS:PANIC",
            "PYTHOS:CORE:DISK_WRITE_ATTEMPT",
            "PYTHOS:CORE:HARDWARE_PROBE:DISK_WRITE_TEST_ARMED",
            "PYTHOS:CORE:DISK:WRITE",
            "PYTHOS:CORE:DISK-WRITE",
        ):
            with self.subTest(marker=marker):
                self.assert_rejected(com2=self.valid_com2() + "\n" + marker)
        self.assert_rejected(outcome="QEMU_OUTCOME timeout")
        self.assert_rejected(outcome="")
        self.assert_rejected(outcome="QEMU_OUTCOME success\nQEMU_OUTCOME success")
        self.assert_rejected(outcome="prefix QEMU_OUTCOME success suffix")
        self.assert_rejected(outcome="QEMU_OUTCOME success suffix")

    def test_terminal_readiness_cannot_precede_required_return_or_event_five(self) -> None:
        self.assert_rejected(
            com1=self.move_marker_before(
                COM1_MARKERS, COM1_MARKERS[-1], COM1_MARKERS[6]
            )
        )
        self.assert_rejected(
            com2=self.move_marker_before(
                COM2_MARKERS, COM2_MARKERS[-1], COM2_MARKERS[6]
            )
        )

    def test_build_order_uses_the_same_absolute_verified_probe_for_packaging(self) -> None:
        commands: list[list[str]] = []
        original_run = globals()["run"]
        try:
            globals()["run"] = lambda command: commands.append(command)
            build_boot_image()
        finally:
            globals()["run"] = original_run

        target_dir = TARGET / "session-input-bridge-probe"
        probe = (target_dir / "x86_64-unknown-none" / "debug" / "pythos-user-session-input-probe").resolve()
        kernel = (target_dir / "x86_64-unknown-none" / "debug" / "pythcore").resolve()
        self.assertEqual(commands[0], ["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
        self.assertEqual(commands[1][-4:], ["--target-dir", str(target_dir), "--features", "session-input-bridge-probe"])
        self.assertEqual(commands[2], [sys.executable, "scripts/build-user-shell.py"])
        self.assertEqual(commands[3], [sys.executable, "scripts/verify-user-elf.py"])
        self.assertEqual(commands[4], [sys.executable, "scripts/build-session-input-probe.py", "--target-dir", str(target_dir)])
        self.assertEqual(commands[5], [sys.executable, "scripts/verify-user-elf.py", "--elf", str(probe)])
        self.assertEqual(commands[6], [sys.executable, "scripts/build-image.py", "--kernel", str(kernel), "--session-input-probe-elf", str(probe)])

    def test_probe_runner_uses_its_own_terminal_readiness_as_success_marker(self) -> None:
        command = probe_runner_command()
        marker_index = command.index("--success-marker")
        self.assertEqual(command[marker_index + 1], COM1_MARKERS[-1])
        self.assertEqual(command[-2:], ["--expect-outcome", "success"])

    def test_runner_capture_reaps_success_and_retains_diagnostics(self) -> None:
        runner, capture = self.spawn_runner("print('runner success diagnostic', flush=True)")
        process = runner.process
        try:
            process.wait(timeout=2)
            cleanup_runner_process(runner, terminate_timeout=0.2)
            self.assertIn("runner success diagnostic", capture.finish(timeout=2))
            self.assertIsNotNone(process.poll())
        finally:
            cleanup_runner_process(runner, terminate_timeout=0.2)

    def test_runner_capture_retains_com2_qmp_failure_diagnostics_after_tree_cleanup(self) -> None:
        runner, capture = self.spawn_runner(
            "import time; print('QMP failed after COM2 connect', flush=True); time.sleep(60)"
        )
        process = runner.process
        try:
            capture.wait_for("QMP failed after COM2 connect", 2)
            cleanup_runner_process(runner, terminate_timeout=0.2)
            self.assert_stopped(process)
            self.assertIn("QMP failed after COM2 connect", capture.finish(timeout=2))
        finally:
            cleanup_runner_process(runner, terminate_timeout=0.2)

    def test_timeout_cleanup_kills_runner_group_and_child(self) -> None:
        child_code = (
            "import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); "
            "print('child alive', flush=True); time.sleep(60)"
        )
        parent_code = (
            "import subprocess,sys,time; "
            f"child=subprocess.Popen([sys.executable, '-u', '-c', {child_code!r}]); "
            "print('timeout child pid=' + str(child.pid), flush=True); time.sleep(60)"
        )
        runner, capture = self.spawn_runner(parent_code)
        process = runner.process
        try:
            capture.wait_for("timeout child pid=", 2)
            child_pid = int(capture.text().split("timeout child pid=", 1)[1].splitlines()[0])
            cleanup_runner_process(runner, terminate_timeout=0.2)
            self.assert_stopped(process)
            deadline = time.monotonic() + 2.0
            while self.process_alive(child_pid) and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertFalse(self.process_alive(child_pid), f"surviving runner child {child_pid}")
            self.assertIn("timeout child pid=", capture.finish(timeout=2))
        finally:
            cleanup_runner_process(runner, terminate_timeout=0.2)

    def test_cleanup_uses_stored_posix_group_after_parent_exit(self) -> None:
        class ExitedProcess:
            def poll(self):
                return 0

            def wait(self, timeout: float):
                self.timeout = timeout
                return 0

        calls: list[tuple[int, int]] = []
        had_killpg = hasattr(os, "killpg")
        original_killpg = getattr(os, "killpg", None)
        try:
            os.killpg = lambda pgid, sig: calls.append((pgid, sig))
            cleanup_posix_process_group(ExitedProcess(), 4242, 0.1)
        finally:
            if had_killpg:
                os.killpg = original_killpg
            else:
                del os.killpg
        self.assertEqual(calls, [(4242, signal.SIGTERM), (4242, POSIX_SIGKILL)])

    def test_cleanup_posix_timeout_escalates_and_reaps_after_sigkill(self) -> None:
        class TimeoutThenExitedProcess:
            def __init__(self) -> None:
                self.waits = 0

            def wait(self, timeout: float):
                self.waits += 1
                if self.waits == 1:
                    raise subprocess.TimeoutExpired(["runner"], timeout)
                return 0

        process = TimeoutThenExitedProcess()
        calls: list[tuple[int, int]] = []
        had_killpg = hasattr(os, "killpg")
        original_killpg = getattr(os, "killpg", None)
        try:
            os.killpg = lambda pgid, sig: calls.append((pgid, sig))
            cleanup_posix_process_group(process, 4242, 0.1)
        finally:
            if had_killpg:
                os.killpg = original_killpg
            else:
                del os.killpg
        self.assertEqual(calls, [(4242, signal.SIGTERM), (4242, POSIX_SIGKILL)])
        self.assertEqual(process.waits, 2)

    @unittest.skipUnless(sys.platform == "win32", "Windows suspended runner test")
    def test_windows_spawn_suspends_before_job_assignment_then_resumes(self) -> None:
        events: list[object] = []

        class FakeProcess:
            pid = 4242
            _handle = 0x1234

        process = FakeProcess()

        def fake_popen(command: list[str], **kwargs: object) -> FakeProcess:
            events.append(("popen", command, kwargs["creationflags"]))
            return process

        class FakeJob:
            def __init__(self, captured_process: FakeProcess) -> None:
                self.process = captured_process
                events.append(("job", captured_process.pid))

        def fake_resume(captured_process: FakeProcess) -> None:
            events.append(("resume", captured_process.pid))

        original_flags = 0x08000000
        with (
            mock.patch.object(qemu_support.subprocess, "Popen", fake_popen),
            mock.patch.object(qemu_support, "WindowsJob", FakeJob),
            mock.patch.object(
                qemu_support, "_resume_windows_process", fake_resume, create=True
            ),
        ):
            runner = qemu_support.spawn_runner_process(
                ["runner.exe"], creationflags=original_flags
            )

        self.assertIs(runner.process, process)
        self.assertIs(runner.job.process, process)
        self.assertEqual(
            events,
            [
                ("popen", ["runner.exe"], original_flags | 0x00000004),
                ("job", 4242),
                ("resume", 4242),
            ],
        )

    @unittest.skipUnless(sys.platform == "win32", "Windows suspended runner test")
    def test_windows_job_assignment_failure_kills_and_reaps_suspended_runner(self) -> None:
        events: list[str] = []

        class FakeProcess:
            pid = 4242
            _handle = 0x1234

            def kill(self) -> None:
                events.append("kill")

            def wait(self, timeout: float) -> int:
                self.timeout = timeout
                events.append("wait")
                return 1

        process = FakeProcess()

        def fake_popen(_command: list[str], **_kwargs: object) -> FakeProcess:
            events.append("popen")
            return process

        class FailingJob:
            def __init__(self, _process: FakeProcess) -> None:
                events.append("job")
                raise OSError("job assignment failed")

        with (
            mock.patch.object(qemu_support.subprocess, "Popen", fake_popen),
            mock.patch.object(qemu_support, "WindowsJob", FailingJob),
            mock.patch.object(
                qemu_support,
                "_resume_windows_process",
                side_effect=AssertionError("resume must not run"),
                create=True,
            ),
            self.assertRaisesRegex(OSError, "job assignment failed"),
        ):
            qemu_support.spawn_runner_process(["runner.exe"])

        self.assertEqual(events, ["popen", "job", "kill", "wait"])

    @unittest.skipUnless(sys.platform == "win32", "Windows suspended runner test")
    def test_windows_thread_or_resume_failure_terminates_job_reaps_and_closes(self) -> None:
        for failure in ("thread discovery", "thread open", "thread resume"):
            with self.subTest(failure=failure):
                events: list[str] = []

                class FakeProcess:
                    pid = 4242
                    _handle = 0x1234

                    def kill(self) -> None:
                        raise AssertionError("assigned process must be killed through its job")

                    def wait(self, timeout: float) -> int:
                        self.timeout = timeout
                        events.append("wait")
                        return 1

                process = FakeProcess()

                def fake_popen(_command: list[str], **_kwargs: object) -> FakeProcess:
                    events.append("popen")
                    return process

                class FakeJob:
                    def __init__(self, _process: FakeProcess) -> None:
                        events.append("job")

                    def terminate(self) -> None:
                        events.append("terminate")

                    def close(self) -> None:
                        events.append("close")

                def fail_resume(_process: FakeProcess) -> None:
                    events.append(failure)
                    raise OSError(failure)

                with (
                    mock.patch.object(qemu_support.subprocess, "Popen", fake_popen),
                    mock.patch.object(qemu_support, "WindowsJob", FakeJob),
                    mock.patch.object(
                        qemu_support, "_resume_windows_process", fail_resume, create=True
                    ),
                    self.assertRaisesRegex(OSError, failure),
                ):
                    qemu_support.spawn_runner_process(["runner.exe"])

                self.assertEqual(
                    events,
                    ["popen", "job", failure, "terminate", "wait", "close"],
                )

    @unittest.skipUnless(sys.platform == "win32", "Windows suspended runner test")
    def test_windows_resume_requires_exactly_one_prior_suspend(self) -> None:
        resume_windows_process = getattr(qemu_support, "_resume_windows_process", None)
        self.assertIsNotNone(
            resume_windows_process, "spawn must resume the assigned suspended runner"
        )
        for prior_count in (0, 2, 0xFFFFFFFF):
            with self.subTest(prior_count=prior_count):
                closed: list[int] = []
                with (
                    mock.patch.object(
                        qemu_support,
                        "_find_sole_windows_thread_id",
                        return_value=8181,
                        create=True,
                    ),
                    mock.patch.object(
                        qemu_support,
                        "_open_windows_thread",
                        return_value=0xCAFE,
                        create=True,
                    ),
                    mock.patch.object(
                        qemu_support,
                        "_resume_windows_thread",
                        return_value=prior_count,
                        create=True,
                    ),
                    mock.patch.object(
                        qemu_support,
                        "_close_windows_handle",
                        side_effect=closed.append,
                        create=True,
                    ),
                    self.assertRaises((AssertionError, OSError)),
                ):
                    resume_windows_process(mock.Mock(pid=4242))
                self.assertEqual(closed, [0xCAFE])

    @unittest.skipUnless(sys.platform == "win32", "Windows Job Object test")
    @staticmethod
    def cleanup_windows_adversarial_resources(
        *,
        runner: RunnerHandle | None,
        child_handle: int | None,
        fallback_process: object | None,
        fallback_handle: int | None,
        kernel32: object,
    ) -> list[str]:
        """Best-effort cleanup with every failure deferred until all resources close."""
        failures: list[str] = []

        if runner is not None:
            try:
                cleanup_runner_process(runner, terminate_timeout=2.0)
            except BaseException as error:
                failures.append(f"runner cleanup raised {error!r}")

                runner_running = True
                try:
                    runner_running = runner.process.poll() is None
                except BaseException as fallback_error:
                    failures.append(
                        f"runner fallback poll raised {fallback_error!r}"
                    )

                if runner_running:
                    try:
                        runner.process.kill()
                    except BaseException as fallback_error:
                        failures.append(
                            f"runner fallback kill raised {fallback_error!r}"
                        )

                try:
                    runner.process.wait(timeout=2.0)
                except BaseException as fallback_error:
                    failures.append(
                        f"runner fallback wait raised {fallback_error!r}"
                    )

                if runner.job is not None:
                    try:
                        runner.job.close()
                    except BaseException as fallback_error:
                        failures.append(
                            f"runner fallback job close raised {fallback_error!r}"
                        )

        if child_handle:
            wait_result: int | None = None
            try:
                wait_result = int(kernel32.WaitForSingleObject(child_handle, 0))
            except BaseException as error:
                failures.append(f"child initial WaitForSingleObject raised {error!r}")

            if wait_result != 0:
                try:
                    terminated = bool(kernel32.TerminateProcess(child_handle, 1))
                except BaseException as error:
                    failures.append(f"child TerminateProcess raised {error!r}")
                else:
                    if not terminated:
                        failures.append("child TerminateProcess returned false")

                try:
                    wait_result = int(kernel32.WaitForSingleObject(child_handle, 2000))
                except BaseException as error:
                    failures.append(f"child WaitForSingleObject raised {error!r}")
                else:
                    if wait_result != 0:
                        failures.append(
                            f"child WaitForSingleObject returned {wait_result:#010x}"
                        )

            try:
                closed = bool(kernel32.CloseHandle(child_handle))
            except BaseException as error:
                failures.append(f"child CloseHandle raised {error!r}")
            else:
                if not closed:
                    failures.append("child CloseHandle returned false")

        if fallback_process is not None:
            fallback_running = True
            try:
                fallback_running = fallback_process.poll() is None
            except BaseException as error:
                failures.append(f"fallback process poll raised {error!r}")
            if fallback_running:
                try:
                    fallback_process.kill()
                except BaseException as error:
                    failures.append(f"fallback process kill raised {error!r}")
            try:
                fallback_process.wait(timeout=2.0)
            except BaseException as error:
                failures.append(f"fallback process wait raised {error!r}")

        if fallback_handle:
            try:
                closed = bool(kernel32.CloseHandle(fallback_handle))
            except BaseException as error:
                failures.append(f"fallback CloseHandle raised {error!r}")
            else:
                if not closed:
                    failures.append("fallback CloseHandle returned false")

        return failures

    @unittest.skipUnless(sys.platform == "win32", "Windows Job Object test")
    def test_windows_adversary_cleanup_continues_after_termination_and_wait_failures(
        self,
    ) -> None:
        cleanup = getattr(self, "cleanup_windows_adversarial_resources", None)
        self.assertIsNotNone(
            cleanup,
            "adversarial cleanup must be isolated so failures can be deferred until every resource is closed",
        )

        events: list[str] = []

        class FakeKernel32:
            wait_timeout = 0x00000102
            wait_failed = 0xFFFFFFFF

            def WaitForSingleObject(self, handle: int, timeout: int) -> int:
                events.append(f"wait:{handle}:{timeout}")
                if timeout == 0:
                    return self.wait_timeout
                return self.wait_failed

            def TerminateProcess(self, handle: int, _exit_code: int) -> bool:
                events.append(f"terminate:{handle}")
                return False

            def CloseHandle(self, handle: int) -> bool:
                events.append(f"close:{handle}")
                return True

        class FakeFallbackProcess:
            def poll(self) -> None:
                events.append("fallback:poll")
                return None

            def kill(self) -> None:
                events.append("fallback:kill")

            def wait(self, timeout: float) -> int:
                events.append(f"fallback:wait:{timeout}")
                return 1

        class FakeRunnerProcess:
            def poll(self) -> None:
                events.append("runner:poll")
                return None

            def kill(self) -> None:
                events.append("runner:kill")
                raise OSError("injected runner kill failure")

            def wait(self, timeout: float) -> int:
                events.append(f"runner:wait:{timeout}")
                raise OSError("injected runner wait failure")

        class FakeRunnerJob:
            def close(self) -> None:
                events.append("runner:job:close")
                raise OSError("injected runner job close failure")

        runner = RunnerHandle(FakeRunnerProcess(), None, FakeRunnerJob())

        def fail_runner_cleanup(
            _runner: RunnerHandle, terminate_timeout: float
        ) -> None:
            events.append(f"runner:cleanup:{terminate_timeout}")
            raise OSError("injected runner cleanup failure")

        with mock.patch(
            f"{__name__}.cleanup_runner_process",
            side_effect=fail_runner_cleanup,
        ):
            failures = cleanup(
                runner=runner,
                child_handle=101,
                fallback_process=FakeFallbackProcess(),
                fallback_handle=202,
                kernel32=FakeKernel32(),
            )

        self.assertEqual(
            events,
            [
                "runner:cleanup:2.0",
                "runner:poll",
                "runner:kill",
                "runner:wait:2.0",
                "runner:job:close",
                "wait:101:0",
                "terminate:101",
                "wait:101:2000",
                "close:101",
                "fallback:poll",
                "fallback:kill",
                "fallback:wait:2.0",
                "close:202",
            ],
        )
        self.assertIn(
            "runner cleanup raised OSError('injected runner cleanup failure')",
            failures,
        )
        self.assertIn(
            "runner fallback kill raised OSError('injected runner kill failure')",
            failures,
        )
        self.assertIn(
            "runner fallback wait raised OSError('injected runner wait failure')",
            failures,
        )
        self.assertIn(
            "runner fallback job close raised OSError('injected runner job close failure')",
            failures,
        )
        self.assertIn("child TerminateProcess returned false", failures)
        self.assertIn("child WaitForSingleObject returned 0xffffffff", failures)

    @unittest.skipUnless(sys.platform == "win32", "Windows Job Object test")
    def test_windows_suspended_assignment_contains_immediate_child(self) -> None:
        original_job = qemu_support.WindowsJob
        runner: RunnerHandle | None = None
        child_handle: int | None = None
        fallback_process: subprocess.Popen[str] | None = None
        fallback_handle: int | None = None

        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory) / "runner-child.txt"

            class InspectingJob:
                def __init__(self, process: subprocess.Popen[str]) -> None:
                    deadline = time.monotonic() + 1.0
                    while not evidence.exists() and time.monotonic() < deadline:
                        time.sleep(0.01)
                    self.executed_before_assignment = evidence.exists()
                    self.inner = original_job(process)

                def terminate(self) -> None:
                    self.inner.terminate()

                def close(self) -> None:
                    self.inner.close()

            child_code = "import time; time.sleep(60)"
            runner_code = (
                "import os,pathlib,subprocess,sys,time; "
                f"child=subprocess.Popen([sys.executable, '-u', '-c', {child_code!r}]); "
                f"pathlib.Path({str(evidence)!r}).write_text(str(os.getpid()) + ' ' + str(child.pid)); "
                "time.sleep(60)"
            )

            kernel32 = qemu_support.ctypes.WinDLL("kernel32", use_last_error=True)
            kernel32.OpenProcess.argtypes = [
                qemu_support.wintypes.DWORD,
                qemu_support.wintypes.BOOL,
                qemu_support.wintypes.DWORD,
            ]
            kernel32.OpenProcess.restype = qemu_support.wintypes.HANDLE
            kernel32.IsProcessInJob.argtypes = [
                qemu_support.wintypes.HANDLE,
                qemu_support.wintypes.HANDLE,
                qemu_support.ctypes.POINTER(qemu_support.wintypes.BOOL),
            ]
            kernel32.IsProcessInJob.restype = qemu_support.wintypes.BOOL
            kernel32.WaitForSingleObject.argtypes = [
                qemu_support.wintypes.HANDLE,
                qemu_support.wintypes.DWORD,
            ]
            kernel32.WaitForSingleObject.restype = qemu_support.wintypes.DWORD
            kernel32.TerminateProcess.argtypes = [
                qemu_support.wintypes.HANDLE,
                qemu_support.wintypes.UINT,
            ]
            kernel32.TerminateProcess.restype = qemu_support.wintypes.BOOL
            kernel32.CloseHandle.argtypes = [qemu_support.wintypes.HANDLE]
            kernel32.CloseHandle.restype = qemu_support.wintypes.BOOL
            process_terminate = 0x00000001
            synchronize = 0x00100000
            query_limited_information = 0x00001000
            child_cleanup_access = (
                process_terminate | synchronize | query_limited_information
            )

            captured_error: BaseException | None = None
            cleanup_failures: list[str] = []
            try:
                fallback_process = subprocess.Popen(
                    [sys.executable, "-u", "-c", "import time; time.sleep(60)"],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
                fallback_handle = kernel32.OpenProcess(
                    child_cleanup_access, False, fallback_process.pid
                )
                self.assertTrue(
                    fallback_handle, qemu_support.ctypes.get_last_error()
                )
                qemu_support.ctypes.set_last_error(0)
                terminated = kernel32.TerminateProcess(fallback_handle, 1)
                termination_error = qemu_support.ctypes.get_last_error()
                self.assertTrue(
                    terminated,
                    f"fallback TerminateProcess lacked authorization: Windows error {termination_error}",
                )
                self.assertEqual(kernel32.WaitForSingleObject(fallback_handle, 2000), 0)
                fallback_process.wait(timeout=2.0)

                with mock.patch.object(qemu_support, "WindowsJob", InspectingJob):
                    runner = qemu_support.spawn_runner_process(
                        [sys.executable, "-u", "-c", runner_code]
                    )

                deadline = time.monotonic() + 3.0
                while not evidence.exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue(evidence.exists(), "runner did not create child evidence")
                _, child_pid_text = evidence.read_text(encoding="utf-8").split()
                child_pid = int(child_pid_text)
                child_handle = kernel32.OpenProcess(
                    child_cleanup_access, False, child_pid
                )
                self.assertTrue(child_handle, qemu_support.ctypes.get_last_error())
                self.assertFalse(runner.job.executed_before_assignment)
                is_member = qemu_support.wintypes.BOOL()
                self.assertTrue(
                    kernel32.IsProcessInJob(
                        child_handle, runner.job.inner._handle, qemu_support.ctypes.byref(is_member)
                    ),
                    qemu_support.ctypes.get_last_error(),
                )
                self.assertTrue(is_member.value, "immediate child escaped the runner job")

                cleanup_runner_process(runner, terminate_timeout=2.0)
                self.assertIsNotNone(runner.process.poll())
                self.assertEqual(kernel32.WaitForSingleObject(child_handle, 2000), 0)
            except BaseException as error:
                captured_error = error
            finally:
                cleanup_failures = self.cleanup_windows_adversarial_resources(
                    runner=runner,
                    child_handle=child_handle,
                    fallback_process=fallback_process,
                    fallback_handle=fallback_handle,
                    kernel32=kernel32,
                )

            if captured_error is not None:
                if cleanup_failures:
                    raise AssertionError(
                        f"{captured_error}\ncleanup failures: {'; '.join(cleanup_failures)}"
                    ) from captured_error
                raise captured_error
            self.assertFalse(
                cleanup_failures,
                f"adversarial cleanup failures: {'; '.join(cleanup_failures)}",
            )

    @unittest.skipUnless(sys.platform == "win32", "Windows Job Object test")
    def test_windows_job_cleans_child_after_parent_has_exited(self) -> None:
        child_pid: int | None = None
        runner, capture = self.spawn_runner(
            "import subprocess,sys,time; child=subprocess.Popen([sys.executable, '-u', '-c', 'import time; time.sleep(60)']); "
            "print('exited parent child pid=' + str(child.pid), flush=True)"
        )
        try:
            capture.wait_for("exited parent child pid=", 2)
            child_pid = int(capture.text().split("exited parent child pid=", 1)[1].splitlines()[0])
            runner.process.wait(timeout=2)
            cleanup_runner_process(runner, terminate_timeout=0.2)
            deadline = time.monotonic() + 2.0
            while self.process_alive(child_pid) and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertFalse(self.process_alive(child_pid), f"surviving exited-parent child {child_pid}")
            self.assertIn("exited parent child pid=", capture.finish(timeout=2))
        finally:
            if child_pid is not None and self.process_alive(child_pid):
                subprocess.run(["taskkill", "/F", "/PID", str(child_pid)], check=False)
            cleanup_runner_process(runner, terminate_timeout=0.2)


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(SessionInputBridgeOracleSelfTest)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.wasSuccessful():
        print("SESSION_INPUT_BRIDGE_ORACLE_SELF_TEST_OK")
        return 0
    return 1


def main() -> int:
    build_boot_image()
    com1, com2, qemu_output, timeline = run_probe_boot()
    assert_session_input_acceptance(com1, com2, qemu_output, timeline)
    print("SESSION_INPUT_BRIDGE_PROBE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-session-input-bridge-probe.py [--self-test]")
    raise SystemExit(main())
