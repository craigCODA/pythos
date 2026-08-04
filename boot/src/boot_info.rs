use crate::elf::LoadedKernel;
#[cfg(feature = "evidence-terminal")]
use crate::evidence_log::AllocatedEvidenceLog;
use crate::firmware::FirmwareTables;
use crate::font::LoadedFont;
use crate::initrd::LoadedInitBundle;
use crate::memory_map::CapturedMemoryMap;
use crate::paging::BootstrapStack;
use crate::uefi::{self, EFI_LOADER_DATA, EFI_SUCCESS, EfiSystemTable};
use core::ffi::c_void;
use core::mem;
use core::ptr;
use pythos_shared::boot_protocol::{
    PYTH_BOOT_ABI_MAJOR, PYTH_BOOT_ABI_MINOR, PYTH_BOOT_MAGIC, PYTH_EVIDENCE_LOG_FLAG_PRESENT,
    PythBootInfo, PythFramebufferInfo,
};

pub(crate) struct AllocatedBootInfo {
    ptr: *mut PythBootInfo,
}

pub(crate) struct BootInfoInputs<'a> {
    pub(crate) framebuffer: PythFramebufferInfo,
    pub(crate) kernel: &'a LoadedKernel,
    pub(crate) init_bundle: &'a LoadedInitBundle,
    pub(crate) font: &'a LoadedFont,
    pub(crate) memory_map: &'a CapturedMemoryMap,
    pub(crate) stack: &'a BootstrapStack,
    pub(crate) firmware_tables: FirmwareTables,
    pub(crate) evidence_log: EvidenceLogInput<'a>,
}

#[derive(Clone, Copy)]
pub(crate) struct EvidenceLogMetadata {
    pub(crate) physical_start: u64,
    pub(crate) len: u32,
}

#[cfg(feature = "evidence-terminal")]
pub(crate) type EvidenceLogInput<'a> = Option<&'a AllocatedEvidenceLog>;

#[cfg(not(feature = "evidence-terminal"))]
pub(crate) type EvidenceLogInput<'a> = Option<()>;

impl AllocatedBootInfo {
    pub(crate) fn allocate(system_table: *mut EfiSystemTable) -> Result<Self, ()> {
        let boot_services = uefi::boot_services(system_table).map_err(|_| ())?;
        let mut buffer: *mut c_void = ptr::null_mut();
        // SAFETY:
        // 1. Invariant: `boot_services` points to the active UEFI boot services table.
        // 2. Established by: `uefi::boot_services()` from the firmware-provided system table.
        // 3. Lifetime: allocation remains owned by the loader for later PythCore handoff.
        // 4. Pointer ownership: loader owns the returned pool allocation.
        // 5. Alignment: firmware pool allocations satisfy `PythBootInfo` alignment.
        // 6. Mapped length: exactly `size_of::<PythBootInfo>()` bytes are requested.
        // 7. Concurrency: no concurrent pool allocation users in this loader slice.
        // 8. Violation: invalid boot services table could call through a bad function pointer.
        let status = unsafe {
            ((*boot_services).allocate_pool)(
                EFI_LOADER_DATA,
                mem::size_of::<PythBootInfo>(),
                &mut buffer,
            )
        };
        if status != EFI_SUCCESS || buffer.is_null() {
            return Err(());
        }
        Ok(Self { ptr: buffer.cast() })
    }

