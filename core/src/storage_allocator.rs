//! Phase 10 journaled free-space bitmap allocator.
#![cfg_attr(test, allow(dead_code))]

use crate::block_device::BlockDeviceInfo;
#[cfg(not(test))]
use crate::serial;

const MAX_ALLOCATOR_BLOCKS: u16 = 64;
const JOURNAL_CAPACITY: usize = 8;
const COMMIT_MARKER: u32 = 0x5059_414C;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllocatorError {
    InvalidLayout,
    OutOfSpace,
    InvalidExtent,
    JournalFull,
    MissingCommitMarker,
    ChecksumMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockExtent {
    start_block: u16,
    block_count: u16,
}

impl BlockExtent {
    pub const fn new(start_block: u16, block_count: u16) -> Self {
        Self {
            start_block,
            block_count,
        }
    }

    pub const fn start_block(self) -> u16 {
        self.start_block
    }

    pub const fn block_count(self) -> u16 {
        self.block_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AllocatorJournalRecord {
    generation: u64,
    previous_bitmap: u64,
    next_bitmap: u64,
    checksum: u32,
    commit_marker: u32,
}

impl AllocatorJournalRecord {
    const fn empty() -> Self {
        Self {
            generation: 0,
            previous_bitmap: 0,
            next_bitmap: 0,
            checksum: 0,
            commit_marker: 0,
        }
    }

    pub const fn next_bitmap(self) -> u64 {
        self.next_bitmap
    }

    pub const fn is_committed(self) -> bool {
        self.commit_marker == COMMIT_MARKER
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AllocatorRecoveryReport {
    replayed_records: usize,
    rolled_back_records: usize,
    committed_bitmap: u64,
}

impl AllocatorRecoveryReport {
    pub const fn replayed_records(self) -> usize {
        self.replayed_records
    }

    pub const fn rolled_back_records(self) -> usize {
        self.rolled_back_records
    }

    pub const fn committed_bitmap(self) -> u64 {
        self.committed_bitmap
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AllocatorJournal {
    records: [AllocatorJournalRecord; JOURNAL_CAPACITY],
    len: usize,
    next_generation: u64,
}

impl AllocatorJournal {
    pub const fn new() -> Self {
        Self {
            records: [AllocatorJournalRecord::empty(); JOURNAL_CAPACITY],
            len: 0,
            next_generation: 1,
        }
    }

    fn append(
        &mut self,
        previous_bitmap: u64,
        next_bitmap: u64,
    ) -> Result<AllocatorJournalRecord, AllocatorError> {
        if self.len == JOURNAL_CAPACITY {
            return Err(AllocatorError::JournalFull);
        }
        let record = AllocatorJournalRecord {
            generation: self.next_generation,
            previous_bitmap,
            next_bitmap,
            checksum: 0,
            commit_marker: 0,
        };
        self.records[self.len] = record;
        self.len += 1;
        self.next_generation = self.next_generation.wrapping_add(1);
        Ok(record)
    }

    pub fn last(self) -> Option<AllocatorJournalRecord> {
        if self.len == 0 {
            None
        } else {
            Some(self.records[self.len - 1])
        }
    }

    fn commit_last(&mut self) -> Result<AllocatorJournalRecord, AllocatorError> {
        if self.len == 0 {
            return Err(AllocatorError::MissingCommitMarker);
        }
        let index = self.len - 1;
        let mut record = self.records[index];
        record.checksum = checksum_record(record);
        record.commit_marker = COMMIT_MARKER;
        validate_record(record)?;
        self.records[index] = record;
        Ok(record)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockAllocator {
    base_sector: u64,
    block_count: u16,
    bitmap: u64,
    committed_bitmap: u64,
}

impl BlockAllocator {
    pub fn new(base_sector: u64, block_count: u16) -> Result<Self, AllocatorError> {
        if block_count == 0 || block_count > MAX_ALLOCATOR_BLOCKS {
            return Err(AllocatorError::InvalidLayout);
        }
        base_sector
            .checked_add(u64::from(block_count))
            .ok_or(AllocatorError::InvalidLayout)?;
        Ok(Self {
            base_sector,
            block_count,
            bitmap: 0,
            committed_bitmap: 0,
        })
    }

    pub fn new_for_device(
        device: BlockDeviceInfo,
        base_sector: u64,
        block_count: u16,
    ) -> Result<Self, AllocatorError> {
        let allocator = Self::new(base_sector, block_count)?;
        let end = base_sector
            .checked_add(u64::from(block_count))
            .ok_or(AllocatorError::InvalidLayout)?;
        if end > device.capacity_sectors() {
            return Err(AllocatorError::InvalidLayout);
        }
        Ok(allocator)
    }

    pub fn restore_from_bitmap(
        base_sector: u64,
        block_count: u16,
        committed_bitmap: u64,
    ) -> Result<Self, AllocatorError> {
        let mut allocator = Self::new(base_sector, block_count)?;
        if bitmap_outside_block_count(committed_bitmap, block_count) {
            return Err(AllocatorError::InvalidExtent);
        }
        allocator.bitmap = committed_bitmap;
        allocator.committed_bitmap = committed_bitmap;
        Ok(allocator)
    }

    pub fn allocate(&mut self, block_count: u16) -> Result<BlockExtent, AllocatorError> {
        let extent = self.find_free_extent(block_count)?;
        self.mark_extent(extent, true)?;
        self.committed_bitmap = self.bitmap;
        Ok(extent)
    }

    pub fn allocate_journaled(
        &mut self,
        block_count: u16,
        journal: &mut AllocatorJournal,
    ) -> Result<BlockExtent, AllocatorError> {
        let extent = self.find_free_extent(block_count)?;
        let previous = self.committed_bitmap;
        let next = bitmap_with_extent(self.bitmap, extent, true)?;
        journal.append(previous, next)?;
        self.bitmap = next;
        Ok(extent)
    }

    pub fn commit_last(
        &mut self,
        journal: &mut AllocatorJournal,
    ) -> Result<AllocatorJournalRecord, AllocatorError> {
        let committed = journal.commit_last()?;
        self.committed_bitmap = committed.next_bitmap;
        self.bitmap = committed.next_bitmap;
        Ok(committed)
    }

    pub fn free(&mut self, extent: BlockExtent) -> Result<(), AllocatorError> {
        self.validate_extent(extent)?;
        self.mark_extent(extent, false)?;
        self.committed_bitmap = self.bitmap;
        Ok(())
    }

    pub const fn committed_bitmap(self) -> u64 {
        self.committed_bitmap
    }

    pub fn used_blocks(self) -> u16 {
        self.bitmap.count_ones() as u16
    }

    fn find_free_extent(self, requested: u16) -> Result<BlockExtent, AllocatorError> {
        if requested == 0 || requested > self.block_count {
            return Err(AllocatorError::InvalidExtent);
        }
        let mut start = 0;
        while start <= self.block_count - requested {
            let extent = BlockExtent::new(start, requested);
            if self.extent_is_free(extent)? {
                return Ok(extent);
            }
            start += 1;
        }
        Err(AllocatorError::OutOfSpace)
    }

    fn extent_is_free(self, extent: BlockExtent) -> Result<bool, AllocatorError> {
        self.validate_extent(extent)?;
        let mut offset = 0;
        while offset < extent.block_count {
            let bit = extent.start_block + offset;
            if self.bitmap & (1u64 << bit) != 0 {
                return Ok(false);
            }
            offset += 1;
        }
        Ok(true)
    }

    fn mark_extent(&mut self, extent: BlockExtent, used: bool) -> Result<(), AllocatorError> {
        self.bitmap = bitmap_with_extent(self.bitmap, extent, used)?;
        Ok(())
    }

    fn validate_extent(self, extent: BlockExtent) -> Result<(), AllocatorError> {
        if extent.block_count == 0 {
            return Err(AllocatorError::InvalidExtent);
        }
        let end = extent
            .start_block
            .checked_add(extent.block_count)
            .ok_or(AllocatorError::InvalidExtent)?;
        if end > self.block_count {
            return Err(AllocatorError::InvalidExtent);
        }
        Ok(())
    }

    pub const fn base_sector(self) -> u64 {
        self.base_sector
    }
}

pub fn recover_allocator_journal(
    journal: AllocatorJournal,
) -> Result<AllocatorRecoveryReport, AllocatorError> {
    let mut replayed_records = 0;
    let mut committed_bitmap = 0;
    while replayed_records < journal.len {
        let record = journal.records[replayed_records];
        if validate_record(record).is_err() || record.previous_bitmap != committed_bitmap {
            return Ok(AllocatorRecoveryReport {
                replayed_records,
                rolled_back_records: journal.len - replayed_records,
                committed_bitmap,
            });
        }
        committed_bitmap = record.next_bitmap;
        replayed_records += 1;
    }
    Ok(AllocatorRecoveryReport {
        replayed_records,
        rolled_back_records: 0,
        committed_bitmap,
    })
}

pub fn run_self_test(device: BlockDeviceInfo) -> Result<(), AllocatorError> {
    let mut allocator = BlockAllocator::new_for_device(device, 64, 16)?;
    let mut journal = AllocatorJournal::new();

    let first = allocator.allocate_journaled(2, &mut journal)?;
    let pending = journal.last().ok_or(AllocatorError::MissingCommitMarker)?;
    if first.start_block() != 0
        || first.block_count() != 2
        || pending.next_bitmap() != 0b11
        || pending.is_committed()
        || allocator.committed_bitmap() != 0
        || allocator.base_sector() != 64
    {
        return Err(AllocatorError::InvalidExtent);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:ALLOCATOR:BITMAP_READY");

    allocator.commit_last(&mut journal)?;
    if allocator.committed_bitmap() != 0b11 {
        return Err(AllocatorError::ChecksumMismatch);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:ALLOCATOR:METADATA_JOURNALED");

    allocator.allocate_journaled(1, &mut journal)?;
    let report = recover_allocator_journal(journal)?;
    if report.replayed_records() != 1
        || report.rolled_back_records() != 1
        || report.committed_bitmap() != 0b11
    {
        return Err(AllocatorError::MissingCommitMarker);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:ALLOCATOR:TORN_METADATA_ROLLED_BACK");

    let mut scratch = BlockAllocator::new_for_device(device, 80, 8)?;
    let left = scratch.allocate(2)?;
    let hole = scratch.allocate(2)?;
    let right = scratch.allocate(2)?;
    scratch.free(hole)?;
    let reused = scratch.allocate(1)?;
    if left.start_block() != 0
        || right.start_block() != 4
        || reused.start_block() != hole.start_block()
        || scratch.used_blocks() != 5
    {
        return Err(AllocatorError::InvalidExtent);
    }
    Ok(())
}

fn bitmap_with_extent(
    mut bitmap: u64,
    extent: BlockExtent,
    used: bool,
) -> Result<u64, AllocatorError> {
    let end = extent
        .start_block
        .checked_add(extent.block_count)
        .ok_or(AllocatorError::InvalidExtent)?;
    if end > MAX_ALLOCATOR_BLOCKS {
        return Err(AllocatorError::InvalidExtent);
    }
    let mut offset = 0;
    while offset < extent.block_count {
        let bit = extent.start_block + offset;
        if used {
            bitmap |= 1u64 << bit;
        } else {
            bitmap &= !(1u64 << bit);
        }
        offset += 1;
    }
    Ok(bitmap)
}

fn bitmap_outside_block_count(bitmap: u64, block_count: u16) -> bool {
    if block_count == MAX_ALLOCATOR_BLOCKS {
        false
    } else {
        let allowed = (1u64 << block_count) - 1;
        bitmap & !allowed != 0
    }
}

fn validate_record(record: AllocatorJournalRecord) -> Result<(), AllocatorError> {
    if record.commit_marker != COMMIT_MARKER {
        return Err(AllocatorError::MissingCommitMarker);
    }
    if record.checksum != checksum_record(record) {
        return Err(AllocatorError::ChecksumMismatch);
    }
    Ok(())
}

fn checksum_record(record: AllocatorJournalRecord) -> u32 {
    let mut checksum = 0x811C_9DC5;
    checksum = checksum_mix(checksum, record.generation);
    checksum = checksum_mix(checksum, record.previous_bitmap);
    checksum = checksum_mix(checksum, record.next_bitmap);
    checksum
}

fn checksum_mix(mut checksum: u32, value: u64) -> u32 {
    let mut remaining = value;
    let mut byte_index = 0;
    while byte_index < 8 {
        checksum ^= (remaining as u8) as u32;
        checksum = checksum.wrapping_mul(0x0100_0193);
        remaining >>= 8;
        byte_index += 1;
    }
    checksum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocator_allocates_first_fit_contiguous_blocks() {
        let mut allocator = BlockAllocator::new(64, 8).unwrap();

        let first = allocator.allocate(2).unwrap();
        let second = allocator.allocate(3).unwrap();

        assert_eq!(first.start_block(), 0);
        assert_eq!(first.block_count(), 2);
        assert_eq!(second.start_block(), 2);
        assert_eq!(second.block_count(), 3);
        assert_eq!(allocator.used_blocks(), 5);
    }

    #[test]
    fn freed_extent_is_reused_before_unallocated_tail() {
        let mut allocator = BlockAllocator::new(64, 8).unwrap();
        let _first = allocator.allocate(2).unwrap();
        let middle = allocator.allocate(2).unwrap();
        let _tail = allocator.allocate(2).unwrap();

        allocator.free(middle).unwrap();
        let reused = allocator.allocate(1).unwrap();

        assert_eq!(reused.start_block(), middle.start_block());
        assert_eq!(allocator.used_blocks(), 5);
    }

    #[test]
    fn allocator_bitmap_update_is_authoritative_only_after_commit() {
        let mut allocator = BlockAllocator::new(64, 8).unwrap();
        let mut journal = AllocatorJournal::new();

        let extent = allocator.allocate_journaled(2, &mut journal).unwrap();
        let pending = journal.last().unwrap();

        assert_eq!(pending.next_bitmap(), 0b11);
        assert!(!pending.is_committed());
        assert_eq!(allocator.committed_bitmap(), 0);

        let committed = allocator.commit_last(&mut journal).unwrap();

        assert!(committed.is_committed());
        assert_eq!(allocator.committed_bitmap(), 0b11);
        assert_eq!(extent.block_count(), 2);
    }

    #[test]
    fn recovery_rolls_back_torn_allocator_metadata() {
        let mut allocator = BlockAllocator::new(64, 8).unwrap();
        let mut journal = AllocatorJournal::new();

        allocator.allocate_journaled(1, &mut journal).unwrap();
        allocator.commit_last(&mut journal).unwrap();
        allocator.allocate_journaled(1, &mut journal).unwrap();

        let report = recover_allocator_journal(journal).unwrap();

        assert_eq!(report.replayed_records(), 1);
        assert_eq!(report.rolled_back_records(), 1);
        assert_eq!(report.committed_bitmap(), 0b1);
    }
}
