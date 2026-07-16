from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "qemu-acceptance.yml"


class CiWorkflowTest(unittest.TestCase):
    def test_qemu_acceptance_workflow_exists_and_runs_required_gates(self) -> None:
        self.assertTrue(WORKFLOW.exists(), "missing QEMU acceptance CI workflow")

        workflow = WORKFLOW.read_text(encoding="utf-8")
        required_snippets = (
            "on:",
            "push:",
            "pull_request:",
            "qemu-system-x86",
            "ovmf",
            "rustup target add x86_64-unknown-uefi x86_64-unknown-none",
            "cargo fmt --check",
            "cargo test -p pythos-shared",
            "cargo test -p pythos-core",
            "cargo clippy -p pythos-core --target x86_64-unknown-none -- -D warnings",
            "cargo clippy -p pythos-boot --target x86_64-unknown-uefi -- -D warnings",
            "python -m unittest tests.test_iso_image tests.test_boot_marker_contract tests.test_qemu_exit tests.test_ci_workflow",
            "python scripts/test-boot.py --slice phase-6-complete",
            "python scripts/test-boot.py --slice graceful-audio-fallback --no-audio-device",
            "python scripts/test-boot.py --slice milestone-1",
            "python scripts/test-boot.py --slice milestone-1 --media iso",
            "python -m unittest tests.boot_core_handoff",
        )

        for snippet in required_snippets:
            self.assertIn(snippet, workflow)


if __name__ == "__main__":
    unittest.main()
