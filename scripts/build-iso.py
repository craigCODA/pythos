#!/usr/bin/env python
"""Build a UEFI El Torito bootable ISO for PythOS."""

from __future__ import annotations

import argparse
import math
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BOOT_EFI = ROOT / "target" / "x86_64-unknown-uefi" / "debug" / "bootx64.efi"
PYTHCORE_ELF = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"
DEFAULT_OUTPUT = ROOT / "target" / "pythos.iso"
INIT_PAK_MAGIC = b"PYTHOS_INIT_PAK_V0"
INIT_PAK_HEADER_LEN = 64
RUNTIME_PAYLOAD_MAGIC = b"PYTHOS_MINRT_V00"
RUNTIME_PAYLOAD_HEADER_LEN = 32
RUNTIME_SOURCE = (
    b"class HelloService(Service):\n"
    b"    async def start(self):\n"
    b"        system.log(\"PythOS [HISS] We Are Woken\")\n"
    b"        self.ready()\n"
)

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


def pythos_boot_files(loader: Path, kernel: Path) -> dict[str, bytes]:
    return {
        "EFI/BOOT/BOOTX64.EFI": loader.read_bytes(),
        "PYTHOS/PYTHCORE.ELF": kernel.read_bytes(),
        "PYTHOS/BOOT.CFG": b"serial=true\nlog_level=trace\npanic=halt\nruntime_bundle=/PYTHOS/INIT.PAK\n",
        "PYTHOS/INIT.PAK": build_init_pak(build_runtime_payload()),
        "PYTHOS/FONT.PSF": b"",
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


def build_iso(output: Path, loader: Path = BOOT_EFI, kernel: Path = PYTHCORE_ELF) -> None:
    if not loader.exists():
        raise SystemExit(f"missing loader: {loader}")
    if not kernel.exists():
        raise SystemExit(f"missing kernel: {kernel}")
    output.parent.mkdir(parents=True, exist_ok=True)
    files = pythos_boot_files(loader, kernel)
    esp_image = build_esp_image(files)
    output.write_bytes(build_iso_bytes(esp_image, files))
    print(f"ISO_READY {output}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--loader", type=Path, default=BOOT_EFI)
    parser.add_argument("--kernel", type=Path, default=PYTHCORE_ELF)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    args = parser.parse_args()
    build_iso(args.output, args.loader, args.kernel)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
