//! PythCore-owned kernel page tables.
// Some items (the shell's bootstrap-mapped user address-space builder) are
// only used by the normal-boot persistent-launch path (a `verify`-excluded
// set of modules), so they are legitimately unused under `--features verify`.
#![cfg_attr(feature = "verify", allow(dead_code))]

use crate::architecture::x86_64::exceptions;
use crate::memory::physical::{MemoryError, PAGE_SIZE, PhysicalMemory};
use crate::{user_elf, user_mode, user_stacks};
use core::arch::asm;
use core::arch::global_asm;
use core::mem;
use pythos_shared::boot_protocol::{PYTH_BOOT_MAGIC, PythBootInfo};

const TWO_MIB: u64 = 2 * 1024 * 1024;
const OLD_IDENTITY_PROBE: u64 = 64 * 1024 * 1024;
const ENTRY_COUNT: usize = 512;
const MAX_TABLE_FRAMES: usize = 128;
const MAX_USER_ELF_FRAMES: usize = 32;

const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITE: u64 = 1 << 1;
const PTE_USER: u64 = 1 << 2;
const PTE_HUGE: u64 = 1 << 7;
/// PCD: disables caching for this page. Device MMIO registers must never be
/// cached: an AHCI/NVMe-class controller's registers reflect live hardware
/// state, and a cached read/write can silently observe or leave behind a
/// stale value instead of the real one.
const PTE_CACHE_DISABLE: u64 = 1 << 4;
const PTE_NO_EXECUTE: u64 = 1 << 63;
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
const OFFSET_4K_MASK: u64 = 0xFFF;
const OFFSET_2M_MASK: u64 = TWO_MIB - 1;

unsafe extern "C" {
    static __pythcore_rodata_start: u8;
    static __pythcore_rodata_end: u8;
    static __pythcore_text_start: u8;
    static __pythcore_text_end: u8;
    static __pythcore_data_start: u8;
    static __pythcore_data_end: u8;
    static __pythcore_syscall_stack_guard_low_start: u8;
    static __pythcore_syscall_stack_start: u8;
    static __pythcore_syscall_stack_guard_high_start: u8;
    static __pythcore_syscall_stack_guard_high_end: u8;
    fn old_identity_probe_fault(address: u64) -> u64;
}

global_asm!(
    r#"
    .global old_identity_probe_fault
    old_identity_probe_fault:
        push rbx
        mov rbx, rdi
        lea rsi, [rip + .Lold_identity_probe_recovery]
        call expect_page_fault_abi
        mov al, byte ptr [rbx]
        pop rbx
        xor eax, eax
        ret
    .Lold_identity_probe_recovery:
        pop rbx
        mov eax, 1
        ret
    "#
);

