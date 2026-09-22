#!/usr/bin/env python
"""Serialized QEMU acceptance for the bounded capability-gated socket proof."""

from __future__ import annotations

import importlib.util
import select
import socket
import sys
import threading
import time
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
QEMU_TIMEOUT_SECONDS = 30.0
PEER_MAC = bytes.fromhex("020000000002")
DESCRIBED_DEVICE_MAC = bytes.fromhex("525400123456")

DENIED_SUCCESS_MARKER = "PYTHOS:CORE:SOCKET_DENIED_READY"
DENIED_REQUIRED_MARKERS = (
    "PYTHOS:CORE:SOCKET:DENIED_BOOTSTRAPPED",
    "PYTHOS:CORE:SOCKET:OPEN_WITHOUT_CAP_DENIED",
    "PYTHOS:CORE:SOCKET:DENIED_TEARDOWN_COMPLETE",
    DENIED_SUCCESS_MARKER,
)
DENIED_CONSUMER_MARKERS = DENIED_REQUIRED_MARKERS[:2]
DENIED_KERNEL_MARKERS = DENIED_REQUIRED_MARKERS[2:]

GRANTED_SUCCESS_MARKER = "PYTHOS:CORE:SOCKET_READY"
GRANTED_REQUIRED_MARKERS = (
    "PYTHOS:CORE:SOCKET:BOOTSTRAPPED",
    "PYTHOS:CORE:SOCKET:OPEN_GRANTED",
    "PYTHOS:CORE:SOCKET:HANDSHAKE_OK",
    "PYTHOS:CORE:SOCKET:REQUEST_OK",
    "PYTHOS:CORE:SOCKET:RESPONSE_OK",
    "PYTHOS:CORE:SOCKET:CLOSE_OK",
    "PYTHOS:CORE:SOCKET:TEARDOWN_REVOKED",
    GRANTED_SUCCESS_MARKER,
)
GRANTED_CONSUMER_MARKERS = GRANTED_REQUIRED_MARKERS[:6]
GRANTED_KERNEL_MARKERS = GRANTED_REQUIRED_MARKERS[6:]

FORBIDDEN_EVIDENCE = (
    "PYTHOS:CORE:SOCKET:ERROR",
    "PYTHOS:PANIC",
    "TIMEOUT",
    "TRANSPORT_ERROR",
    "transport-error",
    "SOCKET:FAILED",
    "PYTHOS:CORE:BLOCK_DEVICE_READY",
)


