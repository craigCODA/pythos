from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
import unittest.mock
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_script(name: str):
    path = ROOT / "scripts" / name
    module_name = name.replace("-", "_").replace(".", "_")
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {name}")
    module = importlib.util.module_from_spec(spec)
    scripts_dir = str(ROOT / "scripts")
    inserted_scripts_dir = scripts_dir not in sys.path
    if inserted_scripts_dir:
        sys.path.insert(0, scripts_dir)
    previous_module = sys.modules.get(module_name)
    sys.modules[module_name] = module
    try:
        spec.loader.exec_module(module)
    finally:
        if previous_module is None:
            sys.modules.pop(module_name, None)
        else:
            sys.modules[module_name] = previous_module
        if inserted_scripts_dir:
            sys.path.remove(scripts_dir)
    return module


def normalize(command: list[object]) -> list[str]:
    return [str(part).replace("\\", "/") for part in command]


class BuildOrchestrationTest(unittest.TestCase):
    def test_relative_probe_identity_is_resolved_once_before_any_packaging_mutation(self) -> None:
        module = load_script("build-image.py")

        def invoke(verifier_returncode: int, probe_argument: str) -> list[tuple[str, object]]:
            events: list[tuple[str, object]] = []
            with tempfile.TemporaryDirectory() as temp_dir:
                root = Path(temp_dir) / "repository"
                caller = Path(temp_dir) / "caller"
                root.mkdir()
                caller.mkdir()
                loader = caller / "BOOTX64.EFI"
                kernel = caller / "PYTHCORE.ELF"
                root_probe = root / "probe.elf"
                caller_probe = caller / "probe.elf"
                for path, content in (
                    (loader, b"loader"),
                    (kernel, b"kernel"),
                    (root_probe, b"root-probe"),
                    (caller_probe, b"caller-probe"),
                ):
                    path.write_bytes(content)

                def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                    events.append(("verify", normalize(command)))
                    return subprocess.CompletedProcess(command, verifier_returncode)

                def package(*args: object) -> bytes:
                    events.append(("package", args[-1]))
                    return b"pak"

                def mkdir(*_args: object, **_kwargs: object) -> None:
                    events.append(("mkdir", None))

                previous_cwd = Path.cwd()
                try:
                    os.chdir(caller)
                    with unittest.mock.patch.object(module, "ROOT", root), unittest.mock.patch.object(
                        module, "ESP", caller / "esp"
                    ), unittest.mock.patch.object(module, "build_default_init_pak", side_effect=package), unittest.mock.patch.object(
                        module.shutil, "copy2", side_effect=lambda *_args: events.append(("copy", None))
                    ), unittest.mock.patch.object(
                        module, "write_binary_if_changed", side_effect=lambda *_args: events.append(("write", None))
                    ), unittest.mock.patch.object(Path, "mkdir", side_effect=mkdir), unittest.mock.patch.object(
                        subprocess, "run", side_effect=verify
                    ), unittest.mock.patch.object(
                        sys,
                        "argv",
                        [
                            str(module.__file__),
                            "--loader",
                            str(loader),
                            "--kernel",
                            str(kernel),
                            "--session-input-probe-elf",
                            probe_argument,
                        ],
                    ):
                        if verifier_returncode == 0 and probe_argument == "probe.elf":
                            self.assertEqual(module.main(), 0)
                            packaged = next(value for kind, value in events if kind == "package")
                            events.append(
                                (
                                    "identity",
                                    (
                                        Path(events[0][1][-1]).samefile(caller_probe),
                                        Path(packaged).samefile(caller_probe),
                                    ),
                                )
                            )
                        else:
                            with self.assertRaises(SystemExit):
                                module.main()
                finally:
                    os.chdir(previous_cwd)
            return events

        success = invoke(0, "probe.elf")
        self.assertEqual(success[0][0], "verify")
        verified = success[0][1][-1]
        packaged = next(value for kind, value in success if kind == "package")
        self.assertTrue(Path(verified).is_absolute())
        self.assertTrue(Path(packaged).is_absolute())
        self.assertEqual(next(value for kind, value in success if kind == "identity"), (True, True))
        self.assertTrue(all(kind != "verify" for kind, _value in success[1:]))

        failure = invoke(1, "probe.elf")
        self.assertEqual([kind for kind, _value in failure], ["verify"])

        missing = invoke(0, "missing.elf")
        self.assertEqual(missing, [])

    def test_probe_elf_verification_precedes_packaging_and_failure_short_circuits(self) -> None:
        module = load_script("build-image.py")

        def invoke(verifier_returncode: int, include_probe: bool) -> list[tuple[str, object]]:
            events: list[tuple[str, object]] = []
            with tempfile.TemporaryDirectory() as temp_dir:
                root = Path(temp_dir)
                loader = root / "BOOTX64.EFI"
                kernel = root / "PYTHCORE.ELF"
                probe = root / "session-input-probe.elf"
                for path in (loader, kernel, probe):
                    path.write_bytes(b"artifact")

                def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                    events.append(("verify", normalize(command)))
                    return subprocess.CompletedProcess(command, verifier_returncode)

                def mkdir(*args: object, **_kwargs: object) -> None:
                    events.append(("mkdir", None))

                arguments = [
                    str(module.__file__),
                    "--loader",
                    str(loader),
                    "--kernel",
                    str(kernel),
                ]
                if include_probe:
                    arguments.extend(["--session-input-probe-elf", str(probe)])

                with unittest.mock.patch.object(module, "ESP", root / "esp"), unittest.mock.patch.object(
                    module, "build_default_init_pak", return_value=b"pak"
                ), unittest.mock.patch.object(module.shutil, "copy2", side_effect=lambda *_args: events.append(("copy", None))), unittest.mock.patch.object(
                    module, "write_binary_if_changed", side_effect=lambda *_args: events.append(("write", None))
                ), unittest.mock.patch.object(Path, "mkdir", side_effect=mkdir), unittest.mock.patch.object(
                    subprocess, "run", side_effect=verify
                ), unittest.mock.patch.object(
                    sys,
                    "argv",
                    arguments,
                ):
                    if verifier_returncode == 0 or not include_probe:
                        self.assertEqual(module.main(), 0)
                    else:
                        with self.assertRaises(SystemExit):
                            module.main()
            return events

        default = invoke(1, False)
        self.assertTrue(all(kind != "verify" for kind, _value in default))

        success = invoke(0, True)
        self.assertEqual(success[0][0], "verify")
        self.assertEqual(
            success[0][1][-2:],
            ["--elf", success[0][1][-1]],
        )
        self.assertTrue(all(kind != "verify" for kind, _value in success[1:]))
        self.assertTrue(any(kind in {"mkdir", "copy", "write"} for kind, _value in success[1:]))

        failure = invoke(1, True)
        self.assertEqual(len(failure), 1)
        self.assertEqual(failure[0][0], "verify")
        self.assertEqual(failure[0][1][-2:], ["--elf", failure[0][1][-1]])

    def test_session_input_probe_build_is_isolated_and_uses_its_own_linker(self) -> None:
        module = load_script("build-session-input-probe.py")
        calls: list[tuple[list[object], dict[str, object]]] = []
        module.subprocess.call = lambda command, **kwargs: calls.append((command, kwargs)) or 0

        target_dir = ROOT / "target" / "probe-test"
        with unittest.mock.patch.object(sys, "argv", [str(module.__file__), "--target-dir", str(target_dir)]):
            self.assertEqual(module.main(), 0)

        command, kwargs = calls[0]
        normalized = normalize(command)
        self.assertIn("--target-dir", normalized)
        self.assertEqual(normalized[normalized.index("--target-dir") + 1], str(target_dir).replace("\\", "/"))
        self.assertIn("session-input/linker.ld", str(kwargs["env"]["RUSTFLAGS"]).replace("\\", "/"))

    def test_session_runtime_build_is_isolated_and_uses_only_its_own_linker(self) -> None:
        # Catches building the retained runtime with the generic/probe linker or shared target state.
        self.assertTrue(
            (ROOT / "scripts" / "build-session-runtime.py").is_file(),
            "the isolated session-runtime builder must exist",
        )
        module = load_script("build-session-runtime.py")
        calls: list[tuple[list[object], dict[str, object]]] = []
        module.subprocess.call = lambda command, **kwargs: calls.append((command, kwargs)) or 0

        target_dir = ROOT / "target" / "session-runtime-test"
        with unittest.mock.patch.object(
            sys,
            "argv",
            [str(module.__file__), "--target-dir", str(target_dir)],
        ):
            self.assertEqual(module.main(), 0)

        self.assertEqual(len(calls), 1)
        command, kwargs = calls[0]
        normalized = normalize(command)
        self.assertEqual(
            normalized[:7],
            [
                "cargo",
                "build",
                "-p",
                "pythos-user-session-runtime",
                "--target",
                "x86_64-unknown-none",
                "--target-dir",
            ],
        )
        self.assertEqual(normalized[7], str(target_dir).replace("\\", "/"))
        rustflags = str(kwargs["env"]["RUSTFLAGS"]).replace("\\", "/")
        self.assertIn("user/session-runtime/linker.ld", rustflags)
        self.assertNotIn("user/pyth-runtime/linker.ld", rustflags)
        self.assertNotIn("user/probes/session-input/linker.ld", rustflags)

    def test_session_input_probe_opt_in_record_has_exact_identity_and_default_stays_unchanged(self) -> None:
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            probe = root / "probe.elf"
            shell.write_bytes(b"shell")
            probe.write_bytes(b"probe")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                module, "build_runtime_payload", return_value=b"runtime"
            ):
                default = module.build_default_init_pak()
                opted_in = module.build_default_init_pak(session_input_probe_elf=probe)

                expected_default = module.build_init_pak(
                    module.build_init_bundle(
                        [
                            (module.INIT_BUNDLE_RUNTIME_TYPE, b"runtime"),
                            (
                                module.INIT_BUNDLE_NAMED_USER_ELF_TYPE,
                                module.build_named_user_program(
                                    b"shell.elf", module.SHELL_PRINCIPAL_ID, b"shell"
                                ),
                            ),
                            (module.INIT_BUNDLE_USER_ELF_TYPE, module.build_user_elf_payload(b"\xCC\xF4")),
                            (module.INIT_BUNDLE_USER_ELF_TYPE, module.build_user_elf_payload(b"\x0F\x0B\xF4")),
                            (
                                module.INIT_BUNDLE_USER_ELF_TYPE,
                                module.build_user_elf_payload(
                                    b"\x48\xB8" + (0).to_bytes(8, "little") + b"\x8A\x00\xF4"
                                ),
                            ),
                            (module.INIT_BUNDLE_USER_ELF_TYPE, module.build_user_elf_payload(b"\xBA\xF8\x03\x00\x00\xEC\xF4")),
                        ]
                    )
                )

        self.assertEqual(default, expected_default)
        self.assertNotIn(b"session-input-probe.elf", default)
        self.assertIn(b"session-input-probe.elf", opted_in)
        self.assertIn(module.SESSION_INPUT_PROBE_PRINCIPAL_ID.to_bytes(8, "little"), opted_in)
        self.assertIn(module.digest64(b"probe").to_bytes(8, "little"), opted_in)

    def assert_shell_build_verify_before_packaging(
        self, commands: list[list[object]], packaging_script: str
    ) -> None:
        normalized = [normalize(command) for command in commands]

        build_shell = next(
            index
            for index, command in enumerate(normalized)
            if command[-1] == "scripts/build-user-shell.py"
        )
        verify_shell = next(
            index
            for index, command in enumerate(normalized)
            if command[-1] == "scripts/verify-user-elf.py"
        )
        package = next(
            index
            for index, command in enumerate(normalized)
            if packaging_script in command
        )

        self.assertLess(build_shell, verify_shell)
        self.assertLess(verify_shell, package)

    def test_test_boot_prepares_verified_shell_before_esp_packaging(self) -> None:
        module = load_script("test-boot.py")
        calls: list[list[object]] = []
        module.run = calls.append

        module.build_boot_artifacts("esp")

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-image.py")

    def test_test_boot_prepares_verified_shell_before_iso_packaging(self) -> None:
        module = load_script("test-boot.py")
        calls: list[list[object]] = []
        module.run = calls.append

        module.build_boot_artifacts("iso")

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-iso.py")

    def test_normal_fast_boot_prepares_verified_shell_before_packaging(self) -> None:
        module = load_script("test-normal-fast-boot.py")
        calls: list[list[object]] = []
        module.run = lambda command, expected=0: calls.append(command) or ""

        module.build_boot_image()

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-image.py")

    def test_persistent_storage_prepares_verified_shell_before_packaging(self) -> None:
        module = load_script("test-persistent-storage.py")
        calls: list[list[object]] = []
        module.run = lambda command, expected_returncode=0: calls.append(command) or ""

        module.build_boot_image()

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-image.py")

    def test_com2_transport_prepares_verified_shell_before_packaging(self) -> None:
        module = load_script("test-com2-shell-transport.py")
        calls: list[list[object]] = []
        module.run = calls.append

        module.build_boot_image()

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-image.py")

    def test_object_shell_prepares_verified_shell_before_packaging(self) -> None:
        module = load_script("test-object-shell.py")
        calls: list[list[object]] = []
        module.run = calls.append

        module.build_boot_image(module.backend_config("virtio"))

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-image.py")

    def test_pyth_graph_runtime_uses_test_feature_and_opt_in_bundle(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_boot_image()

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        package = next(
            command
            for command in normalized
            if "scripts/build-image.py" in command
        )
        self.assertIn("pythtig-phase2-test", core_build)
        self.assertIn("--with-pythtig", package)

    def test_pyth_graph_runtime_uses_isolated_test_core_artifact(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_boot_image()
        module.rebuild_image_with_current_runtime()

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        packages = [
            command for command in normalized if "scripts/build-image.py" in command
        ]
        self.assertIn("--target-dir", core_build)
        target_dir = core_build[core_build.index("--target-dir") + 1]
        expected_kernel = (
            f"{target_dir}/x86_64-unknown-none/debug/pythcore"
        )
        for package in packages:
            self.assertIn("--kernel", package)
            self.assertEqual(package[package.index("--kernel") + 1], expected_kernel)

    def test_pyth_graph_object_flow_uses_test_feature_and_opt_in_bundle(self) -> None:
        module = load_script("test-pyth-graph-object-flow.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_boot_image()

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        package = next(
            command
            for command in normalized
            if "scripts/build-image.py" in command
        )
        self.assertIn("pythtig-phase2-test", core_build)
        self.assertIn("--with-pythtig-object-flow", package)

    def test_pyth_graph_object_flow_uses_isolated_test_core_artifact(self) -> None:
        module = load_script("test-pyth-graph-object-flow.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_boot_image()

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        package = next(
            command for command in normalized if "scripts/build-image.py" in command
        )
        self.assertIn("--target-dir", core_build)
        target_dir = core_build[core_build.index("--target-dir") + 1]
        expected_kernel = f"{target_dir}/x86_64-unknown-none/debug/pythcore"
        self.assertIn("--kernel", package)
        self.assertEqual(package[package.index("--kernel") + 1], expected_kernel)

    def test_pyth_native_codegen_uses_isolated_test_core_artifact(self) -> None:
        module = load_script("test-pyth-native-codegen.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_base_artifacts()
        module.build_interpreter_image("--with-pythtig")
        module.build_native_image(Path("target/pyth-native/hello.elf"))

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        packages = [
            command for command in normalized if "scripts/build-image.py" in command
        ]
        self.assertIn("--target-dir", core_build)
        target_dir = core_build[core_build.index("--target-dir") + 1]
        expected_kernel = f"{target_dir}/x86_64-unknown-none/debug/pythcore"
        for package in packages:
            self.assertIn("--kernel", package)
            self.assertEqual(package[package.index("--kernel") + 1], expected_kernel)

    def test_pyth_cross_target_uses_isolated_test_core_artifact(self) -> None:
        module = load_script("pyth_cross_target.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_pythtig_hello_image()

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        package = next(
            command for command in normalized if "scripts/build-image.py" in command
        )
        self.assertIn("--target-dir", core_build)
        target_dir = core_build[core_build.index("--target-dir") + 1]
        expected_kernel = f"{target_dir}/x86_64-unknown-none/debug/pythcore"
        self.assertIn("--kernel", package)
        self.assertEqual(package[package.index("--kernel") + 1], expected_kernel)

    def test_pyth_graph_runtime_copies_source_esp_for_each_scenario(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = Path(temp_dir)
            module.TARGET = temp_root / "target"
            module.ESP = temp_root / "source-esp"
            boot_file = module.ESP / "EFI" / "BOOT" / "BOOTX64.EFI"
            boot_file.parent.mkdir(parents=True)
            boot_file.write_bytes(b"scenario-isolation")

            module.run_qemu("success", module.CONTROL_LAUNCH_HELLO, "success")
            module.run_qemu("invalid", module.CONTROL_LAUNCH_INVALID, "invalid")

            success_esp = module.TARGET / "pyth-graph-runtime-success-esp"
            invalid_esp = module.TARGET / "pyth-graph-runtime-invalid-esp"
            self.assertTrue(success_esp.is_dir())
            self.assertTrue(invalid_esp.is_dir())
            self.assertEqual(
                (success_esp / "EFI" / "BOOT" / "BOOTX64.EFI").read_bytes(),
                b"scenario-isolation",
            )
            self.assertEqual(
                (invalid_esp / "EFI" / "BOOT" / "BOOTX64.EFI").read_bytes(),
                b"scenario-isolation",
            )
            self.assertNotEqual(success_esp, invalid_esp)

    def test_pyth_graph_runtime_passes_scenario_esp_to_qemu(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = Path(temp_dir)
            module.TARGET = temp_root / "target"
            module.ESP = temp_root / "source-esp"
            module.ESP.mkdir()

            module.run_qemu("invalid", module.CONTROL_LAUNCH_INVALID, "rejected")

            command = normalize(calls[-1])
            self.assertIn("--esp", command)
            esp_index = command.index("--esp")
            self.assertEqual(
                command[esp_index + 1],
                str(module.TARGET / "pyth-graph-runtime-invalid-esp").replace(
                    "\\", "/"
                ),
            )

    def test_pyth_graph_object_flow_isolates_esp_but_reuses_storage(self) -> None:
        module = load_script("test-pyth-graph-object-flow.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = Path(temp_dir)
            module.TARGET = temp_root / "target"
            module.ESP = temp_root / "source-esp"
            module.STORAGE_IMAGE = temp_root / "target" / "object-flow.img"
            boot_file = module.ESP / "EFI" / "BOOT" / "BOOTX64.EFI"
            boot_file.parent.mkdir(parents=True)
            boot_file.write_bytes(b"object-flow-isolation")
            module.prepare_fresh_storage_image(module.STORAGE_IMAGE)

            module.run_qemu("create", module.CONTROL_LAUNCH_OBJECT_CREATE)
            module.run_qemu("restore", module.CONTROL_LAUNCH_OBJECT_RESTORE)

            create_esp = module.TARGET / "pyth-graph-object-flow-create-esp"
            restore_esp = module.TARGET / "pyth-graph-object-flow-restore-esp"
            self.assertTrue(create_esp.is_dir())
            self.assertTrue(restore_esp.is_dir())
            self.assertNotEqual(create_esp, restore_esp)
            self.assertEqual(
                (create_esp / "EFI" / "BOOT" / "BOOTX64.EFI").read_bytes(),
                b"object-flow-isolation",
            )
            self.assertEqual(
                (restore_esp / "EFI" / "BOOT" / "BOOTX64.EFI").read_bytes(),
                b"object-flow-isolation",
            )

            create_command = normalize(calls[0])
            restore_command = normalize(calls[1])
            self.assertIn("--storage-image", create_command)
            self.assertIn("--storage-image", restore_command)
            self.assertEqual(
                create_command[create_command.index("--storage-image") + 1],
                str(module.STORAGE_IMAGE).replace("\\", "/"),
            )
            self.assertEqual(
                restore_command[restore_command.index("--storage-image") + 1],
                str(module.STORAGE_IMAGE).replace("\\", "/"),
            )

    def test_pyth_graph_runtime_negative_assertions_require_pre_entry_rejection(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        invalid_string = "\n".join(
            (
                "PYTHOS:LOADER:ENTER",
                "PYTHOS:PYTHTIG:PACKAGE_REJECTED error:VERIFY_NONCANONICAL_ENCODING",
            )
        )
        parameterized = "\n".join(
            (
                "PYTHOS:LOADER:ENTER",
                "PYTHOS:PYTHTIG:PACKAGE_REJECTED error:UNSUPPORTED_PHASE2_CONTROL_FLOW",
            )
        )

        module.assert_invalid_string_rejected(invalid_string)
        module.assert_parameterized_jump_rejected(parameterized)
        with self.assertRaises(AssertionError):
            module.assert_parameterized_jump_rejected(
                parameterized + "\nPYTHOS:PYTHTIG:RUNTIME_ENTER package:0000000000000000"
            )

    def test_pyth_graph_fault_assertion_rejects_false_peer_claim(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        prefix = "\n".join(
            (
                "PYTHOS:PYTHTIG:RUNTIME_ENTER package:0000000000000001",
                "PYTHOS:CORE:CRASH:USER_FAULT",
                "PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED principal:5059544852540001",
            )
        )

        module.assert_fault_contained(
            prefix + "\nPYTHOS:PYTHTIG:RUNTIME_FAULT_SAFE_IDLE"
        )
        with self.assertRaises(AssertionError):
            module.assert_fault_contained(prefix + "\nPYTHOS:CORE:CRASH:PEER_ALIVE")

    def test_pyth_graph_success_assertion_requires_termination_transition(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        exit_only = "\n".join(
            (
                "PYTHOS:PYTHTIG:PACKAGE_VALID package:0000000000000001 nodes:5 blocks:1",
                "PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:5059544847520001 imports:1",
                "PYTHOS:PYTHTIG:RUNTIME_ENTER package:0000000000000001",
                "PYTHOS:PYTHTIG:PROGRAM_LOG",
                "PYTHOS:PYTHTIG:RUNTIME_EXIT status:0",
            )
        )

        with self.assertRaises(AssertionError):
            module.assert_pyth_tig_success(exit_only)
        module.assert_pyth_tig_success(
            exit_only
            + "\nPYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001"
        )

    def test_pyth_native_equivalence_normalizes_process_identity_boundary(self) -> None:
        module = load_script("test-pyth-native-codegen.py")
        interpreter = [
            "PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001",
        ]
        native = [
            "PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544847520001",
        ]

        self.assertEqual(
            module.normalized_pythtig_trace(interpreter),
            module.normalized_pythtig_trace(native),
        )

    def test_pyth_graph_object_flow_assertions_require_runtime_entry_and_termination(self) -> None:
        module = load_script("test-pyth-graph-object-flow.py")
        valid = "\n".join(
            (
                "PYTHOS:LOADER:ENTER",
                "PYTHOS:PYTHTIG:PACKAGE_VALID package:0000000000000001 nodes:11 blocks:1",
                "PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:5059544847520006 imports:1",
                "PYTHOS:PYTHTIG:RUNTIME_ENTER package:0000000000000001",
                "PYTHOS:PYTHTIG:OBJECT_CREATED object:1042 revision:1",
                "PYTHOS:PYTHTIG:OBJECT_REVISED object:1042 revision:2",
                "PYTHOS:PYTHTIG:OBJECT_INSPECTED object:1042 revision:2",
                "PYTHOS:PYTHTIG:RUNTIME_EXIT status:0",
                "PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001",
            )
        )

        module.assert_object_create_flow(valid)
        with self.assertRaises(AssertionError):
            module.assert_object_create_flow(valid.replace("PYTHOS:LOADER:ENTER\n", ""))
        with self.assertRaises(AssertionError):
            module.assert_object_create_flow(
                valid.replace(
                    "\nPYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001",
                    "",
                )
            )

    def test_makefile_image_and_iso_targets_depend_on_verified_shell(self) -> None:
        makefile = (ROOT / "Makefile").read_text(encoding="utf-8")

        self.assertIn("build-user-shell:", makefile)
        self.assertIn("verify-user-shell: build-user-shell", makefile)
        self.assertIn("image: build-loader build-core verify-user-shell", makefile)
        self.assertIn("iso: build-loader build-core verify-user-shell", makefile)


if __name__ == "__main__":
    unittest.main()