#[unsafe(no_mangle)]
pub extern "C" fn expect_page_fault_abi(address: u64, recovery_rip: u64) {
    exceptions::expect_page_fault(address, recovery_rip);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmError {
    Memory(MemoryError),
    RangeOverflow,
    UnmappedSource,
    DuplicateMapping,
    LowGuardMapping,
    OutOfTableFrames,
    BadActiveCr3,
    BadBootInfo,
    NotIsolated,
    UserAccessViolation,
    UserStackGuardViolation,
    ActiveAddressSpace,
    UserElfTooLarge,
}

impl From<MemoryError> for VmError {
    fn from(error: MemoryError) -> Self {
        Self::Memory(error)
    }
}

pub fn prove_old_identity_map_removed() -> Result<(), VmError> {
    if translate_active(OLD_IDENTITY_PROBE).is_ok() {
        return Err(VmError::LowGuardMapping);
    }

    // SAFETY:
    // 1. Invariant: `OLD_IDENTITY_PROBE` is intentionally unmapped after the
    //    PythCore-owned `CR3` switch. The assembly probe arms the exception
    //    handler with an internal recovery RIP, performs one byte read, and
    //    returns 1 only if the exact expected page fault recovers there.
    // 2. Established by: `translate_active(OLD_IDENTITY_PROBE)` returned an
    //    unmapped result immediately before this call.
    // 3. Lifetime: the probe runs synchronously and clears the expected-fault
    //    metadata in the exception handler before returning.
    // 4. Pointer ownership: no ownership is claimed over the old identity
    //    address; it must fault.
    // 5. Alignment: `OLD_IDENTITY_PROBE` is page-aligned.
    // 6. Mapped length: expected to be zero; a successful read returns 0.
    // 7. Concurrency: single-core execution with interrupts disabled.
    // 8. Violation: an unexpected fault is reported by the diagnostic handler
    //    and enters the panic loop.
    if unsafe { old_identity_probe_fault(OLD_IDENTITY_PROBE) } != 1 {
        return Err(VmError::LowGuardMapping);
    }
    Ok(())
}

/// Task 11 regression coverage: prove the two 4 KiB pages `map_kernel_segments`
/// deliberately leaves out of the active mapping (immediately below and above
/// `syscall_kernel_stack`, see `core/linker.ld`) are genuinely unmapped, not
/// just absent from the source symbols by coincidence. Reuses the same
/// expected-page-fault probe `prove_old_identity_map_removed` uses.
pub fn prove_syscall_stack_guard_pages_unmapped() -> Result<(), VmError> {
    let guard_low = symbol_addr(&raw const __pythcore_syscall_stack_guard_low_start);
    let guard_high = symbol_addr(&raw const __pythcore_syscall_stack_guard_high_start);

    if translate_active(guard_low).is_ok() || translate_active(guard_high).is_ok() {
        return Err(VmError::LowGuardMapping);
    }

    // SAFETY: matches `prove_old_identity_map_removed` above - `guard_low`
    // and `guard_high` were just confirmed unmapped by `translate_active`,
    // and the probe's internal recovery RIP means an unexpected fault
    // elsewhere is reported by the diagnostic handler rather than silently
    // continuing.
    if unsafe { old_identity_probe_fault(guard_low) } != 1 {
        return Err(VmError::LowGuardMapping);
    }
    // SAFETY: matches the guard_low probe immediately above.
    if unsafe { old_identity_probe_fault(guard_high) } != 1 {
        return Err(VmError::LowGuardMapping);
    }
    Ok(())
}

pub fn active_mapping_present(virt: u64) -> bool {
    translate_active(virt).is_ok()
}

pub fn translate_active_address(virt: u64) -> Result<u64, VmError> {
    translate_active(virt)
}

pub struct KernelAddressSpace {
    root_table_phys: u64,
}

impl KernelAddressSpace {
    pub fn build(
        allocator: &mut PhysicalMemory,
        boot_info: &PythBootInfo,
        hda_mmio: Option<(u64, u64, u64)>,
        ahci_mmio: Option<(u64, u64, u64)>,
        sdhci_emmc_mmio: Option<(u64, u64, u64)>,
        shell_bootstrap_frame: Option<u64>,
    ) -> Result<Self, VmError> {
        let mut tables = PageTableBuilder::new(allocator)?;
        map_kernel_segments(&mut tables)?;
        map_user_mode_proof_pages(&mut tables)?;
        tables.map_translated_range(
            align_down(boot_info as *const PythBootInfo as u64),
            mem::size_of::<PythBootInfo>() as u64,
            PTE_NO_EXECUTE,
        )?;
        tables.map_translated_range(
            align_down(boot_info.memory_map_ptr),
            boot_info.memory_map_len,
            PTE_NO_EXECUTE,
        )?;
        tables.map_translated_range(
            align_down(boot_info.init_bundle_phys),
            boot_info.init_bundle_len,
            PTE_NO_EXECUTE,
        )?;
        tables.map_translated_range(
            align_down(boot_info.font_phys),
            boot_info.font_len,
            PTE_NO_EXECUTE,
        )?;
        map_firmware_tables(&mut tables, boot_info)?;
        map_bootstrap_stack(&mut tables, boot_info)?;
        tables.map_physical_range(
            boot_info.framebuffer.mapped_virtual_base,
            boot_info.framebuffer.physical_base,
            boot_info.framebuffer.byte_length,
            PTE_WRITE | PTE_NO_EXECUTE,
        )?;
        // ADR 0048: map the runtime-discovered HDA controller MMIO into the
        // kernel device window so the audio driver can reach its registers.
        // Note: this HDA mapping predates PTE_CACHE_DISABLE and has the same
        // latent cacheable-MMIO gap the AHCI mapping below fixes; left as-is,
        // out of scope for ADR 0054 (see that ADR's follow-up risk note).
        if let Some((phys, virt, len)) = hda_mmio {
            tables.map_physical_range(virt, phys, len, PTE_WRITE | PTE_NO_EXECUTE)?;
        }
        // ADR 0054: map the runtime-discovered AHCI controller's ABAR into the
        // kernel device window, uncacheable: device registers reflect live
        // hardware state and must never be served from a stale cache line.
        if let Some((phys, virt, len)) = ahci_mmio {
            tables.map_physical_range(
                virt,
                phys,
                len,
                PTE_WRITE | PTE_NO_EXECUTE | PTE_CACHE_DISABLE,
            )?;
        }
        // ADR 0062: map the selected SDHCI/eMMC BAR0 window uncacheable for
        // synchronous polling PIO. The backend does not enable DMA/ADMA.
        if let Some((phys, virt, len)) = sdhci_emmc_mmio {
            tables.map_physical_range(
                virt,
                phys,
                len,
                PTE_WRITE | PTE_NO_EXECUTE | PTE_CACHE_DISABLE,
            )?;
        }
        if let Some(frame) = shell_bootstrap_frame {
            tables.map_physical_range(frame, frame, PAGE_SIZE, PTE_WRITE | PTE_NO_EXECUTE)?;
        }
        tables.map_allocated_table_frames()?;

        Ok(Self {
            root_table_phys: tables.root_table_phys,
        })
    }

    pub unsafe fn activate(&self) {
        // SAFETY:
        // 1. Invariant: `root_table_phys` is a 4 KiB-aligned PML4 physical
        //    address whose mappings include the currently executing code,
        //    active stack, static descriptor tables, COM1 code path, boot
        //    metadata, framebuffer, and the page-table frames needed for
        //    validation.
        // 2. Established by: `KernelAddressSpace::build`.
        // 3. Lifetime: the page tables are PythCore-owned and intentionally
        //    not reclaimed in this slice.
        // 4. Pointer ownership: the CPU borrows the page-table hierarchy.
        // 5. Alignment: `build` obtains page frames from the physical allocator.
        // 6. Mapped length: one full page-table hierarchy rooted at CR3.
        // 7. Concurrency: single-core execution with interrupts disabled.
        // 8. Violation: an incomplete mapping faults immediately after `mov cr3`.
        unsafe {
            asm!(
                "cli",
                "mov cr3, {root}",
                root = in(reg) self.root_table_phys,
                options(nostack, preserves_flags)
            );
        }
    }

    pub fn validate_active(&self, boot_info: &PythBootInfo) -> Result<(), VmError> {
        if read_cr3() != self.root_table_phys {
            return Err(VmError::BadActiveCr3);
        }
        if boot_info.magic != PYTH_BOOT_MAGIC {
            return Err(VmError::BadBootInfo);
        }

        translate_active(symbol_addr(&raw const __pythcore_text_start))?;
        translate_active(boot_info as *const PythBootInfo as u64)?;
        translate_active(boot_info.memory_map_ptr)?;
        translate_active(boot_info.font_phys)?;
        translate_active(boot_info.bootstrap_stack_top - 8)?;
        translate_active(boot_info.framebuffer.mapped_virtual_base)?;

        let mut stack_probe = 0u64;
        stack_probe = stack_probe.wrapping_add(1);
        if stack_probe != 1 {
            return Err(VmError::UnmappedSource);
        }
        Ok(())
    }

    pub const fn root_table_phys(&self) -> u64 {
        self.root_table_phys
    }
}

pub struct UserAddressSpace {
    root_table_phys: u64,
    table_frames: [u64; MAX_TABLE_FRAMES],
    table_frame_count: usize,
    user_elf_frames: [u64; MAX_USER_ELF_FRAMES],
    user_elf_frame_count: usize,
}

pub struct RetainedUserAddressSpace {
    root_table_phys: u64,
}

impl RetainedUserAddressSpace {
    pub const fn root_table_phys(&self) -> u64 {
        self.root_table_phys
    }

    pub unsafe fn activate(&self) {
        // SAFETY:
        // 1. Invariant: `root_table_phys` is a 4 KiB-aligned PML4 physical
        //    address whose mappings were validated before the larger
        //    `UserAddressSpace` was intentionally retained for boot.
        // 2. Established by: `UserAddressSpace::retain_for_boot` is called
        //    only after `validate_user_elf_entry` or equivalent validation.
        // 3. Lifetime: the page tables are PythCore-owned and intentionally
        //    not reclaimed in this boot path.
        // 4. Pointer ownership: the CPU borrows the page-table hierarchy.
        // 5. Alignment: the root came from the physical page allocator.
        // 6. Mapped length: one complete page-table hierarchy rooted at CR3.
        // 7. Concurrency: single-core boot proof with one active user root.
        // 8. Violation: an incomplete mapping faults during or after `mov cr3`.
        unsafe {
            asm!(
                "cli",
                "mov cr3, {root}",
                root = in(reg) self.root_table_phys,
                options(nostack, preserves_flags)
            );
        }
    }
}

impl UserAddressSpace {
    pub fn build(
        allocator: &mut PhysicalMemory,
        boot_info: &PythBootInfo,
    ) -> Result<Self, VmError> {
        let mut tables = PageTableBuilder::new(allocator)?;
        map_kernel_segments(&mut tables)?;
        map_user_mode_proof_pages(&mut tables)?;
        map_bootstrap_stack(&mut tables, boot_info)?;
        tables.map_allocated_table_frames()?;
        let table_frames = tables.allocated_frames;
        let table_frame_count = tables.allocated_count;

        Ok(Self {
            root_table_phys: tables.root_table_phys,
            table_frames,
            table_frame_count,
            user_elf_frames: [0; MAX_USER_ELF_FRAMES],
            user_elf_frame_count: 0,
        })
    }

    pub fn build_with_user_elf(
        allocator: &mut PhysicalMemory,
        boot_info: &PythBootInfo,
        image: &user_elf::UserElfImage,
        elf_bytes: &[u8],
    ) -> Result<(Self, user_elf::LoadedUserElf), VmError> {
        Self::build_with_user_elf_inner(allocator, boot_info, image, elf_bytes, None)
    }

    pub fn build_with_user_elf_and_bootstrap(
        allocator: &mut PhysicalMemory,
        boot_info: &PythBootInfo,
        image: &user_elf::UserElfImage,
        elf_bytes: &[u8],
        bootstrap_user_ptr: u64,
        bootstrap_physical: u64,
    ) -> Result<(Self, user_elf::LoadedUserElf), VmError> {
        Self::build_with_user_elf_inner(
            allocator,
            boot_info,
            image,
            elf_bytes,
            Some((bootstrap_user_ptr, bootstrap_physical)),
        )
    }

    fn build_with_user_elf_inner(
        allocator: &mut PhysicalMemory,
        boot_info: &PythBootInfo,
        image: &user_elf::UserElfImage,
        elf_bytes: &[u8],
        bootstrap_mapping: Option<(u64, u64)>,
    ) -> Result<(Self, user_elf::LoadedUserElf), VmError> {
        let mut tables = PageTableBuilder::new(allocator)?;
        let mut user_elf_frames = [0u64; MAX_USER_ELF_FRAMES];
        let mut user_elf_frame_count = 0usize;
        map_kernel_segments(&mut tables)?;
        map_user_stack_pages(&mut tables)?;
        map_bootstrap_stack(&mut tables, boot_info)?;

        let mut segment_index = 0;
        while segment_index < image.segment_count() {
            let segment = image.segment(segment_index).ok_or(VmError::RangeOverflow)?;
            map_user_elf_segment(
                &mut tables,
                elf_bytes,
                segment,
                &mut user_elf_frames,
                &mut user_elf_frame_count,
            )?;
            segment_index += 1;
        }
        if let Some((bootstrap_user_ptr, bootstrap_physical)) = bootstrap_mapping {
            remember_user_payload_frame(
                bootstrap_physical,
                &mut user_elf_frames,
                &mut user_elf_frame_count,
            )?;
            tables.map_user_physical_range(
                bootstrap_user_ptr,
                bootstrap_physical,
                PAGE_SIZE,
                PTE_NO_EXECUTE,
            )?;
        }

        tables.map_allocated_table_frames()?;
        let table_frames = tables.allocated_frames;
        let table_frame_count = tables.allocated_count;
        Ok((
            Self {
                root_table_phys: tables.root_table_phys,
                table_frames,
                table_frame_count,
                user_elf_frames,
                user_elf_frame_count,
            },
            user_elf::LoadedUserElf::new(image.entry(), image.segment_count(), true),
        ))
    }

    pub fn validate_isolated_from(&self, kernel: &KernelAddressSpace) -> Result<(), VmError> {
        if self.root_table_phys == kernel.root_table_phys {
            return Err(VmError::NotIsolated);
        }
        let (user_code, _) = user_mode::user_code_region();
        let (user_stack, _) = user_mode::user_stack_region();
        if !user_can_access_from_root(self.root_table_phys, user_code)? {
            return Err(VmError::UserAccessViolation);
        }
        if !user_can_access_from_root(self.root_table_phys, user_stack)? {
            return Err(VmError::UserAccessViolation);
        }
        if user_can_access_from_root(
            self.root_table_phys,
            symbol_addr(&raw const __pythcore_text_start),
        )? {
            return Err(VmError::UserAccessViolation);
        }
        if user_can_access_from_root(
            self.root_table_phys,
            symbol_addr(&raw const __pythcore_data_start),
        )? {
            return Err(VmError::UserAccessViolation);
        }
        Ok(())
    }

    pub fn validate_user_stack_protections(&self) -> Result<(), VmError> {
        for region in user_stacks::regions() {
            if !user_can_access_from_root(self.root_table_phys, region.stack_start)? {
                return Err(VmError::UserAccessViolation);
            }
            if user_can_access_from_root(self.root_table_phys, region.guard_start)? {
                return Err(VmError::UserStackGuardViolation);
            }
        }
        Ok(())
    }

    pub fn validate_user_elf_entry(&self, entry: u64) -> Result<(), VmError> {
        if !user_can_access_from_root(self.root_table_phys, entry)? {
            return Err(VmError::UserAccessViolation);
        }
        self.validate_user_stack_protections()?;
        if user_can_access_from_root(
            self.root_table_phys,
            symbol_addr(&raw const __pythcore_text_start),
        )? {
            return Err(VmError::UserAccessViolation);
        }
        if user_can_access_from_root(
            self.root_table_phys,
            symbol_addr(&raw const __pythcore_data_start),
        )? {
            return Err(VmError::UserAccessViolation);
        }
        Ok(())
    }

    pub fn validate_user_bootstrap_mapping(&self, bootstrap_user_ptr: u64) -> Result<(), VmError> {
        let entry = user_leaf_entry_from_root(self.root_table_phys, bootstrap_user_ptr)?;
        if entry & PTE_WRITE != 0 || entry & PTE_NO_EXECUTE == 0 {
            return Err(VmError::UserAccessViolation);
        }
        Ok(())
    }

    pub const fn root_table_phys(&self) -> u64 {
        self.root_table_phys
    }

    pub fn retain_for_boot(self) -> RetainedUserAddressSpace {
        RetainedUserAddressSpace {
            root_table_phys: self.root_table_phys,
        }
    }

    pub const fn table_frame_count(&self) -> usize {
        self.table_frame_count
    }

    pub fn reclaim(self, allocator: &mut PhysicalMemory) -> Result<usize, VmError> {
        if read_cr3() == self.root_table_phys {
            return Err(VmError::ActiveAddressSpace);
        }
        let mut reclaimed = 0;
        while reclaimed < self.table_frame_count {
            allocator.release_allocated_page(self.table_frames[reclaimed])?;
            reclaimed += 1;
        }
        let mut user_frame = 0;
        while user_frame < self.user_elf_frame_count {
            allocator.release_allocated_page(self.user_elf_frames[user_frame])?;
            reclaimed += 1;
            user_frame += 1;
        }
        Ok(reclaimed)
    }

    pub unsafe fn activate(&self) {
        // SAFETY:
        // 1. Invariant: `root_table_phys` is a 4 KiB-aligned PML4 physical
        //    address whose mappings include supervisor kernel code/data,
        //    the active kernel stack, descriptor-table storage, COM1 code
        //    paths, and either user-accessible proof code/stack pages or a
        //    validated dynamic user ELF plus stack pages.
        // 2. Established by: `UserAddressSpace::build` with
        //    `validate_isolated_from` or `build_with_user_elf` with
        //    `validate_user_elf_entry`.
        // 3. Lifetime: the page tables are PythCore-owned and not reclaimed.
        // 4. Pointer ownership: the CPU borrows the page-table hierarchy.
        // 5. Alignment: the root came from the physical page allocator.
        // 6. Mapped length: one complete page-table hierarchy rooted at CR3.
        // 7. Concurrency: single-core Phase 8 proof with interrupts disabled.
        // 8. Violation: an incomplete mapping faults during or after `mov cr3`.
        unsafe {
            asm!(
                "cli",
                "mov cr3, {root}",
                root = in(reg) self.root_table_phys,
                options(nostack, preserves_flags)
            );
        }
    }
}

struct PageTableBuilder<'a> {
    allocator: &'a mut PhysicalMemory,
    root_table_phys: u64,
    allocated_frames: [u64; MAX_TABLE_FRAMES],
    allocated_count: usize,
}

