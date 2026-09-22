#!/usr/bin/env python
"""Deterministic QEMU acceptance for one bounded ARP/UDP datagram exchange."""

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
SERIAL_LOG = TARGET / "udp-probe-com1.log"
ESP_IMAGE = TARGET / "udp-probe-com1-esp.img"
QEMU_TIMEOUT_SECONDS = 30.0
SUCCESS_MARKER = "PYTHOS:CORE:UDP_READY"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:UDP:BOOTSTRAPPED",
    "PYTHOS:CORE:UDP:DESCRIBE_OK",
    "PYTHOS:CORE:UDP:ARP_SETUP_OK",
    "PYTHOS:CORE:UDP:TX_OK",
    "PYTHOS:CORE:UDP:RX_OK",
    "PYTHOS:CORE:UDP:TEARDOWN_REVOKED",
    SUCCESS_MARKER,
)
CONSUMER_MARKERS = REQUIRED_MARKERS[:5]
KERNEL_MARKERS = REQUIRED_MARKERS[5:]
FORBIDDEN_EVIDENCE = (
    "PYTHOS:CORE:UDP:ERROR",
    "PYTHOS:PANIC",
    "TIMEOUT",
    "TRANSPORT_ERROR",
    "transport-error",
    "UDP:FAILED",
)
ARP_ETHER_TYPE = 0x0806
IPV4_ETHER_TYPE = 0x0800
IPV4_PROTOCOL = 17
IPV4_REQUEST_ID = 0x1405
IPV4_REPLY_ID = 0x1406
IPV4_TTL = 64
UDP_REQUEST_SOURCE_PORT = 0x1405
UDP_REQUEST_DESTINATION_PORT = 0x1406
UDP_DATA = b"PYTHUDP"
PEER_MAC = bytes.fromhex("020000000002")
DESCRIBED_DEVICE_MAC = bytes.fromhex("525400123456")
LOCAL_IPV4 = bytes.fromhex("c0a80e02")
PEER_IPV4 = bytes.fromhex("c0a80e01")
BROADCAST_MAC = bytes(6 * b"\xff")
MIN_ETHERNET_FRAME_BYTES = 60
IPV4_HEADER_BYTES = 20
IPV4_DATAGRAM_BYTES = 35
UDP_DATAGRAM_BYTES = 15
UDP_HEADER_BYTES = 8


def load_ipv4_acceptance():
    path = ROOT / "scripts" / "test-ipv4.py"
    spec = importlib.util.spec_from_file_location("udp_ipv4_acceptance", path)
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
ones_complement_checksum = IPV4.ones_complement_checksum
finalize_com2_transcript = IPV4.finalize_com2_transcript
assert_image_preflight = IPV4.assert_image_preflight
run = IPV4.run


def ipv4_header(identification: int, source: bytes, destination: bytes, payload: bytes) -> bytes:
    if not 0 <= identification <= 0xFFFF:
        raise ValueError("IPv4 identification must fit in sixteen bits")
    if len(source) != 4 or len(destination) != 4:
        raise ValueError("IPv4 addresses must be four bytes")
    total_length = IPV4_HEADER_BYTES + len(payload)
    if total_length != IPV4_DATAGRAM_BYTES:
        raise ValueError("accepted UDP IPv4 datagram must be exactly 35 bytes")
    header = bytearray(
        struct.pack("!BBHHHBBH4s4s", 0x45, 0, total_length, identification, 0, IPV4_TTL, IPV4_PROTOCOL, 0, source, destination)
    )
    header[10:12] = ones_complement_checksum(header).to_bytes(2, "big")
    return bytes(header)


ParsedIpv4 = IPV4.ParsedIpv4


def parse_ipv4_datagram(datagram: bytes) -> ParsedIpv4:
    parsed = IPV4.parse_ipv4_datagram(datagram)
    if len(datagram) != IPV4_DATAGRAM_BYTES or len(parsed.payload) != UDP_DATAGRAM_BYTES:
        raise AssertionError("UDP IPv4 datagram must be exactly 35 bytes")
    if parsed.protocol != IPV4_PROTOCOL:
        raise AssertionError("IPv4 protocol is not UDP")
    if parsed.ttl != IPV4_TTL:
        raise AssertionError("IPv4 TTL is not the accepted UDP value")
    return parsed


class ParsedUdp(NamedTuple):
    source_port: int
    destination_port: int
    length: int
    checksum: int
    data: bytes


def udp_checksum(source: bytes, destination: bytes, datagram: bytes) -> int:
    if len(source) != 4 or len(destination) != 4:
        raise ValueError("UDP pseudo-header addresses must be four bytes")
    if len(datagram) != UDP_DATAGRAM_BYTES:
        raise ValueError("accepted UDP datagram must be exactly 15 bytes")
    pseudo_header = source + destination + bytes((0, IPV4_PROTOCOL)) + len(datagram).to_bytes(2, "big")
    checksum_input = pseudo_header + datagram
    if len(checksum_input) % 2:
        checksum_input += b"\x00"
    return ones_complement_checksum(checksum_input)


