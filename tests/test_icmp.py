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


try:
    ICMP = load_module("icmp_acceptance", "scripts/test-icmp.py")
    LOAD_ERROR: BaseException | None = None
except BaseException as error:
    ICMP = None
    LOAD_ERROR = error


EXPECTED_MARKERS = (
    "PYTHOS:CORE:ICMP:BOOTSTRAPPED",
    "PYTHOS:CORE:ICMP:DESCRIBE_OK",
    "PYTHOS:CORE:ICMP:ARP_SETUP_OK",
    "PYTHOS:CORE:ICMP:TX_OK",
    "PYTHOS:CORE:ICMP:RX_OK",
    "PYTHOS:CORE:ICMP:TEARDOWN_REVOKED",
    "PYTHOS:CORE:ICMP_READY",
)
LOCAL_MAC = bytes.fromhex("525400123456")
PEER_MAC = bytes.fromhex("020000000002")
LOCAL_IPV4 = bytes.fromhex("c0a80e02")
PEER_IPV4 = bytes.fromhex("c0a80e01")
REQUEST_HEADER = bytes.fromhex("45000024140300004001c982c0a80e02c0a80e01")
REPLY_HEADER = bytes.fromhex("45000024140400004001c981c0a80e01c0a80e02")
REQUEST_MESSAGE = bytes.fromhex("0800a8c614030001") + b"PYTHICMP"
REPLY_MESSAGE = bytes.fromhex("0000b0c614030001") + b"PYTHICMP"
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
ICMP_REQUEST = bytes.fromhex("0200000000025254001234560800") + REQUEST_HEADER + REQUEST_MESSAGE + bytes(10)
ICMP_REPLY = bytes.fromhex("5254001234560200000000020800") + REPLY_HEADER + REPLY_MESSAGE + bytes(10)


def repair_ipv4(frame: bytes) -> bytes:
    changed = bytearray(frame)
    changed[24:26] = b"\x00\x00"
    changed[24:26] = ICMP.ones_complement_checksum(changed[14:34]).to_bytes(2, "big")
    return bytes(changed)


def repair_icmp(frame: bytes) -> bytes:
    changed = bytearray(frame)
    changed[36:38] = b"\x00\x00"
    changed[36:38] = ICMP.ones_complement_checksum(changed[34:50]).to_bytes(2, "big")
    return bytes(changed)