impl<'a> PageTableBuilder<'a> {
    fn new(allocator: &'a mut PhysicalMemory) -> Result<Self, VmError> {
        let root = allocator.allocate_zeroed_page()?;
        let mut builder = Self {
            allocator,
            root_table_phys: root,
            allocated_frames: [0; MAX_TABLE_FRAMES],
            allocated_count: 0,
        };
        builder.remember_frame(root)?;
        Ok(builder)
    }

    fn map_translated_range(
        &mut self,
        virt_start: u64,
        byte_len: u64,
        flags: u64,
    ) -> Result<(), VmError> {
        let start = align_down(virt_start);
        let end = align_up(
            virt_start
                .checked_add(byte_len)
                .ok_or(VmError::RangeOverflow)?,
        )?;
        let mut virt = start;
        while virt < end {
            let phys = translate_active(virt)?;
            self.map_4k(virt, phys, flags)?;
            virt = virt.checked_add(PAGE_SIZE).ok_or(VmError::RangeOverflow)?;
        }
        Ok(())
    }

    fn map_user_translated_range(
        &mut self,
        virt_start: u64,
        byte_len: u64,
        flags: u64,
    ) -> Result<(), VmError> {
        let start = align_down(virt_start);
        let end = align_up(
            virt_start
                .checked_add(byte_len)
                .ok_or(VmError::RangeOverflow)?,
        )?;
        let mut virt = start;
        while virt < end {
            let phys = translate_active(virt)?;
            self.map_4k_for_user(virt, phys, flags | PTE_USER)?;
            virt = virt.checked_add(PAGE_SIZE).ok_or(VmError::RangeOverflow)?;
        }
        Ok(())
    }