def udp_datagram(source_port: int, destination_port: int, source: bytes, destination: bytes) -> bytes:
    if not 0 <= source_port <= 0xFFFF or not 0 <= destination_port <= 0xFFFF:
        raise ValueError("UDP ports must fit in sixteen bits")
    datagram = bytearray(struct.pack("!HHHH7s", source_port, destination_port, UDP_DATAGRAM_BYTES, 0, UDP_DATA))
    datagram[6:8] = udp_checksum(source, destination, bytes(datagram)).to_bytes(2, "big")
    return bytes(datagram)


def parse_udp_datagram(
    datagram: bytes,
    source: bytes,
    destination: bytes,
    expected_source_port: int,
    expected_destination_port: int,
) -> ParsedUdp:
    if len(datagram) != UDP_DATAGRAM_BYTES:
        raise AssertionError("UDP datagram must be exactly fifteen bytes")
    parsed = ParsedUdp(
        source_port=int.from_bytes(datagram[0:2], "big"),
        destination_port=int.from_bytes(datagram[2:4], "big"),
        length=int.from_bytes(datagram[4:6], "big"),
        checksum=int.from_bytes(datagram[6:8], "big"),
        data=datagram[8:],
    )
    if parsed.source_port != expected_source_port or parsed.destination_port != expected_destination_port:
        raise AssertionError("unexpected UDP ports")
    if parsed.length != UDP_DATAGRAM_BYTES:
        raise AssertionError("UDP length is not fifteen bytes")
    if parsed.checksum == 0 or udp_checksum(source, destination, datagram) != 0:
        raise AssertionError("UDP pseudo-header checksum is invalid")
    if parsed.data != UDP_DATA:
        raise AssertionError("unexpected UDP data")
    return parsed


arp_payload = IPV4.arp_payload
ethernet_frame = IPV4.ethernet_frame
arp_frame = IPV4.arp_frame


def arp_request(device_mac: bytes) -> bytes:
    return arp_frame(BROADCAST_MAC, device_mac, arp_payload(1, device_mac, LOCAL_IPV4, bytes(6), PEER_IPV4))


def arp_reply(device_mac: bytes) -> bytes:
    return arp_frame(device_mac, PEER_MAC, arp_payload(2, PEER_MAC, PEER_IPV4, device_mac, LOCAL_IPV4))


def udp_frame(destination: bytes, source: bytes, identification: int, source_ipv4: bytes, destination_ipv4: bytes, source_port: int, destination_port: int) -> bytes:
    datagram = udp_datagram(source_port, destination_port, source_ipv4, destination_ipv4)
    return ethernet_frame(destination, source, IPV4_ETHER_TYPE, ipv4_header(identification, source_ipv4, destination_ipv4, datagram) + datagram)


def udp_request(device_mac: bytes) -> bytes:
    return udp_frame(PEER_MAC, device_mac, IPV4_REQUEST_ID, LOCAL_IPV4, PEER_IPV4, UDP_REQUEST_SOURCE_PORT, UDP_REQUEST_DESTINATION_PORT)


def udp_reply(device_mac: bytes) -> bytes:
    return udp_frame(device_mac, PEER_MAC, IPV4_REPLY_ID, PEER_IPV4, LOCAL_IPV4, UDP_REQUEST_DESTINATION_PORT, UDP_REQUEST_SOURCE_PORT)


def _assert_exact(expected: bytes, frame: bytes, description: str) -> None:
    if len(frame) != MIN_ETHERNET_FRAME_BYTES or frame != expected:
        raise AssertionError(f"unexpected {description} frame: {frame.hex()}")


def assert_exact_arp_request(device_mac: bytes, frame: bytes) -> None:
    _assert_exact(arp_request(device_mac), frame, "ARP request")


def assert_exact_arp_reply(device_mac: bytes, frame: bytes) -> None:
    _assert_exact(arp_reply(device_mac), frame, "ARP reply")


def _assert_exact_udp(device_mac: bytes, frame: bytes, reply: bool) -> None:
    if len(frame) >= 49:
        parsed = parse_ipv4_datagram(frame[14:49])
        parse_udp_datagram(
            parsed.payload,
            parsed.source,
            parsed.destination,
            UDP_REQUEST_DESTINATION_PORT if reply else UDP_REQUEST_SOURCE_PORT,
            UDP_REQUEST_SOURCE_PORT if reply else UDP_REQUEST_DESTINATION_PORT,
        )
    _assert_exact(udp_reply(device_mac) if reply else udp_request(device_mac), frame, "UDP reply" if reply else "UDP request")


