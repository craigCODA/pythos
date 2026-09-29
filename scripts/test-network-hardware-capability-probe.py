#!/usr/bin/env python
"""QEMU acceptance oracle for the read-only PCI capability-list snapshot."""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
PREFIX = "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:"
SUCCESS_MARKER = "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE_READY"
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
SUMMARY_FIELDS = ("PM", "PCIE", "MSI", "MSIX")
KIND_LENGTHS = {
    "POWER_MANAGEMENT": 0x08,
    "PCIE": 0x14,
    "MSI": 0x0A,
    "MSIX": 0x0C,
}
KIND_TO_SUMMARY = {
    "POWER_MANAGEMENT": "PM",
    "PCIE": "PCIE",
    "MSI": "MSI",
    "MSIX": "MSIX",
}
ID_KIND_HEADERS = {
    0x01: ("POWER_MANAGEMENT", 0x08),
    0x10: ("PCIE", 0x14),
    0x05: ("MSI", 0x0A),
    0x11: ("MSIX", 0x0C),
}
HEX64 = re.compile(r"0x[0-9A-F]{16}\Z")
HEX8 = re.compile(r"0x[0-9A-F]{2}\Z")
ENTRY_LINE = re.compile(
    rf"{re.escape(PREFIX)}PCI_CAPABILITY_ENTRY="
    r"ID=(0x[0-9A-F]{2});OFFSET=(0x[0-9A-F]{2});NEXT=(0x[0-9A-F]{2});"
    r"KIND=(POWER_MANAGEMENT|PCIE|MSI|MSIX|UNKNOWN);HEADER_LEN=(0x[0-9A-F]{2}|NONE)\Z"
)
SUMMARY_LINE = re.compile(
    rf"{re.escape(PREFIX)}PCI_CAPABILITY_SUMMARY="
    r"PM=(0x[0-9A-F]{2}|NONE);PCIE=(0x[0-9A-F]{2}|NONE);"
    r"MSI=(0x[0-9A-F]{2}|NONE);MSIX=(0x[0-9A-F]{2}|NONE)\Z"
)
INTERRUPT_LINE = re.compile(
    rf"{re.escape(PREFIX)}PCI_INTERRUPT_METADATA=LINE=(0x[0-9A-F]{{2}});PIN=(0x[0-9A-F]{{2}})\Z"
)
MALFORMED_MARKERS = {
    f"{PREFIX}PCI_CAPABILITY_MALFORMED=INVALID_POINTER",
    f"{PREFIX}PCI_CAPABILITY_MALFORMED=REPEATED_OFFSET",
    f"{PREFIX}PCI_CAPABILITY_MALFORMED=ENTRY_LIMIT",
    f"{PREFIX}PCI_CAPABILITY_MALFORMED=RECOGNIZED_HEADER_BOUNDS",
}
FORBIDDEN_OPERATION_FRAGMENTS = (
    "pci_config_write",
    "configuration_write",
    "write_config",
    "bar_write",
    "bar_read",
    "read_bar",
    "read_controller_bar",
    "bar_mapping",
    "bar_mapped",
    "map_bar",
    "bar_dereference",
    "dereference_bar",
    "mmio",
    "device_register",
    "register_access",
    "register_read",
    "read_register",
    "register_write",
    "write_register",
    "bus_master",
    "dma",
    "interrupt_enable",
    "enable_interrupt",
    "interrupt_handler",
    "interrupt_ack",
    "ack_interrupt",
    "irq_",
    "msi_enable",
    "msix_enable",
    "msi-x_enable",
    "power_control",
    "power_state",
    "reset",
    "queue",
    "network_frame",
    "ethernet_frame",
    "frame_access",
    "read_frame",
    "write_frame",
    "packet",
    "socket",
    "wi-fi",
    "wi_fi",
    "wifi",
    "network_port",
    "networkport",
    "virtio_transport",
    "virtiotransport",
    "network_hardware_bar_probe",
    "network_hardware_register_probe",
    "network_hardware_register_enable_probe",
)
FORBIDDEN_EVIDENCE_FRAGMENTS = tuple(fragment.upper() for fragment in FORBIDDEN_OPERATION_FRAGMENTS)
FORBIDDEN_SOURCE_FRAGMENTS = FORBIDDEN_OPERATION_FRAGMENTS
SOURCE_SAFETY_FILES = (
    ROOT / "core" / "src" / "network_hardware_capability_probe_boot.rs",
    ROOT / "core" / "src" / "network_hardware_capability_probe_screen.rs",
    ROOT / "core" / "src" / "network_hardware_capability_probe.rs",
)
READY_EXACT_MARKERS = {
    f"{PREFIX}ENTER",
    f"{PREFIX}PCI_SCAN_READY",
    f"{PREFIX}NETWORK_CONTROLLER_FOUND",
    f"{PREFIX}PCI_CONFIG_HEADER_READY",
    f"{PREFIX}PCI_CAPABILITY_LIST_PRESENT",
    f"{PREFIX}PCI_CAPABILITY_LIST_ABSENT",
    f"{PREFIX}PCI_CONFIG_READ_ONLY",
    f"{PREFIX}FRAMEBUFFER_CAPABILITY_READY",
}
READY_VALUE_MARKERS = (
    f"{PREFIX}NETWORK_BUS=",
    f"{PREFIX}NETWORK_DEVICE=",
    f"{PREFIX}NETWORK_FUNCTION=",
    f"{PREFIX}NETWORK_VENDOR=",
    f"{PREFIX}NETWORK_DEVICE_ID=",
    f"{PREFIX}NETWORK_SUBSYSTEM_VENDOR=",
    f"{PREFIX}NETWORK_SUBSYSTEM_DEVICE=",
    f"{PREFIX}NETWORK_CLASS=",
    f"{PREFIX}NETWORK_SUBCLASS=",
    f"{PREFIX}NETWORK_PROG_IF=",
    f"{PREFIX}PCI_CAPABILITIES_STATUS=",
    f"{PREFIX}PCI_CAPABILITY_POINTER=",
    f"{PREFIX}PCI_CAPABILITY_ENTRY=",
    f"{PREFIX}PCI_CAPABILITY_SUMMARY=",
    f"{PREFIX}PCI_INTERRUPT_METADATA=",
)


