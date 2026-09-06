#!/usr/bin/env python
"""Strict dual-channel acceptance oracle for the opt-in session-input probe."""

from __future__ import annotations

import os
import re
import signal
import socket
import subprocess
import sys
import threading
import time
import unittest
from dataclasses import dataclass
from pathlib import Path

if sys.platform == "win32":
    import ctypes
    from ctypes import wintypes

import launcher_click

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "session-input-bridge-probe-com1.log"
SHELL_PORT = 4592
QEMU_TIMEOUT_SECONDS = 45.0
COM2_CONNECT_TIMEOUT_SECONDS = 20.0
COM2_READ_TIMEOUT_SECONDS = 15.0
DRAIN_SECONDS = 0.25
POSIX_SIGKILL = getattr(signal, "SIGKILL", 9)

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


if sys.platform == "win32":
    class _JobBasicLimitInformation(ctypes.Structure):
        _fields_ = [
            ("per_process_user_time_limit", ctypes.c_longlong),
            ("per_job_user_time_limit", ctypes.c_longlong),
            ("limit_flags", wintypes.DWORD),
            ("minimum_working_set_size", ctypes.c_size_t),
            ("maximum_working_set_size", ctypes.c_size_t),
            ("active_process_limit", wintypes.DWORD),
            ("affinity", ctypes.c_size_t),
            ("priority_class", wintypes.DWORD),
            ("scheduling_class", wintypes.DWORD),
        ]

    class _IoCounters(ctypes.Structure):
        _fields_ = [(name, ctypes.c_ulonglong) for name in (
            "read_operation_count", "write_operation_count", "other_operation_count",
            "read_transfer_count", "write_transfer_count", "other_transfer_count",
        )]

    class _JobExtendedLimitInformation(ctypes.Structure):
        _fields_ = [
            ("basic_limit_information", _JobBasicLimitInformation),
            ("io_info", _IoCounters),
            ("process_memory_limit", ctypes.c_size_t),
            ("job_memory_limit", ctypes.c_size_t),
            ("peak_process_memory_used", ctypes.c_size_t),
            ("peak_job_memory_used", ctypes.c_size_t),
        ]

    class WindowsJob:
        _KILL_ON_JOB_CLOSE = 0x00002000
        _EXTENDED_LIMIT_INFORMATION = 9

        def __init__(self, process: subprocess.Popen[str]) -> None:
            kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
            self._close_handle = kernel32.CloseHandle
            self._terminate_job = kernel32.TerminateJobObject
            self._handle = kernel32.CreateJobObjectW(None, None)
            if not self._handle:
                raise OSError(ctypes.get_last_error(), "CreateJobObjectW failed")
            info = _JobExtendedLimitInformation()
            info.basic_limit_information.limit_flags = self._KILL_ON_JOB_CLOSE
            if not kernel32.SetInformationJobObject(
                self._handle,
                self._EXTENDED_LIMIT_INFORMATION,
                ctypes.byref(info),
                ctypes.sizeof(info),
            ):
                self.close()
                raise OSError(ctypes.get_last_error(), "SetInformationJobObject failed")
            if not kernel32.AssignProcessToJobObject(self._handle, process._handle):
                self.close()
                raise OSError(ctypes.get_last_error(), "AssignProcessToJobObject failed")

        def terminate(self) -> None:
            if self._handle:
                self._terminate_job(self._handle, 1)

        def close(self) -> None:
            if self._handle:
                self._close_handle(self._handle)
                self._handle = None
else:
    WindowsJob = None


@dataclass
class RunnerHandle:
    process: subprocess.Popen[str]
    process_group: int | None
    job: WindowsJob | None


def spawn_runner_process(command: list[str], **popen_kwargs: object) -> RunnerHandle:
    process = subprocess.Popen(
        command,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        **popen_kwargs,
    )
    if sys.platform == "win32":
        return RunnerHandle(process, None, WindowsJob(process))
    return RunnerHandle(process, os.getpgid(process.pid), None)


