use crate::elf::LoadedKernel;
use crate::firmware::FirmwareTables;
use crate::initrd::LoadedInitBundle;
use crate::memory_map::CapturedMemoryMap;
use crate::paging::BootstrapStack;
use crate::uefi::{self, EFI_LOADER_DATA, EFI_SUCCESS, EfiSystemTable};
use core::ffi::c_void;
use core::mem;
use core::ptr;
use pythos_shared::boot_protocol::{
    PYTH_BOOT_ABI_MAJOR, PYTH_BOOT_ABI_MINOR, PYTH_BOOT_MAGIC, PythBootInfo, PythFramebufferInfo,
};

pub(crate) struct AllocatedBootInfo {
    ptr: *mut PythBootInfo,
}

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

    pub(crate) fn populate(
        &self,
        framebuffer: PythFramebufferInfo,
        kernel: &LoadedKernel,
        init_bundle: &LoadedInitBundle,
        memory_map: &CapturedMemoryMap,
        stack: &BootstrapStack,
        firmware_tables: FirmwareTables,
    ) -> Result<*const PythBootInfo, ()> {
        if self.ptr.is_null()
            || !kernel.is_well_formed()
            || !init_bundle.is_loaded()
            || !memory_map.is_captured()
            || stack.physical_start == 0
            || stack.virt_bottom >= stack.virt_top
        {
            return Err(());
        }
        let descriptor_size = u32::try_from(memory_map.descriptor_size).map_err(|_| ())?;
        let memory_map_len = u64::try_from(memory_map.len).map_err(|_| ())?;

        let boot_info = PythBootInfo {
            magic: PYTH_BOOT_MAGIC,
            abi_major: PYTH_BOOT_ABI_MAJOR,
            abi_minor: PYTH_BOOT_ABI_MINOR,
            struct_size: mem::size_of::<PythBootInfo>() as u32,
            flags: 0,
            memory_map_ptr: memory_map.ptr as u64,
            memory_map_len,
            memory_descriptor_size: descriptor_size,
            memory_descriptor_version: memory_map.descriptor_version,
            framebuffer,
            acpi_rsdp: firmware_tables.acpi_rsdp,
            smbios_entry: firmware_tables.smbios_entry,
            kernel_phys_start: kernel.physical_start,
            kernel_phys_end: kernel.physical_end,
            kernel_virt_start: kernel.virtual_start,
            kernel_virt_end: kernel.virtual_end,
            bootstrap_stack_bottom: stack.virt_bottom,
            bootstrap_stack_top: stack.virt_top,
            init_bundle_phys: init_bundle.physical_start,
            init_bundle_len: init_bundle.len,
            runtime_services_ptr: 0,
            command_line_ptr: 0,
            command_line_len: 0,
            reserved: [0; 8],
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
