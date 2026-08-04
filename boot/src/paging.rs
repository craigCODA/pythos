//! Loader-owned temporary page tables for PythCore entry.
//!
//! These tables exist only to carry execution from the loader into PythCore.
//! They provide:
//!
//! * a temporary identity map of low physical memory (2 MiB up to 512 GiB) so
//!   the loader keeps executing across the `CR3` switch regardless of where
//!   firmware loaded the loader image, and so PythCore can read `PythBootInfo`,
//!   the retained UEFI memory map, and `INIT.PAK` through physical addresses
//!   wherever firmware allocated them,
//! * the PythCore ELF segments at their linked higher-half virtual addresses
//!   with writable-XOR-executable permissions,
//! * the framebuffer under the planned device region,
//! * a bootstrap kernel stack with an unmapped guard page below it.
//!
//! The first 2 MiB stay unmapped as a null and low-address guard. The
//! temporary identity map is writable and executable because the loader
//! itself executes from it during the transition; PythCore replaces these
//! tables with kernel-owned mappings in a later milestone slice.

use crate::elf::LoadedKernel;
use crate::uefi::{
    self, EFI_ALLOCATE_ANY_PAGES, EFI_LOADER_DATA, EFI_SUCCESS, EfiPhysicalAddress, EfiSystemTable,
};
use core::ptr;
use pythos_shared::boot_protocol::PythFramebufferInfo;

pub(crate) const DEVICE_VIRT_BASE: u64 = 0xFFFF_C000_0000_0000;
const STACK_REGION_VIRT_BASE: u64 = 0xFFFF_E000_0000_0000;

const PAGE_SIZE: u64 = 4096;
const TWO_MIB: u64 = 2 * 1024 * 1024;
const ONE_GIB: u64 = 1024 * 1024 * 1024;
/// Identity-map the low 512 GiB of physical address space. Real firmware may
/// load the loader image and place `AllocatePages` allocations well above 4 GiB
/// on machines with more RAM; the loader must keep executing and PythCore must
/// keep reading those structures across the `CR3` switch. 512 GiB is one PML4
/// entry, covering all plausible consumer/workstation RAM with a single PDPT.
const IDENTITY_MAP_END: u64 = 512 * ONE_GIB;
/// Fallback identity-map ceiling when the CPU lacks 1 GiB pages: the original
/// 4 GiB, mapped with 2 MiB pages (bounded by the table pool).
const IDENTITY_MAP_FALLBACK_END: u64 = 4 * ONE_GIB;
const TABLE_POOL_PAGES: usize = 64;
const STACK_PAGES: usize = 32;
const ENTRY_COUNT: usize = 512;

const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITE: u64 = 1 << 1;
const PTE_HUGE: u64 = 1 << 7;
const PTE_NO_EXECUTE: u64 = 1 << 63;
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

pub(crate) struct BootstrapStack {
    pub(crate) physical_start: u64,
    pub(crate) virt_bottom: u64,
    pub(crate) virt_top: u64,
    pub(crate) page_count: usize,
}

impl BootstrapStack {
    pub(crate) fn allocate(system_table: *mut EfiSystemTable) -> Result<Self, ()> {
        let physical = allocate_zeroed_pages(system_table, STACK_PAGES)?;
        // One unmapped guard page sits below the stack at the region base.
        let virt_bottom = STACK_REGION_VIRT_BASE + PAGE_SIZE;
        let length = (STACK_PAGES as u64) * PAGE_SIZE;
        Ok(Self {
            physical_start: physical,
            virt_bottom,
            virt_top: virt_bottom + length,
            page_count: STACK_PAGES,
        })
    }
}

pub(crate) struct BootPageTables {
    pml4_phys: u64,
    pool_base: u64,
    pool_used: usize,
}

impl BootPageTables {
    pub(crate) fn pml4_phys(&self) -> u64 {
        self.pml4_phys
    }
}

pub(crate) fn build(
    system_table: *mut EfiSystemTable,
    kernel: &LoadedKernel,
    framebuffer: &PythFramebufferInfo,
    stack: &BootstrapStack,
) -> Result<BootPageTables, ()> {
    if !cpu_supports_no_execute() {
        return Err(());
    }

    let pool_base = allocate_zeroed_pages(system_table, TABLE_POOL_PAGES)?;
    let mut tables = BootPageTables {
        pml4_phys: 0,
        pool_base,
        pool_used: 0,
    };
    tables.pml4_phys = tables.take_table_frame()?;

    map_identity(&mut tables)?;
    map_kernel(&mut tables, kernel)?;
    map_framebuffer(&mut tables, framebuffer)?;
    map_stack(&mut tables, stack)?;
    Ok(tables)
}

