from __future__ import annotations

import importlib.util
import socket
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


ARP = load_module("arp_acceptance", "scripts/test-arp.py")


EXPECTED_MARKERS = (
    "PYTHOS:CORE:ARP:BOOTSTRAPPED",
    "PYTHOS:CORE:ARP:DESCRIBE_OK",
    "PYTHOS:CORE:ARP:REQUEST_OK",
    "PYTHOS:CORE:ARP:REPLY_OK",
    "PYTHOS:CORE:ARP:TEARDOWN_REVOKED",
    "PYTHOS:CORE:ARP_READY",
)
DEVICE_MAC = bytes.fromhex("525400123456")
REQUEST = (
    bytes.fromhex("ffffffffffff5254001234560806")
    + bytes.fromhex("0001080006040001525400123456c0000202000000000000c0000201")
    + bytes(18)
)
REPLY = (
    bytes.fromhex("5254001234560200000000020806")
    + bytes.fromhex("0001080006040002020000000002c0000201525400123456c0000202")
    + bytes(18)
)


class ArpHostTest(unittest.TestCase):
    def valid_consumer_serial(self) -> str:
        return "\n".join(ARP.CONSUMER_MARKERS)

    def valid_kernel_serial(self) -> str:
        return "\n".join(ARP.KERNEL_MARKERS)

    def test_marker_contract_is_frozen(self) -> None:
        self.assertEqual(ARP.REQUIRED_MARKERS, EXPECTED_MARKERS)

    def test_marker_oracle_requires_one_ordered_success(self) -> None:
        valid = self.valid_consumer_serial() + "\n" + self.valid_kernel_serial()
        ARP.assert_arp_acceptance(valid, "QEMU_OUTCOME success\n")
        for serial in (
            "\n".join((*EXPECTED_MARKERS[:2], *EXPECTED_MARKERS[3:])),
            "\n".join((*EXPECTED_MARKERS[:2], EXPECTED_MARKERS[3], EXPECTED_MARKERS[2], *EXPECTED_MARKERS[4:])),
            valid + "\n" + EXPECTED_MARKERS[-1],
        ):
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                ARP.assert_arp_acceptance(serial, "QEMU_OUTCOME success\n")

    def test_marker_oracle_rejects_error_panic_transport_and_storage_evidence(self) -> None:
        valid = self.valid_consumer_serial() + "\n" + self.valid_kernel_serial()
        for evidence in (
            "PYTHOS:CORE:ARP:ERROR",
            "PYTHOS:PANIC",
            "TRANSPORT_ERROR",
            "PYTHOS:CORE:BLOCK_DEVICE_READY",
        ):
            with self.subTest(evidence=evidence), self.assertRaises(AssertionError):
                ARP.assert_arp_acceptance(valid + "\n" + evidence, "QEMU_OUTCOME success\n")

    def test_runner_oracle_rejects_non_success_output(self) -> None:
        for returncode, output in (
            (22, "QEMU_OUTCOME timeout\n"),
            (20, "QEMU_OUTCOME panic\n"),
            (1, "QEMU_OUTCOME success\n"),
            (0, "QEMU_OUTCOME success\nQEMU_OUTCOME success\n"),
        ):
            with self.subTest(returncode=returncode, output=output), self.assertRaises(AssertionError):
                ARP.assert_runner_success(returncode, output)

    def test_exact_request_and_reply_bytes_are_network_order_and_zero_padded(self) -> None:
        self.assertEqual(ARP.arp_frame(bytes.fromhex("ffffffffffff"), DEVICE_MAC, ARP.arp_payload(1, DEVICE_MAC, ARP.LOCAL_IPV4, bytes(6), ARP.PEER_IPV4)), REQUEST)
        self.assertEqual(ARP.peer_reply(DEVICE_MAC), REPLY)

    def test_request_validator_rejects_a_wrong_described_local_mac(self) -> None:
        wrong_source = bytes.fromhex("525400123457")
        frame = ARP.arp_frame(
            bytes.fromhex("ffffffffffff"),
            wrong_source,
            ARP.arp_payload(1, wrong_source, ARP.LOCAL_IPV4, bytes(6), ARP.PEER_IPV4),
        )
        with self.assertRaises(AssertionError):
            ARP.assert_exact_arp_request(DEVICE_MAC, frame)

    def test_loopback_peer_rejects_extra_transmitted_frame(self) -> None:
        peer = ARP.ArpPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(ARP.encode_socket_frame(REQUEST))
                self.assertEqual(ARP.read_socket_frame(connection), REPLY)
                connection.sendall(ARP.encode_socket_frame(REQUEST))
            peer.join(timeout=1.0)
            self.assertIsNotNone(peer.error)
            self.assertIn("additional TX bytes", str(peer.error))
        finally:
            peer.close()

    def test_runner_command_requires_loopback_peer_com2_and_no_virtio_block_disk(self) -> None:
        command = ARP.probe_runner_command(peer_port=4595, shell_port=4596)
        self.assertIn("--no-virtio-blk", command)
        self.assertIn("--virtio-net", command)
        self.assertEqual(command[command.index("--virtio-net-peer-port") + 1], "4595")
        self.assertEqual(command[command.index("--shell-port") + 1], "4596")
        self.assertEqual(command[command.index("--expect-outcome") + 1], "success")


if __name__ == "__main__":
    unittest.main()
