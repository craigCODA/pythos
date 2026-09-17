#!/usr/bin/env python
"""Deterministic QEMU acceptance for the finite ARP consumer probe."""

from __future__ import annotations

import importlib.util
import select
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

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
SERIAL_LOG = TARGET / "arp-probe-com1.log"
ESP_IMAGE = TARGET / "arp-probe-com1-esp.img"
QEMU_TIMEOUT_SECONDS = 30.0
SUCCESS_MARKER = "PYTHOS:CORE:ARP_READY"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:ARP:BOOTSTRAPPED",
    "PYTHOS:CORE:ARP:DESCRIBE_OK",
    "PYTHOS:CORE:ARP:REQUEST_OK",
    "PYTHOS:CORE:ARP:REPLY_OK",
    "PYTHOS:CORE:ARP:TEARDOWN_REVOKED",
    SUCCESS_MARKER,
)
CONSUMER_MARKERS = REQUIRED_MARKERS[:4]
KERNEL_MARKERS = REQUIRED_MARKERS[4:]
FORBIDDEN_EVIDENCE = (
    "PYTHOS:CORE:ARP:ERROR",
    "PYTHOS:PANIC",
    "TIMEOUT",
    "TRANSPORT_ERROR",
    "transport-error",
    "ARP:FAILED",
)
ARP_ETHER_TYPE = 0x0806
PEER_MAC = bytes.fromhex("020000000002")
DESCRIBED_DEVICE_MAC = bytes.fromhex("525400123456")
LOCAL_IPV4 = bytes.fromhex("c0000202")
PEER_IPV4 = bytes.fromhex("c0000201")
BROADCAST_MAC = bytes(6 * b"\xff")
MIN_ETHERNET_FRAME_BYTES = 60
ARP_PAYLOAD_BYTES = 28


def load_virtio_net_acceptance():
    path = ROOT / "scripts" / "test-virtio-net.py"
    spec = importlib.util.spec_from_file_location("arp_virtio_net_peer", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load test-virtio-net.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


VIRTIO_NET = load_virtio_net_acceptance()
encode_socket_frame = VIRTIO_NET.encode_socket_frame
read_socket_frame = VIRTIO_NET.read_socket_frame


def set_abortive_close(connection: socket.socket) -> None:
    """Request an immediate reset using the host's native linger layout."""
    linger = struct.pack("hh", 1, 0) if sys.platform == "win32" else struct.pack("ii", 1, 0)
    connection.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, linger)


def arp_payload(
    operation: int,
    sender_hardware: bytes,
    sender_protocol: bytes,
    target_hardware: bytes,
    target_protocol: bytes,
) -> bytes:
    if not 0 <= operation <= 0xFFFF:
        raise ValueError("ARP operation must fit in sixteen bits")
    if len(sender_hardware) != 6 or len(target_hardware) != 6:
        raise ValueError("ARP hardware addresses must be six bytes")
    if len(sender_protocol) != 4 or len(target_protocol) != 4:
        raise ValueError("ARP protocol addresses must be four bytes")
    return struct.pack(
        "!HHBBH6s4s6s4s",
        1,
        0x0800,
        6,
        4,
        operation,
        sender_hardware,
        sender_protocol,
        target_hardware,
        target_protocol,
    )


def arp_frame(destination: bytes, source: bytes, payload: bytes) -> bytes:
    if len(destination) != 6 or len(source) != 6:
        raise ValueError("Ethernet MAC addresses must be six bytes")
    if len(payload) != ARP_PAYLOAD_BYTES:
        raise ValueError("ARP payload must be exactly 28 bytes")
    frame = destination + source + ARP_ETHER_TYPE.to_bytes(2, "big") + payload
    return frame + bytes(MIN_ETHERNET_FRAME_BYTES - len(frame))


def assert_exact_arp_request(device_mac: bytes, frame: bytes) -> None:
    expected = arp_frame(
        BROADCAST_MAC,
        device_mac,
        arp_payload(1, device_mac, LOCAL_IPV4, bytes(6), PEER_IPV4),
    )
    if frame != expected:
        raise AssertionError(f"unexpected ARP request frame: {frame.hex()}")


def peer_reply(device_mac: bytes) -> bytes:
    return arp_frame(
        device_mac,
        PEER_MAC,
        arp_payload(2, PEER_MAC, PEER_IPV4, device_mac, LOCAL_IPV4),
    )


def assert_exact_arp_reply(device_mac: bytes, frame: bytes) -> None:
    expected = peer_reply(device_mac)
    if frame != expected:
        raise AssertionError(f"unexpected ARP reply frame: {frame.hex()}")


