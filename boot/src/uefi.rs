use core::ffi::c_void;
use core::ptr;

pub(crate) type EfiStatus = usize;
pub(crate) type EfiPhysicalAddress = u64;

pub(crate) const EFI_SUCCESS: EfiStatus = 0;
pub(crate) const EFI_LOAD_ERROR: EfiStatus = 1;
pub(crate) const EFI_LOADER_DATA: u32 = 2;
pub(crate) const EFI_ALLOCATE_ANY_PAGES: u32 = 0;
pub(crate) const EFI_FILE_MODE_READ: u64 = 0x0000_0000_0000_0001;
pub(crate) const EFI_INVALID_PARAMETER: EfiStatus = (1usize << (usize::BITS - 1)) | 2;
pub(crate) const EFI_BUFFER_TOO_SMALL: EfiStatus = (1usize << (usize::BITS - 1)) | 5;

const LOADED_IMAGE_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data1: 0x5B1B_31A1,
    data2: 0x9562,
    data3: 0x11D2,
    data4: [0x8E, 0x3F, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B],
};

const SIMPLE_FILE_SYSTEM_GUID: EfiGuid = EfiGuid {
    data1: 0x964E_5B22,
    data2: 0x6459,
    data3: 0x11D2,
    data4: [0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B],
};

#[repr(C)]
pub struct EfiSystemTable {
    _header: EfiTableHeader,
    _firmware_vendor: *mut u16,
    _firmware_revision: u32,
    _console_in_handle: *mut c_void,
    _con_in: *mut c_void,
    _console_out_handle: *mut c_void,
    _con_out: *mut c_void,
    _standard_error_handle: *mut c_void,
    _std_err: *mut c_void,
    _runtime_services: *mut c_void,
    boot_services: *mut EfiBootServices,
    _number_of_table_entries: usize,
    _configuration_table: *mut c_void,
}

#[repr(C)]
pub(crate) struct EfiTableHeader {
    _signature: u64,
    _revision: u32,
    _header_size: u32,
    _crc32: u32,
    _reserved: u32,
}

#[repr(C)]
pub(crate) struct EfiBootServices {
    _header: EfiTableHeader,
    _raise_tpl: usize,
    _restore_tpl: usize,
    pub(crate) allocate_pages: EfiAllocatePages,
    _free_pages: usize,
    pub(crate) get_memory_map: EfiGetMemoryMap,
    pub(crate) allocate_pool: EfiAllocatePool,
    pub(crate) free_pool: EfiFreePool,
    _create_event: usize,
    _set_timer: usize,
    _wait_for_event: usize,
    _signal_event: usize,
    _close_event: usize,
    _check_event: usize,
    _install_protocol_interface: usize,
    _reinstall_protocol_interface: usize,
    _uninstall_protocol_interface: usize,
    pub(crate) handle_protocol: EfiHandleProtocol,
    _reserved: usize,
    _register_protocol_notify: usize,
    _locate_handle: usize,
    _locate_device_path: usize,
    _install_configuration_table: usize,
    _load_image: usize,
    _start_image: usize,
    _exit: usize,
    _unload_image: usize,
    pub(crate) exit_boot_services: EfiExitBootServices,
    _get_next_monotonic_count: usize,
    _stall: usize,
    _set_watchdog_timer: usize,
    _connect_controller: usize,
    _disconnect_controller: usize,
    _open_protocol: usize,
    _close_protocol: usize,
    _open_protocol_information: usize,
    _protocols_per_handle: usize,
    _locate_handle_buffer: usize,
    pub(crate) locate_protocol: EfiLocateProtocol,
}

pub(crate) type EfiAllocatePages = extern "efiapi" fn(
    allocation_type: u32,
    memory_type: u32,
    pages: usize,
    memory: *mut EfiPhysicalAddress,
) -> EfiStatus;

pub(crate) type EfiAllocatePool =
    extern "efiapi" fn(pool_type: u32, size: usize, buffer: *mut *mut c_void) -> EfiStatus;

pub(crate) type EfiFreePool = extern "efiapi" fn(buffer: *mut c_void) -> EfiStatus;

pub(crate) type EfiExitBootServices =
    extern "efiapi" fn(image_handle: *mut c_void, map_key: usize) -> EfiStatus;

pub(crate) type EfiGetMemoryMap = extern "efiapi" fn(
    memory_map_size: *mut usize,
    memory_map: *mut EfiMemoryDescriptor,
    map_key: *mut usize,
    descriptor_size: *mut usize,
    descriptor_version: *mut u32,
) -> EfiStatus;

pub(crate) type EfiHandleProtocol = extern "efiapi" fn(
    handle: *mut c_void,
    protocol: *const EfiGuid,
    interface: *mut *mut c_void,
) -> EfiStatus;

