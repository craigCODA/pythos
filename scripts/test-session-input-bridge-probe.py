#!/usr/bin/env python
"""Strict dual-channel acceptance oracle for the opt-in session-input probe."""

from __future__ import annotations

import os
import signal
import socket
import subprocess
import sys
import time
import unittest
from pathlib import Path

import launcher_click

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


def assert_exact_ordered_markers(transcript: str, markers: tuple[str, ...], channel: str) -> None:
    lines = transcript.splitlines()
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
    forbidden = ("GAP", "ERROR", "PYTHOS:PANIC", "PYTHOS:CORE:DISK_WRITE", "PYTHOS:CORE:DISK:WRITE")
    for marker in forbidden:
        if marker in transcript:
            raise AssertionError(f"{channel}: forbidden marker {marker!r}")


def assert_com1_transcript(transcript: str) -> None:
    assert_no_failure_markers(transcript, "COM1")
    assert_exact_ordered_markers(transcript, COM1_MARKERS, "COM1")


def assert_com2_transcript(transcript: str) -> None:
    assert_no_failure_markers(transcript, "COM2")
    assert_exact_ordered_markers(transcript, COM2_MARKERS, "COM2")


def assert_qemu_success(output: str) -> None:
    if "QEMU_OUTCOME timeout" in output:
        raise AssertionError("QEMU timed out")
    if output.count("QEMU_OUTCOME success") != 1:
        raise AssertionError("expected exactly one QEMU_OUTCOME success")
    if output.count("QEMU_OUTCOME") != 1:
        raise AssertionError("unexpected additional QEMU outcome")


def assert_session_input_acceptance(com1: str, com2: str, qemu_output: str) -> None:
    """Accept only the complete, ordered, non-error Slice 1 transcript."""
    assert_com1_transcript(com1)
    assert_com2_transcript(com2)
    assert_qemu_success(qemu_output)


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


def read_serial_log() -> str:
    if not SERIAL_LOG.exists():
        return ""
    return SERIAL_LOG.read_text(encoding="utf-8", errors="replace")


def wait_for_com1_marker(process: subprocess.Popen[str], marker: str, timeout: float) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        transcript = read_serial_log()
        if marker in transcript:
            return transcript
        if process.poll() is not None:
            output, _ = process.communicate()
            raise AssertionError(f"QEMU exited before {marker}: {output}")
        time.sleep(0.05)
    raise AssertionError(f"timed out waiting for COM1 marker {marker}")


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


def read_com2_until(sock: socket.socket, marker: bytes, timeout: float) -> bytes:
    deadline = time.monotonic() + timeout
    captured = bytearray()
    sock.settimeout(0.5)
    while time.monotonic() < deadline:
        try:
            chunk = sock.recv(512)
        except socket.timeout:
            continue
        if not chunk:
            raise AssertionError(f"COM2 closed before {marker!r}: {bytes(captured)!r}")
        captured.extend(chunk)
        if marker in captured:
            return bytes(captured)
    raise AssertionError(f"timed out waiting for COM2 {marker!r}: {bytes(captured)!r}")


def cleanup_runner_process(process: subprocess.Popen[str]) -> None:
    """Terminate and reap the runner and its QEMU child on every platform."""
    if process.poll() is not None:
        return
    if sys.platform == "win32":
        subprocess.run(
            ["taskkill", "/F", "/T", "/PID", str(process.pid)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    else:
        try:
            os.killpg(os.getpgid(process.pid), signal.SIGTERM)
        except ProcessLookupError:
            pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def run_probe_boot() -> tuple[str, str, str]:
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
    process = subprocess.Popen(
        command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        **popen_kwargs,
    )
    try:
        # The wait=off COM2 backend drops writes before a client connects.
        with connect_com2(COM2_CONNECT_TIMEOUT_SECONDS) as com2:
            wait_for_com1_marker(process, COM1_MARKERS[0], COM2_CONNECT_TIMEOUT_SECONDS)
            before_input = read_com2_until(com2, COM2_MARKERS[0].encode(), COM2_READ_TIMEOUT_SECONDS)
            launcher_click.type_session_input_bridge_sequence()
            time.sleep(DRAIN_SECONDS)
            com2.sendall(b"G")
            after_input = read_com2_until(com2, COM2_MARKERS[-1].encode(), COM2_READ_TIMEOUT_SECONDS)
        try:
            qemu_output, _ = process.communicate(timeout=QEMU_TIMEOUT_SECONDS + 5)
        except subprocess.TimeoutExpired as error:
            raise AssertionError("QEMU runner did not terminate after probe acknowledgement") from error
        print(qemu_output, end="")
        if process.returncode != 0:
            raise AssertionError(f"QEMU runner failed with {process.returncode}")
        assert_qemu_success(qemu_output)
        return (
            read_serial_log(),
            (before_input + after_input).decode("utf-8", errors="replace"),
            qemu_output,
        )
    finally:
        cleanup_runner_process(process)


class SessionInputBridgeOracleSelfTest(unittest.TestCase):
    def valid_com1(self) -> str:
        return "\n".join(COM1_MARKERS)

    def valid_com2(self) -> str:
        return "\n".join(COM2_MARKERS)

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
            )

    def test_valid_dual_channel_transcript_passes(self) -> None:
        assert_session_input_acceptance(self.valid_com1(), self.valid_com2(), "QEMU_OUTCOME success")

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
        for marker in ("PYTHOS:SESSION_INPUT_PROBE:GAP", "PYTHOS:SESSION_INPUT_PROBE:ERROR", "PYTHOS:PANIC", "PYTHOS:CORE:DISK_WRITE_ATTEMPT"):
            with self.subTest(marker=marker):
                self.assert_rejected(com2=self.valid_com2() + "\n" + marker)
        self.assert_rejected(outcome="QEMU_OUTCOME timeout")
        self.assert_rejected(outcome="")
        self.assert_rejected(outcome="QEMU_OUTCOME success\nQEMU_OUTCOME success")

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


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(SessionInputBridgeOracleSelfTest)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.wasSuccessful():
        print("SESSION_INPUT_BRIDGE_ORACLE_SELF_TEST_OK")
        return 0
    return 1


def main() -> int:
    build_boot_image()
    com1, com2, qemu_output = run_probe_boot()
    assert_session_input_acceptance(com1, com2, qemu_output)
    print("SESSION_INPUT_BRIDGE_PROBE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-session-input-bridge-probe.py [--self-test]")
    raise SystemExit(main())