def load_qemu_runner():
    path = ROOT / "scripts" / "run-qemu.py"
    spec = importlib.util.spec_from_file_location("network_hardware_capability_qemu_runner", path)
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


def require_value(serial_lines: list[str], marker: str, pattern: re.Pattern[str]) -> tuple[int, str]:
    matches = [
        (index, line.removeprefix(marker))
        for index, line in enumerate(serial_lines)
        if line.startswith(marker)
    ]
    if len(matches) != 1 or pattern.fullmatch(matches[0][1]) is None:
        raise AssertionError(f"expected one well-formed value for {marker!r}, got {matches!r}")
    return matches[0]


def assert_no_forbidden_evidence(serial_lines: list[str]) -> None:
    for line in serial_lines:
        upper = line.upper()
        if any(fragment in upper for fragment in FORBIDDEN_EVIDENCE_FRAGMENTS):
            raise AssertionError(f"forbidden capability-probe evidence: {line}")


def assert_source_text_is_safe(source: str, source_name: str = "synthetic source") -> None:
    normalized = source.lower()
    for fragment in FORBIDDEN_SOURCE_FRAGMENTS:
        if fragment in normalized:
            raise AssertionError(f"forbidden capability-probe source fragment {fragment!r} in {source_name}")


def assert_static_source_safety() -> None:
    for path in SOURCE_SAFETY_FILES:
        assert_source_text_is_safe(path.read_text(encoding="utf-8"), str(path))


