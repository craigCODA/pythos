from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
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
