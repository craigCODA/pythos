#!/usr/bin/env python
"""QEMU acceptance oracle for the read-only PCI network BAR-layout probe."""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
PREFIX = "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:"
SUCCESS_MARKER = "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE_READY"
QEMU_TIMEOUT_SECONDS = 20
NETWORK_IDENTITIES = {
    "e1000": {
        "vendor": "0x0000000000008086",
        "device": "0x000000000000100E",
        "class": "0x0000000000000002",
        "subclass": "0x0000000000000000",
        "prog_if": "0x0000000000000000",
    },
    "e1000e": {
        "vendor": "0x0000000000008086",
        "device": "0x00000000000010D3",
        "class": "0x0000000000000002",
        "subclass": "0x0000000000000000",
        "prog_if": "0x0000000000000000",
    },
}
IDENTITY_FIELDS = (
    ("NETWORK_VENDOR", "vendor"),
    ("NETWORK_DEVICE_ID", "device"),
    ("NETWORK_CLASS", "class"),
    ("NETWORK_SUBCLASS", "subclass"),
    ("NETWORK_PROG_IF", "prog_if"),
)
ALLOWED_KINDS = {
    "IO",
    "MEMORY32",
    "MEMORY_BELOW_1MIB",
    "MEMORY64",
    "UNIMPLEMENTED",
}
FORBIDDEN_MARKER_FRAGMENTS = (
    "MMIO",
    "BAR_MAPPED",
    "REGISTER",
    "DMA",
    "INTERRUPT",
    "RESET",
    "BUS_MASTER",
    "NETWORK_FRAME",
    "FRAME_MOVEMENT",
    "PACKET",
    "PCI_CONFIG_WRITE",
    "CONFIGURATION_WRITE",
    "FRAMEBUFFER_BAR_LAYOUT_FAILED",
)
HEX64 = re.compile(r"0x[0-9A-F]{16}\Z")
BAR_LINE = re.compile(rf"{re.escape(PREFIX)}BAR_SLOT_([0-5])_(RAW_LOW|RAW_HIGH|KIND|BASE)=(.*)\Z")


