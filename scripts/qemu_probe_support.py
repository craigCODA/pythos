"""Shared QEMU probe lifecycle helpers with no probe-specific policy."""

from __future__ import annotations

import os
import signal
import socket
import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from pathlib import Path

if sys.platform == "win32":
    import ctypes
    from ctypes import wintypes


POSIX_SIGKILL = getattr(signal, "SIGKILL", 9)


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


class SerialTail:
    """Record complete COM1 lines as QEMU appends them to its serial file."""

    def __init__(self, path: Path, timeline: AcceptanceTimeline) -> None:
        self.path = path
        self.timeline = timeline
        self.offset = 0
        self.remainder = ""
        self.lines: list[str] = []
        self._lock = threading.Lock()

    def poll(self) -> bool:
        with self._lock:
            if not self.path.exists():
                return False
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
            return bool(parts)

    def contains_all(self, markers: tuple[str, ...]) -> bool:
        with self._lock:
            return all(marker in self.lines for marker in markers)

    def transcript(self) -> str:
        with self._lock:
            return "\n".join(self.lines)


class Com1Observer:
    """Drain the COM1 log independently of every COM2 receive wait."""

    def __init__(self, serial: SerialTail, poll_interval: float = 0.01) -> None:
        self.serial = serial
        self.poll_interval = poll_interval
        self._stop = threading.Event()
        self._updated = threading.Event()
        self._failure: BaseException | None = None
        self._thread = threading.Thread(target=self._drain, daemon=True)

    def start(self) -> None:
        self._thread.start()

    def _drain(self) -> None:
        try:
            while not self._stop.is_set():
                if self.serial.poll():
                    self._updated.set()
                self._stop.wait(self.poll_interval)
            if self.serial.poll():
                self._updated.set()
        except BaseException as error:
            self._failure = error
            self._updated.set()

    def wait_for(
        self,
        markers: tuple[str, ...],
        timeout: float,
        process: subprocess.Popen[str] | None = None,
        capture: RunnerCapture | None = None,
    ) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self._failure is not None:
                raise AssertionError("COM1 observer failed") from self._failure
            if self.serial.contains_all(markers):
                return
            if process is not None and process.poll() is not None:
                detail = capture.text() if capture is not None else ""
                raise AssertionError(f"QEMU exited before COM1 markers {markers!r}: {detail}")
            self._updated.clear()
            self._updated.wait(timeout=min(0.05, deadline - time.monotonic()))
        raise AssertionError(f"timed out waiting for COM1 markers {markers!r}")

    def stop_join(self, timeout: float = 5.0) -> None:
        self._stop.set()
        self._thread.join(timeout=timeout)
        if self._thread.is_alive():
            raise AssertionError("COM1 observer did not stop")
        if self._failure is not None:
            raise AssertionError("COM1 observer failed") from self._failure


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

    def read_until(self, marker: bytes, timeout: float) -> bytes:
        start = len(self.captured)
        marker_line = marker.decode("utf-8")
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
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
            if marker_line in self.complete_lines:
                return bytes(self.captured[start:])
        raise AssertionError(f"timed out waiting for COM2 {marker!r}: {bytes(self.captured)!r}")


def connect_com2(port: int, timeout: float) -> socket.socket:
    deadline = time.monotonic() + timeout
    last_error: OSError | None = None
    while time.monotonic() < deadline:
        try:
            return socket.create_connection(("127.0.0.1", port), timeout=1)
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
        try:
            os.killpg(process_group, POSIX_SIGKILL)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=terminate_timeout)
        except subprocess.TimeoutExpired as error:
            raise AssertionError("runner process group did not reap after SIGKILL") from error
        return
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
