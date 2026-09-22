#!/usr/bin/env python
"""Deterministic QEMU acceptance for one bounded ARP/TCP stream exchange."""

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
SERIAL_LOG = TARGET / "tcp-probe-com1.log"
ESP_IMAGE = TARGET / "tcp-probe-com1-esp.img"
QEMU_TIMEOUT_SECONDS = 30.0
SUCCESS_MARKER = "PYTHOS:CORE:TCP_READY"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:TCP:BOOTSTRAPPED",
    "PYTHOS:CORE:TCP:DESCRIBE_OK",
    "PYTHOS:CORE:TCP:ARP_SETUP_OK",
    "PYTHOS:CORE:TCP:HANDSHAKE_OK",
    "PYTHOS:CORE:TCP:TX_OK",
    "PYTHOS:CORE:TCP:RX_OK",
    "PYTHOS:CORE:TCP:CLOSE_OK",
    "PYTHOS:CORE:TCP:TEARDOWN_REVOKED",
    SUCCESS_MARKER,
)
CONSUMER_MARKERS = REQUIRED_MARKERS[:7]
KERNEL_MARKERS = REQUIRED_MARKERS[7:]
FORBIDDEN_EVIDENCE = (
    "PYTHOS:CORE:TCP:ERROR",
    "PYTHOS:PANIC",
    "TIMEOUT",
    "TRANSPORT_ERROR",
    "transport-error",
    "TCP:FAILED",
)
PEER_MAC = bytes.fromhex("020000000002")
DESCRIBED_DEVICE_MAC = bytes.fromhex("525400123456")
LOCAL_IPV4 = bytes.fromhex("c0a80e02")
PEER_IPV4 = bytes.fromhex("c0a80e01")
BROADCAST_MAC = bytes(6 * b"\xff")
IPV4_ETHER_TYPE = 0x0800
IPV4_PROTOCOL = 6
TCP_PROTOCOL = IPV4_PROTOCOL
IPV4_TTL = 64
LOCAL_PORT = 0x1505
PEER_PORT = 0x1506
LOCAL_ISS = 0x15050000
PEER_ISS = 0x25060000
WINDOW = 0x1000
TCP_SOURCE_PORT = LOCAL_PORT
TCP_DESTINATION_PORT = PEER_PORT
TCP_LOCAL_ISS = LOCAL_ISS
TCP_PEER_ISS = PEER_ISS
TCP_WINDOW = WINDOW
TCP_MSS = 1024
MSS_OPTION = bytes.fromhex("02040400")
REQUEST_DATA = b"PYTCPQ"
REPLY_DATA = b"PYTCPR"
MIN_ETHERNET_FRAME_BYTES = 60
ARP_FRAME_COUNT = 2
TCP_FRAME_COUNT = 10
SYN = 0x02
FIN = 0x01
RST = 0x04
ACK = 0x10


