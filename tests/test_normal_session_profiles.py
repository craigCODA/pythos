"""Compile actual feature selections; policy markers alone cannot prove gating."""
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]


class NormalSessionProfilesTest(unittest.TestCase):
    def check_profile(self, features: str):
        return subprocess.run(
            ["cargo", "check", "-p", "pythos-core", "--bin", "pythcore", "--target", "x86_64-unknown-none", "--features", features],
            cwd=ROOT, capture_output=True, text=True, timeout=90,
        )

    def test_normal_default_rejects_conflicting_program_and_polling_profiles(self):
        for features in ("legacy-shell", "pyth-tig-default", "physical-keyboard-console", "pythtig-phase2-test", "pyth-tig-session-manager-fault-test"):
            with self.subTest(features=features):
                result = self.check_profile(features)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("production `normal-session` conflicts", result.stderr)

    def test_earlier_hardware_dispatch_still_compiles_with_default_inheritance(self):
        result = self.check_profile("hardware-probe")
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
