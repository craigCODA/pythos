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


def build_init_pak_with_phase3_object_programs(module) -> bytes:
    with tempfile.TemporaryDirectory() as temp:
        temp_path = Path(temp)
        shell = temp_path / "pythos-user-shell"
        runtime = temp_path / "pythos-user-pyth-runtime"
        object_create = temp_path / "object-create.tig"
        object_restore = temp_path / "object-restore.tig"
        object_known_denied = temp_path / "object-known-denied.tig"
        object_forgery = temp_path / "object-forgery.tig"
        shell.write_bytes(b"\x7fELFshell")
        runtime.write_bytes(b"\x7fELFpyth-runtime")
        object_create.write_bytes(b"PYTHTIG1object-create")
        object_restore.write_bytes(b"PYTHTIG1object-restore")
        object_known_denied.write_bytes(b"PYTHTIG1object-known-denied")
        object_forgery.write_bytes(b"PYTHTIG1object-forgery")
        module.SHELL_ELF = shell
        module.PYTH_RUNTIME_ELF = runtime
        module.PYTH_OBJECT_CREATE_GRAPH_PACKAGE = object_create
        module.PYTH_OBJECT_RESTORE_GRAPH_PACKAGE = object_restore
        module.PYTH_OBJECT_KNOWN_DENIED_GRAPH_PACKAGE = object_known_denied
        module.PYTH_OBJECT_FORGERY_GRAPH_PACKAGE = object_forgery
        return module.build_default_init_pak(include_pythtig_object_flow=True)


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


def native_elf_fixture() -> bytes:
    text_offset = 0x1000
    data_offset = 0x2000
    elf = bytearray(data_offset + 8)
    elf[:4] = b"\x7fELF"
    elf[4:7] = bytes((2, 1, 1))
    elf[16:18] = (2).to_bytes(2, "little")
    elf[18:20] = (0x3E).to_bytes(2, "little")
    elf[20:24] = (1).to_bytes(4, "little")
    elf[24:32] = (0x0040_0000).to_bytes(8, "little")
    elf[32:40] = (64).to_bytes(8, "little")
    elf[52:54] = (64).to_bytes(2, "little")
    elf[54:56] = (56).to_bytes(2, "little")
    elf[56:58] = (2).to_bytes(2, "little")
    elf[58:60] = (64).to_bytes(2, "little")

    def load(index: int, flags: int, offset: int, address: int, size: int) -> None:
        entry = 64 + index * 56
        elf[entry : entry + 4] = (1).to_bytes(4, "little")
        elf[entry + 4 : entry + 8] = flags.to_bytes(4, "little")
        elf[entry + 8 : entry + 16] = offset.to_bytes(8, "little")
        elf[entry + 16 : entry + 24] = address.to_bytes(8, "little")
        elf[entry + 24 : entry + 32] = address.to_bytes(8, "little")
        elf[entry + 32 : entry + 40] = size.to_bytes(8, "little")
        elf[entry + 40 : entry + 48] = size.to_bytes(8, "little")
        elf[entry + 48 : entry + 56] = (0x1000).to_bytes(8, "little")

    load(0, 0x5, text_offset, 0x0040_0000, 1)
    load(1, 0x6, data_offset, 0x0040_1000, 8)
    elf[text_offset] = 0xC3
    return bytes(elf)


