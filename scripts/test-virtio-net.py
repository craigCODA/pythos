#!/usr/bin/env python
"""Loopback-only QEMU socket peer for the bounded virtio-net probe."""

from __future__ import annotations

import socket
import threading


MIN_ETHERNET_FRAME_BYTES = 60
MAX_ETHERNET_FRAME_BYTES = 1514
PEER_MAC = bytes.fromhex("020000000002")
PROBE_ETHER_TYPE = bytes.fromhex("88b5")
TX_PAYLOAD = b"PYTHOS:NIC:TX"
RX_PAYLOAD = b"PYTHOS:NIC:RX"


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
                connection.settimeout(self.timeout)
                self.tx_frame = read_socket_frame(connection)
                device_mac = validate_transmitted_probe_frame(self.tx_frame)
                self.rx_frame = probe_frame(device_mac, PEER_MAC, RX_PAYLOAD)
                connection.sendall(encode_socket_frame(self.rx_frame))
        except BaseException as error:
            self.error = error
        finally:
            self.listener.close()
