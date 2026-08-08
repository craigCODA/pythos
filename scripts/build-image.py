#!/usr/bin/env python
"""Build the first-slice EFI system partition directory."""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ESP = ROOT / "image" / "esp"
BOOT_EFI = ROOT / "target" / "x86_64-unknown-uefi" / "debug" / "bootx64.efi"
PYTHCORE_ELF = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"
SHELL_ELF = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
PYTH_RUNTIME_ELF = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-pyth-runtime"
PYTH_GRAPH_PACKAGE = ROOT / "target" / "pyth-tig" / "hello.tig"
PYTH_BUDGET_GRAPH_PACKAGE = ROOT / "target" / "pyth-tig" / "budget.tig"
PYTH_INVALID_GRAPH_PACKAGE = ROOT / "target" / "pyth-tig" / "invalid.tig"
PYTH_UNSUPPORTED_GRAPH_PACKAGE = ROOT / "target" / "pyth-tig" / "unsupported.tig"
PYTH_INVALID_STRING_GRAPH_PACKAGE = ROOT / "target" / "pyth-tig" / "invalid-string.tig"
PYTH_PARAMETERIZED_GRAPH_PACKAGE = ROOT / "target" / "pyth-tig" / "parameterized.tig"
BOOT_CFG = b"serial=true\nlog_level=trace\npanic=halt\nruntime_bundle=/PYTHOS/INIT.PAK\n"
INIT_PAK_MAGIC = b"PYTHOS_INIT_PAK_V0"
INIT_PAK_HEADER_LEN = 64
RUNTIME_PAYLOAD_MAGIC = b"PYTHOS_MINRT_V00"
RUNTIME_PAYLOAD_HEADER_LEN = 32
INIT_BUNDLE_MAGIC = b"PYTHOS_BUNDLE_V0"
INIT_BUNDLE_HEADER_LEN = 32
INIT_BUNDLE_RECORD_LEN = 32
INIT_BUNDLE_RUNTIME_TYPE = 0x0000_0001
INIT_BUNDLE_USER_ELF_TYPE = 0x0000_0002
INIT_BUNDLE_NAMED_USER_ELF_TYPE = 0x0000_0003
INIT_BUNDLE_PYTH_GRAPH_TYPE = 0x0000_0004
NAMED_USER_PROGRAM_MAGIC = b"PYUPGM01"
NAMED_USER_PROGRAM_HEADER_LEN = 40
MAX_NAMED_PROGRAM_NAME_LEN = 32
NAMED_PYTH_GRAPH_MAGIC = b"PYTIGM01"
NAMED_PYTH_GRAPH_HEADER_LEN = 40
MAX_NAMED_PYTH_GRAPH_NAME_LEN = 32
SHELL_PRINCIPAL_ID = 0x5059_5348_454C_4C01
PYTH_RUNTIME_PRINCIPAL_ID = 0x5059_5448_5254_0001
HELLO_GRAPH_PRINCIPAL_ID = 0x5059_5448_4752_0001
BUDGET_GRAPH_PRINCIPAL_ID = 0x5059_5448_4752_0002
INVALID_GRAPH_PRINCIPAL_ID = 0x5059_5448_4752_00FF
UNSUPPORTED_GRAPH_PRINCIPAL_ID = 0x5059_5448_4752_0003
INVALID_STRING_GRAPH_PRINCIPAL_ID = 0x5059_5448_4752_0004
PARAMETERIZED_GRAPH_PRINCIPAL_ID = 0x5059_5448_4752_0005
USER_ELF_ENTRY = 0x00400000
RUNTIME_SOURCE = (
    b"class HelloService(Service):\n"
    b"    async def start(self):\n"
    b"        system.log(\"PythOS [HISS] We Are Woken\")\n"
    b"        self.ready()\n"
)
PSF1_HEADER_LEN = 4
PSF1_GLYPH_COUNT = 256
PSF1_GLYPH_HEIGHT = 8


def build_init_pak(payload: bytes = b"") -> bytes:
    checksum = sum(payload) & 0xFFFFFFFF
    total_len = INIT_PAK_HEADER_LEN + len(payload)
    header = bytearray(INIT_PAK_HEADER_LEN)
    header[: len(INIT_PAK_MAGIC)] = INIT_PAK_MAGIC
    header[18:20] = (0).to_bytes(2, "little")
    header[20:22] = (0).to_bytes(2, "little")
    header[22:26] = INIT_PAK_HEADER_LEN.to_bytes(4, "little")
    header[26:34] = total_len.to_bytes(8, "little")
    header[34:42] = len(payload).to_bytes(8, "little")
    header[42:46] = checksum.to_bytes(4, "little")
    return bytes(header) + payload