    fn map_physical_range(
        &mut self,
        virt_start: u64,
        phys_start: u64,
        byte_len: u64,
        flags: u64,
    ) -> Result<(), VmError> {
        let virt = align_down(virt_start);
        let phys = align_down(phys_start);
        let prefix = virt_start.checked_sub(virt).ok_or(VmError::RangeOverflow)?;
        let len = byte_len.checked_add(prefix).ok_or(VmError::RangeOverflow)?;
        let end = align_up(virt.checked_add(len).ok_or(VmError::RangeOverflow)?)?;
        let mut offset = 0u64;
        while virt.checked_add(offset).ok_or(VmError::RangeOverflow)? < end {
            self.map_4k(
                virt.checked_add(offset).ok_or(VmError::RangeOverflow)?,
                phys.checked_add(offset).ok_or(VmError::RangeOverflow)?,
                flags,
            )?;
            offset = offset
                .checked_add(PAGE_SIZE)
                .ok_or(VmError::RangeOverflow)?;
        }
        Ok(())
    }

    fn map_user_physical_range(
        &mut self,
        virt_start: u64,
        phys_start: u64,
        byte_len: u64,
        flags: u64,
    ) -> Result<(), VmError> {
        let virt = align_down(virt_start);
        let phys = align_down(phys_start);
        let prefix = virt_start.checked_sub(virt).ok_or(VmError::RangeOverflow)?;
        let len = byte_len.checked_add(prefix).ok_or(VmError::RangeOverflow)?;
        let end = align_up(virt.checked_add(len).ok_or(VmError::RangeOverflow)?)?;
        let mut offset = 0u64;
        while virt.checked_add(offset).ok_or(VmError::RangeOverflow)? < end {
            self.map_4k_for_user(
                virt.checked_add(offset).ok_or(VmError::RangeOverflow)?,
                phys.checked_add(offset).ok_or(VmError::RangeOverflow)?,
                flags | PTE_USER,
            )?;
            offset = offset
                .checked_add(PAGE_SIZE)
                .ok_or(VmError::RangeOverflow)?;
        }
        Ok(())
    }

