from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class NetworkHardwareRegisterEnableProbeContractTest(unittest.TestCase):
    def test_feature_has_a_narrow_command_word_write_boundary(self):
        cargo = (ROOT / "core" / "Cargo.toml").read_text(encoding="utf-8")
        main = (ROOT / "core" / "src" / "main.rs").read_text(encoding="utf-8")
        probe = (ROOT / "core" / "src" / "network_hardware_probe.rs").read_text(encoding="utf-8")
        self.assertIn("network-hardware-register-enable-probe = []", cargo)
        self.assertIn("network-hardware-register-enable-probe", main)
        self.assertIn("write_controller_command_word", probe)

    def test_enable_boot_contract_requires_readback_and_restore(self):
        boot = (ROOT / "core" / "src" / "network_hardware_register_enable_probe_boot.rs").read_text(encoding="utf-8")
        for marker in (
            "PCI_COMMAND_MSE_WRITE",
            "PCI_COMMAND_STATUS_AFTER_ENABLE",
            "PCI_COMMAND_MSE_RESTORE",
            "PCI_COMMAND_STATUS_RESTORED",
            "PCI_CONFIG_WRITE_SCOPED",
        ):
            with self.subTest(marker=marker):
                self.assertIn(marker, boot)

    def test_enable_profile_does_not_authorize_operational_networking(self):
        boot = (ROOT / "core" / "src" / "network_hardware_register_enable_probe_boot.rs").read_text(encoding="utf-8")
        for forbidden in (
            "BUS_MASTER",
            "DMA",
            "interrupt",
            "queue",
            "packet",
            "NetworkPort",
            "write_volatile",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, boot)


if __name__ == "__main__":
    unittest.main()
