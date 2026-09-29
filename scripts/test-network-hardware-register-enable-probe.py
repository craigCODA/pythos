#!/usr/bin/env python
"""QEMU acceptance oracle for the bounded PCI Memory Space Enable experiment."""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
PREFIX = "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:"
SUCCESS_MARKER = "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE_READY"
QEMU_TIMEOUT_SECONDS = 20
NETWORK_IDENTITIES = {
    "e1000": ("0x0000000000008086", "0x000000000000100E"),
    "e1000e": ("0x0000000000008086", "0x00000000000010D3"),
}
HEX64 = re.compile(r"0x[0-9A-F]{16}\Z")
FORBIDDEN_MARKER_FRAGMENTS = (
    "CONFIGURATION_WRITE",
    "MMIO_WRITE",
    "BUS_MASTER",
    "DMA",
    "INTERRUPT",
    "MSI",
    "RESET",
    "QUEUE",
    "PACKET",
    "NETWORK_FRAME",
    "FRAME_MOVEMENT",
    "SOCKET",
    "NETWORK_PORT",
    "WIFI_ASSOCIATED",
)


def load_qemu_runner():
    path = ROOT / "scripts" / "run-qemu.py"
    spec = importlib.util.spec_from_file_location("network_hardware_register_enable_qemu_runner", path)
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


def require_hex64(serial_lines: list[str], marker: str) -> tuple[int, int]:
    matches = [
        (index, line.removeprefix(marker))
        for index, line in enumerate(serial_lines)
        if line.startswith(marker)
    ]
    if len(matches) != 1 or HEX64.fullmatch(matches[0][1]) is None:
        raise AssertionError(f"expected one 64-bit marker {marker!r}, got {matches!r}")
    return matches[0][0], int(matches[0][1], 16)


def assert_profile_sources_are_bounded() -> None:
    boot = (ROOT / "core" / "src" / "network_hardware_register_enable_probe_boot.rs").read_text(
        encoding="utf-8"
    )
    probe = (ROOT / "core" / "src" / "network_hardware_probe.rs").read_text(encoding="utf-8")
    if "write_controller_command_word" not in probe or "write_config_u16" not in probe:
        raise AssertionError("missing narrow PCI command-word write boundary")
    if "outw(PCI_CONFIG_DATA + u16::from(offset & 0x02), value)" not in probe:
        raise AssertionError("PCI command write is not a 16-bit command-lane access")
    if "write_config_u32" in probe or "write_volatile" in boot:
        raise AssertionError("enable profile contains a forbidden broad/MMIO write path")
    for forbidden in (
        "enable_bus_master",
        "dma",
        "interrupt",
        "queue",
        "packet",
        "NetworkPort",
        "socket",
    ):
        if forbidden.lower() in boot.lower():
            raise AssertionError(f"enable profile source contains forbidden operation: {forbidden}")