class ArpPeer:
    """One loopback-only peer for exactly one ARP request and reply."""

    def __init__(self, port: int = 0, timeout: float = 10.0) -> None:
        self.listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind(("127.0.0.1", port))
        self.listener.listen(1)
        self.listener.settimeout(timeout)
        self.port = self.listener.getsockname()[1]
        self.timeout = timeout
        self.error: BaseException | None = None
        self.connected = False
        self.completed = False
        self.tx_frame: bytes | None = None
        self.rx_frame: bytes | None = None
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        if self._thread is not None:
            raise RuntimeError("ARP peer is already started")
        self._thread = threading.Thread(target=self._serve_once, daemon=True)
        self._thread.start()

    def join(self, timeout: float | None = None) -> None:
        if self._thread is None:
            raise RuntimeError("ARP peer has not been started")
        self._thread.join(timeout)
        if self._thread.is_alive():
            raise TimeoutError("ARP peer did not finish")

    def close(self) -> None:
        self.listener.close()

    def _serve_once(self) -> None:
        try:
            with self.listener.accept()[0] as connection:
                self.connected = True
                connection.settimeout(self.timeout)
                self.tx_frame = read_socket_frame(connection)
                assert_exact_arp_request(DESCRIBED_DEVICE_MAC, self.tx_frame)
                self.rx_frame = peer_reply(DESCRIBED_DEVICE_MAC)
                connection.sendall(encode_socket_frame(self.rx_frame))
                deadline = time.monotonic() + self.timeout
                while True:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise TimeoutError("ARP peer did not reach clean completion after the reply")
                    if not select.select([connection], [], [], remaining)[0]:
                        raise TimeoutError("ARP peer did not reach clean completion after the reply")
                    try:
                        if connection.recv(4096):
                            raise AssertionError("ARP peer received additional TX bytes after the request")
                    except ConnectionResetError:
                        pass
                    self.completed = True
                    break
        except BaseException as error:
            self.error = error
        finally:
            self.listener.close()


def assert_exact_ordered_markers(serial: str) -> None:
    observed = serial.splitlines()
    previous = -1
    for marker in REQUIRED_MARKERS:
        matches = [index for index, line in enumerate(observed) if line == marker]
        if len(matches) != 1:
            raise AssertionError(f"expected exactly one {marker!r}, found {len(matches)}")
        if matches[0] <= previous:
            raise AssertionError(f"marker order violation at {marker!r}")
        previous = matches[0]


def assert_no_forbidden_evidence(serial: str, qemu_output: str) -> None:
    evidence = serial + "\n" + qemu_output
    for marker in FORBIDDEN_EVIDENCE:
        if marker in evidence:
            raise AssertionError(f"forbidden ARP acceptance evidence: {marker}")
    VIRTIO_NET.assert_no_storage_path_markers(evidence)


def assert_runner_success(returncode: int | None, qemu_output: str) -> None:
    outcomes = [line for line in qemu_output.splitlines() if "QEMU_OUTCOME" in line]
    if returncode != 0:
        raise AssertionError(f"QEMU runner failed with {returncode}: {outcomes!r}")
    if outcomes != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcomes!r}")


def assert_arp_acceptance(serial: str, qemu_output: str) -> None:
    assert_exact_ordered_markers(serial)
    assert_no_forbidden_evidence(serial, qemu_output)
    assert_runner_success(0, qemu_output)


def assert_peer_exchange(peer: ArpPeer) -> None:
    if peer.error is not None:
        raise AssertionError(f"ARP frame peer failed: {peer.error}") from peer.error
    if not peer.connected:
        raise AssertionError("QEMU did not connect to the ARP frame peer")
    if peer.tx_frame is None or peer.rx_frame is None:
        raise AssertionError("ARP frame peer did not exchange both frames")
    if not peer.completed:
        raise AssertionError("ARP frame peer did not reach clean completion")
    assert_exact_arp_request(DESCRIBED_DEVICE_MAC, peer.tx_frame)
    assert_exact_arp_reply(DESCRIBED_DEVICE_MAC, peer.rx_frame)


def assert_live_timeline(timeline: AcceptanceTimeline) -> None:
    expected_sources = (
        *( ("COM2", marker) for marker in CONSUMER_MARKERS),
        *( ("COM1", marker) for marker in KERNEL_MARKERS),
        ("RUNNER", "QEMU_OUTCOME success"),
    )
    for source, marker in expected_sources:
        if timeline.count(source, marker) != 1:
            raise AssertionError(f"timeline expected one {(source, marker)!r}")
    for before, after in zip(expected_sources, expected_sources[1:]):
        timeline.assert_before(*before, *after)


