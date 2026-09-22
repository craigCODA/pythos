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
    TCP = load_module("tcp_acceptance", "scripts/test-tcp.py")
    LOAD_ERROR: BaseException | None = None
except BaseException as error:
    TCP = None
    LOAD_ERROR = error


EXPECTED_MARKERS = (
    "PYTHOS:CORE:TCP:BOOTSTRAPPED",
    "PYTHOS:CORE:TCP:DESCRIBE_OK",
    "PYTHOS:CORE:TCP:ARP_SETUP_OK",
    "PYTHOS:CORE:TCP:HANDSHAKE_OK",
    "PYTHOS:CORE:TCP:TX_OK",
    "PYTHOS:CORE:TCP:RX_OK",
    "PYTHOS:CORE:TCP:CLOSE_OK",
    "PYTHOS:CORE:TCP:TEARDOWN_REVOKED",
    "PYTHOS:CORE:TCP_READY",
)
LOCAL_MAC = bytes.fromhex("525400123456")
PEER_MAC = bytes.fromhex("020000000002")
LOCAL_IPV4 = bytes.fromhex("c0a80e02")
PEER_IPV4 = bytes.fromhex("c0a80e01")
MSS_OPTION = bytes.fromhex("02040400")
REQUEST_DATA = b"PYTCPQ"
REPLY_DATA = b"PYTCPR"
EXPECTED_TCP_CHECKSUMS = (0xAD76, 0x885F, 0x9E68, 0xA974, 0xA96D, 0x9E5C, 0x9E5B, 0x9E5B, 0x9E5A, 0x9E5A)
EXPECTED_IPV4_CHECKSUMS = (0xC877, 0xC876, 0xC879, 0xC872, 0xC871, 0xC876, 0xC875, 0xC874, 0xC873, 0xC872)
EXPECTED_LENGTHS = (44, 44, 40, 46, 46, 40, 40, 40, 40, 40)
EXPECTED_FRAME_PADDING = (2, 2, 6, 0, 0, 6, 6, 6, 6, 6)


