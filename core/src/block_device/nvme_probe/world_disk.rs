//! Partition-relative world records on the identified NVMe namespace.

use pythos_shared::evidence_log::crc32_iso_hdlc;

#[cfg(feature = "world-session-nvme-target")]
use super::target::TARGET;
use super::{DMA_PAGE_BYTES, NvmeNamespace, SECTOR_SIZE, number};

const RECORD_BLOCKS: u64 = (DMA_PAGE_BYTES / SECTOR_SIZE) as u64;
#[cfg(feature = "world-session-nvme-initialize")]
const CHECKPOINT_RECORDS: core::ops::Range<u64> = 1..5;
const GPT_HEADER_BYTES: usize = 92;
const ENTRY_BYTES: usize = 128;
const ENTRY_COUNT: usize = 128;
const TABLE_BYTES: usize = ENTRY_BYTES * ENTRY_COUNT;
const TABLE_BLOCKS: u64 = (TABLE_BYTES / SECTOR_SIZE) as u64;
// EFI GUID encoding stores the first three fields in little-endian order.
const BASIC_DATA: [u8; 16] = [
    0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99, 0xc7,
];
// Unique partition GUID for the disposable QEMU fixture in ADR 0115.
#[cfg(not(feature = "world-session-nvme-target"))]
const WORLD_ID: [u8; 16] = [
    0x08, 0xf3, 0xdf, 0x71, 0xc4, 0x92, 0x88, 0x4e, 0xb4, 0xa9, 0x4b, 0x94, 0x00, 0xc8, 0x7d, 0x66,
];
const READ_ONLY: u64 = 1 << 60;
const SHADOW_COPY: u64 = 1 << 61;
const HIDDEN: u64 = 1 << 62;

pub(crate) struct WorldDisk {
    namespace: NvmeNamespace,
    first_lba: u64,
    records: u64,
    interrupt_before: u64,
}

