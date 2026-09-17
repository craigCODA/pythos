from __future__ import annotations

import importlib.util
import socket
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_module(name: str, relative_path: str):
    path = ROOT / relative_path
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {relative_path}")
    module = importlib.util.module_from_spec(spec)
    scripts_dir = str(ROOT / "scripts")
    inserted_scripts_dir = scripts_dir not in sys.path
    if inserted_scripts_dir:
        sys.path.insert(0, scripts_dir)
    try:
        spec.loader.exec_module(module)
    finally:
        if inserted_scripts_dir:
            sys.path.remove(scripts_dir)
    return module


IPV4 = load_module("ipv4_acceptance", "scripts/test-ipv4.py")


EXPECTED_MARKERS = (
    "PYTHOS:CORE:IPV4:BOOTSTRAPPED",
    "PYTHOS:CORE:IPV4:DESCRIBE_OK",
    "PYTHOS:CORE:IPV4:ARP_SETUP_OK",
    "PYTHOS:CORE:IPV4:TX_OK",
    "PYTHOS:CORE:IPV4:RX_OK",
    "PYTHOS:CORE:IPV4:TEARDOWN_REVOKED",
    "PYTHOS:CORE:IPV4_READY",
)
LOCAL_MAC = bytes.fromhex("525400123456")
PEER_MAC = bytes.fromhex("020000000002")
LOCAL_IPV4 = bytes.fromhex("c0a80e02")
PEER_IPV4 = bytes.fromhex("c0a80e01")
REQUEST_HEADER = bytes.fromhex("4500001c1401000040fdc890c0a80e02c0a80e01")
REPLY_HEADER = bytes.fromhex("4500001c1402000040fdc88fc0a80e01c0a80e02")
ARP_REQUEST = (
    bytes.fromhex("ffffffffffff5254001234560806")
    + bytes.fromhex("0001080006040001525400123456c0a80e02000000000000c0a80e01")
    + bytes(18)
)
ARP_REPLY = (
    bytes.fromhex("5254001234560200000000020806")
    + bytes.fromhex("0001080006040002020000000002c0a80e01525400123456c0a80e02")
    + bytes(18)
)
IPV4_REQUEST = bytes.fromhex("0200000000025254001234560800") + REQUEST_HEADER + b"PYTHIPRQ" + bytes(18)
IPV4_REPLY = bytes.fromhex("5254001234560200000000020800") + REPLY_HEADER + b"PYTHIPRP" + bytes(18)


def repaired(frame: bytes) -> bytes:
    changed = bytearray(frame)
    changed[24:26] = b"\x00\x00"
    changed[24:26] = IPV4.ones_complement_checksum(changed[14:34]).to_bytes(2, "big")
    return bytes(changed)