def load_qemu_runner():
    path = ROOT / "scripts" / "run-qemu.py"
    spec = importlib.util.spec_from_file_location("network_hardware_bar_qemu_runner", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def lines(text: str) -> list[str]:
    return text.splitlines()


def require_exactly_one(serial_lines: list[str], marker: str) -> int:
    matches = [index for index, line in enumerate(serial_lines) if line == marker]
    if len(matches) != 1:
        raise AssertionError(f"expected exactly one {marker!r}, found {len(matches)}")
    return matches[0]


def require_identity_line(serial_lines: list[str], marker: str) -> int:
    matches = [index for index, line in enumerate(serial_lines) if line == marker]
    if len(matches) != 1:
        raise AssertionError(f"expected selected-controller marker {marker!r}, found {len(matches)}")
    return matches[0]


def parse_hex64(value: str, field: str) -> int:
    if not HEX64.fullmatch(value):
        raise AssertionError(f"malformed {field}: {value!r}")
    return int(value, 16)


def expected_bar(raw_low: int, slot: int) -> tuple[str, str, str]:
    if raw_low == 0:
        return "UNIMPLEMENTED", "NONE", "NONE"
    if raw_low & 1:
        return "IO", "NONE", f"0x{raw_low & ~0b11:016X}"

    memory_type = (raw_low >> 1) & 0b11
    if memory_type == 0:
        return "MEMORY32", "NONE", f"0x{raw_low & ~0b1111:016X}"
    if memory_type == 1:
        return "MEMORY_BELOW_1MIB", "NONE", f"0x{raw_low & ~0b1111:016X}"
    if memory_type == 3 or slot == 5:
        return "MALFORMED", "NONE", "NONE"
    return "MEMORY64", "HIGH_DWORD_REQUIRED", "BASE_FROM_HIGH_DWORD"


def assert_bar_probe_report(serial: str, network_device: str) -> None:
    if network_device not in NETWORK_IDENTITIES:
        raise ValueError(f"unsupported network device: {network_device}")

    serial_lines = lines(serial)
    for line in serial_lines:
        if any(fragment in line.upper() for fragment in FORBIDDEN_MARKER_FRAGMENTS):
            raise AssertionError(f"forbidden BAR-probe evidence: {line}")
        if line.startswith(f"{PREFIX}BAR_SLOT_") and BAR_LINE.fullmatch(line) is None:
            raise AssertionError(f"malformed or out-of-range BAR slot marker: {line}")
    if f"{PREFIX}NETWORK_CONTROLLER_NOT_FOUND" in serial_lines:
        raise AssertionError("conflicting NETWORK_CONTROLLER_NOT_FOUND marker")

    ordered = [
        f"{PREFIX}ENTER",
        f"{PREFIX}PCI_SCAN_READY",
        f"{PREFIX}NETWORK_CONTROLLER_FOUND",
    ]
    ordered.extend(
        f"{PREFIX}{marker}={NETWORK_IDENTITIES[network_device][identity_key]}"
        for marker, identity_key in IDENTITY_FIELDS
    )
    ordered.extend(
        (
            f"{PREFIX}BAR_SCAN_READY",
            f"{PREFIX}BAR_LAYOUT_READY",
            f"{PREFIX}FRAMEBUFFER_BAR_LAYOUT_READY",
            f"{PREFIX}PCI_CONFIG_READ_ONLY",
            SUCCESS_MARKER,
        )
    )
    positions = [require_identity_line(serial_lines, marker) for marker in ordered]
    if positions != sorted(positions):
        raise AssertionError("BAR-probe marker order violation")

    bar_start = positions[-5]
    bar_end = positions[-4]
    records: dict[int, dict[str, tuple[str, int]]] = {}
    for index, line in enumerate(serial_lines):
        match = BAR_LINE.fullmatch(line)
        if match is None:
            continue
        if not bar_start < index < bar_end:
            raise AssertionError(f"BAR slot record outside bounded report: {line}")
        slot = int(match.group(1))
        field = match.group(2)
        value = match.group(3)
        if field in records.setdefault(slot, {}):
            raise AssertionError(f"duplicate BAR slot {slot} {field} record")
        records[slot][field] = (value, index)

    slot = 0
    previous = bar_start
    while slot < 6:
        fields = records.get(slot)
        if fields is None:
            raise AssertionError(f"missing BAR slot {slot} record")
        if set(fields) != {"RAW_LOW", "RAW_HIGH", "KIND", "BASE"}:
            raise AssertionError(f"incomplete BAR slot {slot} record")
        field_positions = [fields[field][1] for field in ("RAW_LOW", "RAW_HIGH", "KIND", "BASE")]
        if field_positions != sorted(field_positions) or field_positions[0] <= previous:
            raise AssertionError(f"BAR slot {slot} marker order violation")
        previous = field_positions[-1]

        raw_low = parse_hex64(fields["RAW_LOW"][0], f"BAR_SLOT_{slot}_RAW_LOW")
        if raw_low > 0xFFFF_FFFF:
            raise AssertionError(f"BAR_SLOT_{slot}_RAW_LOW is not a PCI dword")
        kind = fields["KIND"][0]
        raw_high = fields["RAW_HIGH"][0]
        base = fields["BASE"][0]
        if kind not in ALLOWED_KINDS:
            raise AssertionError(f"unsupported BAR_SLOT_{slot}_KIND: {kind!r}")

        expected_kind, expected_high, expected_base = expected_bar(raw_low, slot)
        if expected_kind == "MALFORMED":
            raise AssertionError(f"malformed BAR layout at slot {slot}")
        if kind != expected_kind:
            raise AssertionError(
                f"BAR_SLOT_{slot}_KIND {kind!r} does not decode from RAW_LOW"
            )
        if expected_high == "HIGH_DWORD_REQUIRED":
            high = parse_hex64(raw_high, f"BAR_SLOT_{slot}_RAW_HIGH")
            if high > 0xFFFF_FFFF:
                raise AssertionError(f"BAR_SLOT_{slot}_RAW_HIGH is not a PCI dword")
            expected_base = f"0x{((high << 32) | (raw_low & ~0b1111)):016X}"
            if slot + 1 in records:
                raise AssertionError(f"duplicate consumed-high-slot BAR_SLOT_{slot + 1} record")
            if base != expected_base:
                raise AssertionError(f"BAR_SLOT_{slot}_BASE does not match its 64-bit pair")
            slot += 2
        else:
            if raw_high != expected_high or base != expected_base:
                raise AssertionError(f"BAR_SLOT_{slot} raw/high/base representation is malformed")
            slot += 1

    unexpected_slots = set(records).difference({0, 1, 2, 3, 4, 5})
    if unexpected_slots:
        raise AssertionError(f"out-of-range BAR slots: {sorted(unexpected_slots)}")


def assert_qemu_success(qemu_output: str) -> None:
    outcomes = [line for line in lines(qemu_output) if line.startswith("QEMU_OUTCOME ")]
    if outcomes != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcomes!r}")


