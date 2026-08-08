#!/usr/bin/env python
"""Build a UEFI El Torito bootable ISO for PythOS."""

from __future__ import annotations

import argparse
import math
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
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
PYTH_OBJECT_CREATE_GRAPH_PACKAGE = ROOT / "target" / "pyth-tig" / "object-create.tig"
PYTH_OBJECT_RESTORE_GRAPH_PACKAGE = ROOT / "target" / "pyth-tig" / "object-restore.tig"
PYTH_OBJECT_KNOWN_DENIED_GRAPH_PACKAGE = (
    ROOT / "target" / "pyth-tig" / "object-known-denied.tig"
)
PYTH_OBJECT_FORGERY_GRAPH_PACKAGE = ROOT / "target" / "pyth-tig" / "object-forgery.tig"
DEFAULT_OUTPUT = ROOT / "target" / "pythos.iso"
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
OBJECT_CREATE_GRAPH_PRINCIPAL_ID = 0x5059_5448_4752_0006
OBJECT_RESTORE_GRAPH_PRINCIPAL_ID = 0x5059_5448_4752_0007
OBJECT_KNOWN_DENIED_GRAPH_PRINCIPAL_ID = 0x5059_5448_4752_0008
OBJECT_FORGERY_GRAPH_PRINCIPAL_ID = 0x5059_5448_4752_0009
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

SECTOR_SIZE = 512
ISO_SECTOR_SIZE = 2048
ESP_SIZE = 16 * 1024 * 1024
MEDIA_DESCRIPTOR = 0xF8
FAT_EOC = 0xFFFF