def assert_runner_args_isolated(runner_args: list[str], network_device: str) -> None:
    if network_device not in NETWORK_IDENTITIES:
        raise ValueError(f"unsupported network device: {network_device}")
    if any(value == "-net" or value.startswith("-net=") for value in runner_args):
        raise AssertionError(f"runner included a legacy network backend: {runner_args!r}")
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
    if any(any(token in value.lower() for token in ("netdev", "socket", "backend")) for value in runner_args):
        raise AssertionError(f"runner included a network backend: {runner_args!r}")


def assert_runner_isolated(network_device: str) -> None:
    if network_device not in NETWORK_IDENTITIES:
        raise ValueError(f"unsupported network device: {network_device}")
    runner_args = load_qemu_runner().network_device_qemu_args(network_device)
    assert_runner_args_isolated(runner_args, network_device)


def parse_entry(line: str) -> dict[str, int | str | None]:
    match = ENTRY_LINE.fullmatch(line)
    if match is None:
        raise AssertionError(f"malformed PCI capability entry: {line}")
    identifier, offset, next_pointer, kind, header_length = match.groups()
    return {
        "id": int(identifier, 16),
        "offset": int(offset, 16),
        "next": int(next_pointer, 16),
        "kind": kind,
        "header_len": None if header_length == "NONE" else int(header_length, 16),
    }


def validate_capability_entries(entry_lines: list[str]) -> list[dict[str, int | str | None]]:
    if len(entry_lines) > 48:
        raise AssertionError(f"capability entry count exceeds 48: {len(entry_lines)}")
    entries = [parse_entry(line) for line in entry_lines]
    offsets: set[int] = set()
    for index, entry in enumerate(entries):
        identifier = entry["id"]
        offset = entry["offset"]
        next_pointer = entry["next"]
        kind = entry["kind"]
        header_len = entry["header_len"]
        assert isinstance(identifier, int) and isinstance(offset, int) and isinstance(next_pointer, int)
        assert isinstance(kind, str)
        if not 0x40 <= offset <= 0xFC or offset & 0x03:
            raise AssertionError(f"invalid capability offset: 0x{offset:02X}")
        if offset in offsets:
            raise AssertionError(f"duplicate capability offset: 0x{offset:02X}")
        offsets.add(offset)
        if next_pointer and (not 0x40 <= next_pointer <= 0xFC or next_pointer & 0x03):
            raise AssertionError(f"invalid capability next pointer: 0x{next_pointer:02X}")
        expected_kind, expected_length = ID_KIND_HEADERS.get(identifier, ("UNKNOWN", None))
        if (kind, header_len) != (expected_kind, expected_length):
            raise AssertionError(
                f"capability ID 0x{identifier:02X} requires "
                f"{expected_kind}/{('NONE' if expected_length is None else f'0x{expected_length:02X}')}, "
                f"got {kind}/{('NONE' if header_len is None else f'0x{header_len:02X}')}"
            )
        if expected_length is not None and offset + expected_length - 1 > 0xFF:
            raise AssertionError(f"{kind} header crosses PCI configuration space")
        expected_next = entries[index + 1]["offset"] if index + 1 < len(entries) else 0
        if next_pointer != expected_next:
            raise AssertionError(
                f"capability traversal mismatch at 0x{offset:02X}: "
                f"next is 0x{next_pointer:02X}, expected 0x{expected_next:02X}"
            )
    return entries


def parse_summary(line: str) -> dict[str, int | None]:
    match = SUMMARY_LINE.fullmatch(line)
    if match is None:
        raise AssertionError(f"malformed PCI capability summary: {line}")
    summary = {
        name: None if value == "NONE" else int(value, 16)
        for name, value in zip(SUMMARY_FIELDS, match.groups(), strict=True)
    }
    claimed = [value for value in summary.values() if value is not None]
    if len(claimed) != len(set(claimed)):
        raise AssertionError("summary claims the same capability offset more than once")
    return summary


