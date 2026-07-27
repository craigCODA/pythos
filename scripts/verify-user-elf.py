#!/usr/bin/env python
"""Verify the ring-3 shell ELF's shape with a real ELF64 parser.

This is a self-contained ELF64 header/program-header parser (stdlib only),
not a regex over `readelf` text output (which varies across binutils/LLVM
versions and this Windows host has no `readelf` at all). Its checks mirror
core/src/user_elf.rs::validate() so a Python "pass" here is strong evidence
the same bytes will pass the real Rust loader-side validator later (Task 8),
though it is not a substitute for that real check.

Task 4 only builds and statically validates this ELF; it does not execute the
ring-3 boundary yet (that begins with persistent launch in Task 8).
"""

from __future__ import annotations

import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SHELL_ELF = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"

# Mirrors core/src/user_elf.rs exactly.
EI_CLASS_64 = 2
EI_DATA_LITTLE_ENDIAN = 1
ELF_VERSION_CURRENT = 1
ET_EXEC = 2
EM_X86_64 = 0x3E
PT_LOAD = 1
PT_DYNAMIC = 2
PT_INTERP = 3
PF_X = 0x1
PF_W = 0x2
ELF64_EHDR_SIZE = 64
ELF64_PHDR_SIZE = 56
MAX_LOAD_SEGMENTS = 8
USER_VIRT_MIN = 0x0020_0000
USER_VIRT_MAX = 0x0000_8000_0000_0000
KERNEL_VIRT_MIN = 0xFFFF_FFFF_8000_0000
PAGE_SIZE = 4096


class ElfVerifyError(AssertionError):
    pass


def read_u16(data: bytes, offset: int) -> int:
    return struct.unpack_from("<H", data, offset)[0]


def read_u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def read_u64(data: bytes, offset: int) -> int:
    return struct.unpack_from("<Q", data, offset)[0]


def align_down(value: int) -> int:
    return value & ~(PAGE_SIZE - 1)


def align_up(value: int) -> int:
    return align_down(value + PAGE_SIZE - 1)


