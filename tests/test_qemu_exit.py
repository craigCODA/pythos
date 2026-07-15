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


class QemuExitTest(unittest.TestCase):
    def test_debug_exit_status_decodes_to_success(self) -> None:
        run_qemu = load_run_qemu_module()

        self.assertEqual(
            run_qemu.classify_qemu_exit(run_qemu.debug_exit_status("success"), ""),
            run_qemu.QemuOutcome.SUCCESS,
        )

    def test_panic_marker_overrides_timeout_classification(self) -> None:
        run_qemu = load_run_qemu_module()

        self.assertEqual(
            run_qemu.classify_qemu_exit(None, "PYTHOS:PANIC\n"),
            run_qemu.QemuOutcome.PANIC,
        )


if __name__ == "__main__":
    unittest.main()
