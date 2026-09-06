import unittest
from pathlib import Path

from phase13_cargo_behavior import run_exact_core_test


ROOT = Path(__file__).resolve().parents[1]
CORE_SRC = ROOT / "core" / "src"
PRODUCTION_CFG = 'all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))'


def source(path: str) -> str:
    return (CORE_SRC / path).read_text(encoding="utf-8")


def cfg_block_before_mod(module_name: str) -> str:
    lines = source("main.rs").splitlines()
    needle = f"mod {module_name};"
    for index, line in enumerate(lines):
        if line.strip() == needle:
            start = index
            while start > 0:
                previous = lines[start - 1].strip()
                if (
                    previous.startswith("#[cfg")
                    or previous in {"))]", "))"}
                    or previous.startswith("any(")
                    or previous == "test,"
                    or previous.startswith("feature =")
                    or previous.startswith("all(")
                    or previous.startswith("not(")
                ):
                    start -= 1
                    continue
                break
            return "\n".join(lines[start:index])
    raise AssertionError(f"module {module_name!r} not found in core/src/main.rs")


class Phase13PackageContextProductionWiringTests(unittest.TestCase):
    def test_package_service_modules_include_ordinary_production_cfg(self):
        """Break caught: package service support compiles only under test/acceptance."""
        for module in [
            "package_candidate_store",
            "package_content_store",
            "package_registry",
            "package_service",
            "package_source",
        ]:
            with self.subTest(module=module):
                self.assertIn(PRODUCTION_CFG, cfg_block_before_mod(module))

    def test_package_context_syscall_uses_retained_service_behavior(self):
        run_exact_core_test(
            "syscall::tests::package_context_syscall_uses_phase13_retained_package_service"
        )

    def test_normal_boot_initializes_retained_package_service(self):
        """Break caught: normal boot restores object service but not PackageService."""
        self.assertIn(
            "initialize_package_service_from_device(substrate.block_device)",
            source("normal_boot.rs"),
        )

    def test_production_package_service_restore_uses_retained_slot_in_place(self):
        """Break caught: normal boot stack overflows restoring a local PackageService."""
        text = source("package_service.rs")
        start = text.index("pub(crate) fn initialize_package_service_from_device")
        end = text.index("\n#[cfg(test)]", start)
        initializer = text[start:end]

        self.assertNotIn("let mut service = PackageService::new_empty();", initializer)
        self.assertIn("restore_retained_package_service_from_device", initializer)
        self.assertIn(
            "service.restore_from_storage(device)",
            initializer,
        )


if __name__ == "__main__":
    unittest.main()
