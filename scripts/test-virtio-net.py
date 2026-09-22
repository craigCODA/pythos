#!/usr/bin/env python
"""Loopback-only QEMU socket peer for the bounded virtio-net probe."""

from __future__ import annotations

import importlib.util
import re
import socket
import subprocess
import sys
import threading
import time
import unittest
from pathlib import Path

from qemu_probe_support import AcceptanceTimeline, RunnerCapture, cleanup_runner_process, spawn_runner_process


MIN_ETHERNET_FRAME_BYTES = 60
MAX_ETHERNET_FRAME_BYTES = 1514
PEER_MAC = bytes.fromhex("020000000002")
PROBE_ETHER_TYPE = bytes.fromhex("88b5")
TX_PAYLOAD = b"PYTHOS:NIC:TX"
RX_PAYLOAD = b"PYTHOS:NIC:RX"


SUCCESS_MARKER = "PYTHOS:CORE:VIRTIO_NET_PROBE:READY"
MAC_MARKER_PREFIX = "PYTHOS:CORE:VIRTIO_NET_PROBE:MAC="
ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "virtio-net-probe-com1.log"
ESP_IMAGE = TARGET / "virtio-net-probe-com1-esp.img"
QEMU_TIMEOUT_SECONDS = 30.0
REQUIRED_MARKERS = (
    "PYTHOS:CORE:VIRTIO_NET_PROBE:ENTER",
    "PYTHOS:CORE:VIRTIO_NET_PROBE:PCI_SCAN_READY",
    "PYTHOS:CORE:VIRTIO_NET_PROBE:DEVICE_FOUND",
    "PYTHOS:CORE:VIRTIO_NET_PROBE:LEGACY_TRANSPORT_READY",
    MAC_MARKER_PREFIX,
    "PYTHOS:CORE:VIRTIO_NET_PROBE:FEATURES_NEGOTIATED",
    "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_QUEUE_READY",
    "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_QUEUE_READY",
    "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_FRAME_SENT",
    "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_FRAME_RECEIVED",
    "PYTHOS:CORE:VIRTIO_NET_PROBE:RAW_ETHERNET_READY",
    "PYTHOS:CORE:VIRTIO_NET_PROBE:NO_DISK_WRITES",
    SUCCESS_MARKER,
)
FORBIDDEN_EVIDENCE = (
    "PYTHOS:CORE:VIRTIO_NET_PROBE:ERROR:",
    "PYTHOS:PANIC",
    "DRIVER_ERROR",
    "TIMEOUT",
)
STORAGE_PATH_EVIDENCE_PREFIXES = (
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED",
    "PYTHOS:CORE:BLOCK:SDHCI_EMMC_",
    "PYTHOS:CORE:BLOCK_DEVICE_READY",
    "PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE",
    "PYTHOS:CORE:NORMAL_BOOT_DIAG:BLOCK_",
    "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:BLOCK_",
    "PYTHOS:CORE:STORAGE",
    "PYTHOS:CORE:APPEND_ONLY_JOURNAL_",
    "PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_",
    "PYTHOS:CORE:CRASH_RECOVERY_",
    "PYTHOS:CORE:BLOCK_ALLOCATOR_",
    "PYTHOS:CORE:ALLOCATOR:METADATA_JOURNALED",
    "PYTHOS:CORE:FRAGMENTATION:FREED_BLOCK_REUSED",
    "PYTHOS:CORE:OBJECT_STORE:",
    "PYTHOS:CORE:GENERAL_STORAGE:",
    "PYTHOS:CORE:HARDWARE_PROBE:STORAGE",
    "PYTHOS:CORE:HARDWARE_PROBE:DISK_WRITE",
    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_",
    "PYTHOS:CORE:DISK",
)


