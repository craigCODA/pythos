//! Boot-information ABI validation at PythCore entry.

use pythos_shared::boot_protocol::PythBootInfo;

/// Read and validate the loader-provided `PythBootInfo`.
///
/// # Safety
///
/// `boot_info` must be the physical-address pointer the loader placed in
/// `RDI` per the kernel entry contract: it must either be null or point to
/// memory that stays mapped readable through the loader-built identity map
/// for at least `size_of::<PythBootInfo>()` bytes for the duration of early
/// core initialization.
pub unsafe fn validate(boot_info: *const PythBootInfo) -> Result<&'static PythBootInfo, ()> {
    if boot_info.is_null() || !boot_info.is_aligned() {
        return Err(());
    }
    // SAFETY:
    // 1. Invariant: `boot_info` is non-null, aligned, and points to at least
    //    `size_of::<PythBootInfo>()` readable mapped bytes.
    // 2. Established by: the caller's contract (loader handoff) plus the
    //    null and alignment checks above.
    // 3. Lifetime: the loader-built identity map keeps the structure mapped
    //    for all of early core initialization; nothing unmaps it earlier.
    // 4. Pointer ownership: PythCore owns all loader allocations after entry
    //    and only reads the structure.
    // 5. Alignment: verified by `is_aligned()` above.
    // 6. Mapped length: one full `PythBootInfo`, allocated by the loader
    //    with exactly this size.
    // 7. Concurrency: milestone 1 is single-core; no writers exist.
    // 8. Violation: an unmapped or short structure faults with no handler.
    let info = unsafe { &*boot_info };
    match info.validate() {
        Ok(()) => Ok(info),
        Err(_) => Err(()),
    }
}