    fn map_allocated_table_frames(&mut self) -> Result<(), VmError> {
        let mut index = 0;
        while index < self.allocated_count {
            let frame = self.allocated_frames[index];
            self.map_4k(frame, frame, PTE_WRITE | PTE_NO_EXECUTE)?;
            index += 1;
        }
        Ok(())
    }

    fn map_4k(&mut self, virt: u64, phys: u64, leaf_flags: u64) -> Result<(), VmError> {
        self.map_4k_with_table_flags(virt, phys, leaf_flags, 0, false)
    }

    fn map_4k_for_user(&mut self, virt: u64, phys: u64, leaf_flags: u64) -> Result<(), VmError> {
        self.map_4k_with_table_flags(virt, phys, leaf_flags, PTE_USER, true)
    }

    fn map_4k_with_table_flags(
        &mut self,
        virt: u64,
        phys: u64,
        leaf_flags: u64,
        table_flags: u64,
        update_existing_leaf: bool,
    ) -> Result<(), VmError> {
        if virt < TWO_MIB {
            return Err(VmError::LowGuardMapping);
        }
        if !virt.is_multiple_of(PAGE_SIZE)
            || !phys.is_multiple_of(PAGE_SIZE)
            || phys & !ADDR_MASK != 0
        {
            return Err(VmError::RangeOverflow);
        }
        let pdpt = self.ensure_table(self.root_table_phys, table_index(virt, 39), table_flags)?;
        let pd = self.ensure_table(pdpt, table_index(virt, 30), table_flags)?;
        let pt = self.ensure_table(pd, table_index(virt, 21), table_flags)?;
        let index = table_index(virt, 12);
        if read_entry(pt, index) & PTE_PRESENT != 0 {
            let existing = read_entry(pt, index);
            if existing & ADDR_MASK == phys {
                if update_existing_leaf {
                    write_entry(pt, index, phys | leaf_flags | PTE_PRESENT);
                }
                return Ok(());
            }
            return Err(VmError::DuplicateMapping);
        }
        write_entry(pt, index, phys | leaf_flags | PTE_PRESENT);
        Ok(())
    }

    fn ensure_table(&mut self, table_phys: u64, index: usize, flags: u64) -> Result<u64, VmError> {
        let entry = read_entry(table_phys, index);
        if entry & PTE_PRESENT != 0 {
            if entry & PTE_HUGE != 0 {
                return Err(VmError::DuplicateMapping);
            }
            if flags != 0 && entry & flags != flags {
                write_entry(table_phys, index, entry | flags);
            }
            return Ok(entry & ADDR_MASK);
        }
        let frame = self.allocator.allocate_zeroed_page()?;
        self.remember_frame(frame)?;
        write_entry(table_phys, index, frame | PTE_PRESENT | PTE_WRITE | flags);
        Ok(frame)
    }

    fn remember_frame(&mut self, frame: u64) -> Result<(), VmError> {
        if self.allocated_count >= MAX_TABLE_FRAMES {
            return Err(VmError::OutOfTableFrames);
        }
        self.allocated_frames[self.allocated_count] = frame;
        self.allocated_count += 1;
        Ok(())
    }
}

fn map_kernel_segments(tables: &mut PageTableBuilder<'_>) -> Result<(), VmError> {
    tables.map_translated_range(
        symbol_addr(&raw const __pythcore_rodata_start),
        symbol_range_len(
            &raw const __pythcore_rodata_start,
            &raw const __pythcore_rodata_end,
        )?,
        PTE_NO_EXECUTE,
    )?;
    tables.map_translated_range(
        symbol_addr(&raw const __pythcore_text_start),
        symbol_range_len(
            &raw const __pythcore_text_start,
            &raw const __pythcore_text_end,
        )?,
        0,
    )?;
    // Task 11: map .data/.bss in three pieces, deliberately skipping the two
    // 4 KiB guard pages the linker script placed around `syscall_stack`
    // (core/linker.ld), so an overflow of that stack takes a #PF instead of
    // silently corrupting whatever static data used to sit adjacent to it.
    let data_start = symbol_addr(&raw const __pythcore_data_start);
    let data_end = symbol_addr(&raw const __pythcore_data_end);
    let guard_low_start = symbol_addr(&raw const __pythcore_syscall_stack_guard_low_start);
    let stack_start = symbol_addr(&raw const __pythcore_syscall_stack_start);
    let guard_high_start = symbol_addr(&raw const __pythcore_syscall_stack_guard_high_start);
    let guard_high_end = symbol_addr(&raw const __pythcore_syscall_stack_guard_high_end);

    tables.map_translated_range(
        data_start,
        guard_low_start
            .checked_sub(data_start)
            .ok_or(VmError::RangeOverflow)?,
        PTE_WRITE | PTE_NO_EXECUTE,
    )?;
    tables.map_translated_range(
        stack_start,
        guard_high_start
            .checked_sub(stack_start)
            .ok_or(VmError::RangeOverflow)?,
        PTE_WRITE | PTE_NO_EXECUTE,
    )?;
    tables.map_translated_range(
        guard_high_end,
        data_end
            .checked_sub(guard_high_end)
            .ok_or(VmError::RangeOverflow)?,
        PTE_WRITE | PTE_NO_EXECUTE,
    )?;
    Ok(())
}

fn map_user_mode_proof_pages(tables: &mut PageTableBuilder<'_>) -> Result<(), VmError> {
    let (code_start, code_len) = user_mode::user_code_region();
    tables.map_user_translated_range(code_start, code_len, 0)?;
    map_user_stack_pages(tables)
}

fn map_user_stack_pages(tables: &mut PageTableBuilder<'_>) -> Result<(), VmError> {
    for region in user_stacks::regions() {
        tables.map_user_translated_range(
            region.stack_start,
            region.stack_len,
            PTE_WRITE | PTE_NO_EXECUTE,
        )?;
    }
    Ok(())
}