impl WorldDisk {
    pub(super) fn open(mut namespace: NvmeNamespace) -> Result<Self, &'static str> {
        let (first_lba, last_lba) = locate_partition(&mut namespace)?;
        number("PARTITION_FIRST_LBA=", first_lba);
        number("PARTITION_LAST_LBA=", last_lba);
        let mut disk = Self {
            namespace,
            first_lba,
            records: (last_lba - first_lba + 1) / RECORD_BLOCKS,
            interrupt_before: 0,
        };
        #[cfg(not(feature = "world-session-nvme-target"))]
        {
            let header = disk.read(0)?;
            if &header[..8] != b"PYWNVM01" || header[16..].iter().any(|byte| *byte != 0) {
                return Err("WORLD_IMAGE");
            }
            disk.interrupt_before = u64_field(&header, 8);
        }
        #[cfg(feature = "world-session-nvme-target")]
        disk.open_target_header()?;
        Ok(disk)
    }

    #[cfg(feature = "world-session-nvme-target")]
    fn open_target_header(&mut self) -> Result<(), &'static str> {
        let mut header = [0; SECTOR_SIZE];
        header[..8].copy_from_slice(b"PYWNTG01");
        header[8..24].copy_from_slice(&TARGET.disk_guid);
        header[24..40].copy_from_slice(&TARGET.partition_guid);
        if self.read(0)? == header {
            return Ok(());
        }

        #[cfg(feature = "world-session-nvme-initialize")]
        {
            self.initialize(&header)
        }
        #[cfg(not(feature = "world-session-nvme-initialize"))]
        {
            Err("NVME_TARGET_HEADER_REQUIRED")
        }
    }

    #[cfg(feature = "world-session-nvme-initialize")]
    fn initialize(&mut self, header: &[u8; SECTOR_SIZE]) -> Result<(), &'static str> {
        if self.records < CHECKPOINT_RECORDS.end {
            return Err("WORLD_DEVICE");
        }

        let empty = [0; SECTOR_SIZE];
        if !self.page_matches(0, &empty)? {
            return Err("NVME_TARGET_INIT_HEADER_NOT_BLANK");
        }
        for record in CHECKPOINT_RECORDS {
            if !self.page_matches(record, &empty)? {
                return Err("NVME_TARGET_INIT_CHECKPOINT_NOT_BLANK");
            }
        }

        // These writes leave the store blank and can be repeated until the
        // identity header is committed. The header is the final write.
        for record in CHECKPOINT_RECORDS {
            self.write(record, &empty)?;
            if !self.page_matches(record, &empty)? {
                return Err("NVME_TARGET_INIT_READBACK");
            }
        }
        let lba = self.record_lba(0)?;
        self.namespace.write_page_and_flush(lba, header)?;
        if !self.page_matches(0, header)? {
            return Err("NVME_TARGET_INIT_READBACK");
        }
        super::mark("INITIALIZED");
        Ok(())
    }

    #[cfg(feature = "world-session-nvme-initialize")]
    fn page_matches(
        &mut self,
        record: u64,
        data: &[u8; SECTOR_SIZE],
    ) -> Result<bool, &'static str> {
        let lba = self.record_lba(record)?;
        if self.namespace.read_sector(lba)? != *data {
            return Ok(false);
        }
        for offset in 1..RECORD_BLOCKS {
            if self.namespace.read_sector(lba + offset)? != [0; SECTOR_SIZE] {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn capacity_sectors(&self) -> u64 {
        self.records
    }

    pub(crate) fn interrupt_before(&self) -> u64 {
        self.interrupt_before
    }

    pub(crate) fn read(&mut self, record: u64) -> Result<[u8; SECTOR_SIZE], &'static str> {
        let lba = self.record_lba(record)?;
        self.namespace.read_sector(lba)
    }

    pub(crate) fn write(
        &mut self,
        record: u64,
        bytes: &[u8; SECTOR_SIZE],
    ) -> Result<(), &'static str> {
        if record == 0 {
            return Err("WRITE_RANGE");
        }
        let lba = self.record_lba(record)?;
        self.namespace.write_page_and_flush(lba, bytes)
    }

    fn record_lba(&self, record: u64) -> Result<u64, &'static str> {
        if record >= self.records {
            return Err("PARTITION_RANGE");
        }
        // records counts only complete pages within the validated GPT range.
        Ok(self.first_lba + record * RECORD_BLOCKS)
    }
}

fn locate_partition(namespace: &mut NvmeNamespace) -> Result<(u64, u64), &'static str> {
    let backup_lba = namespace.blocks - 1;
    let mbr = namespace.read_sector(0)?;
    // A saturated protective count also occurs on disks smaller than 2 TiB.
    // The namespace and both GPT tables determine the actual writable range.
    let protective_blocks = u32_field(&mbr, 458);
    if mbr[510..] != [0x55, 0xaa]
        || mbr[446] != 0
        || mbr[450] != 0xee
        || u32_field(&mbr, 454) != 1
        || (protective_blocks != u32::MAX && u64::from(protective_blocks) != backup_lba)
        || mbr[462..510].iter().any(|byte| *byte != 0)
    {
        return Err("GPT_PROTECTIVE_MBR");
    }
    let primary = Header::read(namespace, 1, backup_lba)?;
    let backup = Header::read(namespace, backup_lba, 1)?;
    #[cfg(feature = "world-session-nvme-target")]
    if primary.disk_id != TARGET.disk_guid || backup.disk_id != TARGET.disk_guid {
        return Err("GPT_TARGET_DISK");
    }
    if primary.disk_id != backup.disk_id
        || primary.first_usable != backup.first_usable
        || primary.last_usable != backup.last_usable
        || primary.table_crc != backup.table_crc
        || primary.first_usable > primary.last_usable
        || primary.entries < 2
        || primary
            .entries
            .checked_add(TABLE_BLOCKS)
            .is_none_or(|end| end > primary.first_usable)
        || backup.entries <= primary.last_usable
        || backup
            .entries
            .checked_add(TABLE_BLOCKS)
            .is_none_or(|end| end > backup_lba)
    {
        return Err("GPT_LAYOUT");
    }

    let mut entries = [0; TABLE_BYTES];
    for (offset, sector) in entries.chunks_exact_mut(SECTOR_SIZE).enumerate() {
        let offset = offset as u64;
        let bytes = namespace.read_sector(primary.entries + offset)?;
        if bytes != namespace.read_sector(backup.entries + offset)? {
            return Err("GPT_TABLE_MISMATCH");
        }
        sector.copy_from_slice(&bytes);
    }
    if crc32_iso_hdlc(&entries) != primary.table_crc {
        return Err("GPT_TABLE_CRC");
    }
    select_partition(&entries, primary.first_usable, primary.last_usable)
}

