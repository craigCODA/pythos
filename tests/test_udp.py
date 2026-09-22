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
    UDP = load_module("udp_acceptance", "scripts/test-udp.py")
    LOAD_ERROR: BaseException | None = None
except BaseException as error:
    UDP = None
    LOAD_ERROR = error


EXPECTED_MARKERS = (
    "PYTHOS:CORE:UDP:BOOTSTRAPPED",
    "PYTHOS:CORE:UDP:DESCRIBE_OK",
    "PYTHOS:CORE:UDP:ARP_SETUP_OK",
    "PYTHOS:CORE:UDP:TX_OK",
    "PYTHOS:CORE:UDP:RX_OK",
    "PYTHOS:CORE:UDP:TEARDOWN_REVOKED",
    "PYTHOS:CORE:UDP_READY",
)
LOCAL_MAC = bytes.fromhex("525400123456")
PEER_MAC = bytes.fromhex("020000000002")
LOCAL_IPV4 = bytes.fromhex("c0a80e02")
PEER_IPV4 = bytes.fromhex("c0a80e01")
REQUEST_HEADER = bytes.fromhex("45000023140500004011c971c0a80e02c0a80e01")
REPLY_HEADER = bytes.fromhex("45000023140600004011c970c0a80e01c0a80e02")
REQUEST_DATAGRAM = bytes.fromhex("14051406000ff08a") + b"PYTHUDP"
REPLY_DATAGRAM = bytes.fromhex("14061405000ff08a") + b"PYTHUDP"
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
UDP_REQUEST = bytes.fromhex("0200000000025254001234560800") + REQUEST_HEADER + REQUEST_DATAGRAM + bytes(11)
UDP_REPLY = bytes.fromhex("5254001234560200000000020800") + REPLY_HEADER + REPLY_DATAGRAM + bytes(11)


def repair_ipv4(frame: bytes) -> bytes:
    changed = bytearray(frame)
    changed[24:26] = b"\x00\x00"
    changed[24:26] = UDP.ones_complement_checksum(changed[14:34]).to_bytes(2, "big")
    return bytes(changed)


def repair_udp(frame: bytes, source: bytes, destination: bytes) -> bytes:
    changed = bytearray(frame)
    changed[40:42] = b"\x00\x00"
    changed[40:42] = UDP.udp_checksum(source, destination, bytes(changed[34:49])).to_bytes(2, "big")
    return bytes(changed)


