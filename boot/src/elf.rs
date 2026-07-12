use crate::uefi::{
    self, EFI_ALLOCATE_ANY_PAGES, EFI_FILE_MODE_READ, EFI_LOADER_DATA, EFI_SUCCESS,
    EfiBootServices, EfiFileProtocol, EfiGuid, EfiPhysicalAddress, EfiSimpleFileSystemProtocol,
    EfiSystemTable,
};
use core::ffi::c_void;
use core::ptr;
use core::slice;

const SIMPLE_FILE_SYSTEM_GUID: EfiGuid = EfiGuid {
    data1: 0x964E_5B22,
    data2: 0x6459,
    data3: 0x11D2,
    data4: [0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B],
};

const PYTHCORE_PATH: &[u16] = &[
    b'\\' as u16,
    b'P' as u16,
    b'Y' as u16,
    b'T' as u16,
    b'H' as u16,
    b'O' as u16,
    b'S' as u16,
    b'\\' as u16,
    b'P' as u16,
    b'Y' as u16,
    b'T' as u16,
    b'H' as u16,
    b'C' as u16,
    b'O' as u16,
    b'R' as u16,
    b'E' as u16,
    b'.' as u16,
    b'E' as u16,
    b'L' as u16,
    b'F' as u16,
    0,
];

const MAX_KERNEL_FILE_SIZE: usize = 2 * 1024 * 1024;
const PAGE_SIZE: u64 = 4096;
const EI_CLASS_64: u8 = 2;
const EI_DATA_LITTLE_ENDIAN: u8 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 0x3E;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 0x1;
const PF_W: u32 = 0x2;
const PF_R: u32 = 0x4;
const KERNEL_VIRT_MIN: u64 = 0xFFFF_FFFF_8000_0000;
const MAX_LOAD_SEGMENTS: usize = 16;

#[derive(Clone, Copy)]
struct LoadSegment {
    offset: u64,
    vaddr: u64,
    file_size: u64,
    memory_size: u64,
    flags: u32,
    align: u64,
}

const EMPTY_SEGMENT: LoadSegment = LoadSegment {
    offset: 0,
    vaddr: 0,
    file_size: 0,
    memory_size: 0,
    flags: 0,
    align: 0,
};

struct ElfImage {
    entry: u64,
    segments: [LoadSegment; MAX_LOAD_SEGMENTS],
    segment_count: usize,
}

