use crate::uefi::{self, EFI_INVALID_PARAMETER, EFI_SUCCESS, EfiStatus, EfiSystemTable};
use core::ffi::c_void;

pub(crate) enum ExitBootServicesResult {
    Exited,
    StaleMapKey,
    Failed,
}

pub(crate) fn exit_once(
    system_table: *mut EfiSystemTable,
    image_handle: *mut c_void,
    map_key: usize,
) -> ExitBootServicesResult {
    let Ok(boot_services) = uefi::boot_services(system_table) else {
        return ExitBootServicesResult::Failed;
    };

    // SAFETY:
    // 1. Invariant: `boot_services` points to the active UEFI boot services table.
    // 2. Established by: `uefi::boot_services()` from the firmware-provided system table.
    // 3. Lifetime: valid until this call succeeds; after success boot services are invalid.
    // 4. Pointer ownership: firmware owns boot services; `image_handle` is firmware-provided.
    // 5. Alignment: function table pointer and handle are firmware-provided.
    // 6. Mapped length: boot services table includes `ExitBootServices`.
    // 7. Concurrency: no concurrent boot-service calls are made.
    // 8. Violation: invalid table, handle, or key prevents exit or calls a bad function pointer.
    let status: EfiStatus = unsafe { ((*boot_services).exit_boot_services)(image_handle, map_key) };
    match status {
        EFI_SUCCESS => ExitBootServicesResult::Exited,
        EFI_INVALID_PARAMETER => ExitBootServicesResult::StaleMapKey,
        _ => ExitBootServicesResult::Failed,
    }
}