class VirtioNetAcceptanceSelfTest(unittest.TestCase):
    """No-QEMU checks for the strict raw-frame acceptance boundary."""

    def valid_serial(self) -> str:
        return "\n".join(
            (
                "PYTHOS:CORE:VIRTIO_NET_PROBE:ENTER",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:PCI_SCAN_READY",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:DEVICE_FOUND",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:LEGACY_TRANSPORT_READY",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=52:54:00:12:34:56",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:FEATURES_NEGOTIATED",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_QUEUE_READY",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_QUEUE_READY",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_FRAME_SENT",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_FRAME_RECEIVED",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:RAW_ETHERNET_READY",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:NO_DISK_WRITES",
                "PYTHOS:CORE:VIRTIO_NET_PROBE:READY",
            )
        )

    def test_exact_marker_oracle_accepts_one_complete_success(self) -> None:
        assert_virtio_net_acceptance(self.valid_serial(), "QEMU_OUTCOME success\n")

    def test_exact_marker_oracle_rejects_duplicate_ready(self) -> None:
        duplicate_ready = self.valid_serial() + "\nPYTHOS:CORE:VIRTIO_NET_PROBE:READY"
        with self.assertRaises(AssertionError):
            assert_virtio_net_acceptance(duplicate_ready, "QEMU_OUTCOME success\n")

    def test_storage_path_oracle_rejects_selection_and_write_evidence(self) -> None:
        for marker in (
            "PYTHOS:CORE:BLOCK_DEVICE_READY",
            "PYTHOS:CORE:STORAGE_SERVICE_READY",
            "PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY",
            "PYTHOS:CORE:OBJECT_STORE:CREATED",
            "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ONLY_BLOCK_READY",
            "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE:LBA",
            "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:BLOCK_DEVICE",
        ):
            with self.subTest(marker=marker), self.assertRaises(AssertionError):
                assert_virtio_net_acceptance(
                    self.valid_serial() + "\n" + marker,
                    "QEMU_OUTCOME success\n",
                )

    def test_live_evidence_preserves_the_exact_runner_outcome(self) -> None:
        evidence = format_live_evidence("QEMU_OUTCOME success\n", "QEMU emulator version 11")
        self.assertIn("QEMU_OUTCOME success", evidence)
        self.assertIn("VIRTIO_NET_QEMU_VERSION QEMU emulator version 11", evidence)

    def test_malformed_frame_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            decode_socket_frame(b"\x00\x00\x00\x3b" + bytes(59))

    def test_frame_peer_reports_exact_tx_and_rx_exchange(self) -> None:
        device_mac = bytes.fromhex("525400123456")
        expected_tx = probe_frame(PEER_MAC, device_mac, TX_PAYLOAD)
        expected_rx = probe_frame(device_mac, PEER_MAC, RX_PAYLOAD)
        peer = FramePeer()
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1) as connection:
                connection.sendall(encode_socket_frame(expected_tx))
                self.assertEqual(read_socket_frame(connection), expected_rx)
            peer.join(timeout=1)
            self.assertIsNone(peer.error)
            self.assertEqual(peer.tx_frame, expected_tx)
            self.assertEqual(peer.rx_frame, expected_rx)
        finally:
            peer.close()

    def test_peer_rejects_a_same_shape_tx_with_a_source_mac_that_differs_from_com1(self) -> None:
        com1_mac = bytes.fromhex("525400123456")
        wrong_mac = bytes.fromhex("525400654321")
        peer = FramePeer()
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1) as connection:
                connection.sendall(encode_socket_frame(probe_frame(PEER_MAC, wrong_mac, TX_PAYLOAD)))
                read_socket_frame(connection)
            peer.join(timeout=1)
            peer.expected_device_mac = com1_mac
            with self.assertRaises(AssertionError):
                assert_peer_exchange(peer)
        finally:
            peer.close()


def validate_ethernet_frame(frame: bytes) -> None:
    validate_ethernet_frame_length(len(frame))


def validate_ethernet_frame_length(length: int) -> None:
    if not MIN_ETHERNET_FRAME_BYTES <= length <= MAX_ETHERNET_FRAME_BYTES:
        raise ValueError("Ethernet frame length must be in 60..1514 bytes")


