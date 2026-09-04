from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_run_qemu_module():
    path = ROOT / "scripts" / "run-qemu.py"
    spec = importlib.util.spec_from_file_location("run_qemu", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load run-qemu.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class QemuMarkerActionTest(unittest.TestCase):
    def test_marker_delay_waits_until_the_deadline(self) -> None:
        run_qemu = load_run_qemu_module()

        ready, seen_at = run_qemu.marker_delay_ready(None, True, 2.0, 10.0)
        self.assertFalse(ready)
        self.assertEqual(seen_at, 10.0)

        ready, seen_at = run_qemu.marker_delay_ready(seen_at, True, 2.0, 11.9)
        self.assertFalse(ready)
        self.assertEqual(seen_at, 10.0)

        ready, seen_at = run_qemu.marker_delay_ready(seen_at, True, 2.0, 12.0)
        self.assertTrue(ready)
        self.assertEqual(seen_at, 10.0)


if __name__ == "__main__":
    unittest.main()