def assert_exact_udp_request(device_mac: bytes, frame: bytes) -> None:
    _assert_exact_udp(device_mac, frame, False)


def assert_exact_udp_reply(device_mac: bytes, frame: bytes) -> None:
    _assert_exact_udp(device_mac, frame, True)


def send_udp_reply(connection: socket.socket, device_mac: bytes) -> bytes:
    frame = udp_reply(device_mac)
    connection.sendall(encode_socket_frame(frame))
    return frame


class UdpPeer:
    """Loopback-only peer for exactly one ARP and one UDP datagram exchange."""

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
            raise RuntimeError("UDP peer is already started")
        self._thread = threading.Thread(target=self._serve_once, daemon=True)
        self._thread.start()

    def join(self, timeout: float | None = None) -> None:
        if self._thread is None:
            raise RuntimeError("UDP peer has not been started")
        self._thread.join(timeout)
        if self._thread.is_alive():
            raise TimeoutError("UDP peer did not finish")

    def close(self) -> None:
        self.listener.close()

    def _serve_once(self) -> None:
        try:
            with self.listener.accept()[0] as connection:
                self.connected = True
                connection.settimeout(self.timeout)
                request = read_socket_frame(connection)
                self.tx_frames.append(request)
                assert_exact_arp_request(DESCRIBED_DEVICE_MAC, request)
                reply = arp_reply(DESCRIBED_DEVICE_MAC)
                self.rx_frames.append(reply)
                connection.sendall(encode_socket_frame(reply))
                request = read_socket_frame(connection)
                self.tx_frames.append(request)
                assert_exact_udp_request(DESCRIBED_DEVICE_MAC, request)
                reply = send_udp_reply(connection, DESCRIBED_DEVICE_MAC)
                self.rx_frames.append(reply)
                deadline = time.monotonic() + self.timeout
                while True:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or not select.select([connection], [], [], remaining)[0]:
                        raise TimeoutError("UDP peer did not reach clean completion after the reply")
                    try:
                        if connection.recv(4096):
                            raise AssertionError("UDP peer received additional TX bytes after two requests")
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
            raise AssertionError(f"forbidden UDP acceptance evidence: {marker}")
    VIRTIO_NET.assert_no_storage_path_markers(evidence)


def assert_runner_success(returncode: int | None, qemu_output: str) -> None:
    outcomes = [line for line in qemu_output.splitlines() if "QEMU_OUTCOME" in line]
    if returncode != 0:
        raise AssertionError(f"QEMU runner failed with {returncode}: {outcomes!r}")
    if outcomes != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcomes!r}")


def assert_udp_acceptance(serial: str, qemu_output: str) -> None:
    assert_exact_ordered_markers(serial)
    assert_no_forbidden_evidence(serial, qemu_output)
    assert_runner_success(0, qemu_output)


def assert_peer_exchange(peer: UdpPeer) -> None:
    if peer.error is not None:
        raise AssertionError(f"UDP frame peer failed: {peer.error}") from peer.error
    if not peer.connected:
        raise AssertionError("QEMU did not connect to the UDP frame peer")
    if len(peer.tx_frames) != 2 or len(peer.rx_frames) != 2:
        raise AssertionError("UDP frame peer did not exchange exactly four frames")
    if not peer.completed:
        raise AssertionError("UDP frame peer did not reach clean completion")
    assert_exact_arp_request(DESCRIBED_DEVICE_MAC, peer.tx_frames[0])
    assert_exact_arp_reply(DESCRIBED_DEVICE_MAC, peer.rx_frames[0])
    assert_exact_udp_request(DESCRIBED_DEVICE_MAC, peer.tx_frames[1])
    assert_exact_udp_reply(DESCRIBED_DEVICE_MAC, peer.rx_frames[1])


def probe_runner_command(peer_port: int, shell_port: int) -> list[str]:
    return [
        sys.executable, "scripts/run-qemu.py", "--serial-log", str(SERIAL_LOG), "--success-marker", SUCCESS_MARKER,
        "--timeout", str(int(QEMU_TIMEOUT_SECONDS)), "--no-audio-device", "--no-virtio-blk", "--virtio-net",
        "--virtio-net-peer-port", str(peer_port), "--shell-port", str(shell_port), "--expect-outcome", "success",
    ]


