#!/usr/bin/env python
"""Deterministic QEMU acceptance for one bounded DNS-over-UDP exchange."""

from __future__ import annotations

import importlib.util
import select
import socket
import struct
import sys
import threading
import time
import unittest
from pathlib import Path
from typing import NamedTuple


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "dns-probe-com1.log"
ESP_IMAGE = TARGET / "dns-probe-com1-esp.img"
QEMU_TIMEOUT_SECONDS = 30.0
SUCCESS_MARKER = "PYTHOS:CORE:DNS_READY"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:DNS:BOOTSTRAPPED",
    "PYTHOS:CORE:DNS:DESCRIBE_OK",
    "PYTHOS:CORE:DNS:ARP_SETUP_OK",
    "PYTHOS:CORE:DNS:QUERY_OK",
    "PYTHOS:CORE:DNS:RESPONSE_OK",
    "PYTHOS:CORE:DNS:TEARDOWN_REVOKED",
    SUCCESS_MARKER,
)
CONSUMER_MARKERS = REQUIRED_MARKERS[:5]
KERNEL_MARKERS = REQUIRED_MARKERS[5:]
FORBIDDEN_EVIDENCE = (
    "PYTHOS:CORE:DNS:ERROR",
    "PYTHOS:PANIC",
    "TIMEOUT",
    "TRANSPORT_ERROR",
    "transport-error",
    "DNS:FAILED",
)
LOCAL_MAC = bytes.fromhex("525400123456")
PEER_MAC = bytes.fromhex("020000000002")
LOCAL_IPV4 = bytes.fromhex("c0a80e02")
PEER_IPV4 = bytes.fromhex("c0a80e01")
BROADCAST_MAC = bytes(6 * b"\xff")
IPV4_ETHER_TYPE = 0x0800
ARP_ETHER_TYPE = 0x0806
IPV4_PROTOCOL_UDP = 17
IPV4_TTL = 64
LOCAL_PORT = 0x1605
PEER_PORT = 0x0035
QUERY_ID = 0xD14E
QUERY_DNS = bytes.fromhex(
    "d14e0100000100000000000006707974686f73076578616d706c650000010001"
)
RESPONSE_DNS = bytes.fromhex(
    "d14e8180000100010000000006707974686f73076578616d706c650000010001"
    "c00c000100010000003c0004c000020e"
)
QUERY_UDP_CHECKSUM = 0x8210
RESPONSE_UDP_CHECKSUM = 0x7F11
QUERY_IPV4_CHECKSUM = 0xC75C
RESPONSE_IPV4_CHECKSUM = 0xC74B
QUERY_IPV4_ID = 0x1601
RESPONSE_IPV4_ID = 0x1602
QUERY_UDP_LENGTH = 40
RESPONSE_UDP_LENGTH = 56
QUERY_IPV4_LENGTH = 60
RESPONSE_IPV4_LENGTH = 76
QUERY_FRAME_LENGTH = 74
RESPONSE_FRAME_LENGTH = 90


