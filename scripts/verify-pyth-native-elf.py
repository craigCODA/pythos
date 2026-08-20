#!/usr/bin/env python
"""Verify the bounded ELF shape emitted by the Pyth native code generator."""

from __future__ import annotations

import sys
from pathlib import Path


ET_EXEC = 2
EM_X86_64 = 0x3E
PT_LOAD = 1
PT_INTERP = 3
SHT_DYNAMIC = 6
PF_X = 1
PF_W = 2
PF_R = 4
ELF_HEADER_LEN = 64
PROGRAM_HEADER_LEN = 56
SECTION_HEADER_LEN = 64


def read_u16(data: bytes, offset: int) -> int:
    return int.from_bytes(require_range(data, offset, 2), "little")


def read_u32(data: bytes, offset: int) -> int:
    return int.from_bytes(require_range(data, offset, 4), "little")


def read_u64(data: bytes, offset: int) -> int:
    return int.from_bytes(require_range(data, offset, 8), "little")


def require_range(data: bytes, offset: int, size: int) -> bytes:
    end = offset + size
    if offset < 0 or end < offset or end > len(data):
        raise ValueError("truncated ELF")
    return data[offset:end]


def verify(data: bytes) -> None:
    if len(data) < ELF_HEADER_LEN or data[:4] != b"\x7fELF":
        raise ValueError("not an ELF64 file")
    if data[4:7] != bytes((2, 1, 1)):
        raise ValueError("ELF must be 64-bit little-endian current version")
    if read_u16(data, 16) != ET_EXEC:
        raise ValueError("ELF must be ET_EXEC")
    if read_u16(data, 18) != EM_X86_64:
        raise ValueError("ELF must target x86-64")

    entry = read_u64(data, 24)
    phoff = read_u64(data, 32)
    shoff = read_u64(data, 40)
    phentsize = read_u16(data, 54)
    phnum = read_u16(data, 56)
    shentsize = read_u16(data, 58)
    shnum = read_u16(data, 60)
    if phentsize != PROGRAM_HEADER_LEN:
        raise ValueError("unexpected program-header size")
    if shentsize != SECTION_HEADER_LEN:
        raise ValueError("unexpected section-header size")
    if phoff + phentsize * phnum > len(data) or shoff + shentsize * shnum > len(data):
        raise ValueError("truncated ELF table")

    entry_in_rx_load = False
    has_rw_load = False
    for index in range(phnum):
        offset = phoff + index * phentsize
        kind = read_u32(data, offset)
        flags = read_u32(data, offset + 4)
        vaddr = read_u64(data, offset + 16)
        memsz = read_u64(data, offset + 40)
        if kind == PT_INTERP:
            raise ValueError("interpreter segment is forbidden")
        if kind != PT_LOAD:
            continue
        if flags & PF_W and flags & PF_X:
            raise ValueError("writable executable LOAD is forbidden")
        if flags == (PF_R | PF_W):
            has_rw_load = True
        if flags == (PF_R | PF_X) and vaddr <= entry < vaddr + memsz:
            entry_in_rx_load = True

    if not entry_in_rx_load:
        raise ValueError("entry is not inside an RX LOAD")
    if not has_rw_load:
        raise ValueError("missing RW request/result region")

    for index in range(shnum):
        offset = shoff + index * shentsize
        if read_u32(data, offset + 4) == SHT_DYNAMIC:
            raise ValueError("dynamic section is forbidden")


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: verify-pyth-native-elf.py PROGRAM.ELF")
    path = Path(sys.argv[1])
    try:
        verify(path.read_bytes())
    except (OSError, ValueError) as error:
        raise SystemExit(f"PYTH_NATIVE_ELF_VERIFY_FAILED {error}") from error
    print("PYTH_NATIVE_ELF_VERIFY_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