def load_tcp_acceptance():
    path = ROOT / "scripts" / "test-tcp.py"
    spec = importlib.util.spec_from_file_location("socket_tcp_acceptance", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load test-tcp.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


TCP = load_tcp_acceptance()
IPV4 = TCP.IPV4
AcceptanceTimeline = TCP.AcceptanceTimeline
Com2Collector = TCP.Com2Collector
Com1Observer = IPV4.Com1Observer
RunnerCapture = IPV4.RunnerCapture
SerialTail = IPV4.SerialTail
cleanup_runner_process = TCP.cleanup_runner_process
connect_com2 = IPV4.connect_com2
encode_socket_frame = TCP.encode_socket_frame
read_socket_frame = TCP.read_socket_frame
set_abortive_close = TCP.set_abortive_close
spawn_runner_process = TCP.spawn_runner_process
finalize_com2_transcript = TCP.finalize_com2_transcript
assert_image_preflight = TCP.assert_image_preflight
run = TCP.run


SERIAL_LOG = TARGET / "socket-granted-com1.log"
ESP_IMAGE = TARGET / "socket-granted-com1-esp.img"
SUCCESS_MARKER = GRANTED_SUCCESS_MARKER
REQUIRED_MARKERS = GRANTED_REQUIRED_MARKERS
CONSUMER_MARKERS = GRANTED_CONSUMER_MARKERS
KERNEL_MARKERS = GRANTED_KERNEL_MARKERS
PEER_FACTORY = TCP.TcpPeer
ASSERT_ACCEPTANCE = None
ASSERT_PEER = None


class DeniedPeer:
    """Bounded host transport observer that permits no Ethernet frame."""

    def __init__(self, port: int = 0, timeout: float = 10.0, *, allow_qemu_reset: bool = False) -> None:
        self.listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind(("127.0.0.1", port))
        self.listener.listen(1)
        self.listener.settimeout(timeout)
        self.port = self.listener.getsockname()[1]
        self.timeout = timeout
        self.allow_qemu_reset = allow_qemu_reset
        self.error: BaseException | None = None
        self.connected = False
        self.completed = False
        self.terminal_reset = False
        self.tx_frames: list[bytes] = []
        self.rx_frames: list[bytes] = []
        self._thread = None

    def start(self) -> None:
        if self._thread is not None:
            raise RuntimeError("denied peer is already started")
        self._thread = threading.Thread(target=self._serve_once, daemon=True)
        self._thread.start()

    def join(self, timeout: float | None = None) -> None:
        if self._thread is None:
            raise RuntimeError("denied peer has not been started")
        self._thread.join(timeout)
        if self._thread.is_alive():
            raise TimeoutError("denied peer did not finish")

    def close(self) -> None:
        self.listener.close()

    def _serve_once(self) -> None:
        try:
            ready, _, _ = select.select([self.listener], [], [], self.timeout)
            if not ready:
                raise TimeoutError("denied peer did not accept the QEMU transport connection")
            connection, _ = self.listener.accept()
            with connection:
                self.connected = True
                connection.settimeout(self.timeout)
                deadline = time.monotonic() + self.timeout
                while True:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise TimeoutError("denied peer did not reach clean EOF")
                    ready, _, _ = select.select([connection], [], [], remaining)
                    if not ready:
                        raise TimeoutError("denied peer did not reach clean EOF")
                    try:
                        data = connection.recv(4096)
                    except ConnectionResetError:
                        if not self.allow_qemu_reset:
                            raise
                        self.terminal_reset = True
                        self.completed = True
                        return
                    if data:
                        self.tx_frames.append(data)
                        raise AssertionError("denied socket proof emitted Ethernet bytes")
                    self.completed = True
                    return
        except BaseException as error:
            self.error = error
        finally:
            self.listener.close()


def configure_case(case: str) -> None:
    global ASSERT_ACCEPTANCE, ASSERT_PEER, CONSUMER_MARKERS, ESP_IMAGE
    global KERNEL_MARKERS, PEER_FACTORY, REQUIRED_MARKERS, SERIAL_LOG
    global SUCCESS_MARKER
    if case == "denied":
        SERIAL_LOG = TARGET / "socket-denied-com1.log"
        ESP_IMAGE = TARGET / "socket-denied-com1-esp.img"
        SUCCESS_MARKER = DENIED_SUCCESS_MARKER
        REQUIRED_MARKERS = DENIED_REQUIRED_MARKERS
        CONSUMER_MARKERS = DENIED_CONSUMER_MARKERS
        KERNEL_MARKERS = DENIED_KERNEL_MARKERS
        PEER_FACTORY = lambda port=0, timeout=10.0: DeniedPeer(
            port, timeout, allow_qemu_reset=True
        )
        ASSERT_ACCEPTANCE = assert_denied_acceptance
        ASSERT_PEER = assert_denied_peer
    elif case == "granted":
        SERIAL_LOG = TARGET / "socket-granted-com1.log"
        ESP_IMAGE = TARGET / "socket-granted-com1-esp.img"
        SUCCESS_MARKER = GRANTED_SUCCESS_MARKER
        REQUIRED_MARKERS = GRANTED_REQUIRED_MARKERS
        CONSUMER_MARKERS = GRANTED_CONSUMER_MARKERS
        KERNEL_MARKERS = GRANTED_KERNEL_MARKERS
        PEER_FACTORY = TCP.TcpPeer
        ASSERT_ACCEPTANCE = assert_granted_acceptance
        ASSERT_PEER = TCP.assert_peer_exchange
    else:
        raise ValueError(f"unknown socket proof case: {case}")

    IPV4.SERIAL_LOG = SERIAL_LOG
    IPV4.ESP_IMAGE = ESP_IMAGE
    IPV4.QEMU_TIMEOUT_SECONDS = QEMU_TIMEOUT_SECONDS
    IPV4.SUCCESS_MARKER = SUCCESS_MARKER
    IPV4.REQUIRED_MARKERS = REQUIRED_MARKERS
    IPV4.CONSUMER_MARKERS = CONSUMER_MARKERS
    IPV4.KERNEL_MARKERS = KERNEL_MARKERS
    IPV4.Ipv4Peer = PEER_FACTORY
    IPV4.probe_runner_command = probe_runner_command
    IPV4.assert_ipv4_acceptance = ASSERT_ACCEPTANCE
    IPV4.assert_runner_success = assert_runner_success
    IPV4.assert_peer_exchange = ASSERT_PEER
    IPV4.assert_live_timeline = assert_live_timeline


def probe_runner_command(peer_port: int, shell_port: int) -> list[str]:
    return [
        sys.executable,
        "scripts/run-qemu.py",
        "--serial-log",
        str(SERIAL_LOG),
        "--success-marker",
        SUCCESS_MARKER,
        "--timeout",
        str(int(QEMU_TIMEOUT_SECONDS)),
        "--no-audio-device",
        "--no-virtio-blk",
        "--virtio-net",
        "--virtio-net-peer-port",
        str(peer_port),
        "--shell-port",
        str(shell_port),
        "--expect-outcome",
        "success",
    ]


def build_probe_image(feature: str) -> tuple[Path, Path, Path, Path]:
    loader = ROOT / "target" / "x86_64-unknown-uefi" / "debug" / "bootx64.efi"
    kernel = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"
    probe = ROOT / "target" / "socket-probe.elf"
    shell = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run([
        "cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none",
        "--no-default-features", "--features", feature,
    ])
    run([sys.executable, "scripts/build-socket-probe.py"])
    run([sys.executable, "scripts/verify-user-elf.py", "--elf", str(probe.resolve())])
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([
        sys.executable,
        "scripts/build-image.py",
        "--kernel",
        str(kernel.resolve()),
        "--socket-probe-elf",
        str(probe.resolve()),
    ])
    for artifact in (loader, kernel, probe, shell):
        if not artifact.is_file():
            raise AssertionError(f"expected build artifact is missing: {artifact}")
    assert_image_preflight()
    return loader, kernel, probe, shell


def assert_exact_ordered_markers(serial: str) -> None:
    observed = [line for line in serial.splitlines() if line.startswith("PYTHOS:CORE:SOCKET")]
    if observed != list(REQUIRED_MARKERS):
        raise AssertionError(f"socket markers were not exact and ordered: {observed!r}")


def assert_no_forbidden_evidence(serial: str, qemu_output: str) -> None:
    evidence = serial + "\n" + qemu_output
    for marker in FORBIDDEN_EVIDENCE:
        if marker in evidence:
            raise AssertionError(f"forbidden socket acceptance evidence: {marker}")
    TCP.VIRTIO_NET.assert_no_storage_path_markers(evidence)


def assert_runner_success(returncode: int | None, qemu_output: str) -> None:
    outcomes = [line for line in qemu_output.splitlines() if "QEMU_OUTCOME" in line]
    if returncode != 0 or outcomes != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome, returncode={returncode}, outcomes={outcomes!r}")


def assert_granted_acceptance(serial: str, qemu_output: str) -> None:
    assert_exact_ordered_markers(serial)
    assert_no_forbidden_evidence(serial, qemu_output)
    assert_runner_success(0, qemu_output)


def assert_denied_acceptance(serial: str, qemu_output: str) -> None:
    assert_exact_ordered_markers(serial)
    assert_no_forbidden_evidence(serial, qemu_output)
    assert_runner_success(0, qemu_output)


def assert_denied_peer(peer: DeniedPeer) -> None:
    if peer.error is not None:
        raise AssertionError(f"denied peer failed: {peer.error}") from peer.error
    if peer.tx_frames or peer.rx_frames:
        raise AssertionError("denied socket proof exchanged Ethernet frames")
    if not peer.connected or not peer.completed:
        raise AssertionError("denied peer did not connect and finish cleanly")
    if peer.terminal_reset and not peer.allow_qemu_reset:
        raise AssertionError("denied peer accepted an unapproved transport reset")


def assert_live_timeline(timeline: AcceptanceTimeline) -> None:
    expected_sources = (
        *(('COM2', marker) for marker in CONSUMER_MARKERS),
        *(('COM1', marker) for marker in KERNEL_MARKERS),
        ('RUNNER', "QEMU_OUTCOME success"),
    )
    for source, marker in expected_sources:
        if timeline.count(source, marker) != 1:
            raise AssertionError(f"timeline expected one {(source, marker)!r}")
    for before, after in zip(expected_sources, expected_sources[1:]):
        timeline.assert_before(*before, *after)


def run_self_tests() -> int:
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(SocketAcceptanceSelfTest)
    )
    if result.wasSuccessful():
        print("SOCKET_QEMU_ACCEPTANCE_OK")
        return 0
    return 1


