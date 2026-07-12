use crate::uefi::{self, EFI_SUCCESS, EfiBootServices, EfiGuid, EfiStatus, EfiSystemTable};
use core::ptr;

const GOP_GUID: EfiGuid = EfiGuid {
    data1: 0x9042_A9DE,
    data2: 0x23DC,
    data3: 0x4A38,
    data4: [0x96, 0xFB, 0x7A, 0xDE, 0xD0, 0x80, 0x51, 0x6A],
};

const PIXEL_RED_GREEN_BLUE_RESERVED_8_BIT_PER_COLOR: u32 = 0;
const PIXEL_BLUE_GREEN_RED_RESERVED_8_BIT_PER_COLOR: u32 = 1;
const PIXEL_BIT_MASK: u32 = 2;
const PIXEL_BLT_ONLY: u32 = 3;
const PREFERRED_WIDTH: u32 = 1024;
const PREFERRED_HEIGHT: u32 = 768;

#[repr(C)]
struct EfiGraphicsOutputProtocol {
    query_mode: EfiGopQueryMode,
    set_mode: EfiGopSetMode,
    _blt: usize,
    mode: *mut EfiGraphicsOutputProtocolMode,
}

type EfiGopQueryMode = extern "efiapi" fn(
    this: *mut EfiGraphicsOutputProtocol,
    mode_number: u32,
    size_of_info: *mut usize,
    info: *mut *mut EfiGraphicsOutputModeInformation,
) -> EfiStatus;

type EfiGopSetMode =
    extern "efiapi" fn(this: *mut EfiGraphicsOutputProtocol, mode_number: u32) -> EfiStatus;

#[repr(C)]
struct EfiGraphicsOutputProtocolMode {
    max_mode: u32,
    mode: u32,
    info: *mut EfiGraphicsOutputModeInformation,
    size_of_info: usize,
    framebuffer_base: u64,
    framebuffer_size: usize,
}

#[repr(C)]
struct EfiGraphicsOutputModeInformation {
    _version: u32,
    horizontal_resolution: u32,
    vertical_resolution: u32,
    pixel_format: u32,
    _pixel_information: EfiPixelBitmask,
    pixels_per_scan_line: u32,
}

#[repr(C)]
struct EfiPixelBitmask {
    _red_mask: u32,
    _green_mask: u32,
    _blue_mask: u32,
    _reserved_mask: u32,
}

pub fn initialize_gop(system_table: *mut EfiSystemTable) -> Result<(), ()> {
    let boot_services = uefi::boot_services(system_table).map_err(|_| ())?;
    let gop = uefi::locate_protocol(boot_services, &GOP_GUID).map_err(|_| ())?;
    let selected = select_mode(boot_services, gop)?;
    validate_current_mode(gop, selected)
}

fn select_mode(
    boot_services: *mut EfiBootServices,
    gop: *mut EfiGraphicsOutputProtocol,
) -> Result<u32, ()> {
    let current = current_mode(gop)?;
    let max_mode = current.max_mode;
    let mut fallback = None;
    let mut mode_number = 0;

    while mode_number < max_mode {
        if let Some(info) = query_mode(gop, mode_number) {
            if is_direct_framebuffer_mode(info) {
                if is_preferred_mode(info) {
                    free_mode_info(boot_services, info);
                    return set_mode(gop, mode_number);
                }
                if fallback.is_none() {
                    fallback = Some(mode_number);
                }
            }
            free_mode_info(boot_services, info);
        }
        mode_number += 1;
    }

    fallback.map_or(Err(()), |mode| set_mode(gop, mode))
}

fn current_mode(
    gop: *mut EfiGraphicsOutputProtocol,
) -> Result<&'static mut EfiGraphicsOutputProtocolMode, ()> {
    if gop.is_null() {
        return Err(());
    }

    // SAFETY:
    // 1. Invariant: `gop` is the Graphics Output Protocol interface returned by firmware.
    // 2. Established by: successful `LocateProtocol()` for the GOP GUID.
    // 3. Lifetime: GOP remains valid until `ExitBootServices()`, which this slice does not call.
    // 4. Pointer ownership: firmware owns the protocol and mode structure; loader reads fields.
    // 5. Alignment: UEFI protocol structures are firmware-aligned.
    // 6. Mapped length: at least the GOP function table and `mode` pointer field.
    // 7. Concurrency: no other loader code mutates GOP concurrently.
    // 8. Violation: an invalid GOP pointer would make this read fault or observe garbage.
    let mode = unsafe { (*gop).mode };
    if mode.is_null() {
        return Err(());
    }

    // SAFETY:
    // 1. Invariant: `mode` points to the active GOP mode structure.
    // 2. Established by: firmware-populated GOP interface after `LocateProtocol()`.
    // 3. Lifetime: valid until mode change or `ExitBootServices()`; callers do not retain across calls.
    // 4. Pointer ownership: firmware owns the mode structure; loader reads and validates fields.
    // 5. Alignment: UEFI mode structures are firmware-aligned.
    // 6. Mapped length: at least `EfiGraphicsOutputProtocolMode`.
    // 7. Concurrency: no concurrent GOP mode changes are performed.
    // 8. Violation: an invalid mode pointer would make this reference unsound.
    Ok(unsafe { &mut *mode })
}