    pub(crate) fn populate(&self, inputs: BootInfoInputs<'_>) -> Result<*const PythBootInfo, ()> {
        if self.ptr.is_null()
            || !inputs.kernel.is_well_formed()
            || !inputs.init_bundle.is_loaded()
            || !inputs.font.is_loaded()
            || !inputs.memory_map.is_captured()
            || inputs.stack.physical_start == 0
            || inputs.stack.virt_bottom >= inputs.stack.virt_top
        {
            return Err(());
        }
        let descriptor_size = u32::try_from(inputs.memory_map.descriptor_size).map_err(|_| ())?;
        let memory_map_len = u64::try_from(inputs.memory_map.len).map_err(|_| ())?;

        let evidence_log = evidence_log_metadata(inputs.evidence_log);
        let boot_info = PythBootInfo {
            magic: PYTH_BOOT_MAGIC,
            abi_major: PYTH_BOOT_ABI_MAJOR,
            abi_minor: PYTH_BOOT_ABI_MINOR,
            struct_size: mem::size_of::<PythBootInfo>() as u32,
            flags: 0,
            memory_map_ptr: inputs.memory_map.ptr as u64,
            memory_map_len,
            memory_descriptor_size: descriptor_size,
            memory_descriptor_version: inputs.memory_map.descriptor_version,
            framebuffer: inputs.framebuffer,
            acpi_rsdp: inputs.firmware_tables.acpi_rsdp,
            smbios_entry: inputs.firmware_tables.smbios_entry,
            kernel_phys_start: inputs.kernel.physical_start,
            kernel_phys_end: inputs.kernel.physical_end,
            kernel_virt_start: inputs.kernel.virtual_start,
            kernel_virt_end: inputs.kernel.virtual_end,
            bootstrap_stack_bottom: inputs.stack.virt_bottom,
            bootstrap_stack_top: inputs.stack.virt_top,
            init_bundle_phys: inputs.init_bundle.physical_start,
            init_bundle_len: inputs.init_bundle.len,
            font_phys: inputs.font.physical_start,
            font_len: inputs.font.len,
            runtime_services_ptr: 0,
            command_line_ptr: 0,
            command_line_len: 0,
            evidence_log_phys: evidence_log.map_or(0, |log| log.physical_start),
            evidence_log_len: evidence_log.map_or(0, |log| log.len),
            evidence_log_flags: evidence_log.map_or(0, |_| PYTH_EVIDENCE_LOG_FLAG_PRESENT),
            reserved: [0; 6],
        };

        // SAFETY:
        // 1. Invariant: `self.ptr` points to a pool allocation sized for `PythBootInfo`.
        // 2. Established by: `AllocatedBootInfo::allocate`.
        // 3. Lifetime: allocation remains owned by the loader for later PythCore handoff.
        // 4. Pointer ownership: loader owns the destination allocation.
        // 5. Alignment: firmware pool allocation is sufficient for `PythBootInfo`.
        // 6. Mapped length: exactly one `PythBootInfo` is written.
        // 7. Concurrency: no concurrent access to the boot-info allocation.
        // 8. Violation: invalid allocation would corrupt memory or fault.
        unsafe {
            self.ptr.write(boot_info);
        }

        Ok(self.ptr.cast_const())
    }
}

#[cfg(feature = "evidence-terminal")]
fn evidence_log_metadata(input: EvidenceLogInput<'_>) -> Option<EvidenceLogMetadata> {
    input.map(AllocatedEvidenceLog::metadata)
}

