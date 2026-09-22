from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ADR = ROOT / "docs" / "decisions" / "0105-phase-15-network-hardware-identity-probe.md"
PLAN = ROOT / "docs" / "superpowers" / "plans" / "2026-09-22-phase-15-network-hardware-identity.md"


class NetworkHardwareProbeContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.adr = ADR.read_text(encoding="utf-8")
        self.plan = PLAN.read_text(encoding="utf-8")

    def test_qemu_identity_fixtures_are_frozen(self) -> None:
        for text in (self.adr, self.plan):
            self.assertIn("e1000", text)
            self.assertIn("e1000e", text)
            self.assertIn("0x8086", text)
            self.assertIn("0x100E", text)
            self.assertIn("0x10D3", text)
            self.assertIn("0x02/0x00/0x00", text)

    def test_dedicated_feature_and_boot_boundary_are_explicit(self) -> None:
        for text in (self.adr, self.plan):
            normalized = " ".join(text.split())
            self.assertIn("network-hardware-probe", normalized)
            self.assertIn("NETWORK_HARDWARE_PROBE:PCI_SCAN_READY", normalized)
            self.assertIn("NETWORK_HARDWARE_PROBE:NETWORK_IDENTITY_READY", normalized)
            self.assertIn("NETWORK_HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY", normalized)
            self.assertIn("NETWORK_HARDWARE_PROBE:PCI_CONFIG_READ_ONLY", normalized)
            self.assertIn("NETWORK_HARDWARE_PROBE_READY", normalized)

    def test_architectural_names_and_boundaries_are_explicit(self) -> None:
        for text in (self.adr, self.plan):
            normalized = " ".join(text.split())
            self.assertIn("VirtioTransport", text)
            self.assertIn("transport adapter", text)
            self.assertIn("NetworkPort", text)
            for exclusion in (
                "Wi-Fi frames",
                "Wi-Fi behavior",
                "network datapath",
                "MMIO control",
                "DMA",
                "bus mastering",
                "interrupts",
                "reset",
                "modern Virtio",
                "physical hardware support",
                "generalized hardware abstraction",
            ):
                self.assertIn(exclusion, normalized)

    def test_no_architectural_driver_noun_or_broader_probe_surface(self) -> None:
        for text in (self.adr, self.plan):
            self.assertIsNone(re.search(r"\bdriver\b", text, re.IGNORECASE))

    def test_plan_requires_dedicated_write_scope_and_no_storage_probe_change(self) -> None:
        for path in (
            "core/src/network_hardware_probe.rs",
            "core/src/network_hardware_probe_boot.rs",
            "core/src/network_hardware_probe_screen.rs",
            "core/src/main.rs",
            "core/Cargo.toml",
            "scripts/run-qemu.py",
            "scripts/test-network-hardware-probe.py",
        ):
            self.assertIn(path, self.plan)
        self.assertIn("--network-device {e1000,e1000e}", self.plan)
        self.assertIn("existing storage `hardware-probe` remains unchanged", self.plan)


if __name__ == "__main__":
    unittest.main()
