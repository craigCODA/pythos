#!/usr/bin/env python
"""Deterministic QEMU acceptance for one bounded ARP/IPv4 exchange."""

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
from typing import NamedTuple

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
SERIAL_LOG = TARGET / "ipv4-probe-com1.log"
ESP_IMAGE = TARGET / "ipv4-probe-com1-esp.img"
QEMU_TIMEOUT_SECONDS = 30.0
SUCCESS_MARKER = "PYTHOS:CORE:IPV4_READY"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:IPV4:BOOTSTRAPPED",
    "PYTHOS:CORE:IPV4:DESCRIBE_OK",
    "PYTHOS:CORE:IPV4:ARP_SETUP_OK",
    "PYTHOS:CORE:IPV4:TX_OK",
    "PYTHOS:CORE:IPV4:RX_OK",
    "PYTHOS:CORE:IPV4:TEARDOWN_REVOKED",
    SUCCESS_MARKER,
)
CONSUMER_MARKERS = REQUIRED_MARKERS[:5]
KERNEL_MARKERS = REQUIRED_MARKERS[5:]
FORBIDDEN_EVIDENCE = (
    "PYTHOS:CORE:IPV4:ERROR",
    "PYTHOS:PANIC",
    "TIMEOUT",
    "TRANSPORT_ERROR",
    "transport-error",
    "IPV4:FAILED",
)
ARP_ETHER_TYPE = 0x0806
IPV4_ETHER_TYPE = 0x0800
IPV4_PROTOCOL = 253
IPV4_REQUEST_ID = 0x1401
IPV4_REPLY_ID = 0x1402
IPV4_TTL = 64
PEER_MAC = bytes.fromhex("020000000002")
DESCRIBED_DEVICE_MAC = bytes.fromhex("525400123456")
LOCAL_IPV4 = bytes.fromhex("c0a80e02")
PEER_IPV4 = bytes.fromhex("c0a80e01")
IPV4_REQUEST_PAYLOAD = b"PYTHIPRQ"
IPV4_REPLY_PAYLOAD = b"PYTHIPRP"
BROADCAST_MAC = bytes(6 * b"\xff")
MIN_ETHERNET_FRAME_BYTES = 60
ARP_PAYLOAD_BYTES = 28
IPV4_HEADER_BYTES = 20
IPV4_DATAGRAM_BYTES = 28


def load_virtio_net_acceptance():
    path = ROOT / "scripts" / "test-virtio-net.py"
    spec = importlib.util.spec_from_file_location("ipv4_virtio_net_peer", path)
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
    total_length = IPV4_HEADER_BYTES + len(payload)
    if total_length > 0xFFFF:
        raise ValueError("IPv4 datagram exceeds the sixteen-bit total length")
    header = bytearray(
        struct.pack(
            "!BBHHHBBH4s4s",
            0x45,
            0,
            total_length,
            identification,
            0,
            IPV4_TTL,
            IPV4_PROTOCOL,
            0,
            source,
            destination,
        )
    )
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
    payload: bytes


def parse_ipv4_datagram(datagram: bytes) -> ParsedIpv4:
    if len(datagram) < IPV4_HEADER_BYTES:
        raise AssertionError("IPv4 datagram is shorter than the fixed header")
    version = datagram[0] >> 4
    ihl = datagram[0] & 0x0F
    if version != 4:
        raise AssertionError("IPv4 version is not four")
    if ihl != 5:
        raise AssertionError("IPv4 options and noncanonical IHL are rejected")
    header_length = ihl * 4
    if header_length > len(datagram):
        raise AssertionError("IPv4 header exceeds the datagram")
    total_length = int.from_bytes(datagram[2:4], "big")
    if total_length < header_length or total_length > len(datagram):
        raise AssertionError("IPv4 total length is outside the supplied datagram")
    if ones_complement_checksum(datagram[:header_length]) != 0:
        raise AssertionError("IPv4 header checksum is invalid")
    flags_fragment_offset = int.from_bytes(datagram[6:8], "big")
    if flags_fragment_offset != 0:
        raise AssertionError("fragmented IPv4 datagrams are rejected")
    return ParsedIpv4(
        identification=int.from_bytes(datagram[4:6], "big"),
        flags_fragment_offset=flags_fragment_offset,
        ttl=datagram[8],
        protocol=datagram[9],
        checksum=int.from_bytes(datagram[10:12], "big"),
        source=datagram[12:16],
        destination=datagram[16:20],
        payload=datagram[header_length:total_length],
    )


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
        IPV4_ETHER_TYPE,
        6,
        4,
        operation,
        sender_hardware,
        sender_protocol,
        target_hardware,
        target_protocol,
    )


