use core::convert::TryFrom;

use pythos_shared::boot_protocol::{PYTH_EVIDENCE_LOG_FLAG_PRESENT, PythBootInfo};
use pythos_shared::evidence_log::{self, EvidenceLogError};

#[cfg(test)]
extern crate std;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceLogAttachError {
    Absent,
    BadLength,
    InvalidLog(EvidenceLogError),
}

#[derive(Clone, Copy)]
struct InstalledLog {
    ptr: *mut u8,
    len: usize,
}

impl InstalledLog {
    fn from_buffer(buffer: &mut [u8]) -> Self {
        Self {
            ptr: buffer.as_mut_ptr(),
            len: buffer.len(),
        }
    }
}

#[cfg(not(test))]
static mut INSTALLED_LOG: Option<InstalledLog> = None;

#[cfg(test)]
std::thread_local! {
    static TEST_INSTALLED_LOG: std::cell::RefCell<Option<InstalledLog>> =
        const { std::cell::RefCell::new(None) };
}

pub fn attach_from_boot_info(boot_info: &PythBootInfo) -> Result<(), EvidenceLogAttachError> {
    if boot_info.evidence_log_flags & PYTH_EVIDENCE_LOG_FLAG_PRESENT == 0 {
        return Err(EvidenceLogAttachError::Absent);
    }
    let len = usize::try_from(boot_info.evidence_log_len)
        .map_err(|_| EvidenceLogAttachError::BadLength)?;
    let ptr = boot_info.evidence_log_phys as *mut u8;
    // SAFETY:
    // 1. Invariant: `ptr..ptr+len` is the 64 KiB loader-owned evidence buffer
    //    advertised by validated ABI 0.3 boot info.
    // 2. Established by: `PythBootInfo::validate` checked alignment, flags,
    //    and exact length before this function is called in the verify path.
    // 3. Lifetime: loader allocations are retained for this boot proof.
    // 4. Pointer ownership: PythCore owns the allocation after handoff.
    // 5. Alignment: page alignment is validated by the boot protocol.
    // 6. Mapped length: exactly `len` bytes.
    // 7. Concurrency: single-core verification path.
    // 8. Violation: a bad pointer corrupts memory or faults before rendering.
    let buffer = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
    install(buffer)
}

pub fn append_marker(marker: &str) {
    let _ = with_installed_buffer_mut(|buffer| {
        let _ = evidence_log::append_line(buffer, marker);
    });
}

fn install(buffer: &mut [u8]) -> Result<(), EvidenceLogAttachError> {
    evidence_log::snapshot(buffer).map_err(EvidenceLogAttachError::InvalidLog)?;
    let installed = InstalledLog::from_buffer(buffer);

    #[cfg(test)]
    TEST_INSTALLED_LOG.with(|state| {
        *state.borrow_mut() = Some(installed);
    });

    #[cfg(not(test))]
    // SAFETY:
    // 1. Invariant: `INSTALLED_LOG` is the single-core global evidence-log
    //    attachment state for the current boot.
    // 2. Established by: the verify path installs at most one loader-owned
    //    buffer and later appends only through this module.
    // 3. Lifetime: the installed pointer remains valid for the boot proof.
    // 4. Pointer ownership: the raw pointer refers to the retained loader
    //    allocation now owned by PythCore.
    // 5. Alignment: the buffer alignment was validated before installation.
    // 6. Mapped length: `installed.len` bytes.
    // 7. Concurrency: single-core verification path.
    // 8. Violation: overwriting with an invalid pointer corrupts later writes.
    unsafe {
        core::ptr::addr_of_mut!(INSTALLED_LOG).write(Some(installed));
    }

    Ok(())
}

