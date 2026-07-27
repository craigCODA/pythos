from __future__ import annotations

import importlib.util
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
    spec.loader.exec_module(module)
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

        module.build_boot_image()

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-image.py")

    def test_makefile_image_and_iso_targets_depend_on_verified_shell(self) -> None:
        makefile = (ROOT / "Makefile").read_text(encoding="utf-8")

        self.assertIn("build-user-shell:", makefile)
        self.assertIn("verify-user-shell: build-user-shell", makefile)
        self.assertIn("image: build-loader build-core verify-user-shell", makefile)
        self.assertIn("iso: build-loader build-core verify-user-shell", makefile)


if __name__ == "__main__":
    unittest.main()
