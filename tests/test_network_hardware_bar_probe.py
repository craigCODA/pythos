import importlib.util
from pathlib import Path
import re
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
ADR = ROOT / "docs" / "decisions" / "0106-phase-15-network-hardware-bar-layout-probe.md"
PLAN = ROOT / "docs" / "superpowers" / "plans" / "2026-09-24-phase-15-network-hardware-bar-layout.md"
CORE_CARGO = ROOT / "core" / "Cargo.toml"
CORE_MAIN = ROOT / "core" / "src" / "main.rs"
IDENTITY_BOOT = ROOT / "core" / "src" / "network_hardware_probe_boot.rs"
BAR_BOOT = ROOT / "core" / "src" / "network_hardware_bar_probe_boot.rs"
BAR_SCREEN = ROOT / "core" / "src" / "network_hardware_bar_probe_screen.rs"
BAR_ORACLE = ROOT / "scripts" / "test-network-hardware-bar-probe.py"


def load_bar_oracle():
    spec = importlib.util.spec_from_file_location("network_hardware_bar_oracle", BAR_ORACLE)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {BAR_ORACLE}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class NetworkHardwareBarProbeContractTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.adr_text = ADR.read_text(encoding="utf-8")
        cls.plan_text = PLAN.read_text(encoding="utf-8")
        cls.cargo_text = CORE_CARGO.read_text(encoding="utf-8")
        cls.main_text = CORE_MAIN.read_text(encoding="utf-8")
        cls.identity_boot_text = IDENTITY_BOOT.read_text(encoding="utf-8")
        cls.bar_boot_text = BAR_BOOT.read_text(encoding="utf-8")
        cls.bar_screen_text = BAR_SCREEN.read_text(encoding="utf-8")
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
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_VENDOR=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_DEVICE_ID=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_CLASS=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_SUBCLASS=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_PROG_IF=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SCAN_READY",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_RAW_LOW=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_RAW_HIGH=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_KIND=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_BASE=",
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

    def test_bar_boot_and_screen_preserve_read_only_complete_metadata(self):
        for marker in (
            "NETWORK_HARDWARE_BAR_PROBE:ENTER",
            "NETWORK_HARDWARE_BAR_PROBE:PCI_SCAN_READY",
            "NETWORK_HARDWARE_BAR_PROBE:NETWORK_CONTROLLER_FOUND",
            "NETWORK_HARDWARE_BAR_PROBE:NETWORK_VENDOR=",
            "NETWORK_HARDWARE_BAR_PROBE:NETWORK_DEVICE_ID=",
            "NETWORK_HARDWARE_BAR_PROBE:NETWORK_CLASS=",
            "NETWORK_HARDWARE_BAR_PROBE:NETWORK_SUBCLASS=",
            "NETWORK_HARDWARE_BAR_PROBE:NETWORK_PROG_IF=",
            "NETWORK_HARDWARE_BAR_PROBE:BAR_SCAN_READY",
            "NETWORK_HARDWARE_BAR_PROBE:BAR_LAYOUT_READY",
            "NETWORK_HARDWARE_BAR_PROBE:FRAMEBUFFER_BAR_LAYOUT_READY",
            "NETWORK_HARDWARE_BAR_PROBE:PCI_CONFIG_READ_ONLY",
            "NETWORK_HARDWARE_BAR_PROBE_READY",
        ):
            with self.subTest(marker=marker):
                self.assertIn(marker, self.bar_boot_text)
        self.assertIn("decode_bar_layout", self.bar_boot_text)
        self.assertIn("read_controller_bar_dwords", self.bar_boot_text)
        self.assertIn("BAR_SLOT_0_BASE=", self.bar_boot_text)
        self.assertIn("BAR_SLOT_5_BASE=", self.bar_boot_text)
        self.assertIn("const MAX_LINES: usize = 12", self.bar_screen_text)
        self.assertIn("const MAX_BYTES: usize = 48", self.bar_screen_text)
        self.assertIn("class sub if", self.bar_screen_text)
        for slot in range(6):
            for field in ("RAW_LOW", "RAW_HIGH", "KIND", "BASE"):
                with self.subTest(slot=slot, field=field):
                    self.assertIn(f"BAR_SLOT_{slot}_{field}", self.bar_boot_text)
        implementation_markers = (
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:ENTER",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:PCI_SCAN_READY",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_CONTROLLER_FOUND",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_VENDOR=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_DEVICE_ID=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_CLASS=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_SUBCLASS=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_PROG_IF=",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SCAN_READY",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_LAYOUT_READY",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:FRAMEBUFFER_BAR_LAYOUT_READY",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:PCI_CONFIG_READ_ONLY",
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE_READY",
        )
        positions = [self.bar_boot_text.find(marker) for marker in implementation_markers]
        self.assertTrue(all(position >= 0 for position in positions))
        self.assertEqual(positions, sorted(positions))
        self.assertRegex(
            self.bar_boot_text,
            r"else \{\s*"
            r'serial::write_line\("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:FRAMEBUFFER_BAR_LAYOUT_FAILED"\);\s*'
            r'serial::write_line\("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:PCI_CONFIG_READ_ONLY"\);\s*'
            r"halt\(\);\s*\}",
        )
        self.assertIn("BAR_SLOT_{n}_BASE", self.plan_text)
        for forbidden in (
            "write_config",
            "BAR_MAPPED",
            "MMIO",
            "REGISTER",
            "DMA",
            "INTERRUPT",
            "RESET",
            "BUS_MASTER",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, self.bar_boot_text)

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

    def test_bar_profile_is_isolated_without_changing_identity_dispatch(self):
        self.assertIn("network-hardware-bar-probe = []", self.cargo_text)
        for feature in (
            "normal-session",
            "verify",
            "hardware-probe",
            "usb-xhci-probe",
            "network-hardware-probe",
        ):
            with self.subTest(feature=feature):
                self.assertRegex(
                    self.main_text,
                    rf'#\[cfg\(all\(\s*feature = "{feature}",\s*'
                    r'feature = "network-hardware-bar-probe"\s*\)\)\]',
                )

        self.assertIn(
            '#[cfg(all(not(test), feature = "network-hardware-probe"))]\n'
            "    network_hardware_probe_boot::run(boot_info, &mut physical_memory);",
            self.main_text,
        )
        self.assertIn(
            '#[cfg(all(not(test), feature = "network-hardware-bar-probe"))]\n'
            "    network_hardware_bar_probe_boot::run(boot_info, &mut physical_memory);",
            self.main_text,
        )
        self.assertIn(
            "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:ENTER",
            self.identity_boot_text,
        )
        self.assertNotIn("NETWORK_HARDWARE_BAR_PROBE", self.identity_boot_text)


class NetworkHardwareBarProbeOracleTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.oracle = load_bar_oracle()

    def test_runner_and_probe_command_are_backend_free(self):
        for device in ("e1000", "e1000e"):
            with self.subTest(device=device):
                self.oracle.assert_runner_isolated(device)
                command = self.oracle.probe_runner_command(device)
                self.assertIn("--no-virtio-blk", command)
                self.assertNotIn("--virtio-net", command)
                self.assertNotIn("-netdev", command)
                self.assertNotIn("socket", " ".join(command))

    def test_parser_accepts_complete_reports_for_both_models(self):
        for device in ("e1000", "e1000e"):
            with self.subTest(device=device):
                self.oracle.assert_bar_probe_report(
                    self.oracle.synthetic_serial_report(device), device
                )

    def test_parser_rejects_wrong_device_and_missing_bar_slot(self):
        serial = self.oracle.synthetic_serial_report("e1000").replace(
            "NETWORK_DEVICE_ID=0x000000000000100E",
            "NETWORK_DEVICE_ID=0x00000000000010D3",
        )
        with self.assertRaises(AssertionError):
            self.oracle.assert_bar_probe_report(serial, "e1000")

        serial = self.oracle.synthetic_serial_report("e1000").replace(
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_2_BASE=0x0000000000001000\n", ""
        )
        with self.assertRaises(AssertionError):
            self.oracle.assert_bar_probe_report(serial, "e1000")

    def test_parser_rejects_malformed_pairs_and_control_markers(self):
        serial = self.oracle.synthetic_serial_report("e1000").replace(
            "BAR_SLOT_0_KIND=MEMORY64", "BAR_SLOT_0_KIND=MEMORY32"
        )
        with self.assertRaises(AssertionError):
            self.oracle.assert_bar_probe_report(serial, "e1000")

    def test_parser_rejects_malformed_layouts_and_out_of_range_fields(self):
        valid = self.oracle.synthetic_serial_report("e1000")
        malformed = valid.replace(
            "BAR_SLOT_5_RAW_LOW=0x0000000000000000",
            "BAR_SLOT_5_RAW_LOW=0x0000000000000006",
        ).replace("BAR_SLOT_5_KIND=UNIMPLEMENTED", "BAR_SLOT_5_KIND=MALFORMED")
        raw_high_above_dword = valid.replace(
            "BAR_SLOT_0_RAW_HIGH=0x0000000000000001",
            "BAR_SLOT_0_RAW_HIGH=0x0000000100000000",
        ).replace(
            "BAR_SLOT_0_BASE=0x0000000100000000",
            "BAR_SLOT_0_BASE=0x10000000000000000",
        )
        out_of_range_slot = valid + (
            "\nPYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_6_RAW_LOW="
            "0x0000000000000000"
        )
        for serial in (malformed, raw_high_above_dword, out_of_range_slot):
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                self.oracle.assert_bar_probe_report(serial, "e1000")

        serial = self.oracle.synthetic_serial_report("e1000") + "\nNETWORK_DMA"
        with self.assertRaises(AssertionError):
            self.oracle.assert_bar_probe_report(serial, "e1000")


if __name__ == "__main__":
    unittest.main()