def assert_summary_matches_entries(summary: dict[str, int | None], entries: list[dict[str, int | str | None]]) -> None:
    expected: dict[str, int | None] = {field: None for field in SUMMARY_FIELDS}
    for entry in entries:
        kind = entry["kind"]
        offset = entry["offset"]
        assert isinstance(kind, str) and isinstance(offset, int)
        summary_field = KIND_TO_SUMMARY.get(kind)
        if summary_field is not None and expected[summary_field] is None:
            expected[summary_field] = offset
    if summary != expected:
        raise AssertionError(f"summary does not report the first traversal offsets: {summary!r} != {expected!r}")


def assert_capability_probe_report(serial: str, network_device: str) -> None:
    if network_device not in NETWORK_IDENTITIES:
        raise ValueError(f"unsupported network device: {network_device}")
    serial_lines = lines(serial)
    assert_no_forbidden_evidence(serial_lines)
    for line in serial_lines:
        if line.startswith(PREFIX) and line not in READY_EXACT_MARKERS and not any(
            line.startswith(marker) for marker in READY_VALUE_MARKERS
        ):
            raise AssertionError(f"unexpected capability-probe marker: {line}")
    if any(line in MALFORMED_MARKERS for line in serial_lines):
        raise AssertionError("malformed diagnostic cannot be accepted as a ready transcript")
    if f"{PREFIX}NETWORK_CONTROLLER_NOT_FOUND" in serial_lines:
        raise AssertionError("conflicting NETWORK_CONTROLLER_NOT_FOUND marker")

    identity = NETWORK_IDENTITIES[network_device]
    ordered_positions = [
        require_exactly_one(serial_lines, f"{PREFIX}ENTER"),
        require_exactly_one(serial_lines, f"{PREFIX}PCI_SCAN_READY"),
        require_exactly_one(serial_lines, f"{PREFIX}NETWORK_CONTROLLER_FOUND"),
    ]
    for marker in ("NETWORK_BUS=", "NETWORK_DEVICE=", "NETWORK_FUNCTION="):
        position, _ = require_value(serial_lines, f"{PREFIX}{marker}", HEX8)
        ordered_positions.append(position)
    for marker, identity_key in IDENTITY_FIELDS[:2]:
        position, value = require_value(serial_lines, f"{PREFIX}{marker}=", HEX64)
        if value != identity[identity_key]:
            raise AssertionError(f"wrong {marker} for {network_device}: {value}")
        ordered_positions.append(position)
    for marker in ("NETWORK_SUBSYSTEM_VENDOR=", "NETWORK_SUBSYSTEM_DEVICE="):
        position, _ = require_value(serial_lines, f"{PREFIX}{marker}", HEX64)
        ordered_positions.append(position)
    for marker, identity_key in IDENTITY_FIELDS[2:]:
        position, value = require_value(serial_lines, f"{PREFIX}{marker}=", HEX64)
        if value != identity[identity_key]:
            raise AssertionError(f"wrong {marker} for {network_device}: {value}")
        ordered_positions.append(position)
    header_position = require_exactly_one(serial_lines, f"{PREFIX}PCI_CONFIG_HEADER_READY")
    ordered_positions.append(header_position)
    status_position, status_value = require_value(serial_lines, f"{PREFIX}PCI_CAPABILITIES_STATUS=", HEX64)
    ordered_positions.append(status_position)
    if ordered_positions != sorted(ordered_positions):
        raise AssertionError("identity/header marker order violation")

    present_marker = f"{PREFIX}PCI_CAPABILITY_LIST_PRESENT"
    absent_marker = f"{PREFIX}PCI_CAPABILITY_LIST_ABSENT"
    present_count = serial_lines.count(present_marker)
    absent_count = serial_lines.count(absent_marker)
    if present_count + absent_count != 1:
        raise AssertionError("expected exactly one capability-list state marker")
    list_position = require_exactly_one(serial_lines, present_marker if present_count else absent_marker)
    pointer_position, pointer_value = require_value(
        serial_lines, f"{PREFIX}PCI_CAPABILITY_POINTER=", HEX8
    )
    pointer = int(pointer_value, 16)
    status = int(status_value, 16)
    if bool(status & (1 << 4)) != bool(present_count):
        raise AssertionError("capability-list status bit and list-state marker disagree")
    entry_candidates = [
        (index, line)
        for index, line in enumerate(serial_lines)
        if line.startswith(f"{PREFIX}PCI_CAPABILITY_ENTRY=")
    ]
    summary_candidates = [
        (index, line)
        for index, line in enumerate(serial_lines)
        if line.startswith(f"{PREFIX}PCI_CAPABILITY_SUMMARY=")
    ]
    if len(summary_candidates) != 1:
        raise AssertionError(f"expected exactly one capability summary, found {len(summary_candidates)}")
    summary_position, summary_line = summary_candidates[0]
    if not status_position < pointer_position < list_position < summary_position:
        raise AssertionError(
            "capability pointer/list state is not between status and summary"
        )
    if SUMMARY_LINE.fullmatch(summary_line) is None:
        raise AssertionError(f"malformed capability summary: {summary_line}")
    if any(ENTRY_LINE.fullmatch(line) is None for _, line in entry_candidates):
        raise AssertionError("malformed capability-entry marker")
    if any(not list_position < index < summary_position for index, _ in entry_candidates):
        raise AssertionError("capability entries are outside the bounded traversal section")
    entries = validate_capability_entries([line for _, line in entry_candidates])
    if present_count:
        expected_first = entries[0]["offset"] if entries else 0
        if pointer != expected_first:
            raise AssertionError(
                f"raw capability pointer 0x{pointer:02X} does not match first entry "
                f"0x{expected_first:02X}"
            )
    summary = parse_summary(summary_line)
    assert_summary_matches_entries(summary, entries)
    if not present_count and (entries or any(value is not None for value in summary.values())):
        raise AssertionError("absent capability list must have no entries and an all-NONE summary")

    interrupt_candidates = [
        (index, line)
        for index, line in enumerate(serial_lines)
        if line.startswith(f"{PREFIX}PCI_INTERRUPT_METADATA=")
    ]
    if len(interrupt_candidates) != 1 or INTERRUPT_LINE.fullmatch(interrupt_candidates[0][1]) is None:
        raise AssertionError(f"expected one well-formed interrupt metadata marker, got {interrupt_candidates!r}")
    interrupt_position = interrupt_candidates[0][0]
    readonly_position = require_exactly_one(serial_lines, f"{PREFIX}PCI_CONFIG_READ_ONLY")
    framebuffer_position = require_exactly_one(serial_lines, f"{PREFIX}FRAMEBUFFER_CAPABILITY_READY")
    success_position = require_exactly_one(serial_lines, SUCCESS_MARKER)
    if not summary_position < interrupt_position < readonly_position < framebuffer_position < success_position:
        raise AssertionError("capability report terminal marker order violation")