def encode_socket_frame(frame: bytes) -> bytes:
    validate_ethernet_frame(frame)
    return len(frame).to_bytes(4, "big") + frame


def decode_socket_frame(encoded: bytes) -> bytes:
    if len(encoded) < 4:
        raise ValueError("socket frame is missing its length prefix")
    length = int.from_bytes(encoded[:4], "big")
    frame = encoded[4:]
    if len(frame) != length:
        raise ValueError("socket frame length prefix does not match its payload")
    validate_ethernet_frame(frame)
    return frame


def receive_exact(connection: socket.socket, length: int) -> bytes:
    received = bytearray()
    while len(received) < length:
        chunk = connection.recv(length - len(received))
        if not chunk:
            raise ConnectionError("socket peer closed before the complete frame arrived")
        received.extend(chunk)
    return bytes(received)


def read_socket_frame(connection: socket.socket) -> bytes:
    prefix = receive_exact(connection, 4)
    length = int.from_bytes(prefix, "big")
    validate_ethernet_frame_length(length)
    return decode_socket_frame(prefix + receive_exact(connection, length))


def probe_frame(destination: bytes, source: bytes, payload: bytes) -> bytes:
    if len(destination) != 6 or len(source) != 6:
        raise ValueError("Ethernet MAC addresses must be six bytes")
    frame = destination + source + PROBE_ETHER_TYPE + payload
    return frame + bytes(MIN_ETHERNET_FRAME_BYTES - len(frame))


def validate_transmitted_probe_frame(frame: bytes) -> bytes:
    validate_ethernet_frame(frame)
    device_mac = frame[6:12]
    expected = probe_frame(PEER_MAC, device_mac, TX_PAYLOAD)
    if frame != expected:
        raise ValueError("TX frame mismatch")
    return device_mac


class FramePeer:
    """Accept one loopback QEMU socket connection and exchange one raw frame."""

    def __init__(self, port: int = 0, timeout: float = 10.0) -> None:
        self.listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind(("127.0.0.1", port))
        self.listener.listen(1)
        self.listener.settimeout(timeout)
        self.port = self.listener.getsockname()[1]
        self.timeout = timeout
        self.error: BaseException | None = None
        self.tx_frame: bytes | None = None
        self.rx_frame: bytes | None = None
        self.connected = False
        self.tx_matched = False
        self.rx_delivered = False
        self.expected_device_mac: bytes | None = None
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        if self._thread is not None:
            raise RuntimeError("frame peer is already started")
        self._thread = threading.Thread(target=self._serve_once, daemon=True)
        self._thread.start()

    def join(self, timeout: float | None = None) -> None:
        if self._thread is None:
            raise RuntimeError("frame peer has not been started")
        self._thread.join(timeout)
        if self._thread.is_alive():
            raise TimeoutError("frame peer did not finish")

    def close(self) -> None:
        self.listener.close()

    def _serve_once(self) -> None:
        try:
            with self.listener.accept()[0] as connection:
                self.connected = True
                connection.settimeout(self.timeout)
                self.tx_frame = read_socket_frame(connection)
                device_mac = validate_transmitted_probe_frame(self.tx_frame)
                self.rx_frame = probe_frame(device_mac, PEER_MAC, RX_PAYLOAD)
                connection.sendall(encode_socket_frame(self.rx_frame))
                self.rx_delivered = True
        except BaseException as error:
            self.error = error
        finally:
            self.listener.close()


