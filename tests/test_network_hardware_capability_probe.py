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
            "#[cfg(any(test, feature = \"network-hardware-capability-probe\"))]\n"
            "mod network_hardware_capability_probe;",
            self.main,
        )

    def test_main_declares_isolated_capability_profile_and_dispatch(self):
        self.assertIn('network-hardware-capability-probe = []', (ROOT / "core" / "Cargo.toml").read_text(encoding="utf-8"))
        for declaration in (
            '#[cfg(any(test, feature = "network-hardware-capability-probe"))]\n'
            'mod network_hardware_capability_probe;',
            '#[cfg(all(not(test), feature = "network-hardware-capability-probe"))]\n'
            'mod network_hardware_capability_probe_boot;',
            '#[cfg(all(not(test), feature = "network-hardware-capability-probe"))]\n'
            'mod network_hardware_capability_probe_screen;',
        ):
            with self.subTest(declaration=declaration):
                self.assertIn(declaration, self.main)
        self.assertIn(
            '#[cfg(all(not(test), feature = "network-hardware-capability-probe"))]',
            self.main,
        )
        self.assertIn(
            'network_hardware_capability_probe_boot::run(boot_info, &mut physical_memory);',
            self.main,
        )

    def test_config_access_is_internal_read_only_and_byte_extracts_aligned_dword(self):
        probe = (ROOT / "core" / "src" / "network_hardware_probe.rs").read_text(encoding="utf-8")
        for declaration in (
            'pub(crate) fn read_controller_config_dword(controller: NetworkController, offset: u8) -> u32',
            'pub(crate) fn read_controller_config_byte(controller: NetworkController, offset: u8) -> u8',
            'read_controller_config_dword(controller, offset & 0xFC)',
            '((offset & 0x03) * 8)',
        ):
            with self.subTest(declaration=declaration):
                self.assertIn(declaration, probe)
        self.assertNotIn(
            '#[cfg(all(not(test), feature = "network-hardware-capability-probe"))]\n'
            'fn write_config',
            probe,
        )

    def test_capability_profile_is_mutually_exclusive_with_phase15_profiles(self):
        for feature in (
            "normal-session",
            "verify",
            "hardware-probe",
            "usb-xhci-probe",
            "network-hardware-probe",
            "network-hardware-bar-probe",
            "network-hardware-register-probe",
            "network-hardware-register-enable-probe",
        ):
            self.assertRegex(
                self.main,
                rf'all\(\s*feature = "{feature}",\s*feature = "network-hardware-capability-probe"',
            )


if __name__ == "__main__":
    unittest.main()
