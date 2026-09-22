from __future__ import annotations

import importlib.util
import socket
import struct
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
    SOCKET = load_module("socket_acceptance", "scripts/test-socket.py")
    LOAD_ERROR: BaseException | None = None
except BaseException as error:
    SOCKET = None
    LOAD_ERROR = error


LOCAL_MAC = bytes.fromhex("525400123456")
PEER_MAC = bytes.fromhex("020000000002")
EXPECTED_DENIED_MARKERS = (
    "PYTHOS:CORE:SOCKET:DENIED_BOOTSTRAPPED",
    "PYTHOS:CORE:SOCKET:OPEN_WITHOUT_CAP_DENIED",
    "PYTHOS:CORE:SOCKET:DENIED_TEARDOWN_COMPLETE",
    "PYTHOS:CORE:SOCKET_DENIED_READY",
)
EXPECTED_GRANTED_MARKERS = (
    "PYTHOS:CORE:SOCKET:BOOTSTRAPPED",
    "PYTHOS:CORE:SOCKET:OPEN_GRANTED",
    "PYTHOS:CORE:SOCKET:HANDSHAKE_OK",
    "PYTHOS:CORE:SOCKET:REQUEST_OK",
    "PYTHOS:CORE:SOCKET:RESPONSE_OK",
    "PYTHOS:CORE:SOCKET:CLOSE_OK",
    "PYTHOS:CORE:SOCKET:TEARDOWN_REVOKED",
    "PYTHOS:CORE:SOCKET_READY",
)


class SocketHostTest(unittest.TestCase):
    def setUp(self) -> None:
        if LOAD_ERROR is not None:
            self.fail(f"socket acceptance harness could not be loaded: {LOAD_ERROR}")

    def test_exact_markers_and_adr0101_frame_profile_are_frozen(self) -> None:
        self.assertEqual(SOCKET.DENIED_REQUIRED_MARKERS, EXPECTED_DENIED_MARKERS)
        self.assertEqual(SOCKET.GRANTED_REQUIRED_MARKERS, EXPECTED_GRANTED_MARKERS)
        frames = SOCKET.TCP.exchange_frames(LOCAL_MAC)
        self.assertEqual(len(frames), 12)
        self.assertEqual([len(frame) for frame in frames], [60] * 12)
        SOCKET.TCP.assert_exact_exchange(LOCAL_MAC, frames)
        self.assertEqual(SOCKET.TCP.PEER_MAC, PEER_MAC)

    def test_granted_and_denied_marker_outcome_storage_oracles_reject_regressions(self) -> None:
        SOCKET.configure_case("granted")
        granted = "\n".join(EXPECTED_GRANTED_MARKERS)
        SOCKET.assert_granted_acceptance(granted, "QEMU_OUTCOME success\n")
        for invalid in (
            "\n".join((*EXPECTED_GRANTED_MARKERS[:2], *EXPECTED_GRANTED_MARKERS[3:])),
            granted + "\n" + EXPECTED_GRANTED_MARKERS[-1],
            granted + "\nPYTHOS:CORE:BLOCK_DEVICE_READY",
        ):
            with self.subTest(invalid=invalid), self.assertRaises(AssertionError):
                SOCKET.assert_granted_acceptance(invalid, "QEMU_OUTCOME success\n")

        SOCKET.configure_case("denied")
        denied = "\n".join(EXPECTED_DENIED_MARKERS)
        SOCKET.assert_denied_acceptance(denied, "QEMU_OUTCOME success\n")
        with self.assertRaises(AssertionError):
            SOCKET.assert_denied_acceptance(denied, "QEMU_OUTCOME success\nQEMU_OUTCOME success\n")

    def test_runner_command_is_network_only_and_profile_independent(self) -> None:
        SOCKET.configure_case("granted")
        command = SOCKET.probe_runner_command(4595, 4596)
        self.assertIn("--no-virtio-blk", command)
        self.assertIn("--virtio-net", command)
        self.assertEqual(command[command.index("--virtio-net-peer-port") + 1], "4595")
        self.assertEqual(command[command.index("--shell-port") + 1], "4596")
        self.assertEqual(command[command.index("--expect-outcome") + 1], "success")

        SOCKET.configure_case("denied")
        denied_command = SOCKET.probe_runner_command(4595, 4596)
        self.assertIn("--virtio-net", denied_command)
        self.assertEqual(
            denied_command[denied_command.index("--success-marker") + 1],
            EXPECTED_DENIED_MARKERS[-1],
        )

    def test_denied_peer_accepts_connected_zero_frame_eof(self) -> None:
        peer = SOCKET.DeniedPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0):
                pass
            peer.join(timeout=2.0)
            SOCKET.assert_denied_peer(peer)
        finally:
            peer.close()

    def test_denied_peer_rejects_connection_reset(self) -> None:
        peer = SOCKET.DeniedPeer(timeout=1.0)
        peer.start()
        try:
            connection = socket.create_connection(("127.0.0.1", peer.port), timeout=1.0)
            SOCKET.set_abortive_close(connection)
            connection.close()
            peer.join(timeout=2.0)
            self.assertIsInstance(peer.error, ConnectionResetError)
            self.assertFalse(peer.completed)
            with self.assertRaises(AssertionError):
                SOCKET.assert_denied_peer(peer)
        finally:
            peer.close()

    def test_denied_peer_rejects_no_connection_timeout(self) -> None:
        peer = SOCKET.DeniedPeer(timeout=0.1)
        peer.start()
        try:
            peer.join(timeout=1.0)
            self.assertIsInstance(peer.error, TimeoutError)
            self.assertFalse(peer.connected)
            self.assertFalse(peer.completed)
            with self.assertRaises(AssertionError):
                SOCKET.assert_denied_peer(peer)
        finally:
            peer.close()

    def test_denied_peer_rejects_connected_quiet_timeout(self) -> None:
        peer = SOCKET.DeniedPeer(timeout=0.1)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0):
                peer.join(timeout=1.0)
            self.assertIsInstance(peer.error, TimeoutError)
            self.assertTrue(peer.connected)
            self.assertFalse(peer.completed)
            with self.assertRaises(AssertionError):
                SOCKET.assert_denied_peer(peer)
        finally:
            peer.close()

    def test_denied_peer_rejects_abortive_reset(self) -> None:
        peer = SOCKET.DeniedPeer(timeout=1.0)
        peer.start()
        try:
            connection = socket.create_connection(("127.0.0.1", peer.port), timeout=1.0)
            try:
                linger = struct.pack("hh", 1, 0) if sys.platform == "win32" else struct.pack("ii", 1, 0)
                connection.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, linger)
            finally:
                connection.close()
            peer.join(timeout=2.0)
            self.assertIsInstance(peer.error, ConnectionResetError)
            self.assertFalse(peer.completed)
            with self.assertRaises(AssertionError):
                SOCKET.assert_denied_peer(peer)
        finally:
            peer.close()

    def test_denied_peer_rejects_any_frame(self) -> None:
        peer = SOCKET.DeniedPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(b"unexpected-frame")
            peer.join(timeout=2.0)
            self.assertIsNotNone(peer.error)
            self.assertIn("Ethernet", str(peer.error))
        finally:
            peer.close()

    def test_self_test_command_exercises_the_real_oracle(self) -> None:
        completed = subprocess.run(
            [sys.executable, "scripts/test-socket.py", "--self-test"],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
        self.assertIn("SOCKET_QEMU_ACCEPTANCE_OK", completed.stdout)


if __name__ == "__main__":
    unittest.main()