def assert_malformed_diagnostic(serial: str) -> None:
    serial_lines = lines(serial)
    assert_no_forbidden_evidence(serial_lines)
    malformed = [(index, line) for index, line in enumerate(serial_lines) if line in MALFORMED_MARKERS]
    if len(malformed) != 1:
        raise AssertionError(f"expected one malformed capability marker, got {malformed!r}")
    readonly_position = require_exactly_one(serial_lines, f"{PREFIX}PCI_CONFIG_READ_ONLY")
    diagnostic_position = require_exactly_one(
        serial_lines, f"{PREFIX}FRAMEBUFFER_CAPABILITY_DIAGNOSTIC_READY"
    )
    if SUCCESS_MARKER in serial_lines:
        raise AssertionError("malformed diagnostic emitted the final ready marker")
    if not malformed[0][0] < readonly_position < diagnostic_position:
        raise AssertionError("malformed diagnostic marker order violation")


def assert_qemu_success(qemu_output: str) -> None:
    outcomes = [line for line in lines(qemu_output) if line.startswith("QEMU_OUTCOME ")]
    if outcomes != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcomes!r}")


def probe_runner_command(network_device: str) -> list[str]:
    if network_device not in NETWORK_IDENTITIES:
        raise ValueError(f"unsupported network device: {network_device}")
    return [
        sys.executable,
        "scripts/run-qemu.py",
        "--serial-log",
        str(TARGET / f"network-hardware-capability-probe-{network_device}-com1.log"),
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
            "network-hardware-capability-probe",
        ]
    )
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-image.py"])


