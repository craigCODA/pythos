use crate::{
    block_device::{BlockDeviceInfo, SECTOR_SIZE},
    package_candidate_store::{read_package_candidate_sector, write_package_candidate_sector},
    package_registry::{PackageRegistry, PackageRegistryContentRecord},
};
use pythos_shared::package_abi::{
    MAX_CONTENT_BYTES, MAX_CONTENT_EXTENTS_PER_RECORD, PACKAGE_CONTENT_BASE_SECTOR,
    PACKAGE_CONTENT_BITMAP_WORDS, PACKAGE_CONTENT_MAX_BLOCKS, PACKAGE_CONTENT_MAX_STAGED_RECORDS,
    PackageStatus,
};
use pythos_shared::sha256::sha256;

const PACKAGE_CONTENT_BLOCK_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageExtent {
    pub start_block: u16,
    pub block_count: u16,
}

impl PackageExtent {
    pub const EMPTY: Self = Self {
        start_block: 0,
        block_count: 0,
    };

    pub const fn new(start_block: u16, block_count: u16) -> Self {
        Self {
            start_block,
            block_count,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentId {
    pub package_object_id: u64,
    pub release_digest: [u8; 32],
    pub content_index: u16,
}

impl ContentId {
    pub const fn new(package_object_id: u64, release_digest: [u8; 32], content_index: u16) -> Self {
        Self {
            package_object_id,
            release_digest,
            content_index,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageContentRecord {
    pub content_id: ContentId,
    pub role: u16,
    pub format: u16,
    pub digest: [u8; 32],
    pub byte_len: u64,
    pub extents: [PackageExtent; MAX_CONTENT_EXTENTS_PER_RECORD],
    pub extent_count: u16,
    pub retention_count: u16,
    pub committed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PackageContentSlot<'a> {
    record: PackageContentRecord,
    bytes: &'a [u8],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageContentTransaction<'a> {
    package_object_id: u64,
    release_digest: [u8; 32],
    staged: [Option<PackageContentSlot<'a>>; PACKAGE_CONTENT_MAX_STAGED_RECORDS],
    staged_count: u16,
    next_content_index: u16,
}

impl<'a> PackageContentTransaction<'a> {
    pub const fn new(package_object_id: u64, release_digest: [u8; 32]) -> Self {
        Self {
            package_object_id,
            release_digest,
            staged: [None; PACKAGE_CONTENT_MAX_STAGED_RECORDS],
            staged_count: 0,
            next_content_index: 0,
        }
    }

    pub fn reset(&mut self, package_object_id: u64, release_digest: [u8; 32]) {
        self.package_object_id = package_object_id;
        self.release_digest = release_digest;
        self.clear();
    }

    pub const fn staged_count(&self) -> usize {
        self.staged_count as usize
    }

    fn push_staged(&mut self, slot: PackageContentSlot<'a>) -> Result<(), PackageStatus> {
        if self.staged_count as usize >= PACKAGE_CONTENT_MAX_STAGED_RECORDS {
            return Err(PackageStatus::QuotaDenied);
        }

        self.staged[self.staged_count as usize] = Some(slot);
        self.staged_count += 1;
        self.next_content_index += 1;
        Ok(())
    }

    fn clear(&mut self) {
        self.staged = [None; PACKAGE_CONTENT_MAX_STAGED_RECORDS];
        self.staged_count = 0;
        self.next_content_index = 0;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageContentCommit {
    record_count: u16,
    committed_bitmap: [u64; PACKAGE_CONTENT_BITMAP_WORDS],
}

impl PackageContentCommit {
    pub const fn empty() -> Self {
        Self {
            record_count: 0,
            committed_bitmap: [0; PACKAGE_CONTENT_BITMAP_WORDS],
        }
    }

    pub const fn record_count(&self) -> usize {
        self.record_count as usize
    }

    pub const fn committed_bitmap(&self) -> [u64; PACKAGE_CONTENT_BITMAP_WORDS] {
        self.committed_bitmap
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageContentStore<'a> {
    allocator: PackageExtentAllocator,
    committed: [Option<PackageContentSlot<'a>>; PACKAGE_CONTENT_MAX_STAGED_RECORDS],
    committed_count: u16,
}

impl<'a> PackageContentStore<'a> {
    pub const fn empty() -> Self {
        Self {
            allocator: PackageExtentAllocator::empty(),
            committed: [None; PACKAGE_CONTENT_MAX_STAGED_RECORDS],
            committed_count: 0,
        }
    }

    pub fn stage_content(
        &mut self,
        transaction: &mut PackageContentTransaction<'a>,
        role: u16,
        format: u16,
        bytes: &'a [u8],
        expected_digest: [u8; 32],
    ) -> Result<ContentId, PackageStatus> {
        if bytes.len() > MAX_CONTENT_BYTES {
            return Err(PackageStatus::BoundsExceeded);
        }
        if sha256(bytes) != expected_digest {
            return Err(PackageStatus::DigestMismatch);
        }

        let block_count = blocks_for_len(bytes.len())?;
        let mut extents = [PackageExtent::EMPTY; MAX_CONTENT_EXTENTS_PER_RECORD];
        let extent_count;
        if block_count == 0 {
            extent_count = 0;
        } else {
            extents[0] = self.allocator.allocate_staged(block_count)?;
            extent_count = 1;
        }

        let content_id = ContentId::new(
            transaction.package_object_id,
            transaction.release_digest,
            transaction.next_content_index,
        );
        let record = PackageContentRecord {
            content_id,
            role,
            format,
            digest: expected_digest,
            byte_len: bytes.len() as u64,
            extents,
            extent_count,
            retention_count: 0,
            committed: false,
        };

        if let Err(error) = transaction.push_staged(PackageContentSlot { record, bytes }) {
            if block_count != 0 {
                self.allocator.rollback_staged();
            }
            return Err(error);
        }

        Ok(content_id)
    }

    pub fn read_committed(&self, content_id: ContentId) -> Result<&'a [u8], PackageStatus> {
        for slot in self.committed.iter().flatten() {
            if slot.record.content_id == content_id {
                if slot.record.byte_len != slot.bytes.len() as u64
                    || sha256(slot.bytes) != slot.record.digest
                {
                    return Err(PackageStatus::ContentCorrupt);
                }
                return Ok(slot.bytes);
            }
        }
        Err(PackageStatus::NotFound)
    }

    pub fn rollback(&mut self, transaction: &mut PackageContentTransaction<'a>) {
        self.allocator.rollback_staged();
        transaction.clear();
    }

    pub fn commit(
        &mut self,
        transaction: &mut PackageContentTransaction<'a>,
    ) -> Result<PackageContentCommit, PackageStatus> {
        if (self.committed_count as usize + transaction.staged_count as usize)
            > PACKAGE_CONTENT_MAX_STAGED_RECORDS
        {
            return Err(PackageStatus::QuotaDenied);
        }

        let mut record_count = 0usize;

        for slot_index in 0..PACKAGE_CONTENT_MAX_STAGED_RECORDS {
            if let Some(mut slot) = transaction.staged[slot_index] {
                slot.record.committed = true;
                self.committed[self.committed_count as usize] = Some(slot);
                self.committed_count += 1;
                record_count += 1;
            }
        }

        let committed_bitmap = self.allocator.commit_staged();
        transaction.clear();

        Ok(PackageContentCommit {
            record_count: record_count as u16,
            committed_bitmap,
        })
    }

    pub const fn committed_bitmap(&self) -> [u64; PACKAGE_CONTENT_BITMAP_WORDS] {
        self.allocator.committed_bitmap()
    }

    pub const fn staged_bitmap(&self) -> [u64; PACKAGE_CONTENT_BITMAP_WORDS] {
        self.allocator.staged_bitmap()
    }

    pub fn add_staged_records_to_registry(
        &self,
        transaction: &PackageContentTransaction<'a>,
        registry: &mut PackageRegistry,
    ) -> Result<(), PackageStatus> {
        for slot in transaction.staged.iter().flatten() {
            registry.add_content_record(PackageRegistryContentRecord {
                package_object_id: slot.record.content_id.package_object_id,
                release_digest: slot.record.content_id.release_digest,
                content_index: slot.record.content_id.content_index,
                role: slot.record.role,
                format: slot.record.format,
                digest: slot.record.digest,
                byte_len: slot.record.byte_len,
                extents: slot.record.extents,
                extent_count: slot.record.extent_count,
                retention_count: slot.record.retention_count,
                flags: 0,
            })?;
        }
        Ok(())
    }

    pub fn write_candidate_content(
        &self,
        device: BlockDeviceInfo,
        transaction: &PackageContentTransaction<'a>,
    ) -> Result<PackageContentCommit, PackageStatus> {
        let mut bitmap = self.allocator.committed_bitmap();
        let mut record_count = 0u16;
        for slot in transaction.staged.iter().flatten() {
            write_record_bytes(device, slot.record, slot.bytes)?;
            mark_record_extents(&mut bitmap, slot.record.extents, slot.record.extent_count)?;
            record_count = record_count
                .checked_add(1)
                .ok_or(PackageStatus::LengthOverflow)?;
        }
        Ok(PackageContentCommit {
            record_count,
            committed_bitmap: bitmap,
        })
    }

    pub fn read_validate_candidate_content(
        device: BlockDeviceInfo,
        registry: &PackageRegistry,
    ) -> Result<PackageContentCommit, PackageStatus> {
        let mut bitmap = [0u64; PACKAGE_CONTENT_BITMAP_WORDS];
        let mut record_count = 0u16;
        let mut index = 0usize;
        while let Some(record) = registry.content_record(index) {
            validate_record_bytes(device, record)?;
            mark_record_extents(&mut bitmap, record.extents, record.extent_count)?;
            record_count = record_count
                .checked_add(1)
                .ok_or(PackageStatus::LengthOverflow)?;
            index += 1;
        }
        Ok(PackageContentCommit {
            record_count,
            committed_bitmap: bitmap,
        })
    }

    pub fn live_bitmap_from_registry(
        registry: &PackageRegistry,
    ) -> Result<[u64; PACKAGE_CONTENT_BITMAP_WORDS], PackageStatus> {
        let mut bitmap = [0u64; PACKAGE_CONTENT_BITMAP_WORDS];
        let mut index = 0usize;
        while let Some(record) = registry.content_record(index) {
            mark_record_extents(&mut bitmap, record.extents, record.extent_count)?;
            index += 1;
        }
        Ok(bitmap)
    }

    pub fn extent_live_in_registry(
        registry: &PackageRegistry,
        extent: PackageExtent,
    ) -> Result<bool, PackageStatus> {
        validate_extent(extent)?;
        let bitmap = Self::live_bitmap_from_registry(registry)?;
        let end_block = extent.start_block + extent.block_count;
        for block in extent.start_block..end_block {
            if !bit_is_set(&bitmap, block) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageExtentAllocator {
    committed: [u64; PACKAGE_CONTENT_BITMAP_WORDS],
    staged: [u64; PACKAGE_CONTENT_BITMAP_WORDS],
}

impl PackageExtentAllocator {
    pub const fn empty() -> Self {
        Self {
            committed: [0; PACKAGE_CONTENT_BITMAP_WORDS],
            staged: [0; PACKAGE_CONTENT_BITMAP_WORDS],
        }
    }

    pub fn restore(bitmap: [u64; PACKAGE_CONTENT_BITMAP_WORDS]) -> Result<Self, PackageStatus> {
        Ok(Self {
            committed: bitmap,
            staged: [0; PACKAGE_CONTENT_BITMAP_WORDS],
        })
    }

    pub fn allocate_staged(&mut self, block_count: u16) -> Result<PackageExtent, PackageStatus> {
        validate_extent(PackageExtent::new(0, block_count))?;

        let max_start = PACKAGE_CONTENT_MAX_BLOCKS - block_count;
        for start_block in 0..=max_start {
            if self.range_is_free(start_block, block_count) {
                mark_range(&mut self.staged, start_block, block_count, true);
                return Ok(PackageExtent::new(start_block, block_count));
            }
        }

        Err(PackageStatus::QuotaDenied)
    }

    pub fn rollback_staged(&mut self) {
        self.staged = [0; PACKAGE_CONTENT_BITMAP_WORDS];
    }

    pub fn commit_staged(&mut self) -> [u64; PACKAGE_CONTENT_BITMAP_WORDS] {
        for word in 0..PACKAGE_CONTENT_BITMAP_WORDS {
            self.committed[word] |= self.staged[word];
        }
        self.rollback_staged();
        self.committed
    }

    pub fn free_committed(&mut self, extent: PackageExtent) -> Result<(), PackageStatus> {
        validate_extent(extent)?;
        let end_block = extent.start_block + extent.block_count;
        for block in extent.start_block..end_block {
            if !bit_is_set(&self.committed, block) {
                return Err(PackageStatus::NotFound);
            }
        }

        mark_range(
            &mut self.committed,
            extent.start_block,
            extent.block_count,
            false,
        );
        Ok(())
    }

    pub const fn committed_bitmap(&self) -> [u64; PACKAGE_CONTENT_BITMAP_WORDS] {
        self.committed
    }

    pub const fn staged_bitmap(&self) -> [u64; PACKAGE_CONTENT_BITMAP_WORDS] {
        self.staged
    }

    pub fn is_committed_allocated(&self, block: u16) -> bool {
        if block >= PACKAGE_CONTENT_MAX_BLOCKS {
            return false;
        }
        bit_is_set(&self.committed, block)
    }

    fn range_is_free(&self, start_block: u16, block_count: u16) -> bool {
        let end_block = start_block + block_count;
        for block in start_block..end_block {
            if bit_is_set(&self.committed, block) || bit_is_set(&self.staged, block) {
                return false;
            }
        }
        true
    }
}

fn validate_extent(extent: PackageExtent) -> Result<(), PackageStatus> {
    if extent.block_count == 0 {
        return Err(PackageStatus::BoundsExceeded);
    }

    let end = u32::from(extent.start_block) + u32::from(extent.block_count);
    if end > u32::from(PACKAGE_CONTENT_MAX_BLOCKS) {
        return Err(PackageStatus::BoundsExceeded);
    }

    Ok(())
}

fn bit_is_set(bitmap: &[u64; PACKAGE_CONTENT_BITMAP_WORDS], block: u16) -> bool {
    let block = block as usize;
    let word = block / 64;
    let bit = block % 64;
    (bitmap[word] & (1u64 << bit)) != 0
}

fn mark_range(
    bitmap: &mut [u64; PACKAGE_CONTENT_BITMAP_WORDS],
    start_block: u16,
    block_count: u16,
    allocated: bool,
) {
    let end_block = start_block + block_count;
    for block in start_block..end_block {
        let block = block as usize;
        let word = block / 64;
        let bit = block % 64;
        let mask = 1u64 << bit;
        if allocated {
            bitmap[word] |= mask;
        } else {
            bitmap[word] &= !mask;
        }
    }
}

fn blocks_for_len(byte_len: usize) -> Result<u16, PackageStatus> {
    if byte_len == 0 {
        return Ok(0);
    }
    let blocks = byte_len
        .checked_add(PACKAGE_CONTENT_BLOCK_BYTES - 1)
        .ok_or(PackageStatus::LengthOverflow)?
        / PACKAGE_CONTENT_BLOCK_BYTES;
    if blocks > PACKAGE_CONTENT_MAX_BLOCKS as usize {
        return Err(PackageStatus::BoundsExceeded);
    }
    Ok(blocks as u16)
}

fn write_record_bytes(
    device: BlockDeviceInfo,
    record: PackageContentRecord,
    bytes: &[u8],
) -> Result<(), PackageStatus> {
    if record.byte_len != bytes.len() as u64 || sha256(bytes) != record.digest {
        return Err(PackageStatus::ContentCorrupt);
    }
    let mut byte_offset = 0usize;
    let mut extent_index = 0usize;
    while extent_index < record.extent_count as usize {
        let extent = record.extents[extent_index];
        validate_extent(extent)?;
        for block_offset in 0..extent.block_count {
            let mut sector = [0u8; SECTOR_SIZE];
            let remaining = bytes.len().saturating_sub(byte_offset);
            let copy_len = core::cmp::min(remaining, SECTOR_SIZE);
            if copy_len != 0 {
                sector[..copy_len].copy_from_slice(&bytes[byte_offset..byte_offset + copy_len]);
                byte_offset += copy_len;
            }
            let sector_number = PACKAGE_CONTENT_BASE_SECTOR
                .checked_add(u64::from(extent.start_block) + u64::from(block_offset))
                .ok_or(PackageStatus::LengthOverflow)?;
            write_package_candidate_sector(device, sector_number, &sector)
                .map_err(|_| PackageStatus::RegistryWriteDenied)?;
        }
        extent_index += 1;
    }
    if byte_offset != bytes.len() {
        return Err(PackageStatus::BoundsExceeded);
    }
    validate_stored_bytes(
        device,
        record.digest,
        record.byte_len,
        record.extents,
        record.extent_count,
    )
}

fn validate_record_bytes(
    device: BlockDeviceInfo,
    record: PackageRegistryContentRecord,
) -> Result<(), PackageStatus> {
    validate_stored_bytes(
        device,
        record.digest,
        record.byte_len,
        record.extents,
        record.extent_count,
    )
}

fn validate_stored_bytes(
    device: BlockDeviceInfo,
    expected_digest: [u8; 32],
    byte_len: u64,
    extents: [PackageExtent; MAX_CONTENT_EXTENTS_PER_RECORD],
    extent_count: u16,
) -> Result<(), PackageStatus> {
    if byte_len > MAX_CONTENT_BYTES as u64 || extent_count as usize > MAX_CONTENT_EXTENTS_PER_RECORD
    {
        return Err(PackageStatus::BoundsExceeded);
    }
    let expected_blocks = byte_len
        .checked_add((SECTOR_SIZE - 1) as u64)
        .ok_or(PackageStatus::LengthOverflow)?
        / SECTOR_SIZE as u64;
    let mut blocks = 0u64;
    let mut hasher = pythos_shared::sha256::Sha256::new();
    let mut remaining = byte_len as usize;
    let mut extent_index = 0usize;
    while extent_index < extent_count as usize {
        let extent = extents[extent_index];
        validate_extent(extent)?;
        blocks += u64::from(extent.block_count);
        for block_offset in 0..extent.block_count {
            let sector_number = PACKAGE_CONTENT_BASE_SECTOR
                .checked_add(u64::from(extent.start_block) + u64::from(block_offset))
                .ok_or(PackageStatus::LengthOverflow)?;
            let sector = read_package_candidate_sector(device, sector_number)
                .map_err(|_| PackageStatus::ContentCorrupt)?;
            let take = core::cmp::min(remaining, SECTOR_SIZE);
            hasher.update(&sector[..take]);
            remaining -= take;
        }
        extent_index += 1;
    }
    if blocks != expected_blocks || remaining != 0 || hasher.finalize() != expected_digest {
        return Err(PackageStatus::ContentCorrupt);
    }
    Ok(())
}

fn mark_record_extents(
    bitmap: &mut [u64; PACKAGE_CONTENT_BITMAP_WORDS],
    extents: [PackageExtent; MAX_CONTENT_EXTENTS_PER_RECORD],
    extent_count: u16,
) -> Result<(), PackageStatus> {
    if extent_count as usize > MAX_CONTENT_EXTENTS_PER_RECORD {
        return Err(PackageStatus::BoundsExceeded);
    }
    let mut index = 0usize;
    while index < extent_count as usize {
        let extent = extents[index];
        validate_extent(extent)?;
        mark_range(bitmap, extent.start_block, extent.block_count, true);
        index += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ContentId, PackageContentStore, PackageContentTransaction, PackageExtent,
        PackageExtentAllocator,
    };
    use crate::{
        block_device::BlockDeviceInfo,
        package_candidate_store::{
            PACKAGE_CANDIDATE_STORAGE_TEST_LOCK, read_candidate_registry_generation,
            reset_package_candidate_storage_for_test, write_candidate_registry_generation,
        },
        package_registry::PackageRegistry,
    };
    use pythos_shared::package_abi::{
        PACKAGE_CONTENT_BITMAP_WORDS, PACKAGE_CONTENT_MAX_BLOCKS, PackageStatus,
    };
    use pythos_shared::sha256::sha256;

    #[test]
    fn package_extent_allocator_allocates_all_blocks_and_denies_8193rd() {
        let mut allocator = PackageExtentAllocator::empty();

        for block in 0..PACKAGE_CONTENT_MAX_BLOCKS {
            let extent = allocator.allocate_staged(1).unwrap();

            assert_eq!(extent, PackageExtent::new(block, 1));
        }

        assert_eq!(
            allocator.allocate_staged(1),
            Err(PackageStatus::QuotaDenied)
        );
    }

    #[test]
    fn package_extent_allocator_rolls_back_staged_blocks_without_committing() {
        let mut allocator = PackageExtentAllocator::empty();

        assert_eq!(
            allocator.allocate_staged(2).unwrap(),
            PackageExtent::new(0, 2)
        );
        assert_eq!(
            allocator.committed_bitmap(),
            [0; PACKAGE_CONTENT_BITMAP_WORDS]
        );

        allocator.rollback_staged();

        assert_eq!(
            allocator.committed_bitmap(),
            [0; PACKAGE_CONTENT_BITMAP_WORDS]
        );
        assert_eq!(allocator.staged_bitmap(), [0; PACKAGE_CONTENT_BITMAP_WORDS]);
        assert_eq!(
            allocator.allocate_staged(1).unwrap(),
            PackageExtent::new(0, 1)
        );
    }

    #[test]
    fn package_extent_allocator_commits_and_restores_bitmap() {
        let mut allocator = PackageExtentAllocator::empty();

        assert_eq!(
            allocator.allocate_staged(3).unwrap(),
            PackageExtent::new(0, 3)
        );
        let committed = allocator.commit_staged();
        assert_ne!(committed, [0; PACKAGE_CONTENT_BITMAP_WORDS]);
        assert_eq!(allocator.staged_bitmap(), [0; PACKAGE_CONTENT_BITMAP_WORDS]);

        let mut restored = PackageExtentAllocator::restore(committed).unwrap();

        assert!(restored.is_committed_allocated(0));
        assert!(restored.is_committed_allocated(1));
        assert!(restored.is_committed_allocated(2));
        assert_eq!(
            restored.allocate_staged(1).unwrap(),
            PackageExtent::new(3, 1)
        );
    }

    #[test]
    fn package_extent_allocator_rejects_extents_outside_block_range() {
        let mut allocator = PackageExtentAllocator::empty();

        assert_eq!(
            allocator.free_committed(PackageExtent::new(PACKAGE_CONTENT_MAX_BLOCKS, 1)),
            Err(PackageStatus::BoundsExceeded)
        );
        assert_eq!(
            allocator.free_committed(PackageExtent::new(PACKAGE_CONTENT_MAX_BLOCKS - 1, 2)),
            Err(PackageStatus::BoundsExceeded)
        );
        assert_eq!(
            allocator.free_committed(PackageExtent::new(0, 0)),
            Err(PackageStatus::BoundsExceeded)
        );
    }

    #[test]
    fn package_content_store_stages_bytes_and_rejects_digest_mismatch() {
        let mut store = PackageContentStore::empty();
        let mut transaction = PackageContentTransaction::new(7, release_digest(1));

        assert_eq!(
            store.stage_content(&mut transaction, 1, 0, b"schema", [0xAA; 32]),
            Err(PackageStatus::DigestMismatch)
        );
        assert_eq!(transaction.staged_count(), 0);
        assert_eq!(
            store.read_committed(ContentId::new(7, release_digest(1), 0)),
            Err(PackageStatus::NotFound)
        );
    }

    #[test]
    fn package_content_store_hides_staged_content_until_commit() {
        let mut store = PackageContentStore::empty();
        let release = release_digest(2);
        let bytes = b"graph-package";
        let mut transaction = PackageContentTransaction::new(11, release);

        let content_id = store
            .stage_content(&mut transaction, 2, 1, bytes, sha256(bytes))
            .unwrap();

        assert_eq!(content_id, ContentId::new(11, release, 0));
        assert_eq!(
            store.read_committed(content_id),
            Err(PackageStatus::NotFound)
        );

        let commit = store.commit(&mut transaction).unwrap();

        assert_eq!(commit.record_count(), 1);
        assert_eq!(commit.committed_bitmap(), store.committed_bitmap());
        assert_eq!(store.read_committed(content_id), Ok(bytes.as_slice()));
    }

    #[test]
    fn package_content_store_rolls_back_staged_extents() {
        let mut store = PackageContentStore::empty();
        let release = release_digest(3);
        let bytes = b"temporary";
        let mut transaction = PackageContentTransaction::new(13, release);

        assert_eq!(
            store
                .stage_content(&mut transaction, 1, 0, bytes, sha256(bytes))
                .unwrap(),
            ContentId::new(13, release, 0)
        );

        store.rollback(&mut transaction);

        assert_eq!(transaction.staged_count(), 0);
        assert_eq!(store.staged_bitmap(), [0; PACKAGE_CONTENT_BITMAP_WORDS]);
        assert_eq!(
            store
                .stage_content(&mut transaction, 1, 0, bytes, sha256(bytes))
                .unwrap(),
            ContentId::new(13, release, 0)
        );
    }

    #[test]
    fn package_content_store_content_id_is_scoped_to_package_and_release() {
        let bytes = b"shared";
        let digest = sha256(bytes);
        let release_a = release_digest(4);
        let release_b = release_digest(5);
        let mut store = PackageContentStore::empty();
        let mut package_a = PackageContentTransaction::new(21, release_a);
        let mut package_b = PackageContentTransaction::new(22, release_a);
        let mut package_a_new_release = PackageContentTransaction::new(21, release_b);

        let id_a = store
            .stage_content(&mut package_a, 3, 0, bytes, digest)
            .unwrap();
        store.rollback(&mut package_a);
        let id_b = store
            .stage_content(&mut package_b, 3, 0, bytes, digest)
            .unwrap();
        store.rollback(&mut package_b);
        let id_a_new_release = store
            .stage_content(&mut package_a_new_release, 3, 0, bytes, digest)
            .unwrap();

        assert_eq!(id_a, ContentId::new(21, release_a, 0));
        assert_eq!(id_b, ContentId::new(22, release_a, 0));
        assert_eq!(id_a_new_release, ContentId::new(21, release_b, 0));
        assert_ne!(id_a, id_b);
        assert_ne!(id_a, id_a_new_release);
    }

    #[test]
    fn package_candidate_content_bytes_survive_reconstruction_without_liveness() {
        static CONTENT: &[u8] = b"candidate-content";

        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_candidate_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let mut store = PackageContentStore::empty();
        let mut transaction = PackageContentTransaction::new(42, sha256(b"release"));
        store
            .stage_content(&mut transaction, 1, 1, CONTENT, sha256(CONTENT))
            .unwrap();
        let mut registry = PackageRegistry::empty();
        store
            .add_staged_records_to_registry(&transaction, &mut registry)
            .unwrap();
        let generation = write_candidate_registry_generation(device, &registry).unwrap();
        let restored = read_candidate_registry_generation(device, generation).unwrap();
        let expected_extent = restored.content_record(0).unwrap().extents[0];

        let written = store.write_candidate_content(device, &transaction).unwrap();
        drop(transaction);
        drop(store);
        let validated =
            PackageContentStore::read_validate_candidate_content(device, &restored).unwrap();

        assert_eq!(validated, written);
        assert_eq!(validated.record_count(), 1);
        assert_eq!(
            PackageContentStore::live_bitmap_from_registry(&PackageRegistry::empty()).unwrap(),
            [0; PACKAGE_CONTENT_BITMAP_WORDS]
        );
        assert!(
            !PackageContentStore::extent_live_in_registry(
                &PackageRegistry::empty(),
                expected_extent,
            )
            .unwrap()
        );
    }

    fn release_digest(seed: u8) -> [u8; 32] {
        [seed; 32]
    }
}
