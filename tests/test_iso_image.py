from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_build_iso_module():
    path = ROOT / "scripts" / "build-iso.py"
    spec = importlib.util.spec_from_file_location("build_iso", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load build-iso.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_build_image_module():
    path = ROOT / "scripts" / "build-image.py"
    spec = importlib.util.spec_from_file_location("build_image", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load build-image.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build_init_pak_with_shell(module) -> bytes:
    with tempfile.TemporaryDirectory() as temp:
        shell = Path(temp) / "pythos-user-shell"
        shell.write_bytes(b"\x7fELFshell")
        module.SHELL_ELF = shell
        return module.build_default_init_pak()


class IsoImageTest(unittest.TestCase):
    def test_generated_init_pak_contains_inner_bundle_records(self) -> None:
        for module in (load_build_image_module(), load_build_iso_module()):
            init_pak = build_init_pak_with_shell(module)
            payload = init_pak[module.INIT_PAK_HEADER_LEN :]
            record_count = 6
            table_start = module.INIT_BUNDLE_HEADER_LEN

            self.assertEqual(payload[:16], b"PYTHOS_BUNDLE_V0")
            self.assertEqual(int.from_bytes(payload[24:26], "little"), record_count)
            record_types = [
                int.from_bytes(
                    payload[
                        table_start
                        + index * module.INIT_BUNDLE_RECORD_LEN : table_start
                        + index * module.INIT_BUNDLE_RECORD_LEN
                        + 4
                    ],
                    "little",
                )
                for index in range(record_count)
            ]
            self.assertEqual(
                record_types,
                [
                    module.INIT_BUNDLE_RUNTIME_TYPE,
                    module.INIT_BUNDLE_NAMED_USER_ELF_TYPE,
                    module.INIT_BUNDLE_USER_ELF_TYPE,
                    module.INIT_BUNDLE_USER_ELF_TYPE,
                    module.INIT_BUNDLE_USER_ELF_TYPE,
                    module.INIT_BUNDLE_USER_ELF_TYPE,
                ],
            )

            for index, expected_magic in enumerate(
                (
                    module.RUNTIME_PAYLOAD_MAGIC,
                    module.NAMED_USER_PROGRAM_MAGIC,
                    b"\x7fELF",
                    b"\x7fELF",
                    b"\x7fELF",
                    b"\x7fELF",
                )
            ):
                entry = table_start + index * module.INIT_BUNDLE_RECORD_LEN
                start = int.from_bytes(payload[entry + 8 : entry + 16], "little")
                self.assertEqual(payload[start : start + len(expected_magic)], expected_magic)
            named_entry = table_start + module.INIT_BUNDLE_RECORD_LEN
            named_start = int.from_bytes(payload[named_entry + 8 : named_entry + 16], "little")
            name_start = named_start + module.NAMED_USER_PROGRAM_HEADER_LEN
            self.assertEqual(payload[name_start : name_start + len(b"shell.elf")], b"shell.elf")

    def test_iso_contains_el_torito_uefi_boot_catalog(self) -> None:
        build_iso = load_build_iso_module()

        with tempfile.TemporaryDirectory() as temp:
            temp_path = Path(temp)
            loader = temp_path / "BOOTX64.EFI"
            kernel = temp_path / "PYTHCORE.ELF"
            shell = temp_path / "pythos-user-shell"
            output = temp_path / "pythos.iso"
            loader.write_bytes(b"MZ" + bytes(4094))
            kernel.write_bytes(b"\x7fELF" + bytes(4092))
            shell.write_bytes(b"\x7fELFshell")
            build_iso.SHELL_ELF = shell

            build_iso.build_iso(output, loader, kernel)
            iso = output.read_bytes()

        self.assertIn(b"CD001", iso)
        self.assertIn(b"EL TORITO SPECIFICATION", iso)
        self.assertIn(b"BOOTX64.EFI;1", iso)
        self.assertIn(b"PYTHCORE.ELF;1", iso)

        boot_record = iso[17 * 2048 : 18 * 2048]
        catalog_lba = int.from_bytes(boot_record[71:75], "little")
        catalog = iso[catalog_lba * 2048 : (catalog_lba + 1) * 2048]

        self.assertEqual(catalog[0], 0x01)
        self.assertEqual(catalog[1], 0xEF)
        self.assertEqual(catalog[30:32], b"\x55\xaa")
        words = [
            int.from_bytes(catalog[index : index + 2], "little")
            for index in range(0, 32, 2)
        ]
        self.assertEqual(sum(words) & 0xFFFF, 0)
        self.assertEqual(catalog[32], 0x88)
        self.assertEqual(catalog[33], 0x00)
        boot_lba = int.from_bytes(catalog[40:44], "little")
        self.assertGreater(boot_lba, catalog_lba)
        self.assertEqual(iso[boot_lba * 2048 : boot_lba * 2048 + 3], b"\xeb\x3c\x90")


if __name__ == "__main__":
    unittest.main()