#[cfg(not(feature = "evidence-terminal"))]
fn evidence_log_metadata(_input: EvidenceLogInput<'_>) -> Option<EvidenceLogMetadata> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::{LoadedKernel, LoadedSegment, SegmentPermissions};
    #[cfg(feature = "evidence-terminal")]
    use crate::evidence_log::AllocatedEvidenceLog;
    use crate::firmware::FirmwareTables;
    use crate::font::LoadedFont;
    use crate::initrd::LoadedInitBundle;
    use crate::memory_map::CapturedMemoryMap;
    use crate::paging::BootstrapStack;
    use core::mem;
    use pythos_shared::boot_protocol::PIXEL_FORMAT_BGR_RESERVED_8BIT;
    #[cfg(feature = "evidence-terminal")]
    use pythos_shared::evidence_log::EVIDENCE_LOG_TOTAL_BYTES;

    fn populate_boot_info(
        storage: &mut PythBootInfo,
        evidence_log: EvidenceLogInput<'_>,
    ) -> Result<*const PythBootInfo, ()> {
        let allocated = AllocatedBootInfo {
            ptr: storage as *mut PythBootInfo,
        };
        let kernel = loaded_kernel();
        let init_bundle = loaded_init_bundle();
        let font = loaded_font();
        let memory_map = captured_memory_map();
        let stack = bootstrap_stack();
        allocated.populate(BootInfoInputs {
            framebuffer: framebuffer(),
            kernel: &kernel,
            init_bundle: &init_bundle,
            font: &font,
            memory_map: &memory_map,
            stack: &stack,
            firmware_tables: FirmwareTables {
                acpi_rsdp: 0x50_0000,
                smbios_entry: 0x60_0000,
            },
            evidence_log,
        })
    }

    fn blank_boot_info() -> PythBootInfo {
        PythBootInfo {
            magic: 0,
            abi_major: 0,
            abi_minor: 0,
            struct_size: 0,
            flags: 0,
            memory_map_ptr: 0,
            memory_map_len: 0,
            memory_descriptor_size: 0,
            memory_descriptor_version: 0,
            framebuffer: framebuffer(),
            acpi_rsdp: 0,
            smbios_entry: 0,
            kernel_phys_start: 0,
            kernel_phys_end: 0,
            kernel_virt_start: 0,
            kernel_virt_end: 0,
            bootstrap_stack_bottom: 0,
            bootstrap_stack_top: 0,
            init_bundle_phys: 0,
            init_bundle_len: 0,
            font_phys: 0,
            font_len: 0,
            runtime_services_ptr: 0,
            command_line_ptr: 0,
            command_line_len: 0,
            evidence_log_phys: 0,
            evidence_log_len: 0,
            evidence_log_flags: 0,
            reserved: [0; 6],
        }
    }

    #[test]
    fn boot_info_populates_absent_evidence_metadata() {
        let mut storage = blank_boot_info();
        populate_boot_info(&mut storage, None).unwrap();

        assert_eq!(storage.evidence_log_phys, 0);
        assert_eq!(storage.evidence_log_len, 0);
        assert_eq!(storage.evidence_log_flags, 0);
    }

    #[test]
    #[cfg(feature = "evidence-terminal")]
    fn boot_info_populates_present_evidence_metadata() {
        let log = AllocatedEvidenceLog::for_test(0x80_0000, EVIDENCE_LOG_TOTAL_BYTES as u32);
        let mut storage = blank_boot_info();
        let ptr = populate_boot_info(&mut storage, Some(&log)).unwrap();

        // SAFETY:
        // 1. Invariant: `populate_boot_info` returns a pointer to one initialized
        //    `PythBootInfo` in caller-owned test storage.
        // 2. Established by: this test keeps `storage` alive for this assertion.
        // 3. Lifetime: valid until the test function exits.
        // 4. Pointer ownership: this test owns the backing object.
        // 5. Alignment: `storage` is a real `PythBootInfo`.
        // 6. Mapped length: exactly one `PythBootInfo`.
        // 7. Concurrency: single-threaded unit test.
        // 8. Violation: a bad helper pointer would make the test fail or fault.
        let by_ptr = unsafe { &*ptr };
        assert_eq!(by_ptr.evidence_log_phys, 0x80_0000);
        assert_eq!(by_ptr.evidence_log_len, EVIDENCE_LOG_TOTAL_BYTES as u32);
        assert_eq!(by_ptr.evidence_log_flags, PYTH_EVIDENCE_LOG_FLAG_PRESENT);
    }

    fn framebuffer() -> PythFramebufferInfo {
        PythFramebufferInfo {
            physical_base: 0xC000_0000,
            mapped_virtual_base: 0xFFFF_C000_0000_0000,
            byte_length: 1024 * 768 * 4,
            width: 1024,
            height: 768,
            pixels_per_scanline: 1024,
            pixel_format: PIXEL_FORMAT_BGR_RESERVED_8BIT,
            red_mask: 0,
            green_mask: 0,
            blue_mask: 0,
            reserved_mask: 0,
        }
    }

    fn loaded_kernel() -> LoadedKernel {
        LoadedKernel {
            entry: 0xFFFF_FFFF_8000_1000,
            segments: [LoadedSegment {
                physical_start: 0x10_0000,
                virtual_start: 0xFFFF_FFFF_8000_0000,
                page_count: 1,
                file_size: 0x1000,
                memory_size: 0x1000,
                permissions: SegmentPermissions {
                    readable: true,
                    writable: false,
                    executable: true,
                },
            }; 16],
            segment_count: 1,
            physical_start: 0x10_0000,
            physical_end: 0x11_0000,
            virtual_start: 0xFFFF_FFFF_8000_0000,
            virtual_end: 0xFFFF_FFFF_8001_0000,
        }
    }

    fn loaded_init_bundle() -> LoadedInitBundle {
        LoadedInitBundle {
            physical_start: 0x30_0000,
            len: 4096,
            page_count: 1,
        }
    }

    fn loaded_font() -> LoadedFont {
        LoadedFont {
            physical_start: 0x40_0000,
            len: 4096,
            page_count: 1,
        }
    }

    fn captured_memory_map() -> CapturedMemoryMap {
        let descriptor_size = mem::size_of::<crate::uefi::EfiMemoryDescriptor>();
        CapturedMemoryMap::for_test(
            0x70_0000 as *mut _,
            descriptor_size * 4,
            7,
            descriptor_size,
            1,
        )
    }

    fn bootstrap_stack() -> BootstrapStack {
        BootstrapStack {
            physical_start: 0x90_0000,
            virt_bottom: 0xFFFF_E000_0000_1000,
            virt_top: 0xFFFF_E000_0001_1000,
            page_count: 16,
        }
    }
}
