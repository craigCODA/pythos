#!/usr/bin/env python
"""Acceptance test for the dedicated read-only PCI network identity probe."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SUCCESS_MARKER = "PYTHOS:CORE:NETWORK_HARDWARE_PROBE_READY"
QEMU_TIMEOUT_SECONDS = 20

NETWORK_IDENTITIES = {
    "e1000": {
        "vendor": "0x0000000000008086",
        "device": "0x000000000000100E",
        "class": "0x0000000000000002",
        "subclass": "0x0000000000000000",
    },
    "e1000e": {
        "vendor": "0x0000000000008086",
        "device": "0x00000000000010D3",
        "class": "0x0000000000000002",
        "subclass": "0x0000000000000000",
    },
}

FIXED_REQUIRED_MARKERS = (
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:ENTER",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PCI_SCAN_READY",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_COUNT=0x0000000000000001",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_CONTROLLER_FOUND",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_KIND:ETHERNET",
)
IDENTITY_MARKER_PREFIXES = (
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:BUS=",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:DEVICE=",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:FUNCTION=",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:VENDOR=",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBSYSTEM_VENDOR=",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBSYSTEM_DEVICE=",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PROG_IF=",
)
FINAL_REQUIRED_MARKERS = (
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_IDENTITY_READY",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY",
    "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PCI_CONFIG_READ_ONLY",
    SUCCESS_MARKER,
)
FORBIDDEN_MARKER_FRAGMENTS = (
    "PYTHOS:CORE:HARDWARE_PROBE",
    "PYTHOS:CORE:NETWORK_PORT",
    "PYTHOS:CORE:VIRTIO_NET",
    "PYTHOS:CORE:BLOCK:",
    "PYTHOS:CORE:OBJECT_STORE:",
    "PYTHOS:SHELL:RING3_ENTER",
    "NETWORK_PACKET",
    "NETWORK_FRAME",
    "NETWORK_MMIO",
    "NETWORK_DMA",
    "NETWORK_INTERRUPT",
    "NETWORK_BUS_MASTER",
    "NETWORK_RESET",
    "NETWORK_BAR_ACCESS",
    "PCI_CONFIG_WRITE",
)


def load_qemu_runner():
    path = ROOT / "scripts" / "run-qemu.py"
    spec = importlib.util.spec_from_file_location("network_hardware_qemu_runner", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def lines(serial: str) -> list[str]:
    return serial.splitlines()


def assert_ordered_lines(serial: str, markers: tuple[str, ...]) -> None:
    serial_lines = lines(serial)
    previous = -1
    for marker in markers:
        if marker.endswith("="):
            matches = [i for i, line in enumerate(serial_lines) if line.startswith(marker)]
        else:
            matches = [i for i, line in enumerate(serial_lines) if line == marker]
        if len(matches) != 1:
            raise AssertionError(f"expected exactly one {marker!r}, found {len(matches)}")
        if matches[0] <= previous:
            raise AssertionError(f"marker order violation at {marker!r}")
        previous = matches[0]


def required_markers(network_device: str) -> tuple[str, ...]:
    identity = NETWORK_IDENTITIES[network_device]
    return (
        *FIXED_REQUIRED_MARKERS,
        *IDENTITY_MARKER_PREFIXES[:3],
        f"PYTHOS:CORE:NETWORK_HARDWARE_PROBE:VENDOR={identity['vendor']}",
        f"PYTHOS:CORE:NETWORK_HARDWARE_PROBE:DEVICE_ID={identity['device']}",
        *IDENTITY_MARKER_PREFIXES[4:6],
        f"PYTHOS:CORE:NETWORK_HARDWARE_PROBE:CLASS={identity['class']}",
        f"PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBCLASS={identity['subclass']}",
        IDENTITY_MARKER_PREFIXES[6],
        *FINAL_REQUIRED_MARKERS,
    )


def assert_network_identity(serial: str, network_device: str) -> None:
    assert_ordered_lines(serial, required_markers(network_device))


def assert_forbidden_markers_absent(serial: str) -> None:
    for line in lines(serial):
        if any(fragment in line for fragment in FORBIDDEN_MARKER_FRAGMENTS):
            raise AssertionError(f"forbidden network-probe evidence: {line}")


def assert_qemu_success(qemu_output: str) -> None:
    outcomes = [line for line in lines(qemu_output) if line.startswith("QEMU_OUTCOME ")]
    if outcomes != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcomes!r}")


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
            "network-hardware-probe",
        ]
    )
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-image.py"])


def probe_runner_command(network_device: str) -> list[str]:
    if network_device not in NETWORK_IDENTITIES:
        raise ValueError(f"unsupported network device: {network_device}")
    return [
        sys.executable,
        "scripts/run-qemu.py",
        "--serial-log",
        str(TARGET / f"network-hardware-probe-{network_device}-com1.log"),
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


def run_probe_boot(network_device: str) -> tuple[str, str]:
    serial_log = TARGET / f"network-hardware-probe-{network_device}-com1.log"
    if serial_log.exists():
        serial_log.unlink()
    output = run(probe_runner_command(network_device))
    return serial_log.read_text(encoding="utf-8", errors="replace"), output


class NetworkHardwareProbeSelfTest(unittest.TestCase):
    def test_qemu_runner_selects_one_explicit_identity_device(self) -> None:
        runner = load_qemu_runner()
        for device in NETWORK_IDENTITIES:
            with self.subTest(device=device):
                self.assertEqual(
                    runner.network_device_qemu_args(device),
                    ["-nic", "none", "-device", f"{device},id=pythos_network_identity"],
                )

    def test_qemu_runner_rejects_unlisted_device(self) -> None:
        runner = load_qemu_runner()
        with self.assertRaises(ValueError):
            runner.network_device_qemu_args("rtl8139")

    def test_identity_oracle_accepts_both_models(self) -> None:
        for device in NETWORK_IDENTITIES:
            with self.subTest(device=device):
                serial = "\n".join(
                    marker if not marker.endswith("=") else f"{marker}0x0000000000000000"
                    for marker in required_markers(device)
                )
                assert_network_identity(serial, device)

    def test_identity_oracle_rejects_wrong_device(self) -> None:
        serial = "\n".join(
            marker if not marker.endswith("=") else f"{marker}0x0000000000000000"
            for marker in required_markers("e1000")
        ).replace("DEVICE_ID=0x000000000000100E", "DEVICE_ID=0x00000000000010D3")
        with self.assertRaises(AssertionError):
            assert_network_identity(serial, "e1000")

    def test_probe_command_has_no_peer_or_storage_device(self) -> None:
        command = probe_runner_command("e1000e")
        self.assertIn("--network-device", command)
        self.assertIn("--no-virtio-blk", command)
        self.assertNotIn("--virtio-net", command)
        self.assertNotIn("socket", " ".join(command))

    def test_forbidden_control_markers_are_rejected(self) -> None:
        valid = "\n".join(
            marker if not marker.endswith("=") else f"{marker}0x0000000000000000"
            for marker in required_markers("e1000")
        )
        for marker in FORBIDDEN_MARKER_FRAGMENTS:
            with self.subTest(marker=marker), self.assertRaises(AssertionError):
                assert_forbidden_markers_absent(valid + "\n" + marker)


def main() -> int:
    build_probe_image()
    for device in NETWORK_IDENTITIES:
        serial, output = run_probe_boot(device)
        assert_network_identity(serial, device)
        assert_forbidden_markers_absent(serial)
        assert_qemu_success(output)
    print("NETWORK_HARDWARE_PROBE_TEST_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        result = unittest.TextTestRunner(verbosity=2).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(NetworkHardwareProbeSelfTest)
        )
        raise SystemExit(0 if result.wasSuccessful() else 1)
    if sys.argv[1:]:
        raise SystemExit("usage: test-network-hardware-probe.py [--self-test]")
    raise SystemExit(main())
