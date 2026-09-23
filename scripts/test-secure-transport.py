#!/usr/bin/env python
"""Serialized QEMU acceptance for the finite Phase 14 TLS 1.3 proof."""

from __future__ import annotations

import base64
import contextlib
import hashlib
import importlib.util
import os
import select
import shutil
import socket
import ssl
import sys
import threading
import time
import unittest
from pathlib import Path
from typing import Iterator, NamedTuple


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
QEMU_TIMEOUT_SECONDS = 30.0
PEER_MAC = bytes.fromhex("020000000002")
DESCRIBED_DEVICE_MAC = bytes.fromhex("525400123456")
LOCAL_IPV4 = bytes.fromhex("c0a80e02")
PEER_IPV4 = bytes.fromhex("c0a80e01")
LOCAL_PORT = 0x1505
PEER_PORT = 0x1506
LOCAL_ISS = 0x15050000
PEER_ISS = 0x25060000
WINDOW = 0x1000
SYN = 0x02
FIN = 0x01
RST = 0x04
ACK = 0x10
MSS_OPTION = bytes.fromhex("02040400")
IPV4_ETHER_TYPE = 0x0800
MIN_ETHERNET_FRAME_BYTES = 60
MAX_ETHERNET_FRAME_BYTES = 1514
MAX_TCP_PAYLOAD = 700
REQUEST_PLAINTEXT = b"PYTHOS-SECURE-REQUEST"
RESPONSE_PLAINTEXT = b"PYTHOS-SECURE-RESPONSE"

GRANTED_REQUIRED_MARKERS = (
    "PYTHOS:CORE:SECURE:BOOTSTRAPPED",
    "PYTHOS:CORE:SECURE:OPEN_GRANTED",
    "PYTHOS:CORE:SECURE:TCP_READY",
    "PYTHOS:CORE:SECURE:TLS_HANDSHAKE_OK",
    "PYTHOS:CORE:SECURE:REQUEST_ENCRYPTED",
    "PYTHOS:CORE:SECURE:RESPONSE_DECRYPTED",
    "PYTHOS:CORE:SECURE:CLOSE_OK",
    "PYTHOS:CORE:SECURE:TEARDOWN_REVOKED",
    "PYTHOS:CORE:SECURE_READY",
)
GRANTED_CONSUMER_MARKERS = GRANTED_REQUIRED_MARKERS[:7]
GRANTED_KERNEL_MARKERS = GRANTED_REQUIRED_MARKERS[7:]
TAMPER_REQUIRED_MARKERS = (
    "PYTHOS:CORE:SECURE:BOOTSTRAPPED",
    "PYTHOS:CORE:SECURE:OPEN_GRANTED",
    "PYTHOS:CORE:SECURE:TCP_READY",
    "PYTHOS:CORE:SECURE:TLS_HANDSHAKE_OK",
    "PYTHOS:CORE:SECURE:REQUEST_ENCRYPTED",
    "PYTHOS:CORE:SECURE:TAMPER_REJECTED",
    "PYTHOS:CORE:SECURE:TEARDOWN_REVOKED",
    "PYTHOS:CORE:SECURE_TAMPER_READY",
)
TAMPER_CONSUMER_MARKERS = TAMPER_REQUIRED_MARKERS[:6]
TAMPER_KERNEL_MARKERS = TAMPER_REQUIRED_MARKERS[6:]
DENIED_REQUIRED_MARKERS = (
    "PYTHOS:CORE:SECURE:DENIED_BOOTSTRAPPED",
    "PYTHOS:CORE:SECURE:OPEN_WITHOUT_CAP_DENIED",
    "PYTHOS:CORE:SECURE:DENIED_TEARDOWN_COMPLETE",
    "PYTHOS:CORE:SECURE_DENIED_READY",
)
DENIED_CONSUMER_MARKERS = DENIED_REQUIRED_MARKERS[:2]
DENIED_KERNEL_MARKERS = DENIED_REQUIRED_MARKERS[2:]
FORBIDDEN_EVIDENCE = (
    "PYTHOS:CORE:SECURE:ERROR",
    "PYTHOS:CORE:SOCKET",
    "PYTHOS:PANIC",
    "TIMEOUT",
    "TRANSPORT_ERROR",
    "transport-error",
    "SECURE:FAILED",
    "PYTHOS:CORE:BLOCK_DEVICE_READY",
)

