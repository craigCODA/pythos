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


def build_init_pak_with_phase2_programs(module) -> bytes:
    with tempfile.TemporaryDirectory() as temp:
        temp_path = Path(temp)
        shell = temp_path / "pythos-user-shell"
        runtime = temp_path / "pythos-user-pyth-runtime"
        graph = temp_path / "hello.tig"
        budget_graph = temp_path / "budget.tig"
        invalid_graph = temp_path / "invalid.tig"
        unsupported_graph = temp_path / "unsupported.tig"
        invalid_string_graph = temp_path / "invalid-string.tig"
        parameterized_graph = temp_path / "parameterized.tig"
        shell.write_bytes(b"\x7fELFshell")
        runtime.write_bytes(b"\x7fELFpyth-runtime")
        graph.write_bytes(b"PYTHTIG1hello")
        budget_graph.write_bytes(b"PYTHTIG1budget")
        invalid_graph.write_bytes(b"PYTHTIG1invalid")
        unsupported_graph.write_bytes(b"PYTHTIG1unsupported")
        invalid_string_graph.write_bytes(b"PYTHTIG1invalid-string")
        parameterized_graph.write_bytes(b"PYTHTIG1parameterized")
        module.SHELL_ELF = shell
        module.PYTH_RUNTIME_ELF = runtime
        module.PYTH_GRAPH_PACKAGE = graph
        module.PYTH_BUDGET_GRAPH_PACKAGE = budget_graph
        module.PYTH_INVALID_GRAPH_PACKAGE = invalid_graph
        module.PYTH_UNSUPPORTED_GRAPH_PACKAGE = unsupported_graph
        module.PYTH_INVALID_STRING_GRAPH_PACKAGE = invalid_string_graph
        module.PYTH_PARAMETERIZED_GRAPH_PACKAGE = parameterized_graph
        return module.build_default_init_pak(include_pythtig=True)


def build_init_pak_without_phase2_programs(module) -> bytes:
    with tempfile.TemporaryDirectory() as temp:
        temp_path = Path(temp)
        shell = temp_path / "pythos-user-shell"
        shell.write_bytes(b"\x7fELFshell")
        module.SHELL_ELF = shell
        module.PYTH_RUNTIME_ELF = temp_path / "missing-pyth-runtime"
        module.PYTH_GRAPH_PACKAGE = temp_path / "missing-hello.tig"
        module.PYTH_BUDGET_GRAPH_PACKAGE = temp_path / "missing-budget.tig"
        module.PYTH_INVALID_GRAPH_PACKAGE = temp_path / "missing-invalid.tig"
        module.PYTH_UNSUPPORTED_GRAPH_PACKAGE = temp_path / "missing-unsupported.tig"
        module.PYTH_INVALID_STRING_GRAPH_PACKAGE = temp_path / "missing-invalid-string.tig"
        module.PYTH_PARAMETERIZED_GRAPH_PACKAGE = temp_path / "missing-parameterized.tig"
        return module.build_default_init_pak()