fn query_mode(
    gop: *mut EfiGraphicsOutputProtocol,
    mode_number: u32,
) -> Option<*mut EfiGraphicsOutputModeInformation> {
    let mut size_of_info = 0usize;
    let mut info: *mut EfiGraphicsOutputModeInformation = ptr::null_mut();

    // SAFETY:
    // 1. Invariant: `gop` is an active firmware-provided GOP interface.
    // 2. Established by: successful system-table and GOP discovery before this call.
    // 3. Lifetime: `QueryMode` is valid until `ExitBootServices()`; returned info is freed promptly.
    // 4. Pointer ownership: firmware allocates/owns service code; returned buffer is released via FreePool.
    // 5. Alignment: `size_of_info` and `info` are stack locals with valid alignment.
    // 6. Mapped length: `size_of_info` is one usize; `info` is one pointer output.
    // 7. Concurrency: no concurrent GOP calls are made.
    // 8. Violation: invalid interfaces could call through a bad function pointer or write bad outputs.
    let status = unsafe { ((*gop).query_mode)(gop, mode_number, &mut size_of_info, &mut info) };
    if status == EFI_SUCCESS
        && !info.is_null()
        && size_of_info >= core::mem::size_of::<EfiGraphicsOutputModeInformation>()
    {
        Some(info)
    } else {
        None
    }
}

fn free_mode_info(
    boot_services: *mut EfiBootServices,
    info: *mut EfiGraphicsOutputModeInformation,
) {
    if info.is_null() {
        return;
    }

    // SAFETY:
    // 1. Invariant: `info` is a buffer returned by GOP `QueryMode`.
    // 2. Established by: `query_mode()` only returns non-null successful `QueryMode` buffers.
    // 3. Lifetime: released immediately after inspecting the mode.
    // 4. Pointer ownership: ownership is returned to firmware via `FreePool`.
    // 5. Alignment: firmware returned the buffer with valid pool allocation alignment.
    // 6. Mapped length: the whole allocation returned by firmware is released.
    // 7. Concurrency: no other code keeps or uses this `info` pointer.
    // 8. Violation: freeing a non-pool pointer could corrupt firmware allocator state.
    let _ = unsafe { ((*boot_services).free_pool)(info.cast()) };
}

fn is_direct_framebuffer_mode(info: *const EfiGraphicsOutputModeInformation) -> bool {
    if info.is_null() {
        return false;
    }

    // SAFETY:
    // 1. Invariant: `info` points to a GOP mode information structure.
    // 2. Established by: successful `QueryMode()` and size validation.
    // 3. Lifetime: valid until the caller frees the query buffer after this check.
    // 4. Pointer ownership: firmware owns the allocation; loader only reads it.
    // 5. Alignment: firmware returned a properly aligned mode information pointer.
    // 6. Mapped length: at least `EfiGraphicsOutputModeInformation`.
    // 7. Concurrency: no concurrent access to the query buffer.
    // 8. Violation: invalid info would make these reads fault or classify garbage.
    let info = unsafe { &*info };
    matches!(
        info.pixel_format,
        PIXEL_RED_GREEN_BLUE_RESERVED_8_BIT_PER_COLOR
            | PIXEL_BLUE_GREEN_RED_RESERVED_8_BIT_PER_COLOR
            | PIXEL_BIT_MASK
    ) && info.pixel_format != PIXEL_BLT_ONLY
        && info.horizontal_resolution > 0
        && info.vertical_resolution > 0
        && info.pixels_per_scan_line >= info.horizontal_resolution
}

fn is_preferred_mode(info: *const EfiGraphicsOutputModeInformation) -> bool {
    if info.is_null() {
        return false;
    }

    // SAFETY:
    // 1. Invariant: `info` points to a GOP mode information structure.
    // 2. Established by: successful `QueryMode()` and size validation.
    // 3. Lifetime: valid until the caller frees the query buffer after this check.
    // 4. Pointer ownership: firmware owns the allocation; loader only reads it.
    // 5. Alignment: firmware returned a properly aligned mode information pointer.
    // 6. Mapped length: at least `EfiGraphicsOutputModeInformation`.
    // 7. Concurrency: no concurrent access to the query buffer.
    // 8. Violation: invalid info would make these reads fault or compare garbage.
    let info = unsafe { &*info };
    info.horizontal_resolution == PREFERRED_WIDTH && info.vertical_resolution == PREFERRED_HEIGHT
}

fn set_mode(gop: *mut EfiGraphicsOutputProtocol, mode_number: u32) -> Result<u32, ()> {
    // SAFETY:
    // 1. Invariant: `gop` is a valid Graphics Output Protocol interface.
    // 2. Established by: successful `LocateProtocol()` and mode enumeration.
    // 3. Lifetime: valid until `ExitBootServices()`, which this slice does not call.
    // 4. Pointer ownership: firmware owns GOP; loader requests a mode transition only.
    // 5. Alignment: firmware-provided protocol pointer is aligned.
    // 6. Mapped length: GOP function table includes `SetMode`.
    // 7. Concurrency: no concurrent framebuffer or GOP users exist in this loader slice.
    // 8. Violation: invalid `gop` could call through a bad function pointer.
    let status = unsafe { ((*gop).set_mode)(gop, mode_number) };
    if status == EFI_SUCCESS {
        Ok(mode_number)
    } else {
        Err(())
    }
}

fn validate_current_mode(
    gop: *mut EfiGraphicsOutputProtocol,
    selected_mode: u32,
) -> Result<(), ()> {
    let mode = current_mode(gop)?;
    if mode.mode != selected_mode
        || mode.info.is_null()
        || mode.framebuffer_base == 0
        || mode.framebuffer_size == 0
    {
        return Err(());
    }

    if !is_direct_framebuffer_mode(mode.info) {
        return Err(());
    }

    Ok(())
}
