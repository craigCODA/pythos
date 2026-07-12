use crate::uefi::{
    self, EFI_ALLOCATE_ANY_PAGES, EFI_FILE_MODE_READ, EFI_LOADER_DATA, EFI_SUCCESS,
    EfiBootServices, EfiFileProtocol, EfiGuid, EfiPhysicalAddress, EfiSimpleFileSystemProtocol,
    EfiSystemTable,
};
use core::ffi::c_void;
use core::ptr;

const SIMPLE_FILE_SYSTEM_GUID: EfiGuid = EfiGuid {
    data1: 0x964E_5B22,
    data2: 0x6459,
    data3: 0x11D2,
    data4: [0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B],
};

const INIT_PAK_PATH: &[u16] = &[
    b'\\' as u16,
    b'P' as u16,
    b'Y' as u16,
    b'T' as u16,
    b'H' as u16,
    b'O' as u16,
    b'S' as u16,
    b'\\' as u16,
    b'I' as u16,
    b'N' as u16,
    b'I' as u16,
    b'T' as u16,
    b'.' as u16,
    b'P' as u16,
    b'A' as u16,
    b'K' as u16,
    0,
];

const MAX_INIT_PAK_SIZE: usize = 1024 * 1024;
const PAGE_SIZE: usize = 4096;

pub(crate) struct LoadedInitBundle {
    pub(crate) physical_start: u64,
    pub(crate) len: u64,
    pub(crate) page_count: usize,
}

impl LoadedInitBundle {
    pub(crate) fn is_loaded(&self) -> bool {
        self.physical_start != 0 && self.len > 0 && self.page_count > 0
    }
}

struct LoadedFile {
    ptr: *mut u8,
    len: usize,
}

pub(crate) fn load_init_pak(system_table: *mut EfiSystemTable) -> Result<LoadedInitBundle, ()> {
    let boot_services = uefi::boot_services(system_table).map_err(|_| ())?;
    let file = read_init_file(boot_services)?;
    let pages = page_count(file.len)?;
    let mut physical: EfiPhysicalAddress = 0;

    // SAFETY:
    // 1. Invariant: `boot_services` points to the active UEFI boot services table.
    // 2. Established by: `uefi::boot_services()` before init bundle loading began.
    // 3. Lifetime: allocated pages stay owned by the loader for later PythCore handoff.
    // 4. Pointer ownership: firmware transfers ownership of allocated physical pages to loader.
    // 5. Alignment: `AllocatePages` returns 4 KiB-aligned physical pages.
    // 6. Mapped length: `pages * PAGE_SIZE` bytes are allocated.
    // 7. Concurrency: no concurrent page allocation users in this loader slice.
    // 8. Violation: invalid boot services table could call through a bad function pointer.
    let status = unsafe {
        ((*boot_services).allocate_pages)(
            EFI_ALLOCATE_ANY_PAGES,
            EFI_LOADER_DATA,
            pages,
            &mut physical,
        )
    };
    if status != EFI_SUCCESS || physical == 0 {
        free_pool(boot_services, file.ptr.cast());
        return Err(());
    }

    let total_len = pages.checked_mul(PAGE_SIZE).ok_or(())?;
    // SAFETY:
    // 1. Invariant: `physical` is the base of `pages` pages returned by UEFI `AllocatePages`.
    // 2. Established by: successful `AllocatePages` immediately before this copy.
    // 3. Lifetime: allocated pages remain valid through the loader and future handoff.
    // 4. Pointer ownership: loader owns destination pages and source pool buffer.
    // 5. Alignment: `physical` is 4 KiB-aligned; byte writes require only 1-byte alignment.
    // 6. Mapped length: destination has `total_len` bytes; source has `file.len` bytes.
    // 7. Concurrency: no concurrent access to destination pages or source buffer.
    // 8. Violation: incorrect identity mapping or length checks would corrupt memory.
    unsafe {
        let dst = physical as *mut u8;
        ptr::write_bytes(dst, 0, total_len);
        ptr::copy_nonoverlapping(file.ptr, dst, file.len);
    }

    let loaded = LoadedInitBundle {
        physical_start: physical,
        len: file.len as u64,
        page_count: pages,
    };
    free_pool(boot_services, file.ptr.cast());
    if loaded.is_loaded() {
        Ok(loaded)
    } else {
        Err(())
    }
}