def ethernet_frame(destination: bytes, source: bytes, ether_type: int, payload: bytes) -> bytes:
    if len(destination) != 6 or len(source) != 6:
        raise ValueError("Ethernet MAC addresses must be six bytes")
    if not 0 <= ether_type <= 0xFFFF:
        raise ValueError("EtherType must fit in sixteen bits")
    frame = destination + source + ether_type.to_bytes(2, "big") + payload
    if len(frame) > MIN_ETHERNET_FRAME_BYTES:
        raise ValueError("software frame exceeds the accepted sixty-byte profile")
    return frame + bytes(MIN_ETHERNET_FRAME_BYTES - len(frame))


def arp_frame(destination: bytes, source: bytes, payload: bytes) -> bytes:
    if len(payload) != ARP_PAYLOAD_BYTES:
        raise ValueError("ARP payload must be exactly 28 bytes")
    return ethernet_frame(destination, source, ARP_ETHER_TYPE, payload)


def ipv4_frame(
    destination: bytes,
    source: bytes,
    identification: int,
    source_ipv4: bytes,
    destination_ipv4: bytes,
    payload: bytes,
) -> bytes:
    datagram = ipv4_header(identification, source_ipv4, destination_ipv4, payload) + payload
    if len(datagram) != IPV4_DATAGRAM_BYTES:
        raise ValueError("accepted IPv4 datagram must be exactly 28 bytes")
    return ethernet_frame(destination, source, IPV4_ETHER_TYPE, datagram)


def arp_request(device_mac: bytes) -> bytes:
    return arp_frame(
        BROADCAST_MAC,
        device_mac,
        arp_payload(1, device_mac, LOCAL_IPV4, bytes(6), PEER_IPV4),
    )


def arp_reply(device_mac: bytes) -> bytes:
    return arp_frame(
        device_mac,
        PEER_MAC,
        arp_payload(2, PEER_MAC, PEER_IPV4, device_mac, LOCAL_IPV4),
    )


def ipv4_request(device_mac: bytes) -> bytes:
    return ipv4_frame(
        PEER_MAC,
        device_mac,
        IPV4_REQUEST_ID,
        LOCAL_IPV4,
        PEER_IPV4,
        IPV4_REQUEST_PAYLOAD,
    )


def ipv4_reply(device_mac: bytes) -> bytes:
    return ipv4_frame(
        device_mac,
        PEER_MAC,
        IPV4_REPLY_ID,
        PEER_IPV4,
        LOCAL_IPV4,
        IPV4_REPLY_PAYLOAD,
    )


def _assert_exact(expected: bytes, frame: bytes, description: str) -> None:
    if len(frame) != MIN_ETHERNET_FRAME_BYTES or frame != expected:
        raise AssertionError(f"unexpected {description} frame: {frame.hex()}")


def assert_exact_arp_request(device_mac: bytes, frame: bytes) -> None:
    _assert_exact(arp_request(device_mac), frame, "ARP request")


def assert_exact_arp_reply(device_mac: bytes, frame: bytes) -> None:
    _assert_exact(arp_reply(device_mac), frame, "ARP reply")


def assert_exact_ipv4_request(device_mac: bytes, frame: bytes) -> None:
    if len(frame) >= 42:
        parse_ipv4_datagram(frame[14:42])
    _assert_exact(ipv4_request(device_mac), frame, "IPv4 request")


def assert_exact_ipv4_reply(device_mac: bytes, frame: bytes) -> None:
    if len(frame) >= 42:
        parse_ipv4_datagram(frame[14:42])
    _assert_exact(ipv4_reply(device_mac), frame, "IPv4 reply")