fn map_identity(tables: &mut BootPageTables) -> Result<(), ()> {
    // Map the low 1 GiB with 2 MiB pages, skipping the first 2 MiB so null and
    // low-address bugs fault. The low 1 GiB always uses 2 MiB pages so the guard
    // hole is preserved even when 1 GiB huge pages cover everything above it.
    let mut physical = TWO_MIB;
    while physical < ONE_GIB {
        tables.map_2m(physical, physical, PTE_WRITE)?;
        physical += TWO_MIB;
    }

    if cpu_supports_gib_pages() {
        // Cover 1 GiB .. 512 GiB with 1 GiB huge pages. This is what lets a
        // machine whose firmware loaded the loader (or PythCore's boot
        // structures) above 4 GiB survive the CR3 switch: a single PDPT of
        // huge entries maps the whole range for the cost of no extra tables.
        while physical < IDENTITY_MAP_END {
            tables.map_1g(physical, physical, PTE_WRITE)?;
            physical += ONE_GIB;
        }
    } else {
        // No 1 GiB pages: fall back to the original 4 GiB ceiling with 2 MiB
        // pages (the table pool cannot afford 512 GiB of 2 MiB entries). Such
        // CPUs are rare; a loader placed above 4 GiB there would still fault.
        while physical < IDENTITY_MAP_FALLBACK_END {
            tables.map_2m(physical, physical, PTE_WRITE)?;
            physical += TWO_MIB;
        }
    }
    Ok(())
}

/// Whether the CPU supports 1 GiB pages (CPUID.80000001h:EDX.PDPE1GB [bit 26]).
fn cpu_supports_gib_pages() -> bool {
    // SAFETY:
    // 1. Invariant: the CPU executes `cpuid` with the requested leaves.
    // 2. Established by: x86-64 long mode guarantees `cpuid` support.
    // 3. Lifetime: valid for these two instructions.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to `cpuid`.
    // 6. Mapped length: not applicable; `cpuid` uses registers only.
    // 7. Concurrency: single-core execution during milestone 1.
    // 8. Violation: none; `cpuid` cannot fault in long mode.
    let max_extended = unsafe { core::arch::x86_64::__cpuid(0x8000_0000) }.eax;
    if max_extended < 0x8000_0001 {
        return false;
    }
    // SAFETY: identical invariants to the `cpuid` call above.
    let features = unsafe { core::arch::x86_64::__cpuid(0x8000_0001) };
    features.edx & (1 << 26) != 0
}

fn map_kernel(tables: &mut BootPageTables, kernel: &LoadedKernel) -> Result<(), ()> {
    for segment in kernel.segments.iter().take(kernel.segment_count) {
        let flags = if segment.permissions.executable {
            0
        } else if segment.permissions.writable {
            PTE_WRITE | PTE_NO_EXECUTE
        } else {
            PTE_NO_EXECUTE
        };
        for page in 0..segment.page_count as u64 {
            let offset = page.checked_mul(PAGE_SIZE).ok_or(())?;
            let virt = segment.virtual_start.checked_add(offset).ok_or(())?;
            let phys = segment.physical_start.checked_add(offset).ok_or(())?;
            tables.map_4k(virt, phys, flags)?;
        }
    }
    Ok(())
}

fn map_framebuffer(
    tables: &mut BootPageTables,
    framebuffer: &PythFramebufferInfo,
) -> Result<(), ()> {
    if framebuffer.physical_base == 0 || framebuffer.byte_length == 0 {
        return Err(());
    }
    let pages = framebuffer
        .byte_length
        .checked_add(PAGE_SIZE - 1)
        .ok_or(())?
        / PAGE_SIZE;
    for page in 0..pages {
        let offset = page.checked_mul(PAGE_SIZE).ok_or(())?;
        let virt = DEVICE_VIRT_BASE.checked_add(offset).ok_or(())?;
        let phys = framebuffer.physical_base.checked_add(offset).ok_or(())?;
        tables.map_4k(virt, phys, PTE_WRITE | PTE_NO_EXECUTE)?;
    }
    Ok(())
}