def assert_exact_ordered_markers(serial: str) -> bytes:
    lines = serial.splitlines()
    previous = -1
    for marker in REQUIRED_MARKERS:
        matches = (
            [index for index, line in enumerate(lines) if line.startswith(marker)]
            if marker == MAC_MARKER_PREFIX
            else [index for index, line in enumerate(lines) if line == marker]
        )
        if len(matches) != 1:
            raise AssertionError(f"expected exactly one {marker!r}, found {len(matches)}")
        if matches[0] <= previous:
            raise AssertionError(f"marker order violation at {marker!r}")
        previous = matches[0]

    mac_line = lines[[index for index, line in enumerate(lines) if line.startswith(MAC_MARKER_PREFIX)][0]]
    if re.fullmatch(r"PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=(?:[0-9A-F]{2}:){5}[0-9A-F]{2}", mac_line) is None:
        raise AssertionError(f"malformed virtio-net MAC evidence: {mac_line!r}")
    return bytes.fromhex(mac_line.removeprefix(MAC_MARKER_PREFIX).replace(":", ""))


def assert_no_storage_path_markers(serial: str) -> None:
    for marker in FORBIDDEN_EVIDENCE:
        if marker in serial:
            raise AssertionError(f"forbidden virtio-net acceptance evidence: {marker}")
    for line in serial.splitlines():
        for prefix in STORAGE_PATH_EVIDENCE_PREFIXES:
            if line.startswith(prefix):
                raise AssertionError(f"forbidden virtio-net storage evidence: {line}")


def assert_qemu_success(qemu_output: str) -> None:
    outcome_lines = [line for line in qemu_output.splitlines() if "QEMU_OUTCOME" in line]
    if outcome_lines != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcome_lines!r}")


def assert_virtio_net_acceptance(serial: str, qemu_output: str) -> bytes:
    device_mac = assert_exact_ordered_markers(serial)
    assert_no_storage_path_markers(serial)
    assert_qemu_success(qemu_output)
    return device_mac


def assert_peer_exchange(peer: FramePeer) -> None:
    if peer.error is not None:
        raise AssertionError(f"virtio-net frame peer failed: {peer.error}") from peer.error
    if not peer.connected:
        raise AssertionError("QEMU did not connect to the virtio-net frame peer")
    if peer.expected_device_mac is None:
        raise AssertionError("serial MAC was not bound to the virtio-net frame peer")
    expected_tx = probe_frame(PEER_MAC, peer.expected_device_mac, TX_PAYLOAD)
    if peer.tx_frame != expected_tx:
        raise AssertionError("QEMU TX frame does not exactly match the COM1 MAC evidence")
    peer.tx_matched = True
    expected_rx = probe_frame(peer.expected_device_mac, PEER_MAC, RX_PAYLOAD)
    if peer.rx_frame != expected_rx:
        raise AssertionError("virtio-net RX frame does not exactly match the COM1 MAC evidence")
    if not peer.rx_delivered or peer.rx_frame is None:
        raise AssertionError("virtio-net frame peer did not deliver the RX frame")


def run(command: list[str]) -> str:
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
        raise AssertionError(f"command failed ({result.returncode}): {' '.join(command)}")
    return result.stdout


def build_probe_image() -> tuple[Path, Path, Path]:
    loader = ROOT / "target" / "x86_64-unknown-uefi" / "debug" / "bootx64.efi"
    kernel = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"
    shell = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run([
        "cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none",
        "--no-default-features", "--features", "virtio-net-probe",
    ])
    # build-image.py requires the verified default shell in INIT.PAK, even though
    # this probe never enters the default boot profile.
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-image.py", "--kernel", str(kernel.resolve())])
    for artifact in (loader, kernel, shell):
        if not artifact.is_file():
            raise AssertionError(f"expected build artifact is missing: {artifact}")
    return loader, kernel, shell


def load_qemu_runner():
    path = ROOT / "scripts" / "run-qemu.py"
    spec = importlib.util.spec_from_file_location("virtio_net_qemu_runner", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def qemu_version() -> str:
    runner = load_qemu_runner()
    output = run([runner.find_qemu(None), "--version"])
    return output.splitlines()[0]


def format_live_evidence(qemu_output: str, version: str) -> str:
    return qemu_output.rstrip("\n") + "\n" + f"VIRTIO_NET_QEMU_VERSION {version}\n"


def probe_runner_command(peer_port: int) -> list[str]:
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
        "--expect-outcome",
        "success",
    ]