fn read_init_file(boot_services: *mut EfiBootServices) -> Result<LoadedFile, ()> {
    let filesystem: *mut EfiSimpleFileSystemProtocol =
        uefi::locate_protocol(boot_services, &SIMPLE_FILE_SYSTEM_GUID).map_err(|_| ())?;
    let mut root: *mut EfiFileProtocol = ptr::null_mut();

    // SAFETY:
    // 1. Invariant: `filesystem` is the Simple File System protocol returned by firmware.
    // 2. Established by: successful `LocateProtocol()` for the Simple File System GUID.
    // 3. Lifetime: valid until `ExitBootServices()`, which this slice does not call.
    // 4. Pointer ownership: firmware owns the protocol; opened root handle is closed below.
    // 5. Alignment: firmware returns aligned protocol pointers; `root` is an aligned output pointer.
    // 6. Mapped length: protocol table contains `OpenVolume`; `root` is one pointer output.
    // 7. Concurrency: no concurrent filesystem protocol use by this loader.
    // 8. Violation: invalid protocol pointer could call through a bad function pointer.
    let status = unsafe { ((*filesystem).open_volume)(filesystem, &mut root) };
    if status != EFI_SUCCESS || root.is_null() {
        return Err(());
    }

    let mut file: *mut EfiFileProtocol = ptr::null_mut();
    // SAFETY:
    // 1. Invariant: `root` is an open EFI file handle for the ESP root directory.
    // 2. Established by: successful `OpenVolume()` above.
    // 3. Lifetime: valid until closed by `Close`.
    // 4. Pointer ownership: firmware owns handle internals; loader receives and closes `file`.
    // 5. Alignment: UEFI file handles and output pointers are naturally aligned.
    // 6. Mapped length: `INIT_PAK_PATH` is a null-terminated UTF-16 path; `file` is one pointer output.
    // 7. Concurrency: no concurrent file operations are issued.
    // 8. Violation: invalid handles or path pointers could make firmware read invalid memory.
    let status = unsafe {
        ((*root).open)(
            root,
            &mut file,
            INIT_PAK_PATH.as_ptr(),
            EFI_FILE_MODE_READ,
            0,
        )
    };
    if status != EFI_SUCCESS || file.is_null() {
        close_file(root);
        return Err(());
    }

    let mut buffer: *mut c_void = ptr::null_mut();
    // SAFETY:
    // 1. Invariant: `boot_services` points to the active UEFI boot services table.
    // 2. Established by: `uefi::boot_services()` before this function was called.
    // 3. Lifetime: allocation is valid until explicitly released with `FreePool`.
    // 4. Pointer ownership: loader owns the pool allocation returned through `buffer`.
    // 5. Alignment: firmware pool allocations satisfy general-purpose alignment.
    // 6. Mapped length: `MAX_INIT_PAK_SIZE` bytes are requested.
    // 7. Concurrency: no concurrent pool allocation users in this loader slice.
    // 8. Violation: invalid boot services table could call through a bad function pointer.
    let status = unsafe {
        ((*boot_services).allocate_pool)(EFI_LOADER_DATA, MAX_INIT_PAK_SIZE, &mut buffer)
    };
    if status != EFI_SUCCESS || buffer.is_null() {
        close_file(file);
        close_file(root);
        return Err(());
    }

    let mut bytes_read = MAX_INIT_PAK_SIZE;
    // SAFETY:
    // 1. Invariant: `file` is an open EFI file handle and `buffer` has `MAX_INIT_PAK_SIZE` bytes.
    // 2. Established by: successful file open and pool allocation above.
    // 3. Lifetime: handle and buffer remain valid for this call.
    // 4. Pointer ownership: firmware writes into the loader-owned buffer.
    // 5. Alignment: `bytes_read` is aligned for usize; byte buffer alignment is sufficient.
    // 6. Mapped length: buffer length is exactly `MAX_INIT_PAK_SIZE`.
    // 7. Concurrency: no concurrent reads or buffer accesses.
    // 8. Violation: an invalid handle or buffer would let firmware write invalid memory.
    let status = unsafe { ((*file).read)(file, &mut bytes_read, buffer) };
    close_file(file);
    close_file(root);

    if status != EFI_SUCCESS || bytes_read == 0 || bytes_read == MAX_INIT_PAK_SIZE {
        free_pool(boot_services, buffer);
        return Err(());
    }

    Ok(LoadedFile {
        ptr: buffer.cast(),
        len: bytes_read,
    })
}

fn page_count(size: usize) -> Result<usize, ()> {
    size.checked_add(PAGE_SIZE - 1)
        .map(|rounded| rounded / PAGE_SIZE)
        .ok_or(())
}

fn close_file(file: *mut EfiFileProtocol) {
    if file.is_null() {
        return;
    }
    // SAFETY:
    // 1. Invariant: `file` is an EFI file handle returned by `OpenVolume` or `Open`.
    // 2. Established by: callers only pass non-null handles from successful UEFI file calls.
    // 3. Lifetime: handle is valid until this close call returns.
    // 4. Pointer ownership: firmware owns handle internals; loader releases the handle.
    // 5. Alignment: firmware file handles are aligned.
    // 6. Mapped length: file protocol table contains `Close`.
    // 7. Concurrency: no concurrent operations use this handle.
    // 8. Violation: closing an invalid handle could corrupt firmware file state.
    let _ = unsafe { ((*file).close)(file) };
}

fn free_pool(boot_services: *mut EfiBootServices, buffer: *mut c_void) {
    if buffer.is_null() {
        return;
    }
    // SAFETY:
    // 1. Invariant: `buffer` is a pool allocation returned by UEFI `AllocatePool`.
    // 2. Established by: callers only pass successful pool allocations from this loader.
    // 3. Lifetime: allocation is no longer used after this call.
    // 4. Pointer ownership: ownership returns from loader to firmware.
    // 5. Alignment: firmware returned the buffer with valid pool allocation alignment.
    // 6. Mapped length: the whole pool allocation is released.
    // 7. Concurrency: no other loader code accesses the buffer.
    // 8. Violation: freeing a non-pool pointer could corrupt firmware allocator state.
    let _ = unsafe { ((*boot_services).free_pool)(buffer) };
}