def assert_register_enable_report(serial: str, network_device: str) -> None:
    if network_device not in NETWORK_IDENTITIES:
        raise ValueError(f"unsupported network device: {network_device}")
    serial_lines = lines(serial)
    for line in serial_lines:
        if any(fragment in line.upper() for fragment in FORBIDDEN_MARKER_FRAGMENTS):
            raise AssertionError(f"forbidden enable-probe evidence: {line}")

    vendor, device = NETWORK_IDENTITIES[network_device]
    prefix = [
        f"{PREFIX}ENTER",
        f"{PREFIX}PCI_SCAN_READY",
        f"{PREFIX}NETWORK_CONTROLLER_FOUND",
        f"{PREFIX}NETWORK_VENDOR={vendor}",
        f"{PREFIX}NETWORK_DEVICE_ID={device}",
        f"{PREFIX}PCI_COMMAND_STATUS_ORIGINAL=",
    ]
    positions = [
        require_exactly_one(serial_lines, marker) if not marker.endswith("=") else require_hex64(serial_lines, marker)[0]
        for marker in prefix
    ]
    if positions != sorted(positions):
        raise AssertionError("enable-probe prefix marker order violation")

    original_index, original = require_hex64(
        serial_lines, f"{PREFIX}PCI_COMMAND_STATUS_ORIGINAL="
    )
    if original & 0x2:
        suffix = [
            f"{PREFIX}PCI_MEMORY_SPACE_ALREADY_ENABLED",
            f"{PREFIX}PCI_CONFIG_WRITE_NOT_NEEDED",
            f"{PREFIX}MMIO_MAPPED",
            f"{PREFIX}REGISTER_READ_VALUE=",
            f"{PREFIX}FRAMEBUFFER_REGISTER_READY",
            f"{PREFIX}REGISTER_REACHABILITY_READY",
            f"{PREFIX}PCI_CONFIG_READ_WRITE_BOUNDARY",
            SUCCESS_MARKER,
        ]
        suffix_positions = [
            require_hex64(serial_lines, marker)[0] if marker.endswith("=") else require_exactly_one(serial_lines, marker)
            for marker in suffix
        ]
        if suffix_positions != sorted(suffix_positions) or suffix_positions[0] <= original_index:
            raise AssertionError("already-enabled marker order violation")
    else:
        suffix = [
            f"{PREFIX}PCI_MEMORY_SPACE_DISABLED",
            f"{PREFIX}PCI_COMMAND_MSE_WRITE",
            f"{PREFIX}PCI_COMMAND_STATUS_AFTER_ENABLE=",
            f"{PREFIX}PCI_MEMORY_SPACE_ENABLED",
            f"{PREFIX}MMIO_MAPPED",
            f"{PREFIX}REGISTER_READ_VALUE=",
            f"{PREFIX}PCI_COMMAND_MSE_RESTORE",
            f"{PREFIX}PCI_COMMAND_STATUS_RESTORED=",
            f"{PREFIX}PCI_CONFIG_WRITE_SCOPED",
            f"{PREFIX}FRAMEBUFFER_REGISTER_READY",
            f"{PREFIX}REGISTER_REACHABILITY_READY",
            f"{PREFIX}PCI_CONFIG_READ_WRITE_BOUNDARY",
            SUCCESS_MARKER,
        ]
        suffix_positions = [
            require_hex64(serial_lines, marker)[0] if marker.endswith("=") else require_exactly_one(serial_lines, marker)
            for marker in suffix
        ]
        if suffix_positions != sorted(suffix_positions) or suffix_positions[0] <= original_index:
            raise AssertionError("write/restore marker order violation")
        _, after_enable = require_hex64(serial_lines, f"{PREFIX}PCI_COMMAND_STATUS_AFTER_ENABLE=")
        _, restored = require_hex64(serial_lines, f"{PREFIX}PCI_COMMAND_STATUS_RESTORED=")
        if after_enable & 0x2 == 0 or after_enable & 0x4:
            raise AssertionError(f"enable readback did not prove only MSE: 0x{after_enable:x}")
        if restored & 0xFFFF != original & 0xFFFF:
            raise AssertionError(f"command restoration mismatch: 0x{restored:x} vs 0x{original:x}")


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
    if nic_pairs != [["-nic", "none"]] or explicit_devices != [network_device]:
        raise AssertionError(f"runner did not isolate {network_device}: {runner_args!r}")
    if any("netdev" in value or "socket" in value for value in runner_args):
        raise AssertionError(f"runner included a network backend: {runner_args!r}")


def probe_runner_command(network_device: str) -> list[str]:
    if network_device not in NETWORK_IDENTITIES:
        raise ValueError(f"unsupported network device: {network_device}")
    return [
        sys.executable,
        "scripts/run-qemu.py",
        "--serial-log",
        str(TARGET / f"network-hardware-register-enable-probe-{network_device}-com1.log"),
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
            "network-hardware-register-enable-probe",
        ]
    )
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-image.py"])


def run_probe_boot(network_device: str) -> tuple[str, str]:
    serial_log = TARGET / f"network-hardware-register-enable-probe-{network_device}-com1.log"
    if serial_log.exists():
        serial_log.unlink()
    output = run(probe_runner_command(network_device))
    return serial_log.read_text(encoding="utf-8", errors="replace"), output