fn select_partition(
    table: &[u8; TABLE_BYTES],
    first_usable: u64,
    last_usable: u64,
) -> Result<(u64, u64), &'static str> {
    let entries = || {
        table
            .chunks_exact(ENTRY_BYTES)
            .enumerate()
            .filter(|(_, entry)| entry[..16] != [0; 16])
    };
    #[cfg(not(feature = "world-session-nvme-target"))]
    let expected_id = WORLD_ID;
    #[cfg(feature = "world-session-nvme-target")]
    let expected_id = TARGET.partition_guid;
    let mut selected = None;
    for (index, entry) in entries() {
        let first = u64_field(entry, 32);
        let last = u64_field(entry, 40);
        if entry[16..32] == [0; 16] || first < first_usable || first > last || last > last_usable {
            return Err("GPT_ENTRY");
        }
        if entry[16..32] == expected_id {
            if selected.is_some() {
                return Err("GPT_WORLD_AMBIGUOUS");
            }
            let attributes = u64_field(entry, 48);
            if entry[..16] != BASIC_DATA
                || attributes & (HIDDEN | READ_ONLY | SHADOW_COPY) != HIDDEN
            {
                return Err("GPT_WORLD_ATTRIBUTES");
            }
            selected = Some((index, first, last));
        }
    }
    let (index, first, last) = selected.ok_or("GPT_WORLD_ABSENT")?;
    #[cfg(feature = "world-session-nvme-target")]
    if first != TARGET.first_lba || last != TARGET.last_lba {
        return Err("GPT_TARGET_GEOMETRY");
    }
    if !first.is_multiple_of(RECORD_BLOCKS) || last - first + 1 < RECORD_BLOCKS {
        return Err("GPT_WORLD_RANGE");
    }
    if entries().any(|(other, entry)| {
        other != index && u64_field(entry, 32) <= last && u64_field(entry, 40) >= first
    }) {
        return Err("GPT_OVERLAP");
    }
    Ok((first, last))
}

struct Header {
    disk_id: [u8; 16],
    first_usable: u64,
    last_usable: u64,
    entries: u64,
    table_crc: u32,
}

impl Header {
    fn read(namespace: &mut NvmeNamespace, lba: u64, alternate: u64) -> Result<Self, &'static str> {
        let mut bytes = namespace.read_sector(lba)?;
        let crc = u32_field(&bytes, 16);
        bytes[16..20].fill(0);
        if &bytes[..8] != b"EFI PART"
            || u32_field(&bytes, 8) != 0x0001_0000
            || u32_field(&bytes, 12) != GPT_HEADER_BYTES as u32
            || u32_field(&bytes, 20) != 0
            || u64_field(&bytes, 24) != lba
            || u64_field(&bytes, 32) != alternate
            || bytes[56..72] == [0; 16]
            || u32_field(&bytes, 80) != ENTRY_COUNT as u32
            || u32_field(&bytes, 84) != ENTRY_BYTES as u32
            || crc32_iso_hdlc(&bytes[..GPT_HEADER_BYTES]) != crc
        {
            return Err("GPT_HEADER");
        }
        Ok(Self {
            disk_id: bytes[56..72].try_into().unwrap(),
            first_usable: u64_field(&bytes, 40),
            last_usable: u64_field(&bytes, 48),
            entries: u64_field(&bytes, 72),
            table_crc: u32_field(&bytes, 88),
        })
    }
}

fn u32_field(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_field(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