# The fixture is an acceptance-only P-256 identity.  It is decoded into a
# TemporaryDirectory immediately before SSLContext.load_cert_chain and is
# never written to the repository or retained after the peer exits.
CERTIFICATE_PEM_B64 = (
    "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSUJNekNCMjZBREFnRUNBZ0VCTUFvR0NDcUdTTTQ5QkFNQ01CWXhGREFTQmdOVkJBTU1DM0I1ZEdodmN5NTAK"
    "WlhOME1CNFhEVEl3TURFd01UQXdNREF3TUZvWERUTXdNREV3TVRBd01EQXdNRm93RmpFVU1CSUdBMVVFQXd3TApjSGwwYUc5ekxuUmxjM1F3V1RBVEJnY3Foa2pPUFFJQkJnZ3Foa2pPUFFNQkJ3TkNBQVROS1RUeEdzMnZOUFlPCjN0NFVZMnNSYUl3c2F6eDBnR0tma0V3N3UzNXlKT3FYVHRsRmhzVjdaQmhwWndkNFFEbnQ3NmYvNlJvNkpjMUkKSjc3M3NQK3hveG93R0RBV0JnTlZIUkVFRHpBTmdndHdlWFJvYjNNdWRHVnpkREFLQmdncWhrak9QUVFEQWdOSApBREJFQWlBaXdsai9qdjJvSEZlcGREVk1mc05NNEZvZkI2WjVhT0Z0RndSNU9rbno4QUlnTmNRcVpKZC9XRENYCldXNTlZOXBOZlFWMlFXNGlKYWlJcnFFUTR0Zmg0Mnc9Ci0tLS0tRU5EIENFUlRJRklDQVRFLS0tLS0K"
)
PRIVATE_KEY_PEM_B64 = (
    "LS0tLS1CRUdJTiBFQyBQUklWQVRFIEtFWS0tLS0tCk1IY0NBUUVFSUJoVFVnaXN5RnM3cG9BVXYybUYzbURxZUpCL20rNzVvTS9TbFJTS25xN1ZvQW9HQ0NxR1NNNDkK"
    "QXdFSG9VUURRZ0FFelNrMDhSck5yelQyRHQ3ZUZHTnJFV2lNTEdzOGRJQmluNUJNTzd0K2NpVHFsMDdaUlliRgplMlFZYVdjSGVFQTU3ZStuLytrYU9pWE5TQ2UrOTdEL3NRPT0KLS0tLS1FTkQgRUMgUFJJVkFURSBLRVktLS0tLQo="
)
CERTIFICATE_SHA256 = "05fbe163a52218a9f419c17b540e73b963f9a265a43b7bc05587934b07ea7a4e"


