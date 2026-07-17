import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
TEST_BOOT = ROOT / "scripts" / "test-boot.py"
EXPECTED_PAGE_FAULT = "PYTHOS:CORE:EXPECTED_PAGE_FAULT"
KERNEL_STACKS_READY = "PYTHOS:CORE:KERNEL_STACKS_READY"
CAPABILITY_BOUNDARY_READY = "PYTHOS:CORE:CAPABILITY_BOUNDARY_READY"
FRAMEBUFFER_READY = "PYTHOS:CORE:FRAMEBUFFER_READY"
DYNAMIC_ELF_READY = "PYTHOS:CORE:DYNAMIC_ELF_LOADING_READY"
GENERAL_SYSCALL_ABI_READY = "PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY"
COPY_IN_COPY_OUT_READY = "PYTHOS:CORE:COPY_IN_COPY_OUT_READY"


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

    def test_dynamic_elf_loading_extends_capability_boundary_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        dynamic_markers = test_boot.SLICE_MARKERS["dynamic-elf-loading"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            dynamic_markers.index(CAPABILITY_BOUNDARY_READY),
            dynamic_markers.index(DYNAMIC_ELF_READY),
        )
        self.assertLess(
            milestone_markers.index(DYNAMIC_ELF_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_general_syscall_abi_extends_dynamic_elf_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        general_markers = test_boot.SLICE_MARKERS["general-syscall-abi"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            general_markers.index(DYNAMIC_ELF_READY),
            general_markers.index(GENERAL_SYSCALL_ABI_READY),
        )
        self.assertLess(
            milestone_markers.index(GENERAL_SYSCALL_ABI_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_copy_in_copy_out_extends_general_syscall_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        copy_markers = test_boot.SLICE_MARKERS["copy-in-copy-out-policy"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            copy_markers.index(GENERAL_SYSCALL_ABI_READY),
            copy_markers.index(COPY_IN_COPY_OUT_READY),
        )
        self.assertLess(
            milestone_markers.index(COPY_IN_COPY_OUT_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )


if __name__ == "__main__":
    unittest.main()
