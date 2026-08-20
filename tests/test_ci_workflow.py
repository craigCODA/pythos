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
            'PYTHOS_TEST_BOOT_TIMEOUT: "60"',
            "rustup target add x86_64-unknown-uefi x86_64-unknown-none",
            "cargo fmt --check",
            "cargo test -p pythos-shared",
            "cargo test -p pythos-shared --features pyth-tig-test-support",
            "cargo test -p pythos-core",
            "cargo test -p pythos-core pyth_service_supervisor",
            "cargo test -p pythc",
            "cargo test -p pythos-user-pyth-runtime",
            "python scripts/test-pyth-tig-format.py",
            "cargo run -p pythc -- build programs/session-manager/main.pyth -o target/pyth-tig/session-manager.tig",
            "cargo run -p pyth-tig-tool -- verify target/pyth-tig/session-manager.tig",
            "python scripts/test-pythc.py",
            "python scripts/build-user-shell.py",
            "python scripts/verify-user-elf.py",
            "cargo clippy -p pythos-core --target x86_64-unknown-none --features verify -- -D warnings",
            "cargo clippy -p pythos-core --target x86_64-unknown-none --features verify,sdhci-emmc-backend -- -D warnings",
            "cargo clippy -p pythos-boot --target x86_64-unknown-uefi -- -D warnings",
            "python -m py_compile scripts/build-pyth-graph.py scripts/build-image.py scripts/build-iso.py scripts/test-pyth-default-boot.py scripts/test-normal-fast-boot.py scripts/test-object-shell.py scripts/test-com2-shell-transport.py scripts/test-normal-boot-interactive.py scripts/test-pyth-graph-runtime.py scripts/test-pyth-graph-object-flow.py scripts/test-pyth-native-codegen.py scripts/pyth_cross_target.py scripts/test-pyth-cross-target.py scripts/prepare-pyth-physical-image.py scripts/verify-pyth-physical-log.py",
            "python -m unittest tests.test_iso_image tests.test_boot_marker_contract tests.test_qemu_exit tests.test_ci_workflow tests.test_build_orchestration tests.test_verify_user_elf tests.test_interface_compatibility_freeze",
            "python scripts/test-pyth-graph-runtime.py",
            "python scripts/test-pyth-graph-object-flow.py",
            "python scripts/test-pyth-native-codegen.py",
            "python scripts/test-pyth-cross-target.py --unit-only",
            "python scripts/test-pyth-cross-target.py --automated-only",
            "python scripts/test-pyth-default-boot.py",
            "python scripts/test-object-shell.py",
            "python scripts/test-normal-fast-boot.py",
            "python scripts/test-boot.py --slice phase-6-complete",
            "python scripts/test-boot.py --slice phase-6-complete --timeout 60",
            "python scripts/test-boot.py --slice graceful-audio-fallback --no-audio-device --timeout 60",
            "python scripts/test-boot.py --slice milestone-1 --timeout 60",
            "python scripts/test-boot.py --slice milestone-1 --media iso --timeout 60",
            "python scripts/test-persistent-storage.py",
            "python scripts/verify-pyth-physical-log.py --self-test",
            "python scripts/prepare-pyth-physical-image.py --manifest target/pyth-physical-image-manifest.json",
            "python scripts/verify-pyth-physical-log.py --manifest target/pyth-physical-image-manifest.json --log target/pyth-cross-target-ahci.log --backend ahci --target-id qemu-ahci-import-smoke --output target/pyth-physical-log-verification-ahci.json",
            "python -m unittest tests.boot_core_handoff",
        )

        for snippet in required_snippets:
            self.assertIn(snippet, workflow)


if __name__ == "__main__":
    unittest.main()