def load_tcp_acceptance():
    path = ROOT / "scripts" / "test-tcp.py"
    spec = importlib.util.spec_from_file_location("secure_tcp_acceptance", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load test-tcp.py")
    module = importlib.util.module_from_spec(spec)
    scripts_dir = str(ROOT / "scripts")
    inserted = scripts_dir not in sys.path
    if inserted:
        sys.path.insert(0, scripts_dir)
    try:
        spec.loader.exec_module(module)
    finally:
        if inserted:
            sys.path.remove(scripts_dir)
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


SERIAL_LOG = TARGET / "secure-granted-com1.log"
ESP_IMAGE = TARGET / "secure-granted-com1-esp.img"
SUCCESS_MARKER = GRANTED_REQUIRED_MARKERS[-1]
REQUIRED_MARKERS = GRANTED_REQUIRED_MARKERS
CONSUMER_MARKERS = GRANTED_CONSUMER_MARKERS
KERNEL_MARKERS = GRANTED_KERNEL_MARKERS
PEER_FACTORY = None
ASSERT_ACCEPTANCE = None
CASE = "granted"


class ParsedSecureFrame(NamedTuple):
    sequence: int
    acknowledgment: int
    flags: int
    source_port: int
    destination_port: int
    data: bytes


def base64_fixture(value: str) -> bytes:
    return base64.b64decode(value.encode("ascii"), validate=True)


def certificate_fingerprint(certificate_der: bytes) -> str:
    if certificate_der.startswith(b"-----BEGIN CERTIFICATE-----"):
        converted = ssl.PEM_cert_to_DER_cert(certificate_der.decode("ascii"))
        certificate_der = (
            bytes.fromhex(converted) if isinstance(converted, str) else bytes(converted)
        )
    return hashlib.sha256(certificate_der).hexdigest()


@contextlib.contextmanager
def temporary_tls_fixture() -> Iterator[tuple[Path, Path]]:
    certificate_pem = base64_fixture(CERTIFICATE_PEM_B64)
    key_pem = base64_fixture(PRIVATE_KEY_PEM_B64)
    TARGET.mkdir(parents=True, exist_ok=True)
    directory = TARGET / f".pythos-secure-tls-{os.getpid()}-{time.time_ns()}"
    directory.mkdir()
    try:
        root = directory
        certificate = root / "server.crt"
        key = root / "server.key"
        certificate.write_bytes(certificate_pem)
        key.write_bytes(key_pem)
        yield certificate, key
    finally:
        shutil.rmtree(directory, ignore_errors=False)


def make_server_context(certificate: Path, key: Path) -> ssl.SSLContext:
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_3
    context.maximum_version = ssl.TLSVersion.TLSv1_3
    context.load_cert_chain(certfile=str(certificate), keyfile=str(key))
    context.options |= getattr(ssl, "OP_NO_TICKET", 0)
    if hasattr(context, "num_tickets"):
        context.num_tickets = 0
    return context


def tls_records(wire: bytes) -> list[bytes]:
    records: list[bytes] = []
    offset = 0
    while offset < len(wire):
        if len(wire) - offset < 5:
            raise AssertionError("TLS wire has a truncated record header")
        length = int.from_bytes(wire[offset + 3 : offset + 5], "big")
        end = offset + 5 + length
        if end > len(wire):
            raise AssertionError("TLS wire has a truncated record payload")
        records.append(wire[offset:end])
        offset = end
    if not records:
        raise AssertionError("TLS wire has no complete records")
    return records


def tamper_first_application_record(wire: bytes) -> bytes:
    mutated = bytearray(wire)
    offset = 0
    while offset < len(mutated):
        if len(mutated) - offset < 5:
            raise AssertionError("TLS wire has a truncated record header")
        length = int.from_bytes(mutated[offset + 3 : offset + 5], "big")
        end = offset + 5 + length
        if end > len(mutated):
            raise AssertionError("TLS wire has a truncated record payload")
        if mutated[offset] == 0x17:
            if length == 0:
                raise AssertionError("TLS application record has no protected bytes")
            mutated[end - 1] ^= 0x01
            return bytes(mutated)
        offset = end
    raise AssertionError("TLS wire has no application-data record")


def _ethernet_frame(destination: bytes, source: bytes, ether_type: int, payload: bytes) -> bytes:
    if len(destination) != 6 or len(source) != 6:
        raise ValueError("Ethernet addresses must be six bytes")
    frame = destination + source + ether_type.to_bytes(2, "big") + payload
    if len(frame) < MIN_ETHERNET_FRAME_BYTES:
        frame += bytes(MIN_ETHERNET_FRAME_BYTES - len(frame))
    if len(frame) > MAX_ETHERNET_FRAME_BYTES:
        raise ValueError("Ethernet frame exceeds the bounded NetworkPort maximum")
    return frame


def build_tcp_frame(
    *,
    source: bytes,
    destination: bytes,
    source_mac: bytes,
    destination_mac: bytes,
    source_port: int,
    destination_port: int,
    sequence: int,
    acknowledgment: int,
    flags: int,
    identification: int,
    data: bytes = b"",
) -> bytes:
    options = MSS_OPTION if flags & SYN else b""
    segment = TCP.tcp_datagram(
        source_port,
        destination_port,
        sequence,
        acknowledgment,
        flags,
        source,
        destination,
        data,
        options,
    )
    datagram = TCP.ipv4_header(identification, source, destination, segment) + segment
    return _ethernet_frame(destination_mac, source_mac, IPV4_ETHER_TYPE, datagram)


def parse_secure_frame(
    frame: bytes, *, expect_source: bytes, expect_destination: bytes
) -> ParsedSecureFrame:
    if not MIN_ETHERNET_FRAME_BYTES <= len(frame) <= MAX_ETHERNET_FRAME_BYTES:
        raise AssertionError("Ethernet frame length is outside the bounded profile")
    if expect_source == LOCAL_IPV4 and expect_destination == PEER_IPV4:
        expected_destination_mac, expected_source_mac = PEER_MAC, DESCRIBED_DEVICE_MAC
        expected_source_port, expected_destination_port = LOCAL_PORT, PEER_PORT
    elif expect_source == PEER_IPV4 and expect_destination == LOCAL_IPV4:
        expected_destination_mac, expected_source_mac = DESCRIBED_DEVICE_MAC, PEER_MAC
        expected_source_port, expected_destination_port = PEER_PORT, LOCAL_PORT
    else:
        raise AssertionError("secure frame IP direction is outside the finite profile")
    if frame[:6] != expected_destination_mac or frame[6:12] != expected_source_mac:
        raise AssertionError("Ethernet direction or address is invalid")
    if int.from_bytes(frame[12:14], "big") != IPV4_ETHER_TYPE:
        raise AssertionError("Ethernet type is not IPv4")
    datagram = frame[14:]
    parsed_ipv4 = TCP.parse_ipv4_datagram(datagram[: int.from_bytes(datagram[2:4], "big")])
    if parsed_ipv4.source != expect_source or parsed_ipv4.destination != expect_destination:
        raise AssertionError("IPv4 direction is invalid")
    if frame[14 + parsed_ipv4.total_length :] != bytes(len(frame) - 14 - parsed_ipv4.total_length):
        raise AssertionError("Ethernet padding is not zero")
    parsed_tcp = TCP.parse_tcp_segment(
        parsed_ipv4.payload, parsed_ipv4.source, parsed_ipv4.destination
    )
    if (parsed_tcp.source_port, parsed_tcp.destination_port) != (
        expected_source_port,
        expected_destination_port,
    ):
        raise AssertionError("TCP direction or port is invalid")
    return ParsedSecureFrame(
        parsed_tcp.sequence,
        parsed_tcp.acknowledgment,
        parsed_tcp.flags,
        parsed_tcp.source_port,
        parsed_tcp.destination_port,
        parsed_tcp.data,
    )


def _drain_bio(bio: ssl.MemoryBIO) -> bytes:
    output = bytearray()
    while bio.pending:
        output.extend(bio.read())
    return bytes(output)


def memory_bio_exchange(*, tamper: bool) -> tuple[bytes, bytes, bool]:
    """Run the host TLS oracle without QEMU for deterministic self-tests."""
    with temporary_tls_fixture() as (certificate, key):
        server_context = make_server_context(certificate, key)
        client_context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
        client_context.minimum_version = ssl.TLSVersion.TLSv1_3
        client_context.maximum_version = ssl.TLSVersion.TLSv1_3
        client_context.check_hostname = False
        client_context.verify_mode = ssl.CERT_NONE
        client_in = ssl.MemoryBIO()
        client_out = ssl.MemoryBIO()
        server_in = ssl.MemoryBIO()
        server_out = ssl.MemoryBIO()
        client = client_context.wrap_bio(client_in, client_out, False, server_hostname="pythos.test")
        server = server_context.wrap_bio(server_in, server_out, True)
        client_done = server_done = False
        for _ in range(128):
            if not client_done:
                try:
                    client.do_handshake()
                    client_done = True
                except ssl.SSLWantReadError:
                    pass
            pending = _drain_bio(client_out)
            if pending:
                server_in.write(pending)
            if not server_done:
                try:
                    server.do_handshake()
                    server_done = True
                except ssl.SSLWantReadError:
                    pass
            pending = _drain_bio(server_out)
            if pending:
                client_in.write(pending)
            if client_done and server_done:
                break
        else:
            raise AssertionError("MemoryBIO TLS handshake did not converge")
        if not client_done or not server_done:
            raise AssertionError("MemoryBIO TLS handshake did not complete")

        if client.version() != "TLSv1.3":
            raise AssertionError(f"TLS version is not TLS 1.3: {client.version()!r}")
        client.write(REQUEST_PLAINTEXT)
        request_wire = _drain_bio(client_out)
        if not request_wire:
            raise AssertionError("TLS client produced no protected request")
        server_in.write(request_wire)
        received = bytearray()
        while True:
            try:
                chunk = server.read(4096)
            except ssl.SSLWantReadError:
                break
            if not chunk:
                break
            received.extend(chunk)
        if bytes(received) != REQUEST_PLAINTEXT:
            raise AssertionError("TLS server did not recover the exact request")
        server.write(RESPONSE_PLAINTEXT)
        response_wire = _drain_bio(server_out)
        if tamper:
            response_wire = tamper_first_application_record(response_wire)
        client_in.write(response_wire)
        try:
            response = client.read(4096)
        except ssl.SSLError:
            response = b""
        return request_wire, response, tamper


def _send_tls_wire(peer: "SecurePeer", connection: socket.socket, wire: bytes, *, tamper: bool) -> None:
    if tamper:
        wire = tamper_first_application_record(wire)
        peer.tamper_applied = True
    for offset in range(0, len(wire), MAX_TCP_PAYLOAD):
        peer._send_tcp(connection, wire[offset : offset + MAX_TCP_PAYLOAD])


class SecurePeer:
    """Host-side ARP/TCP peer with a stdlib TLS 1.3 MemoryBIO server."""

    def __init__(self, port: int = 0, timeout: float = 10.0, *, tamper: bool = False) -> None:
        self.listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind(("127.0.0.1", port))
        self.listener.listen(1)
        self.listener.settimeout(timeout)
        self.port = self.listener.getsockname()[1]
        self.timeout = timeout
        self.tamper = tamper
        self.error: BaseException | None = None
        self.connected = False
        self.completed = False
        self.tls_handshake_complete = False
        self.tamper_applied = False
        self.tx_frames: list[bytes] = []
        self.rx_frames: list[bytes] = []
        self.encrypted_request = b""
        self.encrypted_response = b""
        self.plaintext_request = b""
        self.plaintext_response = b""
        self.client_plaintext_after_response = b""
        self._guest_next = 0
        self._host_next = PEER_ISS
        self._ip_identification = 0x1601
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        if self._thread is not None:
            raise RuntimeError("secure peer is already started")
        self._thread = threading.Thread(target=self._serve_once, daemon=True)
        self._thread.start()

    def join(self, timeout: float | None = None) -> None:
        if self._thread is None:
            raise RuntimeError("secure peer has not been started")
        self._thread.join(timeout)
        if self._thread.is_alive():
            raise TimeoutError("secure peer did not finish")

    def close(self) -> None:
        self.listener.close()

    def _send_frame(self, connection: socket.socket, frame: bytes) -> None:
        self.rx_frames.append(frame)
        connection.sendall(encode_socket_frame(frame))

    def _send_tcp(self, connection: socket.socket, data: bytes = b"", flags: int = ACK) -> None:
        frame = build_tcp_frame(
            source=PEER_IPV4,
            destination=LOCAL_IPV4,
            source_mac=PEER_MAC,
            destination_mac=DESCRIBED_DEVICE_MAC,
            source_port=PEER_PORT,
            destination_port=LOCAL_PORT,
            sequence=self._host_next,
            acknowledgment=self._guest_next,
            flags=flags,
            identification=self._ip_identification,
            data=data,
        )
        self._ip_identification = (self._ip_identification + 1) & 0xFFFF
        sequence_space = 1 if flags & (SYN | FIN) else 0
        self._host_next = (self._host_next + len(data) + sequence_space) & 0xFFFFFFFF
        self._send_frame(connection, frame)

    def _read_guest(self, connection: socket.socket) -> ParsedSecureFrame:
        frame = read_socket_frame(connection)
        self.tx_frames.append(frame)
        parsed = parse_secure_frame(
            frame, expect_source=LOCAL_IPV4, expect_destination=PEER_IPV4
        )
        if parsed.acknowledgment > self._host_next:
            raise AssertionError("guest acknowledged beyond the host send sequence")
        if parsed.sequence != self._guest_next:
            raise AssertionError(
                f"guest TCP sequence is not contiguous: expected {self._guest_next:#x}, got {parsed.sequence:#x}"
            )
        self._guest_next = (self._guest_next + len(parsed.data)) & 0xFFFFFFFF
        if parsed.flags & FIN:
            self._guest_next = (self._guest_next + 1) & 0xFFFFFFFF
        if parsed.data or parsed.flags & FIN:
            self._send_tcp(connection)
        return parsed

    def _handshake_tcp(self, connection: socket.socket) -> None:
        first = read_socket_frame(connection)
        self.tx_frames.append(first)
        TCP.assert_exact_arp_request(DESCRIBED_DEVICE_MAC, first)
        reply = TCP.arp_reply(DESCRIBED_DEVICE_MAC)
        self.rx_frames.append(reply)
        connection.sendall(encode_socket_frame(reply))

        self._guest_next = LOCAL_ISS
        syn = self._read_guest(connection)
        if syn.flags != SYN or syn.acknowledgment != 0:
            raise AssertionError("guest did not begin with the canonical SYN")
        self._guest_next = (syn.sequence + 1) & 0xFFFFFFFF
        self._host_next = PEER_ISS
        self._send_tcp(connection, flags=SYN | ACK)
        ack = self._read_guest(connection)
        if ack.flags != ACK or ack.data or ack.acknowledgment != self._host_next:
            raise AssertionError("guest did not complete the canonical TCP handshake")

    def _read_until_tls_data(
        self, connection: socket.socket, *, tls_done: bool
    ) -> tuple[bytes, bool]:
        while True:
            parsed = self._read_guest(connection)
            if parsed.data:
                if tls_done:
                    self.encrypted_request += parsed.data
                return parsed.data, bool(parsed.flags & FIN)
            if parsed.flags & FIN:
                return b"", True

    def _drive_tls(self, connection: socket.socket, tls: ssl.SSLObject, inbound: ssl.MemoryBIO, outbound: ssl.MemoryBIO) -> None:
        pending = b""
        while not self.tls_handshake_complete:
            if pending:
                inbound.write(pending)
                pending = b""
            try:
                tls.do_handshake()
                self.tls_handshake_complete = True
            except ssl.SSLWantReadError:
                pass
            wire = _drain_bio(outbound)
            if wire:
                _send_tls_wire(self, connection, wire, tamper=False)
            if not self.tls_handshake_complete:
                pending, fin = self._read_until_tls_data(connection, tls_done=False)
                if fin:
                    raise AssertionError("guest closed TCP during the TLS handshake")

        wire = _drain_bio(outbound)
        if wire:
            _send_tls_wire(self, connection, wire, tamper=False)

        while not self.plaintext_request:
            payload, fin = self._read_until_tls_data(connection, tls_done=True)
            if fin:
                raise AssertionError("guest closed TCP before sending the TLS request")
            inbound.write(payload)
            while True:
                try:
                    plaintext = tls.read(4096)
                except ssl.SSLWantReadError:
                    break
                if not plaintext:
                    break
                self.plaintext_request += plaintext
            wire = _drain_bio(outbound)
            if wire:
                _send_tls_wire(self, connection, wire, tamper=False)
        if self.plaintext_request != REQUEST_PLAINTEXT:
            raise AssertionError("TLS request plaintext does not match the finite fixture")

        written = tls.write(RESPONSE_PLAINTEXT)
        if written != len(RESPONSE_PLAINTEXT):
            raise AssertionError("TLS server did not accept the complete response fixture")
        wire = _drain_bio(outbound)
        self.encrypted_response = wire
        if not wire or not any(record[0] == 0x17 for record in tls_records(wire)):
            raise AssertionError("TLS server produced no protected application response")
        _send_tls_wire(self, connection, wire, tamper=self.tamper)
        self.plaintext_response = RESPONSE_PLAINTEXT

        peer_fin = False
        local_fin = False
        while not peer_fin:
            parsed = self._read_guest(connection)
            if parsed.data:
                if self.tamper:
                    try:
                        inbound.write(parsed.data)
                        while True:
                            plaintext = tls.read(4096)
                            if not plaintext:
                                break
                            self.client_plaintext_after_response += plaintext
                    except ssl.SSLError:
                        pass
                else:
                    inbound.write(parsed.data)
                    try:
                        while True:
                            plaintext = tls.read(4096)
                            if not plaintext:
                                break
                            self.client_plaintext_after_response += plaintext
                    except ssl.SSLError:
                        pass
            if parsed.flags & FIN:
                peer_fin = True
                if not local_fin:
                    self._send_tcp(connection, flags=FIN | ACK)
                    local_fin = True
        if not local_fin:
            self._send_tcp(connection, flags=FIN | ACK)
        for _ in range(4):
            final = self._read_guest(connection)
            if final.acknowledgment == self._host_next:
                self.completed = True
                return
            if final.flags & FIN:
                continue
        raise AssertionError("guest did not acknowledge the host FIN")

    def _serve_once(self) -> None:
        try:
            ready, _, _ = select.select([self.listener], [], [], self.timeout)
            if not ready:
                raise TimeoutError("secure peer did not accept the QEMU transport connection")
            connection, _ = self.listener.accept()
            with connection:
                self.connected = True
                connection.settimeout(self.timeout)
                with temporary_tls_fixture() as (certificate, key):
                    server_context = make_server_context(certificate, key)
                    inbound = ssl.MemoryBIO()
                    outbound = ssl.MemoryBIO()
                    tls = server_context.wrap_bio(inbound, outbound, True)
                    self._handshake_tcp(connection)
                    self._drive_tls(connection, tls, inbound, outbound)
        except BaseException as error:
            self.error = error
        finally:
            self.listener.close()


def load_socket_acceptance():
    path = ROOT / "scripts" / "test-socket.py"
    spec = importlib.util.spec_from_file_location("secure_socket_acceptance", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load test-socket.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SOCKET = load_socket_acceptance()
DeniedPeer = SOCKET.DeniedPeer


def assert_denied_peer(peer: DeniedPeer) -> None:
    SOCKET.assert_denied_peer(peer)


def assert_exact_ordered_markers(serial: str, markers: tuple[str, ...]) -> None:
    observed = [line for line in serial.splitlines() if line.startswith("PYTHOS:CORE:SECURE")]
    if observed != list(markers):
        raise AssertionError(f"secure markers were not exact and ordered: {observed!r}")


def assert_no_forbidden_evidence(serial: str, qemu_output: str) -> None:
    evidence = serial + "\n" + qemu_output
    for marker in FORBIDDEN_EVIDENCE:
        if marker in evidence:
            raise AssertionError(f"forbidden secure acceptance evidence: {marker}")
    TCP.VIRTIO_NET.assert_no_storage_path_markers(evidence)


def assert_runner_success(returncode: int | None, qemu_output: str) -> None:
    outcomes = [line for line in qemu_output.splitlines() if "QEMU_OUTCOME" in line]
    if returncode != 0 or outcomes != ["QEMU_OUTCOME success"]:
        raise AssertionError(
            f"expected one exact success outcome, returncode={returncode}, outcomes={outcomes!r}"
        )


def _assert_case(serial: str, qemu_output: str, markers: tuple[str, ...]) -> None:
    assert_exact_ordered_markers(serial, markers)
    assert_no_forbidden_evidence(serial, qemu_output)
    assert_runner_success(0, qemu_output)


def assert_granted_acceptance(serial: str, qemu_output: str) -> None:
    _assert_case(serial, qemu_output, GRANTED_REQUIRED_MARKERS)


def assert_tamper_acceptance(serial: str, qemu_output: str) -> None:
    _assert_case(serial, qemu_output, TAMPER_REQUIRED_MARKERS)


def assert_denied_acceptance(serial: str, qemu_output: str) -> None:
    _assert_case(serial, qemu_output, DENIED_REQUIRED_MARKERS)


def assert_secure_frame_counts(peer: SecurePeer) -> None:
    tx_count = len(peer.tx_frames)
    rx_count = len(peer.rx_frames)
    total_count = tx_count + rx_count
    if (tx_count, rx_count, total_count) != (11, 10, 21):
        raise AssertionError(
            "secure frame count mismatch: "
            f"expected tx=11 rx=10 total=21, got "
            f"tx={tx_count} rx={rx_count} total={total_count}"
        )


def assert_peer_exchange(peer: SecurePeer, case: str) -> None:
    if isinstance(peer, DeniedPeer):
        assert_denied_peer(peer)
        return
    if peer.error is not None:
        raise AssertionError(f"secure frame peer failed: {peer.error}") from peer.error
    if not peer.connected or not peer.completed:
        raise AssertionError("secure frame peer did not connect and finish cleanly")
    assert_secure_frame_counts(peer)
    TCP.assert_exact_arp_request(DESCRIBED_DEVICE_MAC, peer.tx_frames[0])
    TCP.assert_exact_arp_reply(DESCRIBED_DEVICE_MAC, peer.rx_frames[0])
    for frame in peer.tx_frames[1:]:
        parse_secure_frame(frame, expect_source=LOCAL_IPV4, expect_destination=PEER_IPV4)
    for frame in peer.rx_frames[1:]:
        parsed = parse_secure_frame(
            frame, expect_source=PEER_IPV4, expect_destination=LOCAL_IPV4
        )
        if parsed.source_port != PEER_PORT or parsed.destination_port != LOCAL_PORT:
            raise AssertionError("secure host frame ports are not reversed")
    if not peer.tls_handshake_complete:
        raise AssertionError("secure peer did not complete TLS handshake")
    if REQUEST_PLAINTEXT in peer.encrypted_request:
        raise AssertionError("TLS request wire exposed plaintext")
    if RESPONSE_PLAINTEXT in peer.encrypted_response:
        raise AssertionError("TLS response wire exposed plaintext")
    if peer.plaintext_request != REQUEST_PLAINTEXT:
        raise AssertionError("TLS request plaintext was not exact")
    if case == "tamper":
        if not peer.tamper_applied or peer.client_plaintext_after_response:
            raise AssertionError("tampered TLS response released application plaintext")
    else:
        if peer.tamper_applied:
            raise AssertionError("granted TLS case unexpectedly tampered with response")


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


def configure_case(case: str) -> None:
    global ASSERT_ACCEPTANCE, CASE, CONSUMER_MARKERS, ESP_IMAGE, KERNEL_MARKERS
    global PEER_FACTORY, REQUIRED_MARKERS, SERIAL_LOG, SUCCESS_MARKER
    CASE = case
    if case == "granted":
        SERIAL_LOG = TARGET / "secure-granted-com1.log"
        ESP_IMAGE = TARGET / "secure-granted-com1-esp.img"
        SUCCESS_MARKER = GRANTED_REQUIRED_MARKERS[-1]
        REQUIRED_MARKERS = GRANTED_REQUIRED_MARKERS
        CONSUMER_MARKERS = GRANTED_CONSUMER_MARKERS
        KERNEL_MARKERS = GRANTED_KERNEL_MARKERS
        PEER_FACTORY = lambda port=0, timeout=10.0: SecurePeer(port, timeout, tamper=False)
        ASSERT_ACCEPTANCE = assert_granted_acceptance
    elif case == "tamper":
        SERIAL_LOG = TARGET / "secure-tamper-com1.log"
        ESP_IMAGE = TARGET / "secure-tamper-com1-esp.img"
        SUCCESS_MARKER = TAMPER_REQUIRED_MARKERS[-1]
        REQUIRED_MARKERS = TAMPER_REQUIRED_MARKERS
        CONSUMER_MARKERS = TAMPER_CONSUMER_MARKERS
        KERNEL_MARKERS = TAMPER_KERNEL_MARKERS
        PEER_FACTORY = lambda port=0, timeout=10.0: SecurePeer(port, timeout, tamper=True)
        ASSERT_ACCEPTANCE = assert_tamper_acceptance
    elif case == "denied":
        SERIAL_LOG = TARGET / "secure-denied-com1.log"
        ESP_IMAGE = TARGET / "secure-denied-com1-esp.img"
        SUCCESS_MARKER = DENIED_REQUIRED_MARKERS[-1]
        REQUIRED_MARKERS = DENIED_REQUIRED_MARKERS
        CONSUMER_MARKERS = DENIED_CONSUMER_MARKERS
        KERNEL_MARKERS = DENIED_KERNEL_MARKERS
        PEER_FACTORY = lambda port=0, timeout=10.0: DeniedPeer(
            port, timeout, allow_qemu_reset=True
        )
        ASSERT_ACCEPTANCE = assert_denied_acceptance
    else:
        raise ValueError(f"unknown secure proof case: {case}")

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
    IPV4.assert_peer_exchange = lambda peer: assert_peer_exchange(peer, CASE)
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
    probe_dir = ROOT / "target" / "secure-transport-probe"
    probe = probe_dir / "secure-transport-probe.elf"
    shell = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run([
        "cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none",
        "--no-default-features", "--features", feature,
    ])
    run([sys.executable, "scripts/build-secure-transport-probe.py"])
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


class SecureTransportSelfTest(unittest.TestCase):
    def test_exact_markers_and_runner_oracle(self) -> None:
        configure_case("granted")
        assert_granted_acceptance("\n".join(GRANTED_REQUIRED_MARKERS), "QEMU_OUTCOME success\n")
        with self.assertRaises(AssertionError):
            assert_granted_acceptance(
                "\n".join((*GRANTED_REQUIRED_MARKERS[:2], *GRANTED_REQUIRED_MARKERS[3:])),
                "QEMU_OUTCOME success\n",
            )
        configure_case("tamper")
        assert_tamper_acceptance("\n".join(TAMPER_REQUIRED_MARKERS), "QEMU_OUTCOME success\n")
        configure_case("denied")
        assert_denied_acceptance("\n".join(DENIED_REQUIRED_MARKERS), "QEMU_OUTCOME success\n")

    def test_exact_tcp_frame_and_checksum_oracle(self) -> None:
        frame = build_tcp_frame(
            source=PEER_IPV4,
            destination=LOCAL_IPV4,
            source_mac=PEER_MAC,
            destination_mac=DESCRIBED_DEVICE_MAC,
            source_port=PEER_PORT,
            destination_port=LOCAL_PORT,
            sequence=PEER_ISS,
            acknowledgment=LOCAL_ISS + 1,
            flags=ACK,
            identification=0x1701,
            data=b"fixture",
        )
        self.assertEqual(
            parse_secure_frame(frame, expect_source=PEER_IPV4, expect_destination=LOCAL_IPV4).data,
            b"fixture",
        )
        bad = bytearray(frame)
        bad[34 + 16] ^= 1
        with self.assertRaises(AssertionError):
            parse_secure_frame(bytes(bad), expect_source=PEER_IPV4, expect_destination=LOCAL_IPV4)

    def test_memory_bio_authenticated_request_response_and_tamper(self) -> None:
        request_wire, response, tampered = memory_bio_exchange(tamper=False)
        self.assertNotIn(REQUEST_PLAINTEXT, request_wire)
        self.assertEqual(response, RESPONSE_PLAINTEXT)
        self.assertFalse(tampered)
        request_wire, response, tampered = memory_bio_exchange(tamper=True)
        self.assertNotIn(REQUEST_PLAINTEXT, request_wire)
        self.assertEqual(response, b"")
        self.assertTrue(tampered)

    def test_denied_peer_has_no_frame_path(self) -> None:
        peer = DeniedPeer(timeout=1.0, allow_qemu_reset=True)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0):
                pass
            peer.join(timeout=2.0)
            assert_denied_peer(peer)
        finally:
            peer.close()


def run_self_tests() -> int:
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(SecureTransportSelfTest)
    )
    if result.wasSuccessful():
        print("SECURE_TRANSPORT_QEMU_ACCEPTANCE_OK")
        return 0
    return 1