class UdpHostTest(unittest.TestCase):
    def setUp(self) -> None:
        if LOAD_ERROR is not None:
            self.fail(f"UDP acceptance harness could not be loaded: {LOAD_ERROR}")

    def valid_serial(self) -> str:
        return "\n".join(EXPECTED_MARKERS)

    def test_exact_constants_and_marker_contract_are_frozen(self) -> None:
        self.assertEqual(UDP.PEER_MAC, PEER_MAC)
        self.assertEqual(UDP.DESCRIBED_DEVICE_MAC, LOCAL_MAC)
        self.assertEqual(UDP.LOCAL_IPV4, LOCAL_IPV4)
        self.assertEqual(UDP.PEER_IPV4, PEER_IPV4)
        self.assertEqual(UDP.IPV4_ETHER_TYPE, 0x0800)
        self.assertEqual(UDP.IPV4_PROTOCOL, 17)
        self.assertEqual(UDP.IPV4_REQUEST_ID, 0x1405)
        self.assertEqual(UDP.IPV4_REPLY_ID, 0x1406)
        self.assertEqual(UDP.UDP_REQUEST_SOURCE_PORT, 0x1405)
        self.assertEqual(UDP.UDP_REQUEST_DESTINATION_PORT, 0x1406)
        self.assertEqual(UDP.UDP_DATA, b"PYTHUDP")
        self.assertEqual(UDP.MIN_ETHERNET_FRAME_BYTES, 60)
        self.assertEqual(UDP.REQUIRED_MARKERS, EXPECTED_MARKERS)

    def test_exact_headers_datagrams_and_checksums(self) -> None:
        self.assertEqual(UDP.ones_complement_checksum(REQUEST_HEADER[:10] + b"\x00\x00" + REQUEST_HEADER[12:]), 0xC971)
        self.assertEqual(UDP.ones_complement_checksum(REPLY_HEADER[:10] + b"\x00\x00" + REPLY_HEADER[12:]), 0xC970)
        self.assertEqual(UDP.udp_checksum(LOCAL_IPV4, PEER_IPV4, REQUEST_DATAGRAM[:6] + b"\x00\x00" + REQUEST_DATAGRAM[8:]), 0xF08A)
        self.assertEqual(UDP.udp_checksum(PEER_IPV4, LOCAL_IPV4, REPLY_DATAGRAM[:6] + b"\x00\x00" + REPLY_DATAGRAM[8:]), 0xF08A)
        self.assertEqual(UDP.ones_complement_checksum(REQUEST_HEADER), 0)
        self.assertEqual(UDP.ones_complement_checksum(REPLY_HEADER), 0)
        self.assertEqual(UDP.udp_datagram(0x1405, 0x1406, LOCAL_IPV4, PEER_IPV4), REQUEST_DATAGRAM)
        self.assertEqual(UDP.udp_datagram(0x1406, 0x1405, PEER_IPV4, LOCAL_IPV4), REPLY_DATAGRAM)

    def test_exact_four_frames_are_sixty_bytes_and_zero_padded(self) -> None:
        self.assertEqual(UDP.arp_request(LOCAL_MAC), ARP_REQUEST)
        self.assertEqual(UDP.arp_reply(LOCAL_MAC), ARP_REPLY)
        self.assertEqual(UDP.udp_request(LOCAL_MAC), UDP_REQUEST)
        self.assertEqual(UDP.udp_reply(LOCAL_MAC), UDP_REPLY)
        for frame in (ARP_REQUEST, ARP_REPLY, UDP_REQUEST, UDP_REPLY):
            self.assertEqual(len(frame), 60)
        self.assertEqual(UDP_REQUEST[49:], bytes(11))
        self.assertEqual(UDP_REPLY[49:], bytes(11))

    def test_parsers_reject_malformed_ipv4_and_udp_fields(self) -> None:
        valid = bytearray(REQUEST_HEADER + REQUEST_DATAGRAM)
        cases = [bytes(valid[:34])]
        for first in (0x55, 0x44, 0x46):
            datagram = bytearray(valid)
            datagram[0] = first
            datagram[10:12] = b"\x00\x00"
            datagram[10:12] = UDP.ones_complement_checksum(datagram[:20]).to_bytes(2, "big")
            cases.append(bytes(datagram))
        for total_length in (34, 36):
            datagram = bytearray(valid)
            datagram[2:4] = total_length.to_bytes(2, "big")
            datagram[10:12] = b"\x00\x00"
            datagram[10:12] = UDP.ones_complement_checksum(datagram[:20]).to_bytes(2, "big")
            cases.append(bytes(datagram))
        for offset, value in ((8, 1), (9, 1)):
            datagram = bytearray(valid)
            datagram[offset] ^= value
            datagram[10:12] = b"\x00\x00"
            datagram[10:12] = UDP.ones_complement_checksum(datagram[:20]).to_bytes(2, "big")
            cases.append(bytes(datagram))
        bad_checksum = bytearray(valid)
        bad_checksum[10] ^= 1
        cases.append(bytes(bad_checksum))
        for datagram in cases:
            with self.subTest(datagram=datagram.hex()), self.assertRaises(AssertionError):
                UDP.parse_ipv4_datagram(datagram)

    def test_udp_parser_rejects_ports_length_data_and_pseudo_header_mismatches(self) -> None:
        UDP.parse_udp_datagram(REQUEST_DATAGRAM, LOCAL_IPV4, PEER_IPV4, 0x1405, 0x1406)
        for name, offset, value in (
            ("source port", 1, 1),
            ("destination port", 3, 1),
            ("length", 5, 1),
            ("data", 14, 1),
        ):
            datagram = bytearray(REQUEST_DATAGRAM)
            datagram[offset] ^= value
            datagram[6:8] = b"\x00\x00"
            datagram[6:8] = UDP.udp_checksum(LOCAL_IPV4, PEER_IPV4, bytes(datagram)).to_bytes(2, "big")
            with self.subTest(name=name), self.assertRaises(AssertionError):
                UDP.parse_udp_datagram(bytes(datagram), LOCAL_IPV4, PEER_IPV4, 0x1405, 0x1406)
        for source, destination in ((b"\xc0\xa8\x0e\x03", PEER_IPV4), (LOCAL_IPV4, b"\xc0\xa8\x0e\x03")):
            with self.subTest(source=source.hex(), destination=destination.hex()), self.assertRaises(AssertionError):
                UDP.parse_udp_datagram(REQUEST_DATAGRAM, source, destination, 0x1405, 0x1406)
        bad_checksum = bytearray(REQUEST_DATAGRAM)
        bad_checksum[6] ^= 1
        with self.assertRaises(AssertionError):
            UDP.parse_udp_datagram(bytes(bad_checksum), LOCAL_IPV4, PEER_IPV4, 0x1405, 0x1406)

    def test_exact_validators_reject_malformed_ethernet_addresses_and_padding(self) -> None:
        UDP.assert_exact_arp_request(LOCAL_MAC, ARP_REQUEST)
        UDP.assert_exact_arp_reply(LOCAL_MAC, ARP_REPLY)
        UDP.assert_exact_udp_request(LOCAL_MAC, UDP_REQUEST)
        UDP.assert_exact_udp_reply(LOCAL_MAC, UDP_REPLY)
        mutations = {
            "destination MAC": (0, False, False),
            "source MAC": (11, False, False),
            "EtherType": (13, False, False),
            "source address": (29, True, True),
            "destination address": (33, True, True),
            "padding": (49, False, False),
        }
        for name, (offset, fix_ip, fix_udp) in mutations.items():
            frame = bytearray(UDP_REPLY)
            frame[offset] ^= 1
            candidate = bytes(frame)
            if fix_ip:
                candidate = repair_ipv4(candidate)
            if fix_udp:
                candidate = repair_udp(candidate, PEER_IPV4, LOCAL_IPV4)
            with self.subTest(name=name), self.assertRaises(AssertionError):
                UDP.assert_exact_udp_reply(LOCAL_MAC, candidate)
        for frame in (UDP_REPLY[:-1], UDP_REPLY + b"\x00"):
            with self.assertRaises(AssertionError):
                UDP.assert_exact_udp_reply(LOCAL_MAC, frame)

    def test_marker_oracle_requires_exactly_one_ordered_success(self) -> None:
        UDP.assert_udp_acceptance(self.valid_serial(), "QEMU_OUTCOME success\n")
        invalid = (
            "\n".join((*EXPECTED_MARKERS[:2], *EXPECTED_MARKERS[3:])),
            "\n".join((*EXPECTED_MARKERS[:2], EXPECTED_MARKERS[3], EXPECTED_MARKERS[2], *EXPECTED_MARKERS[4:])),
            self.valid_serial() + "\n" + EXPECTED_MARKERS[-1],
        )
        for serial in invalid:
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                UDP.assert_udp_acceptance(serial, "QEMU_OUTCOME success\n")

    def test_oracle_rejects_error_storage_and_non_success_outcome(self) -> None:
        for evidence in ("PYTHOS:CORE:UDP:ERROR", "PYTHOS:PANIC", "TRANSPORT_ERROR", "PYTHOS:CORE:BLOCK_DEVICE_READY"):
            with self.subTest(evidence=evidence), self.assertRaises(AssertionError):
                UDP.assert_udp_acceptance(self.valid_serial() + "\n" + evidence, "QEMU_OUTCOME success\n")
        for returncode, output in ((22, "QEMU_OUTCOME timeout\n"), (1, "QEMU_OUTCOME success\n"), (0, "QEMU_OUTCOME success\nQEMU_OUTCOME success\n")):
            with self.subTest(returncode=returncode, output=output), self.assertRaises(AssertionError):
                UDP.assert_runner_success(returncode, output)

    def test_loopback_peer_rejects_extra_transmitted_frame(self) -> None:
        peer = UDP.UdpPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(UDP.encode_socket_frame(ARP_REQUEST))
                self.assertEqual(UDP.read_socket_frame(connection), ARP_REPLY)
                connection.sendall(UDP.encode_socket_frame(UDP_REQUEST))
                self.assertEqual(UDP.read_socket_frame(connection), UDP_REPLY)
                connection.sendall(UDP.encode_socket_frame(UDP_REQUEST))
            peer.join(timeout=1.0)
            self.assertIsNotNone(peer.error)
            self.assertIn("additional TX bytes", str(peer.error))
        finally:
            peer.close()

    def test_runner_command_requires_snapshot_peer_com2_and_no_virtio_block_disk(self) -> None:
        command = UDP.probe_runner_command(peer_port=4595, shell_port=4596)
        self.assertIn("--no-virtio-blk", command)
        self.assertIn("--virtio-net", command)
        self.assertEqual(command[command.index("--virtio-net-peer-port") + 1], "4595")
        self.assertEqual(command[command.index("--shell-port") + 1], "4596")
        self.assertEqual(command[command.index("--expect-outcome") + 1], "success")
        self.assertEqual(UDP.ESP_IMAGE.name, "udp-probe-com1-esp.img")

    def test_reused_cleanup_reaps_a_runner_process(self) -> None:
        kwargs: dict[str, object] = {"cwd": ROOT}
        if sys.platform != "win32":
            kwargs["start_new_session"] = True
        runner = UDP.spawn_runner_process([sys.executable, "-c", "import time; time.sleep(30)"], **kwargs)
        UDP.cleanup_runner_process(runner)
        if runner.process.stdout is not None:
            runner.process.stdout.close()
        self.assertIsNotNone(runner.process.poll())

    def test_self_test_command_exercises_the_real_oracle(self) -> None:
        completed = subprocess.run(
            [sys.executable, "scripts/test-udp.py", "--self-test"],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
        self.assertIn("UDP_QEMU_ACCEPTANCE_OK", completed.stdout)


if __name__ == "__main__":
    unittest.main()