fn with_installed_buffer_mut<R>(f: impl FnOnce(&mut [u8]) -> R) -> Option<R> {
    #[cfg(test)]
    {
        return TEST_INSTALLED_LOG.with(|state| {
            let mut state = state.borrow_mut();
            let installed = state.as_mut()?;
            // SAFETY:
            // 1. Invariant: `installed.ptr..ptr+len` names the test-local
            //    evidence buffer previously validated and installed.
            // 2. Established by: `install_for_test` or `attach_from_boot_info`
            //    storing only buffers that passed shared-format validation.
            // 3. Lifetime: the test retains the backing aligned buffer for the
            //    duration of this callback.
            // 4. Pointer ownership: the test owns the backing buffer and this
            //    module has exclusive mutable access during the callback.
            // 5. Alignment: byte-slice access has no stronger alignment need.
            // 6. Mapped length: exactly `installed.len` bytes.
            // 7. Concurrency: thread-local state isolates host test threads.
            // 8. Violation: a stale pointer corrupts memory or panics in tests.
            let buffer = unsafe { core::slice::from_raw_parts_mut(installed.ptr, installed.len) };
            Some(f(buffer))
        });
    }

    #[cfg(not(test))]
    // SAFETY:
    // 1. Invariant: `INSTALLED_LOG`, when present, holds the single retained
    //    boot evidence buffer for this core.
    // 2. Established by: `install` above validates and stores the buffer once
    //    for the single-core verify path.
    // 3. Lifetime: the backing loader allocation is retained for the boot.
    // 4. Pointer ownership: PythCore owns the allocation after handoff and
    //    mutates it only through this module.
    // 5. Alignment: byte-slice access has no stronger alignment need.
    // 6. Mapped length: exactly `installed.len` bytes.
    // 7. Concurrency: single-core verification path.
    // 8. Violation: a stale pointer corrupts memory or faults.
    unsafe {
        let installed = core::ptr::addr_of!(INSTALLED_LOG).read()?;
        let buffer = core::slice::from_raw_parts_mut(installed.ptr, installed.len);
        Some(f(buffer))
    }
}

#[cfg(test)]
pub(crate) fn install_for_test(buffer: &mut [u8]) -> Result<(), EvidenceLogAttachError> {
    install(buffer)
}

#[cfg(test)]
pub(crate) fn reset_for_test() {
    TEST_INSTALLED_LOG.with(|state| {
        *state.borrow_mut() = None;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::evidence_log::{EVIDENCE_LOG_TOTAL_BYTES, initialize, snapshot};

    #[repr(align(4096))]
    struct AlignedEvidenceBuffer([u8; EVIDENCE_LOG_TOTAL_BYTES]);

    struct ResetOnDrop;

    impl Drop for ResetOnDrop {
        fn drop(&mut self) {
            reset_for_test();
        }
    }

    fn evidence_boot_info(buffer: &mut [u8]) -> PythBootInfo {
        PythBootInfo {
            magic: 0,
            abi_major: 0,
            abi_minor: 0,
            struct_size: 0,
            flags: 0,
            memory_map_ptr: 0,
            memory_map_len: 0,
            memory_descriptor_size: 0,
            memory_descriptor_version: 0,
            framebuffer: pythos_shared::boot_protocol::PythFramebufferInfo {
                physical_base: 0,
                mapped_virtual_base: 0,
                byte_length: 0,
                width: 0,
                height: 0,
                pixels_per_scanline: 0,
                pixel_format: 0,
                red_mask: 0,
                green_mask: 0,
                blue_mask: 0,
                reserved_mask: 0,
            },
            acpi_rsdp: 0,
            smbios_entry: 0,
            kernel_phys_start: 0,
            kernel_phys_end: 0,
            kernel_virt_start: 0,
            kernel_virt_end: 0,
            bootstrap_stack_bottom: 0,
            bootstrap_stack_top: 0,
            init_bundle_phys: 0,
            init_bundle_len: 0,
            font_phys: 0,
            font_len: 0,
            runtime_services_ptr: 0,
            command_line_ptr: 0,
            command_line_len: 0,
            evidence_log_phys: buffer.as_mut_ptr() as u64,
            evidence_log_len: EVIDENCE_LOG_TOTAL_BYTES as u32,
            evidence_log_flags: PYTH_EVIDENCE_LOG_FLAG_PRESENT,
            reserved: [0; 6],
        }
    }

    #[test]
    fn attach_from_boot_info_appends_marker_to_evidence_log() {
        let _reset = ResetOnDrop;
        let mut buffer = AlignedEvidenceBuffer([0u8; EVIDENCE_LOG_TOTAL_BYTES]);
        initialize(&mut buffer.0).unwrap();
        let boot_info = evidence_boot_info(&mut buffer.0);

        attach_from_boot_info(&boot_info).unwrap();
        append_marker("PYTHOS:CORE:BOOTINFO_VALID");

        let snapshot = snapshot(&buffer.0).unwrap();
        assert_eq!(snapshot.payload, b"PYTHOS:CORE:BOOTINFO_VALID\n");
    }

    #[test]
    fn attach_from_boot_info_rejects_absent_metadata() {
        let _reset = ResetOnDrop;
        let mut buffer = AlignedEvidenceBuffer([0u8; EVIDENCE_LOG_TOTAL_BYTES]);
        initialize(&mut buffer.0).unwrap();
        let mut boot_info = evidence_boot_info(&mut buffer.0);
        boot_info.evidence_log_phys = 0;
        boot_info.evidence_log_len = 0;
        boot_info.evidence_log_flags = 0;

        assert_eq!(
            attach_from_boot_info(&boot_info),
            Err(EvidenceLogAttachError::Absent)
        );
    }
}