fn map_stack(tables: &mut BootPageTables, stack: &BootstrapStack) -> Result<(), ()> {
    for page in 0..stack.page_count as u64 {
        let offset = page.checked_mul(PAGE_SIZE).ok_or(())?;
        let virt = stack.virt_bottom.checked_add(offset).ok_or(())?;
        let phys = stack.physical_start.checked_add(offset).ok_or(())?;
        tables.map_4k(virt, phys, PTE_WRITE | PTE_NO_EXECUTE)?;
    }
    Ok(())
}

impl BootPageTables {
    fn take_table_frame(&mut self) -> Result<u64, ()> {
        if self.pool_used >= TABLE_POOL_PAGES {
            return Err(());
        }
        let frame = self
            .pool_base
            .checked_add((self.pool_used as u64) * PAGE_SIZE)
            .ok_or(())?;
        self.pool_used += 1;
        Ok(frame)
    }

    fn map_4k(&mut self, virt: u64, phys: u64, leaf_flags: u64) -> Result<(), ()> {
        if !virt.is_multiple_of(PAGE_SIZE)
            || !phys.is_multiple_of(PAGE_SIZE)
            || phys & !ADDR_MASK != 0
        {
            return Err(());
        }
        let pdpt = self.ensure_table(self.pml4_phys, table_index(virt, 39))?;
        let pd = self.ensure_table(pdpt, table_index(virt, 30))?;
        let pt = self.ensure_table(pd, table_index(virt, 21))?;
        let index = table_index(virt, 12);
        if read_entry(pt, index) & PTE_PRESENT != 0 {
            return Err(());
        }
        write_entry(pt, index, phys | leaf_flags | PTE_PRESENT);
        Ok(())
    }

    fn map_2m(&mut self, virt: u64, phys: u64, leaf_flags: u64) -> Result<(), ()> {
        if !virt.is_multiple_of(TWO_MIB) || !phys.is_multiple_of(TWO_MIB) || phys & !ADDR_MASK != 0
        {
            return Err(());
        }
        let pdpt = self.ensure_table(self.pml4_phys, table_index(virt, 39))?;
        let pd = self.ensure_table(pdpt, table_index(virt, 30))?;
        let index = table_index(virt, 21);
        if read_entry(pd, index) & PTE_PRESENT != 0 {
            return Err(());
        }
        write_entry(pd, index, phys | leaf_flags | PTE_PRESENT | PTE_HUGE);
        Ok(())
    }

    fn map_1g(&mut self, virt: u64, phys: u64, leaf_flags: u64) -> Result<(), ()> {
        if !virt.is_multiple_of(ONE_GIB) || !phys.is_multiple_of(ONE_GIB) || phys & !ADDR_MASK != 0
        {
            return Err(());
        }
        let pdpt = self.ensure_table(self.pml4_phys, table_index(virt, 39))?;
        let index = table_index(virt, 30);
        if read_entry(pdpt, index) & PTE_PRESENT != 0 {
            return Err(());
        }
        write_entry(pdpt, index, phys | leaf_flags | PTE_PRESENT | PTE_HUGE);
        Ok(())
    }

    fn ensure_table(&mut self, table_phys: u64, index: usize) -> Result<u64, ()> {
        let entry = read_entry(table_phys, index);
        if entry & PTE_PRESENT != 0 {
            if entry & PTE_HUGE != 0 {
                return Err(());
            }
            return Ok(entry & ADDR_MASK);
        }
        let frame = self.take_table_frame()?;
        write_entry(table_phys, index, frame | PTE_PRESENT | PTE_WRITE);
        Ok(frame)
    }
}

fn table_index(virt: u64, shift: u32) -> usize {
    ((virt >> shift) & 0x1FF) as usize
}

fn read_entry(table_phys: u64, index: usize) -> u64 {
    debug_assert!(index < ENTRY_COUNT);
    // SAFETY:
    // 1. Invariant: `table_phys` is a 4 KiB page-table frame from the loader's
    //    zeroed pool and `index` is below 512.
    // 2. Established by: `take_table_frame()` only hands out pool frames, and
    //    callers derive `index` from `table_index()` masking to 9 bits.
    // 3. Lifetime: pool frames stay loader-owned until PythCore takes over.
    // 4. Pointer ownership: the loader exclusively owns the pool frames.
    // 5. Alignment: pool frames are 4 KiB aligned; entries are 8-byte aligned.
    // 6. Mapped length: UEFI identity-maps conventional memory, so the frame's
    //    4096 bytes are readable at their physical address.
    // 7. Concurrency: milestone 1 is single-core with no concurrent table use.
    // 8. Violation: reading outside the frame would observe unrelated memory.
    unsafe { (table_phys as *const u64).add(index).read_volatile() }
}

