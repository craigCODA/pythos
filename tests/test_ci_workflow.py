from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "qemu-acceptance.yml"


class CiWorkflowTest(unittest.TestCase):
    NORMAL_SESSION_MILESTONE_ONLY_COMMANDS = (
        "cargo test -p pythos-user-session-runtime --features normal-session",
        "cargo test -p pythos-user-session-runtime --features normal-session-fault-test",
        "cargo test -p pythos-core normal_session",
        "cargo run -p pythc -- build programs/normal-session-manager/main.pyth -o target/normal-session/pyth-tig/session-manager.tig",
        "cargo run -p pyth-tig-tool -- verify target/normal-session/pyth-tig/session-manager.tig",
        "python scripts/build-session-runtime.py --features normal-session",
        "python scripts/build-session-runtime.py --features normal-session-fault-test",
        "cargo clippy -p pythos-user-session-runtime --target x86_64-unknown-none --bin pythos-normal-session --features normal-session -- -D warnings",
        "cargo clippy -p pythos-user-session-runtime --target x86_64-unknown-none --bin pythos-normal-session --features normal-session-fault-test -- -D warnings",
        "python -m py_compile scripts/test-normal-session.py",
        "python scripts/test-normal-session.py --self-test",
        "python scripts/test-normal-session.py",
        "python scripts/test-normal-session.py --fault",
    )
    LINK_LAYER_MILESTONE_ONLY_COMMANDS = (
        "python scripts/test-link-layer.py --self-test",
        "python scripts/test-link-layer.py",
    )
    ARP_MILESTONE_ONLY_COMMANDS = (
        "cargo test -p pythos-user-arp-probe",
        "cargo clippy -p pythos-core --target x86_64-unknown-none --features arp-probe -- -D warnings",
        "cargo clippy -p pythos-user-arp-probe --target x86_64-unknown-none -- -D warnings",
        "python -m py_compile scripts/build-arp-probe.py scripts/test-arp.py",
        "python -m unittest tests.test_arp",
        "python scripts/test-arp.py --self-test",
        "python scripts/test-arp.py",
    )
    IPV4_MILESTONE_ONLY_COMMANDS = (
        "cargo test -p pythos-user-ipv4-probe",
        "cargo clippy -p pythos-core --target x86_64-unknown-none --features ipv4-probe -- -D warnings",
        "cargo clippy -p pythos-user-ipv4-probe --target x86_64-unknown-none -- -D warnings",
        "python -m py_compile scripts/build-ipv4-probe.py scripts/test-ipv4.py",
        "python -m unittest tests.test_ipv4",
        "python scripts/test-ipv4.py --self-test",
        "python scripts/test-ipv4.py",
    )
    ICMP_MILESTONE_ONLY_COMMANDS = (
        "cargo test -p pythos-user-icmp-probe",
        "python scripts/build-icmp-probe.py",
        "cargo clippy -p pythos-core --target x86_64-unknown-none --features icmp-probe -- -D warnings",
        "cargo clippy -p pythos-user-icmp-probe --target x86_64-unknown-none -- -D warnings",
        "python -m py_compile scripts/build-icmp-probe.py scripts/test-icmp.py",
        "python -m unittest tests.test_icmp",
        "python scripts/test-icmp.py --self-test",
        "python scripts/test-icmp.py",
    )
    UDP_MILESTONE_ONLY_COMMANDS = (
        "cargo test -p pythos-user-udp-probe",
        "python scripts/build-udp-probe.py",
        "cargo clippy -p pythos-core --target x86_64-unknown-none --features udp-probe -- -D warnings",
        "cargo clippy -p pythos-user-udp-probe --target x86_64-unknown-none -- -D warnings",
        "python -m py_compile scripts/build-udp-probe.py",
    )
    TCP_MILESTONE_ONLY_COMMANDS = (
        "cargo test -p pythos-user-tcp-probe",
        "python scripts/build-tcp-probe.py",
        "cargo clippy -p pythos-core --target x86_64-unknown-none --features tcp-probe -- -D warnings",
        "cargo clippy -p pythos-user-tcp-probe --target x86_64-unknown-none -- -D warnings",
        "python -m py_compile scripts/build-tcp-probe.py scripts/test-tcp.py",
        "python -m unittest tests.test_tcp",
        "python scripts/test-tcp.py --self-test",
        "python scripts/test-tcp.py",
    )
    DNS_MILESTONE_ONLY_COMMANDS = (
        "cargo test -p pythos-user-dns-probe",
        "cargo test -p pythos-core dns_probe --features dns-probe",
        "python scripts/build-dns-probe.py",
        "cargo clippy -p pythos-core --target x86_64-unknown-none --features dns-probe -- -D warnings",
        "cargo clippy -p pythos-user-dns-probe --target x86_64-unknown-none -- -D warnings",
        "python -m py_compile scripts/build-dns-probe.py scripts/test-dns.py",
        "python -m unittest tests.test_dns",
        "python scripts/test-dns.py --self-test",
        "python scripts/test-dns.py",
    )
    SESSION_RUNTIME_MILESTONE_ONLY_COMMANDS = (
        "cargo test -p pythos-user-session-runtime",
        "cargo test -p pythos-core session_runtime",
        "cargo test -p pythos-core pyth_service_supervisor",
        "python scripts/build-session-runtime.py --target-dir target/session-runtime-probe",
        "python scripts/verify-user-elf.py --elf target/session-runtime-probe/x86_64-unknown-none/debug/pythos-user-session-runtime",
        "cargo clippy -p pythos-user-session-runtime --target x86_64-unknown-none -- -D warnings",
        "cargo clippy -p pythos-core --target x86_64-unknown-none --features session-runtime-probe -- -D warnings",
        "python -m py_compile scripts/qemu_probe_support.py scripts/build-session-runtime.py scripts/test-session-runtime-probe.py",
        "python -m unittest tests.test_iso_image tests.test_boot_marker_contract tests.test_qemu_exit tests.test_qemu_boot_media tests.test_ci_workflow tests.test_build_orchestration tests.test_verify_user_elf tests.test_interface_compatibility_freeze tests.test_session_input_bridge_boundary tests.test_session_runtime_boundary",
        "python scripts/test-session-input-bridge-probe.py --self-test",
        "python scripts/test-session-input-bridge-probe.py",
        "python scripts/test-session-runtime-probe.py --self-test",
        "python scripts/test-session-runtime-probe.py --fault-test",
        "python scripts/test-session-runtime-probe.py",
        "cargo test -p pythos-user-session-runtime --features session-viewing",
        "cargo clippy -p pythos-user-session-runtime --target x86_64-unknown-none --features session-viewing -- -D warnings",
        "cargo clippy -p pythos-core --target x86_64-unknown-none --features session-viewing-probe -- -D warnings",
        "python -m py_compile scripts/test-session-viewing-probe.py",
        "python scripts/test-session-viewing-probe.py --self-test",
        "python scripts/test-session-viewing-probe.py",
    )

    @staticmethod
    def _commands(block: str) -> list[str]:
        commands = []
        for line in block.splitlines():
            command = line.strip()
            if command.startswith("run: "):
                command = command.removeprefix("run: ")
            commands.append(command)
        return commands

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

    @classmethod
    def _workflow_with_handoff_command(cls, workflow: str, command: str) -> str:
        handoff_start = workflow.index("  handoff_acceptance:\n")
        upload_marker = "      - name: Upload handoff logs\n"
        insertion_at = workflow.index(upload_marker, handoff_start)
        injected_step = (
            "      - name: Injected duplicate\n"
            f"        run: {command}\n\n"
        )
        return workflow[:insertion_at] + injected_step + workflow[insertion_at:]

    def _assert_handoff_excludes_session_runtime_gates(self, workflow: str) -> None:
        handoff_commands = self._commands(self._job_block(workflow, "handoff_acceptance"))
        for command in self.SESSION_RUNTIME_MILESTONE_ONLY_COMMANDS:
            self.assertNotIn(
                command,
                handoff_commands,
                f"handoff must not duplicate milestone-only command: {command}",
            )

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
            "cargo test -p pythos-user-link-layer-probe",
            "python scripts/test-pyth-tig-format.py",
            "cargo run -p pythc -- build programs/session-manager/main.pyth -o target/pyth-tig/session-manager.tig",
            "cargo run -p pyth-tig-tool -- verify target/pyth-tig/session-manager.tig",
            "python scripts/test-pythc.py",
            "python scripts/build-user-shell.py",
            "python scripts/verify-user-elf.py",
            "cargo clippy -p pythos-core --target x86_64-unknown-none --features verify -- -D warnings",
            "cargo clippy -p pythos-core --target x86_64-unknown-none --features verify,sdhci-emmc-backend -- -D warnings",
            "cargo clippy -p pythos-core --target x86_64-unknown-none --features session-input-bridge-probe -- -D warnings",
            "cargo clippy -p pythos-core --target x86_64-unknown-none --features link-layer-probe -- -D warnings",
            "cargo clippy -p pythos-user-link-layer-probe --target x86_64-unknown-none -- -D warnings",
            "cargo clippy -p pythos-boot --target x86_64-unknown-uefi -- -D warnings",
            "scripts/build-session-input-probe.py",
            "scripts/test-session-input-bridge-probe.py",
            "python -m py_compile scripts/test-link-layer.py",
            "python scripts/test-link-layer.py --self-test",
            "python scripts/test-link-layer.py",
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

    def test_session_runtime_slice_is_fully_gated_in_milestone_only(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        milestone = self._job_block(workflow, "milestone_acceptance")
        aggregate = self._job_block(workflow, "qemu_acceptance")
        milestone_commands = self._commands(milestone)

        for command in self.SESSION_RUNTIME_MILESTONE_ONLY_COMMANDS:
            self.assertEqual(
                milestone_commands.count(command),
                1,
                f"milestone must run exactly once: {command}",
            )

        self.assertLess(
            milestone_commands.index("python scripts/test-session-input-bridge-probe.py --self-test"),
            milestone_commands.index("python scripts/test-session-input-bridge-probe.py"),
            "Slice 1 oracle self-tests must precede its live QEMU proof",
        )
        self.assertLess(
            milestone_commands.index("python scripts/test-session-runtime-probe.py --self-test"),
            milestone_commands.index("python scripts/test-session-runtime-probe.py --fault-test"),
            "Slice 2 oracle self-tests must precede its live fault proof",
        )
        self.assertLess(
            milestone_commands.index("python scripts/test-session-runtime-probe.py --fault-test"),
            milestone_commands.index("python scripts/test-session-runtime-probe.py"),
            "Slice 2 fault proof must precede its standard live QEMU proof",
        )
        first_live = min(
            milestone_commands.index("python scripts/test-session-input-bridge-probe.py"),
            milestone_commands.index("python scripts/test-session-runtime-probe.py"),
        )
        last_self_test = max(
            milestone_commands.index("python scripts/test-session-input-bridge-probe.py --self-test"),
            milestone_commands.index("python scripts/test-session-runtime-probe.py --self-test"),
            milestone_commands.index("python scripts/test-session-viewing-probe.py --self-test"),
        )
        self.assertLess(last_self_test, first_live, "all oracle self-tests must precede live QEMU")
        self.assertLess(
            milestone_commands.index("python scripts/test-session-runtime-probe.py"),
            milestone_commands.index("python scripts/test-session-viewing-probe.py"),
            "accepted runtime predecessor must pass before the Viewing profile",
        )

        self._assert_handoff_excludes_session_runtime_gates(workflow)
        self.assertEqual(workflow.count("  qemu_acceptance:\n"), 1)
        self.assertIn("- milestone_acceptance", aggregate)
        self.assertIn("- handoff_acceptance", aggregate)

    def test_handoff_rejects_every_session_runtime_gate_duplicate(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        for command in self.SESSION_RUNTIME_MILESTONE_ONLY_COMMANDS:
            with self.subTest(command=command):
                mutated = self._workflow_with_handoff_command(workflow, command)
                mutated_handoff = self._commands(
                    self._job_block(mutated, "handoff_acceptance")
                )
                self.assertEqual(mutated_handoff.count(command), 1)
                with self.assertRaises(AssertionError):
                    self._assert_handoff_excludes_session_runtime_gates(mutated)

    def test_normal_session_has_real_host_strict_build_and_live_gates(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        milestone = self._job_block(workflow, "milestone_acceptance")
        handoff = self._job_block(workflow, "handoff_acceptance")
        commands = self._commands(milestone)
        for command in self.NORMAL_SESSION_MILESTONE_ONLY_COMMANDS:
            self.assertEqual(commands.count(command), 1, f"missing or duplicate normal gate: {command}")
            self.assertNotIn(command, self._commands(handoff))
        self.assertLess(
            commands.index("python scripts/test-normal-session.py --self-test"),
            commands.index("python scripts/test-normal-session.py"),
        )
        self.assertLess(
            commands.index("python scripts/test-normal-session.py"),
            commands.index("python scripts/test-normal-session.py --fault"),
        )
        self.assertNotIn(
            "cargo clippy -p pythos-core --target x86_64-unknown-none --features verify,normal-session -- -D warnings",
            workflow,
            "verify excludes the real normal kernel and cannot be presented as its strict gate",
        )

    def test_link_layer_has_host_strict_build_and_sequential_live_gates(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        milestone = self._job_block(workflow, "milestone_acceptance")
        handoff = self._job_block(workflow, "handoff_acceptance")
        commands = self._commands(milestone)

        for command in self.LINK_LAYER_MILESTONE_ONLY_COMMANDS:
            self.assertEqual(commands.count(command), 1, f"missing or duplicate link-layer gate: {command}")
            self.assertNotIn(command, self._commands(handoff))

        self.assertLess(
            commands.index("python scripts/test-link-layer.py --self-test"),
            commands.index("python scripts/test-link-layer.py"),
            "link-layer oracle self-test must precede its live QEMU proof",
        )
        self.assertLess(
            commands.index("python scripts/test-network-port.py"),
            commands.index("python scripts/test-link-layer.py"),
            "link-layer live proof must follow the NetworkPort proof",
        )

    def test_arp_has_host_strict_build_and_sequential_live_gates(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        milestone = self._job_block(workflow, "milestone_acceptance")
        handoff = self._job_block(workflow, "handoff_acceptance")
        commands = self._commands(milestone)

        for command in self.ARP_MILESTONE_ONLY_COMMANDS:
            self.assertEqual(commands.count(command), 1, f"missing or duplicate ARP gate: {command}")
            self.assertNotIn(command, self._commands(handoff))

        self.assertIn("cargo test -p pythos-user-session-runtime", commands)
        self.assertIn(
            "cargo test -p pythos-user-session-runtime --features normal-session",
            commands,
        )
        self.assertLess(
            commands.index("cargo test -p pythos-user-link-layer-probe"),
            commands.index("cargo test -p pythos-user-arp-probe"),
            "ARP probe unit coverage must follow the link-layer probe",
        )
        self.assertLess(
            commands.index("cargo clippy -p pythos-user-link-layer-probe --target x86_64-unknown-none -- -D warnings"),
            commands.index("cargo clippy -p pythos-core --target x86_64-unknown-none --features arp-probe -- -D warnings"),
            "ARP strict coverage must follow link-layer strict coverage",
        )
        self.assertLess(
            commands.index("cargo clippy -p pythos-core --target x86_64-unknown-none --features arp-probe -- -D warnings"),
            commands.index("cargo clippy -p pythos-user-arp-probe --target x86_64-unknown-none -- -D warnings"),
            "ARP consumer strict coverage must follow the ARP core profile",
        )
        self.assertLess(
            commands.index("python scripts/test-link-layer.py"),
            commands.index("python scripts/test-arp.py --self-test"),
            "ARP oracle self-test must follow the link-layer live proof",
        )
        self.assertLess(
            commands.index("python scripts/test-arp.py --self-test"),
            commands.index("python scripts/test-arp.py"),
            "ARP oracle self-test must precede its live QEMU proof",
        )
        self.assertLess(
            commands.index("python scripts/test-virtio-net.py"),
            commands.index("python scripts/test-link-layer.py"),
            "link-layer live proof must follow the raw Virtio proof",
        )
        self.assertLess(
            commands.index("python scripts/test-network-port.py"),
            commands.index("python scripts/test-link-layer.py"),
            "link-layer live proof must follow the NetworkPort proof",
        )
        self.assertLess(
            commands.index("python scripts/test-link-layer.py"),
            commands.index("python scripts/test-arp.py"),
            "ARP live proof must follow the link-layer proof",
        )

    def test_ipv4_has_host_strict_build_and_sequential_future_live_gates(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        milestone = self._job_block(workflow, "milestone_acceptance")
        handoff = self._job_block(workflow, "handoff_acceptance")
        commands = self._commands(milestone)

        for command in self.IPV4_MILESTONE_ONLY_COMMANDS:
            self.assertEqual(commands.count(command), 1, f"missing or duplicate IPv4 gate: {command}")
            self.assertNotIn(command, self._commands(handoff))

        ordered_pairs = (
            ("cargo test -p pythos-user-arp-probe", "cargo test -p pythos-user-ipv4-probe"),
            (
                "cargo clippy -p pythos-user-arp-probe --target x86_64-unknown-none -- -D warnings",
                "cargo clippy -p pythos-core --target x86_64-unknown-none --features ipv4-probe -- -D warnings",
            ),
            (
                "cargo clippy -p pythos-core --target x86_64-unknown-none --features ipv4-probe -- -D warnings",
                "cargo clippy -p pythos-user-ipv4-probe --target x86_64-unknown-none -- -D warnings",
            ),
            (
                "python -m py_compile scripts/build-arp-probe.py scripts/test-arp.py",
                "python -m py_compile scripts/build-ipv4-probe.py scripts/test-ipv4.py",
            ),
            ("python -m unittest tests.test_arp", "python -m unittest tests.test_ipv4"),
            ("python scripts/test-arp.py", "python scripts/test-ipv4.py --self-test"),
            ("python scripts/test-ipv4.py --self-test", "python scripts/test-ipv4.py"),
        )
        for predecessor, successor in ordered_pairs:
            self.assertLess(
                commands.index(predecessor),
                commands.index(successor),
                f"IPv4 gate must follow predecessor: {predecessor}",
            )

    def test_icmp_has_ordered_unit_build_strict_and_live_gates_after_ipv4(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        milestone = self._job_block(workflow, "milestone_acceptance")
        handoff = self._job_block(workflow, "handoff_acceptance")
        commands = self._commands(milestone)

        for command in self.ICMP_MILESTONE_ONLY_COMMANDS:
            self.assertEqual(commands.count(command), 1, f"missing or duplicate ICMP gate: {command}")
            self.assertNotIn(command, self._commands(handoff))

        ordered_pairs = (
            ("cargo test -p pythos-user-ipv4-probe", "cargo test -p pythos-user-icmp-probe"),
            ("cargo test -p pythos-user-icmp-probe", "python scripts/build-icmp-probe.py"),
            (
                "cargo clippy -p pythos-user-ipv4-probe --target x86_64-unknown-none -- -D warnings",
                "cargo clippy -p pythos-core --target x86_64-unknown-none --features icmp-probe -- -D warnings",
            ),
            (
                "cargo clippy -p pythos-core --target x86_64-unknown-none --features icmp-probe -- -D warnings",
                "cargo clippy -p pythos-user-icmp-probe --target x86_64-unknown-none -- -D warnings",
            ),
            (
                "python -m py_compile scripts/build-ipv4-probe.py scripts/test-ipv4.py",
                "python -m py_compile scripts/build-icmp-probe.py scripts/test-icmp.py",
            ),
            ("python -m unittest tests.test_ipv4", "python -m unittest tests.test_icmp"),
            ("python scripts/test-ipv4.py", "python scripts/test-icmp.py --self-test"),
            ("python scripts/test-icmp.py --self-test", "python scripts/test-icmp.py"),
        )
        for predecessor, successor in ordered_pairs:
            self.assertLess(
                commands.index(predecessor),
                commands.index(successor),
                f"ICMP gate must follow predecessor: {predecessor}",
            )

    def test_udp_has_ordered_unit_build_and_strict_gates_after_icmp(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        milestone = self._job_block(workflow, "milestone_acceptance")
        handoff = self._job_block(workflow, "handoff_acceptance")
        commands = self._commands(milestone)

        for command in self.UDP_MILESTONE_ONLY_COMMANDS:
            self.assertEqual(commands.count(command), 1, f"missing or duplicate UDP gate: {command}")
            self.assertNotIn(command, self._commands(handoff))

        ordered_pairs = (
            ("cargo test -p pythos-user-icmp-probe", "cargo test -p pythos-user-udp-probe"),
            ("python scripts/build-icmp-probe.py", "python scripts/build-udp-probe.py"),
            (
                "cargo clippy -p pythos-user-icmp-probe --target x86_64-unknown-none -- -D warnings",
                "cargo clippy -p pythos-core --target x86_64-unknown-none --features udp-probe -- -D warnings",
            ),
            (
                "cargo clippy -p pythos-core --target x86_64-unknown-none --features udp-probe -- -D warnings",
                "cargo clippy -p pythos-user-udp-probe --target x86_64-unknown-none -- -D warnings",
            ),
            (
                "python -m py_compile scripts/build-icmp-probe.py scripts/test-icmp.py",
                "python -m py_compile scripts/build-udp-probe.py",
            ),
        )
        for predecessor, successor in ordered_pairs:
            self.assertLess(
                commands.index(predecessor),
                commands.index(successor),
                f"UDP gate must follow predecessor: {predecessor}",
            )

    def test_tcp_has_ordered_unit_build_and_strict_gates_after_udp(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        milestone = self._job_block(workflow, "milestone_acceptance")
        handoff = self._job_block(workflow, "handoff_acceptance")
        commands = self._commands(milestone)

        for command in self.TCP_MILESTONE_ONLY_COMMANDS:
            self.assertEqual(commands.count(command), 1, f"missing or duplicate TCP gate: {command}")
            self.assertNotIn(command, self._commands(handoff))

        ordered_pairs = (
            ("cargo test -p pythos-user-udp-probe", "cargo test -p pythos-user-tcp-probe"),
            ("cargo test -p pythos-user-tcp-probe", "python scripts/build-tcp-probe.py"),
            ("python scripts/build-udp-probe.py", "python scripts/build-tcp-probe.py"),
            (
                "cargo clippy -p pythos-user-udp-probe --target x86_64-unknown-none -- -D warnings",
                "cargo clippy -p pythos-core --target x86_64-unknown-none --features tcp-probe -- -D warnings",
            ),
            (
                "cargo clippy -p pythos-core --target x86_64-unknown-none --features tcp-probe -- -D warnings",
                "cargo clippy -p pythos-user-tcp-probe --target x86_64-unknown-none -- -D warnings",
            ),
            (
                "python -m py_compile scripts/build-udp-probe.py",
                "python -m py_compile scripts/build-tcp-probe.py scripts/test-tcp.py",
            ),
            (
                "python -m py_compile scripts/build-tcp-probe.py scripts/test-tcp.py",
                "python -m unittest tests.test_tcp",
            ),
            (
                "python -m unittest tests.test_tcp",
                "python scripts/test-tcp.py --self-test",
            ),
            (
                "python scripts/test-tcp.py --self-test",
                "python scripts/test-tcp.py",
            ),
        )
        for predecessor, successor in ordered_pairs:
            self.assertLess(
                commands.index(predecessor),
                commands.index(successor),
                f"TCP gate must follow predecessor: {predecessor}",
            )

    def test_dns_has_ordered_compile_package_clippy_and_self_test_gates_after_tcp(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        milestone = self._job_block(workflow, "milestone_acceptance")
        handoff = self._job_block(workflow, "handoff_acceptance")
        commands = self._commands(milestone)

        for command in self.DNS_MILESTONE_ONLY_COMMANDS:
            self.assertEqual(commands.count(command), 1, f"missing or duplicate DNS gate: {command}")
            self.assertNotIn(command, self._commands(handoff))

        ordered_pairs = (
            ("cargo test -p pythos-user-tcp-probe", "cargo test -p pythos-user-dns-probe"),
            ("cargo test -p pythos-user-dns-probe", "cargo test -p pythos-core dns_probe --features dns-probe"),
            ("python scripts/build-tcp-probe.py", "python scripts/build-dns-probe.py"),
            (
                "cargo clippy -p pythos-user-tcp-probe --target x86_64-unknown-none -- -D warnings",
                "cargo clippy -p pythos-core --target x86_64-unknown-none --features dns-probe -- -D warnings",
            ),
            (
                "cargo clippy -p pythos-core --target x86_64-unknown-none --features dns-probe -- -D warnings",
                "cargo clippy -p pythos-user-dns-probe --target x86_64-unknown-none -- -D warnings",
            ),
            (
                "python -m py_compile scripts/build-tcp-probe.py scripts/test-tcp.py",
                "python -m py_compile scripts/build-dns-probe.py scripts/test-dns.py",
            ),
            (
                "python -m py_compile scripts/build-dns-probe.py scripts/test-dns.py",
                "python -m unittest tests.test_dns",
            ),
            (
                "python -m unittest tests.test_dns",
                "python scripts/test-dns.py --self-test",
            ),
            (
                "python scripts/test-dns.py --self-test",
                "python scripts/test-dns.py",
            ),
        )
        for predecessor, successor in ordered_pairs:
            self.assertLess(
                commands.index(predecessor),
                commands.index(successor),
                f"DNS gate must follow predecessor: {predecessor}",
            )


    def test_pull_requests_do_not_also_run_feature_branch_push_acceptance(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        trigger_block = workflow[: workflow.index("\njobs:")]

        self.assertIn("push:\n    branches:\n      - main", trigger_block)
        self.assertIn("pull_request:", trigger_block)


if __name__ == "__main__":
    unittest.main()
