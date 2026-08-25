import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SYSCALL_RS = ROOT / "core" / "src" / "syscall.rs"
PYTH_RUNTIME_LAUNCH_RS = ROOT / "core" / "src" / "pyth_runtime_launch.rs"
PACKAGE_ACCEPTANCE_RS = ROOT / "core" / "src" / "package_acceptance.rs"
PACKAGE_LAUNCH_SCRIPT = ROOT / "scripts" / "test-phase13-package-launch.py"
MAIN_RS = ROOT / "core" / "src" / "main.rs"

PHASE13_FEATURE = 'feature = "phase13-package-test"'
PHASE13_ONLY_HELPER_CFG = 'all(not(test), feature = "phase13-package-test")'
VERIFY_ONLY_STUB_CFG = (
    'all(not(test), feature = "verify", not(feature = "phase13-package-test"))'
)


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def compact(text: str) -> str:
    return "".join(text.split())


def cfg_before(path: Path, anchor: str) -> str:
    lines = read(path).splitlines()
    for index, line in enumerate(lines):
        if anchor in line:
            start = index
            while start > 0 and lines[start - 1].strip():
                start -= 1
            return "\n".join(lines[start:index])
    raise AssertionError(f"anchor {anchor!r} not found in {path.relative_to(ROOT)}")


class Phase13PackageRuntimeLaunchWiringTests(unittest.TestCase):
    def test_pyth_runtime_launch_real_path_is_available_to_phase13_acceptance(self):
        """Break caught: Phase 13 can build a bootstrap but cannot prepare ring-3 runtime launch."""
        launch_source = read(PYTH_RUNTIME_LAUNCH_RS)

        self.assertIn("prepare_package_pyth_runtime_launch", launch_source)
        self.assertIn(
            PHASE13_FEATURE,
            cfg_before(PYTH_RUNTIME_LAUNCH_RS, "pub struct PreparedPythRuntimeLaunch"),
        )
        self.assertIn(
            PHASE13_FEATURE,
            cfg_before(PYTH_RUNTIME_LAUNCH_RS, "fn prepare_pyth_launch_with_package"),
        )

    def test_phase13_package_acceptance_passes_allocator_to_runtime_launch(self):
        """Break caught: acceptance still stops at PROCESS_CREATED without a real user address space."""
        acceptance_source = read(PACKAGE_ACCEPTANCE_RS)
        main_source = read(MAIN_RS)

        self.assertIn("&mut memory::physical::PhysicalMemory", acceptance_source)
        self.assertIn("prepare_package_pyth_runtime_launch", acceptance_source)
        self.assertIn("enter_prepared_pyth_runtime_launch", acceptance_source)
        self.assertIn("&mut physical_memory", main_source)
        self.assertIn("phase13_supervisor_mappings", main_source)

    def test_package_runtime_bootstrap_uses_launch_granted_import_capabilities(self):
        """Break caught: runtime validates launch grants then replaces them with fresh host grants."""
        launch_source = read(PYTH_RUNTIME_LAUNCH_RS)
        package_prepare_start = launch_source.index("pub fn prepare_package_pyth_runtime_launch")
        helper_start = launch_source.index("fn prepare_pyth_launch_with_package")
        package_prepare = launch_source[package_prepare_start:helper_start]

        self.assertIn(
            "let import_capabilities = package_launch_import_capabilities",
            package_prepare,
        )
        self.assertIn("Some(import_capabilities)", package_prepare)
        self.assertIn(
            "package_import_capabilities: Option<PythGraphImportCapabilities>",
            launch_source,
        )

    def test_package_launch_validation_and_runtime_syscall_use_same_capability_table(self):
        """Break caught: launch validates a local test table but runtime syscalls use the real table."""
        acceptance_source = read(PACKAGE_ACCEPTANCE_RS)
        syscall_source = read(SYSCALL_RS)

        self.assertIn(
            "with_pyth_graph_system_log_launch_capability",
            acceptance_source,
        )
        self.assertIn(
            "pub fn with_pyth_graph_system_log_launch_capability",
            syscall_source,
        )
        self.assertIn(
            "let handle = table.grant(",
            syscall_source,
        )
        self.assertIn(
            "Ok(f(handle, table))",
            syscall_source,
        )

    def test_system_log_launch_requirement_uses_kernel_log_right_not_graph_read_right(self):
        """Break caught: package launch grant validates but fails the real graph-log syscall."""
        acceptance_source = read(PACKAGE_ACCEPTANCE_RS)
        requirement_start = acceptance_source.index("fn launch_requirement()")
        identity_start = acceptance_source.index("fn launch_identity_for_acceptance()")
        requirement = acceptance_source[requirement_start:identity_start]

        self.assertIn("RIGHTS_READ", acceptance_source)
        self.assertIn("bootstrap.imports[0].rights != RIGHTS_READ", acceptance_source)
        self.assertIn("rights: RightsMask::new(RightsMask::LOG)", requirement)
        self.assertNotIn("rights: RightsMask::new(RightsMask::READ)", requirement)

    def test_graph_log_and_exit_are_real_only_for_production_or_phase13_acceptance(self):
        """Break caught: verify+phase13 runtime entry reaches verify-only BadResult graph syscalls."""
        real_log_cfg = cfg_before(SYSCALL_RS, "fn dispatch_pyth_graph_log(args")
        real_exit_cfg = cfg_before(SYSCALL_RS, "fn dispatch_pyth_graph_exit(args")
        graph_launch_capability_cfg = cfg_before(
            SYSCALL_RS, "pub fn with_pyth_graph_system_log_launch_capability"
        )
        stub_log_cfg = cfg_before(SYSCALL_RS, "fn dispatch_pyth_graph_log(_args")
        stub_exit_cfg = cfg_before(SYSCALL_RS, "fn dispatch_pyth_graph_exit(_args")

        self.assertIn(PHASE13_FEATURE, real_log_cfg)
        self.assertIn(PHASE13_FEATURE, real_exit_cfg)
        self.assertIn(compact(PHASE13_ONLY_HELPER_CFG), compact(graph_launch_capability_cfg))
        self.assertNotIn('not(feature = "verify")', graph_launch_capability_cfg)
        self.assertIn(compact(VERIFY_ONLY_STUB_CFG), compact(stub_log_cfg))
        self.assertIn(compact(VERIFY_ONLY_STUB_CFG), compact(stub_exit_cfg))

    def test_launch_acceptance_harness_requires_runtime_execution_markers(self):
        """Break caught: QEMU launch success passes with only bootstrap/process-created markers."""
        script = read(PACKAGE_LAUNCH_SCRIPT)

        self.assertIn("scripts/build-pyth-runtime.py", script)
        self.assertIn("scripts/verify-pyth-runtime-elf.py", script)
        self.assertIn('"--with-pythtig"', script)
        self.assertIn('"success_marker": "PYTHOS:PYTHTIG:RUNTIME_TERMINATED"', script)
        self.assertIn("PYTHOS:PYTHTIG:RUNTIME_ENTER package:", script)
        self.assertIn("PYTHOS:PYTHTIG:PROGRAM_LOG", script)
        self.assertIn("PYTHOS:PYTHTIG:RUNTIME_EXIT status:0", script)


if __name__ == "__main__":
    unittest.main()