pub(crate) type EfiLocateProtocol = extern "efiapi" fn(
    protocol: *const EfiGuid,
    registration: *mut c_void,
    interface: *mut *mut c_void,
) -> EfiStatus;

#[repr(C)]
pub(crate) struct EfiGuid {
    pub(crate) data1: u32,
    pub(crate) data2: u16,
    pub(crate) data3: u16,
    pub(crate) data4: [u8; 8],
}

#[repr(C)]
pub(crate) struct EfiConfigurationTable {
    pub(crate) vendor_guid: EfiGuid,
    pub(crate) vendor_table: *mut c_void,
}

#[repr(C)]
struct EfiLoadedImageProtocol {
    _revision: u32,
    _parent_handle: *mut c_void,
    _system_table: *mut EfiSystemTable,
    device_handle: *mut c_void,
    _file_path: *mut c_void,
    _reserved: *mut c_void,
    _load_options_size: u32,
    _load_options: *mut c_void,
    _image_base: *mut c_void,
    _image_size: u64,
    _image_code_type: u32,
    _image_data_type: u32,
    _unload: usize,
}

#[repr(C)]
pub(crate) struct EfiMemoryDescriptor {
    pub(crate) memory_type: u32,
    pub(crate) physical_start: u64,
    pub(crate) virtual_start: u64,
    pub(crate) number_of_pages: u64,
    pub(crate) attribute: u64,
}

#[repr(C)]
pub(crate) struct EfiSimpleFileSystemProtocol {
    _revision: u64,
    pub(crate) open_volume: EfiOpenVolume,
}

pub(crate) type EfiOpenVolume = extern "efiapi" fn(
    this: *mut EfiSimpleFileSystemProtocol,
    root: *mut *mut EfiFileProtocol,
) -> EfiStatus;

#[repr(C)]
pub(crate) struct EfiFileProtocol {
    _revision: u64,
    pub(crate) open: EfiFileOpen,
    pub(crate) close: EfiFileClose,
    _delete: usize,
    pub(crate) read: EfiFileRead,
    _write: usize,
    _get_position: usize,
    _set_position: usize,
    _get_info: usize,
    _set_info: usize,
    _flush: usize,
}

pub(crate) type EfiFileOpen = extern "efiapi" fn(
    this: *mut EfiFileProtocol,
    new_handle: *mut *mut EfiFileProtocol,
    file_name: *const u16,
    open_mode: u64,
    attributes: u64,
) -> EfiStatus;

pub(crate) type EfiFileClose = extern "efiapi" fn(this: *mut EfiFileProtocol) -> EfiStatus;

pub(crate) type EfiFileRead = extern "efiapi" fn(
    this: *mut EfiFileProtocol,
    buffer_size: *mut usize,
    buffer: *mut c_void,
) -> EfiStatus;

pub(crate) fn boot_services(
    system_table: *mut EfiSystemTable,
) -> Result<*mut EfiBootServices, EfiStatus> {
    if system_table.is_null() {
        return Err(EFI_LOAD_ERROR);
    }

    // SAFETY:
    // 1. Invariant: `system_table` is the UEFI system table pointer passed to `efi_main`.
    // 2. Established by: UEFI firmware when invoking the EFI application entry point.
    // 3. Lifetime: valid until `ExitBootServices()`, which this slice does not call.
    // 4. Pointer ownership: firmware owns the table; the loader only reads it.
    // 5. Alignment: UEFI tables are naturally aligned by firmware.
    // 6. Mapped length: at least the system-table header and fixed fields read here.
    // 7. Concurrency: milestone 1 runs on one firmware-started CPU with no loader threads.
    // 8. Violation: a bogus pointer would make this read fault or fetch an invalid service table.
    let boot_services = unsafe { (*system_table).boot_services };
    if boot_services.is_null() {
        return Err(EFI_LOAD_ERROR);
    }
    Ok(boot_services)
}

pub(crate) fn configuration_tables(
    system_table: *mut EfiSystemTable,
) -> Result<(*const EfiConfigurationTable, usize), EfiStatus> {
    if system_table.is_null() {
        return Err(EFI_LOAD_ERROR);
    }

    // SAFETY:
    // 1. Invariant: `system_table` is the UEFI system table pointer passed to `efi_main`.
    // 2. Established by: UEFI firmware when invoking the EFI application entry point.
    // 3. Lifetime: valid until `ExitBootServices()`; callers only inspect entries before then.
    // 4. Pointer ownership: firmware owns the table and its entries.
    // 5. Alignment: UEFI tables are naturally aligned by firmware.
    // 6. Mapped length: fixed system-table fields through the configuration-table pointer.
    // 7. Concurrency: milestone 1.5 loader code is single-threaded.
    // 8. Violation: a bogus system table would fault or return invalid metadata.
    let (entries, table) = unsafe {
        (
            (*system_table)._number_of_table_entries,
            (*system_table)._configuration_table,
        )
    };
    if entries == 0 || table.is_null() {
        return Err(EFI_LOAD_ERROR);
    }
    Ok((table.cast(), entries))
}

