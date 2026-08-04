use crate::boot_info::EvidenceLogMetadata;
use crate::uefi::{
    self, EFI_ALLOCATE_ANY_PAGES, EFI_LOADER_DATA, EFI_SUCCESS, EfiPhysicalAddress, EfiSystemTable,
};
use pythos_shared::evidence_log::{self, EVIDENCE_LOG_TOTAL_BYTES};

const PAGE_SIZE: usize = 4096;
const EVIDENCE_LOG_PAGES: usize = EVIDENCE_LOG_TOTAL_BYTES / PAGE_SIZE;

pub(crate) struct AllocatedEvidenceLog {
    physical_start: u64,
    len: u32,
    ptr: *mut u8,
}

impl AllocatedEvidenceLog {
    pub(crate) fn allocate(system_table: *mut EfiSystemTable) -> Result<Self, ()> {
        let boot_services = uefi::boot_services(system_table).map_err(|_| ())?;
        let mut physical_start: EfiPhysicalAddress = 0;
        // SAFETY:
        // 1. Invariant: `boot_services` points to the active UEFI boot services
        //    table and `physical_start` is a valid out-parameter.
        // 2. Established by: `uefi::boot_services()` from the firmware system table.
        // 3. Lifetime: allocation remains owned by the loader until PythCore handoff.
        // 4. Pointer ownership: firmware writes only the output physical address.
        // 5. Alignment: UEFI page allocation returns 4 KiB-aligned memory.
        // 6. Mapped length: exactly `EVIDENCE_LOG_PAGES * 4096` bytes.
        // 7. Concurrency: single-threaded loader.
        // 8. Violation: invalid boot services would call through a bad pointer.
        let status = unsafe {
            ((*boot_services).allocate_pages)(
                EFI_ALLOCATE_ANY_PAGES,
                EFI_LOADER_DATA,
                EVIDENCE_LOG_PAGES,
                &mut physical_start,
            )
        };
        if status != EFI_SUCCESS || physical_start == 0 {
            return Err(());
        }
        let ptr = physical_start as *mut u8;
        // SAFETY:
        // 1. Invariant: `ptr` is the page-aligned allocation returned above.
        // 2. Established by: successful UEFI `AllocatePages`.
        // 3. Lifetime: retained until PythCore handoff.
        // 4. Pointer ownership: loader owns the allocation.
        // 5. Alignment: UEFI page allocation provides page alignment.
        // 6. Mapped length: `EVIDENCE_LOG_TOTAL_BYTES`.
        // 7. Concurrency: single-threaded loader.
        // 8. Violation: a wrong length could corrupt adjacent loader memory.
        let buffer = unsafe { core::slice::from_raw_parts_mut(ptr, EVIDENCE_LOG_TOTAL_BYTES) };
        evidence_log::initialize(buffer).map_err(|_| ())?;
        Ok(Self {
            physical_start,
            len: EVIDENCE_LOG_TOTAL_BYTES as u32,
            ptr,
        })
    }

    pub(crate) const fn metadata(&self) -> EvidenceLogMetadata {
        EvidenceLogMetadata {
            physical_start: self.physical_start,
            len: self.len,
        }
    }

    #[cfg(test)]
    pub(crate) const fn for_test(physical_start: u64, len: u32) -> Self {
        Self {
            physical_start,
            len,
            ptr: core::ptr::null_mut(),
        }
    }

    pub(crate) fn append_line(&mut self, marker: &str) -> Result<(), ()> {
        // SAFETY:
        // 1. Invariant: `self.ptr` points to the 64 KiB evidence buffer owned
        //    by this allocation.
        // 2. Established by: `AllocatedEvidenceLog::allocate`.
        // 3. Lifetime: retained until PythCore handoff.
        // 4. Pointer ownership: loader has exclusive mutable access before handoff.
        // 5. Alignment: UEFI page allocation provides page alignment.
        // 6. Mapped length: `EVIDENCE_LOG_TOTAL_BYTES`.
        // 7. Concurrency: single-threaded loader.
        // 8. Violation: a stale pointer would corrupt memory or fault.
        let buffer = unsafe { core::slice::from_raw_parts_mut(self.ptr, EVIDENCE_LOG_TOTAL_BYTES) };
        evidence_log::append_line(buffer, marker).map_err(|_| ())
    }
}

pub(crate) fn write_marker(log: &mut Option<AllocatedEvidenceLog>, marker: &str) {
    crate::serial::write_line(marker);
    if let Some(log) = log.as_mut() {
        let _ = log.append_line(marker);
    }
}
