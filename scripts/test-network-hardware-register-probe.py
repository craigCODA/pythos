#!/usr/bin/env python
"""QEMU acceptance oracle for the bounded read-only network register probe."""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
PREFIX = "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:"
SUCCESS_MARKER = "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE_READY"
QEMU_TIMEOUT_SECONDS = 20
NETWORK_IDENTITIES = {
    "e1000": ("0x0000000000008086", "0x000000000000100E"),
    "e1000e": ("0x0000000000008086", "0x00000000000010D3"),
}
HEX64 = re.compile(r"0x[0-9A-F]{16}\Z")
FORBIDDEN_MARKER_FRAGMENTS = (
    "PCI_CONFIG_WRITE",
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
    spec = importlib.util.spec_from_file_location("network_hardware_register_qemu_runner", path)
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


def assert_register_probe_report(serial: str, network_device: str) -> None:
    if network_device not in NETWORK_IDENTITIES:
        raise ValueError(f"unsupported network device: {network_device}")
    serial_lines = lines(serial)
    for line in serial_lines:
        if any(fragment in line.upper() for fragment in FORBIDDEN_MARKER_FRAGMENTS):
            raise AssertionError(f"forbidden register-probe evidence: {line}")

    vendor, device = NETWORK_IDENTITIES[network_device]
    ordered = [
        f"{PREFIX}ENTER",
        f"{PREFIX}PCI_SCAN_READY",
        f"{PREFIX}NETWORK_CONTROLLER_FOUND",
        f"{PREFIX}NETWORK_VENDOR={vendor}",
        f"{PREFIX}NETWORK_DEVICE_ID={device}",
        f"{PREFIX}PCI_COMMAND_STATUS=",
    ]
    positions = [
        require_exactly_one(serial_lines, marker) if not marker.endswith("=") else require_hex64(serial_lines, marker)[0]
        for marker in ordered
    ]
    if positions != sorted(positions):
        raise AssertionError("register-probe prefix marker order violation")

    command_index, command_status = require_hex64(
        serial_lines, f"{PREFIX}PCI_COMMAND_STATUS="
    )
    if command_status & 0x2:
        suffix = [
            f"{PREFIX}PCI_MEMORY_SPACE_ENABLED",
            f"{PREFIX}BAR_TARGET_SELECTED",
            f"{PREFIX}TARGET_INTEL_DEVICE_STATUS",
            f"{PREFIX}REGISTER_OFFSET=",
            f"{PREFIX}MMIO_MAPPED",
            f"{PREFIX}REGISTER_READ_VALUE=",
            f"{PREFIX}REGISTER_REACHABILITY_READY",
            f"{PREFIX}PCI_CONFIG_READ_ONLY",
            SUCCESS_MARKER,
        ]
        suffix_positions = []
        for marker in suffix:
            if marker.endswith("="):
                suffix_positions.append(require_hex64(serial_lines, marker)[0])
            else:
                suffix_positions.append(require_exactly_one(serial_lines, marker))
        if suffix_positions != sorted(suffix_positions) or suffix_positions[0] <= command_index:
            raise AssertionError("register-probe read marker order violation")
        _, offset = require_hex64(serial_lines, f"{PREFIX}REGISTER_OFFSET=")
        if offset != 0x08:
            raise AssertionError(f"unexpected Intel status offset: 0x{offset:x}")
    else:
        suffix = [
            f"{PREFIX}PCI_MEMORY_SPACE_DISABLED",
            f"{PREFIX}REGISTER_REACHABILITY_SKIPPED",
            f"{PREFIX}PCI_CONFIG_READ_ONLY",
            SUCCESS_MARKER,
        ]
        suffix_positions = [require_exactly_one(serial_lines, marker) for marker in suffix]
        if suffix_positions != sorted(suffix_positions) or suffix_positions[0] <= command_index:
            raise AssertionError("register-probe safe-skip marker order violation")


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
        str(TARGET / f"network-hardware-register-probe-{network_device}-com1.log"),
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
            "network-hardware-register-probe",
        ]
    )
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-image.py"])


def run_probe_boot(network_device: str) -> tuple[str, str]:
    serial_log = TARGET / f"network-hardware-register-probe-{network_device}-com1.log"
    if serial_log.exists():
        serial_log.unlink()
    output = run(probe_runner_command(network_device))
    return serial_log.read_text(encoding="utf-8", errors="replace"), output