struct LoadedFile {
    ptr: *mut u8,
    len: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct LoadedSegment {
    pub(crate) physical_start: u64,
    pub(crate) virtual_start: u64,
    pub(crate) page_count: usize,
    pub(crate) file_size: u64,
    pub(crate) memory_size: u64,
    pub(crate) permissions: SegmentPermissions,
}

const EMPTY_LOADED_SEGMENT: LoadedSegment = LoadedSegment {
    physical_start: 0,
    virtual_start: 0,
    page_count: 0,
    file_size: 0,
    memory_size: 0,
    permissions: SegmentPermissions {
        readable: false,
        writable: false,
        executable: false,
    },
};

#[derive(Clone, Copy)]
pub(crate) struct SegmentPermissions {
    pub(crate) readable: bool,
    pub(crate) writable: bool,
    pub(crate) executable: bool,
}

pub(crate) struct LoadedKernel {
    pub(crate) entry: u64,
    pub(crate) segments: [LoadedSegment; MAX_LOAD_SEGMENTS],
    pub(crate) segment_count: usize,
    pub(crate) physical_start: u64,
    pub(crate) physical_end: u64,
    pub(crate) virtual_start: u64,
    pub(crate) virtual_end: u64,
}

impl LoadedKernel {
    pub(crate) fn is_well_formed(&self) -> bool {
        self.segment_count > 0
            && self.segment_count <= MAX_LOAD_SEGMENTS
            && self.entry >= self.virtual_start
            && self.entry < self.virtual_end
            && self.physical_start < self.physical_end
            && self.virtual_start < self.virtual_end
            && self
                .segments
                .iter()
                .take(self.segment_count)
                .all(LoadedSegment::is_well_formed)
    }
}

impl LoadedSegment {
    fn is_well_formed(&self) -> bool {
        self.physical_start != 0
            && self.virtual_start >= KERNEL_VIRT_MIN
            && self.page_count > 0
            && self.memory_size > 0
            && self.file_size <= self.memory_size
            && !(self.permissions.writable && self.permissions.executable)
            && (self.permissions.readable
                || self.permissions.writable
                || self.permissions.executable)
    }
}

pub fn load_pythcore(system_table: *mut EfiSystemTable) -> Result<LoadedKernel, ()> {
    let boot_services = uefi::boot_services(system_table).map_err(|_| ())?;
    let file = read_kernel_file(boot_services)?;

    // SAFETY:
    // 1. Invariant: `file.ptr` points to an allocated loader buffer containing `file.len` bytes.
    // 2. Established by: `read_kernel_file()` allocates pool memory and UEFI `Read` fills it.
    // 3. Lifetime: valid until this function releases the pool with `FreePool`.
    // 4. Pointer ownership: the loader owns the pool allocation until it calls `FreePool`.
    // 5. Alignment: byte slices require only 1-byte alignment.
    // 6. Mapped length: exactly `file.len` bytes were reported by UEFI `Read`.
    // 7. Concurrency: no other loader code accesses this file buffer.
    // 8. Violation: an invalid pointer or length would make slice creation unsound.
    let bytes = unsafe { slice::from_raw_parts(file.ptr.cast_const(), file.len) };

    let result = validate_elf(bytes).and_then(|image| load_segments(boot_services, bytes, &image));
    free_pool(boot_services, file.ptr.cast());
    result
}

fn read_kernel_file(boot_services: *mut EfiBootServices) -> Result<LoadedFile, ()> {
    let filesystem: *mut EfiSimpleFileSystemProtocol =
        uefi::locate_protocol(boot_services, &SIMPLE_FILE_SYSTEM_GUID).map_err(|_| ())?;
    let mut root: *mut EfiFileProtocol = ptr::null_mut();

    // SAFETY:
    // 1. Invariant: `filesystem` is the Simple File System protocol returned by firmware.
    // 2. Established by: successful `LocateProtocol()` for the Simple File System GUID.
    // 3. Lifetime: valid until `ExitBootServices()`, which this slice does not call.
    // 4. Pointer ownership: firmware owns the protocol; the opened root handle is closed below.
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
    // 6. Mapped length: `PYTHCORE_PATH` is a null-terminated UTF-16 path; `file` is one pointer output.
    // 7. Concurrency: no concurrent file operations are issued.
    // 8. Violation: invalid handles or path pointers could make firmware read invalid memory.
    let status = unsafe {
        ((*root).open)(
            root,
            &mut file,
            PYTHCORE_PATH.as_ptr(),
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
    // 6. Mapped length: `MAX_KERNEL_FILE_SIZE` bytes are requested.
    // 7. Concurrency: no concurrent pool allocation users in this loader slice.
    // 8. Violation: invalid boot services table could call through a bad function pointer.
    let status = unsafe {
        ((*boot_services).allocate_pool)(EFI_LOADER_DATA, MAX_KERNEL_FILE_SIZE, &mut buffer)
    };
    if status != EFI_SUCCESS || buffer.is_null() {
        close_file(file);
        close_file(root);
        return Err(());
    }

    let mut bytes_read = MAX_KERNEL_FILE_SIZE;
    // SAFETY:
    // 1. Invariant: `file` is an open EFI file handle and `buffer` has `MAX_KERNEL_FILE_SIZE` bytes.
    // 2. Established by: successful file open and pool allocation above.
    // 3. Lifetime: handle and buffer remain valid for this call.
    // 4. Pointer ownership: firmware writes into the loader-owned buffer.
    // 5. Alignment: `bytes_read` is aligned for usize; byte buffer alignment is sufficient.
    // 6. Mapped length: buffer length is exactly `MAX_KERNEL_FILE_SIZE`.
    // 7. Concurrency: no concurrent reads or buffer accesses.
    // 8. Violation: an invalid handle or buffer would let firmware write invalid memory.
    let status = unsafe { ((*file).read)(file, &mut bytes_read, buffer) };
    close_file(file);
    close_file(root);

    if status != EFI_SUCCESS || bytes_read == 0 || bytes_read == MAX_KERNEL_FILE_SIZE {
        free_pool(boot_services, buffer);
        return Err(());
    }

    Ok(LoadedFile {
        ptr: buffer.cast(),
        len: bytes_read,
    })
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

fn validate_elf(bytes: &[u8]) -> Result<ElfImage, ()> {
    if bytes.len() < 64 || &bytes[0..4] != b"\x7FELF" {
        return Err(());
    }
    if bytes[4] != EI_CLASS_64 || bytes[5] != EI_DATA_LITTLE_ENDIAN || bytes[6] != 1 {
        return Err(());
    }

    let elf_type = read_u16(bytes, 16)?;
    let machine = read_u16(bytes, 18)?;
    if elf_type != ET_EXEC || machine != EM_X86_64 {
        return Err(());
    }

    let entry = read_u64(bytes, 24)?;
    let phoff = read_u64(bytes, 32)?;
    let phentsize = read_u16(bytes, 54)?;
    let phnum = read_u16(bytes, 56)?;
    if phentsize as usize != 56 || phnum == 0 {
        return Err(());
    }
    let ph_table_size = (phentsize as usize).checked_mul(phnum as usize).ok_or(())?;
    let ph_table_end = (phoff as usize).checked_add(ph_table_size).ok_or(())?;
    if ph_table_end > bytes.len() {
        return Err(());
    }

    let mut image = ElfImage {
        entry,
        segments: [EMPTY_SEGMENT; MAX_LOAD_SEGMENTS],
        segment_count: 0,
    };

    for index in 0..phnum as usize {
        let off = phoff as usize + index * phentsize as usize;
        let segment_type = read_u32(bytes, off)?;
        if segment_type != PT_LOAD {
            continue;
        }
        if image.segment_count == MAX_LOAD_SEGMENTS {
            return Err(());
        }

        let segment = LoadSegment {
            flags: read_u32(bytes, off + 4)?,
            offset: read_u64(bytes, off + 8)?,
            vaddr: read_u64(bytes, off + 16)?,
            file_size: read_u64(bytes, off + 32)?,
            memory_size: read_u64(bytes, off + 40)?,
            align: read_u64(bytes, off + 48)?,
        };
        validate_segment(bytes, &image, &segment)?;
        image.segments[image.segment_count] = segment;
        image.segment_count += 1;
    }

    if image.segment_count == 0 || !entry_is_executable(&image) {
        return Err(());
    }
    Ok(image)
}

fn validate_segment(bytes: &[u8], image: &ElfImage, segment: &LoadSegment) -> Result<(), ()> {
    if segment.file_size > segment.memory_size || segment.memory_size == 0 {
        return Err(());
    }
    if (segment.flags & (PF_W | PF_X)) == (PF_W | PF_X) {
        return Err(());
    }
    if segment.align == 0 || !segment.align.is_power_of_two() {
        return Err(());
    }
    if segment.vaddr < KERNEL_VIRT_MIN {
        return Err(());
    }
    let vend = segment.vaddr.checked_add(segment.memory_size).ok_or(())?;
    let fend = segment.offset.checked_add(segment.file_size).ok_or(())?;
    if fend as usize > bytes.len() || (segment.offset as usize) > bytes.len() {
        return Err(());
    }
    if segment.vaddr % segment.align != segment.offset % segment.align {
        return Err(());
    }

    for existing in image.segments.iter().take(image.segment_count) {
        let existing_end = existing.vaddr.checked_add(existing.memory_size).ok_or(())?;
        if segment.vaddr < existing_end && existing.vaddr < vend {
            return Err(());
        }
    }
    Ok(())
}

fn entry_is_executable(image: &ElfImage) -> bool {
    image
        .segments
        .iter()
        .take(image.segment_count)
        .any(|segment| {
            let Some(end) = segment.vaddr.checked_add(segment.memory_size) else {
                return false;
            };
            (segment.flags & PF_X) != 0 && image.entry >= segment.vaddr && image.entry < end
        })
}

fn load_segments(
    boot_services: *mut EfiBootServices,
    bytes: &[u8],
    image: &ElfImage,
) -> Result<LoadedKernel, ()> {
    let mut loaded = LoadedKernel {
        entry: image.entry,
        segments: [EMPTY_LOADED_SEGMENT; MAX_LOAD_SEGMENTS],
        segment_count: 0,
        physical_start: u64::MAX,
        physical_end: 0,
        virtual_start: u64::MAX,
        virtual_end: 0,
    };

    for segment in image.segments.iter().take(image.segment_count) {
        let pages = page_count(segment.memory_size)?;
        let mut physical: EfiPhysicalAddress = 0;
        // SAFETY:
        // 1. Invariant: `boot_services` points to the active UEFI boot services table.
        // 2. Established by: `uefi::boot_services()` before ELF loading began.
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
            return Err(());
        }

        copy_segment(bytes, segment, physical, pages)?;
        let physical_end = physical
            .checked_add((pages as u64).checked_mul(PAGE_SIZE).ok_or(())?)
            .ok_or(())?;
        let virtual_end = segment.vaddr.checked_add(segment.memory_size).ok_or(())?;

        loaded.physical_start = loaded.physical_start.min(physical);
        loaded.physical_end = loaded.physical_end.max(physical_end);
        loaded.virtual_start = loaded.virtual_start.min(segment.vaddr);
        loaded.virtual_end = loaded.virtual_end.max(virtual_end);
        loaded.segments[loaded.segment_count] = LoadedSegment {
            physical_start: physical,
            virtual_start: segment.vaddr,
            page_count: pages,
            file_size: segment.file_size,
            memory_size: segment.memory_size,
            permissions: SegmentPermissions {
                readable: (segment.flags & PF_R) != 0,
                writable: (segment.flags & PF_W) != 0,
                executable: (segment.flags & PF_X) != 0,
            },
        };
        loaded.segment_count += 1;
    }

    if loaded.is_well_formed() {
        Ok(loaded)
    } else {
        Err(())
    }
}

fn copy_segment(
    bytes: &[u8],
    segment: &LoadSegment,
    physical: EfiPhysicalAddress,
    pages: usize,
) -> Result<(), ()> {
    let total_len = pages.checked_mul(PAGE_SIZE as usize).ok_or(())?;
    let src_offset = segment.offset as usize;
    let file_size = segment.file_size as usize;
    let memory_size = segment.memory_size as usize;
    let src_end = src_offset.checked_add(file_size).ok_or(())?;
    if src_end > bytes.len() || memory_size > total_len {
        return Err(());
    }

    // SAFETY:
    // 1. Invariant: `physical` is the base of `pages` pages returned by UEFI `AllocatePages`.
    // 2. Established by: successful `AllocatePages` immediately before this copy.
    // 3. Lifetime: allocated pages remain valid through the loader and future handoff.
    // 4. Pointer ownership: loader owns the destination pages; source bytes are immutable file buffer.
    // 5. Alignment: `physical` is 4 KiB-aligned; byte writes require only 1-byte alignment.
    // 6. Mapped length: destination has `total_len` bytes; source has `file_size` bytes.
    // 7. Concurrency: no concurrent access to destination pages or source buffer.
    // 8. Violation: incorrect identity mapping or length checks would corrupt memory.
    unsafe {
        let dst = physical as *mut u8;
        ptr::write_bytes(dst, 0, total_len);
        ptr::copy_nonoverlapping(bytes.as_ptr().add(src_offset), dst, file_size);
    }
    Ok(())
}

fn page_count(size: u64) -> Result<usize, ()> {
    let rounded = size.checked_add(PAGE_SIZE - 1).ok_or(())? / PAGE_SIZE;
    usize::try_from(rounded).map_err(|_| ())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ()> {
    let end = offset.checked_add(2).ok_or(())?;
    let raw = bytes.get(offset..end).ok_or(())?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ()> {
    let end = offset.checked_add(4).ok_or(())?;
    let raw = bytes.get(offset..end).ok_or(())?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, ()> {
    let end = offset.checked_add(8).ok_or(())?;
    let raw = bytes.get(offset..end).ok_or(())?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}
