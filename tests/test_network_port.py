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


NETWORK_PORT = load_module("network_port_acceptance", "scripts/test-network-port.py")


EXPECTED_MARKERS = (
    "PYTHOS:CORE:NETWORK_PORT:BOOTSTRAPPED",
    "PYTHOS:CORE:NETWORK_PORT:DESCRIBE_OK",
    "PYTHOS:CORE:NETWORK_PORT:TX_OK",
    "PYTHOS:CORE:NETWORK_PORT:RX_OK",
    "PYTHOS:CORE:NETWORK_PORT:FORGED_DENIED",
    "PYTHOS:CORE:NETWORK_PORT:WRONG_HOLDER_DENIED",
    "PYTHOS:CORE:NETWORK_PORT:BAD_BUFFER_DENIED",
    "PYTHOS:CORE:NETWORK_PORT:TEARDOWN_REVOKED",
    "PYTHOS:CORE:NETWORK_PORT_READY",
)


class NetworkPortHostTest(unittest.TestCase):
    def test_shared_abi_constants_and_marker_contract_are_frozen(self) -> None:
        abi = (ROOT / "shared" / "src" / "network_port_abi.rs").read_text(
            encoding="utf-8"
        )
        for declaration in (
            "pub const NETWORK_PORT_ABI_MAJOR: u16 = 1;",
            "pub const NETWORK_PORT_ABI_MINOR: u16 = 0;",
            "pub const NETWORK_PORT_MIN_FRAME_BYTES: usize = 60;",
            "pub const NETWORK_PORT_MAX_FRAME_BYTES: usize = 1514;",
        ):
            self.assertIn(declaration, abi)
        self.assertEqual(NETWORK_PORT.REQUIRED_MARKERS, EXPECTED_MARKERS)

    def test_marker_oracle_requires_exact_order_denials_and_clean_terminal_state(self) -> None:
        serial = "\n".join(EXPECTED_MARKERS)
        NETWORK_PORT.assert_network_port_acceptance(serial, "QEMU_OUTCOME success\n")

        for malformed in (
            "\n".join(marker for marker in EXPECTED_MARKERS if "DENIED" not in marker),
            "\n".join((*EXPECTED_MARKERS[:4], EXPECTED_MARKERS[5], EXPECTED_MARKERS[4], *EXPECTED_MARKERS[6:])),
            serial + "\nPYTHOS:CORE:NETWORK_PORT_READY",
            serial + "\nPYTHOS:PANIC",
            serial + "\nPYTHOS:CORE:NETWORK_PORT:ERROR:RUN",
            serial + "\nTIMEOUT",
        ):
            with self.subTest(serial=malformed):
                with self.assertRaises(AssertionError):
                    NETWORK_PORT.assert_network_port_acceptance(
                        malformed, "QEMU_OUTCOME success\n"
                    )

    def test_runner_outcome_rejects_timeout_nonzero_and_reset(self) -> None:
        for returncode, output in (
            (22, "QEMU_OUTCOME timeout\n"),
            (20, "QEMU_OUTCOME panic\n"),
            (21, "QEMU_OUTCOME reset\n"),
            (1, "QEMU_OUTCOME success\n"),
        ):
            with self.subTest(returncode=returncode, output=output):
                with self.assertRaises(AssertionError):
                    NETWORK_PORT.assert_runner_success(returncode, output)

    def test_loopback_peer_validates_the_bounded_network_port_frames(self) -> None:
        device_mac = bytes.fromhex("525400123456")
        expected_tx = (
            bytes.fromhex("020000000002")
            + device_mac
            + bytes.fromhex("88b5")
            + b"PYTHOS:NIC:TX"
            + bytes(60 - 14 - len(b"PYTHOS:NIC:TX"))
        )
        expected_rx = (
            device_mac
            + bytes.fromhex("02000000000288b5")
            + b"PYTHOS:NIC:RX"
            + bytes(60 - 14 - len(b"PYTHOS:NIC:RX"))
        )
        peer = NETWORK_PORT.FramePeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1) as connection:
                connection.sendall(NETWORK_PORT.encode_socket_frame(expected_tx))
                self.assertEqual(NETWORK_PORT.read_socket_frame(connection), expected_rx)
            peer.join(timeout=1)
            peer.expected_device_mac = device_mac
            NETWORK_PORT.assert_peer_exchange(peer)
        finally:
            peer.close()

    def test_runner_command_isolated_to_legacy_peer_without_data_disk(self) -> None:
        self.assertEqual(
            NETWORK_PORT.probe_runner_command(peer_port=4595, shell_port=4596),
            [
                sys.executable,
                "scripts/run-qemu.py",
                "--serial-log",
                str(NETWORK_PORT.SERIAL_LOG),
                "--success-marker",
                "PYTHOS:CORE:NETWORK_PORT_READY",
                "--timeout",
                "30",
                "--no-audio-device",
                "--no-virtio-blk",
                "--virtio-net",
                "--virtio-net-peer-port",
                "4595",
                "--shell-port",
                "4596",
                "--expect-outcome",
                "success",
            ],
        )


if __name__ == "__main__":
    unittest.main()