def main() -> int:
    for case, feature in (
        ("granted", "secure-transport-probe"),
        ("tamper", "secure-transport-tamper-probe"),
        ("denied", "secure-transport-denied-probe"),
    ):
        configure_case(case)
        loader, kernel, probe, shell = build_probe_image(feature)
        consumer, kernel_serial, qemu_output, peer = IPV4.run_probe_boot()
        print(
            f"SECURE_LIVE_CASE {case} "
            f"tx={len(peer.tx_frames)} rx={len(peer.rx_frames)} "
            f"total={len(peer.tx_frames) + len(peer.rx_frames)}"
        )
        observed = consumer.splitlines() + kernel_serial.splitlines()
        marker_evidence = [line for line in observed if line in REQUIRED_MARKERS]
        print(f"SECURE_LIVE_MARKERS {case} {' > '.join(marker_evidence)}")
        print(
            f"SECURE_LIVE_OUTCOME {case} "
            f"{next(line for line in qemu_output.splitlines() if line == 'QEMU_OUTCOME success')}"
        )
        print(
            f"SECURE_LIVE_CLEANUP {case} "
            f"serial_log_exists={SERIAL_LOG.exists()} esp_snapshot_exists={ESP_IMAGE.exists()}"
        )
        if SERIAL_LOG.exists() or ESP_IMAGE.exists():
            raise AssertionError(f"{case} proof left acceptance artifacts behind")
        print(f"SECURE_ARTIFACT {case} loader={loader.resolve()}")
        print(f"SECURE_ARTIFACT {case} kernel={kernel.resolve()}")
        print(f"SECURE_ARTIFACT {case} probe={probe.resolve()}")
        print(f"SECURE_ARTIFACT {case} shell={shell.resolve()}")
    print("SECURE_TRANSPORT_QEMU_ACCEPTANCE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-secure-transport.py [--self-test]")
    raise SystemExit(main())
