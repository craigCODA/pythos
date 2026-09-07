from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REQUIRED_FILES = {
    "EFI/BOOT/BOOTX64.EFI": b"unique-loader-payload",
    "PYTHOS/PYTHCORE.ELF": b"unique-kernel-payload",
    "PYTHOS/BOOT.CFG": b"unique-config-payload",
    "PYTHOS/INIT.PAK": b"unique-init-payload",
    "PYTHOS/FONT.PSF": b"unique-font-payload",
}


def load_run_qemu_module():
    path = ROOT / "scripts" / "run-qemu.py"
    spec = importlib.util.spec_from_file_location("run_qemu_boot_media", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load run-qemu.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_esp(root: Path) -> None:
    for relative, payload in REQUIRED_FILES.items():
        path = root / Path(relative)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(payload)


class QemuBootMediaTest(unittest.TestCase):
    def test_esp_image_is_deterministic_read_only_input_without_nvvars(self) -> None:
        run_qemu = load_run_qemu_module()
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            esp = root / "esp"
            write_esp(esp)
            nvvars_sentinel = b"generated-nvvars-must-not-enter-image"
            (esp / "NvVars").write_bytes(nvvars_sentinel)

            first = run_qemu.prepare_esp_image(esp, root / "first.img")
            second = run_qemu.prepare_esp_image(esp, root / "second.img")
            first_bytes = first.read_bytes()
            second_bytes = second.read_bytes()

            self.assertEqual(len(first_bytes), 16 * 1024 * 1024)
            self.assertEqual(first_bytes, second_bytes)
            self.assertEqual(first_bytes[54:62], b"FAT16   ")
            for payload in REQUIRED_FILES.values():
                self.assertIn(payload, first_bytes)
            self.assertNotIn(nvvars_sentinel, first_bytes)

    def test_esp_image_fails_closed_when_a_required_artifact_is_missing(self) -> None:
        run_qemu = load_run_qemu_module()
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            esp = root / "esp"
            write_esp(esp)
            (esp / "PYTHOS" / "INIT.PAK").unlink()

            with self.assertRaisesRegex(FileNotFoundError, "PYTHOS/INIT.PAK"):
                run_qemu.prepare_esp_image(esp, root / "boot.img")

    def test_drive_arguments_separate_firmware_state_and_isolate_esp_writes(self) -> None:
        run_qemu = load_run_qemu_module()

        arguments = run_qemu.qemu_firmware_and_esp_args(
            Path("OVMF_CODE.fd"),
            Path("run-esp.img"),
        )

        self.assertEqual(
            arguments,
            [
                "-drive",
                "if=pflash,format=raw,unit=0,readonly=on,file=OVMF_CODE.fd",
                "-drive",
                "if=none,id=pythos_esp,format=raw,snapshot=on,file=run-esp.img",
                "-device",
                "ide-hd,drive=pythos_esp,bootindex=1",
            ],
        )
        self.assertNotIn("fat:rw:", " ".join(arguments))
        self.assertEqual(" ".join(arguments).count("if=pflash"), 1)


if __name__ == "__main__":
    unittest.main()
