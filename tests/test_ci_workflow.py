from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "qemu-acceptance.yml"


class CiWorkflowTest(unittest.TestCase):
    @staticmethod
    def _job_block(workflow: str, job_id: str) -> str:
        marker = f"  {job_id}:\n"
        start = workflow.find(marker)
        if start < 0:
            raise AssertionError(f"missing workflow job: {job_id}")

        next_job = workflow.find("\n  ", start + len(marker))
        while next_job >= 0:
            line_end = workflow.find("\n", next_job + 1)
            candidate = workflow[next_job + 1 : line_end if line_end >= 0 else None]
            if candidate.startswith("  ") and not candidate.startswith("    "):
                return workflow[start:next_job]
            next_job = workflow.find("\n  ", next_job + 3)

        return workflow[start:]

    def test_qemu_runtime_and_firmware_are_pinned_and_asserted(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        required_snippets = (
            "runs-on: ubuntu-24.04",
            'QEMU_VERSION: "11.1.1"',
            'QEMU_SHA256: "079ffbff8a7111bbc89022107cbabf3bbfd614d5fc9d7cc675991196aca12482"',
            'OVMF_VERSION: "2024.02-2ubuntu0.9"',
            "uses: actions/cache@v4",
            "https://download.qemu.org/qemu-${QEMU_VERSION}.tar.xz",
            "sha256sum --check --strict",
            '"ovmf=${OVMF_VERSION}"',
            'qemu-system-x86_64 --version | grep -F "QEMU emulator version ${QEMU_VERSION}"',
            'dpkg-query -W -f=\'${Version}\\n\' ovmf | grep -Fx "${OVMF_VERSION}"',
            "PYTHOS_OVMF_CODE=/usr/share/OVMF/OVMF_CODE_4M.fd",
            "tests.test_qemu_boot_media",
        )

        for snippet in required_snippets:
            self.assertIn(snippet, workflow)

        self.assertNotIn("runs-on: ubuntu-latest", workflow)
        self.assertNotIn("PYTHOS_OVMF_VARS", workflow)

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
            "cargo test -p pythos-user-session-input-probe",
            "python scripts/test-pyth-tig-format.py",
            "cargo run -p pythc -- build programs/session-manager/main.pyth -o target/pyth-tig/session-manager.tig",
            "cargo run -p pyth-tig-tool -- verify target/pyth-tig/session-manager.tig",
            "python scripts/test-pythc.py",
            "python scripts/build-user-shell.py",
            "python scripts/verify-user-elf.py",
            "cargo clippy -p pythos-core --target x86_64-unknown-none --features verify -- -D warnings",
            "cargo clippy -p pythos-core --target x86_64-unknown-none --features verify,sdhci-emmc-backend -- -D warnings",
            "cargo clippy -p pythos-core --target x86_64-unknown-none --features session-input-bridge-probe -- -D warnings",
            "cargo clippy -p pythos-boot --target x86_64-unknown-uefi -- -D warnings",
            "scripts/build-session-input-probe.py",
            "scripts/test-session-input-bridge-probe.py",
            "python scripts/test-session-input-bridge-probe.py --self-test",
            "python -m unittest tests.test_iso_image tests.test_boot_marker_contract tests.test_qemu_exit tests.test_qemu_boot_media tests.test_ci_workflow tests.test_build_orchestration tests.test_verify_user_elf tests.test_interface_compatibility_freeze",
            "python scripts/test-pyth-graph-runtime.py",
            "python scripts/test-pyth-graph-object-flow.py",
            "python scripts/test-pyth-native-codegen.py",
            "python scripts/test-pyth-cross-target.py --unit-only",
            "python scripts/test-pyth-cross-target.py --automated-only",
            "python scripts/test-pyth-default-boot.py",
            "python scripts/test-object-shell.py",
            "python scripts/test-normal-fast-boot.py",
            "python scripts/test-physical-keyboard-console.py",
            "python scripts/test-boot.py --slice phase-6-complete",
            "python scripts/test-boot.py --slice phase-6-complete --timeout 60",
            "python scripts/test-boot.py --slice graceful-audio-fallback --no-audio-device --timeout 60",
            "python scripts/test-boot.py --slice milestone-1 --timeout 60",
            "python scripts/test-boot.py --slice milestone-1 --media iso --timeout 60",
            "python scripts/test-persistent-storage.py",
            "python scripts/test-session-input-bridge-probe.py",
            "python scripts/verify-pyth-physical-log.py --self-test",
            "python scripts/prepare-pyth-physical-image.py --manifest target/pyth-physical-image-manifest.json",
            "python scripts/verify-pyth-physical-log.py --manifest target/pyth-physical-image-manifest.json --log target/pyth-cross-target-ahci.log --backend ahci --target-id qemu-ahci-import-smoke --output target/pyth-physical-log-verification-ahci.json",
            "python -m unittest tests.boot_core_handoff",
        )

        for snippet in required_snippets:
            self.assertIn(snippet, workflow)

        self.assertLess(
            workflow.index("python scripts/test-session-input-bridge-probe.py --self-test"),
            workflow.index("python scripts/test-session-input-bridge-probe.py\n"),
            "bridge oracle self-tests must run before the live QEMU proof",
        )

    def test_long_handoff_suite_runs_parallel_and_has_one_aggregate_gate(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        milestone = self._job_block(workflow, "milestone_acceptance")
        handoff = self._job_block(workflow, "handoff_acceptance")
        aggregate = self._job_block(workflow, "qemu_acceptance")

        self.assertNotIn("needs:", milestone)
        self.assertNotIn("needs:", handoff)
        self.assertIn("python scripts/test-session-input-bridge-probe.py", milestone)
        self.assertNotIn("python -m unittest tests.boot_core_handoff", milestone)
        self.assertEqual(handoff.count("python -m unittest tests.boot_core_handoff"), 1)

        self.assertIn("name: qemu-acceptance", aggregate)
        self.assertIn("- milestone_acceptance", aggregate)
        self.assertIn("- handoff_acceptance", aggregate)
        self.assertIn("if: ${{ always() }}", aggregate)
        self.assertIn("needs.milestone_acceptance.result", aggregate)
        self.assertIn("needs.handoff_acceptance.result", aggregate)

    def test_pull_requests_do_not_also_run_feature_branch_push_acceptance(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        trigger_block = workflow[: workflow.index("\njobs:")]

        self.assertIn("push:\n    branches:\n      - main", trigger_block)
        self.assertIn("pull_request:", trigger_block)


if __name__ == "__main__":
    unittest.main()
