import unittest
from pathlib import Path


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

    def test_non_verify_package_context_provider_uses_retained_service(self):
        """Break caught: ordinary production provider is a Denied stub."""
        text = source("syscall.rs")

        self.assertIn(PRODUCTION_CFG, text)
        self.assertIn("with_retained_package_service_for_phase13", text)
        self.assertNotIn(
            "not(feature = \"verify\"),\n    not(feature = \"phase13-package-test\")\n))]\nfn package_runtime_schema_binding",
            text,
        )
        self.assertIn(
            'feature = "hardware-probe",\n        not(feature = "phase13-package-test")',
            text,
        )

    def test_normal_boot_initializes_retained_package_service(self):
        """Break caught: normal boot restores object service but not PackageService."""
        self.assertIn(
            "initialize_package_service_from_device(substrate.block_device)",
            source("normal_boot.rs"),
        )


if __name__ == "__main__":
    unittest.main()
