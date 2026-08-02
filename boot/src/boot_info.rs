use crate::elf::LoadedKernel;
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
    PYTH_BOOT_ABI_MAJOR, PYTH_BOOT_ABI_MINOR, PYTH_BOOT_MAGIC, PythBootInfo, PythFramebufferInfo,
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
            evidence_log_phys: 0,
            evidence_log_len: 0,
            evidence_log_flags: 0,
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