def run_probe_boot(network_device: str) -> tuple[str, str]:
    serial_log = TARGET / f"network-hardware-capability-probe-{network_device}-com1.log"
    if serial_log.exists():
        serial_log.unlink()
    output = run(probe_runner_command(network_device))
    return serial_log.read_text(encoding="utf-8", errors="replace"), output


def synthetic_serial_report(
    network_device: str,
    entries: list[tuple[int, int, str]] | None = None,
    *,
    list_present: bool = True,
    capability_pointer: int | None = None,
) -> str:
    if network_device not in NETWORK_IDENTITIES:
        raise ValueError(f"unsupported network device: {network_device}")
    if entries is None:
        entries = [(0x01, 0x40, "POWER_MANAGEMENT"), (0x10, 0x48, "PCIE"), (0x7F, 0x60, "UNKNOWN")]
    if not list_present:
        entries = []
    if capability_pointer is None:
        capability_pointer = entries[0][1] if entries else 0
    if not 0 <= capability_pointer <= 0xFF:
        raise ValueError(f"capability pointer is not a byte: {capability_pointer}")
    identity = NETWORK_IDENTITIES[network_device]
    status = 0x10 if list_present else 0
    report = [
        f"{PREFIX}ENTER",
        f"{PREFIX}PCI_SCAN_READY",
        f"{PREFIX}NETWORK_CONTROLLER_FOUND",
        f"{PREFIX}NETWORK_BUS=0x00",
        f"{PREFIX}NETWORK_DEVICE=0x03",
        f"{PREFIX}NETWORK_FUNCTION=0x00",
        f"{PREFIX}NETWORK_VENDOR={identity['vendor']}",
        f"{PREFIX}NETWORK_DEVICE_ID={identity['device']}",
        f"{PREFIX}NETWORK_SUBSYSTEM_VENDOR=0x0000000000008086",
        f"{PREFIX}NETWORK_SUBSYSTEM_DEVICE=0x0000000000000001",
        f"{PREFIX}NETWORK_CLASS={identity['class']}",
        f"{PREFIX}NETWORK_SUBCLASS={identity['subclass']}",
        f"{PREFIX}NETWORK_PROG_IF={identity['prog_if']}",
        f"{PREFIX}PCI_CONFIG_HEADER_READY",
        f"{PREFIX}PCI_CAPABILITIES_STATUS=0x{status:016X}",
        f"{PREFIX}PCI_CAPABILITY_POINTER=0x{capability_pointer:02X}",
        f"{PREFIX}{'PCI_CAPABILITY_LIST_PRESENT' if list_present else 'PCI_CAPABILITY_LIST_ABSENT'}",
    ]
    summary: dict[str, int | None] = {field: None for field in SUMMARY_FIELDS}
    for index, (identifier, offset, kind) in enumerate(entries):
        next_pointer = entries[index + 1][1] if index + 1 < len(entries) else 0
        header = "NONE" if kind == "UNKNOWN" else f"0x{KIND_LENGTHS[kind]:02X}"
        report.append(
            f"{PREFIX}PCI_CAPABILITY_ENTRY=ID=0x{identifier:02X};OFFSET=0x{offset:02X};"
            f"NEXT=0x{next_pointer:02X};KIND={kind};HEADER_LEN={header}"
        )
        summary_field = KIND_TO_SUMMARY.get(kind)
        if summary_field is not None and summary[summary_field] is None:
            summary[summary_field] = offset
    report.extend(
        (
            f"{PREFIX}PCI_CAPABILITY_SUMMARY="
            + ";".join(
                f"{field}={'NONE' if summary[field] is None else f'0x{summary[field]:02X}'}"
                for field in SUMMARY_FIELDS
            ),
            f"{PREFIX}PCI_INTERRUPT_METADATA=LINE=0x0B;PIN=0x01",
            f"{PREFIX}PCI_CONFIG_READ_ONLY",
            f"{PREFIX}FRAMEBUFFER_CAPABILITY_READY",
            SUCCESS_MARKER,
        )
    )
    return "\n".join(report)


