from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
CAPABILITY = ROOT / "core" / "src" / "network_hardware_capability_probe.rs"
BOOT = ROOT / "core" / "src" / "network_hardware_capability_probe_boot.rs"
SCREEN = ROOT / "core" / "src" / "network_hardware_capability_probe_screen.rs"
MAIN = ROOT / "core" / "src" / "main.rs"


class NetworkHardwareCapabilityProbeContractTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = CAPABILITY.read_text(encoding="utf-8")
        cls.boot = BOOT.read_text(encoding="utf-8")
        cls.screen = SCREEN.read_text(encoding="utf-8")
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

    def test_boot_serial_contract_covers_ready_absent_and_malformed_paths(self):
        prefix = "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:"
        markers = (
            "ENTER",
            "PCI_SCAN_READY",
            "NETWORK_CONTROLLER_FOUND",
            "NETWORK_VENDOR=",
            "NETWORK_DEVICE_ID=",
            "NETWORK_SUBSYSTEM_VENDOR=",
            "NETWORK_SUBSYSTEM_DEVICE=",
            "NETWORK_CLASS=",
            "NETWORK_SUBCLASS=",
            "NETWORK_PROG_IF=",
            "PCI_CONFIG_HEADER_READY",
            "PCI_CAPABILITIES_STATUS=",
            "PCI_CAPABILITY_LIST_PRESENT",
            "PCI_CAPABILITY_LIST_ABSENT",
            "PCI_CAPABILITY_ENTRY=ID=",
            "PCI_CAPABILITY_SUMMARY=PM=",
            "PCI_INTERRUPT_METADATA=LINE=",
            "PCI_CONFIG_READ_ONLY",
            "FRAMEBUFFER_CAPABILITY_READY",
            "FRAMEBUFFER_CAPABILITY_DIAGNOSTIC_READY",
            "FRAMEBUFFER_CAPABILITY_FAILED",
            "NETWORK_CONTROLLER_NOT_FOUND",
        )
        for marker in markers:
            with self.subTest(marker=marker):
                self.assertIn(prefix + marker, self.boot)

        for fragment in (";OFFSET=", ";NEXT=", ";KIND=", ";HEADER_LEN=", ";PIN="):
            with self.subTest(fragment=fragment):
                self.assertIn(fragment, self.boot)
        for malformed in (
            "PCI_CAPABILITY_MALFORMED=INVALID_POINTER",
            "PCI_CAPABILITY_MALFORMED=REPEATED_OFFSET",
            "PCI_CAPABILITY_MALFORMED=ENTRY_LIMIT",
            "PCI_CAPABILITY_MALFORMED=RECOGNIZED_HEADER_BOUNDS",
        ):
            with self.subTest(malformed=malformed):
                self.assertIn(prefix + malformed, self.boot)
        self.assertIn("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE_READY", self.boot)

        start = self.boot.index("fn emit_ready")
        end = self.boot.index("fn emit_entries")
        ready = self.boot[start:end]
        ready_order = (
            "PCI_CAPABILITIES_STATUS=",
            "PCI_CAPABILITY_LIST_PRESENT",
            "emit_entries(&snapshot)",
            "emit_summary(&snapshot)",
            "emit_interrupt_metadata(interrupt_line, interrupt_pin)",
            "network_hardware_capability_probe_screen::render_ready",
            "PCI_CONFIG_READ_ONLY",
            "FRAMEBUFFER_CAPABILITY_READY",
            "NETWORK_HARDWARE_CAPABILITY_PROBE_READY",
        )
        positions = [ready.index(marker) for marker in ready_order]
        self.assertEqual(positions, sorted(positions))

    def test_boot_uses_only_the_bounded_header_and_parser_read_boundary(self):
        for declaration in (
            "network_hardware_probe::run_probe()",
            "report.preferred_controller()",
            "controller, 0x06,",
            "controller, 0x07,",
            "read_controller_config_byte(controller, 0x34)",
            "read_controller_config_byte(controller, 0x3C)",
            "read_controller_config_byte(controller, 0x3D)",
            "parse_capability_list(status, capability_pointer",
            "network_hardware_probe::read_controller_config_byte(controller, offset)",
        ):
            with self.subTest(declaration=declaration):
                self.assertIn(declaration, self.boot)

        for forbidden in (
            "read_controller_bar",
            "read_controller_command",
            "write_controller",
            "write_config",
            "network_hardware_register",
            "mmio",
            "dma",
            "NetworkPort",
            "socket",
            "queue",
            "packet",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, self.boot)

    def test_fixed_framebuffer_contract_renders_ready_and_malformed_panels(self):
        for declaration in (
            "const MAX_LINES: usize = 12",
            "const MAX_BYTES: usize = 48",
            "framebuffer::render_hardware_probe_lines",
            "PythOS",
            "network pci caps",
            "config read only",
            "capability malformed",
            "list present",
            "list absent",
            "pm ",
            "pcie ",
            "msi ",
            "msix ",
        ):
            with self.subTest(declaration=declaration):
                self.assertIn(declaration, self.screen)

        for forbidden in (
            "Vec<",
            "alloc::",
            "unsafe",
            "write_config",
            "mmio",
            "dma",
            "NetworkPort",
            "socket",
            "queue",
            "packet",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, self.screen)

    def test_malformed_terminal_does_not_emit_the_final_ready_marker(self):
        start = self.boot.index("fn emit_malformed")
        end = self.boot.index("fn capability_error_marker")
        malformed = self.boot[start:end]
        self.assertIn("FRAMEBUFFER_CAPABILITY_DIAGNOSTIC_READY", malformed)
        self.assertIn("PCI_CONFIG_READ_ONLY", malformed)
        self.assertNotIn("NETWORK_HARDWARE_CAPABILITY_PROBE_READY", malformed)


if __name__ == "__main__":
    unittest.main()
