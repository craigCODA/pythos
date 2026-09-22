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
    inserted = scripts_dir not in sys.path
    if inserted:
        sys.path.insert(0, scripts_dir)
    try:
        spec.loader.exec_module(module)
    finally:
        if inserted:
            sys.path.remove(scripts_dir)
    return module


try:
    DNS = load_module("dns_acceptance", "scripts/test-dns.py")
    LOAD_ERROR: BaseException | None = None
except BaseException as error:
    DNS = None
    LOAD_ERROR = error


EXPECTED_MARKERS = (
    "PYTHOS:CORE:DNS:BOOTSTRAPPED",
    "PYTHOS:CORE:DNS:DESCRIBE_OK",
    "PYTHOS:CORE:DNS:ARP_SETUP_OK",
    "PYTHOS:CORE:DNS:QUERY_OK",
    "PYTHOS:CORE:DNS:RESPONSE_OK",
    "PYTHOS:CORE:DNS:TEARDOWN_REVOKED",
    "PYTHOS:CORE:DNS_READY",
)
QUERY_DNS = bytes.fromhex(
    "d14e0100000100000000000006707974686f73076578616d706c650000010001"
)
RESPONSE_DNS = bytes.fromhex(
    "d14e8180000100010000000006707974686f73076578616d706c650000010001"
    "c00c000100010000003c0004c000020e"
)


class DnsHostTest(unittest.TestCase):
    def setUp(self) -> None:
        if LOAD_ERROR is not None:
            self.fail(f"DNS acceptance harness could not be loaded: {LOAD_ERROR}")

    def valid_serial(self) -> str:
        return "\n".join(EXPECTED_MARKERS)

    def test_exact_profile_constants_and_payloads_are_frozen(self) -> None:
        self.assertEqual(DNS.REQUIRED_MARKERS, EXPECTED_MARKERS)
        self.assertEqual(DNS.QUERY_DNS, QUERY_DNS)
        self.assertEqual(DNS.RESPONSE_DNS, RESPONSE_DNS)
        self.assertEqual(DNS.LOCAL_PORT, 0x1605)
        self.assertEqual(DNS.PEER_PORT, 53)
        frames = DNS.expected_frames()
        self.assertEqual([len(frame) for frame in frames], [60, 60, 74, 90])
        DNS.assert_exact_exchange(frames)

    def test_wrong_frame_order_duplicate_extra_and_corruption_fail(self) -> None:
        frames = DNS.expected_frames()
        invalid = (
            frames[:1] + frames[2:],
            frames[:2] + [frames[3], frames[2]],
            frames + [frames[-1]],
        )
        for candidate in invalid:
            with self.subTest(candidate=[len(frame) for frame in candidate]), self.assertRaises(AssertionError):
                DNS.assert_exact_exchange(candidate)
        corrupt = bytearray(frames[2])
        corrupt[14 + 20 + 8] ^= 1
        with self.assertRaises(AssertionError):
            DNS.assert_exact_exchange(frames[:2] + [bytes(corrupt), frames[3]])

    def test_marker_outcome_and_storage_oracles_reject_regressions(self) -> None:
        DNS.assert_dns_acceptance(self.valid_serial(), "QEMU_OUTCOME success\n")
        invalid_serials = (
            self.valid_serial() + "\n" + EXPECTED_MARKERS[-1],
            "\n".join((*EXPECTED_MARKERS[:3], *EXPECTED_MARKERS[4:])),
            self.valid_serial() + "\nPYTHOS:CORE:BLOCK_DEVICE_READY",
        )
        for serial in invalid_serials:
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                DNS.assert_dns_acceptance(serial, "QEMU_OUTCOME success\n")
        for output in ("QEMU_OUTCOME success\nQEMU_OUTCOME success\n", "QEMU_OUTCOME timeout\n"):
            with self.subTest(output=output), self.assertRaises(AssertionError):
                DNS.assert_dns_acceptance(self.valid_serial(), output)

    def test_runner_command_is_network_only_and_dns_profiled(self) -> None:
        command = DNS.probe_runner_command(4595, 4596)
        self.assertIn("--no-virtio-blk", command)
        self.assertIn("--virtio-net", command)
        self.assertEqual(command[command.index("--virtio-net-peer-port") + 1], "4595")
        self.assertEqual(command[command.index("--shell-port") + 1], "4596")
        self.assertEqual(command[command.index("--success-marker") + 1], "PYTHOS:CORE:DNS_READY")

    def test_loopback_peer_rejects_an_additional_frame(self) -> None:
        peer = DNS.DnsPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                frames = DNS.expected_frames()
                connection.sendall(DNS.encode_socket_frame(frames[0]))
                self.assertEqual(DNS.read_socket_frame(connection), frames[1])
                connection.sendall(DNS.encode_socket_frame(frames[2]))
                self.assertEqual(DNS.read_socket_frame(connection), frames[3])
                connection.sendall(DNS.encode_socket_frame(frames[3]))
            peer.join(timeout=1.0)
            self.assertIsNotNone(peer.error)
            self.assertIn("additional", str(peer.error))
        finally:
            peer.close()

    def test_self_test_command_exercises_the_real_oracle(self) -> None:
        completed = subprocess.run(
            [sys.executable, "scripts/test-dns.py", "--self-test"],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
        self.assertIn("DNS_QEMU_ACCEPTANCE_OK", completed.stdout)


if __name__ == "__main__":
    unittest.main()