def align_up(value: int, alignment: int) -> int:
    return ((value + alignment - 1) // alignment) * alignment


def short_name(name: str) -> bytes:
    stem, dot, suffix = name.partition(".")
    if not stem or len(stem) > 8 or (dot and len(suffix) > 3):
        raise ValueError(f"not an 8.3 name: {name}")
    raw = stem.upper().ljust(8) + suffix.upper().ljust(3)
    encoded = raw.encode("ascii")
    if any(byte in b'"+,;=[]' for byte in encoded):
        raise ValueError(f"unsupported FAT name: {name}")
    return encoded


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


def require_file(path: Path, description: str) -> bytes:
    if not path.exists():
        raise SystemExit(f"missing {description}: {path}")
    return path.read_bytes()


def pyth_runtime_record() -> tuple[int, bytes]:
    return (
        INIT_BUNDLE_NAMED_USER_ELF_TYPE,
        build_named_user_program(
            b"pyth-runtime.elf",
            PYTH_RUNTIME_PRINCIPAL_ID,
            require_file(PYTH_RUNTIME_ELF, "PythTIG runtime ELF"),
        ),
    )


def phase2_pyth_graph_records() -> list[tuple[int, bytes]]:
    graph_specs = [
        (b"hello.tig", HELLO_GRAPH_PRINCIPAL_ID, PYTH_GRAPH_PACKAGE, "PythTIG graph package"),
        (
            b"budget.tig",
            BUDGET_GRAPH_PRINCIPAL_ID,
            PYTH_BUDGET_GRAPH_PACKAGE,
            "PythTIG budget graph package",
        ),
        (
            b"invalid.tig",
            INVALID_GRAPH_PRINCIPAL_ID,
            PYTH_INVALID_GRAPH_PACKAGE,
            "PythTIG invalid graph package",
        ),
        (
            b"unsupported.tig",
            UNSUPPORTED_GRAPH_PRINCIPAL_ID,
            PYTH_UNSUPPORTED_GRAPH_PACKAGE,
            "PythTIG unsupported graph package",
        ),
        (
            b"invalid-string.tig",
            INVALID_STRING_GRAPH_PRINCIPAL_ID,
            PYTH_INVALID_STRING_GRAPH_PACKAGE,
            "PythTIG invalid-string graph package",
        ),
        (
            b"parameterized.tig",
            PARAMETERIZED_GRAPH_PRINCIPAL_ID,
            PYTH_PARAMETERIZED_GRAPH_PACKAGE,
            "PythTIG parameterized graph package",
        ),
    ]
    return [
        (
            INIT_BUNDLE_PYTH_GRAPH_TYPE,
            build_named_pyth_graph(name, principal_id, require_file(path, description)),
        )
        for name, principal_id, path, description in graph_specs
    ]


def phase3_object_pyth_graph_records() -> list[tuple[int, bytes]]:
    graph_specs = [
        (
            b"object-create.tig",
            OBJECT_CREATE_GRAPH_PRINCIPAL_ID,
            PYTH_OBJECT_CREATE_GRAPH_PACKAGE,
            "PythTIG object-create graph package",
        ),
        (
            b"object-restore.tig",
            OBJECT_RESTORE_GRAPH_PRINCIPAL_ID,
            PYTH_OBJECT_RESTORE_GRAPH_PACKAGE,
            "PythTIG object-restore graph package",
        ),
        (
            b"object-known-denied.tig",
            OBJECT_KNOWN_DENIED_GRAPH_PRINCIPAL_ID,
            PYTH_OBJECT_KNOWN_DENIED_GRAPH_PACKAGE,
            "PythTIG object-known-denied graph package",
        ),
        (
            b"object-forgery.tig",
            OBJECT_FORGERY_GRAPH_PRINCIPAL_ID,
            PYTH_OBJECT_FORGERY_GRAPH_PACKAGE,
            "PythTIG object-forgery graph package",
        ),
    ]
    return [
        (
            INIT_BUNDLE_PYTH_GRAPH_TYPE,
            build_named_pyth_graph(name, principal_id, require_file(path, description)),
        )
        for name, principal_id, path, description in graph_specs
    ]


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


def build_default_init_pak(
    include_pythtig: bool = False, include_pythtig_object_flow: bool = False
) -> bytes:
    if include_pythtig and include_pythtig_object_flow:
        raise SystemExit(
            "select either --with-pythtig or --with-pythtig-object-flow, not both; "
            "the current INIT.PAK bundle table admits one PythTIG acceptance set per image"
        )
    shell_elf = require_file(SHELL_ELF, "shell ELF")
    records = [
        (INIT_BUNDLE_RUNTIME_TYPE, build_runtime_payload()),
        (
            INIT_BUNDLE_NAMED_USER_ELF_TYPE,
            build_named_user_program(b"shell.elf", SHELL_PRINCIPAL_ID, shell_elf),
        ),
    ]
    if include_pythtig:
        records.append(pyth_runtime_record())
        records.extend(phase2_pyth_graph_records())
    if include_pythtig_object_flow:
        records.append(pyth_runtime_record())
        records.extend(phase3_object_pyth_graph_records())
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


def directory_entry(name: str, attr: int, cluster: int, size: int) -> bytes:
    entry = bytearray(32)
    entry[0:11] = short_name(name)
    entry[11] = attr
    entry[26:28] = cluster.to_bytes(2, "little")
    entry[28:32] = size.to_bytes(4, "little")
    return bytes(entry)


def dot_entry(name: str, cluster: int) -> bytes:
    entry = bytearray(32)
    entry[0:11] = name.encode("ascii").ljust(11)
    entry[11] = 0x10
    entry[26:28] = cluster.to_bytes(2, "little")
    return bytes(entry)


def build_directory(entries: list[bytes], cluster_size: int) -> bytes:
    content = b"".join(entries)
    if len(content) + 1 > cluster_size:
        raise ValueError("directory does not fit in one cluster")
    return content + bytes(cluster_size - len(content))


def fat16_layout(total_sectors: int) -> tuple[int, int, int, int]:
    sectors_per_cluster = 1
    reserved_sectors = 1
    root_entries = 512
    root_dir_sectors = (root_entries * 32 + SECTOR_SIZE - 1) // SECTOR_SIZE
    fat_sectors = 1
    while True:
        data_sectors = total_sectors - reserved_sectors - root_dir_sectors - 2 * fat_sectors
        cluster_count = data_sectors // sectors_per_cluster
        needed_fat_sectors = math.ceil((cluster_count + 2) * 2 / SECTOR_SIZE)
        if needed_fat_sectors == fat_sectors:
            return sectors_per_cluster, fat_sectors, root_dir_sectors, cluster_count
        fat_sectors = needed_fat_sectors


def pythos_boot_files(
    loader: Path,
    kernel: Path,
    include_pythtig: bool = False,
    include_pythtig_object_flow: bool = False,
) -> dict[str, bytes]:
    return {
        "EFI/BOOT/BOOTX64.EFI": loader.read_bytes(),
        "PYTHOS/PYTHCORE.ELF": kernel.read_bytes(),
        "PYTHOS/BOOT.CFG": b"serial=true\nlog_level=trace\npanic=halt\nruntime_bundle=/PYTHOS/INIT.PAK\n",
        "PYTHOS/INIT.PAK": build_default_init_pak(
            include_pythtig, include_pythtig_object_flow
        ),
        "PYTHOS/FONT.PSF": build_font_psf(),
    }


def build_esp_image(files: dict[str, bytes]) -> bytes:
    total_sectors = ESP_SIZE // SECTOR_SIZE
    sectors_per_cluster, fat_sectors, root_dir_sectors, cluster_count = fat16_layout(total_sectors)
    cluster_size = sectors_per_cluster * SECTOR_SIZE
    root_dir_start = 1 + 2 * fat_sectors
    data_start = root_dir_start + root_dir_sectors

    fat_entries = [0] * (fat_sectors * SECTOR_SIZE // 2)
    fat_entries[0] = 0xFF00 | MEDIA_DESCRIPTOR
    fat_entries[1] = FAT_EOC
    clusters: dict[int, bytes] = {}
    next_cluster = 2

    def allocate_chain(data: bytes) -> list[int]:
        nonlocal next_cluster
        if not data:
            return []
        count = math.ceil(len(data) / cluster_size)
        chain = list(range(next_cluster, next_cluster + count))
        next_cluster += count
        if next_cluster > cluster_count + 2:
            raise ValueError("ESP image is too small")
        padded = data + bytes(count * cluster_size - len(data))
        for index, cluster in enumerate(chain):
            clusters[cluster] = padded[index * cluster_size : (index + 1) * cluster_size]
            fat_entries[cluster] = chain[index + 1] if index + 1 < len(chain) else FAT_EOC
        return chain

    dir_clusters = {
        "EFI": allocate_chain(bytes(cluster_size))[0],
        "EFI/BOOT": allocate_chain(bytes(cluster_size))[0],
        "PYTHOS": allocate_chain(bytes(cluster_size))[0],
    }
    file_chains = {path: allocate_chain(data) for path, data in files.items()}

    root_entries = [
        directory_entry("EFI", 0x10, dir_clusters["EFI"], 0),
        directory_entry("PYTHOS", 0x10, dir_clusters["PYTHOS"], 0),
    ]
    root_dir = b"".join(root_entries)
    root_dir = root_dir + bytes(root_dir_sectors * SECTOR_SIZE - len(root_dir))

    efi_entries = [
        dot_entry(".", dir_clusters["EFI"]),
        dot_entry("..", 0),
        directory_entry("BOOT", 0x10, dir_clusters["EFI/BOOT"], 0),
    ]
    clusters[dir_clusters["EFI"]] = build_directory(efi_entries, cluster_size)

    boot_file_chain = file_chains["EFI/BOOT/BOOTX64.EFI"]
    boot_entries = [
        dot_entry(".", dir_clusters["EFI/BOOT"]),
        dot_entry("..", dir_clusters["EFI"]),
        directory_entry("BOOTX64.EFI", 0x20, boot_file_chain[0], len(files["EFI/BOOT/BOOTX64.EFI"])),
    ]
    clusters[dir_clusters["EFI/BOOT"]] = build_directory(boot_entries, cluster_size)

    pythos_entries = [
        dot_entry(".", dir_clusters["PYTHOS"]),
        dot_entry("..", 0),
    ]
    for leaf in ("PYTHCORE.ELF", "BOOT.CFG", "INIT.PAK", "FONT.PSF"):
        path = f"PYTHOS/{leaf}"
        chain = file_chains[path]
        pythos_entries.append(directory_entry(leaf, 0x20, chain[0] if chain else 0, len(files[path])))
    clusters[dir_clusters["PYTHOS"]] = build_directory(pythos_entries, cluster_size)

    image = bytearray(ESP_SIZE)
    boot = bytearray(SECTOR_SIZE)
    boot[0:3] = b"\xEB\x3C\x90"
    boot[3:11] = b"PYTHOS  "
    boot[11:13] = SECTOR_SIZE.to_bytes(2, "little")
    boot[13] = sectors_per_cluster
    boot[14:16] = (1).to_bytes(2, "little")
    boot[16] = 2
    boot[17:19] = (512).to_bytes(2, "little")
    boot[19:21] = total_sectors.to_bytes(2, "little")
    boot[21] = MEDIA_DESCRIPTOR
    boot[22:24] = fat_sectors.to_bytes(2, "little")
    boot[24:26] = (32).to_bytes(2, "little")
    boot[26:28] = (64).to_bytes(2, "little")
    boot[36] = 0x80
    boot[38] = 0x29
    boot[39:43] = (0x50595448).to_bytes(4, "little")
    boot[43:54] = b"PYTHOS     "
    boot[54:62] = b"FAT16   "
    boot[510:512] = b"\x55\xAA"
    image[0:SECTOR_SIZE] = boot

    fat = bytearray(fat_sectors * SECTOR_SIZE)
    for index, value in enumerate(fat_entries):
        fat[index * 2 : index * 2 + 2] = value.to_bytes(2, "little")
    fat_start = SECTOR_SIZE
    image[fat_start : fat_start + len(fat)] = fat
    image[fat_start + len(fat) : fat_start + 2 * len(fat)] = fat
    root_offset = root_dir_start * SECTOR_SIZE
    image[root_offset : root_offset + len(root_dir)] = root_dir
    for cluster, content in clusters.items():
        offset = (data_start + (cluster - 2) * sectors_per_cluster) * SECTOR_SIZE
        image[offset : offset + len(content)] = content
    return bytes(image)


def both_endian_16(value: int) -> bytes:
    return value.to_bytes(2, "little") + value.to_bytes(2, "big")


def both_endian_32(value: int) -> bytes:
    return value.to_bytes(4, "little") + value.to_bytes(4, "big")


def directory_record(extent: int, data_len: int, flags: int, file_id: bytes) -> bytes:
    length = 33 + len(file_id)
    if length % 2:
        length += 1
    record = bytearray(length)
    record[0] = length
    record[2:10] = both_endian_32(extent)
    record[10:18] = both_endian_32(data_len)
    record[25] = flags
    record[28:32] = both_endian_16(1)
    record[32] = len(file_id)
    record[33 : 33 + len(file_id)] = file_id
    return bytes(record)


def boot_catalog(boot_lba: int, boot_image_len: int) -> bytes:
    catalog = bytearray(ISO_SECTOR_SIZE)
    catalog[0] = 0x01
    catalog[1] = 0xEF
    catalog[4:28] = b"PYTHOS".ljust(24)
    catalog[30:32] = b"\x55\xAA"
    checksum = 0
    for index in range(0, 32, 2):
        checksum = (checksum + int.from_bytes(catalog[index : index + 2], "little")) & 0xFFFF
    catalog[28:30] = ((-checksum) & 0xFFFF).to_bytes(2, "little")

    sector_count = math.ceil(boot_image_len / SECTOR_SIZE)
    catalog[32] = 0x88
    catalog[33] = 0x00
    catalog[34:36] = (0).to_bytes(2, "little")
    catalog[36] = 0
    catalog[37] = 0
    catalog[38:40] = min(sector_count, 0xFFFF).to_bytes(2, "little")
    catalog[40:44] = boot_lba.to_bytes(4, "little")
    return bytes(catalog)


def path_table_record(name: bytes, extent: int, parent: int) -> bytes:
    record = bytearray()
    record.append(len(name))
    record.append(0)
    record += extent.to_bytes(4, "little")
    record += parent.to_bytes(2, "little")
    record += name
    if len(name) % 2:
        record.append(0)
    return bytes(record)


def directory_sector(records: list[bytes]) -> bytes:
    content = b"".join(records)
    if len(content) > ISO_SECTOR_SIZE:
        raise ValueError("ISO directory does not fit in one sector")
    return content + bytes(ISO_SECTOR_SIZE - len(content))


def padded_iso_payload(data: bytes) -> bytes:
    return data + bytes(align_up(max(1, len(data)), ISO_SECTOR_SIZE) - len(data))


def build_iso_bytes(esp_image: bytes, iso_files: dict[str, bytes]) -> bytes:
    pvd_lba = 16
    boot_record_lba = 17
    terminator_lba = 18
    catalog_lba = 19
    current_lba = 20

    dir_lbas = {
        "": current_lba,
        "EFI": current_lba + 1,
        "EFI/BOOT": current_lba + 2,
        "PYTHOS": current_lba + 3,
    }
    current_lba += len(dir_lbas)

    path_table = b"".join(
        [
            path_table_record(b"\x00", dir_lbas[""], 1),
            path_table_record(b"EFI", dir_lbas["EFI"], 1),
            path_table_record(b"BOOT", dir_lbas["EFI/BOOT"], 2),
            path_table_record(b"PYTHOS", dir_lbas["PYTHOS"], 1),
        ]
    )
    path_table_lba = current_lba
    path_table_sectors = math.ceil(len(path_table) / ISO_SECTOR_SIZE)
    current_lba += path_table_sectors

    boot_image_lba = current_lba
    boot_image_sectors = math.ceil(len(esp_image) / ISO_SECTOR_SIZE)
    current_lba += boot_image_sectors

    file_order = [
        "EFI/BOOT/BOOTX64.EFI",
        "PYTHOS/PYTHCORE.ELF",
        "PYTHOS/BOOT.CFG",
        "PYTHOS/INIT.PAK",
        "PYTHOS/FONT.PSF",
    ]
    file_lbas = {}
    for path in file_order:
        file_lbas[path] = current_lba
        current_lba += math.ceil(max(1, len(iso_files[path])) / ISO_SECTOR_SIZE)
    volume_sectors = current_lba

    root_record = directory_record(dir_lbas[""], ISO_SECTOR_SIZE, 0x02, b"\x00")
    root_dir = directory_sector(
        [
            directory_record(dir_lbas[""], ISO_SECTOR_SIZE, 0x02, b"\x00"),
            directory_record(dir_lbas[""], ISO_SECTOR_SIZE, 0x02, b"\x01"),
            directory_record(dir_lbas["EFI"], ISO_SECTOR_SIZE, 0x02, b"EFI"),
            directory_record(dir_lbas["PYTHOS"], ISO_SECTOR_SIZE, 0x02, b"PYTHOS"),
        ]
    )
    efi_dir = directory_sector(
        [
            directory_record(dir_lbas["EFI"], ISO_SECTOR_SIZE, 0x02, b"\x00"),
            directory_record(dir_lbas[""], ISO_SECTOR_SIZE, 0x02, b"\x01"),
            directory_record(dir_lbas["EFI/BOOT"], ISO_SECTOR_SIZE, 0x02, b"BOOT"),
        ]
    )
    boot_dir = directory_sector(
        [
            directory_record(dir_lbas["EFI/BOOT"], ISO_SECTOR_SIZE, 0x02, b"\x00"),
            directory_record(dir_lbas["EFI"], ISO_SECTOR_SIZE, 0x02, b"\x01"),
            directory_record(
                file_lbas["EFI/BOOT/BOOTX64.EFI"],
                len(iso_files["EFI/BOOT/BOOTX64.EFI"]),
                0x00,
                b"BOOTX64.EFI;1",
            ),
        ]
    )
    pythos_dir_records = [
        directory_record(dir_lbas["PYTHOS"], ISO_SECTOR_SIZE, 0x02, b"\x00"),
        directory_record(dir_lbas[""], ISO_SECTOR_SIZE, 0x02, b"\x01"),
    ]
    for leaf in ("PYTHCORE.ELF", "BOOT.CFG", "INIT.PAK", "FONT.PSF"):
        path = f"PYTHOS/{leaf}"
        pythos_dir_records.append(
            directory_record(file_lbas[path], len(iso_files[path]), 0x00, f"{leaf};1".encode("ascii"))
        )
    pythos_dir = directory_sector(pythos_dir_records)
    path_table_size = len(path_table)
    path_table = path_table + bytes(path_table_sectors * ISO_SECTOR_SIZE - len(path_table))

    pvd = bytearray(ISO_SECTOR_SIZE)
    pvd[0] = 1
    pvd[1:6] = b"CD001"
    pvd[6] = 1
    pvd[8:40] = b"PYTHOS".ljust(32)
    pvd[40:72] = b"PYTHOS_BOOT".ljust(32)
    pvd[80:88] = both_endian_32(volume_sectors)
    pvd[120:124] = both_endian_16(1)
    pvd[124:128] = both_endian_16(1)
    pvd[128:132] = both_endian_16(ISO_SECTOR_SIZE)
    pvd[132:140] = both_endian_32(path_table_size)
    pvd[140:144] = path_table_lba.to_bytes(4, "little")
    pvd[148:152] = path_table_lba.to_bytes(4, "big")
    pvd[156 : 156 + len(root_record)] = root_record

    boot_record = bytearray(ISO_SECTOR_SIZE)
    boot_record[0] = 0
    boot_record[1:6] = b"CD001"
    boot_record[6] = 1
    boot_record[7:39] = b"EL TORITO SPECIFICATION".ljust(32)
    boot_record[71:75] = catalog_lba.to_bytes(4, "little")

    terminator = bytearray(ISO_SECTOR_SIZE)
    terminator[0] = 255
    terminator[1:6] = b"CD001"
    terminator[6] = 1

    image = bytearray(volume_sectors * ISO_SECTOR_SIZE)
    image[pvd_lba * ISO_SECTOR_SIZE : (pvd_lba + 1) * ISO_SECTOR_SIZE] = pvd
    image[
        boot_record_lba * ISO_SECTOR_SIZE : (boot_record_lba + 1) * ISO_SECTOR_SIZE
    ] = boot_record
    image[
        terminator_lba * ISO_SECTOR_SIZE : (terminator_lba + 1) * ISO_SECTOR_SIZE
    ] = terminator
    image[
        catalog_lba * ISO_SECTOR_SIZE : (catalog_lba + 1) * ISO_SECTOR_SIZE
    ] = boot_catalog(boot_image_lba, len(esp_image))
    image[dir_lbas[""] * ISO_SECTOR_SIZE : (dir_lbas[""] + 1) * ISO_SECTOR_SIZE] = root_dir
    image[dir_lbas["EFI"] * ISO_SECTOR_SIZE : (dir_lbas["EFI"] + 1) * ISO_SECTOR_SIZE] = efi_dir
    image[
        dir_lbas["EFI/BOOT"] * ISO_SECTOR_SIZE : (dir_lbas["EFI/BOOT"] + 1) * ISO_SECTOR_SIZE
    ] = boot_dir
    image[dir_lbas["PYTHOS"] * ISO_SECTOR_SIZE : (dir_lbas["PYTHOS"] + 1) * ISO_SECTOR_SIZE] = pythos_dir
    image[
        path_table_lba * ISO_SECTOR_SIZE : (path_table_lba + 1) * ISO_SECTOR_SIZE
    ] = path_table
    boot_offset = boot_image_lba * ISO_SECTOR_SIZE
    image[boot_offset : boot_offset + len(esp_image)] = esp_image
    for path in file_order:
        offset = file_lbas[path] * ISO_SECTOR_SIZE
        payload = padded_iso_payload(iso_files[path])
        image[offset : offset + len(payload)] = payload
    return bytes(image)


def build_iso(
    output: Path,
    loader: Path = BOOT_EFI,
    kernel: Path = PYTHCORE_ELF,
    include_pythtig: bool = False,
    include_pythtig_object_flow: bool = False,
) -> None:
    if not loader.exists():
        raise SystemExit(f"missing loader: {loader}")
    if not kernel.exists():
        raise SystemExit(f"missing kernel: {kernel}")
    output.parent.mkdir(parents=True, exist_ok=True)
    files = pythos_boot_files(
        loader, kernel, include_pythtig, include_pythtig_object_flow
    )
    esp_image = build_esp_image(files)
    output.write_bytes(build_iso_bytes(esp_image, files))
    print(f"ISO_READY {output}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--loader", type=Path, default=BOOT_EFI)
    parser.add_argument("--kernel", type=Path, default=PYTHCORE_ELF)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--with-pythtig", action="store_true")
    parser.add_argument("--with-pythtig-object-flow", action="store_true")
    args = parser.parse_args()
    build_iso(
        args.output,
        args.loader,
        args.kernel,
        args.with_pythtig,
        args.with_pythtig_object_flow,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
