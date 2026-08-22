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
DYNAMIC_CAPABILITY_GRANTS_READY = "PYTHOS:CORE:DYNAMIC_CAPABILITY_GRANTS_READY"
PROCESS_ARGV_ENV_READY = "PYTHOS:CORE:PROCESS_ARGV_ENV_READY"
GENERAL_FAULT_ISOLATION_READY = "PYTHOS:CORE:GENERAL_FAULT_ISOLATION_READY"
PROCESS_MODEL_ADVERSARIAL_READY = "PYTHOS:CORE:PROCESS_MODEL_ADVERSARIAL_READY"
PHASE_9_COMPLETE = "PYTHOS:CORE:PHASE_9_COMPLETE"
BLOCK_ALLOCATOR_READY = "PYTHOS:CORE:BLOCK_ALLOCATOR_READY"
DYNAMIC_OBJECT_COUNT_READY = "PYTHOS:CORE:DYNAMIC_OBJECT_COUNT_READY"
FRAGMENTATION_COMPACTION_POLICY_READY = (
    "PYTHOS:CORE:FRAGMENTATION_COMPACTION_POLICY_READY"
)
STORAGE_QUOTA_PER_SERVICE_READY = "PYTHOS:CORE:STORAGE_QUOTA_PER_SERVICE_READY"
CONCURRENT_WRITE_SAFETY_READY = "PYTHOS:CORE:CONCURRENT_WRITE_SAFETY_READY"
STORAGE_ADVERSARIAL_SUITE_READY = "PYTHOS:CORE:STORAGE_ADVERSARIAL_SUITE_READY"
PHASE_10_COMPLETE = "PYTHOS:CORE:PHASE_10_COMPLETE"
OBJECT_LOCATOR_RESOLUTION_READY = "PYTHOS:CORE:OBJECT_LOCATOR_RESOLUTION_READY"
PATH_ADVERSARIAL_SUITE_READY = "PYTHOS:CORE:PATH_ADVERSARIAL_SUITE_READY"
PHASE_12_COMPLETE = "PYTHOS:CORE:PHASE_12_COMPLETE"
PACKAGE_FORMAT_READY = "PYTHOS:CORE:PACKAGE_FORMAT_READY"
PACKAGE_INSTALL_READY = "PYTHOS:CORE:PACKAGE_INSTALL_READY"
PACKAGE_LAUNCH_READY = "PYTHOS:CORE:PACKAGE_LAUNCH_READY"
PACKAGE_UNINSTALL_READY = "PYTHOS:CORE:PACKAGE_UNINSTALL_READY"
INDEPENDENT_PACKAGE_READY = "PYTHOS:CORE:INDEPENDENT_PACKAGE_READY"
PACKAGE_SCHEMA_EXTENSIBILITY_READY = "PYTHOS:CORE:PACKAGE_SCHEMA_EXTENSIBILITY_READY"
PHASE_13_COMPLETE = "PYTHOS:CORE:PHASE_13_COMPLETE"