def assert_runner_isolated(network_device: str) -> None:
    runner_args = load_qemu_runner().network_device_qemu_args(network_device)
    nic_pairs = [
        runner_args[index : index + 2]
        for index, value in enumerate(runner_args)
        if value == "-nic"
    ]
    explicit_devices = [
        runner_args[index + 1].split(",", 1)[0]
        for index, value in enumerate(runner_args[:-1])
        if value == "-device"
    ]
    if nic_pairs != [["-nic", "none"]]:
        raise AssertionError(f"runner did not disable implicit NICs: {runner_args!r}")
    if explicit_devices != [network_device]:
        raise AssertionError(f"runner explicit devices are not isolated: {runner_args!r}")
    if any("netdev" in value or "socket" in value for value in runner_args):
        raise AssertionError(f"runner included a network backend: {runner_args!r}")


def probe_runner_command(network_device: str) -> list[str]:
    if network_device not in NETWORK_IDENTITIES:
        raise ValueError(f"unsupported network device: {network_device}")
    return [
        sys.executable,
        "scripts/run-qemu.py",
        "--serial-log",
        str(TARGET / f"network-hardware-bar-probe-{network_device}-com1.log"),
        "--success-marker",
        SUCCESS_MARKER,
        "--timeout",
        str(QEMU_TIMEOUT_SECONDS),
        "--no-audio-device",
        "--no-virtio-blk",
        "--network-device",
        network_device,
        "--expect-outcome",
        "success",
    ]


def run(command: list[str]) -> str:
    print("+ " + " ".join(command), flush=True)
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout, end="")
    if result.returncode != 0:
        raise AssertionError(f"command failed ({result.returncode}): {' '.join(command)}")
    return result.stdout


def build_probe_image() -> None:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(
        [
            "cargo",
            "build",
            "-p",
            "pythos-core",
            "--target",
            "x86_64-unknown-none",
            "--no-default-features",
            "--features",
            "network-hardware-bar-probe",
        ]
    )
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-image.py"])


def run_probe_boot(network_device: str) -> tuple[str, str]:
    serial_log = TARGET / f"network-hardware-bar-probe-{network_device}-com1.log"
    if serial_log.exists():
        serial_log.unlink()
    output = run(probe_runner_command(network_device))
    return serial_log.read_text(encoding="utf-8", errors="replace"), output