fn write_entry(table_phys: u64, index: usize, value: u64) {
    debug_assert!(index < ENTRY_COUNT);
    // SAFETY:
    // 1. Invariant: `table_phys` is a 4 KiB page-table frame from the loader's
    //    zeroed pool and `index` is below 512.
    // 2. Established by: `take_table_frame()` only hands out pool frames, and
    //    callers derive `index` from `table_index()` masking to 9 bits.
    // 3. Lifetime: pool frames stay loader-owned until PythCore takes over.
    // 4. Pointer ownership: the loader exclusively owns the pool frames.
    // 5. Alignment: pool frames are 4 KiB aligned; entries are 8-byte aligned.
    // 6. Mapped length: UEFI identity-maps conventional memory, so the frame's
    //    4096 bytes are writable at their physical address.
    // 7. Concurrency: milestone 1 is single-core with no concurrent table use.
    // 8. Violation: writing outside the frame would corrupt unrelated memory.
    unsafe { (table_phys as *mut u64).add(index).write_volatile(value) }
}

fn cpu_supports_no_execute() -> bool {
    // SAFETY:
    // 1. Invariant: the CPU executes `cpuid` with the requested leaves.
    // 2. Established by: x86-64 long mode guarantees `cpuid` support.
    // 3. Lifetime: valid for these two instructions.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to `cpuid`.
    // 6. Mapped length: not applicable; `cpuid` uses registers only.
    // 7. Concurrency: single-core execution during milestone 1.
    // 8. Violation: none; `cpuid` cannot fault in long mode.
    let max_extended = unsafe { core::arch::x86_64::__cpuid(0x8000_0000) }.eax;
    if max_extended < 0x8000_0001 {
        return false;
    }
    // SAFETY: identical invariants to the `cpuid` call above.
    let features = unsafe { core::arch::x86_64::__cpuid(0x8000_0001) };
    features.edx & (1 << 20) != 0
}

fn allocate_zeroed_pages(system_table: *mut EfiSystemTable, pages: usize) -> Result<u64, ()> {
    let boot_services = uefi::boot_services(system_table).map_err(|_| ())?;
    let mut physical: EfiPhysicalAddress = 0;
    // SAFETY:
    // 1. Invariant: `boot_services` points to the active UEFI boot services table.
    // 2. Established by: `uefi::boot_services()` from the firmware system table.
    // 3. Lifetime: allocated pages stay loader-owned through the PythCore handoff.
    // 4. Pointer ownership: firmware transfers page ownership to the loader.
    // 5. Alignment: `AllocatePages` returns 4 KiB-aligned physical pages.
    // 6. Mapped length: `pages * PAGE_SIZE` bytes are allocated.
    // 7. Concurrency: no concurrent page allocation users in this loader slice.
    // 8. Violation: an invalid table could call through a bad function pointer.
    let status = unsafe {
        ((*boot_services).allocate_pages)(
            EFI_ALLOCATE_ANY_PAGES,
            EFI_LOADER_DATA,
            pages,
            &mut physical,
        )
    };
    if status != EFI_SUCCESS || physical == 0 || !physical.is_multiple_of(PAGE_SIZE) {
        return Err(());
    }
    let total_len = pages.checked_mul(PAGE_SIZE as usize).ok_or(())?;
    // SAFETY:
    // 1. Invariant: `physical` is the base of `pages` freshly allocated pages.
    // 2. Established by: successful `AllocatePages` immediately above.
    // 3. Lifetime: pages remain valid through the loader and PythCore handoff.
    // 4. Pointer ownership: the loader owns the destination pages.
    // 5. Alignment: `physical` is 4 KiB aligned; byte writes need 1-byte alignment.
    // 6. Mapped length: exactly `total_len` bytes were allocated.
    // 7. Concurrency: no concurrent access to the fresh allocation.
    // 8. Violation: a wrong length would corrupt adjacent memory.
    unsafe {
        ptr::write_bytes(physical as *mut u8, 0, total_len);
    }
    Ok(physical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_stack_keeps_guard_and_has_phase_2_headroom() {
        assert_eq!(STACK_PAGES, 32);
        let virt_bottom = STACK_REGION_VIRT_BASE + PAGE_SIZE;
        let virt_top = virt_bottom + STACK_PAGES as u64 * PAGE_SIZE;

        assert_eq!(virt_bottom - PAGE_SIZE, STACK_REGION_VIRT_BASE);
        assert_eq!(virt_top - virt_bottom, 128 * 1024);
    }
}