def build_probe_image() -> tuple[Path, Path, Path, Path]:
    loader = ROOT / "target" / "x86_64-unknown-uefi" / "debug" / "bootx64.efi"
    kernel = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"
    probe = ROOT / "target" / "udp-probe" / "udp-probe.elf"
    shell = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none", "--no-default-features", "--features", "udp-probe"])
    run([sys.executable, "scripts/build-udp-probe.py"])
    run([sys.executable, "scripts/verify-user-elf.py", "--elf", str(probe.resolve())])
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-image.py", "--kernel", str(kernel.resolve()), "--udp-probe-elf", str(probe.resolve())])
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
IPV4.Ipv4Peer = UdpPeer
IPV4.probe_runner_command = probe_runner_command
IPV4.assert_ipv4_acceptance = assert_udp_acceptance
IPV4.assert_runner_success = assert_runner_success
IPV4.assert_peer_exchange = assert_peer_exchange


def run_probe_boot() -> tuple[str, str, str, UdpPeer]:
    return IPV4.run_probe_boot()


class UdpAcceptanceSelfTest(unittest.TestCase):
    def valid_serial(self) -> str:
        return "\n".join(REQUIRED_MARKERS)

    def test_exact_frames_and_checksums(self) -> None:
        self.assertEqual(udp_request(DESCRIBED_DEVICE_MAC)[14:34].hex(), "45000023140500004011c971c0a80e02c0a80e01")
        self.assertEqual(udp_reply(DESCRIBED_DEVICE_MAC)[14:34].hex(), "45000023140600004011c970c0a80e01c0a80e02")
        self.assertEqual(udp_request(DESCRIBED_DEVICE_MAC)[34:49].hex(), "14051406000ff08a50595448554450")
        self.assertEqual(udp_reply(DESCRIBED_DEVICE_MAC)[34:49].hex(), "14061405000ff08a50595448554450")
        self.assertEqual(udp_reply(DESCRIBED_DEVICE_MAC)[49:], bytes(11))

    def test_malformed_packets_are_rejected(self) -> None:
        bad_header = bytearray(udp_reply(DESCRIBED_DEVICE_MAC))
        bad_header[14] = 0x55
        with self.assertRaises(AssertionError):
            assert_exact_udp_reply(DESCRIBED_DEVICE_MAC, bytes(bad_header))
        bad_udp = bytearray(udp_reply(DESCRIBED_DEVICE_MAC))
        bad_udp[34] ^= 1
        with self.assertRaises(AssertionError):
            assert_exact_udp_reply(DESCRIBED_DEVICE_MAC, bytes(bad_udp))

    def test_marker_storage_and_outcome_oracles(self) -> None:
        assert_udp_acceptance(self.valid_serial(), "QEMU_OUTCOME success\n")
        for serial in (
            "\n".join((*REQUIRED_MARKERS[:2], *REQUIRED_MARKERS[3:])),
            "\n".join((*REQUIRED_MARKERS[:2], REQUIRED_MARKERS[3], REQUIRED_MARKERS[2], *REQUIRED_MARKERS[4:])),
            self.valid_serial() + "\n" + SUCCESS_MARKER,
            self.valid_serial() + "\nPYTHOS:CORE:BLOCK_DEVICE_READY",
        ):
            with self.assertRaises(AssertionError):
                assert_udp_acceptance(serial, "QEMU_OUTCOME success\n")
        with self.assertRaises(AssertionError):
            assert_udp_acceptance(self.valid_serial(), "QEMU_OUTCOME timeout\n")

    def test_peer_rejects_extra_transmit(self) -> None:
        peer = UdpPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(encode_socket_frame(arp_request(DESCRIBED_DEVICE_MAC)))
                self.assertEqual(read_socket_frame(connection), arp_reply(DESCRIBED_DEVICE_MAC))
                connection.sendall(encode_socket_frame(udp_request(DESCRIBED_DEVICE_MAC)))
                self.assertEqual(read_socket_frame(connection), udp_reply(DESCRIBED_DEVICE_MAC))
                connection.sendall(encode_socket_frame(udp_request(DESCRIBED_DEVICE_MAC)))
            peer.join(timeout=1.0)
            self.assertIsNotNone(peer.error)
        finally:
            peer.close()


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(UdpAcceptanceSelfTest)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.wasSuccessful():
        print("UDP_QEMU_ACCEPTANCE_OK")
        return 0
    return 1


def main() -> int:
    loader, kernel, probe, shell = build_probe_image()
    _consumer, _kernel, _qemu_output, _peer = run_probe_boot()
    print(f"UDP_ARTIFACT loader={loader.resolve()}")
    print(f"UDP_ARTIFACT kernel={kernel.resolve()}")
    print(f"UDP_ARTIFACT probe={probe.resolve()}")
    print(f"UDP_ARTIFACT shell={shell.resolve()}")
    print(f"UDP_ARTIFACT esp={ROOT / 'image' / 'esp'}")
    print(f"UDP_ARTIFACT serial-log-cleaned={SERIAL_LOG.resolve()}")
    print("UDP_QEMU_ACCEPTANCE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-udp.py [--self-test]")
    raise SystemExit(main())