def load_ipv4_acceptance():
    path = ROOT / "scripts" / "test-ipv4.py"
    spec = importlib.util.spec_from_file_location("tcp_ipv4_acceptance", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load test-ipv4.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


IPV4 = load_ipv4_acceptance()
VIRTIO_NET = IPV4.VIRTIO_NET
AcceptanceTimeline = IPV4.AcceptanceTimeline
Com2Collector = IPV4.Com2Collector
cleanup_runner_process = IPV4.cleanup_runner_process
encode_socket_frame = IPV4.encode_socket_frame
read_socket_frame = IPV4.read_socket_frame
set_abortive_close = IPV4.set_abortive_close
spawn_runner_process = IPV4.spawn_runner_process
finalize_com2_transcript = IPV4.finalize_com2_transcript
assert_image_preflight = IPV4.assert_image_preflight
run = IPV4.run
arp_payload = IPV4.arp_payload
arp_frame = IPV4.arp_frame
ethernet_frame = IPV4.ethernet_frame


def ones_complement_checksum(data: bytes) -> int:
    if len(data) % 2:
        raise ValueError("checksum input must contain an even number of bytes")
    total = 0
    for offset in range(0, len(data), 2):
        total += int.from_bytes(data[offset : offset + 2], "big")
        total = (total & 0xFFFF) + (total >> 16)
    while total >> 16:
        total = (total & 0xFFFF) + (total >> 16)
    return (~total) & 0xFFFF


def ipv4_header(identification: int, source: bytes, destination: bytes, payload: bytes) -> bytes:
    if not 0 <= identification <= 0xFFFF:
        raise ValueError("IPv4 identification must fit in sixteen bits")
    if len(source) != 4 or len(destination) != 4:
        raise ValueError("IPv4 addresses must be four bytes")
    total_length = 20 + len(payload)
    if total_length > 0xFFFF:
        raise ValueError("IPv4 datagram exceeds the sixteen-bit total length")
    header = bytearray(struct.pack("!BBHHHBBH4s4s", 0x45, 0, total_length, identification, 0, IPV4_TTL, IPV4_PROTOCOL, 0, source, destination))
    header[10:12] = ones_complement_checksum(header).to_bytes(2, "big")
    return bytes(header)


class ParsedIpv4(NamedTuple):
    identification: int
    flags_fragment_offset: int
    ttl: int
    protocol: int
    checksum: int
    source: bytes
    destination: bytes
    total_length: int
    payload: bytes


def parse_ipv4_datagram(datagram: bytes) -> ParsedIpv4:
    if len(datagram) < 20:
        raise AssertionError("IPv4 datagram is shorter than the fixed header")
    if datagram[0] != 0x45:
        raise AssertionError("IPv4 version/IHL is not canonical")
    total_length = int.from_bytes(datagram[2:4], "big")
    if total_length != len(datagram) or total_length < 20:
        raise AssertionError("IPv4 total length does not match the supplied datagram")
    if ones_complement_checksum(datagram[:20]) != 0:
        raise AssertionError("IPv4 header checksum is invalid")
    flags_fragment_offset = int.from_bytes(datagram[6:8], "big")
    if flags_fragment_offset != 0:
        raise AssertionError("fragmented IPv4 datagrams are rejected")
    if datagram[8] != IPV4_TTL or datagram[9] != IPV4_PROTOCOL:
        raise AssertionError("IPv4 TTL or protocol is not the accepted TCP profile")
    return ParsedIpv4(
        int.from_bytes(datagram[4:6], "big"),
        flags_fragment_offset,
        datagram[8],
        datagram[9],
        int.from_bytes(datagram[10:12], "big"),
        datagram[12:16],
        datagram[16:20],
        total_length,
        datagram[20:],
    )


def tcp_checksum(source: bytes, destination: bytes, segment: bytes) -> int:
    if len(source) != 4 or len(destination) != 4:
        raise ValueError("TCP pseudo-header addresses must be four bytes")
    if len(segment) > 0xFFFF:
        raise ValueError("TCP segment is too long")
    value = source + destination + bytes((0, IPV4_PROTOCOL)) + len(segment).to_bytes(2, "big") + segment
    if len(value) % 2:
        value += b"\x00"
    return ones_complement_checksum(value)


class SegmentSpec(NamedTuple):
    direction: str
    identification: int
    flags: int
    sequence: int
    acknowledgment: int
    data: bytes
    ip_checksum: int
    tcp_checksum: int
    padding: int


TCP_SEGMENTS = (
    SegmentSpec("local", 0x1501, SYN, 0x15050000, 0, b"", 0xC877, 0xAD76, 2),
    SegmentSpec("peer", 0x1502, SYN | ACK, 0x25060000, 0x15050001, b"", 0xC876, 0x885F, 2),
    SegmentSpec("local", 0x1503, ACK, 0x15050001, 0x25060001, b"", 0xC879, 0x9E68, 6),
    SegmentSpec("local", 0x1504, ACK, 0x15050001, 0x25060001, REQUEST_DATA, 0xC872, 0xA974, 0),
    SegmentSpec("peer", 0x1505, ACK, 0x25060001, 0x15050007, REPLY_DATA, 0xC871, 0xA96D, 0),
    SegmentSpec("local", 0x1506, ACK, 0x15050007, 0x25060007, b"", 0xC876, 0x9E5C, 6),
    SegmentSpec("local", 0x1507, FIN | ACK, 0x15050007, 0x25060007, b"", 0xC875, 0x9E5B, 6),
    SegmentSpec("peer", 0x1508, ACK, 0x25060007, 0x15050008, b"", 0xC874, 0x9E5B, 6),
    SegmentSpec("peer", 0x1509, FIN | ACK, 0x25060007, 0x15050008, b"", 0xC873, 0x9E5A, 6),
    SegmentSpec("local", 0x150A, ACK, 0x15050008, 0x25060008, b"", 0xC872, 0x9E5A, 6),
)


class ParsedTcp(NamedTuple):
    source_port: int
    destination_port: int
    sequence: int
    acknowledgment: int
    flags: int
    window: int
    checksum: int
    urgent_pointer: int
    options: bytes
    data: bytes


def tcp_datagram(
    source_port: int,
    destination_port: int,
    sequence: int,
    acknowledgment: int,
    flags: int,
    source: bytes,
    destination: bytes,
    data: bytes = b"",
    options: bytes = b"",
) -> bytes:
    if flags & SYN:
        if options != MSS_OPTION:
            raise ValueError("SYN segments require the exact MSS option")
    elif options:
        raise ValueError("ordinary TCP segments cannot carry options")
    if len(options) % 4 or len(options) > 40:
        raise ValueError("TCP options must be a bounded four-byte multiple")
    header = bytearray(struct.pack("!HHIIBBHHH", source_port, destination_port, sequence, acknowledgment, (5 + len(options) // 4) << 4, flags, WINDOW, 0, 0))
    header.extend(options)
    header[16:18] = tcp_checksum(source, destination, bytes(header) + data).to_bytes(2, "big")
    return bytes(header) + data


def parse_tcp_segment(segment: bytes, source: bytes, destination: bytes, expected: SegmentSpec | None = None) -> ParsedTcp:
    if len(segment) < 20:
        raise AssertionError("TCP segment is shorter than the fixed header")
    data_offset = segment[12] >> 4
    if data_offset not in (5, 6) or data_offset * 4 > len(segment):
        raise AssertionError("TCP data offset is outside the canonical profile")
    if segment[12] & 0x0F:
        raise AssertionError("TCP reserved data-offset bits are nonzero")
    header_length = data_offset * 4
    options = segment[20:header_length]
    flags = segment[13]
    if flags & ~(SYN | FIN | RST | ACK) or flags & RST:
        raise AssertionError("TCP flags are outside the finite non-RST profile")
    if data_offset == 6:
        if flags & SYN == 0 or options != MSS_OPTION:
            raise AssertionError("SYN option is not the exact MSS option")
    elif options:
        raise AssertionError("ordinary TCP segment unexpectedly carries options")
    if tcp_checksum(source, destination, segment) != 0:
        raise AssertionError("TCP pseudo-header checksum is invalid")
    parsed = ParsedTcp(
        int.from_bytes(segment[0:2], "big"),
        int.from_bytes(segment[2:4], "big"),
        int.from_bytes(segment[4:8], "big"),
        int.from_bytes(segment[8:12], "big"),
        flags,
        int.from_bytes(segment[14:16], "big"),
        int.from_bytes(segment[16:18], "big"),
        int.from_bytes(segment[18:20], "big"),
        options,
        segment[header_length:],
    )
    if parsed.window != WINDOW or parsed.urgent_pointer != 0 or parsed.flags & 0xE0:
        raise AssertionError("TCP window, reserved flags, or urgent pointer is invalid")
    if expected is not None:
        source_port = LOCAL_PORT if expected.direction == "local" else PEER_PORT
        destination_port = PEER_PORT if expected.direction == "local" else LOCAL_PORT
        if (parsed.source_port, parsed.destination_port, parsed.sequence, parsed.acknowledgment, parsed.flags, parsed.data) != (
            source_port,
            destination_port,
            expected.sequence,
            expected.acknowledgment,
            expected.flags,
            expected.data,
        ):
            raise AssertionError("TCP segment does not match the expected sequence transition")
        if parsed.checksum != expected.tcp_checksum:
            raise AssertionError("TCP checksum is not the accepted value")
    return parsed


def tcp_frame(device_mac: bytes, spec: SegmentSpec) -> bytes:
    if spec.direction == "local":
        destination, source, source_ipv4, destination_ipv4 = PEER_MAC, device_mac, LOCAL_IPV4, PEER_IPV4
        source_port, destination_port = LOCAL_PORT, PEER_PORT
    else:
        destination, source, source_ipv4, destination_ipv4 = device_mac, PEER_MAC, PEER_IPV4, LOCAL_IPV4
        source_port, destination_port = PEER_PORT, LOCAL_PORT
    options = MSS_OPTION if spec.flags & SYN else b""
    segment = tcp_datagram(source_port, destination_port, spec.sequence, spec.acknowledgment, spec.flags, source_ipv4, destination_ipv4, spec.data, options)
    frame = ethernet_frame(destination, source, IPV4_ETHER_TYPE, ipv4_header(spec.identification, source_ipv4, destination_ipv4, segment) + segment)
    if len(frame) != MIN_ETHERNET_FRAME_BYTES or (spec.padding and frame[-spec.padding:] != bytes(spec.padding)):
        raise AssertionError("TCP frame did not produce the exact minimum-frame padding")
    return frame


def tcp_frames(device_mac: bytes) -> list[bytes]:
    return [tcp_frame(device_mac, spec) for spec in TCP_SEGMENTS]


def arp_request(device_mac: bytes) -> bytes:
    return arp_frame(BROADCAST_MAC, device_mac, arp_payload(1, device_mac, LOCAL_IPV4, bytes(6), PEER_IPV4))


def arp_reply(device_mac: bytes) -> bytes:
    return arp_frame(device_mac, PEER_MAC, arp_payload(2, PEER_MAC, PEER_IPV4, device_mac, LOCAL_IPV4))


def assert_exact_arp_request(device_mac: bytes, frame: bytes) -> None:
    _assert_exact(arp_request(device_mac), frame, "ARP request")


def assert_exact_arp_reply(device_mac: bytes, frame: bytes) -> None:
    _assert_exact(arp_reply(device_mac), frame, "ARP reply")


def exchange_frames(device_mac: bytes) -> list[bytes]:
    segments = tcp_frames(device_mac)
    return [arp_request(device_mac), arp_reply(device_mac), segments[0], segments[1], segments[2], segments[3], segments[4], segments[5], segments[6], segments[7], segments[8], segments[9]]


def parse_tcp_frame(frame: bytes, device_mac: bytes, index: int) -> ParsedTcp:
    if not 0 <= index < TCP_FRAME_COUNT:
        raise ValueError("TCP segment index is outside the ten-frame profile")
    spec = TCP_SEGMENTS[index]
    destination = PEER_MAC if spec.direction == "local" else device_mac
    source = device_mac if spec.direction == "local" else PEER_MAC
    if len(frame) != MIN_ETHERNET_FRAME_BYTES or frame[:6] != destination or frame[6:12] != source or int.from_bytes(frame[12:14], "big") != IPV4_ETHER_TYPE:
        raise AssertionError("Ethernet direction, addresses, type, or length is invalid")
    datagram = frame[14 : 14 + 20 + 20 + len(spec.data) + (len(MSS_OPTION) if spec.flags & SYN else 0)]
    parsed_ipv4 = parse_ipv4_datagram(datagram)
    if parsed_ipv4.identification != spec.identification or parsed_ipv4.source != (LOCAL_IPV4 if spec.direction == "local" else PEER_IPV4) or parsed_ipv4.destination != (PEER_IPV4 if spec.direction == "local" else LOCAL_IPV4):
        raise AssertionError("IPv4 identity or direction is invalid")
    parsed_tcp = parse_tcp_segment(parsed_ipv4.payload, parsed_ipv4.source, parsed_ipv4.destination, spec)
    if parsed_ipv4.checksum != spec.ip_checksum or len(frame) - 14 - parsed_ipv4.total_length != spec.padding:
        raise AssertionError("IPv4 checksum or Ethernet padding is invalid")
    if frame[14 + parsed_ipv4.total_length :] != bytes(spec.padding):
        raise AssertionError("Ethernet padding is not exact zero padding")
    return parsed_tcp


def _assert_exact(expected: bytes, frame: bytes, description: str) -> None:
    if frame != expected or len(frame) != MIN_ETHERNET_FRAME_BYTES:
        raise AssertionError(f"unexpected {description} frame: {frame.hex()}")


def assert_exact_tcp_frame(device_mac: bytes, frame: bytes, index: int) -> None:
    parse_tcp_frame(frame, device_mac, index)
    _assert_exact(tcp_frames(device_mac)[index], frame, f"TCP frame {index + 1}")


def assert_exact_exchange(device_mac: bytes, frames: list[bytes]) -> None:
    expected = exchange_frames(device_mac)
    if len(frames) != len(expected):
        raise AssertionError(f"expected exactly twelve Ethernet frames, got {len(frames)}")
    _assert_exact(expected[0], frames[0], "ARP request")
    _assert_exact(expected[1], frames[1], "ARP reply")
    for index, frame in enumerate(frames[2:]):
        assert_exact_tcp_frame(device_mac, frame, index)


class TcpPeer:
    """Loopback-only peer for exactly two ARP and ten TCP frames."""

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
            raise RuntimeError("TCP peer is already started")
        self._thread = threading.Thread(target=self._serve_once, daemon=True)
        self._thread.start()

    def join(self, timeout: float | None = None) -> None:
        if self._thread is None:
            raise RuntimeError("TCP peer has not been started")
        self._thread.join(timeout)
        if self._thread.is_alive():
            raise TimeoutError("TCP peer did not finish")

    def close(self) -> None:
        self.listener.close()

    def _read_local(self, connection: socket.socket, index: int, expected: list[bytes]) -> None:
        frame = read_socket_frame(connection)
        self.tx_frames.append(frame)
        assert_exact_tcp_frame(DESCRIBED_DEVICE_MAC, frame, index)
        if frame != expected[index + 2]:
            raise AssertionError("TCP peer received a reordered frame")

    def _send_peer(self, connection: socket.socket, index: int, expected: list[bytes]) -> None:
        frame = expected[index + 2]
        self.rx_frames.append(frame)
        connection.sendall(encode_socket_frame(frame))

    def _serve_once(self) -> None:
        try:
            with self.listener.accept()[0] as connection:
                self.connected = True
                connection.settimeout(self.timeout)
                first = read_socket_frame(connection)
                self.tx_frames.append(first)
                assert_exact_arp_request(DESCRIBED_DEVICE_MAC, first)
                reply = arp_reply(DESCRIBED_DEVICE_MAC)
                self.rx_frames.append(reply)
                connection.sendall(encode_socket_frame(reply))
                expected = exchange_frames(DESCRIBED_DEVICE_MAC)
                self._read_local(connection, 0, expected)
                self._send_peer(connection, 1, expected)
                self._read_local(connection, 2, expected)
                self._read_local(connection, 3, expected)
                self._send_peer(connection, 4, expected)
                self._read_local(connection, 5, expected)
                self._read_local(connection, 6, expected)
                self._send_peer(connection, 7, expected)
                self._send_peer(connection, 8, expected)
                self._read_local(connection, 9, expected)
                deadline = time.monotonic() + self.timeout
                while True:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or not select.select([connection], [], [], remaining)[0]:
                        raise TimeoutError("TCP peer did not reach clean completion after final ACK")
                    try:
                        data = connection.recv(4096)
                    except ConnectionResetError:
                        data = b""
                    if data:
                        raise AssertionError("TCP peer received additional TX bytes after twelve frames")
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
            raise AssertionError(f"forbidden TCP acceptance evidence: {marker}")
    VIRTIO_NET.assert_no_storage_path_markers(evidence)


def assert_runner_success(returncode: int | None, qemu_output: str) -> None:
    outcomes = [line for line in qemu_output.splitlines() if "QEMU_OUTCOME" in line]
    if returncode != 0 or outcomes != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome, returncode={returncode}, outcomes={outcomes!r}")


def assert_tcp_acceptance(serial: str, qemu_output: str) -> None:
    assert_exact_ordered_markers(serial)
    assert_no_forbidden_evidence(serial, qemu_output)
    assert_runner_success(0, qemu_output)


def assert_peer_exchange(peer: TcpPeer) -> None:
    if peer.error is not None:
        raise AssertionError(f"TCP frame peer failed: {peer.error}") from peer.error
    if not peer.connected or not peer.completed:
        raise AssertionError("TCP frame peer did not connect and finish cleanly")
    if len(peer.tx_frames) != 7 or len(peer.rx_frames) != 5:
        raise AssertionError(f"TCP frame peer did not exchange exactly twelve frames: TX={len(peer.tx_frames)} RX={len(peer.rx_frames)}")
    assert_exact_arp_request(DESCRIBED_DEVICE_MAC, peer.tx_frames[0])
    assert_exact_arp_reply(DESCRIBED_DEVICE_MAC, peer.rx_frames[0])
    for tx, index in zip(peer.tx_frames[1:], (0, 2, 3, 5, 6, 9)):
        assert_exact_tcp_frame(DESCRIBED_DEVICE_MAC, tx, index)
    for rx, index in zip(peer.rx_frames[1:], (1, 4, 7, 8)):
        assert_exact_tcp_frame(DESCRIBED_DEVICE_MAC, rx, index)


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
    probe = ROOT / "target" / "tcp-probe" / "tcp-probe.elf"
    shell = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none", "--no-default-features", "--features", "tcp-probe"])
    run([sys.executable, "scripts/build-tcp-probe.py"])
    run([sys.executable, "scripts/verify-user-elf.py", "--elf", str(probe.resolve())])
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-image.py", "--kernel", str(kernel.resolve()), "--tcp-probe-elf", str(probe.resolve())])
    for artifact in (loader, kernel, probe, shell):
        if not artifact.is_file():
            raise AssertionError(f"expected build artifact is missing: {artifact}")
    assert_image_preflight()
    return loader, kernel, probe, shell


IPV4.SERIAL_LOG = SERIAL_LOG
IPV4.ESP_IMAGE = ESP_IMAGE
IPV4.QEMU_TIMEOUT_SECONDS = QEMU_TIMEOUT_SECONDS
IPV4.REQUIRED_MARKERS = REQUIRED_MARKERS
IPV4.CONSUMER_MARKERS = CONSUMER_MARKERS
IPV4.KERNEL_MARKERS = KERNEL_MARKERS
IPV4.Ipv4Peer = TcpPeer
IPV4.probe_runner_command = probe_runner_command
IPV4.assert_ipv4_acceptance = assert_tcp_acceptance
IPV4.assert_runner_success = assert_runner_success
IPV4.assert_peer_exchange = assert_peer_exchange
run_probe_boot = IPV4.run_probe_boot


class TcpAcceptanceSelfTest(unittest.TestCase):
    def valid_serial(self) -> str:
        return "\n".join(REQUIRED_MARKERS)

    def test_exact_frames(self) -> None:
        frames = exchange_frames(DESCRIBED_DEVICE_MAC)
        assert_exact_exchange(DESCRIBED_DEVICE_MAC, frames)
        self.assertEqual([len(frame) for frame in frames], [60] * 12)
        self.assertEqual([spec.ip_checksum for spec in TCP_SEGMENTS], [0xC877, 0xC876, 0xC879, 0xC872, 0xC871, 0xC876, 0xC875, 0xC874, 0xC873, 0xC872])
        self.assertEqual([spec.tcp_checksum for spec in TCP_SEGMENTS], [0xAD76, 0x885F, 0x9E68, 0xA974, 0xA96D, 0x9E5C, 0x9E5B, 0x9E5B, 0x9E5A, 0x9E5A])

    def test_malformed_and_evidence_oracles(self) -> None:
        frame = exchange_frames(DESCRIBED_DEVICE_MAC)[2]
        bad = bytearray(frame)
        bad[34] ^= 1
        with self.assertRaises(AssertionError):
            assert_exact_tcp_frame(DESCRIBED_DEVICE_MAC, bytes(bad), 0)
        assert_tcp_acceptance(self.valid_serial(), "QEMU_OUTCOME success\n")
        with self.assertRaises(AssertionError):
            assert_tcp_acceptance(self.valid_serial() + "\nPYTHOS:CORE:BLOCK_DEVICE_READY", "QEMU_OUTCOME success\n")


def run_self_tests() -> int:
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(TcpAcceptanceSelfTest))
    if result.wasSuccessful():
        print("TCP_QEMU_ACCEPTANCE_OK")
        return 0
    return 1


def main() -> int:
    loader, kernel, probe, shell = build_probe_image()
    consumer, kernel_serial, qemu_output, peer = run_probe_boot()
    print(f"TCP_LIVE_FRAMES tx={len(peer.tx_frames)} rx={len(peer.rx_frames)} total={len(peer.tx_frames) + len(peer.rx_frames)}")
    observed_markers = consumer.splitlines() + kernel_serial.splitlines()
    marker_evidence = [line for line in observed_markers if line in REQUIRED_MARKERS]
    print(f"TCP_LIVE_MARKERS {' > '.join(marker_evidence)}")
    print(f"TCP_LIVE_OUTCOME {next(line for line in qemu_output.splitlines() if line == 'QEMU_OUTCOME success')}")
    print(f"TCP_LIVE_CLEANUP serial_log_exists={SERIAL_LOG.exists()} esp_snapshot_exists={ESP_IMAGE.exists()}")
    print(f"TCP_ARTIFACT loader={loader.resolve()}")
    print(f"TCP_ARTIFACT kernel={kernel.resolve()}")
    print(f"TCP_ARTIFACT probe={probe.resolve()}")
    print(f"TCP_ARTIFACT shell={shell.resolve()}")
    print(f"TCP_ARTIFACT esp={ROOT / 'image' / 'esp'}")
    print(f"TCP_ARTIFACT serial-log-cleaned={SERIAL_LOG.resolve()}")
    print("TCP_QEMU_ACCEPTANCE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-tcp.py [--self-test]")
    raise SystemExit(main())
