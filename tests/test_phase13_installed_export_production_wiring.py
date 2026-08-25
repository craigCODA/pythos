import unittest
from pathlib import Path


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

    def test_install_paths_materialize_manifest_exports_without_seed_helper(self):
        text = source("package_service.rs")
        helper = text.index("fn add_manifest_exports_to_registry")
        tests_mod = text.index("#[cfg(test)]\nmod tests")
        compatibility = text[
            text.index("fn install_compatibility") : text.index("pub fn prepare_install_candidate")
        ]
        candidate = text[
            text.index("fn prepare_install_candidate_inner") : text.index(
                "pub fn publish_install_candidate"
            )
        ]

        self.assertLess(helper, tests_mod)
        self.assertIn("MANIFEST_RECORD_PACKAGE_EXPORT", text[:tests_mod])
        self.assertIn("add_manifest_exports_to_registry", compatibility)
        self.assertIn("add_manifest_exports_to_registry", candidate)
        self.assertNotIn("seed_launch_export_for_test(", compatibility)
        self.assertNotIn("seed_launch_export_for_test(", candidate)

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
