from __future__ import annotations

import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class BootCoreHandoffTest(unittest.TestCase):
    def test_loader_enter_marker_is_observed_in_serial_output(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "loader-enter"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_gop_ready_marker_is_observed_after_loader_enter(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "gop-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_kernel_loaded_marker_is_observed_after_gop_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "kernel-loaded"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_memory_map_ready_marker_is_observed_after_kernel_loaded(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "memory-map-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_exit_boot_services_marker_is_observed_after_memory_map_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "exit-boot-services-ok"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_core_enter_marker_is_observed_after_exit_boot_services(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "core-enter"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_bootinfo_valid_marker_is_observed_after_core_enter(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "bootinfo-valid"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_framebuffer_ready_marker_is_observed_after_bootinfo_valid(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "framebuffer-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_memory_ready_marker_is_observed_after_bootinfo_valid(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "memory-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_gdt_ready_marker_is_observed_after_memory_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "gdt-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_idt_ready_marker_is_observed_after_gdt_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "idt-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_vm_ready_marker_is_observed_after_idt_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "vm-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_exceptions_diagnostic_marker_is_observed_after_idt_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "exceptions-diagnostic"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_identity_map_removed_marker_is_observed_after_expected_page_fault(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "identity-map-removed"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_bootinfo_complete_marker_is_observed_after_identity_map_removed(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "bootinfo-complete"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_milestone_1_complete_marker_is_observed_after_framebuffer_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "milestone-1"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_milestone_1_complete_marker_is_observed_when_booting_iso(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                "scripts/test-boot.py",
                "--slice",
                "milestone-1",
                "--media",
                "iso",
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)


if __name__ == "__main__":
    unittest.main()