def synthetic_serial_report(network_device: str) -> str:
    identity = NETWORK_IDENTITIES[network_device]
    return "\n".join(
        (
            f"{PREFIX}ENTER",
            f"{PREFIX}PCI_SCAN_READY",
            f"{PREFIX}NETWORK_CONTROLLER_FOUND",
            f"{PREFIX}NETWORK_VENDOR={identity['vendor']}",
            f"{PREFIX}NETWORK_DEVICE_ID={identity['device']}",
            f"{PREFIX}NETWORK_CLASS={identity['class']}",
            f"{PREFIX}NETWORK_SUBCLASS={identity['subclass']}",
            f"{PREFIX}NETWORK_PROG_IF={identity['prog_if']}",
            f"{PREFIX}BAR_SCAN_READY",
            f"{PREFIX}BAR_SLOT_0_RAW_LOW=0x0000000000000004",
            f"{PREFIX}BAR_SLOT_0_RAW_HIGH=0x0000000000000001",
            f"{PREFIX}BAR_SLOT_0_KIND=MEMORY64",
            f"{PREFIX}BAR_SLOT_0_BASE=0x0000000100000000",
            f"{PREFIX}BAR_SLOT_2_RAW_LOW=0x0000000000001001",
            f"{PREFIX}BAR_SLOT_2_RAW_HIGH=NONE",
            f"{PREFIX}BAR_SLOT_2_KIND=IO",
            f"{PREFIX}BAR_SLOT_2_BASE=0x0000000000001000",
            f"{PREFIX}BAR_SLOT_3_RAW_LOW=0x00000000FEBF0000",
            f"{PREFIX}BAR_SLOT_3_RAW_HIGH=NONE",
            f"{PREFIX}BAR_SLOT_3_KIND=MEMORY32",
            f"{PREFIX}BAR_SLOT_3_BASE=0x00000000FEBF0000",
            f"{PREFIX}BAR_SLOT_4_RAW_LOW=0x0000000000000000",
            f"{PREFIX}BAR_SLOT_4_RAW_HIGH=NONE",
            f"{PREFIX}BAR_SLOT_4_KIND=UNIMPLEMENTED",
            f"{PREFIX}BAR_SLOT_4_BASE=NONE",
            f"{PREFIX}BAR_SLOT_5_RAW_LOW=0x0000000000000000",
            f"{PREFIX}BAR_SLOT_5_RAW_HIGH=NONE",
            f"{PREFIX}BAR_SLOT_5_KIND=UNIMPLEMENTED",
            f"{PREFIX}BAR_SLOT_5_BASE=NONE",
            f"{PREFIX}BAR_LAYOUT_READY",
            f"{PREFIX}FRAMEBUFFER_BAR_LAYOUT_READY",
            f"{PREFIX}PCI_CONFIG_READ_ONLY",
            SUCCESS_MARKER,
        )
    )


