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


if __name__ == "__main__":
    unittest.main()
