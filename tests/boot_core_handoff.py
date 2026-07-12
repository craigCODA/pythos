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


if __name__ == "__main__":
    unittest.main()