def find_free_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def finalize_com2_transcript(collector: Com2Collector, timeout: float = 5.0) -> str:
    """Drain COM2 after runner exit so late duplicate markers cannot be missed."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            chunk = collector.sock.recv(512)
        except socket.timeout:
            continue
        except ConnectionResetError:
            if collector.remainder:
                complete = collector.remainder.rstrip(b"\r").decode("utf-8", errors="replace")
                collector.complete_lines.append(complete)
                collector.timeline.record("COM2", complete)
                collector.remainder = b""
            return "\n".join(collector.complete_lines)
        if not chunk:
            if collector.remainder:
                complete = collector.remainder.rstrip(b"\r").decode("utf-8", errors="replace")
                collector.complete_lines.append(complete)
                collector.timeline.record("COM2", complete)
                collector.remainder = b""
            return "\n".join(collector.complete_lines)
        collector.captured.extend(chunk)
        collector._record_complete_lines(chunk)
    raise AssertionError("COM2 did not close after the QEMU runner exited")


def assert_image_preflight(esp: Path = ROOT / "image" / "esp") -> None:
    for relative_path in (
        Path("EFI") / "BOOT" / "BOOTX64.EFI",
        Path("PYTHOS") / "PYTHCORE.ELF",
        Path("PYTHOS") / "INIT.PAK",
    ):
        artifact = esp / relative_path
        if not artifact.is_file():
            raise AssertionError(f"expected ESP artifact is missing: {artifact}")


def run(command: list[str]) -> str:
    print("+ " + " ".join(command), flush=True)
    result = subprocess.run(
        command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False
    )
    print(result.stdout, end="")
    if result.returncode != 0:
        raise AssertionError(f"command failed ({result.returncode}): {' '.join(command)}")
    return result.stdout


def build_probe_image() -> tuple[Path, Path, Path, Path]:
    loader = ROOT / "target" / "x86_64-unknown-uefi" / "debug" / "bootx64.efi"
    kernel = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"
    probe = ROOT / "target" / "arp-probe" / "x86_64-unknown-none" / "debug" / "pythos-user-arp-probe"
    shell = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none", "--no-default-features", "--features", "arp-probe"])
    run([sys.executable, "scripts/build-arp-probe.py"])
    run([sys.executable, "scripts/verify-user-elf.py", "--elf", str(probe.resolve())])
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-image.py", "--kernel", str(kernel.resolve()), "--arp-probe-elf", str(probe.resolve())])
    for artifact in (loader, kernel, probe, shell):
        if not artifact.is_file():
            raise AssertionError(f"expected build artifact is missing: {artifact}")
    assert_image_preflight()
    return loader, kernel, probe, shell


def probe_runner_command(peer_port: int, shell_port: int) -> list[str]:
    return [
        sys.executable, "scripts/run-qemu.py", "--serial-log", str(SERIAL_LOG),
        "--success-marker", SUCCESS_MARKER, "--timeout", str(int(QEMU_TIMEOUT_SECONDS)),
        "--no-audio-device", "--no-virtio-blk", "--virtio-net", "--virtio-net-peer-port",
        str(peer_port), "--shell-port", str(shell_port), "--expect-outcome", "success",
    ]


def wait_for_runner_exit(process: subprocess.Popen[str], timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while process.poll() is None and time.monotonic() < deadline:
        time.sleep(0.05)
    if process.poll() is None:
        raise AssertionError("QEMU runner exceeded the bounded acceptance deadline")


def run_probe_boot() -> tuple[str, str, str, ArpPeer]:
    TARGET.mkdir(parents=True, exist_ok=True)
    for path in (SERIAL_LOG, ESP_IMAGE):
        if path.exists():
            path.unlink()
    peer = ArpPeer(timeout=QEMU_TIMEOUT_SECONDS)
    shell_port = find_free_loopback_port()
    runner = capture = observer = collector = com2 = None
    consumer_serial = kernel_serial = qemu_output = ""
    cleanup_errors: list[BaseException] = []
    try:
        peer.start()
        popen_kwargs: dict[str, object] = {"cwd": ROOT}
        if sys.platform != "win32":
            popen_kwargs["start_new_session"] = True
        command = probe_runner_command(peer.port, shell_port)
        print("+ " + " ".join(command), flush=True)
        runner = spawn_runner_process(command, **popen_kwargs)
        timeline = AcceptanceTimeline()
        capture = RunnerCapture(runner.process, timeline)
        observer = Com1Observer(SerialTail(SERIAL_LOG, timeline))
        capture.start()
        observer.start()
        com2 = connect_com2(shell_port, QEMU_TIMEOUT_SECONDS)
        collector = Com2Collector(com2, timeline)
        collector.read_until(CONSUMER_MARKERS[-1].encode("utf-8"), QEMU_TIMEOUT_SECONDS)
        observer.wait_for(KERNEL_MARKERS, QEMU_TIMEOUT_SECONDS, runner.process, capture)
        wait_for_runner_exit(runner.process, QEMU_TIMEOUT_SECONDS + 5.0)
        observer.stop_join()
        kernel_serial = observer.serial.transcript()
        observer = None
        consumer_serial = finalize_com2_transcript(collector)
        set_abortive_close(com2)
        com2.close()
        com2 = None
        qemu_output = capture.finish()
        assert_arp_acceptance(consumer_serial + "\n" + kernel_serial, qemu_output)
        assert_runner_success(runner.process.returncode, qemu_output)
        peer.join(timeout=5.0)
        assert_peer_exchange(peer)
        assert_live_timeline(timeline)
        return consumer_serial, kernel_serial, qemu_output, peer
    except BaseException as error:
        if capture is not None:
            qemu_output = capture.text()
        if observer is not None:
            kernel_serial = observer.serial.transcript()
        raise AssertionError(f"{error}\nCOM2:\n{consumer_serial}\nCOM1:\n{kernel_serial}\nrunner output:\n{qemu_output}") from error
    finally:
        if observer is not None:
            try:
                observer.stop_join()
            except BaseException as error:
                cleanup_errors.append(error)
        if com2 is not None:
            try:
                set_abortive_close(com2)
                com2.close()
            except OSError as error:
                cleanup_errors.append(error)
        if runner is not None:
            try:
                cleanup_runner_process(runner)
            except BaseException as error:
                cleanup_errors.append(error)
        if capture is not None:
            try:
                capture.finish()
            except BaseException as error:
                cleanup_errors.append(error)
        peer.close()
        try:
            peer.join(timeout=1.0)
        except (RuntimeError, TimeoutError) as error:
            cleanup_errors.append(error)
        for path in (SERIAL_LOG, ESP_IMAGE):
            try:
                if path.exists():
                    path.unlink()
            except OSError as error:
                cleanup_errors.append(error)
        if cleanup_errors:
            raise AssertionError("ARP acceptance cleanup failed: " + "; ".join(str(error) for error in cleanup_errors))


class ArpAcceptanceSelfTest(unittest.TestCase):
    """No-QEMU checks for the bounded ARP acceptance boundary."""

    def valid_consumer_serial(self) -> str:
        return "\n".join(CONSUMER_MARKERS)

    def valid_kernel_serial(self) -> str:
        return "\n".join(KERNEL_MARKERS)

    def test_exact_marker_oracle_accepts_one_complete_success(self) -> None:
        assert_arp_acceptance(self.valid_consumer_serial() + "\n" + self.valid_kernel_serial(), "QEMU_OUTCOME success\n")

    def test_marker_oracle_rejects_missing_reordered_and_duplicate_markers(self) -> None:
        valid = self.valid_consumer_serial() + "\n" + self.valid_kernel_serial()
        for serial in (
            "\n".join((*REQUIRED_MARKERS[:2], *REQUIRED_MARKERS[3:])),
            "\n".join((*REQUIRED_MARKERS[:2], REQUIRED_MARKERS[3], REQUIRED_MARKERS[2], *REQUIRED_MARKERS[4:])),
            valid + "\n" + SUCCESS_MARKER,
        ):
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                assert_arp_acceptance(serial, "QEMU_OUTCOME success\n")

    def test_exact_arp_request_and_reply_are_zero_padded(self) -> None:
        request = arp_frame(BROADCAST_MAC, DESCRIBED_DEVICE_MAC, arp_payload(1, DESCRIBED_DEVICE_MAC, LOCAL_IPV4, bytes(6), PEER_IPV4))
        self.assertEqual(request, bytes.fromhex("ffffffffffff52540012345608060001080006040001525400123456c0000202000000000000c0000201") + bytes(18))
        self.assertEqual(peer_reply(DESCRIBED_DEVICE_MAC), bytes.fromhex("52540012345602000000000208060001080006040002020000000002c0000201525400123456c0000202") + bytes(18))
        assert_exact_arp_request(DESCRIBED_DEVICE_MAC, request)
        assert_exact_arp_reply(DESCRIBED_DEVICE_MAC, peer_reply(DESCRIBED_DEVICE_MAC))

    def test_request_and_reply_validators_reject_a_wrong_local_mac(self) -> None:
        wrong_mac = bytes.fromhex("525400123457")
        with self.assertRaises(AssertionError):
            assert_exact_arp_request(DESCRIBED_DEVICE_MAC, arp_frame(BROADCAST_MAC, wrong_mac, arp_payload(1, wrong_mac, LOCAL_IPV4, bytes(6), PEER_IPV4)))
        with self.assertRaises(AssertionError):
            assert_exact_arp_reply(DESCRIBED_DEVICE_MAC, peer_reply(wrong_mac))

    def test_peer_rejects_extra_transmitted_data_after_one_request(self) -> None:
        peer = ArpPeer(timeout=1.0)
        peer.start()
        request = arp_frame(BROADCAST_MAC, DESCRIBED_DEVICE_MAC, arp_payload(1, DESCRIBED_DEVICE_MAC, LOCAL_IPV4, bytes(6), PEER_IPV4))
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(encode_socket_frame(request))
                self.assertEqual(read_socket_frame(connection), peer_reply(DESCRIBED_DEVICE_MAC))
                connection.sendall(encode_socket_frame(request))
            peer.join(timeout=1.0)
            self.assertIsNotNone(peer.error)
            self.assertIn("additional TX bytes", str(peer.error))
        finally:
            peer.close()

    def test_peer_accepts_clean_abortive_completion_after_one_reply(self) -> None:
        peer = ArpPeer(timeout=1.0)
        peer.start()
        request = arp_frame(BROADCAST_MAC, DESCRIBED_DEVICE_MAC, arp_payload(1, DESCRIBED_DEVICE_MAC, LOCAL_IPV4, bytes(6), PEER_IPV4))
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(encode_socket_frame(request))
                self.assertEqual(read_socket_frame(connection), peer_reply(DESCRIBED_DEVICE_MAC))
                set_abortive_close(connection)
            peer.join(timeout=1.0)
            assert_peer_exchange(peer)
        finally:
            peer.close()

    def test_runner_oracle_rejects_non_success_outcomes(self) -> None:
        for returncode, output in ((22, "QEMU_OUTCOME timeout\n"), (1, "QEMU_OUTCOME success\n"), (0, "QEMU_OUTCOME success\nQEMU_OUTCOME success\n")):
            with self.subTest(returncode=returncode, output=output), self.assertRaises(AssertionError):
                assert_runner_success(returncode, output)

    def test_finalized_com2_transcript_rejects_a_late_duplicate_marker(self) -> None:
        reader, writer = socket.socketpair()
        collector = Com2Collector(reader, AcceptanceTimeline())
        try:
            writer.sendall((self.valid_consumer_serial() + "\n").encode("utf-8"))
            collector.read_until(CONSUMER_MARKERS[-1].encode("utf-8"), 1.0)
            writer.sendall((CONSUMER_MARKERS[-1] + "\n").encode("utf-8"))
            writer.close()
            with self.assertRaises(AssertionError):
                assert_arp_acceptance(finalize_com2_transcript(collector) + "\n" + self.valid_kernel_serial(), "QEMU_OUTCOME success\n")
        finally:
            reader.close()
            writer.close()

    def test_image_preflight_requires_all_boot_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            esp = Path(temporary_directory)
            artifacts = (esp / "EFI" / "BOOT" / "BOOTX64.EFI", esp / "PYTHOS" / "PYTHCORE.ELF", esp / "PYTHOS" / "INIT.PAK")
            for artifact in artifacts:
                artifact.parent.mkdir(parents=True, exist_ok=True)
                artifact.write_bytes(b"artifact")
            assert_image_preflight(esp)
            artifacts[-1].unlink()
            with self.assertRaises(AssertionError):
                assert_image_preflight(esp)


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(ArpAcceptanceSelfTest)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.wasSuccessful():
        print("ARP_QEMU_ACCEPTANCE_OK")
        return 0
    return 1


def main() -> int:
    loader, kernel, probe, shell = build_probe_image()
    _consumer, _kernel, _qemu_output, _peer = run_probe_boot()
    print(f"ARP_ARTIFACT loader={loader.resolve()}")
    print(f"ARP_ARTIFACT kernel={kernel.resolve()}")
    print(f"ARP_ARTIFACT probe={probe.resolve()}")
    print(f"ARP_ARTIFACT shell={shell.resolve()}")
    print(f"ARP_ARTIFACT esp={ROOT / 'image' / 'esp'}")
    print(f"ARP_ARTIFACT serial-log-cleaned={SERIAL_LOG.resolve()}")
    print("ARP_QEMU_ACCEPTANCE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-arp.py [--self-test]")
    raise SystemExit(main())
