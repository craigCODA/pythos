from __future__ import annotations

import struct
import subprocess
import unittest
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
KERNEL_ELF = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"

SHT_SYMTAB = 2


@dataclass(frozen=True)
class Section:
    name: str
    address: int
    size: int
    offset: int
    entry_size: int
    link: int
    kind: int


def read_u16(data: bytes, offset: int) -> int:
    return struct.unpack_from("<H", data, offset)[0]


def read_u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def read_u64(data: bytes, offset: int) -> int:
    return struct.unpack_from("<Q", data, offset)[0]


def c_string(data: bytes, offset: int) -> str:
    end = data.index(0, offset)
    return data[offset:end].decode("ascii")


def read_sections(data: bytes) -> list[Section]:
    if len(data) < 64 or data[0:4] != b"\x7fELF":
        raise AssertionError("pythcore is not an ELF64 file")
    if data[4] != 2 or data[5] != 1:
        raise AssertionError("pythcore is not a little-endian ELF64 file")

    section_offset = read_u64(data, 40)
    section_entry_size = read_u16(data, 58)
    section_count = read_u16(data, 60)
    section_name_index = read_u16(data, 62)
    if section_entry_size != 64:
        raise AssertionError(f"unexpected section header size: {section_entry_size}")
    if section_name_index >= section_count:
        raise AssertionError("invalid section-name table index")

    raw_sections = []
    for index in range(section_count):
        entry = section_offset + index * section_entry_size
        raw_sections.append(
            {
                "name_offset": read_u32(data, entry),
                "kind": read_u32(data, entry + 4),
                "address": read_u64(data, entry + 16),
                "offset": read_u64(data, entry + 24),
                "size": read_u64(data, entry + 32),
                "link": read_u32(data, entry + 40),
                "entry_size": read_u64(data, entry + 56),
            }
        )

    names_header = raw_sections[section_name_index]
    names = data[names_header["offset"] : names_header["offset"] + names_header["size"]]
    return [
        Section(
            name=c_string(names, raw["name_offset"]) if raw["name_offset"] else "",
            address=raw["address"],
            size=raw["size"],
            offset=raw["offset"],
            entry_size=raw["entry_size"],
            link=raw["link"],
            kind=raw["kind"],
        )
        for raw in raw_sections
    ]


def read_symbols(data: bytes, sections: list[Section]) -> dict[str, int]:
    symbols: dict[str, int] = {}
    for section in sections:
        if section.kind != SHT_SYMTAB:
            continue
        if section.entry_size != 24:
            raise AssertionError(f"unexpected symbol entry size: {section.entry_size}")
        names_header = sections[section.link]
        names = data[names_header.offset : names_header.offset + names_header.size]
        count = section.size // section.entry_size
        for index in range(count):
            entry = section.offset + index * section.entry_size
            name_offset = read_u32(data, entry)
            if name_offset == 0:
                continue
            symbols[c_string(names, name_offset)] = read_u64(data, entry + 8)
    return symbols


class KernelLinkerSymbolsTest(unittest.TestCase):
    def test_kernel_mapping_symbols_survive_duplicate_linker_script_args(self) -> None:
        subprocess.run(
            [
                "cargo",
                "rustc",
                "-p",
                "pythos-core",
                "--target",
                "x86_64-unknown-none",
                "--features",
                "verify",
                "--",
                "-C",
                "link-arg=-Tcore/linker.ld",
            ],
            cwd=ROOT,
            check=True,
        )

        data = KERNEL_ELF.read_bytes()
        sections = {section.name: section for section in read_sections(data)}
        symbols = read_symbols(data, list(sections.values()))

        self.assertEqual(symbols["__pythcore_rodata_start"], sections[".rodata"].address)
        self.assertEqual(
            symbols["__pythcore_rodata_end"],
            sections[".rodata"].address + sections[".rodata"].size,
        )
        self.assertEqual(symbols["__pythcore_text_start"], sections[".text"].address)
        self.assertEqual(
            symbols["__pythcore_text_end"],
            sections[".text"].address + sections[".text"].size,
        )
        self.assertEqual(symbols["__pythcore_data_start"], sections[".data"].address)
        self.assertLess(symbols["__pythcore_data_start"], symbols["__pythcore_data_end"])
        self.assertLess(
            symbols["__pythcore_syscall_stack_guard_low_start"],
            symbols["__pythcore_syscall_stack_start"],
        )
        self.assertEqual(
            symbols["__pythcore_syscall_stack_start"],
            sections[".syscall_stack"].address,
        )
        self.assertGreater(
            symbols["__pythcore_syscall_stack_guard_high_start"],
            symbols["__pythcore_syscall_stack_start"],
        )
        self.assertEqual(
            symbols["__pythcore_syscall_stack_guard_high_end"],
            symbols["__pythcore_data_end"],
        )


if __name__ == "__main__":
    unittest.main()