def load_ipv4_acceptance():
    path = ROOT / "scripts" / "test-ipv4.py"
    spec = importlib.util.spec_from_file_location("dns_ipv4_acceptance", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load test-ipv4.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


IPV4 = load_ipv4_acceptance()
AcceptanceTimeline = IPV4.AcceptanceTimeline
Com2Collector = IPV4.Com2Collector
Com1Observer = IPV4.Com1Observer
RunnerCapture = IPV4.RunnerCapture
SerialTail = IPV4.SerialTail
cleanup_runner_process = IPV4.cleanup_runner_process
encode_socket_frame = IPV4.encode_socket_frame
read_socket_frame = IPV4.read_socket_frame
set_abortive_close = IPV4.set_abortive_close
spawn_runner_process = IPV4.spawn_runner_process
finalize_com2_transcript = IPV4.finalize_com2_transcript
arp_payload = IPV4.arp_payload
arp_frame = IPV4.arp_frame
ethernet_frame = IPV4.ethernet_frame


def ones_complement_checksum(data: bytes) -> int:
    if len(data) % 2:
        data += b"\x00"
    total = 0
    for offset in range(0, len(data), 2):
        total += int.from_bytes(data[offset : offset + 2], "big")
        total = (total & 0xFFFF) + (total >> 16)
    while total >> 16:
        total = (total & 0xFFFF) + (total >> 16)
    return (~total) & 0xFFFF


def udp_datagram(source_port: int, destination_port: int, source: bytes, destination: bytes, payload: bytes) -> bytes:
    length = 8 + len(payload)
    header = struct.pack("!HHHH", source_port, destination_port, length, 0)
    pseudo = source + destination + bytes((0, IPV4_PROTOCOL_UDP)) + struct.pack("!H", length)
    checksum = ones_complement_checksum(pseudo + header + payload)
    return struct.pack("!HHHH", source_port, destination_port, length, checksum) + payload


def ipv4_datagram(identification: int, source: bytes, destination: bytes, payload: bytes) -> bytes:
    total_length = 20 + len(payload)
    header = bytearray(
        struct.pack(
            "!BBHHHBBH4s4s",
            0x45,
            0,
            total_length,
            identification,
            0,
            IPV4_TTL,
            IPV4_PROTOCOL_UDP,
            0,
            source,
            destination,
        )
    )
    header[10:12] = ones_complement_checksum(header).to_bytes(2, "big")
    return bytes(header) + payload


def arp_request() -> bytes:
    return arp_frame(
        BROADCAST_MAC,
        LOCAL_MAC,
        arp_payload(1, LOCAL_MAC, LOCAL_IPV4, bytes(6), PEER_IPV4),
    )


def arp_reply() -> bytes:
    return arp_frame(
        LOCAL_MAC,
        PEER_MAC,
        arp_payload(2, PEER_MAC, PEER_IPV4, LOCAL_MAC, LOCAL_IPV4),
    )


def dns_ethernet_frame(destination: bytes, source: bytes, payload: bytes) -> bytes:
    if len(destination) != 6 or len(source) != 6:
        raise ValueError("Ethernet addresses must be six bytes")
    return destination + source + struct.pack("!H", IPV4_ETHER_TYPE) + payload


def dns_query_frame() -> bytes:
    udp = udp_datagram(LOCAL_PORT, PEER_PORT, LOCAL_IPV4, PEER_IPV4, QUERY_DNS)
    if int.from_bytes(udp[6:8], "big") != QUERY_UDP_CHECKSUM:
        raise AssertionError("DNS query UDP checksum profile is inconsistent")
    ip = ipv4_datagram(QUERY_IPV4_ID, LOCAL_IPV4, PEER_IPV4, udp)
    if int.from_bytes(ip[10:12], "big") != QUERY_IPV4_CHECKSUM:
        raise AssertionError("DNS query IPv4 checksum profile is inconsistent")
    frame = dns_ethernet_frame(PEER_MAC, LOCAL_MAC, ip)
    if len(frame) != QUERY_FRAME_LENGTH:
        raise AssertionError("DNS query frame length is not the exact profile")
    return frame


def dns_response_frame() -> bytes:
    udp = udp_datagram(PEER_PORT, LOCAL_PORT, PEER_IPV4, LOCAL_IPV4, RESPONSE_DNS)
    if int.from_bytes(udp[6:8], "big") != RESPONSE_UDP_CHECKSUM:
        raise AssertionError("DNS response UDP checksum profile is inconsistent")
    ip = ipv4_datagram(RESPONSE_IPV4_ID, PEER_IPV4, LOCAL_IPV4, udp)
    if int.from_bytes(ip[10:12], "big") != RESPONSE_IPV4_CHECKSUM:
        raise AssertionError("DNS response IPv4 checksum profile is inconsistent")
    frame = dns_ethernet_frame(LOCAL_MAC, PEER_MAC, ip)
    if len(frame) != RESPONSE_FRAME_LENGTH:
        raise AssertionError("DNS response frame length is not the exact profile")
    return frame


def expected_frames() -> list[bytes]:
    return [arp_request(), arp_reply(), dns_query_frame(), dns_response_frame()]


def _assert_exact(expected: bytes, actual: bytes, description: str) -> None:
    if actual != expected:
        raise AssertionError(f"unexpected {description}: {actual.hex()}")


def assert_dns_payload_fields(query: bytes, response: bytes) -> None:
    if len(query) != 32 or len(response) != 48:
        raise AssertionError("DNS payload lengths are not the exact query/response profile")
    if query[0:2] != struct.pack("!H", QUERY_ID) or query[2:4] != b"\x01\x00":
        raise AssertionError("DNS query ID or flags are invalid")
    if query[4:12] != b"\x00\x01\x00\x00\x00\x00\x00\x00":
        raise AssertionError("DNS query counts are invalid")
    if query[12:28] != b"\x06pythos\x07example\x00":
        raise AssertionError("DNS query labels are invalid")
    if query[28:32] != b"\x00\x01\x00\x01":
        raise AssertionError("DNS query type or class is invalid")
    if response[0:2] != struct.pack("!H", QUERY_ID) or response[2:4] != b"\x81\x80":
        raise AssertionError("DNS response ID or flags are invalid")
    if response[4:12] != b"\x00\x01\x00\x01\x00\x00\x00\x00":
        raise AssertionError("DNS response counts are invalid")
    if response[12:32] != query[12:32]:
        raise AssertionError("DNS response question is not byte-identical")
    if response[32:34] != b"\xc0\x0c" or response[34:38] != b"\x00\x01\x00\x01":
        raise AssertionError("DNS answer pointer, type, or class is invalid")
    if response[38:44] != b"\x00\x00\x00\x3c\x00\x04":
        raise AssertionError("DNS answer TTL or RDATA length is invalid")
    if response[44:48] != bytes.fromhex("c000020e"):
        raise AssertionError("DNS A answer is not 192.0.2.14")


def assert_exact_exchange(frames: list[bytes]) -> None:
    expected = expected_frames()
    if len(frames) != len(expected):
        raise AssertionError(f"expected exactly four Ethernet frames, got {len(frames)}")
    for index, (actual, wanted) in enumerate(zip(frames, expected)):
        _assert_exact(wanted, actual, f"frame {index + 1}")
    if [len(frame) for frame in frames] != [60, 60, QUERY_FRAME_LENGTH, RESPONSE_FRAME_LENGTH]:
        raise AssertionError("DNS frame lengths are not the exact four-frame profile")
    if frames[2][14 + 20 + 8 + 0 : 14 + 20 + 8 + len(QUERY_DNS)] != QUERY_DNS:
        raise AssertionError("DNS query payload is not byte-identical")
    if frames[3][14 + 20 + 8 : 14 + 20 + 8 + len(RESPONSE_DNS)] != RESPONSE_DNS:
        raise AssertionError("DNS response payload is not byte-identical")
    assert_dns_payload_fields(QUERY_DNS, RESPONSE_DNS)


def assert_peer_exchange(peer: "DnsPeer") -> None:
    if peer.error is not None:
        raise AssertionError(f"DNS frame peer failed: {peer.error}") from peer.error
    if not peer.connected or not peer.completed:
        raise AssertionError("DNS frame peer did not connect and finish cleanly")
    if len(peer.tx_frames) != 2 or len(peer.rx_frames) != 2:
        raise AssertionError(
            f"DNS frame peer did not exchange four frames: TX={len(peer.tx_frames)} RX={len(peer.rx_frames)}"
        )
    assert_exact_exchange(peer.tx_frames[:1] + peer.rx_frames[:1] + peer.tx_frames[1:] + peer.rx_frames[1:])


def assert_ordered_markers(serial: str) -> None:
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
            raise AssertionError(f"forbidden DNS acceptance evidence: {marker}")
    IPV4.VIRTIO_NET.assert_no_storage_path_markers(evidence)


def assert_runner_success(returncode: int | None, qemu_output: str) -> None:
    outcomes = [line for line in qemu_output.splitlines() if "QEMU_OUTCOME" in line]
    if returncode != 0 or outcomes != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome, returncode={returncode}, outcomes={outcomes!r}")


def assert_dns_acceptance(serial: str, qemu_output: str) -> None:
    assert_ordered_markers(serial)
    assert_no_forbidden_evidence(serial, qemu_output)
    assert_runner_success(0, qemu_output)


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


class DnsPeer:
    """Loopback-only host peer for exactly two ARP and two DNS frames."""

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
        self.tx_frames: list[bytes] = []
        self.rx_frames: list[bytes] = []
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        if self._thread is not None:
            raise RuntimeError("DNS peer is already started")
        self._thread = threading.Thread(target=self._serve_once, daemon=True)
        self._thread.start()

    def join(self, timeout: float | None = None) -> None:
        if self._thread is None:
            raise RuntimeError("DNS peer has not been started")
        self._thread.join(timeout)
        if self._thread.is_alive():
            raise TimeoutError("DNS peer did not finish")

    def close(self) -> None:
        self.listener.close()

    def _serve_once(self) -> None:
        try:
            with self.listener.accept()[0] as connection:
                self.connected = True
                connection.settimeout(self.timeout)
                request = read_socket_frame(connection)
                self.tx_frames.append(request)
                _assert_exact(arp_request(), request, "ARP request")
                reply = arp_reply()
                self.rx_frames.append(reply)
                connection.sendall(encode_socket_frame(reply))

                request = read_socket_frame(connection)
                self.tx_frames.append(request)
                _assert_exact(dns_query_frame(), request, "DNS query")
                reply = dns_response_frame()
                self.rx_frames.append(reply)
                connection.sendall(encode_socket_frame(reply))

                deadline = time.monotonic() + self.timeout
                while True:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or not select.select([connection], [], [], remaining)[0]:
                        raise TimeoutError("DNS peer did not reach clean completion after response")
                    try:
                        if connection.recv(4096):
                            raise AssertionError("DNS peer received an additional frame")
                    except ConnectionResetError:
                        pass
                    self.completed = True
                    break
        except BaseException as error:
            self.error = error
        finally:
            self.listener.close()


def find_free_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


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


def build_probe_image() -> tuple[Path, Path, Path, Path]:
    loader = ROOT / "target" / "x86_64-unknown-uefi" / "debug" / "bootx64.efi"
    kernel = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"
    probe = ROOT / "target" / "dns-probe.elf"
    shell = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
    IPV4.run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    IPV4.run([
        "cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none",
        "--no-default-features", "--features", "dns-probe",
    ])
    IPV4.run([sys.executable, "scripts/build-dns-probe.py"])
    IPV4.run([sys.executable, "scripts/verify-user-elf.py", "--elf", str(probe.resolve())])
    IPV4.run([sys.executable, "scripts/build-user-shell.py"])
    IPV4.run([sys.executable, "scripts/verify-user-elf.py"])
    IPV4.run([
        sys.executable,
        "scripts/build-image.py",
        "--kernel",
        str(kernel.resolve()),
        "--dns-probe-elf",
        str(probe.resolve()),
    ])
    for artifact in (loader, kernel, probe, shell):
        if not artifact.is_file():
            raise AssertionError(f"expected build artifact is missing: {artifact}")
    IPV4.assert_image_preflight()
    return loader, kernel, probe, shell


IPV4.SERIAL_LOG = SERIAL_LOG
IPV4.ESP_IMAGE = ESP_IMAGE
IPV4.QEMU_TIMEOUT_SECONDS = QEMU_TIMEOUT_SECONDS
IPV4.REQUIRED_MARKERS = REQUIRED_MARKERS
IPV4.CONSUMER_MARKERS = CONSUMER_MARKERS
IPV4.KERNEL_MARKERS = KERNEL_MARKERS
IPV4.Ipv4Peer = DnsPeer
IPV4.probe_runner_command = probe_runner_command
IPV4.assert_ipv4_acceptance = assert_dns_acceptance
IPV4.assert_runner_success = assert_runner_success
IPV4.assert_peer_exchange = assert_peer_exchange
IPV4.assert_live_timeline = assert_live_timeline
run_probe_boot = IPV4.run_probe_boot


class DnsAcceptanceSelfTest(unittest.TestCase):
    def valid_serial(self) -> str:
        return "\n".join(REQUIRED_MARKERS)

    def test_exact_frames_and_wire_profile(self) -> None:
        frames = expected_frames()
        assert_exact_exchange(frames)
        self.assertEqual([len(frame) for frame in frames], [60, 60, 74, 90])
        self.assertEqual(int.from_bytes(frames[2][14 + 20 + 6 : 14 + 20 + 8], "big"), QUERY_UDP_CHECKSUM)
        self.assertEqual(int.from_bytes(frames[3][14 + 20 + 6 : 14 + 20 + 8], "big"), RESPONSE_UDP_CHECKSUM)
        self.assertEqual(int.from_bytes(frames[2][14 + 10 : 14 + 12], "big"), QUERY_IPV4_CHECKSUM)
        self.assertEqual(int.from_bytes(frames[3][14 + 10 : 14 + 12], "big"), RESPONSE_IPV4_CHECKSUM)

    def test_wrong_order_extra_and_corrupt_frames_fail(self) -> None:
        frames = expected_frames()
        with self.assertRaises(AssertionError):
            assert_exact_exchange(frames[:1] + frames[2:])
        with self.assertRaises(AssertionError):
            assert_exact_exchange(frames[:2] + [frames[3], frames[2]])
        with self.assertRaises(AssertionError):
            assert_exact_exchange(frames + [frames[-1]])
        corrupt = bytearray(frames[3])
        corrupt[-1] ^= 1
        with self.assertRaises(AssertionError):
            assert_exact_exchange(frames[:3] + [bytes(corrupt)])

    def test_markers_outcome_storage_and_forbidden_evidence_are_exact(self) -> None:
        assert_dns_acceptance(self.valid_serial(), "QEMU_OUTCOME success\n")
        with self.assertRaises(AssertionError):
            assert_dns_acceptance(self.valid_serial() + "\n" + SUCCESS_MARKER, "QEMU_OUTCOME success\n")
        with self.assertRaises(AssertionError):
            assert_dns_acceptance("\n".join((*REQUIRED_MARKERS[:3], *REQUIRED_MARKERS[4:])), "QEMU_OUTCOME success\n")
        with self.assertRaises(AssertionError):
            assert_dns_acceptance(self.valid_serial(), "QEMU_OUTCOME success\nQEMU_OUTCOME success\n")
        with self.assertRaises(AssertionError):
            assert_dns_acceptance(self.valid_serial() + "\nPYTHOS:CORE:BLOCK_DEVICE_READY", "QEMU_OUTCOME success\n")

    def test_runner_requires_network_only_profile(self) -> None:
        command = probe_runner_command(4595, 4596)
        self.assertIn("--no-virtio-blk", command)
        self.assertIn("--virtio-net", command)
        self.assertEqual(command[command.index("--virtio-net-peer-port") + 1], "4595")
        self.assertEqual(command[command.index("--shell-port") + 1], "4596")
        self.assertEqual(command[command.index("--expect-outcome") + 1], "success")


def run_self_tests() -> int:
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(DnsAcceptanceSelfTest)
    )
    if result.wasSuccessful():
        print("DNS_QEMU_ACCEPTANCE_OK")
        return 0
    return 1


