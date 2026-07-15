import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
TEST_BOOT = ROOT / "scripts" / "test-boot.py"
EXPECTED_PAGE_FAULT = "PYTHOS:CORE:EXPECTED_PAGE_FAULT"
KERNEL_STACKS_READY = "PYTHOS:CORE:KERNEL_STACKS_READY"


def load_test_boot_module():
    spec = importlib.util.spec_from_file_location("test_boot_script", TEST_BOOT)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class BootMarkerContractTest(unittest.TestCase):
    def test_kernel_stack_slices_assert_guard_page_fault_before_ready(self) -> None:
        test_boot = load_test_boot_module()

        for slice_name in ("kernel-stacks", "milestone-1"):
            markers = test_boot.SLICE_MARKERS[slice_name]
            fault_indexes = [
                index
                for index, marker in enumerate(markers)
                if marker == EXPECTED_PAGE_FAULT
            ]
            ready_index = markers.index(KERNEL_STACKS_READY)

            self.assertGreaterEqual(len(fault_indexes), 2, slice_name)
            self.assertLess(fault_indexes[-1], ready_index, slice_name)


if __name__ == "__main__":
    unittest.main()