class AcceptanceTimeline:
    """One observer-owned ordering record for COM1, COM2, and harness phases."""

    def __init__(self) -> None:
        self.events: list[tuple[str, str]] = []
        self._lock = threading.Lock()

    def record(self, source: str, value: str) -> None:
        with self._lock:
            self.events.append((source, value))

    def count(self, source: str, value: str) -> int:
        with self._lock:
            return self.events.count((source, value))

    def index(self, source: str, value: str) -> int:
        with self._lock:
            return self._index_unlocked(source, value)

    def _index_unlocked(self, source: str, value: str) -> int:
        for index, event in enumerate(self.events):
            if event == (source, value):
                return index
        raise AssertionError(f"timeline missing {(source, value)!r}")

    def move_after(self, source: str, value: str, before_source: str, before_value: str) -> None:
        with self._lock:
            moved = self.events.pop(self._index_unlocked(source, value))
            self.events.insert(
                self._index_unlocked(before_source, before_value) + 1, moved
            )

    def assert_before(
        self, before_source: str, before_value: str, after_source: str, after_value: str
    ) -> None:
        with self._lock:
            before_count = self.events.count((before_source, before_value))
            after_count = self.events.count((after_source, after_value))
            if before_count != 1 or after_count != 1:
                raise AssertionError(
                    f"timeline must contain one {(before_source, before_value)!r} and one {(after_source, after_value)!r}"
                )
            if self._index_unlocked(before_source, before_value) >= self._index_unlocked(after_source, after_value):
                raise AssertionError(
                    f"timeline order violation: {(before_source, before_value)!r} must precede {(after_source, after_value)!r}"
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


class SerialTail:
    """Record complete COM1 lines as QEMU appends them to its serial file."""

    def __init__(self, path: Path, timeline: AcceptanceTimeline) -> None:
        self.path = path
        self.timeline = timeline
        self.offset = 0
        self.remainder = ""
        self.lines: list[str] = []

    def poll(self) -> None:
        if not self.path.exists():
            return
        data = self.path.read_bytes()
        if len(data) < self.offset:
            self.offset = 0
            self.remainder = ""
            self.lines.clear()
        chunk = data[self.offset :].decode("utf-8", errors="replace")
        self.offset = len(data)
        parts = (self.remainder + chunk).split("\n")
        self.remainder = parts.pop()
        for line in parts:
            complete = line.rstrip("\r")
            self.lines.append(complete)
            self.timeline.record("COM1", complete)

    def transcript(self) -> str:
        self.poll()
        return "\n".join(self.lines)


class RunnerCapture:
    """Continuously drain runner output so failures cannot lose diagnostics."""

    def __init__(self, process: subprocess.Popen[str], timeline: AcceptanceTimeline) -> None:
        self.process = process
        self.timeline = timeline
        self._lines: list[str] = []
        self._lock = threading.Lock()
        self._updated = threading.Event()
        self._thread = threading.Thread(target=self._drain, daemon=True)

    def start(self) -> None:
        self._thread.start()

    def _drain(self) -> None:
        if self.process.stdout is None:
            return
        for raw_line in self.process.stdout:
            line = raw_line.rstrip("\r\n")
            with self._lock:
                self._lines.append(raw_line)
            self._updated.set()
            self.timeline.record("RUNNER_STDOUT", line)
            if line == "QEMU_OUTCOME success":
                self.timeline.record("RUNNER", line)

    def text(self) -> str:
        with self._lock:
            return "".join(self._lines)

    def finish(self, timeout: float = 5.0) -> str:
        self._thread.join(timeout=timeout)
        if self._thread.is_alive():
            raise AssertionError("runner output drainer did not finish")
        return self.text()

    def wait_for(self, marker: str, timeout: float) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if marker in self.text():
                return
            self._updated.clear()
            self._updated.wait(timeout=min(0.05, deadline - time.monotonic()))
        raise AssertionError(f"runner output did not contain {marker!r}: {self.text()}")


class Com2Collector:
    def __init__(self, sock: socket.socket, timeline: AcceptanceTimeline) -> None:
        self.sock = sock
        self.timeline = timeline
        self.captured = bytearray()
        self.remainder = b""
        self.complete_lines: list[str] = []
        self.sock.settimeout(0.25)

    def _record_complete_lines(self, chunk: bytes) -> None:
        parts = (self.remainder + chunk).split(b"\n")
        self.remainder = parts.pop()
        for line in parts:
            complete = line.rstrip(b"\r").decode("utf-8", errors="replace")
            self.complete_lines.append(complete)
            self.timeline.record("COM2", complete)

    def read_until(self, marker: bytes, timeout: float, poll_com1) -> bytes:
        start = len(self.captured)
        marker_line = marker.decode("utf-8")
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            poll_com1()
            if marker_line in self.complete_lines:
                return bytes(self.captured[start:])
            try:
                chunk = self.sock.recv(512)
            except socket.timeout:
                continue
            if not chunk:
                raise AssertionError(f"COM2 closed before {marker!r}: {bytes(self.captured)!r}")
            self.captured.extend(chunk)
            self._record_complete_lines(chunk)
            poll_com1()
            if marker_line in self.complete_lines:
                return bytes(self.captured[start:])
        raise AssertionError(f"timed out waiting for COM2 {marker!r}: {bytes(self.captured)!r}")


def wait_for_com1_marker(
    process: subprocess.Popen[str], capture: RunnerCapture, serial: SerialTail, marker: str, timeout: float
) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        serial.poll()
        if marker in serial.lines:
            return serial.transcript()
        if process.poll() is not None:
            raise AssertionError(f"QEMU exited before {marker}: {capture.text()}")
        time.sleep(0.05)
    raise AssertionError(f"timed out waiting for COM1 marker {marker}")


def wait_for_com1_markers(
    process: subprocess.Popen[str], capture: RunnerCapture, serial: SerialTail, markers: tuple[str, ...], timeout: float
) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        serial.poll()
        if all(marker in serial.lines for marker in markers):
            return
        if process.poll() is not None:
            raise AssertionError(f"QEMU exited before input IRQ evidence: {capture.text()}")
        time.sleep(0.01)
    raise AssertionError(f"timed out waiting for COM1 IRQ evidence {markers!r}")


def connect_com2(timeout: float) -> socket.socket:
    deadline = time.monotonic() + timeout
    last_error: OSError | None = None
    while time.monotonic() < deadline:
        try:
            return socket.create_connection(("127.0.0.1", SHELL_PORT), timeout=1)
        except OSError as error:
            last_error = error
            time.sleep(0.05)
    raise AssertionError(f"could not connect COM2 before readiness: {last_error}")


def cleanup_posix_process_group(process, process_group: int, terminate_timeout: float) -> None:
    try:
        os.killpg(process_group, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=terminate_timeout)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process_group, POSIX_SIGKILL)
    except ProcessLookupError:
        pass


