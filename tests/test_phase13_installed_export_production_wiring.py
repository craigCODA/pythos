import unittest
from pathlib import Path

from phase13_cargo_behavior import run_exact_core_test


ROOT = Path(__file__).resolve().parents[1]
CORE_SRC = ROOT / "core" / "src"


def source(path: str) -> str:
    return (CORE_SRC / path).read_text(encoding="utf-8")


class Phase13InstalledExportProductionWiringTests(unittest.TestCase):
    def test_seed_launch_export_helper_remains_test_only(self):
        text = source("package_service.rs")
        helper = text.index("pub fn seed_launch_export_for_test")
        prefix = text[:helper].splitlines()[-3:]

        self.assertIn("#[cfg(test)]", "\n".join(prefix))

    def test_installed_manifest_export_survives_restore_and_launches_from_registry(self):
        text = source("package_service.rs")
        compatibility_start = text.index("fn install_compatibility(")
        compatibility_end = text.index("pub fn prepare_install_candidate", compatibility_start)
        candidate_start = text.index("fn prepare_install_candidate_inner(")
        candidate_end = text.index("pub fn publish_install_candidate", candidate_start)
        compatibility = text[compatibility_start:compatibility_end]
        candidate = text[candidate_start:candidate_end]
        materializer = "add_manifest_exports_and_requirements_to_registry"

        self.assertEqual(compatibility.count(materializer), 1)
        self.assertEqual(candidate.count(materializer), 1)
        self.assertNotIn("seed_launch_export_for_test(", compatibility)
        self.assertNotIn("seed_launch_export_for_test(", candidate)
        run_exact_core_test(
            "package_service::tests::package_install_into_preserves_valid_compatibility_install"
        )
        run_exact_core_test(
            "package_service::tests::installed_manifest_export_survives_restore_and_launch_uses_registry_path"
        )

    def test_launch_consumes_only_launchable_exports(self):
        text = source("package_service.rs")
        launch = text[text.index("pub fn launch(") : text.index("pub fn runtime_schema_binding")]

        self.assertIn("ensure_launchable_export(export)", launch)
        self.assertIn("PACKAGE_EXPORT_KIND_TOOL", text)

    def test_registry_snapshot_persists_export_count_and_records(self):
        text = source("package_registry.rs")

        self.assertIn("write_u32(out, 72, self.export_count)", text)
        self.assertIn("PACKAGE_REGISTRY_EXPORT_RECORD_LEN", text)
        self.assertIn("decode_export_record(bytes, offset)", text)


if __name__ == "__main__":
    unittest.main()