fn map_bootstrap_stack(
    tables: &mut PageTableBuilder<'_>,
    boot_info: &PythBootInfo,
) -> Result<(), VmError> {
    tables.map_translated_range(
        boot_info.bootstrap_stack_bottom,
        boot_info
            .bootstrap_stack_top
            .checked_sub(boot_info.bootstrap_stack_bottom)
            .ok_or(VmError::RangeOverflow)?,
        PTE_WRITE | PTE_NO_EXECUTE,
    )
}

fn map_user_elf_segment(
    tables: &mut PageTableBuilder<'_>,
    elf_bytes: &[u8],
    segment: &user_elf::LoadSegment,
    user_elf_frames: &mut [u64; MAX_USER_ELF_FRAMES],
    user_elf_frame_count: &mut usize,
) -> Result<(), VmError> {
    let page_len = segment.page_len();
    let mut page_offset = 0u64;
    while page_offset < page_len {
        let physical = tables.allocator.allocate_zeroed_page()?;
        copy_user_elf_page(elf_bytes, segment, page_offset, physical)?;
        remember_user_payload_frame(physical, user_elf_frames, user_elf_frame_count)?;

        let flags = user_elf_page_flags(segment);
        tables.map_user_physical_range(
            segment
                .page_start()
                .checked_add(page_offset)
                .ok_or(VmError::RangeOverflow)?,
            physical,
            PAGE_SIZE,
            flags,
        )?;
        page_offset = page_offset
            .checked_add(PAGE_SIZE)
            .ok_or(VmError::RangeOverflow)?;
    }
    Ok(())
}

fn remember_user_payload_frame(
    physical: u64,
    user_elf_frames: &mut [u64; MAX_USER_ELF_FRAMES],
    user_elf_frame_count: &mut usize,
) -> Result<(), VmError> {
    if *user_elf_frame_count >= MAX_USER_ELF_FRAMES {
        return Err(VmError::UserElfTooLarge);
    }
    user_elf_frames[*user_elf_frame_count] = physical;
    *user_elf_frame_count += 1;
    Ok(())
}

fn copy_user_elf_page(
    elf_bytes: &[u8],
    segment: &user_elf::LoadSegment,
    page_offset: u64,
    physical: u64,
) -> Result<(), VmError> {
    let page_virtual_start = segment
        .page_start()
        .checked_add(page_offset)
        .ok_or(VmError::RangeOverflow)?;
    let page_virtual_end = page_virtual_start
        .checked_add(PAGE_SIZE)
        .ok_or(VmError::RangeOverflow)?;
    let file_virtual_start = segment.virtual_start();
    let file_virtual_end = file_virtual_start
        .checked_add(segment.file_size())
        .ok_or(VmError::RangeOverflow)?;
    let copy_start = max_u64(page_virtual_start, file_virtual_start);
    let copy_end = min_u64(page_virtual_end, file_virtual_end);

    // SAFETY:
    // 1. Invariant: `physical` is a freshly allocated, page-aligned frame
    //    reachable through the loader identity map while user ELF address
    //    spaces are built before the kernel `CR3` switch.
    // 2. Established by: `PhysicalMemory::allocate_zeroed_page` returning
    //    success during the pre-activation address-space construction phase.
    // 3. Lifetime: the frame is owned by the new user address space after this
    //    copy and is tracked in `user_elf_frames`.
    // 4. Pointer ownership: PythCore has exclusive mutable ownership of the
    //    freshly allocated frame.
    // 5. Alignment: allocator returns 4 KiB-aligned page frames.
    // 6. Mapped length: exactly one 4 KiB page is written.
    // 7. Concurrency: single-core early boot before user program execution.
    // 8. Violation: a bad frame pointer would corrupt memory or fault before
    //    the Phase 9 marker can be emitted.
    let destination =
        unsafe { core::slice::from_raw_parts_mut(physical as *mut u8, PAGE_SIZE as usize) };
    destination.fill(0);
    if copy_start < copy_end {
        let source_start = segment
            .file_offset()
            .checked_add(
                copy_start
                    .checked_sub(file_virtual_start)
                    .ok_or(VmError::RangeOverflow)?,
            )
            .ok_or(VmError::RangeOverflow)?;
        let source_len = copy_end
            .checked_sub(copy_start)
            .ok_or(VmError::RangeOverflow)?;
        let source_end = source_start
            .checked_add(source_len)
            .ok_or(VmError::RangeOverflow)?;
        let source_start = usize::try_from(source_start).map_err(|_| VmError::RangeOverflow)?;
        let source_end = usize::try_from(source_end).map_err(|_| VmError::RangeOverflow)?;
        let dest_start = usize::try_from(
            copy_start
                .checked_sub(page_virtual_start)
                .ok_or(VmError::RangeOverflow)?,
        )
        .map_err(|_| VmError::RangeOverflow)?;
        let source = elf_bytes
            .get(source_start..source_end)
            .ok_or(VmError::UnmappedSource)?;
        let dest_end = dest_start
            .checked_add(source.len())
            .ok_or(VmError::RangeOverflow)?;
        destination
            .get_mut(dest_start..dest_end)
            .ok_or(VmError::RangeOverflow)?
            .copy_from_slice(source);
    }
    Ok(())
}

fn user_elf_page_flags(segment: &user_elf::LoadSegment) -> u64 {
    let mut flags = 0u64;
    if segment.writable() {
        flags |= PTE_WRITE;
    }
    if !segment.executable() {
        flags |= PTE_NO_EXECUTE;
    }
    flags
}

fn map_firmware_tables(
    tables: &mut PageTableBuilder<'_>,
    boot_info: &PythBootInfo,
) -> Result<(), VmError> {
    if boot_info.acpi_rsdp == 0 || boot_info.smbios_entry == 0 {
        return Err(VmError::BadBootInfo);
    }

    let rsdp_len = acpi_rsdp_len(boot_info.acpi_rsdp)?;
    tables.map_physical_range(
        boot_info.acpi_rsdp,
        boot_info.acpi_rsdp,
        rsdp_len,
        PTE_NO_EXECUTE,
    )?;
    let root_table = acpi_root_table(boot_info.acpi_rsdp)?;
    let root_len = acpi_sdt_len(root_table)?;
    tables.map_physical_range(root_table, root_table, root_len, PTE_NO_EXECUTE)?;

    let smbios_len = smbios_entry_len(boot_info.smbios_entry)?;
    tables.map_physical_range(
        boot_info.smbios_entry,
        boot_info.smbios_entry,
        smbios_len,
        PTE_NO_EXECUTE,
    )?;
    Ok(())
}

