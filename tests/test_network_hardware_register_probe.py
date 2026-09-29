from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ORACLE = ROOT / "scripts" / "test-network-hardware-register-probe.py"


def load_oracle():
    spec = importlib.util.spec_from_file_location("network_hardware_register_oracle", ORACLE)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {ORACLE}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class NetworkHardwareRegisterProbeContractTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.oracle = load_oracle()
        cls.plan = (ROOT / "docs" / "superpowers" / "plans" / "2026-09-24-phase-15-network-hardware-register-reachability.md").read_text(encoding="utf-8")
        cls.adr = (ROOT / "docs" / "decisions" / "0107-phase-15-network-hardware-register-reachability.md").read_text(encoding="utf-8")
        cls.cargo = (ROOT / "core" / "Cargo.toml").read_text(encoding="utf-8")
        cls.main = (ROOT / "core" / "src" / "main.rs").read_text(encoding="utf-8")
        cls.boot = (ROOT / "core" / "src" / "network_hardware_register_probe_boot.rs").read_text(encoding="utf-8")
        cls.probe = (ROOT / "core" / "src" / "network_hardware_register_probe.rs").read_text(encoding="utf-8")

    def test_scope_names_gate_and_fixed_offsets_are_documented(self):
        for document in (self.plan, self.adr):
            for phrase in (
                "network-hardware-register-probe",
                "PCI Memory Space Enable",
                "0x08",
                "0x00F4",
                "no DMA",
                "no interrupts",
                "no packet",
            ):
                with self.subTest(phrase=phrase):
                    self.assertIn(phrase, document)

    def test_feature_and_boot_path_are_isolated(self):
        self.assertIn("network-hardware-register-probe = []", self.cargo)
        self.assertIn('feature = "network-hardware-register-probe"', self.main)
        self.assertIn("network_hardware_register_probe_boot::run", self.main)
        self.assertIn("network_hardware_register_mmio", self.boot)
        self.assertIn("read_volatile", self.probe)

    def test_read_path_has_no_write_or_operational_markers(self):
        for forbidden in (
            "write_config",
            "write_volatile",
            "bus_master",
            "DMA",
            "interrupt",
            "queue",
            "packet",
            "NetworkPort",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, self.boot)

    def test_synthetic_read_and_skip_reports_are_accepted(self):
        self.oracle.assert_register_probe_report(
            self.oracle.synthetic_serial_report("e1000"), "e1000"
        )
        self.oracle.assert_register_probe_report(
            self.oracle.synthetic_serial_report("e1000", 0), "e1000"
        )


if __name__ == "__main__":
    unittest.main()