def synthetic_serial_report(network_device: str, command_status: int = 0x2) -> str:
    vendor, device = NETWORK_IDENTITIES[network_device]
    common = [
        f"{PREFIX}ENTER",
        f"{PREFIX}PCI_SCAN_READY",
        f"{PREFIX}NETWORK_CONTROLLER_FOUND",
        f"{PREFIX}NETWORK_VENDOR={vendor}",
        f"{PREFIX}NETWORK_DEVICE_ID={device}",
        f"{PREFIX}PCI_COMMAND_STATUS=0x{command_status:016X}",
    ]
    if command_status & 0x2:
        common.extend(
            (
                f"{PREFIX}PCI_MEMORY_SPACE_ENABLED",
                f"{PREFIX}BAR_SLOT=0x0000000000000000",
                f"{PREFIX}BAR_TARGET_SELECTED",
                f"{PREFIX}TARGET_INTEL_DEVICE_STATUS",
                f"{PREFIX}REGISTER_OFFSET=0x0000000000000008",
                f"{PREFIX}MMIO_MAPPED",
                f"{PREFIX}REGISTER_READ_VALUE=0x0000000080000000",
                f"{PREFIX}REGISTER_REACHABILITY_READY",
            )
        )
    else:
        common.append(f"{PREFIX}PCI_MEMORY_SPACE_DISABLED")
        common.append(f"{PREFIX}REGISTER_REACHABILITY_SKIPPED")
    common.extend((f"{PREFIX}PCI_CONFIG_READ_ONLY", SUCCESS_MARKER))
    return "\n".join(common)


class NetworkHardwareRegisterProbeSelfTest(unittest.TestCase):
    def test_runner_is_one_backend_free_explicit_device(self) -> None:
        for device in NETWORK_IDENTITIES:
            with self.subTest(device=device):
                assert_runner_isolated(device)
                command = probe_runner_command(device)
                self.assertIn("--no-virtio-blk", command)
                self.assertNotIn("--virtio-net", command)

    def test_parser_accepts_qemu_read_transcripts(self) -> None:
        for device in NETWORK_IDENTITIES:
            with self.subTest(device=device):
                assert_register_probe_report(synthetic_serial_report(device), device)

    def test_parser_accepts_memory_disabled_safe_skip(self) -> None:
        assert_register_probe_report(synthetic_serial_report("e1000", 0), "e1000")

    def test_parser_rejects_wrong_offset_or_marker_order(self) -> None:
        valid = synthetic_serial_report("e1000")
        wrong_offset = valid.replace(
            "REGISTER_OFFSET=0x0000000000000008",
            "REGISTER_OFFSET=0x00000000000000F4",
        )
        with self.assertRaises(AssertionError):
            assert_register_probe_report(wrong_offset, "e1000")
        reordered = valid.replace(
            f"{PREFIX}MMIO_MAPPED\n{PREFIX}REGISTER_READ_VALUE=",
            f"{PREFIX}REGISTER_READ_VALUE=",
        )
        with self.assertRaises(AssertionError):
            assert_register_probe_report(reordered, "e1000")

    def test_parser_rejects_forbidden_operations_and_missing_ready(self) -> None:
        valid = synthetic_serial_report("e1000")
        for fragment in FORBIDDEN_MARKER_FRAGMENTS:
            with self.subTest(fragment=fragment), self.assertRaises(AssertionError):
                assert_register_probe_report(valid + f"\n{PREFIX}{fragment}", "e1000")
        with self.assertRaises(AssertionError):
            assert_register_probe_report(valid.replace(SUCCESS_MARKER, ""), "e1000")

    def test_parser_rejects_wrong_model_identity(self) -> None:
        wrong = synthetic_serial_report("e1000").replace(
            "NETWORK_DEVICE_ID=0x000000000000100E",
            "NETWORK_DEVICE_ID=0x00000000000010D3",
        )
        with self.assertRaises(AssertionError):
            assert_register_probe_report(wrong, "e1000")


def main() -> int:
    build_probe_image()
    for device in NETWORK_IDENTITIES:
        assert_runner_isolated(device)
        serial, output = run_probe_boot(device)
        assert_register_probe_report(serial, device)
        assert_qemu_success(output)
    print("NETWORK_HARDWARE_REGISTER_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        result = unittest.TextTestRunner(verbosity=2).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(NetworkHardwareRegisterProbeSelfTest)
        )
        raise SystemExit(0 if result.wasSuccessful() else 1)
    if sys.argv[1:]:
        raise SystemExit("usage: test-network-hardware-register-probe.py [--self-test]")
    raise SystemExit(main())