class SocketAcceptanceSelfTest(unittest.TestCase):
    def test_exact_adr0101_frames_are_reused(self) -> None:
        frames = TCP.exchange_frames(DESCRIBED_DEVICE_MAC)
        TCP.assert_exact_exchange(DESCRIBED_DEVICE_MAC, frames)
        self.assertEqual(len(frames), 12)
        self.assertEqual([len(frame) for frame in frames], [60] * 12)

    def test_granted_marker_and_outcome_oracle_rejects_mutations(self) -> None:
        configure_case("granted")
        valid = "\n".join(GRANTED_REQUIRED_MARKERS)
        assert_granted_acceptance(valid, "QEMU_OUTCOME success\n")
        for invalid in (
            "\n".join((*GRANTED_REQUIRED_MARKERS[:2], *GRANTED_REQUIRED_MARKERS[3:])),
            valid + "\n" + GRANTED_SUCCESS_MARKER,
            valid + "\nPYTHOS:CORE:BLOCK_DEVICE_READY",
        ):
            with self.assertRaises(AssertionError):
                assert_granted_acceptance(invalid, "QEMU_OUTCOME success\n")
        with self.assertRaises(AssertionError):
            assert_granted_acceptance(valid, "QEMU_OUTCOME success\nQEMU_OUTCOME success\n")

    def test_denied_marker_and_zero_frame_oracle(self) -> None:
        configure_case("denied")
        valid = "\n".join(DENIED_REQUIRED_MARKERS)
        assert_denied_acceptance(valid, "QEMU_OUTCOME success\n")

    def test_denied_peer_accepts_connected_zero_frame_eof(self) -> None:
        peer = DeniedPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0):
                pass
            peer.join(timeout=2.0)
            assert_denied_peer(peer)
        finally:
            peer.close()

    def test_denied_peer_rejects_connection_reset(self) -> None:
        peer = DeniedPeer(timeout=1.0)
        peer.start()
        try:
            connection = socket.create_connection(("127.0.0.1", peer.port), timeout=1.0)
            set_abortive_close(connection)
            connection.close()
            peer.join(timeout=2.0)
            self.assertIsInstance(peer.error, ConnectionResetError)
            self.assertFalse(peer.completed)
            with self.assertRaises(AssertionError):
                assert_denied_peer(peer)
        finally:
            peer.close()

    def test_denied_peer_rejects_no_connection_timeout(self) -> None:
        peer = DeniedPeer(timeout=0.1)
        peer.start()
        try:
            peer.join(timeout=1.0)
            self.assertIsInstance(peer.error, TimeoutError)
            self.assertFalse(peer.connected)
            self.assertFalse(peer.completed)
            with self.assertRaises(AssertionError):
                assert_denied_peer(peer)
        finally:
            peer.close()

    def test_denied_peer_rejects_connected_quiet_timeout(self) -> None:
        peer = DeniedPeer(timeout=0.1)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0):
                peer.join(timeout=1.0)
            self.assertIsInstance(peer.error, TimeoutError)
            self.assertTrue(peer.connected)
            self.assertFalse(peer.completed)
            with self.assertRaises(AssertionError):
                assert_denied_peer(peer)
        finally:
            peer.close()

    def test_denied_peer_rejects_any_frame(self) -> None:
        peer = DeniedPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(b"unexpected-frame")
            peer.join(timeout=2.0)
            self.assertIsNotNone(peer.error)
            self.assertIn("Ethernet", str(peer.error))
        finally:
            peer.close()