def load_native_elf_verifier():
    path = ROOT / "scripts" / "verify-pyth-native-elf.py"
    spec = importlib.util.spec_from_file_location("verify_pyth_native_elf", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load verify-pyth-native-elf.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class IsoImageTest(unittest.TestCase):
    def test_native_elf_verifier_rejects_invalid_load_ranges(self) -> None:
        verifier = load_native_elf_verifier()
        truncated_load = bytearray(native_elf_fixture())
        truncated_load[64 + 32 : 64 + 40] = (2).to_bytes(8, "little")
        truncated_load[64 + 40 : 64 + 48] = (1).to_bytes(8, "little")
        with self.assertRaisesRegex(ValueError, "file size exceeds memory size"):
            verifier.verify(bytes(truncated_load))

        out_of_range_load = bytearray(native_elf_fixture())
        out_of_range_load[64 + 8 : 64 + 16] = (len(out_of_range_load)).to_bytes(8, "little")
        out_of_range_load[64 + 32 : 64 + 40] = (1).to_bytes(8, "little")
        with self.assertRaisesRegex(ValueError, "file range is outside"):
            verifier.verify(bytes(out_of_range_load))

    def test_native_binding_record_binds_graph_and_elf_digests(self) -> None:
        principal = 0x5059_5448_4752_0001
        for module in (load_build_image_module(), load_build_iso_module()):
            with tempfile.TemporaryDirectory() as temp:
                temp_path = Path(temp)
                graph_dir = temp_path / "graphs"
                graph_dir.mkdir()
                graph = graph_dir / "hello.tig"
                elf = temp_path / "hello.elf"
                graph_bytes = b"PYTHTIG1" + bytes(16) + principal.to_bytes(8, "little")
                graph.write_bytes(graph_bytes)
                elf_bytes = native_elf_fixture()
                elf.write_bytes(elf_bytes)
                module.PYTH_GRAPH_OUTPUT_DIR = graph_dir

                records = module.native_pyth_graph_records(elf)

                self.assertEqual(
                    [record_type for record_type, _ in records],
                    [
                        module.INIT_BUNDLE_PYTH_GRAPH_TYPE,
                        module.INIT_BUNDLE_NAMED_USER_ELF_TYPE,
                        module.INIT_BUNDLE_PYTH_NATIVE_BINDING_TYPE,
                    ],
                )
                binding = records[2][1]
                graph_name = graph.name.encode("ascii")
                elf_name = elf.name.encode("ascii")
                self.assertEqual(binding[:8], module.PYTH_NATIVE_BINDING_MAGIC)
                self.assertEqual(int.from_bytes(binding[8:10], "little"), 1)
                self.assertEqual(int.from_bytes(binding[10:12], "little"), 0)
                self.assertEqual(int.from_bytes(binding[12:14], "little"), len(graph_name))
                self.assertEqual(int.from_bytes(binding[14:16], "little"), len(elf_name))
                self.assertEqual(int.from_bytes(binding[16:24], "little"), principal)
                self.assertEqual(
                    int.from_bytes(binding[24:32], "little"), module.digest64(graph_bytes)
                )
                self.assertEqual(
                    int.from_bytes(binding[32:40], "little"), module.digest64(elf_bytes)
                )
                self.assertEqual(binding[40:48], bytes(8))
                self.assertEqual(binding[48 : 48 + len(graph_name)], graph_name)
                self.assertEqual(binding[48 + len(graph_name) :], elf_name)

                substituted_elf = elf_bytes[:-1] + bytes((elf_bytes[-1] ^ 1,))
                self.assertNotEqual(
                    int.from_bytes(binding[32:40], "little"),
                    module.digest64(substituted_elf),
                )

    def test_native_packaging_rejects_invalid_elf(self) -> None:
        for module in (load_build_image_module(), load_build_iso_module()):
            with tempfile.TemporaryDirectory() as temp:
                temp_path = Path(temp)
                graph_dir = temp_path / "graphs"
                graph_dir.mkdir()
                (graph_dir / "hello.tig").write_bytes(b"PYTHTIG1" + bytes(24))
                elf = temp_path / "hello.elf"
                elf.write_bytes(b"not an ELF")
                module.PYTH_GRAPH_OUTPUT_DIR = graph_dir

                with self.assertRaisesRegex(ValueError, "not an ELF64 file"):
                    module.native_pyth_graph_records(elf)

    def test_native_packaging_rejects_invalid_load_ranges(self) -> None:
        def make_filesz_greater_than_memsz(elf: bytearray) -> None:
            elf[64 + 32 : 64 + 40] = (2).to_bytes(8, "little")
            elf[64 + 40 : 64 + 48] = (1).to_bytes(8, "little")

        def make_file_range_outside_elf(elf: bytearray) -> None:
            elf[64 + 8 : 64 + 16] = len(elf).to_bytes(8, "little")
            elf[64 + 32 : 64 + 40] = (1).to_bytes(8, "little")

        for module in (load_build_image_module(), load_build_iso_module()):
            for label, mutate, expected in (
                (
                    "filesz-greater-than-memsz",
                    make_filesz_greater_than_memsz,
                    "file size exceeds memory size",
                ),
                (
                    "file-range-outside-elf",
                    make_file_range_outside_elf,
                    "file range is outside",
                ),
            ):
                with tempfile.TemporaryDirectory() as temp:
                    temp_path = Path(temp)
                    graph_dir = temp_path / "graphs"
                    graph_dir.mkdir()
                    (graph_dir / f"{label}.tig").write_bytes(b"PYTHTIG1" + bytes(24))
                    elf = temp_path / f"{label}.elf"
                    elf_bytes = bytearray(native_elf_fixture())
                    mutate(elf_bytes)
                    elf.write_bytes(bytes(elf_bytes))
                    module.PYTH_GRAPH_OUTPUT_DIR = graph_dir

                    with self.assertRaisesRegex(ValueError, expected):
                        module.native_pyth_graph_records(elf)

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

    def test_phase3_object_flow_opt_in_requires_runtime_artifact(self) -> None:
        for module in (load_build_image_module(), load_build_iso_module()):
            with tempfile.TemporaryDirectory() as temp:
                temp_path = Path(temp)
                shell = temp_path / "pythos-user-shell"
                shell.write_bytes(b"\x7fELFshell")
                module.SHELL_ELF = shell
                module.PYTH_RUNTIME_ELF = temp_path / "missing-pyth-runtime"

                with self.assertRaisesRegex(SystemExit, "missing PythTIG runtime ELF"):
                    module.build_default_init_pak(include_pythtig_object_flow=True)

    def test_phase2_and_phase3_object_graph_sets_are_not_co_packaged(self) -> None:
        for module in (load_build_image_module(), load_build_iso_module()):
            with self.assertRaisesRegex(
                SystemExit, "select either --with-pythtig or --with-pythtig-object-flow"
            ):
                module.build_default_init_pak(
                    include_pythtig=True, include_pythtig_object_flow=True
                )

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

    def test_phase3_object_flow_init_pak_contains_bounded_object_graph_set(self) -> None:
        for module in (load_build_image_module(), load_build_iso_module()):
            init_pak = build_init_pak_with_phase3_object_programs(module)
            payload = init_pak[module.INIT_PAK_HEADER_LEN :]
            record_count = 11
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
                    module.INIT_BUNDLE_USER_ELF_TYPE,
                    module.INIT_BUNDLE_USER_ELF_TYPE,
                    module.INIT_BUNDLE_USER_ELF_TYPE,
                    module.INIT_BUNDLE_USER_ELF_TYPE,
                ],
            )

            for graph_index, expected_name in (
                (3, b"object-create.tig"),
                (4, b"object-restore.tig"),
                (5, b"object-known-denied.tig"),
                (6, b"object-forgery.tig"),
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