class IcmpHostTest(unittest.TestCase):
    def setUp(self) -> None:
        if LOAD_ERROR is not None:
            self.fail(f"ICMP acceptance harness could not be loaded: {LOAD_ERROR}")

    def valid_serial(self) -> str:
        return "\n".join(EXPECTED_MARKERS)

    def test_exact_constants_and_marker_contract_are_frozen(self) -> None:
        self.assertEqual(ICMP.PEER_MAC, PEER_MAC)
        self.assertEqual(ICMP.DESCRIBED_DEVICE_MAC, LOCAL_MAC)
        self.assertEqual(ICMP.LOCAL_IPV4, LOCAL_IPV4)
        self.assertEqual(ICMP.PEER_IPV4, PEER_IPV4)
        self.assertEqual(ICMP.IPV4_ETHER_TYPE, 0x0800)
        self.assertEqual(ICMP.IPV4_PROTOCOL, 1)
        self.assertEqual(ICMP.IPV4_REQUEST_ID, 0x1403)
        self.assertEqual(ICMP.IPV4_REPLY_ID, 0x1404)
        self.assertEqual(ICMP.ICMP_IDENTIFIER, 0x1403)
        self.assertEqual(ICMP.ICMP_SEQUENCE, 1)
        self.assertEqual(ICMP.ICMP_DATA, b"PYTHICMP")
        self.assertEqual(ICMP.MIN_ETHERNET_FRAME_BYTES, 60)
        self.assertEqual(ICMP.REQUIRED_MARKERS, EXPECTED_MARKERS)

    def test_exact_headers_messages_and_checksums(self) -> None:
        self.assertEqual(ICMP.ones_complement_checksum(REQUEST_HEADER[:10] + b"\x00\x00" + REQUEST_HEADER[12:]), 0xC982)
        self.assertEqual(ICMP.ones_complement_checksum(REPLY_HEADER[:10] + b"\x00\x00" + REPLY_HEADER[12:]), 0xC981)
        self.assertEqual(ICMP.ones_complement_checksum(REQUEST_MESSAGE[:2] + b"\x00\x00" + REQUEST_MESSAGE[4:]), 0xA8C6)
        self.assertEqual(ICMP.ones_complement_checksum(REPLY_MESSAGE[:2] + b"\x00\x00" + REPLY_MESSAGE[4:]), 0xB0C6)
        self.assertEqual(ICMP.ones_complement_checksum(REQUEST_HEADER), 0)
        self.assertEqual(ICMP.ones_complement_checksum(REQUEST_MESSAGE), 0)
        self.assertEqual(ICMP.icmp_echo(8), REQUEST_MESSAGE)
        self.assertEqual(ICMP.icmp_echo(0), REPLY_MESSAGE)

    def test_exact_arp_and_icmp_frames_are_sixty_bytes_and_zero_padded(self) -> None:
        self.assertEqual(ICMP.arp_request(LOCAL_MAC), ARP_REQUEST)
        self.assertEqual(ICMP.arp_reply(LOCAL_MAC), ARP_REPLY)
        self.assertEqual(ICMP.icmp_request(LOCAL_MAC), ICMP_REQUEST)
        self.assertEqual(ICMP.icmp_reply(LOCAL_MAC), ICMP_REPLY)
        for frame in (ARP_REQUEST, ARP_REPLY, ICMP_REQUEST, ICMP_REPLY):
            self.assertEqual(len(frame), 60)
        self.assertEqual(ICMP_REQUEST[50:], bytes(10))
        self.assertEqual(ICMP_REPLY[50:], bytes(10))

    def test_parsers_reject_malformed_version_ihl_length_checksum_and_fragment(self) -> None:
        valid = bytearray(REQUEST_HEADER + REQUEST_MESSAGE)
        cases = [bytes(valid[:19])]
        for first in (0x55, 0x44, 0x46):
            datagram = bytearray(valid)
            datagram[0] = first
            datagram[10:12] = b"\x00\x00"
            datagram[10:12] = ICMP.ones_complement_checksum(datagram[:20]).to_bytes(2, "big")
            cases.append(bytes(datagram))
        for total_length in (19, 35, 37):
            datagram = bytearray(valid)
            datagram[2:4] = total_length.to_bytes(2, "big")
            datagram[10:12] = b"\x00\x00"
            datagram[10:12] = ICMP.ones_complement_checksum(datagram[:20]).to_bytes(2, "big")
            cases.append(bytes(datagram))
        bad_checksum = bytearray(valid)
        bad_checksum[10] ^= 1
        cases.append(bytes(bad_checksum))
        fragment = bytearray(valid)
        fragment[6:8] = b"\x20\x00"
        fragment[10:12] = b"\x00\x00"
        fragment[10:12] = ICMP.ones_complement_checksum(fragment[:20]).to_bytes(2, "big")
        cases.append(bytes(fragment))
        for datagram in cases:
            with self.subTest(datagram=datagram.hex()), self.assertRaises(AssertionError):
                ICMP.parse_ipv4_datagram(datagram)

    def test_icmp_parser_rejects_malformed_type_code_checksum_id_sequence_and_data(self) -> None:
        for name, offset in (("type", 0), ("code", 1), ("identifier", 5), ("sequence", 7), ("data", 15)):
            message = bytearray(REPLY_MESSAGE)
            message[offset] ^= 1
            if name != "checksum":
                message[2:4] = b"\x00\x00"
                message[2:4] = ICMP.ones_complement_checksum(message).to_bytes(2, "big")
            with self.subTest(name=name), self.assertRaises(AssertionError):
                ICMP.parse_icmp_echo(bytes(message), expected_type=0)
        bad_checksum = bytearray(REPLY_MESSAGE)
        bad_checksum[2] ^= 1
        with self.assertRaises(AssertionError):
            ICMP.parse_icmp_echo(bytes(bad_checksum), expected_type=0)
        for wrong_length in (REPLY_MESSAGE[:-1], REPLY_MESSAGE + b"\x00"):
            with self.assertRaises(AssertionError):
                ICMP.parse_icmp_echo(wrong_length, expected_type=0)

    def test_exact_validators_reject_wrong_mac_ethertype_address_protocol_padding_and_length(self) -> None:
        ICMP.assert_exact_arp_request(LOCAL_MAC, ARP_REQUEST)
        ICMP.assert_exact_arp_reply(LOCAL_MAC, ARP_REPLY)
        ICMP.assert_exact_icmp_request(LOCAL_MAC, ICMP_REQUEST)
        ICMP.assert_exact_icmp_reply(LOCAL_MAC, ICMP_REPLY)
        mutations = {
            "destination MAC": (0, False, False),
            "source MAC": (11, False, False),
            "EtherType": (13, False, False),
            "source address": (29, True, False),
            "destination address": (33, True, False),
            "protocol": (23, True, False),
            "ICMP data": (49, False, True),
            "padding": (50, False, False),
        }
        for name, (offset, fix_ip, fix_icmp) in mutations.items():
            frame = bytearray(ICMP_REPLY)
            frame[offset] ^= 1
            candidate = bytes(frame)
            if fix_ip:
                candidate = repair_ipv4(candidate)
            if fix_icmp:
                candidate = repair_icmp(candidate)
            with self.subTest(name=name), self.assertRaises(AssertionError):
                ICMP.assert_exact_icmp_reply(LOCAL_MAC, candidate)
        for frame in (ICMP_REPLY[:-1], ICMP_REPLY + b"\x00"):
            with self.assertRaises(AssertionError):
                ICMP.assert_exact_icmp_reply(LOCAL_MAC, frame)

    def test_marker_oracle_requires_exactly_one_ordered_success(self) -> None:
        ICMP.assert_icmp_acceptance(self.valid_serial(), "QEMU_OUTCOME success\n")
        invalid = (
            "\n".join((*EXPECTED_MARKERS[:2], *EXPECTED_MARKERS[3:])),
            "\n".join((*EXPECTED_MARKERS[:2], EXPECTED_MARKERS[3], EXPECTED_MARKERS[2], *EXPECTED_MARKERS[4:])),
            self.valid_serial() + "\n" + EXPECTED_MARKERS[-1],
        )
        for serial in invalid:
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                ICMP.assert_icmp_acceptance(serial, "QEMU_OUTCOME success\n")

    def test_oracle_rejects_error_storage_and_non_success_outcome(self) -> None:
        for evidence in ("PYTHOS:CORE:ICMP:ERROR", "PYTHOS:PANIC", "TRANSPORT_ERROR", "PYTHOS:CORE:BLOCK_DEVICE_READY"):
            with self.subTest(evidence=evidence), self.assertRaises(AssertionError):
                ICMP.assert_icmp_acceptance(self.valid_serial() + "\n" + evidence, "QEMU_OUTCOME success\n")
        for returncode, output in ((22, "QEMU_OUTCOME timeout\n"), (1, "QEMU_OUTCOME success\n"), (0, "QEMU_OUTCOME success\nQEMU_OUTCOME success\n")):
            with self.subTest(returncode=returncode, output=output), self.assertRaises(AssertionError):
                ICMP.assert_runner_success(returncode, output)

    def test_loopback_peer_rejects_extra_transmitted_frame(self) -> None:
        peer = ICMP.IcmpPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(ICMP.encode_socket_frame(ARP_REQUEST))
                self.assertEqual(ICMP.read_socket_frame(connection), ARP_REPLY)
                connection.sendall(ICMP.encode_socket_frame(ICMP_REQUEST))
                self.assertEqual(ICMP.read_socket_frame(connection), ICMP_REPLY)
                connection.sendall(ICMP.encode_socket_frame(ICMP_REQUEST))
            peer.join(timeout=1.0)
            self.assertIsNotNone(peer.error)
            self.assertIn("additional TX bytes", str(peer.error))
        finally:
            peer.close()

    def test_runner_command_requires_snapshot_peer_com2_and_no_virtio_block_disk(self) -> None:
        command = ICMP.probe_runner_command(peer_port=4595, shell_port=4596)
        self.assertIn("--no-virtio-blk", command)
        self.assertIn("--virtio-net", command)
        self.assertEqual(command[command.index("--virtio-net-peer-port") + 1], "4595")
        self.assertEqual(command[command.index("--shell-port") + 1], "4596")
        self.assertEqual(command[command.index("--expect-outcome") + 1], "success")

    def test_reused_cleanup_reaps_a_runner_process(self) -> None:
        kwargs: dict[str, object] = {"cwd": ROOT}
        if sys.platform != "win32":
            kwargs["start_new_session"] = True
        runner = ICMP.spawn_runner_process([sys.executable, "-c", "import time; time.sleep(30)"], **kwargs)
        ICMP.cleanup_runner_process(runner)
        if runner.process.stdout is not None:
            runner.process.stdout.close()
        self.assertIsNotNone(runner.process.poll())


if __name__ == "__main__":
    unittest.main()