PHASE13_MARKERS = [
    PACKAGE_FORMAT_READY,
    PACKAGE_INSTALL_READY,
    PACKAGE_LAUNCH_READY,
    PACKAGE_UNINSTALL_READY,
    INDEPENDENT_PACKAGE_READY,
    PACKAGE_SCHEMA_EXTENSIBILITY_READY,
    PHASE_13_COMPLETE,
]


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

    def test_dynamic_capability_grants_extend_copy_policy_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        grant_markers = test_boot.SLICE_MARKERS["dynamic-capability-grants"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            grant_markers.index(COPY_IN_COPY_OUT_READY),
            grant_markers.index(DYNAMIC_CAPABILITY_GRANTS_READY),
        )
        self.assertLess(
            milestone_markers.index(DYNAMIC_CAPABILITY_GRANTS_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_process_argv_environment_extends_dynamic_grants_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        launch_markers = test_boot.SLICE_MARKERS["process-argv-and-environment"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            launch_markers.index(DYNAMIC_CAPABILITY_GRANTS_READY),
            launch_markers.index(PROCESS_ARGV_ENV_READY),
        )
        self.assertLess(
            milestone_markers.index(PROCESS_ARGV_ENV_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_general_fault_isolation_extends_process_argv_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        fault_markers = test_boot.SLICE_MARKERS["general-fault-isolation"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            fault_markers.index(PROCESS_ARGV_ENV_READY),
            fault_markers.index(GENERAL_FAULT_ISOLATION_READY),
        )
        self.assertLess(
            milestone_markers.index(GENERAL_FAULT_ISOLATION_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_process_model_adversarial_suite_completes_phase_9_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        adversarial_markers = test_boot.SLICE_MARKERS["process-model-adversarial-suite"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            adversarial_markers.index(GENERAL_FAULT_ISOLATION_READY),
            adversarial_markers.index(PROCESS_MODEL_ADVERSARIAL_READY),
        )
        self.assertLess(
            adversarial_markers.index(PROCESS_MODEL_ADVERSARIAL_READY),
            adversarial_markers.index(PHASE_9_COMPLETE),
        )
        self.assertLess(
            milestone_markers.index(PHASE_9_COMPLETE),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_block_allocator_extends_phase_9_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        allocator_markers = test_boot.SLICE_MARKERS["block-allocator"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            allocator_markers.index(PHASE_9_COMPLETE),
            allocator_markers.index(BLOCK_ALLOCATOR_READY),
        )
        self.assertLess(
            milestone_markers.index(BLOCK_ALLOCATOR_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_dynamic_object_count_extends_allocator_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        dynamic_markers = test_boot.SLICE_MARKERS["dynamic-object-count"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            dynamic_markers.index(BLOCK_ALLOCATOR_READY),
            dynamic_markers.index(DYNAMIC_OBJECT_COUNT_READY),
        )
        self.assertLess(
            milestone_markers.index(DYNAMIC_OBJECT_COUNT_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_fragmentation_policy_extends_dynamic_objects_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        fragmentation_markers = test_boot.SLICE_MARKERS[
            "fragmentation-and-compaction-policy"
        ]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            fragmentation_markers.index(DYNAMIC_OBJECT_COUNT_READY),
            fragmentation_markers.index(FRAGMENTATION_COMPACTION_POLICY_READY),
        )
        self.assertLess(
            milestone_markers.index(FRAGMENTATION_COMPACTION_POLICY_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_storage_quota_extends_fragmentation_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        quota_markers = test_boot.SLICE_MARKERS["storage-quota-per-service"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            quota_markers.index(FRAGMENTATION_COMPACTION_POLICY_READY),
            quota_markers.index(STORAGE_QUOTA_PER_SERVICE_READY),
        )
        self.assertLess(
            milestone_markers.index(STORAGE_QUOTA_PER_SERVICE_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_concurrent_write_extends_quota_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        concurrent_markers = test_boot.SLICE_MARKERS["concurrent-write-safety"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            concurrent_markers.index(STORAGE_QUOTA_PER_SERVICE_READY),
            concurrent_markers.index(CONCURRENT_WRITE_SAFETY_READY),
        )
        self.assertLess(
            milestone_markers.index(CONCURRENT_WRITE_SAFETY_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_storage_adversarial_suite_completes_phase_10_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        adversarial_markers = test_boot.SLICE_MARKERS["storage-adversarial-suite"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            adversarial_markers.index(CONCURRENT_WRITE_SAFETY_READY),
            adversarial_markers.index(STORAGE_ADVERSARIAL_SUITE_READY),
        )
        self.assertLess(
            adversarial_markers.index(STORAGE_ADVERSARIAL_SUITE_READY),
            adversarial_markers.index(PHASE_10_COMPLETE),
        )
        self.assertLess(
            milestone_markers.index(PHASE_10_COMPLETE),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_path_resolution_extends_phase_10_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        locator_markers = test_boot.SLICE_MARKERS["path-resolution"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            locator_markers.index(PHASE_10_COMPLETE),
            locator_markers.index(OBJECT_LOCATOR_RESOLUTION_READY),
        )
        self.assertLess(
            milestone_markers.index(OBJECT_LOCATOR_RESOLUTION_READY),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_path_adversarial_suite_completes_phase_12_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        adversarial_markers = test_boot.SLICE_MARKERS["path-adversarial-suite"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            adversarial_markers.index(OBJECT_LOCATOR_RESOLUTION_READY),
            adversarial_markers.index(PATH_ADVERSARIAL_SUITE_READY),
        )
        self.assertLess(
            adversarial_markers.index(PATH_ADVERSARIAL_SUITE_READY),
            adversarial_markers.index(PHASE_12_COMPLETE),
        )
        self.assertLess(
            milestone_markers.index(PHASE_12_COMPLETE),
            milestone_markers.index(FRAMEBUFFER_READY),
        )

    def test_phase13_package_lifecycle_extends_phase_12_before_framebuffer(self) -> None:
        test_boot = load_test_boot_module()

        phase13_markers = test_boot.SLICE_MARKERS["phase13-package-lifecycle"]
        milestone_markers = test_boot.SLICE_MARKERS["milestone-1"]

        self.assertLess(
            phase13_markers.index(PHASE_12_COMPLETE),
            phase13_markers.index(PACKAGE_FORMAT_READY),
        )
        for left, right in zip(PHASE13_MARKERS, PHASE13_MARKERS[1:]):
            self.assertLess(phase13_markers.index(left), phase13_markers.index(right))
            self.assertLess(milestone_markers.index(left), milestone_markers.index(right))
        self.assertLess(
            milestone_markers.index(PHASE_13_COMPLETE),
            milestone_markers.index(FRAMEBUFFER_READY),
        )


def load_tests(loader, tests, pattern):
    compatibility_path = ROOT / "tests" / "test_interface_compatibility_freeze.py"
    spec = importlib.util.spec_from_file_location(
        "interface_compatibility_freeze_tests", compatibility_path
    )
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    tests.addTests(loader.loadTestsFromModule(module))
    return tests


if __name__ == "__main__":
    unittest.main()
