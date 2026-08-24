//! Physical page ownership and bitmap allocation.

use core::cell::UnsafeCell;
use core::mem;
#[cfg(not(test))]
use core::ptr;
use pythos_shared::boot_protocol::PythBootInfo;

pub const PAGE_SIZE: u64 = 4096;
const MAX_MANAGED_PHYSICAL: u64 = 4 * 1024 * 1024 * 1024;
const MAX_MANAGED_PAGES: usize = (MAX_MANAGED_PHYSICAL / PAGE_SIZE) as usize;
const BITMAP_WORDS: usize = MAX_MANAGED_PAGES / 64;
#[cfg(not(test))]
const MIN_ALLOCATABLE_PHYSICAL: u64 = 2 * 1024 * 1024;

#[cfg(test)]
pub const EFI_BOOT_SERVICES_DATA: u32 = 4;
pub const EFI_CONVENTIONAL_MEMORY: u32 = 7;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawMemoryDescriptor {
    pub memory_type: u32,
    _padding: u32,
    pub physical_start: u64,
    _virtual_start: u64,
    pub number_of_pages: u64,
    _attribute: u64,
}

impl RawMemoryDescriptor {
    #[cfg(test)]
    fn for_test(memory_type: u32, physical_start: u64, number_of_pages: u64) -> Self {
        Self {
            memory_type,
            _padding: 0,
            physical_start,
            _virtual_start: 0,
            number_of_pages,
            _attribute: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageOwner {
    Free,
    Reserved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryError {
    BadDescriptorSize,
    BadMemoryMap,
    RangeOverflow,
    OutOfManagedRange,
    #[cfg(not(test))]
    NotAllocated,
    #[cfg(not(test))]
    OutOfMemory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageRange {
    start_page: u64,
    end_page: u64,
}

impl PageRange {
    pub fn new(start: u64, end: u64) -> Result<Self, MemoryError> {
        if start >= end {
            return Err(MemoryError::BadMemoryMap);
        }
        let start_page = start / PAGE_SIZE;
        let end_page = end
            .checked_add(PAGE_SIZE - 1)
            .ok_or(MemoryError::RangeOverflow)?
            / PAGE_SIZE;
        if end_page as usize > MAX_MANAGED_PAGES {
            return Err(MemoryError::OutOfManagedRange);
        }
        Ok(Self {
            start_page,
            end_page,
        })
    }

    fn contains_page(&self, page: u64) -> bool {
        page >= self.start_page && page < self.end_page
    }
}

#[cfg(test)]
#[derive(Debug, Eq, PartialEq)]
pub struct MemoryPlan<'a> {
    descriptors: &'a [RawMemoryDescriptor],
    reserved: &'a [PageRange],
    pub total_pages: u64,
    pub free_pages: u64,
    pub reserved_pages: u64,
}

#[cfg(test)]
impl MemoryPlan<'_> {
    pub fn owner_at(&self, physical: u64) -> Option<PageOwner> {
        if !physical.is_multiple_of(PAGE_SIZE) {
            return None;
        }
        let page = physical / PAGE_SIZE;
        for range in self.reserved {
            if range.contains_page(page) {
                return Some(PageOwner::Reserved);
            }
        }
        for descriptor in self.descriptors {
            let start_page = descriptor.physical_start / PAGE_SIZE;
            let end_page = descriptor_end_page(descriptor).ok()?;
            if page >= start_page && page < end_page {
                return if descriptor.memory_type == EFI_CONVENTIONAL_MEMORY {
                    Some(PageOwner::Free)
                } else {
                    Some(PageOwner::Reserved)
                };
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalMemory {
    pub total_pages: u64,
    pub free_pages: u64,
    pub reserved_pages: u64,
}

struct BitmapStorage(UnsafeCell<[u64; BITMAP_WORDS]>);

// SAFETY: milestone 1 executes on one CPU with interrupts disabled during
// physical-memory initialization. All access goes through `initialize()`.
unsafe impl Sync for BitmapStorage {}

static BITMAP: BitmapStorage = BitmapStorage(UnsafeCell::new([u64::MAX; BITMAP_WORDS]));

pub fn initialize(boot_info: &PythBootInfo) -> Result<PhysicalMemory, MemoryError> {
    if boot_info.memory_descriptor_size < mem::size_of::<RawMemoryDescriptor>() as u32
        || boot_info.memory_map_len == 0
        || !boot_info
            .memory_map_len
            .is_multiple_of(u64::from(boot_info.memory_descriptor_size))
    {
        return Err(MemoryError::BadDescriptorSize);
    }

    let reserved = required_reserved_ranges(boot_info)?;
    let descriptor_count = boot_info.memory_map_len / u64::from(boot_info.memory_descriptor_size);
    let mut total_pages = 0u64;
    let mut free_pages = 0u64;
    let mut reserved_pages = 0u64;

    // SAFETY:
    // 1. Invariant: the global bitmap is only mutably accessed during this
    //    single initialization call.
    // 2. Established by: milestone-1 entry is single-core with interrupts
    //    disabled and no allocator users before `initialize()`.
    // 3. Lifetime: the bitmap is static and remains mapped for all PythCore.
    // 4. Pointer ownership: PythCore owns this static storage exclusively.
    // 5. Alignment: `UnsafeCell<[u64; N]>` preserves `u64` alignment.
    // 6. Mapped length: exactly `BITMAP_WORDS * size_of::<u64>()` bytes.
    // 7. Concurrency: no other CPU or interrupt handler can access it.
    // 8. Violation: concurrent access would corrupt allocator ownership bits.
    let bitmap = unsafe { &mut *BITMAP.0.get() };
    bitmap.fill(u64::MAX);

    for index in 0..descriptor_count {
        let descriptor = read_descriptor(boot_info, index)?;
        let start_page = descriptor.physical_start / PAGE_SIZE;
        let end_page = descriptor_end_page(&descriptor)?;
        if end_page as usize > MAX_MANAGED_PAGES {
            continue;
        }
        for page in start_page..end_page {
            total_pages = total_pages
                .checked_add(1)
                .ok_or(MemoryError::RangeOverflow)?;
            let owner = page_owner(&descriptor, &reserved, page);
            match owner {
                PageOwner::Free => {
                    clear_allocated(bitmap, page as usize);
                    free_pages = free_pages
                        .checked_add(1)
                        .ok_or(MemoryError::RangeOverflow)?;
                }
                PageOwner::Reserved => {
                    set_allocated(bitmap, page as usize);
                    reserved_pages = reserved_pages
                        .checked_add(1)
                        .ok_or(MemoryError::RangeOverflow)?;
                }
            }
        }
    }

    if total_pages == 0 || free_pages == 0 {
        return Err(MemoryError::BadMemoryMap);
    }
    Ok(PhysicalMemory {
        total_pages,
        free_pages,
        reserved_pages,
    })
}

#[cfg(not(test))]
impl PhysicalMemory {
    pub fn allocate_zeroed_page(&mut self) -> Result<u64, MemoryError> {
        let physical = self.allocate_unzeroed_page()?;
        // SAFETY:
        // 1. Invariant: `physical` is a free conventional page above the low
        //    guard and currently reachable through the loader's temporary
        //    identity mapping.
        // 2. Established by: `allocate_unzeroed_page` selecting the frame from
        //    the allocator bitmap during early pre-activation initialization.
        // 3. Lifetime: this function initializes the page before handing
        //    exclusive ownership to its caller.
        // 4. Pointer ownership: ownership transfers from the free bitmap to the
        //    caller, and no other owner can access the page.
        // 5. Alignment: pages are 4 KiB aligned by construction.
        // 6. Mapped length: exactly one 4 KiB page is written.
        // 7. Concurrency: single-core execution with interrupts disabled.
        // 8. Violation: calling this after broad identity removal would fault;
        //    late callers must use a VM-provided mapping before initializing.
        unsafe {
            ptr::write_bytes(physical as *mut u8, 0, PAGE_SIZE as usize);
        }
        Ok(physical)
    }

    pub fn allocate_unzeroed_page(&mut self) -> Result<u64, MemoryError> {
        // SAFETY:
        // 1. Invariant: the global bitmap is only mutably accessed by early
        //    PythCore initialization while interrupts remain disabled.
        // 2. Established by: the milestone-1.5 boot contract, before any
        //    scheduler, interrupt allocation, or second CPU exists.
        // 3. Lifetime: the bitmap remains static for all early PythCore.
        // 4. Pointer ownership: PythCore exclusively owns this bitmap.
        // 5. Alignment: `UnsafeCell<[u64; N]>` preserves `u64` alignment.
        // 6. Mapped length: exactly `BITMAP_WORDS * size_of::<u64>()` bytes.
        // 7. Concurrency: single-core execution with interrupts disabled.
        // 8. Violation: concurrent mutation would corrupt page ownership.
        let bitmap = unsafe { &mut *BITMAP.0.get() };
        let start_page = (MIN_ALLOCATABLE_PHYSICAL / PAGE_SIZE) as usize;
        for page in start_page..MAX_MANAGED_PAGES {
            if bitmap[page / 64] & (1u64 << (page % 64)) == 0 {
                set_allocated(bitmap, page);
                self.free_pages = self
                    .free_pages
                    .checked_sub(1)
                    .ok_or(MemoryError::BadMemoryMap)?;
                self.reserved_pages = self
                    .reserved_pages
                    .checked_add(1)
                    .ok_or(MemoryError::RangeOverflow)?;
                let physical = (page as u64)
                    .checked_mul(PAGE_SIZE)
                    .ok_or(MemoryError::RangeOverflow)?;
                return Ok(physical);
            }
        }
        Err(MemoryError::OutOfMemory)
    }

    pub fn release_allocated_page(&mut self, physical: u64) -> Result<(), MemoryError> {
        if !physical.is_multiple_of(PAGE_SIZE) {
            return Err(MemoryError::BadMemoryMap);
        }
        let page = physical / PAGE_SIZE;
        if page as usize >= MAX_MANAGED_PAGES || physical < MIN_ALLOCATABLE_PHYSICAL {
            return Err(MemoryError::OutOfManagedRange);
        }

        // SAFETY:
        // 1. Invariant: the global bitmap is only mutably accessed by early
        //    PythCore initialization while interrupts remain disabled.
        // 2. Established by: Phase 8 process-termination runs before SMP,
        //    asynchronous allocator use, or user-mode task migration exists.
        // 3. Lifetime: the bitmap remains static for all early PythCore.
        // 4. Pointer ownership: PythCore exclusively owns this bitmap.
        // 5. Alignment: `UnsafeCell<[u64; N]>` preserves `u64` alignment.
        // 6. Mapped length: exactly `BITMAP_WORDS * size_of::<u64>()` bytes.
        // 7. Concurrency: single-core execution with interrupts disabled.
        // 8. Violation: concurrent mutation would corrupt page ownership.
        let bitmap = unsafe { &mut *BITMAP.0.get() };
        let index = page as usize;
        if bitmap[index / 64] & (1u64 << (index % 64)) == 0 {
            return Err(MemoryError::NotAllocated);
        }
        clear_allocated(bitmap, index);
        self.free_pages = self
            .free_pages
            .checked_add(1)
            .ok_or(MemoryError::RangeOverflow)?;
        self.reserved_pages = self
            .reserved_pages
            .checked_sub(1)
            .ok_or(MemoryError::BadMemoryMap)?;
        Ok(())
    }
}

#[cfg(test)]
pub fn build_plan<'a>(
    descriptors: &'a [RawMemoryDescriptor],
    reserved: &'a [PageRange],
) -> Result<MemoryPlan<'a>, MemoryError> {
    let mut total_pages = 0u64;
    let mut free_pages = 0u64;
    let mut reserved_pages = 0u64;
    for descriptor in descriptors {
        let start_page = descriptor.physical_start / PAGE_SIZE;
        let end_page = descriptor_end_page(descriptor)?;
        if end_page as usize > MAX_MANAGED_PAGES {
            return Err(MemoryError::OutOfManagedRange);
        }
        for page in start_page..end_page {
            total_pages = total_pages
                .checked_add(1)
                .ok_or(MemoryError::RangeOverflow)?;
            match page_owner(descriptor, reserved, page) {
                PageOwner::Free => {
                    free_pages = free_pages
                        .checked_add(1)
                        .ok_or(MemoryError::RangeOverflow)?;
                }
                PageOwner::Reserved => {
                    reserved_pages = reserved_pages
                        .checked_add(1)
                        .ok_or(MemoryError::RangeOverflow)?;
                }
            }
        }
    }
    Ok(MemoryPlan {
        descriptors,
        reserved,
        total_pages,
        free_pages,
        reserved_pages,
    })
}

fn required_reserved_ranges(boot_info: &PythBootInfo) -> Result<[PageRange; 4], MemoryError> {
    Ok([
        PageRange::new(boot_info.kernel_phys_start, boot_info.kernel_phys_end)?,
        PageRange::new(
            boot_info.init_bundle_phys,
            boot_info
                .init_bundle_phys
                .checked_add(boot_info.init_bundle_len)
                .ok_or(MemoryError::RangeOverflow)?,
        )?,
        PageRange::new(
            boot_info.font_phys,
            boot_info
                .font_phys
                .checked_add(boot_info.font_len)
                .ok_or(MemoryError::RangeOverflow)?,
        )?,
        PageRange::new(
            boot_info.framebuffer.physical_base,
            boot_info
                .framebuffer
                .physical_base
                .checked_add(boot_info.framebuffer.byte_length)
                .ok_or(MemoryError::RangeOverflow)?,
        )?,
    ])
}

fn page_owner(descriptor: &RawMemoryDescriptor, reserved: &[PageRange], page: u64) -> PageOwner {
    if descriptor.memory_type != EFI_CONVENTIONAL_MEMORY {
        return PageOwner::Reserved;
    }
    if reserved.iter().any(|range| range.contains_page(page)) {
        return PageOwner::Reserved;
    }
    PageOwner::Free
}

fn descriptor_end_page(descriptor: &RawMemoryDescriptor) -> Result<u64, MemoryError> {
    if !descriptor.physical_start.is_multiple_of(PAGE_SIZE) || descriptor.number_of_pages == 0 {
        return Err(MemoryError::BadMemoryMap);
    }
    let byte_len = descriptor
        .number_of_pages
        .checked_mul(PAGE_SIZE)
        .ok_or(MemoryError::RangeOverflow)?;
    let end = descriptor
        .physical_start
        .checked_add(byte_len)
        .ok_or(MemoryError::RangeOverflow)?;
    Ok(end / PAGE_SIZE)
}

fn read_descriptor(
    boot_info: &PythBootInfo,
    index: u64,
) -> Result<RawMemoryDescriptor, MemoryError> {
    let offset = index
        .checked_mul(u64::from(boot_info.memory_descriptor_size))
        .ok_or(MemoryError::RangeOverflow)?;
    if offset >= boot_info.memory_map_len {
        return Err(MemoryError::BadMemoryMap);
    }
    let address = boot_info
        .memory_map_ptr
        .checked_add(offset)
        .ok_or(MemoryError::RangeOverflow)?;
    // SAFETY:
    // 1. Invariant: `address` points at the start of a retained UEFI memory
    //    descriptor whose first fields match `RawMemoryDescriptor`.
    // 2. Established by: the loader captured the final UEFI memory map and
    //    `PythBootInfo::validate()` checked pointer, length, and stride.
    // 3. Lifetime: the memory-map buffer is loader-owned and transferred to
    //    PythCore for all early initialization.
    // 4. Pointer ownership: PythCore owns the buffer after handoff and reads it.
    // 5. Alignment: `read_unaligned` is used because UEFI descriptor stride can
    //    include padding beyond the C layout size.
    // 6. Mapped length: `memory_descriptor_size` bytes are available at this
    //    descriptor, and it is at least `size_of::<RawMemoryDescriptor>()`.
    // 7. Concurrency: single-core entry; no writer mutates the retained map.
    // 8. Violation: a bad pointer would fault before the IDT is installed.
    Ok(unsafe { (address as *const RawMemoryDescriptor).read_unaligned() })
}

fn set_allocated(bitmap: &mut [u64; BITMAP_WORDS], page: usize) {
    bitmap[page / 64] |= 1u64 << (page % 64);
}

fn clear_allocated(bitmap: &mut [u64; BITMAP_WORDS], page: usize) {
    bitmap[page / 64] &= !(1u64 << (page % 64));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conventional_pages_start_free_and_required_ranges_are_reserved() {
        let descriptors = [
            RawMemoryDescriptor::for_test(EFI_CONVENTIONAL_MEMORY, 0x100000, 16),
            RawMemoryDescriptor::for_test(EFI_BOOT_SERVICES_DATA, 0x200000, 4),
        ];
        let reserved = [PageRange::new(0x104000, 0x108000).unwrap()];

        let plan = build_plan(&descriptors, &reserved).unwrap();

        assert_eq!(plan.total_pages, 20);
        assert_eq!(plan.free_pages, 12);
        assert_eq!(plan.reserved_pages, 8);
        assert_eq!(plan.owner_at(0x100000), Some(PageOwner::Free));
        assert_eq!(plan.owner_at(0x104000), Some(PageOwner::Reserved));
        assert_eq!(plan.owner_at(0x200000), Some(PageOwner::Reserved));
    }

    #[test]
    fn malformed_descriptor_ranges_are_rejected() {
        let descriptors = [RawMemoryDescriptor::for_test(
            EFI_CONVENTIONAL_MEMORY,
            u64::MAX - 0xFFF,
            2,
        )];

        assert_eq!(
            build_plan(&descriptors, &[]),
            Err(MemoryError::RangeOverflow)
        );
    }
}