def build_runtime_payload(source: bytes = RUNTIME_SOURCE) -> bytes:
    checksum = sum(source) & 0xFFFFFFFF
    header = bytearray(RUNTIME_PAYLOAD_HEADER_LEN)
    header[: len(RUNTIME_PAYLOAD_MAGIC)] = RUNTIME_PAYLOAD_MAGIC
    header[16:18] = (0).to_bytes(2, "little")
    header[18:20] = (0).to_bytes(2, "little")
    header[20:24] = RUNTIME_PAYLOAD_HEADER_LEN.to_bytes(4, "little")
    header[24:28] = len(source).to_bytes(4, "little")
    header[28:32] = checksum.to_bytes(4, "little")
    return bytes(header) + source


def build_init_bundle(records: list[tuple[int, bytes]]) -> bytes:
    table_len = len(records) * INIT_BUNDLE_RECORD_LEN
    cursor = INIT_BUNDLE_HEADER_LEN + table_len
    header = bytearray(INIT_BUNDLE_HEADER_LEN)
    header[: len(INIT_BUNDLE_MAGIC)] = INIT_BUNDLE_MAGIC
    header[16:18] = (0).to_bytes(2, "little")
    header[18:20] = (0).to_bytes(2, "little")
    header[20:24] = INIT_BUNDLE_HEADER_LEN.to_bytes(4, "little")
    header[24:26] = len(records).to_bytes(2, "little")

    table = bytearray(table_len)
    payloads = bytearray()
    for index, (record_type, payload) in enumerate(records):
        entry = index * INIT_BUNDLE_RECORD_LEN
        table[entry : entry + 4] = record_type.to_bytes(4, "little")
        table[entry + 8 : entry + 16] = cursor.to_bytes(8, "little")
        table[entry + 16 : entry + 24] = len(payload).to_bytes(8, "little")
        table[entry + 24 : entry + 28] = (sum(payload) & 0xFFFFFFFF).to_bytes(4, "little")
        payloads.extend(payload)
        cursor += len(payload)
    return bytes(header) + bytes(table) + bytes(payloads)


def digest64(payload: bytes) -> int:
    value = 0xCBF2_9CE4_8422_2325
    for byte in payload:
        value ^= byte
        value = (value * 0x0000_0100_0000_01B3) & 0xFFFF_FFFF_FFFF_FFFF
    return value


def build_named_user_program(name: bytes, principal_id: int, elf: bytes) -> bytes:
    if len(name) > MAX_NAMED_PROGRAM_NAME_LEN:
        raise ValueError("named user program name is too long")
    if len(elf) > 0xFFFF_FFFF:
        raise ValueError("named user program ELF is too large")

    header = bytearray(NAMED_USER_PROGRAM_HEADER_LEN)
    header[0:8] = NAMED_USER_PROGRAM_MAGIC
    header[8:10] = (1).to_bytes(2, "little")
    header[10:12] = (0).to_bytes(2, "little")
    header[12:14] = len(name).to_bytes(2, "little")
    header[16:24] = principal_id.to_bytes(8, "little")
    header[24:32] = digest64(elf).to_bytes(8, "little")
    header[32:36] = len(elf).to_bytes(4, "little")
    return bytes(header) + name + elf


def build_named_pyth_graph(name: bytes, principal_id: int, package: bytes) -> bytes:
    if len(name) > MAX_NAMED_PYTH_GRAPH_NAME_LEN:
        raise ValueError("named Pyth graph name is too long")
    if len(package) > 0xFFFF_FFFF:
        raise ValueError("Pyth graph package is too large")

    header = bytearray(NAMED_PYTH_GRAPH_HEADER_LEN)
    header[0:8] = NAMED_PYTH_GRAPH_MAGIC
    header[8:10] = (1).to_bytes(2, "little")
    header[10:12] = (0).to_bytes(2, "little")
    header[12:14] = len(name).to_bytes(2, "little")
    header[16:24] = principal_id.to_bytes(8, "little")
    header[24:32] = digest64(package).to_bytes(8, "little")
    header[32:36] = len(package).to_bytes(4, "little")
    return bytes(header) + name + package