fn acpi_rsdp_len(address: u64) -> Result<u64, VmError> {
    if address == 0 {
        return Err(VmError::BadBootInfo);
    }
    let revision = read_u8(address, 15);
    if revision < 2 {
        return Ok(20);
    }
    let length = u64::from(read_u32(address, 20));
    if !(36..=4096).contains(&length) {
        return Err(VmError::BadBootInfo);
    }
    Ok(length)
}

fn acpi_root_table(rsdp: u64) -> Result<u64, VmError> {
    let revision = read_u8(rsdp, 15);
    if revision >= 2 {
        let xsdt = read_u64(rsdp, 24);
        if xsdt != 0 {
            return Ok(xsdt);
        }
    }
    let rsdt = u64::from(read_u32(rsdp, 16));
    if rsdt == 0 {
        return Err(VmError::BadBootInfo);
    }
    Ok(rsdt)
}

fn acpi_sdt_len(address: u64) -> Result<u64, VmError> {
    let length = u64::from(read_u32(address, 4));
    if !(36..=4096).contains(&length) {
        return Err(VmError::BadBootInfo);
    }
    Ok(length)
}

fn smbios_entry_len(address: u64) -> Result<u64, VmError> {
    if address == 0 {
        return Err(VmError::BadBootInfo);
    }
    if read_bytes_eq(address, b"_SM3_") {
        let length = u64::from(read_u8(address, 6));
        if length < 0x18 {
            return Err(VmError::BadBootInfo);
        }
        return Ok(length);
    }
    if read_bytes_eq(address, b"_SM_") {
        let length = u64::from(read_u8(address, 5));
        if length < 0x1F {
            return Err(VmError::BadBootInfo);
        }
        return Ok(length);
    }
    Err(VmError::BadBootInfo)
}

fn translate_active(virt: u64) -> Result<u64, VmError> {
    translate_from_root(read_cr3(), virt)
}

fn translate_from_root(root_table_phys: u64, virt: u64) -> Result<u64, VmError> {
    let pml4e = read_entry(root_table_phys, table_index(virt, 39));
    if pml4e & PTE_PRESENT == 0 {
        return Err(VmError::UnmappedSource);
    }
    let pdpte = read_entry(pml4e & ADDR_MASK, table_index(virt, 30));
    if pdpte & PTE_PRESENT == 0 {
        return Err(VmError::UnmappedSource);
    }
    if pdpte & PTE_HUGE != 0 {
        return Ok((pdpte & ADDR_MASK) + (virt & 0x3FFF_FFFF));
    }
    let pde = read_entry(pdpte & ADDR_MASK, table_index(virt, 21));
    if pde & PTE_PRESENT == 0 {
        return Err(VmError::UnmappedSource);
    }
    if pde & PTE_HUGE != 0 {
        return Ok((pde & ADDR_MASK) + (virt & OFFSET_2M_MASK));
    }
    let pte = read_entry(pde & ADDR_MASK, table_index(virt, 12));
    if pte & PTE_PRESENT == 0 {
        return Err(VmError::UnmappedSource);
    }
    Ok((pte & ADDR_MASK) + (virt & OFFSET_4K_MASK))
}

fn user_can_access_from_root(root_table_phys: u64, virt: u64) -> Result<bool, VmError> {
    let pml4e = read_entry(root_table_phys, table_index(virt, 39));
    if !present_user(pml4e) {
        return Ok(false);
    }
    let pdpte = read_entry(pml4e & ADDR_MASK, table_index(virt, 30));
    if !present_user(pdpte) {
        return Ok(false);
    }
    if pdpte & PTE_HUGE != 0 {
        return Ok(true);
    }
    let pde = read_entry(pdpte & ADDR_MASK, table_index(virt, 21));
    if !present_user(pde) {
        return Ok(false);
    }
    if pde & PTE_HUGE != 0 {
        return Ok(true);
    }
    let pte = read_entry(pde & ADDR_MASK, table_index(virt, 12));
    Ok(present_user(pte))
}

fn user_leaf_entry_from_root(root_table_phys: u64, virt: u64) -> Result<u64, VmError> {
    let pml4e = read_entry(root_table_phys, table_index(virt, 39));
    if !present_user(pml4e) {
        return Err(VmError::UserAccessViolation);
    }
    let pdpte = read_entry(pml4e & ADDR_MASK, table_index(virt, 30));
    if !present_user(pdpte) || pdpte & PTE_HUGE != 0 {
        return Err(VmError::UserAccessViolation);
    }
    let pde = read_entry(pdpte & ADDR_MASK, table_index(virt, 21));
    if !present_user(pde) || pde & PTE_HUGE != 0 {
        return Err(VmError::UserAccessViolation);
    }
    let pte = read_entry(pde & ADDR_MASK, table_index(virt, 12));
    if !present_user(pte) {
        return Err(VmError::UserAccessViolation);
    }
    Ok(pte)
}

fn present_user(entry: u64) -> bool {
    entry & PTE_PRESENT != 0 && entry & PTE_USER != 0
}

fn read_cr3() -> u64 {
    let cr3: u64;
    // SAFETY:
    // 1. Invariant: reading CR3 is valid in x86-64 ring 0.
    // 2. Established by: PythCore runs as the native kernel after firmware exit.
    // 3. Lifetime: this instruction has no borrowed memory lifetime.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable.
    // 6. Mapped length: not applicable.
    // 7. Concurrency: single-core execution with interrupts disabled.
    // 8. Violation: none in ring 0; outside ring 0 this would fault.
    unsafe {
        asm!("mov {out}, cr3", out = out(reg) cr3, options(nomem, nostack, preserves_flags));
    }
    cr3 & ADDR_MASK
}

fn read_entry(table_phys: u64, index: usize) -> u64 {
    debug_assert!(index < ENTRY_COUNT);
    // SAFETY:
    // 1. Invariant: `table_phys` names a mapped 4 KiB page-table frame and
    //    `index` is below 512.
    // 2. Established by: callers use CR3-derived page-table frames or frames
    //    allocated and remembered by `PageTableBuilder`.
    // 3. Lifetime: page-table frames remain allocated for this slice.
    // 4. Pointer ownership: PythCore owns new frames; old loader frames are
    //    read-only inputs before replacement.
    // 5. Alignment: page-table frames are 4 KiB aligned; entries are 8-byte aligned.
    // 6. Mapped length: one 4 KiB page contains all 512 entries.
    // 7. Concurrency: single-core execution with interrupts disabled.
    // 8. Violation: a bad frame would fault or read unrelated memory.
    unsafe { (table_phys as *const u64).add(index).read_volatile() }
}