def run_probe_boot() -> tuple[str, str, FramePeer]:
    TARGET.mkdir(parents=True, exist_ok=True)
    for path in (SERIAL_LOG, ESP_IMAGE):
        if path.exists():
            path.unlink()
    peer = FramePeer(timeout=QEMU_TIMEOUT_SECONDS)
    runner = None
    capture = None
    serial = ""
    qemu_output = ""
    cleanup_error: BaseException | None = None
    try:
        peer.start()
        popen_kwargs: dict[str, object] = {"cwd": ROOT}
        if sys.platform != "win32":
            popen_kwargs["start_new_session"] = True
        command = probe_runner_command(peer.port)
        print("+ " + " ".join(command), flush=True)
        runner = spawn_runner_process(command, **popen_kwargs)
        capture = RunnerCapture(runner.process, AcceptanceTimeline())
        capture.start()
        deadline = time.monotonic() + QEMU_TIMEOUT_SECONDS + 10.0
        while runner.process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.05)
        if runner.process.poll() is None:
            raise AssertionError("QEMU runner exceeded the bounded acceptance deadline")
        qemu_output = capture.finish()
        if runner.process.returncode != 0:
            raise AssertionError(f"QEMU runner failed with {runner.process.returncode}")
        peer.join(timeout=5.0)
        serial = SERIAL_LOG.read_text(encoding="utf-8", errors="replace")
        peer.expected_device_mac = assert_virtio_net_acceptance(serial, qemu_output)
        assert_peer_exchange(peer)
        return serial, qemu_output, peer
    except BaseException as error:
        if capture is not None:
            qemu_output = capture.text()
        if SERIAL_LOG.exists():
            serial = SERIAL_LOG.read_text(encoding="utf-8", errors="replace")
        raise AssertionError(
            f"{error}\nCOM1:\n{serial}\nrunner output:\n{qemu_output}"
        ) from error
    finally:
        if runner is not None:
            try:
                cleanup_runner_process(runner)
            except BaseException as error:
                cleanup_error = error
        peer.close()
        try:
            peer.join(timeout=1.0)
        except (RuntimeError, TimeoutError) as error:
            if cleanup_error is None:
                cleanup_error = error
        for path in (SERIAL_LOG, ESP_IMAGE):
            if path.exists():
                path.unlink()
        if cleanup_error is not None:
            raise AssertionError(f"virtio-net acceptance cleanup failed: {cleanup_error}") from cleanup_error


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(VirtioNetAcceptanceSelfTest)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.wasSuccessful():
        print("VIRTIO_NET_ACCEPTANCE_SELF_TEST_OK")
        return 0
    return 1


def main() -> int:
    loader, kernel, shell = build_probe_image()
    version = qemu_version()
    _serial, qemu_output, _peer = run_probe_boot()
    print(format_live_evidence(qemu_output, version), end="")
    print(f"VIRTIO_NET_ARTIFACT loader={loader.resolve()}")
    print(f"VIRTIO_NET_ARTIFACT kernel={kernel.resolve()}")
    print(f"VIRTIO_NET_ARTIFACT shell={shell.resolve()}")
    print(f"VIRTIO_NET_ARTIFACT esp={ROOT / 'image' / 'esp'}")
    print(f"VIRTIO_NET_ARTIFACT serial-log-cleaned={SERIAL_LOG.resolve()}")
    print("VIRTIO_NET_PEER_CONNECTED")
    print("VIRTIO_NET_PEER_TX_MATCHED")
    print("VIRTIO_NET_PEER_RX_DELIVERED")
    print("VIRTIO_NET_RUNNER_AND_CHILD_CLEANED")
    print("VIRTIO_NET_ACCEPTANCE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-virtio-net.py [--self-test]")
    raise SystemExit(main())