def verify(data: bytes) -> None:
    if len(data) < ELF64_EHDR_SIZE:
        raise ElfVerifyError(f"file too short for an ELF64 header: {len(data)} bytes")
    if data[0:4] != b"\x7fELF":
        raise ElfVerifyError("bad ELF magic")
    if data[4] != EI_CLASS_64:
        raise ElfVerifyError(f"not ELF64 (EI_CLASS={data[4]})")
    if data[5] != EI_DATA_LITTLE_ENDIAN:
        raise ElfVerifyError(f"not little-endian (EI_DATA={data[5]})")
    if data[6] != ELF_VERSION_CURRENT:
        raise ElfVerifyError(f"unsupported ELF ident version (EI_VERSION={data[6]})")

    e_type = read_u16(data, 16)
    if e_type != ET_EXEC:
        raise ElfVerifyError(f"not ET_EXEC (e_type={e_type}); PIE/ET_DYN is rejected")
    e_machine = read_u16(data, 18)
    if e_machine != EM_X86_64:
        raise ElfVerifyError(f"not x86-64 (e_machine=0x{e_machine:x})")
    e_version = read_u32(data, 20)
    if e_version != ELF_VERSION_CURRENT:
        raise ElfVerifyError(f"unsupported ELF header version (e_version={e_version})")

    e_entry = read_u64(data, 24)
    e_phoff = read_u64(data, 32)
    e_phentsize = read_u16(data, 54)
    e_phnum = read_u16(data, 56)
    if e_phentsize != ELF64_PHDR_SIZE:
        raise ElfVerifyError(f"unexpected program header entry size: {e_phentsize}")
    if e_phnum == 0:
        raise ElfVerifyError("no program headers")
    ph_table_end = e_phoff + e_phnum * ELF64_PHDR_SIZE
    if ph_table_end > len(data):
        raise ElfVerifyError("program header table runs past end of file")

    load_segments: list[dict[str, int]] = []
    for index in range(e_phnum):
        entry = e_phoff + index * ELF64_PHDR_SIZE
        p_type = read_u32(data, entry)
        if p_type == PT_INTERP:
            raise ElfVerifyError("PT_INTERP segment present (dynamic linking is forbidden)")
        if p_type == PT_DYNAMIC:
            raise ElfVerifyError("PT_DYNAMIC segment present (dynamic linking is forbidden)")
        if p_type != PT_LOAD:
            raise ElfVerifyError(
                f"unexpected non-PT_LOAD program header type 0x{p_type:x} at index {index} "
                "(the explicit PHDRS linker script should prevent this - check linker.ld)"
            )
        if len(load_segments) >= MAX_LOAD_SEGMENTS:
            raise ElfVerifyError(
                f"too many PT_LOAD segments ({len(load_segments) + 1}); "
                f"kernel loader limit is {MAX_LOAD_SEGMENTS}"
            )

        flags = read_u32(data, entry + 4)
        p_offset = read_u64(data, entry + 8)
        p_vaddr = read_u64(data, entry + 16)
        p_filesz = read_u64(data, entry + 32)
        p_memsz = read_u64(data, entry + 40)
        p_align = read_u64(data, entry + 48)

        if p_memsz == 0 or p_filesz > p_memsz:
            raise ElfVerifyError(f"segment {index}: empty or filesz > memsz")
        if p_offset + p_filesz > len(data):
            raise ElfVerifyError(f"segment {index}: file range runs past end of file")
        virtual_end = p_vaddr + p_memsz
        if (
            p_vaddr < USER_VIRT_MIN
            or virtual_end > USER_VIRT_MAX
            or p_vaddr >= KERNEL_VIRT_MIN
            or virtual_end >= KERNEL_VIRT_MIN
        ):
            raise ElfVerifyError(
                f"segment {index}: virtual range 0x{p_vaddr:x}..0x{virtual_end:x} "
                "outside the permitted user range"
            )
        if p_align == 0 or (p_align & (p_align - 1)) != 0:
            raise ElfVerifyError(f"segment {index}: alignment {p_align} is not a power of two")
        if p_vaddr % p_align != p_offset % p_align:
            raise ElfVerifyError(f"segment {index}: vaddr/offset alignment mismatch")
        if flags & PF_W and flags & PF_X:
            raise ElfVerifyError(f"segment {index}: writable AND executable (W^X violation)")
        if p_vaddr % PAGE_SIZE != 0:
            raise ElfVerifyError(f"segment {index}: vaddr 0x{p_vaddr:x} is not page-aligned")

        page_start = align_down(p_vaddr)
        page_end = align_up(virtual_end)
        for other in load_segments:
            if page_start < other["page_end"] and other["page_start"] < page_end:
                raise ElfVerifyError(
                    f"segment {index} overlaps an earlier segment "
                    f"(0x{page_start:x}..0x{page_end:x} vs "
                    f"0x{other['page_start']:x}..0x{other['page_end']:x})"
                )

        load_segments.append(
            {
                "vaddr": p_vaddr,
                "memsz": p_memsz,
                "flags": flags,
                "page_start": page_start,
                "page_end": page_end,
            }
        )

    if not load_segments:
        raise ElfVerifyError("no PT_LOAD segments")

    entry_ok = any(
        segment["flags"] & PF_X and segment["vaddr"] <= e_entry < segment["vaddr"] + segment["memsz"]
        for segment in load_segments
    )
    if not entry_ok:
        raise ElfVerifyError(f"entry point 0x{e_entry:x} is not inside an executable PT_LOAD segment")


def main() -> int:
    if not SHELL_ELF.exists():
        raise ElfVerifyError(f"missing built ELF: {SHELL_ELF}")
    data = SHELL_ELF.read_bytes()
    verify(data)
    print("USER_ELF_VERIFY_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
