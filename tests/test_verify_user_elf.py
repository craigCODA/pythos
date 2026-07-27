from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "verify-user-elf.py"
SPEC = importlib.util.spec_from_file_location("verify_user_elf", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
verifier = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verifier)


def write_u16(data: bytearray, offset: int, value: int) -> None:
    data[offset : offset + 2] = value.to_bytes(2, "little")


def write_u32(data: bytearray, offset: int, value: int) -> None:
    data[offset : offset + 4] = value.to_bytes(4, "little")


def write_u64(data: bytearray, offset: int, value: int) -> None:
    data[offset : offset + 8] = value.to_bytes(8, "little")


def build_valid_elf(phnum: int = 2) -> bytes:
    first_payload = 0x1000
    total = first_payload + phnum * 0x1000 + 1
    elf = bytearray(total)
    elf[0:4] = b"\x7fELF"
    elf[4] = verifier.EI_CLASS_64
    elf[5] = verifier.EI_DATA_LITTLE_ENDIAN
    elf[6] = 1
    write_u16(elf, 16, verifier.ET_EXEC)
    write_u16(elf, 18, verifier.EM_X86_64)
    write_u32(elf, 20, 1)
    write_u64(elf, 24, 0x0040_0000)
    write_u64(elf, 32, verifier.ELF64_EHDR_SIZE)
    write_u16(elf, 52, verifier.ELF64_EHDR_SIZE)
    write_u16(elf, 54, verifier.ELF64_PHDR_SIZE)
    write_u16(elf, 56, phnum)

    for index in range(phnum):
        entry = verifier.ELF64_EHDR_SIZE + index * verifier.ELF64_PHDR_SIZE
        offset = first_payload + index * verifier.PAGE_SIZE
        vaddr = 0x0040_0000 + index * verifier.PAGE_SIZE
        flags = verifier.PF_X if index == 0 else verifier.PF_W
        write_u32(elf, entry, verifier.PT_LOAD)
        write_u32(elf, entry + 4, flags)
        write_u64(elf, entry + 8, offset)
        write_u64(elf, entry + 16, vaddr)
        write_u64(elf, entry + 24, vaddr)
        write_u64(elf, entry + 32, 1)
        write_u64(elf, entry + 40, 1)
        write_u64(elf, entry + 48, verifier.PAGE_SIZE)
        elf[offset] = 0xCC
    return bytes(elf)


class VerifyUserElfTests(unittest.TestCase):
    def test_rejects_bad_elf_ident_version(self) -> None:
        elf = bytearray(build_valid_elf())
        elf[6] = 0

        with self.assertRaises(verifier.ElfVerifyError):
            verifier.verify(bytes(elf))

    def test_rejects_bad_e_version(self) -> None:
        elf = bytearray(build_valid_elf())
        write_u32(elf, 20, 0)

        with self.assertRaises(verifier.ElfVerifyError):
            verifier.verify(bytes(elf))

    def test_rejects_more_than_kernel_segment_limit(self) -> None:
        with self.assertRaises(verifier.ElfVerifyError):
            verifier.verify(build_valid_elf(phnum=9))


if __name__ == "__main__":
    unittest.main()