def send_ipv4_reply(connection: socket.socket, device_mac: bytes) -> bytes:
    frame = ipv4_reply(device_mac)
    connection.sendall(encode_socket_frame(frame))
    return frame


class Ipv4Peer:
    """Loopback-only peer for exactly one ARP and one IPv4 exchange."""

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
            raise RuntimeError("IPv4 peer is already started")
        self._thread = threading.Thread(target=self._serve_once, daemon=True)
        self._thread.start()

    def join(self, timeout: float | None = None) -> None:
        if self._thread is None:
            raise RuntimeError("IPv4 peer has not been started")
        self._thread.join(timeout)
        if self._thread.is_alive():
            raise TimeoutError("IPv4 peer did not finish")

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
                assert_exact_ipv4_request(DESCRIBED_DEVICE_MAC, request)
                reply = send_ipv4_reply(connection, DESCRIBED_DEVICE_MAC)
                self.rx_frames.append(reply)

                deadline = time.monotonic() + self.timeout
                while True:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or not select.select([connection], [], [], remaining)[0]:
                        raise TimeoutError("IPv4 peer did not reach clean completion after the reply")
                    try:
                        if connection.recv(4096):
                            raise AssertionError("IPv4 peer received additional TX bytes after two requests")
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
            raise AssertionError(f"forbidden IPv4 acceptance evidence: {marker}")
    VIRTIO_NET.assert_no_storage_path_markers(evidence)


def assert_runner_success(returncode: int | None, qemu_output: str) -> None:
    outcomes = [line for line in qemu_output.splitlines() if "QEMU_OUTCOME" in line]
    if returncode != 0:
        raise AssertionError(f"QEMU runner failed with {returncode}: {outcomes!r}")
    if outcomes != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcomes!r}")


def assert_ipv4_acceptance(serial: str, qemu_output: str) -> None:
    assert_exact_ordered_markers(serial)
    assert_no_forbidden_evidence(serial, qemu_output)
    assert_runner_success(0, qemu_output)


def assert_peer_exchange(peer: Ipv4Peer) -> None:
    if peer.error is not None:
        raise AssertionError(f"IPv4 frame peer failed: {peer.error}") from peer.error
    if not peer.connected:
        raise AssertionError("QEMU did not connect to the IPv4 frame peer")
    if len(peer.tx_frames) != 2 or len(peer.rx_frames) != 2:
        raise AssertionError("IPv4 frame peer did not exchange exactly four frames")
    if not peer.completed:
        raise AssertionError("IPv4 frame peer did not reach clean completion")
    assert_exact_arp_request(DESCRIBED_DEVICE_MAC, peer.tx_frames[0])
    assert_exact_arp_reply(DESCRIBED_DEVICE_MAC, peer.rx_frames[0])
    assert_exact_ipv4_request(DESCRIBED_DEVICE_MAC, peer.tx_frames[1])
    assert_exact_ipv4_reply(DESCRIBED_DEVICE_MAC, peer.rx_frames[1])