def build_user_elf_payload(text: bytes) -> bytes:
    data = b"DATA"
    text_offset = 0x1000
    data_offset = 0x2000
    data_memsz = 16
    elf = bytearray(data_offset + len(data))
    elf[0:4] = b"\x7fELF"
    elf[4] = 2
    elf[5] = 1
    elf[6] = 1
    elf[16:18] = (2).to_bytes(2, "little")
    elf[18:20] = (0x3E).to_bytes(2, "little")
    elf[20:24] = (1).to_bytes(4, "little")
    elf[24:32] = USER_ELF_ENTRY.to_bytes(8, "little")
    elf[32:40] = (64).to_bytes(8, "little")
    elf[52:54] = (64).to_bytes(2, "little")
    elf[54:56] = (56).to_bytes(2, "little")
    elf[56:58] = (2).to_bytes(2, "little")

    def phdr(index: int, flags: int, offset: int, vaddr: int, filesz: int, memsz: int) -> None:
        entry = 64 + index * 56
        elf[entry : entry + 4] = (1).to_bytes(4, "little")
        elf[entry + 4 : entry + 8] = flags.to_bytes(4, "little")
        elf[entry + 8 : entry + 16] = offset.to_bytes(8, "little")
        elf[entry + 16 : entry + 24] = vaddr.to_bytes(8, "little")
        elf[entry + 24 : entry + 32] = vaddr.to_bytes(8, "little")
        elf[entry + 32 : entry + 40] = filesz.to_bytes(8, "little")
        elf[entry + 40 : entry + 48] = memsz.to_bytes(8, "little")
        elf[entry + 48 : entry + 56] = (0x1000).to_bytes(8, "little")

    phdr(0, 0x5, text_offset, USER_ELF_ENTRY, len(text), len(text))
    phdr(1, 0x6, data_offset, USER_ELF_ENTRY + 0x1000, len(data), data_memsz)
    elf[text_offset : text_offset + len(text)] = text
    elf[data_offset : data_offset + len(data)] = data
    return bytes(elf)


def build_default_init_pak(include_pythtig: bool = False) -> bytes:
    if not SHELL_ELF.exists():
        raise SystemExit(f"missing shell ELF: {SHELL_ELF}")
    records = [
        (INIT_BUNDLE_RUNTIME_TYPE, build_runtime_payload()),
        (
            INIT_BUNDLE_NAMED_USER_ELF_TYPE,
            build_named_user_program(
                b"shell.elf", SHELL_PRINCIPAL_ID, SHELL_ELF.read_bytes()
            ),
        ),
    ]
    if include_pythtig:
        if not PYTH_RUNTIME_ELF.exists():
            raise SystemExit(f"missing PythTIG runtime ELF: {PYTH_RUNTIME_ELF}")
        if not PYTH_GRAPH_PACKAGE.exists():
            raise SystemExit(f"missing PythTIG graph package: {PYTH_GRAPH_PACKAGE}")
        if not PYTH_BUDGET_GRAPH_PACKAGE.exists():
            raise SystemExit(
                f"missing PythTIG budget graph package: {PYTH_BUDGET_GRAPH_PACKAGE}"
            )
        if not PYTH_INVALID_GRAPH_PACKAGE.exists():
            raise SystemExit(
                f"missing PythTIG invalid graph package: {PYTH_INVALID_GRAPH_PACKAGE}"
            )
        if not PYTH_UNSUPPORTED_GRAPH_PACKAGE.exists():
            raise SystemExit(
                "missing PythTIG unsupported graph package: "
                f"{PYTH_UNSUPPORTED_GRAPH_PACKAGE}"
            )
        if not PYTH_INVALID_STRING_GRAPH_PACKAGE.exists():
            raise SystemExit(
                "missing PythTIG invalid-string graph package: "
                f"{PYTH_INVALID_STRING_GRAPH_PACKAGE}"
            )
        if not PYTH_PARAMETERIZED_GRAPH_PACKAGE.exists():
            raise SystemExit(
                "missing PythTIG parameterized graph package: "
                f"{PYTH_PARAMETERIZED_GRAPH_PACKAGE}"
            )
        records.extend(
            [
                (
                    INIT_BUNDLE_NAMED_USER_ELF_TYPE,
                    build_named_user_program(
                        b"pyth-runtime.elf",
                        PYTH_RUNTIME_PRINCIPAL_ID,
                        PYTH_RUNTIME_ELF.read_bytes(),
                    ),
                ),
                (
                    INIT_BUNDLE_PYTH_GRAPH_TYPE,
                    build_named_pyth_graph(
                        b"hello.tig",
                        HELLO_GRAPH_PRINCIPAL_ID,
                        PYTH_GRAPH_PACKAGE.read_bytes(),
                    ),
                ),
                (
                    INIT_BUNDLE_PYTH_GRAPH_TYPE,
                    build_named_pyth_graph(
                        b"budget.tig",
                        BUDGET_GRAPH_PRINCIPAL_ID,
                        PYTH_BUDGET_GRAPH_PACKAGE.read_bytes(),
                    ),
                ),
                (
                    INIT_BUNDLE_PYTH_GRAPH_TYPE,
                    build_named_pyth_graph(
                        b"invalid.tig",
                        INVALID_GRAPH_PRINCIPAL_ID,
                        PYTH_INVALID_GRAPH_PACKAGE.read_bytes(),
                    ),
                ),
                (
                    INIT_BUNDLE_PYTH_GRAPH_TYPE,
                    build_named_pyth_graph(
                        b"unsupported.tig",
                        UNSUPPORTED_GRAPH_PRINCIPAL_ID,
                        PYTH_UNSUPPORTED_GRAPH_PACKAGE.read_bytes(),
                    ),
                ),
                (
                    INIT_BUNDLE_PYTH_GRAPH_TYPE,
                    build_named_pyth_graph(
                        b"invalid-string.tig",
                        INVALID_STRING_GRAPH_PRINCIPAL_ID,
                        PYTH_INVALID_STRING_GRAPH_PACKAGE.read_bytes(),
                    ),
                ),
                (
                    INIT_BUNDLE_PYTH_GRAPH_TYPE,
                    build_named_pyth_graph(
                        b"parameterized.tig",
                        PARAMETERIZED_GRAPH_PRINCIPAL_ID,
                        PYTH_PARAMETERIZED_GRAPH_PACKAGE.read_bytes(),
                    ),
                ),
            ]
        )
    records.extend(
        [
            (INIT_BUNDLE_USER_ELF_TYPE, build_user_elf_payload(b"\xCC\xF4")),
            (INIT_BUNDLE_USER_ELF_TYPE, build_user_elf_payload(b"\x0F\x0B\xF4")),
            (
                INIT_BUNDLE_USER_ELF_TYPE,
                build_user_elf_payload(
                    b"\x48\xB8" + (0).to_bytes(8, "little") + b"\x8A\x00\xF4"
                ),
            ),
            (
                INIT_BUNDLE_USER_ELF_TYPE,
                build_user_elf_payload(b"\xBA\xF8\x03\x00\x00\xEC\xF4"),
            ),
        ]
    )
    return build_init_pak(build_init_bundle(records))