class IsoImageTest(unittest.TestCase):
    def test_default_init_pak_excludes_phase2_runtime_and_graphs(self) -> None:
        for module in (load_build_image_module(), load_build_iso_module()):
            init_pak = build_init_pak_without_phase2_programs(module)
            payload = init_pak[module.INIT_PAK_HEADER_LEN :]
            record_count = int.from_bytes(payload[24:26], "little")
            table_start = module.INIT_BUNDLE_HEADER_LEN
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

            self.assertEqual(record_count, 6)
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
            self.assertNotIn(module.INIT_BUNDLE_PYTH_GRAPH_TYPE, record_types)

    def test_phase2_opt_in_requires_runtime_artifact(self) -> None:
        for module in (load_build_image_module(), load_build_iso_module()):
            with tempfile.TemporaryDirectory() as temp:
                temp_path = Path(temp)
                shell = temp_path / "pythos-user-shell"
                shell.write_bytes(b"\x7fELFshell")
                module.SHELL_ELF = shell
                module.PYTH_RUNTIME_ELF = temp_path / "missing-pyth-runtime"

                with self.assertRaisesRegex(SystemExit, "missing PythTIG runtime ELF"):
                    module.build_default_init_pak(include_pythtig=True)

    def test_generated_init_pak_contains_inner_bundle_records(self) -> None:
        for module in (load_build_image_module(), load_build_iso_module()):
            init_pak = build_init_pak_with_phase2_programs(module)
            payload = init_pak[module.INIT_PAK_HEADER_LEN :]
            record_count = 13
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
                    module.INIT_BUNDLE_NAMED_USER_ELF_TYPE,
                    module.INIT_BUNDLE_PYTH_GRAPH_TYPE,
                    module.INIT_BUNDLE_PYTH_GRAPH_TYPE,
                    module.INIT_BUNDLE_PYTH_GRAPH_TYPE,
                    module.INIT_BUNDLE_PYTH_GRAPH_TYPE,
                    module.INIT_BUNDLE_PYTH_GRAPH_TYPE,
                    module.INIT_BUNDLE_PYTH_GRAPH_TYPE,
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
                    module.NAMED_USER_PROGRAM_MAGIC,
                    module.NAMED_PYTH_GRAPH_MAGIC,
                    module.NAMED_PYTH_GRAPH_MAGIC,
                    module.NAMED_PYTH_GRAPH_MAGIC,
                    module.NAMED_PYTH_GRAPH_MAGIC,
                    module.NAMED_PYTH_GRAPH_MAGIC,
                    module.NAMED_PYTH_GRAPH_MAGIC,
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
            runtime_entry = table_start + 2 * module.INIT_BUNDLE_RECORD_LEN
            runtime_start = int.from_bytes(payload[runtime_entry + 8 : runtime_entry + 16], "little")
            runtime_name_start = runtime_start + module.NAMED_USER_PROGRAM_HEADER_LEN
            self.assertEqual(
                payload[runtime_name_start : runtime_name_start + len(b"pyth-runtime.elf")],
                b"pyth-runtime.elf",
            )
            graph_entry = table_start + 3 * module.INIT_BUNDLE_RECORD_LEN
            graph_start = int.from_bytes(payload[graph_entry + 8 : graph_entry + 16], "little")
            graph_name_start = graph_start + module.NAMED_PYTH_GRAPH_HEADER_LEN
            self.assertEqual(
                payload[graph_name_start : graph_name_start + len(b"hello.tig")],
                b"hello.tig",
            )
            for graph_index, expected_name in (
                (4, b"budget.tig"),
                (5, b"invalid.tig"),
                (6, b"unsupported.tig"),
                (7, b"invalid-string.tig"),
                (8, b"parameterized.tig"),
            ):
                graph_entry = table_start + graph_index * module.INIT_BUNDLE_RECORD_LEN
                graph_start = int.from_bytes(
                    payload[graph_entry + 8 : graph_entry + 16], "little"
                )
                graph_name_start = graph_start + module.NAMED_PYTH_GRAPH_HEADER_LEN
                self.assertEqual(
                    payload[
                        graph_name_start : graph_name_start + len(expected_name)
                    ],
                    expected_name,
                )

    def test_iso_contains_el_torito_uefi_boot_catalog(self) -> None:
        build_iso = load_build_iso_module()

        with tempfile.TemporaryDirectory() as temp:
            temp_path = Path(temp)
            loader = temp_path / "BOOTX64.EFI"
            kernel = temp_path / "PYTHCORE.ELF"
            shell = temp_path / "pythos-user-shell"
            runtime = temp_path / "pythos-user-pyth-runtime"
            graph = temp_path / "hello.tig"
            budget_graph = temp_path / "budget.tig"
            invalid_graph = temp_path / "invalid.tig"
            unsupported_graph = temp_path / "unsupported.tig"
            invalid_string_graph = temp_path / "invalid-string.tig"
            parameterized_graph = temp_path / "parameterized.tig"
            output = temp_path / "pythos.iso"
            loader.write_bytes(b"MZ" + bytes(4094))
            kernel.write_bytes(b"\x7fELF" + bytes(4092))
            shell.write_bytes(b"\x7fELFshell")
            runtime.write_bytes(b"\x7fELFpyth-runtime")
            graph.write_bytes(b"PYTHTIG1hello")
            budget_graph.write_bytes(b"PYTHTIG1budget")
            invalid_graph.write_bytes(b"PYTHTIG1invalid")
            unsupported_graph.write_bytes(b"PYTHTIG1unsupported")
            invalid_string_graph.write_bytes(b"PYTHTIG1invalid-string")
            parameterized_graph.write_bytes(b"PYTHTIG1parameterized")
            build_iso.SHELL_ELF = shell
            build_iso.PYTH_RUNTIME_ELF = runtime
            build_iso.PYTH_GRAPH_PACKAGE = graph
            build_iso.PYTH_BUDGET_GRAPH_PACKAGE = budget_graph
            build_iso.PYTH_INVALID_GRAPH_PACKAGE = invalid_graph
            build_iso.PYTH_UNSUPPORTED_GRAPH_PACKAGE = unsupported_graph
            build_iso.PYTH_INVALID_STRING_GRAPH_PACKAGE = invalid_string_graph
            build_iso.PYTH_PARAMETERIZED_GRAPH_PACKAGE = parameterized_graph

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