class TcpHostTest(unittest.TestCase):
    def setUp(self) -> None:
        if LOAD_ERROR is not None:
            self.fail(f"TCP acceptance harness could not be loaded: {LOAD_ERROR}")

    def valid_serial(self) -> str:
        return "\n".join(EXPECTED_MARKERS)

    def test_exact_profile_constants_and_markers_are_frozen(self) -> None:
        self.assertEqual(TCP.PEER_MAC, PEER_MAC)
        self.assertEqual(TCP.DESCRIBED_DEVICE_MAC, LOCAL_MAC)
        self.assertEqual(TCP.LOCAL_IPV4, LOCAL_IPV4)
        self.assertEqual(TCP.PEER_IPV4, PEER_IPV4)
        self.assertEqual(TCP.IPV4_PROTOCOL, 6)
        self.assertEqual(TCP.LOCAL_PORT, 0x1505)
        self.assertEqual(TCP.PEER_PORT, 0x1506)
        self.assertEqual(TCP.LOCAL_ISS, 0x15050000)
        self.assertEqual(TCP.PEER_ISS, 0x25060000)
        self.assertEqual(TCP.WINDOW, 0x1000)
        self.assertEqual(TCP.MSS_OPTION, MSS_OPTION)
        self.assertEqual(TCP.REQUEST_DATA, REQUEST_DATA)
        self.assertEqual(TCP.REPLY_DATA, REPLY_DATA)
        self.assertEqual(TCP.REQUIRED_MARKERS, EXPECTED_MARKERS)
        self.assertEqual(TCP.ARP_FRAME_COUNT + TCP.TCP_FRAME_COUNT, 12)

    def test_exact_twelve_frames_and_all_checksums_options_lengths_and_padding(self) -> None:
        frames = TCP.exchange_frames(LOCAL_MAC)
        self.assertEqual(len(frames), 12)
        TCP.assert_exact_arp_request(LOCAL_MAC, frames[0])
        TCP.assert_exact_arp_reply(LOCAL_MAC, frames[1])
        self.assertEqual(frames[0], TCP.arp_request(LOCAL_MAC))
        self.assertEqual(frames[1], TCP.arp_reply(LOCAL_MAC))
        tcp_frames = frames[2:]
        self.assertEqual([len(frame) for frame in tcp_frames], [60] * 10)
        for index, frame in enumerate(tcp_frames):
            datagram = frame[14 : 14 + EXPECTED_LENGTHS[index]]
            parsed_ipv4 = TCP.parse_ipv4_datagram(datagram)
            parsed_tcp = TCP.parse_tcp_segment(
                parsed_ipv4.payload,
                parsed_ipv4.source,
                parsed_ipv4.destination,
                TCP.TCP_SEGMENTS[index],
            )
            self.assertEqual(parsed_ipv4.checksum, EXPECTED_IPV4_CHECKSUMS[index])
            self.assertEqual(parsed_tcp.checksum, EXPECTED_TCP_CHECKSUMS[index])
            self.assertEqual(parsed_ipv4.total_length, EXPECTED_LENGTHS[index])
            self.assertEqual(frame[14 + EXPECTED_LENGTHS[index] :], bytes(EXPECTED_FRAME_PADDING[index]))
            if index in (0, 1):
                self.assertEqual(parsed_tcp.options, MSS_OPTION)
            else:
                self.assertEqual(parsed_tcp.options, b"")

    def test_sequence_ack_flags_and_payload_profile_is_exact(self) -> None:
        frames = TCP.exchange_frames(LOCAL_MAC)[2:]
        observed = [TCP.parse_tcp_frame(frame, LOCAL_MAC, index) for index, frame in enumerate(frames)]
        self.assertEqual(
            [(item.sequence, item.acknowledgment, item.flags, item.data) for item in observed],
            [
                (0x15050000, 0, TCP.SYN, b""),
                (0x25060000, 0x15050001, TCP.SYN | TCP.ACK, b""),
                (0x15050001, 0x25060001, TCP.ACK, b""),
                (0x15050001, 0x25060001, TCP.ACK, REQUEST_DATA),
                (0x25060001, 0x15050007, TCP.ACK, REPLY_DATA),
                (0x15050007, 0x25060007, TCP.ACK, b""),
                (0x15050007, 0x25060007, TCP.FIN | TCP.ACK, b""),
                (0x25060007, 0x15050008, TCP.ACK, b""),
                (0x25060007, 0x15050008, TCP.FIN | TCP.ACK, b""),
                (0x15050008, 0x25060008, TCP.ACK, b""),
            ],
        )

    def test_checksum_zeroing_and_non_transmitted_arithmetic_padding_are_explicit(self) -> None:
        frames = TCP.exchange_frames(LOCAL_MAC)[2:]
        for index, frame in enumerate(frames):
            datagram = frame[14 : 14 + EXPECTED_LENGTHS[index]]
            tcp = bytearray(datagram[20:])
            tcp[16:18] = b"\x00\x00"
            self.assertEqual(TCP.tcp_checksum(datagram[12:16], datagram[16:20], bytes(tcp)), EXPECTED_TCP_CHECKSUMS[index])
            self.assertEqual(frame[-EXPECTED_FRAME_PADDING[index] :] if EXPECTED_FRAME_PADDING[index] else b"", bytes(EXPECTED_FRAME_PADDING[index]))

    def test_parser_rejects_malformed_ipv4_tcp_options_checksum_direction_and_padding(self) -> None:
        frame = TCP.exchange_frames(LOCAL_MAC)[2]
        for mutation in (
            lambda value: value.__setitem__(14, 0x55),
            lambda value: value.__setitem__(13, 0x06),
            lambda value: value.__setitem__(23, 17),
            lambda value: value.__setitem__(26, 0),
            lambda value: value.__setitem__(34 + 13, TCP.RST),
            lambda value: value.__setitem__(14 + 20 + 20, 0),
        ):
            candidate = bytearray(frame)
            mutation(candidate)
            with self.assertRaises(AssertionError):
                TCP.assert_exact_tcp_frame(LOCAL_MAC, bytes(candidate), 0)
        with self.assertRaises(AssertionError):
            TCP.assert_exact_tcp_frame(LOCAL_MAC, frame[:-1], 0)
        with self.assertRaises(AssertionError):
            TCP.assert_exact_tcp_frame(LOCAL_MAC, frame + b"\x00", 0)

        ordinary = bytearray(TCP.exchange_frames(LOCAL_MAC)[4])
        ordinary[14 + 20 + 12] = 0x60
        with self.assertRaises(AssertionError):
            TCP.assert_exact_tcp_frame(LOCAL_MAC, bytes(ordinary), 2)

    def test_tcp_parser_rejects_reserved_bits_and_rst_with_valid_checksums(self) -> None:
        frame = TCP.exchange_frames(LOCAL_MAC)[2]
        segment = bytearray(frame[34:58])
        for mutation in (lambda value: value.__setitem__(12, value[12] | 1), lambda value: value.__setitem__(13, TCP.RST)):
            candidate = bytearray(segment)
            mutation(candidate)
            candidate[16:18] = b"\x00\x00"
            candidate[16:18] = TCP.tcp_checksum(LOCAL_IPV4, PEER_IPV4, bytes(candidate)).to_bytes(2, "big")
            with self.subTest(segment=bytes(candidate).hex()), self.assertRaises(AssertionError):
                TCP.parse_tcp_segment(bytes(candidate), LOCAL_IPV4, PEER_IPV4)

    def test_wrong_direction_duplicate_reorder_and_extra_frames_fail(self) -> None:
        frames = TCP.exchange_frames(LOCAL_MAC)
        with self.assertRaises(AssertionError):
            TCP.assert_exact_exchange(LOCAL_MAC, frames[:1] + frames[2:])
        with self.assertRaises(AssertionError):
            TCP.assert_exact_exchange(LOCAL_MAC, frames[:4] + [frames[3]] + frames[4:])
        with self.assertRaises(AssertionError):
            TCP.assert_exact_exchange(LOCAL_MAC, frames[:3] + [frames[4], frames[3]] + frames[5:])

    def test_marker_storage_and_runner_outcome_oracles_are_exact(self) -> None:
        TCP.assert_tcp_acceptance(self.valid_serial(), "QEMU_OUTCOME success\n")
        invalid_serials = (
            "\n".join((*EXPECTED_MARKERS[:3], *EXPECTED_MARKERS[4:])),
            "\n".join((*EXPECTED_MARKERS[:3], EXPECTED_MARKERS[4], EXPECTED_MARKERS[3], *EXPECTED_MARKERS[5:])),
            self.valid_serial() + "\n" + EXPECTED_MARKERS[-1],
            self.valid_serial() + "\nPYTHOS:CORE:BLOCK_DEVICE_READY",
        )
        for serial in invalid_serials:
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                TCP.assert_tcp_acceptance(serial, "QEMU_OUTCOME success\n")
        for returncode, output in ((1, "QEMU_OUTCOME success\n"), (0, "QEMU_OUTCOME success\nQEMU_OUTCOME success\n"), (0, "QEMU_OUTCOME timeout\n")):
            with self.subTest(returncode=returncode, output=output), self.assertRaises(AssertionError):
                TCP.assert_runner_success(returncode, output)

    def test_close_marker_is_user_consumer_evidence_before_kernel_teardown(self) -> None:
        self.assertEqual(TCP.CONSUMER_MARKERS, EXPECTED_MARKERS[:7])
        self.assertEqual(TCP.KERNEL_MARKERS, EXPECTED_MARKERS[7:])

    def test_loopback_peer_rejects_extra_transmitted_frame(self) -> None:
        peer = TCP.TcpPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                expected = TCP.exchange_frames(LOCAL_MAC)
                connection.sendall(TCP.encode_socket_frame(expected[0]))
                self.assertEqual(TCP.read_socket_frame(connection), expected[1])
                connection.sendall(TCP.encode_socket_frame(expected[2]))
                self.assertEqual(TCP.read_socket_frame(connection), expected[3])
                connection.sendall(TCP.encode_socket_frame(expected[4]))
                connection.sendall(TCP.encode_socket_frame(expected[5]))
                self.assertEqual(TCP.read_socket_frame(connection), expected[6])
                connection.sendall(TCP.encode_socket_frame(expected[7]))
                connection.sendall(TCP.encode_socket_frame(expected[8]))
                self.assertEqual(TCP.read_socket_frame(connection), expected[9])
                self.assertEqual(TCP.read_socket_frame(connection), expected[10])
                connection.sendall(TCP.encode_socket_frame(expected[11]))
                connection.sendall(TCP.encode_socket_frame(expected[11]))
            peer.join(timeout=1.0)
            self.assertIsNotNone(peer.error)
            self.assertIn("additional", str(peer.error))
        finally:
            peer.close()

    def test_runner_command_requires_no_virtio_block_peer_com2_and_snapshot_esp(self) -> None:
        command = TCP.probe_runner_command(peer_port=4595, shell_port=4596)
        self.assertIn("--no-virtio-blk", command)
        self.assertIn("--virtio-net", command)
        self.assertEqual(command[command.index("--virtio-net-peer-port") + 1], "4595")
        self.assertEqual(command[command.index("--shell-port") + 1], "4596")
        self.assertEqual(command[command.index("--expect-outcome") + 1], "success")
        self.assertEqual(TCP.ESP_IMAGE.name, "tcp-probe-com1-esp.img")

    def test_cleanup_reaps_a_runner_process(self) -> None:
        kwargs: dict[str, object] = {"cwd": ROOT}
        if sys.platform != "win32":
            kwargs["start_new_session"] = True
        runner = TCP.spawn_runner_process([sys.executable, "-c", "import time; time.sleep(30)"], **kwargs)
        TCP.cleanup_runner_process(runner)
        if runner.process.stdout is not None:
            runner.process.stdout.close()
        self.assertIsNotNone(runner.process.poll())

    def test_self_test_command_exercises_the_real_oracle(self) -> None:
        completed = subprocess.run(
            [sys.executable, "scripts/test-tcp.py", "--self-test"],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
        self.assertIn("TCP_QEMU_ACCEPTANCE_OK", completed.stdout)


if __name__ == "__main__":
    unittest.main()