class NetworkHardwareBarProbeSelfTest(unittest.TestCase):
    def test_runner_has_one_explicit_backend_free_device(self) -> None:
        for device in NETWORK_IDENTITIES:
            with self.subTest(device=device):
                assert_runner_isolated(device)
                command = probe_runner_command(device)
                self.assertIn("--no-virtio-blk", command)
                self.assertNotIn("--virtio-net", command)
                self.assertNotIn("-netdev", command)
                self.assertNotIn("socket", " ".join(command))

    def test_parser_accepts_complete_ordered_reports_for_both_models(self) -> None:
        for device in NETWORK_IDENTITIES:
            with self.subTest(device=device):
                assert_bar_probe_report(synthetic_serial_report(device), device)

    def test_parser_uses_the_boot_path_final_success_marker(self) -> None:
        serial = synthetic_serial_report("e1000").replace(
            SUCCESS_MARKER,
            "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE_READY",
        )
        assert_bar_probe_report(serial, "e1000")

    def test_parser_rejects_missing_controller_identity_fields(self) -> None:
        for marker, _ in IDENTITY_FIELDS:
            with self.subTest(marker=marker):
                serial = synthetic_serial_report("e1000").replace(
                    f"{PREFIX}{marker}=", f"{PREFIX}MISSING_{marker}=", 1
                )
                with self.assertRaises(AssertionError):
                    assert_bar_probe_report(serial, "e1000")

    def test_parser_rejects_wrong_model_identity(self) -> None:
        serial = synthetic_serial_report("e1000").replace(
            "NETWORK_DEVICE_ID=0x000000000000100E",
            "NETWORK_DEVICE_ID=0x00000000000010D3",
        )
        with self.assertRaises(AssertionError):
            assert_bar_probe_report(serial, "e1000")

    def test_parser_rejects_conflicting_controller_results(self) -> None:
        serial = synthetic_serial_report("e1000") + f"\n{PREFIX}NETWORK_CONTROLLER_NOT_FOUND"
        with self.assertRaises(AssertionError):
            assert_bar_probe_report(serial, "e1000")

    def test_parser_rejects_missing_or_malformed_slot_fields(self) -> None:
        valid = synthetic_serial_report("e1000")
        variants = (
            valid.replace(f"{PREFIX}BAR_SLOT_2_BASE=0x0000000000001000\n", ""),
            valid.replace("BAR_SLOT_2_RAW_LOW=0x0000000000001001", "BAR_SLOT_2_RAW_LOW=0x1"),
            valid.replace("BAR_SLOT_0_RAW_HIGH=0x0000000000000001", "BAR_SLOT_0_RAW_HIGH=NONE"),
            valid.replace("BAR_SLOT_3_BASE=0x00000000FEBF0000", "BAR_SLOT_3_BASE=0xFEBF0000"),
        )
        for serial in variants:
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                assert_bar_probe_report(serial, "e1000")

    def test_parser_rejects_bad_64_bit_pairing_and_consumed_slot_duplicates(self) -> None:
        valid = synthetic_serial_report("e1000")
        wrong_kind = valid.replace("BAR_SLOT_0_KIND=MEMORY64", "BAR_SLOT_0_KIND=MEMORY32")
        duplicate_high_slot = valid.replace(
            f"{PREFIX}BAR_LAYOUT_READY",
            f"{PREFIX}BAR_SLOT_1_RAW_LOW=0x0000000000000001\n"
            f"{PREFIX}BAR_SLOT_1_RAW_HIGH=NONE\n"
            f"{PREFIX}BAR_SLOT_1_KIND=IO\n"
            f"{PREFIX}BAR_SLOT_1_BASE=0x0000000000000000\n"
            f"{PREFIX}BAR_LAYOUT_READY",
        )
        for serial in (wrong_kind, duplicate_high_slot):
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                assert_bar_probe_report(serial, "e1000")

    def test_parser_rejects_malformed_bar_layout(self) -> None:
        serial = synthetic_serial_report("e1000").replace(
            f"{PREFIX}BAR_SLOT_5_RAW_LOW=0x0000000000000000\n"
            f"{PREFIX}BAR_SLOT_5_RAW_HIGH=NONE\n"
            f"{PREFIX}BAR_SLOT_5_KIND=UNIMPLEMENTED\n"
            f"{PREFIX}BAR_SLOT_5_BASE=NONE",
            f"{PREFIX}BAR_SLOT_5_RAW_LOW=0x0000000000000006\n"
            f"{PREFIX}BAR_SLOT_5_RAW_HIGH=NONE\n"
            f"{PREFIX}BAR_SLOT_5_KIND=MALFORMED\n"
            f"{PREFIX}BAR_SLOT_5_BASE=NONE",
        )
        with self.assertRaises(AssertionError):
            assert_bar_probe_report(serial, "e1000")

    def test_parser_rejects_raw_high_above_a_pci_dword(self) -> None:
        serial = synthetic_serial_report("e1000").replace(
            "BAR_SLOT_0_RAW_HIGH=0x0000000000000001",
            "BAR_SLOT_0_RAW_HIGH=0x0000000100000000",
        ).replace(
            "BAR_SLOT_0_BASE=0x0000000100000000",
            "BAR_SLOT_0_BASE=0x10000000000000000",
        )
        with self.assertRaises(AssertionError):
            assert_bar_probe_report(serial, "e1000")

    def test_parser_rejects_out_of_range_bar_slot_marker(self) -> None:
        serial = synthetic_serial_report("e1000") + (
            f"\n{PREFIX}BAR_SLOT_6_RAW_LOW=0x0000000000000000"
        )
        with self.assertRaises(AssertionError):
            assert_bar_probe_report(serial, "e1000")

    def test_parser_rejects_forbidden_evidence_and_missing_final_markers(self) -> None:
        valid = synthetic_serial_report("e1000")
        for marker in FORBIDDEN_MARKER_FRAGMENTS:
            with self.subTest(marker=marker), self.assertRaises(AssertionError):
                assert_bar_probe_report(valid + "\n" + marker, "e1000")
        with self.assertRaises(AssertionError):
            assert_bar_probe_report(valid.replace(SUCCESS_MARKER, ""), "e1000")
        with self.assertRaises(AssertionError):
            assert_qemu_success("QEMU_OUTCOME reset")


def main() -> int:
    build_probe_image()
    for device in NETWORK_IDENTITIES:
        assert_runner_isolated(device)
        serial, output = run_probe_boot(device)
        assert_bar_probe_report(serial, device)
        assert_qemu_success(output)
    print("NETWORK_HARDWARE_BAR_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        result = unittest.TextTestRunner(verbosity=2).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(NetworkHardwareBarProbeSelfTest)
        )
        raise SystemExit(0 if result.wasSuccessful() else 1)
    if sys.argv[1:]:
        raise SystemExit("usage: test-network-hardware-bar-probe.py [--self-test]")
    raise SystemExit(main())
