from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class PersistentObjectStorageHarnessTest(unittest.TestCase):
    def test_reboot_and_torn_write_harness_is_present(self) -> None:
        script = ROOT / "scripts" / "test-persistent-storage.py"

        self.assertTrue(script.exists(), "missing persistent storage acceptance harness")
        source = script.read_text(encoding="utf-8")

        self.assertIn("--storage-image", source)
        self.assertIn("PYTHOS:CORE:OBJECT_STORE:RESTORED", source)
        self.assertIn("PYTHOS:CORE:OBJECT_STORE:TORN_WRITE_RECOVERED", source)
        self.assertIn("kill", source.lower())


if __name__ == "__main__":
    unittest.main()