def assert_live_timeline(timeline: AcceptanceTimeline) -> None:
    expected_sources = (
        *(("COM2", marker) for marker in CONSUMER_MARKERS),
        *(("COM1", marker) for marker in KERNEL_MARKERS),
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
            chunk = b""
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


def build_probe_image() -> tuple[Path, Path, Path, Path]:
    loader = ROOT / "target" / "x86_64-unknown-uefi" / "debug" / "bootx64.efi"
    kernel = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"
    probe = ROOT / "target" / "ipv4-probe" / "ipv4-probe.elf"
    shell = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run([
        "cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none",
        "--no-default-features", "--features", "ipv4-probe",
    ])
    run([sys.executable, "scripts/build-ipv4-probe.py"])
    run([sys.executable, "scripts/verify-user-elf.py", "--elf", str(probe.resolve())])
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([
        sys.executable,
        "scripts/build-image.py",
        "--kernel",
        str(kernel.resolve()),
        "--ipv4-probe-elf",
        str(probe.resolve()),
    ])
    for artifact in (loader, kernel, probe, shell):
        if not artifact.is_file():
            raise AssertionError(f"expected build artifact is missing: {artifact}")
    assert_image_preflight()
    return loader, kernel, probe, shell


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


def wait_for_runner_exit(process: subprocess.Popen[str], timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while process.poll() is None and time.monotonic() < deadline:
        time.sleep(0.05)
    if process.poll() is None:
        raise AssertionError("QEMU runner exceeded the bounded acceptance deadline")


def run_probe_boot() -> tuple[str, str, str, Ipv4Peer]:
    TARGET.mkdir(parents=True, exist_ok=True)
    for path in (SERIAL_LOG, ESP_IMAGE):
        if path.exists():
            path.unlink()
    peer = Ipv4Peer(timeout=QEMU_TIMEOUT_SECONDS)
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
        assert_ipv4_acceptance(consumer_serial + "\n" + kernel_serial, qemu_output)
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
        raise AssertionError(
            f"{error}\nCOM2:\n{consumer_serial}\nCOM1:\n{kernel_serial}\nrunner output:\n{qemu_output}"
        ) from error
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
            raise AssertionError(
                "IPv4 acceptance cleanup failed: " + "; ".join(str(error) for error in cleanup_errors)
            )


class Ipv4AcceptanceSelfTest(unittest.TestCase):
    """No-QEMU checks for the bounded IPv4 acceptance boundary."""

    def valid_serial(self) -> str:
        return "\n".join(REQUIRED_MARKERS)

    def test_exact_frames_and_checksums(self) -> None:
        self.assertEqual(ipv4_request(DESCRIBED_DEVICE_MAC)[14:34].hex(), "4500001c1401000040fdc890c0a80e02c0a80e01")
        self.assertEqual(ipv4_reply(DESCRIBED_DEVICE_MAC)[14:34].hex(), "4500001c1402000040fdc88fc0a80e01c0a80e02")
        self.assertEqual(ones_complement_checksum(ipv4_request(DESCRIBED_DEVICE_MAC)[14:34]), 0)
        self.assertEqual(ipv4_request(DESCRIBED_DEVICE_MAC)[42:], bytes(18))

    def test_parser_rejects_malformed_headers_fragments_and_options(self) -> None:
        valid = bytearray(ipv4_request(DESCRIBED_DEVICE_MAC)[14:42])
        cases = [bytes(valid[:19])]
        wrong_version = bytearray(valid)
        wrong_version[0] = 0x55
        cases.append(wrong_version)
        short_ihl = bytearray(valid)
        short_ihl[0] = 0x44
        cases.append(short_ihl)
        bad_checksum = bytearray(valid)
        bad_checksum[10] ^= 1
        cases.append(bad_checksum)
        fragment = bytearray(valid)
        fragment[6:8] = b"\x20\x00"
        fragment[10:12] = b"\x00\x00"
        fragment[10:12] = ones_complement_checksum(fragment[:20]).to_bytes(2, "big")
        cases.append(fragment)
        options = bytearray(valid)
        options[0] = 0x46
        cases.append(options)
        short_total = bytearray(valid)
        short_total[2:4] = (19).to_bytes(2, "big")
        short_total[10:12] = b"\x00\x00"
        short_total[10:12] = ones_complement_checksum(short_total[:20]).to_bytes(2, "big")
        cases.append(short_total)
        long_total = bytearray(valid)
        long_total[2:4] = (29).to_bytes(2, "big")
        long_total[10:12] = b"\x00\x00"
        long_total[10:12] = ones_complement_checksum(long_total[:20]).to_bytes(2, "big")
        cases.append(long_total)
        for datagram in cases:
            with self.assertRaises(AssertionError):
                parse_ipv4_datagram(bytes(datagram))

    def test_marker_storage_and_outcome_oracles(self) -> None:
        assert_ipv4_acceptance(self.valid_serial(), "QEMU_OUTCOME success\n")
        for serial in (
            "\n".join((*REQUIRED_MARKERS[:2], *REQUIRED_MARKERS[3:])),
            "\n".join((*REQUIRED_MARKERS[:2], REQUIRED_MARKERS[3], REQUIRED_MARKERS[2], *REQUIRED_MARKERS[4:])),
            self.valid_serial() + "\n" + SUCCESS_MARKER,
            self.valid_serial() + "\nPYTHOS:CORE:BLOCK_DEVICE_READY",
        ):
            with self.assertRaises(AssertionError):
                assert_ipv4_acceptance(serial, "QEMU_OUTCOME success\n")
        with self.assertRaises(AssertionError):
            assert_ipv4_acceptance(self.valid_serial(), "QEMU_OUTCOME timeout\n")

    def test_peer_rejects_extra_transmit(self) -> None:
        peer = Ipv4Peer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(encode_socket_frame(arp_request(DESCRIBED_DEVICE_MAC)))
                self.assertEqual(read_socket_frame(connection), arp_reply(DESCRIBED_DEVICE_MAC))
                connection.sendall(encode_socket_frame(ipv4_request(DESCRIBED_DEVICE_MAC)))
                self.assertEqual(read_socket_frame(connection), ipv4_reply(DESCRIBED_DEVICE_MAC))
                connection.sendall(encode_socket_frame(ipv4_request(DESCRIBED_DEVICE_MAC)))
            peer.join(timeout=1.0)
            self.assertIsNotNone(peer.error)
            self.assertIn("additional TX bytes", str(peer.error))
        finally:
            peer.close()

    def test_peer_accepts_exact_exchange_and_clean_completion(self) -> None:
        peer = Ipv4Peer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(encode_socket_frame(arp_request(DESCRIBED_DEVICE_MAC)))
                self.assertEqual(read_socket_frame(connection), arp_reply(DESCRIBED_DEVICE_MAC))
                connection.sendall(encode_socket_frame(ipv4_request(DESCRIBED_DEVICE_MAC)))
                self.assertEqual(read_socket_frame(connection), ipv4_reply(DESCRIBED_DEVICE_MAC))
                set_abortive_close(connection)
            peer.join(timeout=1.0)
            assert_peer_exchange(peer)
        finally:
            peer.close()

    def test_process_cleanup_reaps_runner(self) -> None:
        kwargs: dict[str, object] = {"cwd": ROOT}
        if sys.platform != "win32":
            kwargs["start_new_session"] = True
        runner = spawn_runner_process(
            [sys.executable, "-c", "import time; time.sleep(30)"], **kwargs
        )
        cleanup_runner_process(runner)
        if runner.process.stdout is not None:
            runner.process.stdout.close()
        self.assertIsNotNone(runner.process.poll())

    def test_image_preflight_requires_boot_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            esp = Path(temporary_directory)
            artifacts = (
                esp / "EFI" / "BOOT" / "BOOTX64.EFI",
                esp / "PYTHOS" / "PYTHCORE.ELF",
                esp / "PYTHOS" / "INIT.PAK",
            )
            for artifact in artifacts:
                artifact.parent.mkdir(parents=True, exist_ok=True)
                artifact.write_bytes(b"artifact")
            assert_image_preflight(esp)
            artifacts[-1].unlink()
            with self.assertRaises(AssertionError):
                assert_image_preflight(esp)


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(Ipv4AcceptanceSelfTest)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.wasSuccessful():
        print("IPV4_QEMU_ACCEPTANCE_OK")
        return 0
    return 1


def main() -> int:
    loader, kernel, probe, shell = build_probe_image()
    _consumer, _kernel, _qemu_output, _peer = run_probe_boot()
    print(f"IPV4_ARTIFACT loader={loader.resolve()}")
    print(f"IPV4_ARTIFACT kernel={kernel.resolve()}")
    print(f"IPV4_ARTIFACT probe={probe.resolve()}")
    print(f"IPV4_ARTIFACT shell={shell.resolve()}")
    print(f"IPV4_ARTIFACT esp={ROOT / 'image' / 'esp'}")
    print(f"IPV4_ARTIFACT serial-log-cleaned={SERIAL_LOG.resolve()}")
    print("IPV4_QEMU_ACCEPTANCE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-ipv4.py [--self-test]")
    raise SystemExit(main())