def build_font_psf() -> bytes:
    font = bytearray(PSF1_HEADER_LEN + PSF1_GLYPH_COUNT * PSF1_GLYPH_HEIGHT)
    font[0:4] = bytes([0x36, 0x04, 0x00, PSF1_GLYPH_HEIGHT])
    a_offset = PSF1_HEADER_LEN + ord("A") * PSF1_GLYPH_HEIGHT
    font[a_offset : a_offset + PSF1_GLYPH_HEIGHT] = bytes(
        [0x18, 0x24, 0x42, 0x7E, 0x42, 0x42, 0x00, 0x00]
    )
    return bytes(font)


FONT_PSF = build_font_psf()


def write_binary_if_changed(path: Path, content: bytes) -> None:
    if path.exists() and path.read_bytes() == content:
        return
    path.write_bytes(content)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--loader", type=Path, default=BOOT_EFI)
    parser.add_argument("--kernel", type=Path, default=PYTHCORE_ELF)
    parser.add_argument("--with-pythtig", action="store_true")
    args = parser.parse_args()

    loader = args.loader
    if not loader.exists():
        raise SystemExit(f"missing loader: {loader}")
    kernel = args.kernel
    if not kernel.exists():
        raise SystemExit(f"missing kernel: {kernel}")

    boot_dir = ESP / "EFI" / "BOOT"
    pythos_dir = ESP / "PYTHOS"
    boot_dir.mkdir(parents=True, exist_ok=True)
    pythos_dir.mkdir(parents=True, exist_ok=True)

    shutil.copy2(loader, boot_dir / "BOOTX64.EFI")
    shutil.copy2(kernel, pythos_dir / "PYTHCORE.ELF")
    write_binary_if_changed(pythos_dir / "BOOT.CFG", BOOT_CFG)
    write_binary_if_changed(
        pythos_dir / "INIT.PAK", build_default_init_pak(args.with_pythtig)
    )
    write_binary_if_changed(pythos_dir / "FONT.PSF", FONT_PSF)

    print(f"ESP_READY {ESP}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
