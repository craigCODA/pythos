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
    SECURE = load_module("secure_transport_acceptance", "scripts/test-secure-transport.py")
    LOAD_ERROR: BaseException | None = None
except BaseException as error:
    SECURE = None
    LOAD_ERROR = error


EXPECTED_GRANTED_MARKERS = (
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
EXPECTED_TAMPER_MARKERS = (
    "PYTHOS:CORE:SECURE:BOOTSTRAPPED",
    "PYTHOS:CORE:SECURE:OPEN_GRANTED",
    "PYTHOS:CORE:SECURE:TCP_READY",
    "PYTHOS:CORE:SECURE:TLS_HANDSHAKE_OK",
    "PYTHOS:CORE:SECURE:TAMPER_REJECTED",
    "PYTHOS:CORE:SECURE:TEARDOWN_REVOKED",
    "PYTHOS:CORE:SECURE_TAMPER_READY",
)
EXPECTED_DENIED_MARKERS = (
    "PYTHOS:CORE:SECURE:DENIED_BOOTSTRAPPED",
    "PYTHOS:CORE:SECURE:OPEN_WITHOUT_CAP_DENIED",
    "PYTHOS:CORE:SECURE:DENIED_TEARDOWN_COMPLETE",
    "PYTHOS:CORE:SECURE_DENIED_READY",
)


class SecureTransportHostTest(unittest.TestCase):
    def setUp(self) -> None:
        if LOAD_ERROR is not None:
            self.fail(f"secure acceptance harness could not be loaded: {LOAD_ERROR}")

    def test_marker_contract_and_case_oracles_are_exact(self) -> None:
        self.assertEqual(SECURE.GRANTED_REQUIRED_MARKERS, EXPECTED_GRANTED_MARKERS)
        self.assertEqual(SECURE.TAMPER_REQUIRED_MARKERS, EXPECTED_TAMPER_MARKERS)
        self.assertEqual(SECURE.DENIED_REQUIRED_MARKERS, EXPECTED_DENIED_MARKERS)
        for case, markers, checker in (
            ("granted", EXPECTED_GRANTED_MARKERS, SECURE.assert_granted_acceptance),
            ("tamper", EXPECTED_TAMPER_MARKERS, SECURE.assert_tamper_acceptance),
            ("denied", EXPECTED_DENIED_MARKERS, SECURE.assert_denied_acceptance),
        ):
            with self.subTest(case=case):
                checker("\n".join(markers), "QEMU_OUTCOME success\n")

    def test_marker_and_outcome_mutations_are_rejected(self) -> None:
        valid = "\n".join(EXPECTED_GRANTED_MARKERS)
        for invalid in (
            "\n".join((*EXPECTED_GRANTED_MARKERS[:3], *EXPECTED_GRANTED_MARKERS[4:])),
            "\n".join((*EXPECTED_GRANTED_MARKERS[:3], EXPECTED_GRANTED_MARKERS[4], EXPECTED_GRANTED_MARKERS[3], *EXPECTED_GRANTED_MARKERS[5:])),
            valid + "\n" + EXPECTED_GRANTED_MARKERS[-1],
            valid + "\nPYTHOS:CORE:BLOCK_DEVICE_READY",
        ):
            with self.subTest(invalid=invalid), self.assertRaises(AssertionError):
                SECURE.assert_granted_acceptance(invalid, "QEMU_OUTCOME success\n")
        for returncode, output in (
            (1, "QEMU_OUTCOME success\n"),
            (0, "QEMU_OUTCOME success\nQEMU_OUTCOME success\n"),
            (0, "QEMU_OUTCOME timeout\n"),
        ):
            with self.subTest(returncode=returncode, output=output), self.assertRaises(AssertionError):
                SECURE.assert_runner_success(returncode, output)

    def test_tls_fixture_is_pinned_and_removed_after_context(self) -> None:
        certificate = SECURE.base64_fixture(SECURE.CERTIFICATE_PEM_B64)
        self.assertEqual(SECURE.certificate_fingerprint(certificate), SECURE.CERTIFICATE_SHA256)
        with SECURE.temporary_tls_fixture() as fixture:
            cert_path, key_path = fixture
            self.assertTrue(cert_path.is_file())
            self.assertTrue(key_path.is_file())
            self.assertEqual(cert_path.read_bytes(), certificate)
        self.assertFalse(cert_path.exists())
        self.assertFalse(key_path.exists())

    def test_tls_record_parser_and_tamper_oracle(self) -> None:
        record = bytes.fromhex("1703030004") + b"test"
        wire = record + bytes.fromhex("1503030002") + b"ok"
        self.assertEqual(SECURE.tls_records(wire), [record, wire[len(record) :]])
        tampered = SECURE.tamper_first_application_record(wire)
        self.assertNotEqual(tampered, wire)
        self.assertEqual(SECURE.tls_records(tampered)[0][:5], record[:5])
        with self.assertRaises(AssertionError):
            SECURE.tamper_first_application_record(bytes.fromhex("1603030001") + b"x")

    def test_secure_tcp_frame_builder_and_parser_reject_mutations(self) -> None:
        frame = SECURE.build_tcp_frame(
            source=SECURE.PEER_IPV4,
            destination=SECURE.LOCAL_IPV4,
            source_mac=SECURE.PEER_MAC,
            destination_mac=SECURE.DESCRIBED_DEVICE_MAC,
            source_port=SECURE.PEER_PORT,
            destination_port=SECURE.LOCAL_PORT,
            sequence=SECURE.PEER_ISS,
            acknowledgment=SECURE.LOCAL_ISS + 1,
            flags=SECURE.ACK,
            identification=0x1701,
            data=b"fixture",
        )
        parsed = SECURE.parse_secure_frame(frame, expect_source=SECURE.PEER_IPV4, expect_destination=SECURE.LOCAL_IPV4)
        self.assertEqual(parsed.data, b"fixture")
        bad = bytearray(frame)
        bad[34 + 16] ^= 1
        with self.assertRaises(AssertionError):
            SECURE.parse_secure_frame(bytes(bad), expect_source=SECURE.PEER_IPV4, expect_destination=SECURE.LOCAL_IPV4)

    def test_denied_peer_accepts_only_connected_zero_frame_eof(self) -> None:
        peer = SECURE.DeniedPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0):
                pass
            peer.join(timeout=2.0)
            SECURE.assert_denied_peer(peer)
        finally:
            peer.close()

    def test_build_script_declares_secure_feature_and_verification(self) -> None:
        source = (ROOT / "scripts" / "build-secure-transport-probe.py").read_text(encoding="utf-8")
        self.assertIn('"--features"', source)
        self.assertIn('"secure-transport"', source)
        self.assertIn('"--release"', source)
        self.assertIn("verify-user-elf.py", source)
        self.assertIn('"--cfg"', source)
        self.assertIn('"aes_force_soft"', source)
        self.assertIn('"polyval_force_soft"', source)
        self.assertIn('"release" / "socket-probe"', source)
        self.assertNotIn('"debug" / "socket-probe"', source)
        self.assertIn("secure-transport-probe.elf", source)

    def test_self_test_command_exercises_the_real_harness(self) -> None:
        completed = subprocess.run(
            [sys.executable, "scripts/test-secure-transport.py", "--self-test"],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
        self.assertIn("SECURE_TRANSPORT_QEMU_ACCEPTANCE_OK", completed.stdout)


if __name__ == "__main__":
    unittest.main()