pub(crate) fn locate_protocol<T>(
    boot_services: *mut EfiBootServices,
    guid: &EfiGuid,
) -> Result<*mut T, EfiStatus> {
    let mut interface: *mut c_void = ptr::null_mut();

    // SAFETY:
    // 1. Invariant: `boot_services` points to the active UEFI boot services table.
    // 2. Established by: `boot_services()` validated the system table field before use.
    // 3. Lifetime: valid until `ExitBootServices()`, which this slice does not call.
    // 4. Pointer ownership: firmware owns boot services and the returned protocol interface.
    // 5. Alignment: UEFI function table and protocol pointers are firmware-aligned.
    // 6. Mapped length: the boot services table includes `LocateProtocol`; `interface` is one pointer.
    // 7. Concurrency: no concurrent boot-service mutation is performed by this loader.
    // 8. Violation: an invalid table could call through a bad function pointer.
    let status =
        unsafe { ((*boot_services).locate_protocol)(guid, ptr::null_mut(), &mut interface) };

    if status != EFI_SUCCESS || interface.is_null() {
        return Err(status);
    }
    Ok(interface.cast())
}

pub(crate) fn handle_protocol<T>(
    boot_services: *mut EfiBootServices,
    handle: *mut c_void,
    guid: &EfiGuid,
) -> Result<*mut T, EfiStatus> {
    if handle.is_null() {
        return Err(EFI_INVALID_PARAMETER);
    }
    let mut interface: *mut c_void = ptr::null_mut();

    // SAFETY:
    // 1. Invariant: `boot_services` points to active UEFI boot services and
    //    `handle` is a firmware-provided EFI handle.
    // 2. Established by: `boot_services()` and the caller's UEFI entry
    //    arguments or protocol fields.
    // 3. Lifetime: valid until `ExitBootServices()`.
    // 4. Pointer ownership: firmware owns the handle and returned protocol.
    // 5. Alignment: firmware protocol pointers and output pointer are aligned.
    // 6. Mapped length: boot-services table includes `HandleProtocol`;
    //    `interface` is one pointer output.
    // 7. Concurrency: no concurrent protocol mutation is performed.
    // 8. Violation: invalid firmware tables could call through a bad pointer.
    let status = unsafe { ((*boot_services).handle_protocol)(handle, guid, &mut interface) };
    if status != EFI_SUCCESS || interface.is_null() {
        return Err(status);
    }
    Ok(interface.cast())
}

pub(crate) fn open_boot_volume(
    system_table: *mut EfiSystemTable,
    image_handle: *mut c_void,
) -> Result<*mut EfiFileProtocol, EfiStatus> {
    let boot_services = boot_services(system_table)?;
    let loaded_image: *mut EfiLoadedImageProtocol =
        handle_protocol(boot_services, image_handle, &LOADED_IMAGE_PROTOCOL_GUID)?;

    // SAFETY:
    // 1. Invariant: `loaded_image` is the EFI Loaded Image Protocol for the
    //    currently executing BOOTX64.EFI image.
    // 2. Established by: `HandleProtocol()` on the loader image handle.
    // 3. Lifetime: protocol remains valid until `ExitBootServices()`.
    // 4. Pointer ownership: firmware owns the protocol; loader reads one field.
    // 5. Alignment: firmware returns aligned protocol pointers.
    // 6. Mapped length: Loaded Image Protocol includes `DeviceHandle`.
    // 7. Concurrency: no concurrent protocol mutation is performed.
    // 8. Violation: invalid protocol layout would return a bad device handle.
    let device_handle = unsafe { (*loaded_image).device_handle };
    let filesystem: *mut EfiSimpleFileSystemProtocol =
        handle_protocol(boot_services, device_handle, &SIMPLE_FILE_SYSTEM_GUID)?;
    let mut root: *mut EfiFileProtocol = ptr::null_mut();

    // SAFETY:
    // 1. Invariant: `filesystem` is the Simple File System protocol attached
    //    to the loaded image's own device handle.
    // 2. Established by: `HandleProtocol()` on `LoadedImage.DeviceHandle`.
    // 3. Lifetime: valid until `ExitBootServices()`; returned root is closed by callers.
    // 4. Pointer ownership: firmware owns protocol internals; loader owns the open handle.
    // 5. Alignment: protocol pointer and `root` output pointer are aligned.
    // 6. Mapped length: protocol table contains `OpenVolume`; `root` is one pointer.
    // 7. Concurrency: no concurrent filesystem access is performed.
    // 8. Violation: invalid protocol pointer could call through a bad function pointer.
    let status = unsafe { ((*filesystem).open_volume)(filesystem, &mut root) };
    if status != EFI_SUCCESS || root.is_null() {
        return Err(status);
    }
    Ok(root)
}