class Ipv4HostTest(unittest.TestCase):
    def valid_serial(self) -> str:
        return "\n".join(EXPECTED_MARKERS)

    def test_exact_constants_and_marker_contract_are_frozen(self) -> None:
        self.assertEqual(IPV4.PEER_MAC, PEER_MAC)
        self.assertEqual(IPV4.DESCRIBED_DEVICE_MAC, LOCAL_MAC)
        self.assertEqual(IPV4.LOCAL_IPV4, LOCAL_IPV4)
        self.assertEqual(IPV4.PEER_IPV4, PEER_IPV4)
        self.assertEqual(IPV4.IPV4_ETHER_TYPE, 0x0800)
        self.assertEqual(IPV4.IPV4_PROTOCOL, 253)
        self.assertEqual(IPV4.IPV4_REQUEST_ID, 0x1401)
        self.assertEqual(IPV4.IPV4_REPLY_ID, 0x1402)
        self.assertEqual(IPV4.IPV4_REQUEST_PAYLOAD, b"PYTHIPRQ")
        self.assertEqual(IPV4.IPV4_REPLY_PAYLOAD, b"PYTHIPRP")
        self.assertEqual(IPV4.MIN_ETHERNET_FRAME_BYTES, 60)
        self.assertEqual(IPV4.REQUIRED_MARKERS, EXPECTED_MARKERS)

    def test_ones_complement_checksum_and_exact_headers(self) -> None:
        self.assertEqual(IPV4.ones_complement_checksum(REQUEST_HEADER[:10] + b"\x00\x00" + REQUEST_HEADER[12:]), 0xC890)
        self.assertEqual(IPV4.ones_complement_checksum(REPLY_HEADER[:10] + b"\x00\x00" + REPLY_HEADER[12:]), 0xC88F)
        self.assertEqual(IPV4.ones_complement_checksum(REQUEST_HEADER), 0)
        self.assertEqual(IPV4.ipv4_header(0x1401, LOCAL_IPV4, PEER_IPV4, b"PYTHIPRQ"), REQUEST_HEADER)
        self.assertEqual(IPV4.ipv4_header(0x1402, PEER_IPV4, LOCAL_IPV4, b"PYTHIPRP"), REPLY_HEADER)
        with self.assertRaises(ValueError):
            IPV4.ones_complement_checksum(b"odd")

    def test_exact_arp_and_ipv4_frames_are_sixty_bytes_and_zero_padded(self) -> None:
        self.assertEqual(IPV4.arp_request(LOCAL_MAC), ARP_REQUEST)
        self.assertEqual(IPV4.arp_reply(LOCAL_MAC), ARP_REPLY)
        self.assertEqual(IPV4.ipv4_request(LOCAL_MAC), IPV4_REQUEST)
        self.assertEqual(IPV4.ipv4_reply(LOCAL_MAC), IPV4_REPLY)
        for frame in (ARP_REQUEST, ARP_REPLY, IPV4_REQUEST, IPV4_REPLY):
            self.assertEqual(len(frame), 60)
            self.assertEqual(frame[42:], bytes(18))

    def test_reply_reverses_exact_link_and_network_fields(self) -> None:
        packet = IPV4.parse_ipv4_datagram(IPV4_REPLY[14:42])
        self.assertEqual(IPV4_REPLY[:6], LOCAL_MAC)
        self.assertEqual(IPV4_REPLY[6:12], PEER_MAC)
        self.assertEqual(packet.identification, 0x1402)
        self.assertEqual(packet.source, PEER_IPV4)
        self.assertEqual(packet.destination, LOCAL_IPV4)
        self.assertEqual(packet.protocol, 253)
        self.assertEqual(packet.payload, b"PYTHIPRP")

    def test_parser_rejects_malformed_version_ihl_and_lengths(self) -> None:
        cases: list[bytes] = []
        short = REQUEST_HEADER[:19]
        cases.append(short)
        for first in (0x55, 0x44, 0x46):
            datagram = bytearray(REQUEST_HEADER + b"PYTHIPRQ")
            datagram[0] = first
            datagram[10:12] = b"\x00\x00"
            datagram[10:12] = IPV4.ones_complement_checksum(datagram[:20]).to_bytes(2, "big")
            cases.append(bytes(datagram))
        for total_length in (19, 29):
            datagram = bytearray(REQUEST_HEADER + b"PYTHIPRQ")
            datagram[2:4] = total_length.to_bytes(2, "big")
            datagram[10:12] = b"\x00\x00"
            datagram[10:12] = IPV4.ones_complement_checksum(datagram[:20]).to_bytes(2, "big")
            cases.append(bytes(datagram))
        for datagram in cases:
            with self.subTest(datagram=datagram.hex()), self.assertRaises(AssertionError):
                IPV4.parse_ipv4_datagram(datagram)

    def test_parser_rejects_bad_checksum_fragments_and_options(self) -> None:
        bad_checksum = bytearray(REQUEST_HEADER + b"PYTHIPRQ")
        bad_checksum[10] ^= 1
        fragment = bytearray(REQUEST_HEADER + b"PYTHIPRQ")
        fragment[6:8] = b"\x20\x00"
        fragment[10:12] = b"\x00\x00"
        fragment[10:12] = IPV4.ones_complement_checksum(fragment[:20]).to_bytes(2, "big")
        options = bytearray(REQUEST_HEADER + b"\x00\x00\x00\x00PYTHIPRQ")
        options[0] = 0x46
        options[2:4] = (32).to_bytes(2, "big")
        options[10:12] = b"\x00\x00"
        options[10:12] = IPV4.ones_complement_checksum(options[:24]).to_bytes(2, "big")
        for datagram in (bad_checksum, fragment, options):
            with self.subTest(datagram=bytes(datagram).hex()), self.assertRaises(AssertionError):
                IPV4.parse_ipv4_datagram(bytes(datagram))

    def test_exact_request_and_reply_validators_reject_wrong_relationships(self) -> None:
        IPV4.assert_exact_arp_request(LOCAL_MAC, ARP_REQUEST)
        IPV4.assert_exact_arp_reply(LOCAL_MAC, ARP_REPLY)
        IPV4.assert_exact_ipv4_request(LOCAL_MAC, IPV4_REQUEST)
        IPV4.assert_exact_ipv4_reply(LOCAL_MAC, IPV4_REPLY)

        arp_wrong_mac = bytearray(ARP_REQUEST)
        arp_wrong_mac[11] ^= 1
        with self.assertRaises(AssertionError):
            IPV4.assert_exact_arp_request(LOCAL_MAC, bytes(arp_wrong_mac))

        mutations = {
            "destination MAC": (0, 1, False),
            "source MAC": (11, 1, False),
            "EtherType": (13, 1, False),
            "source address": (29, 1, True),
            "destination address": (33, 1, True),
            "protocol": (23, 1, True),
            "payload": (41, 1, False),
            "padding": (42, 1, False),
        }
        for name, (offset, delta, repair_checksum) in mutations.items():
            frame = bytearray(IPV4_REPLY)
            frame[offset] ^= delta
            candidate = repaired(bytes(frame)) if repair_checksum else bytes(frame)
            with self.subTest(name=name), self.assertRaises(AssertionError):
                IPV4.assert_exact_ipv4_reply(LOCAL_MAC, candidate)

    def test_marker_oracle_requires_exactly_one_ordered_success(self) -> None:
        IPV4.assert_ipv4_acceptance(self.valid_serial(), "QEMU_OUTCOME success\n")
        invalid = (
            "\n".join((*EXPECTED_MARKERS[:2], *EXPECTED_MARKERS[3:])),
            "\n".join((*EXPECTED_MARKERS[:2], EXPECTED_MARKERS[3], EXPECTED_MARKERS[2], *EXPECTED_MARKERS[4:])),
            self.valid_serial() + "\n" + EXPECTED_MARKERS[-1],
        )
        for serial in invalid:
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                IPV4.assert_ipv4_acceptance(serial, "QEMU_OUTCOME success\n")

    def test_oracle_rejects_error_storage_and_non_success_outcome(self) -> None:
        for evidence in (
            "PYTHOS:CORE:IPV4:ERROR",
            "PYTHOS:PANIC",
            "TRANSPORT_ERROR",
            "PYTHOS:CORE:BLOCK_DEVICE_READY",
        ):
            with self.subTest(evidence=evidence), self.assertRaises(AssertionError):
                IPV4.assert_ipv4_acceptance(self.valid_serial() + "\n" + evidence, "QEMU_OUTCOME success\n")
        for returncode, output in (
            (22, "QEMU_OUTCOME timeout\n"),
            (1, "QEMU_OUTCOME success\n"),
            (0, "QEMU_OUTCOME success\nQEMU_OUTCOME success\n"),
        ):
            with self.subTest(returncode=returncode, output=output), self.assertRaises(AssertionError):
                IPV4.assert_runner_success(returncode, output)

    def test_send_ipv4_reply_emits_one_exact_socket_frame(self) -> None:
        reader, writer = socket.socketpair()
        try:
            IPV4.send_ipv4_reply(writer, LOCAL_MAC)
            self.assertEqual(IPV4.read_socket_frame(reader), IPV4_REPLY)
        finally:
            reader.close()
            writer.close()

    def test_loopback_peer_rejects_extra_transmitted_frame(self) -> None:
        peer = IPV4.Ipv4Peer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(IPV4.encode_socket_frame(ARP_REQUEST))
                self.assertEqual(IPV4.read_socket_frame(connection), ARP_REPLY)
                connection.sendall(IPV4.encode_socket_frame(IPV4_REQUEST))
                self.assertEqual(IPV4.read_socket_frame(connection), IPV4_REPLY)
                connection.sendall(IPV4.encode_socket_frame(IPV4_REQUEST))
            peer.join(timeout=1.0)
            self.assertIsNotNone(peer.error)
            self.assertIn("additional TX bytes", str(peer.error))
        finally:
            peer.close()

    def test_runner_command_requires_peer_com2_and_no_virtio_block_disk(self) -> None:
        command = IPV4.probe_runner_command(peer_port=4595, shell_port=4596)
        self.assertIn("--no-virtio-blk", command)
        self.assertIn("--virtio-net", command)
        self.assertEqual(command[command.index("--virtio-net-peer-port") + 1], "4595")
        self.assertEqual(command[command.index("--shell-port") + 1], "4596")
        self.assertEqual(command[command.index("--expect-outcome") + 1], "success")

    def test_reused_cleanup_reaps_a_runner_process(self) -> None:
        kwargs: dict[str, object] = {"cwd": ROOT}
        if sys.platform != "win32":
            kwargs["start_new_session"] = True
        runner = IPV4.spawn_runner_process(
            [sys.executable, "-c", "import time; time.sleep(30)"], **kwargs
        )
        IPV4.cleanup_runner_process(runner)
        if runner.process.stdout is not None:
            runner.process.stdout.close()
        self.assertIsNotNone(runner.process.poll())


if __name__ == "__main__":
    unittest.main()
