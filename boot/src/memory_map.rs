use crate::uefi::{
    self, EFI_BUFFER_TOO_SMALL, EFI_LOADER_DATA, EFI_SUCCESS, EfiBootServices, EfiMemoryDescriptor,
    EfiSystemTable,
};
use core::ffi::c_void;
use core::ptr;

const EXTRA_DESCRIPTOR_CAPACITY: usize = 16;

pub(crate) struct CapturedMemoryMap {
    pub(crate) ptr: *mut EfiMemoryDescriptor,
    pub(crate) len: usize,
    capacity: usize,
    pub(crate) map_key: usize,
    pub(crate) descriptor_size: usize,
    pub(crate) descriptor_version: u32,
}

impl CapturedMemoryMap {
    pub(crate) fn is_captured(&self) -> bool {
        !self.ptr.is_null()
            && self.len > 0
            && self.capacity >= self.len
            && self.map_key != 0
            && self.descriptor_size >= core::mem::size_of::<EfiMemoryDescriptor>()
            && self.len.is_multiple_of(self.descriptor_size)
    }

    pub(crate) fn refresh(&mut self, system_table: *mut EfiSystemTable) -> Result<(), ()> {
        if self.ptr.is_null() || self.capacity == 0 {
            return Err(());
        }
        let boot_services = uefi::boot_services(system_table).map_err(|_| ())?;
        let mut actual_size = self.capacity;
        let mut map_key = 0usize;
        let mut descriptor_size = 0usize;
        let mut descriptor_version = 0u32;

        // SAFETY:
        // 1. Invariant: `self.ptr` points to a retained map buffer of `self.capacity` bytes.
        // 2. Established by: `capture()` allocated this buffer before the first final map retrieval.
        // 3. Lifetime: buffer remains loader-owned until handoff.
        // 4. Pointer ownership: firmware writes descriptors into loader-owned memory.
        // 5. Alignment: firmware pool allocation satisfies descriptor alignment.
        // 6. Mapped length: `actual_size` initially equals the allocated capacity.
        // 7. Concurrency: no allocations are performed by this refresh path.
        // 8. Violation: invalid buffer/capacity would let firmware overwrite memory.
        let status = unsafe {
            ((*boot_services).get_memory_map)(
                &mut actual_size,
                self.ptr,
                &mut map_key,
                &mut descriptor_size,
                &mut descriptor_version,
            )
        };
        if status != EFI_SUCCESS || actual_size == 0 || descriptor_size == 0 {
            return Err(());
        }

        self.len = actual_size;
        self.map_key = map_key;
        self.descriptor_size = descriptor_size;
        self.descriptor_version = descriptor_version;
        if self.is_captured() { Ok(()) } else { Err(()) }
    }
}

pub(crate) fn capture(system_table: *mut EfiSystemTable) -> Result<CapturedMemoryMap, ()> {
    let boot_services = uefi::boot_services(system_table).map_err(|_| ())?;
    let mut required_size = 0usize;
    let mut map_key = 0usize;
    let mut descriptor_size = 0usize;
    let mut descriptor_version = 0u32;

    // SAFETY:
    // 1. Invariant: `boot_services` points to the active UEFI boot services table.
    // 2. Established by: `uefi::boot_services()` from the firmware-provided system table.
    // 3. Lifetime: boot services remain valid because `ExitBootServices()` has not been called.
    // 4. Pointer ownership: output scalars are loader-owned stack locals.
    // 5. Alignment: output scalar pointers are naturally aligned.
    // 6. Mapped length: no descriptor buffer is supplied for this sizing call.
    // 7. Concurrency: no concurrent memory-map mutation by loader code.
    // 8. Violation: invalid boot services table could call through a bad function pointer.
    let status = unsafe {
        ((*boot_services).get_memory_map)(
            &mut required_size,
            ptr::null_mut(),
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        )
    };
    if status != EFI_BUFFER_TOO_SMALL || descriptor_size == 0 {
        return Err(());
    }

    let extra = descriptor_size
        .checked_mul(EXTRA_DESCRIPTOR_CAPACITY)
        .ok_or(())?;
    let capacity = required_size.checked_add(extra).ok_or(())?;
    let buffer = allocate_pool(boot_services, capacity)?;

    let mut actual_size = capacity;
    // SAFETY:
    // 1. Invariant: `buffer` points to a pool allocation of `capacity` bytes.
    // 2. Established by: successful `AllocatePool` immediately above.
    // 3. Lifetime: allocation remains owned by the loader for PythCore handoff.
    // 4. Pointer ownership: firmware writes descriptors into loader-owned memory.
    // 5. Alignment: firmware pool allocations satisfy descriptor alignment.
    // 6. Mapped length: `actual_size` initially equals the allocated `capacity`.
    // 7. Concurrency: no allocations occur after this final memory-map call in this slice.
    // 8. Violation: invalid buffer/capacity would let firmware overwrite memory.
    let status = unsafe {
        ((*boot_services).get_memory_map)(
            &mut actual_size,
            buffer.cast(),
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        )
    };
    if status != EFI_SUCCESS || actual_size == 0 || descriptor_size == 0 {
        return Err(());
    }

    let captured = CapturedMemoryMap {
        ptr: buffer.cast(),
        len: actual_size,
        capacity,
        map_key,
        descriptor_size,
        descriptor_version,
    };
    if captured.is_captured() {
        Ok(captured)
    } else {
        Err(())
    }
}

fn allocate_pool(boot_services: *mut EfiBootServices, size: usize) -> Result<*mut c_void, ()> {
    let mut buffer: *mut c_void = ptr::null_mut();
    // SAFETY:
    // 1. Invariant: `boot_services` points to the active UEFI boot services table.
    // 2. Established by: `uefi::boot_services()` before memory-map capture.
    // 3. Lifetime: allocation remains valid until released or handed to PythCore.
    // 4. Pointer ownership: loader owns the returned pool allocation.
    // 5. Alignment: firmware pool allocations satisfy general-purpose alignment.
    // 6. Mapped length: `size` bytes are requested.
    // 7. Concurrency: no concurrent pool allocation users in this loader slice.
    // 8. Violation: invalid boot services table could call through a bad function pointer.
    let status = unsafe { ((*boot_services).allocate_pool)(EFI_LOADER_DATA, size, &mut buffer) };
    if status == EFI_SUCCESS && !buffer.is_null() {
        Ok(buffer)
    } else {
        Err(())
    }
}