def main() -> int:
    loader, kernel, probe, shell = build_probe_image()
    consumer, kernel_serial, qemu_output, peer = run_probe_boot()
    print(f"DNS_LIVE_FRAMES tx={len(peer.tx_frames)} rx={len(peer.rx_frames)} total={len(peer.tx_frames) + len(peer.rx_frames)}")
    observed_markers = consumer.splitlines() + kernel_serial.splitlines()
    marker_evidence = [line for line in observed_markers if line in REQUIRED_MARKERS]
    print(f"DNS_LIVE_MARKERS {' > '.join(marker_evidence)}")
    print(f"DNS_LIVE_OUTCOME {next(line for line in qemu_output.splitlines() if line == 'QEMU_OUTCOME success')}")
    print(f"DNS_LIVE_CLEANUP serial_log_exists={SERIAL_LOG.exists()} esp_snapshot_exists={ESP_IMAGE.exists()}")
    print(f"DNS_ARTIFACT loader={loader.resolve()}")
    print(f"DNS_ARTIFACT kernel={kernel.resolve()}")
    print(f"DNS_ARTIFACT probe={probe.resolve()}")
    print(f"DNS_ARTIFACT shell={shell.resolve()}")
    print(f"DNS_ARTIFACT esp={ROOT / 'image' / 'esp'}")
    print(f"DNS_ARTIFACT serial-log-cleaned={SERIAL_LOG.resolve()}")
    print("DNS_QEMU_ACCEPTANCE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-dns.py [--self-test]")
    raise SystemExit(main())
