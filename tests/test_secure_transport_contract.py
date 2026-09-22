from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "docs" / "superpowers" / "specs" / "2026-09-22-phase-14-secure-transport-design.md"
ADR = ROOT / "docs" / "decisions" / "0104-phase-14-secure-transport-proof.md"
MANIFEST = ROOT / "user" / "probes" / "socket" / "Cargo.toml"
CORE_MANIFEST = ROOT / "core" / "Cargo.toml"


class SecureTransportContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.spec = SPEC.read_text(encoding="utf-8")
        self.adr = ADR.read_text(encoding="utf-8")
        self.manifest = MANIFEST.read_text(encoding="utf-8")
        self.core_manifest = CORE_MANIFEST.read_text(encoding="utf-8")

    def test_backend_and_target_policy_are_frozen(self) -> None:
        self.assertIn('embedded-tls` `0.19.0`', self.adr)
        self.assertIn('default features disabled', self.adr)
        self.assertIn('`rustpki`', self.adr)
        self.assertIn('`x86_64-unknown-none`', self.adr)
        self.assertIn('`webpki` feature is deliberately not selected', self.adr)
        self.assertIn('embedded-tls = { version = "=0.19.0", default-features = false, features = ["rustpki"], optional = true }', self.manifest)
        self.assertIn('sha2 = { version = "=0.10.9", default-features = false, features = ["force-soft"], optional = true }', self.manifest)
        self.assertIn('secure-transport = ["dep:embedded-io", "dep:embedded-tls", "dep:rand_core", "dep:sha2"]', self.manifest)
        self.assertIn('secure-transport-probe = ["verify", "socket-api-probe"]', self.core_manifest)
        self.assertIn('secure-transport-denied-probe = ["verify", "socket-api-denied-probe"]', self.core_manifest)

    def test_profile_excludes_production_and_later_scope(self) -> None:
        for text in (self.spec, self.adr):
            self.assertIn("TLS 1.3", text)
            self.assertIn("no 0-RTT", text)
            self.assertIn("resumption", text)
            self.assertIn("KeyUpdate", text)
            self.assertIn("Phase 15", text)
            self.assertIn("NetworkPort", text)
            self.assertIn("VirtioTransport", text)

    def test_architectural_names_and_no_handwritten_crypto_are_preserved(self) -> None:
        self.assertIn("transport adapter", self.spec)
        self.assertIn("Hand-written cryptography is prohibited", self.spec)
        self.assertIn("does not add a public TLS API", self.adr)
        self.assertNotIn("modifies the existing `NetworkPort` ABI", self.spec)

    def test_acceptance_cases_require_authentication_and_tamper_rejection(self) -> None:
        self.assertIn("authenticated handshake", self.spec)
        self.assertIn("tampered ciphertext/tag", self.spec)
        self.assertIn("application bytes", self.spec)
        self.assertIn("zero Ethernet frames", self.spec)
        self.assertIn("clean serial/ESP artifact teardown", self.adr)
        self.assertIn("05fbe163a52218a9f419c17b540e73b963f9a265a43b7bc05587934b07ea7a4e", self.adr)
        self.assertIn("CertVerifier::new(Certificate::X509(...))", self.adr)

    def test_tamper_case_selects_private_core_final_marker_profile(self) -> None:
        socket_probe = (ROOT / "core" / "src" / "socket_probe.rs").read_text(encoding="utf-8")
        harness = (ROOT / "scripts" / "test-secure-transport.py").read_text(encoding="utf-8")

        self.assertIn(
            'secure-transport-tamper-probe = ["secure-transport-probe"]',
            self.core_manifest,
        )
        self.assertIn('("tamper", "secure-transport-tamper-probe")', harness)
        self.assertIn('feature = "secure-transport-tamper-probe"', socket_probe)
        self.assertIn("SECURE_TAMPER_READY_MARKER", socket_probe)
        self.assertIn('not(feature = "secure-transport-tamper-probe")', socket_probe)


if __name__ == "__main__":
    unittest.main()
