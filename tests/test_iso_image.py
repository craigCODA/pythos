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


class IsoImageTest(unittest.TestCase):
    def test_iso_contains_el_torito_uefi_boot_catalog(self) -> None:
        build_iso = load_build_iso_module()

        with tempfile.TemporaryDirectory() as temp:
            temp_path = Path(temp)
            loader = temp_path / "BOOTX64.EFI"
            kernel = temp_path / "PYTHCORE.ELF"
            output = temp_path / "pythos.iso"
            loader.write_bytes(b"MZ" + bytes(4094))
            kernel.write_bytes(b"\x7fELF" + bytes(4092))

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
