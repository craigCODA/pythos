use pythos_shared::package_abi::{
    PackageStatus, PACKAGE_CONTENT_BITMAP_WORDS, PACKAGE_CONTENT_MAX_BLOCKS,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageExtent {
    pub start_block: u16,
    pub block_count: u16,
}

impl PackageExtent {
    pub const fn new(start_block: u16, block_count: u16) -> Self {
        Self {
            start_block,
            block_count,
        }
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

#[cfg(test)]
mod tests {
    use super::{PackageExtent, PackageExtentAllocator};
    use pythos_shared::package_abi::{
        PackageStatus, PACKAGE_CONTENT_BITMAP_WORDS, PACKAGE_CONTENT_MAX_BLOCKS,
    };

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
        assert_eq!(allocator.committed_bitmap(), [0; PACKAGE_CONTENT_BITMAP_WORDS]);

        allocator.rollback_staged();

        assert_eq!(allocator.committed_bitmap(), [0; PACKAGE_CONTENT_BITMAP_WORDS]);
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
}
