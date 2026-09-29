from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
CAPABILITY = ROOT / "core" / "src" / "network_hardware_capability_probe.rs"
MAIN = ROOT / "core" / "src" / "main.rs"


class NetworkHardwareCapabilityProbeContractTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = CAPABILITY.read_text(encoding="utf-8")
        cls.main = MAIN.read_text(encoding="utf-8")

    def test_module_declares_fixed_size_public_parser_contract(self):
        for declaration in (
            "pub const MAX_CAPABILITY_ENTRIES: usize = 48",
            "pub enum CapabilityKind",
            "PowerManagement",
            "Pcie",
            "Msi",
            "Msix",
            "Unknown",
            "pub struct PciCapabilityEntry",
            "pub struct CapabilitySnapshot",
            "pub enum CapabilityParseError",
            "pub fn parse_capability_list",
            "[Option<PciCapabilityEntry>; MAX_CAPABILITY_ENTRIES]",
        ):
            with self.subTest(declaration=declaration):
                self.assertIn(declaration, self.source)

    def test_parser_freezes_bounded_read_only_rules(self):
        for rule in (
            "0x40",
            "0xFC",
            "4-byte",
            "RepeatedOffset",
            "EntryLimitExceeded",
            "RecognizedHeaderOutOfBounds",
            "0x01",
            "0x10",
            "0x05",
            "0x11",
            "0x08",
            "0x14",
            "0x0A",
            "0x0C",
            "[u8; MAX_CAPABILITY_ENTRIES]",
        ):
            with self.subTest(rule=rule):
                self.assertIn(rule, self.source)

        for forbidden in ("Vec<", "alloc::", "unsafe", "write_config", "inout!", "outb"):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, self.source)

    def test_main_declares_parser_for_unit_tests_only(self):
        self.assertIn(
            "#[cfg(test)]\n"
            "mod network_hardware_capability_probe;",
            self.main,
        )


if __name__ == "__main__":
    unittest.main()
