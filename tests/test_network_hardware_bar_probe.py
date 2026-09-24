from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
ADR = ROOT / "docs" / "decisions" / "0106-phase-15-network-hardware-bar-layout-probe.md"
PLAN = ROOT / "docs" / "superpowers" / "plans" / "2026-09-24-phase-15-network-hardware-bar-layout.md"


class NetworkHardwareBarProbeContractTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.adr_text = ADR.read_text(encoding="utf-8")
        cls.plan_text = PLAN.read_text(encoding="utf-8")
        cls.adr_flat = " ".join(cls.adr_text.split())
        cls.plan_flat = " ".join(cls.plan_text.split())

    def test_scope_freezes_bar_profile_and_boundaries(self):
        for document_name, document in (
            ("ADR", self.adr_flat),
            ("plan", self.plan_flat),
        ):
            with self.subTest(document=document_name):
                self.assertIn("network-hardware-bar-probe", document)
                self.assertIn("network-hardware-probe", document)
                for offset in ("0x10", "0x14", "0x18", "0x1c", "0x20", "0x24"):
                    with self.subTest(offset=offset):
                        self.assertIn(offset, document)
                self.assertIn("e1000", document)
                self.assertIn("e1000e", document)
                self.assertIn("Lenovo", document)

        self.assertIn("six standard type-0 BAR slots", self.adr_text)
        self.assertIn("`0x10` through `0x24`", self.adr_text)
        self.assertIn("six-slot", self.plan_text)

        boundary_contract = (
            "no PCI configuration writes",
            "BAR-size writes",
            "BAR mapping or dereference",
            "MMIO or device-register reads",
            "DMA",
            "interrupts",
            "reset",
            "firmware",
            "bus mastering",
            "queue setup",
            "frame movement",
            "controller operation",
        )
        for document_name, document in (
            ("ADR", self.adr_flat),
            ("plan", self.plan_flat),
        ):
            with self.subTest(document_boundary=document_name):
                for boundary in boundary_contract:
                    with self.subTest(boundary=boundary):
                        self.assertIn(boundary, document)

        self.assertIn("Physical observation gate", self.adr_text)
        self.assertIn("target-specific Lenovo evidence", self.plan_text)

    def test_adr_freezes_six_slot_decoder_rules(self):
        for rule in (
            "an all-zero slot is unimplemented",
            "an I/O BAR records",
            "a 32-bit memory BAR records",
            "a below-1-MiB memory BAR (memory type bits `01`)",
            "a 64-bit memory BAR consumes the next slot",
            "a reserved type or an invalid 64-bit BAR in slot 5 is reported as malformed",
            "raw low dword `0x0008_0002`",
            "decodes to base `0x0008_0000`",
            "low type bits are retained in the raw field",
            "removed only when computing the base",
        ):
            with self.subTest(rule=rule):
                self.assertIn(rule, self.adr_flat)

    def test_plan_freezes_marker_contract(self):
        markers = (
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:ENTER",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:PCI_SCAN_READY",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_CONTROLLER_FOUND",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SCAN_READY",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_RAW_LOW=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_RAW_HIGH=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_KIND=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_LAYOUT_READY",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:FRAMEBUFFER_BAR_LAYOUT_READY",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:PCI_CONFIG_READ_ONLY",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE_READY",
        )
        positions = []
        for marker in markers:
            with self.subTest(marker=marker):
                position = self.plan_text.find(marker)
                self.assertNotEqual(position, -1)
                self.assertEqual(self.plan_text.count(marker), 1)
                positions.append(position)
        self.assertEqual(positions, sorted(positions))
        self.assertIn(
            "For each slot `0` through `5`, use the corresponding `BAR_SLOT_{n}_RAW_LOW`,",
            self.plan_text,
        )

    def test_plan_freezes_future_implementation_paths_without_requiring_them(self):
        expected_paths = (
            "core/src/network_hardware_bar_probe.rs",
            "core/Cargo.toml",
            "core/src/main.rs",
            "core/src/network_hardware_probe.rs",
            "core/src/network_hardware_bar_probe_boot.rs",
            "core/src/network_hardware_bar_probe_screen.rs",
            "scripts/run-qemu.py",
            "scripts/test-network-hardware-bar-probe.py",
            "tests/test_network_hardware_bar_probe.py",
        )
        for path in expected_paths:
            with self.subTest(path=path):
                self.assertIn(path, self.plan_text)


if __name__ == "__main__":
    unittest.main()