fn write_entry(table_phys: u64, index: usize, value: u64) {
    debug_assert!(index < ENTRY_COUNT);
    // SAFETY:
    // 1. Invariant: `table_phys` names a mapped PythCore-owned 4 KiB
    //    page-table frame and `index` is below 512.
    // 2. Established by: `PageTableBuilder` only writes frames it allocated.
    // 3. Lifetime: page-table frames remain allocated for this slice.
    // 4. Pointer ownership: PythCore exclusively owns the destination frame.
    // 5. Alignment: page-table frames are 4 KiB aligned; entries are 8-byte aligned.
    // 6. Mapped length: one 4 KiB page contains all 512 entries.
    // 7. Concurrency: single-core execution with interrupts disabled.
    // 8. Violation: writing outside the frame would corrupt memory.
    unsafe { (table_phys as *mut u64).add(index).write_volatile(value) }
}

fn read_u8(address: u64, offset: usize) -> u8 {
    // SAFETY:
    // 1. Invariant: `address + offset` is readable through the loader's
    //    active identity mapping while replacement page tables are being built.
    // 2. Established by: firmware table addresses were validated by the loader
    //    before `ExitBootServices()` and are parsed before the second CR3 switch.
    // 3. Lifetime: firmware metadata remains reserved during early boot.
    // 4. Pointer ownership: firmware owns the table; PythCore reads it.
    // 5. Alignment: byte reads require no stronger alignment.
    // 6. Mapped length: caller reads fields within fixed firmware headers.
    // 7. Concurrency: single-core early boot with interrupts disabled.
    // 8. Violation: a bad firmware pointer would fault before VM activation.
    unsafe { ((address as *const u8).add(offset)).read_volatile() }
}

fn read_u32(address: u64, offset: usize) -> u32 {
    // SAFETY:
    // 1. Invariant: `address + offset..offset+4` is readable through the
    //    loader's active identity mapping while replacement tables are built.
    // 2. Established by: loader-side firmware table validation and bounded
    //    field offsets in ACPI headers.
    // 3. Lifetime: firmware metadata remains reserved during early boot.
    // 4. Pointer ownership: firmware owns the table; PythCore reads it.
    // 5. Alignment: `read_unaligned` handles packed firmware fields.
    // 6. Mapped length: 4 bytes at the requested offset.
    // 7. Concurrency: single-core early boot with interrupts disabled.
    // 8. Violation: a bad firmware pointer would fault before VM activation.
    unsafe {
        (address as *const u8)
            .add(offset)
            .cast::<u32>()
            .read_unaligned()
    }
}

fn read_u64(address: u64, offset: usize) -> u64 {
    // SAFETY:
    // 1. Invariant: `address + offset..offset+8` is readable through the
    //    loader's active identity mapping while replacement tables are built.
    // 2. Established by: loader-side RSDP validation and bounded field offsets.
    // 3. Lifetime: firmware metadata remains reserved during early boot.
    // 4. Pointer ownership: firmware owns the table; PythCore reads it.
    // 5. Alignment: `read_unaligned` handles packed firmware fields.
    // 6. Mapped length: 8 bytes at the requested offset.
    // 7. Concurrency: single-core early boot with interrupts disabled.
    // 8. Violation: a bad firmware pointer would fault before VM activation.
    unsafe {
        (address as *const u8)
            .add(offset)
            .cast::<u64>()
            .read_unaligned()
    }
}

fn read_bytes_eq(address: u64, expected: &[u8]) -> bool {
    for (offset, &byte) in expected.iter().enumerate() {
        if read_u8(address, offset) != byte {
            return false;
        }
    }
    true
}

fn table_index(virt: u64, shift: u32) -> usize {
    ((virt >> shift) & 0x1FF) as usize
}

fn align_down(value: u64) -> u64 {
    value & !(PAGE_SIZE - 1)
}

fn align_up(value: u64) -> Result<u64, VmError> {
    value
        .checked_add(PAGE_SIZE - 1)
        .map(align_down)
        .ok_or(VmError::RangeOverflow)
}

fn min_u64(left: u64, right: u64) -> u64 {
    if left < right { left } else { right }
}

fn max_u64(left: u64, right: u64) -> u64 {
    if left > right { left } else { right }
}

fn symbol_addr(symbol: *const u8) -> u64 {
    symbol as u64
}

fn symbol_range_len(start: *const u8, end: *const u8) -> Result<u64, VmError> {
    symbol_addr(end)
        .checked_sub(symbol_addr(start))
        .ok_or(VmError::RangeOverflow)
}

// This module has no `cargo test` surface at all: `memory/mod.rs` declares
// `pub mod r#virtual;` as `#[cfg(not(test))]`, so nothing in this file is
// even compiled under `cargo test` (a `#[cfg(test)] mod tests` block here
// would silently never run). `read_entry`/`write_entry`/`translate_active`
// also dereference raw physical addresses and read the live CR3 register,
// which only make sense running as the real kernel; every existing
// correctness proof for this file's mappings (e.g.
// `prove_syscall_stack_guard_pages_unmapped`) is a QEMU boot-marker proof,
// not a unit test. `PTE_CACHE_DISABLE`'s bit value and its composition with
// the existing flags is instead checked at compile time below, which *does*
// run whenever this module is built (i.e. every non-test build); the
// mapping's actual effect on real AHCI hardware is proven functionally by
// Task E's QEMU integration test.
const _: () = assert!(PTE_CACHE_DISABLE == 1 << 4);
const _: () = {
    let flags = PTE_WRITE | PTE_NO_EXECUTE | PTE_CACHE_DISABLE;
    assert!(flags & PTE_WRITE == PTE_WRITE);
    assert!(flags & PTE_NO_EXECUTE == PTE_NO_EXECUTE);
    assert!(flags & PTE_CACHE_DISABLE == PTE_CACHE_DISABLE);
    assert!(flags & PTE_PRESENT == 0);
    assert!(flags & PTE_USER == 0);
    assert!(flags & PTE_HUGE == 0);
};