def synthetic_malformed_diagnostic(marker: str = "INVALID_POINTER") -> str:
    return "\n".join(
        (
            f"{PREFIX}PCI_CAPABILITY_MALFORMED={marker}",
            f"{PREFIX}PCI_CONFIG_READ_ONLY",
            f"{PREFIX}FRAMEBUFFER_CAPABILITY_DIAGNOSTIC_READY",
        )
    )


class NetworkHardwareCapabilityProbeSelfTest(unittest.TestCase):
    def test_runner_is_one_backend_free_explicit_device(self) -> None:
        for device in NETWORK_IDENTITIES:
            with self.subTest(device=device):
                assert_runner_isolated(device)
                command = probe_runner_command(device)
                self.assertIn("--no-audio-device", command)
                self.assertIn("--no-virtio-blk", command)
                self.assertNotIn("--virtio-net", command)

    def test_runner_rejects_legacy_net_backends(self) -> None:
        for backend in ("user", "tap,id=forbidden"):
            with self.subTest(backend=backend), self.assertRaises(AssertionError):
                assert_runner_args_isolated(
                    ["-nic", "none", "-device", "e1000", "-net", backend], "e1000"
                )

    def test_static_source_safety(self) -> None:
        assert_static_source_safety()
        assert_source_text_is_safe("PCI_CONFIG_READ_ONLY")
        for fragment in (
            "bar_read",
            "register_read",
            "interrupt_ack",
            "network_port",
            "virtio_transport",
            "wi-fi",
            "frame_access",
        ):
            with self.subTest(fragment=fragment), self.assertRaises(AssertionError):
                assert_source_text_is_safe(f"fn {fragment}() {{}}")

    def test_absent_list_is_a_successful_bounded_observation(self) -> None:
        assert_capability_probe_report(synthetic_serial_report("e1000", list_present=False), "e1000")

    def test_status_clear_reports_raw_pointer_without_traversing_it(self) -> None:
        assert_capability_probe_report(
            synthetic_serial_report(
                "e1000", list_present=False, capability_pointer=0xA5
            ),
            "e1000",
        )

    def test_recognized_and_unknown_entries_follow_the_production_grammar(self) -> None:
        entries = [
            (0x01, 0x40, "POWER_MANAGEMENT"),
            (0x10, 0x48, "PCIE"),
            (0x05, 0x5C, "MSI"),
            (0x11, 0x68, "MSIX"),
            (0x7F, 0x74, "UNKNOWN"),
        ]
        assert_capability_probe_report(synthetic_serial_report("e1000e", entries), "e1000e")

    def test_rejects_identifier_kind_mismatches(self) -> None:
        mismatches = (
            (0x01, "PCIE"),
            (0x10, "MSI"),
            (0x05, "MSIX"),
            (0x11, "POWER_MANAGEMENT"),
            (0x7F, "MSI"),
            (0x01, "UNKNOWN"),
        )
        for identifier, kind in mismatches:
            with self.subTest(identifier=identifier, kind=kind), self.assertRaises(
                AssertionError
            ):
                assert_capability_probe_report(
                    synthetic_serial_report(
                        "e1000e", [(identifier, 0x40, kind)]
                    ),
                    "e1000e",
                )

    def test_rejects_malformed_pointer_and_repeated_offset(self) -> None:
        valid = synthetic_serial_report("e1000")
        malformed_pointer = valid.replace("NEXT=0x48", "NEXT=0x42", 1)
        with self.assertRaises(AssertionError):
            assert_capability_probe_report(malformed_pointer, "e1000")
        repeated_offset = valid.replace("NEXT=0x48", "NEXT=0x40", 1).replace(
            "OFFSET=0x48", "OFFSET=0x40", 1
        )
        with self.assertRaisesRegex(AssertionError, "duplicate capability offset"):
            assert_capability_probe_report(repeated_offset, "e1000")

    def test_rejects_summary_before_list_state(self) -> None:
        valid = synthetic_serial_report("e1000", list_present=False)
        summary = f"{PREFIX}PCI_CAPABILITY_SUMMARY=PM=NONE;PCIE=NONE;MSI=NONE;MSIX=NONE"
        list_state = f"{PREFIX}PCI_CAPABILITY_LIST_ABSENT"
        wrong_order = valid.replace(f"{list_state}\n{summary}", f"{summary}\n{list_state}")
        with self.assertRaisesRegex(AssertionError, "between status and summary"):
            assert_capability_probe_report(wrong_order, "e1000")

    def test_accepts_48_entries_and_rejects_a_49th(self) -> None:
        entries = [(0x7F, 0x40 + index * 4, "UNKNOWN") for index in range(48)]
        valid = synthetic_serial_report("e1000", entries)
        assert_capability_probe_report(valid, "e1000")
        overflow = valid.replace(
            f"{PREFIX}PCI_CAPABILITY_SUMMARY=",
            f"{PREFIX}PCI_CAPABILITY_ENTRY=ID=0x7F;OFFSET=0x40;NEXT=0x00;KIND=UNKNOWN;HEADER_LEN=NONE\n"
            f"{PREFIX}PCI_CAPABILITY_SUMMARY=",
        )
        with self.assertRaises(AssertionError):
            assert_capability_probe_report(overflow, "e1000")

    def test_rejects_recognized_header_crossing_configuration_space(self) -> None:
        serial = synthetic_serial_report("e1000", [(0x10, 0xFC, "PCIE")])
        with self.assertRaises(AssertionError):
            assert_capability_probe_report(serial, "e1000")

    def test_rejects_summary_mismatch_and_forbidden_evidence(self) -> None:
        valid = synthetic_serial_report("e1000")
        mismatch = valid.replace("PM=0x40", "PM=0x48")
        with self.assertRaises(AssertionError):
            assert_capability_probe_report(mismatch, "e1000")
        with self.assertRaises(AssertionError):
            assert_capability_probe_report(valid + f"\n{PREFIX}PCI_CONFIG_WRITE", "e1000")

    def test_malformed_diagnostics_cannot_emit_final_ready(self) -> None:
        diagnostic = synthetic_malformed_diagnostic()
        assert_malformed_diagnostic(diagnostic)
        with self.assertRaises(AssertionError):
            assert_malformed_diagnostic(diagnostic + f"\n{SUCCESS_MARKER}")
        with self.assertRaises(AssertionError):
            assert_capability_probe_report(diagnostic, "e1000")

    def test_qemu_outcome_requires_exactly_one_success(self) -> None:
        assert_qemu_success("QEMU_OUTCOME success")
        with self.assertRaises(AssertionError):
            assert_qemu_success("QEMU_OUTCOME success\nQEMU_OUTCOME success")


def main() -> int:
    assert_static_source_safety()
    build_probe_image()
    for device in NETWORK_IDENTITIES:
        assert_runner_isolated(device)
        serial, output = run_probe_boot(device)
        assert_capability_probe_report(serial, device)
        assert_qemu_success(output)
    print("NETWORK_HARDWARE_CAPABILITY_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        result = unittest.TextTestRunner(verbosity=2).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(NetworkHardwareCapabilityProbeSelfTest)
        )
        raise SystemExit(0 if result.wasSuccessful() else 1)
    if sys.argv[1:]:
        raise SystemExit("usage: test-network-hardware-capability-probe.py [--self-test]")
    raise SystemExit(main())