def synthetic_serial_report(network_device: str, original: int = 0x0010_0000) -> str:
    vendor, device = NETWORK_IDENTITIES[network_device]
    common = [
        f"{PREFIX}ENTER",
        f"{PREFIX}PCI_SCAN_READY",
        f"{PREFIX}NETWORK_CONTROLLER_FOUND",
        f"{PREFIX}NETWORK_VENDOR={vendor}",
        f"{PREFIX}NETWORK_DEVICE_ID={device}",
        f"{PREFIX}PCI_COMMAND_STATUS_ORIGINAL=0x{original:016X}",
    ]
    if original & 0x2:
        common.extend(
            (
                f"{PREFIX}PCI_MEMORY_SPACE_ALREADY_ENABLED",
                f"{PREFIX}PCI_CONFIG_WRITE_NOT_NEEDED",
                f"{PREFIX}MMIO_MAPPED",
                f"{PREFIX}REGISTER_READ_VALUE=0x0000000080000000",
                f"{PREFIX}FRAMEBUFFER_REGISTER_READY",
                f"{PREFIX}REGISTER_REACHABILITY_READY",
                f"{PREFIX}PCI_CONFIG_READ_WRITE_BOUNDARY",
                SUCCESS_MARKER,
            )
        )
    else:
        common.extend(
            (
                f"{PREFIX}PCI_MEMORY_SPACE_DISABLED",
                f"{PREFIX}PCI_COMMAND_MSE_WRITE",
                f"{PREFIX}PCI_COMMAND_STATUS_AFTER_ENABLE=0x{original | 0x2:016X}",
                f"{PREFIX}PCI_MEMORY_SPACE_ENABLED",
                f"{PREFIX}MMIO_MAPPED",
                f"{PREFIX}REGISTER_READ_VALUE=0x0000000080000000",
                f"{PREFIX}PCI_COMMAND_MSE_RESTORE",
                f"{PREFIX}PCI_COMMAND_STATUS_RESTORED=0x{original:016X}",
                f"{PREFIX}PCI_CONFIG_WRITE_SCOPED",
                f"{PREFIX}FRAMEBUFFER_REGISTER_READY",
                f"{PREFIX}REGISTER_REACHABILITY_READY",
                f"{PREFIX}PCI_CONFIG_READ_WRITE_BOUNDARY",
                SUCCESS_MARKER,
            )
        )
    return "\n".join(common)


class NetworkHardwareRegisterEnableProbeSelfTest(unittest.TestCase):
    def test_source_boundary_is_narrow(self) -> None:
        assert_profile_sources_are_bounded()

    def test_runner_is_one_backend_free_explicit_device(self) -> None:
        for device in NETWORK_IDENTITIES:
            with self.subTest(device=device):
                assert_runner_isolated(device)
                command = probe_runner_command(device)
                self.assertIn("--no-virtio-blk", command)
                self.assertNotIn("--virtio-net", command)

    def test_parser_accepts_already_enabled_qemu_transcript(self) -> None:
        for device in NETWORK_IDENTITIES:
            with self.subTest(device=device):
                assert_register_enable_report(synthetic_serial_report(device, 0x7), device)

    def test_parser_accepts_disabled_write_restore_transcript(self) -> None:
        assert_register_enable_report(synthetic_serial_report("e1000"), "e1000")

    def test_parser_rejects_missing_restore(self) -> None:
        valid = synthetic_serial_report("e1000")
        missing = valid.replace(
            f"{PREFIX}PCI_COMMAND_STATUS_RESTORED=0x0000000000100000\n", ""
        )
        with self.assertRaises(AssertionError):
            assert_register_enable_report(missing, "e1000")


def main() -> int:
    build_probe_image()
    for device in NETWORK_IDENTITIES:
        serial, output = run_probe_boot(device)
        assert_register_enable_report(serial, device)
        outcomes = [line for line in lines(output) if line.startswith("QEMU_OUTCOME ")]
        if outcomes != ["QEMU_OUTCOME success"]:
            raise AssertionError(f"expected one exact success outcome line, got {outcomes!r}")
    print("NETWORK_HARDWARE_REGISTER_ENABLE_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        unittest.main(argv=[sys.argv[0]])
    raise SystemExit(main())