def cleanup_runner_process(runner: RunnerHandle, terminate_timeout: float = 5.0) -> None:
    """Terminate and reap the runner and its QEMU child on every platform."""
    process = runner.process
    if sys.platform != "win32" and runner.process_group is not None:
        cleanup_posix_process_group(process, runner.process_group, terminate_timeout)
    elif process.poll() is None:
        if runner.job is not None:
            runner.job.terminate()
        else:
            process.kill()
        process.wait(timeout=terminate_timeout)
    else:
        process.wait(timeout=terminate_timeout)
    if runner.job is not None:
        runner.job.close()


def run_probe_boot() -> tuple[str, str, str, AcceptanceTimeline]:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    popen_kwargs: dict[str, object] = {}
    if sys.platform != "win32":
        popen_kwargs["start_new_session"] = True
    command = [
        sys.executable, "scripts/run-qemu.py", "--serial-log", str(SERIAL_LOG),
        "--shell-port", str(SHELL_PORT), "--timeout", str(QEMU_TIMEOUT_SECONDS),
        "--expect-outcome", "success",
    ]
    print("+ " + " ".join(command), flush=True)
    popen_kwargs["cwd"] = ROOT
    runner = spawn_runner_process(command, **popen_kwargs)
    process = runner.process
    timeline = AcceptanceTimeline()
    serial = SerialTail(SERIAL_LOG, timeline)
    capture = RunnerCapture(process, timeline)
    capture.start()
    captured_error: BaseException | None = None
    result: tuple[str, str, str, AcceptanceTimeline] | None = None
    try:
        # The wait=off COM2 backend drops writes before a client connects.
        with connect_com2(COM2_CONNECT_TIMEOUT_SECONDS) as com2:
            collector = Com2Collector(com2, timeline)
            wait_for_com1_marker(
                process, capture, serial, COM1_MARKERS[0], COM2_CONNECT_TIMEOUT_SECONDS
            )
            before_input = collector.read_until(
                COM2_MARKERS[0].encode(), COM2_READ_TIMEOUT_SECONDS, serial.poll
            )
            serial.poll()
            timeline.record("HARNESS", "QMP_INJECTION_STARTED")
            launcher_click.type_session_input_bridge_sequence()
            drain_deadline = time.monotonic() + DRAIN_SECONDS
            while time.monotonic() < drain_deadline:
                serial.poll()
                if process.poll() is not None:
                    raise AssertionError(f"QEMU exited during input injection: {capture.text()}")
                time.sleep(0.01)
            wait_for_com1_markers(
                process,
                capture,
                serial,
                (COM1_MARKERS[4], COM1_MARKERS[5]),
                COM2_READ_TIMEOUT_SECONDS,
            )
            timeline.record("HARNESS", "G_SENT")
            com2.sendall(b"G")
            after_input = collector.read_until(
                COM2_MARKERS[-1].encode(), COM2_READ_TIMEOUT_SECONDS, serial.poll
            )
        runner_deadline = time.monotonic() + QEMU_TIMEOUT_SECONDS + 5
        while process.poll() is None and time.monotonic() < runner_deadline:
            serial.poll()
            time.sleep(0.05)
        if process.poll() is None:
            raise AssertionError("QEMU runner did not terminate after probe acknowledgement")
        serial.poll()
        qemu_output = capture.finish()
        if process.returncode != 0:
            raise AssertionError(f"QEMU runner failed with {process.returncode}")
        assert_qemu_success(qemu_output)
        result = (
            serial.transcript(),
            bytes(collector.captured).decode("utf-8", errors="replace"),
            qemu_output,
            timeline,
        )
    except BaseException as error:
        captured_error = error
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
        first = collector.read_until(COM2_MARKERS[0].encode(), 1, lambda: None)
        second = collector.read_until(COM2_MARKERS[-1].encode(), 1, lambda: None)
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
            collector.read_until(COM2_MARKERS[0].encode(), 1, lambda: None),
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

        class ReceiveRaceSocket:
            def settimeout(self, _timeout: float) -> None:
                pass

            def recv(self, _size: int) -> bytes:
                timeline.record("COM1", COM1_MARKERS[6])
                return b"\r\n".join(marker.encode() for marker in COM2_MARKERS[1:]) + b"\r\n"

        collector = Com2Collector(ReceiveRaceSocket(), timeline)
        collector.read_until(COM2_MARKERS[-1].encode(), 1, lambda: None)
        timeline.record("COM1", COM1_MARKERS[7])
        timeline.record("COM1", COM1_MARKERS[8])
        timeline.record("RUNNER", "QEMU_OUTCOME success")
        with self.assertRaises(AssertionError):
            assert_session_input_acceptance(
                self.valid_com1(), bytes(collector.captured).decode(), "QEMU_OUTCOME success", timeline
            )

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