def main() -> int:
    for case, feature in (
        ("denied", "socket-api-denied-probe"),
        ("granted", "socket-api-probe"),
    ):
        configure_case(case)
        loader, kernel, probe, shell = build_probe_image(feature)
        consumer, kernel_serial, qemu_output, peer = TCP.run_probe_boot()
        print(
            f"SOCKET_LIVE_CASE {case} "
            f"tx={len(peer.tx_frames)} rx={len(peer.rx_frames)} "
            f"total={len(peer.tx_frames) + len(peer.rx_frames)}"
        )
        observed = consumer.splitlines() + kernel_serial.splitlines()
        marker_evidence = [line for line in observed if line in REQUIRED_MARKERS]
        print(f"SOCKET_LIVE_MARKERS {case} {' > '.join(marker_evidence)}")
        print(
            f"SOCKET_LIVE_OUTCOME {case} "
            f"{next(line for line in qemu_output.splitlines() if line == 'QEMU_OUTCOME success')}"
        )
        print(
            f"SOCKET_LIVE_CLEANUP {case} "
            f"serial_log_exists={SERIAL_LOG.exists()} esp_snapshot_exists={ESP_IMAGE.exists()}"
        )
        if SERIAL_LOG.exists() or ESP_IMAGE.exists():
            raise AssertionError(f"{case} proof left acceptance artifacts behind")
        print(f"SOCKET_ARTIFACT {case} loader={loader.resolve()}")
        print(f"SOCKET_ARTIFACT {case} kernel={kernel.resolve()}")
        print(f"SOCKET_ARTIFACT {case} probe={probe.resolve()}")
        print(f"SOCKET_ARTIFACT {case} shell={shell.resolve()}")
    print("SOCKET_QEMU_ACCEPTANCE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-socket.py [--self-test]")
    raise SystemExit(main())
