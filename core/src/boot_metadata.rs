//! Milestone 1.5 completion validation for firmware-derived boot metadata.

#![cfg_attr(test, allow(dead_code))]

use pythos_shared::{boot_protocol::PythBootInfo, init_pak};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootMetadataError {
    MissingFirmwareTable,
    BadAcpi,
    BadSmbios,
    BadInitPak,
    RuntimeServicesUnsupported,
}

pub fn validate_complete(boot_info: &PythBootInfo) -> Result<(), BootMetadataError> {
    if boot_info.runtime_services_ptr != 0 {
        return Err(BootMetadataError::RuntimeServicesUnsupported);
    }
    validate_acpi(boot_info.acpi_rsdp)?;
    validate_smbios(boot_info.smbios_entry)?;
    validate_init_pak(boot_info)?;
    Ok(())
}

fn validate_init_pak(boot_info: &PythBootInfo) -> Result<(), BootMetadataError> {
    let len =
        usize::try_from(boot_info.init_bundle_len).map_err(|_| BootMetadataError::BadInitPak)?;
    if boot_info.init_bundle_phys == 0 || len == 0 {
        return Err(BootMetadataError::BadInitPak);
    }
    // SAFETY:
    // 1. Invariant: `init_bundle_phys` is mapped readable after `VM_READY`.
    // 2. Established by: `KernelAddressSpace::build()` mapping the exact init bundle range.
    // 3. Lifetime: loader allocation is transferred to PythCore for early boot.
    // 4. Pointer ownership: PythCore owns the buffer and reads it immutably.
    // 5. Alignment: byte slices require no stronger alignment.
    // 6. Mapped length: `init_bundle_len` bytes were mapped by the VM slice.
    // 7. Concurrency: single-core early boot with interrupts disabled.
    // 8. Violation: a bad mapping would fault through exception diagnostics.
    let bytes =
        unsafe { core::slice::from_raw_parts(boot_info.init_bundle_phys as *const u8, len) };
    init_pak::validate(bytes).map_err(|_| BootMetadataError::BadInitPak)?;
    Ok(())
}

fn validate_acpi(rsdp: u64) -> Result<(), BootMetadataError> {
    if rsdp == 0 {
        return Err(BootMetadataError::MissingFirmwareTable);
    }
    let ptr = rsdp as *const u8;
    // SAFETY:
    // 1. Invariant: `rsdp` is mapped readable after `VM_READY`.
    // 2. Established by: `KernelAddressSpace::build()` mapping the RSDP range.
    // 3. Lifetime: firmware table memory remains reserved in early boot.
    // 4. Pointer ownership: firmware owns the table; PythCore reads it.
    // 5. Alignment: byte reads require no stronger alignment.
    // 6. Mapped length: at least 20 RSDP bytes are mapped.
    // 7. Concurrency: firmware metadata is immutable for this phase.
    // 8. Violation: a bad pointer faults through exception diagnostics.
    let base = unsafe { core::slice::from_raw_parts(ptr, 20) };
    if &base[0..8] != b"RSD PTR " || checksum(base) != 0 {
        return Err(BootMetadataError::BadAcpi);
    }

    let root_address = if base[15] >= 2 {
        let length = read_u32(ptr, 20);
        if !(36..=4096).contains(&length) {
            return Err(BootMetadataError::BadAcpi);
        }
        // SAFETY:
        // 1. Invariant: ACPI 2.0 RSDP declared a bounded length mapped by VM setup.
        // 2. Established by: revision and length checks above plus VM firmware mapping.
        // 3. Lifetime: firmware table memory remains reserved in early boot.
        // 4. Pointer ownership: firmware owns the table; PythCore reads it.
        // 5. Alignment: byte reads require no stronger alignment.
        // 6. Mapped length: `length` bytes are mapped.
        // 7. Concurrency: immutable firmware metadata.
        // 8. Violation: a bad table faults or fails checksum.
        let extended = unsafe { core::slice::from_raw_parts(ptr, length as usize) };
        if checksum(extended) != 0 {
            return Err(BootMetadataError::BadAcpi);
        }
        let xsdt = read_u64(ptr, 24);
        if xsdt != 0 {
            xsdt
        } else {
            u64::from(read_u32(ptr, 16))
        }
    } else {
        u64::from(read_u32(ptr, 16))
    };

    validate_acpi_sdt(root_address)
}

fn validate_acpi_sdt(address: u64) -> Result<(), BootMetadataError> {
    if address == 0 {
        return Err(BootMetadataError::BadAcpi);
    }
    let ptr = address as *const u8;
    let length = read_u32(ptr, 4);
    if !(36..=4096).contains(&length) {
        return Err(BootMetadataError::BadAcpi);
    }
    // SAFETY:
    // 1. Invariant: `address` names the mapped ACPI root system-description table.
    // 2. Established by: VM setup parsing RSDP before the CR3 switch and mapping this table.
    // 3. Lifetime: firmware table memory remains reserved in early boot.
    // 4. Pointer ownership: firmware owns the table; PythCore reads it.
    // 5. Alignment: byte reads require no stronger alignment.
    // 6. Mapped length: `length` bytes are mapped for checksum validation.
    // 7. Concurrency: immutable firmware metadata.
    // 8. Violation: a bad table faults or fails checksum.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, length as usize) };
    if checksum(bytes) == 0 {
        Ok(())
    } else {
        Err(BootMetadataError::BadAcpi)
    }
}

fn validate_smbios(address: u64) -> Result<(), BootMetadataError> {
    if address == 0 {
        return Err(BootMetadataError::MissingFirmwareTable);
    }
    let ptr = address as *const u8;
    // SAFETY:
    // 1. Invariant: `address` names a mapped SMBIOS entry point.
    // 2. Established by: loader discovery and VM firmware mapping.
    // 3. Lifetime: firmware metadata remains reserved in early boot.
    // 4. Pointer ownership: firmware owns the table; PythCore reads it.
    // 5. Alignment: byte reads require no stronger alignment.
    // 6. Mapped length: at least 5 bytes for anchor detection.
    // 7. Concurrency: immutable firmware metadata.
    // 8. Violation: a bad pointer faults through exception diagnostics.
    let anchor = unsafe { core::slice::from_raw_parts(ptr, 5) };
    if anchor == b"_SM3_" {
        // SAFETY: same mapped SMBIOS entry-point invariant; length byte is at offset 6.
        let length = unsafe { ptr.add(6).read() };
        return validate_smbios_checksum(ptr, length, 0x18);
    }
    // SAFETY:
    // 1. Invariant: same mapped SMBIOS entry-point pointer, reading legacy anchor.
    // 2. Established by: loader discovery and VM firmware mapping.
    // 3. Lifetime: firmware metadata remains reserved in early boot.
    // 4. Pointer ownership: firmware owns the table; PythCore reads it.
    // 5. Alignment: byte reads require no stronger alignment.
    // 6. Mapped length: at least 4 bytes for `_SM_`.
    // 7. Concurrency: immutable firmware metadata.
    // 8. Violation: a bad pointer faults through exception diagnostics.
    let legacy_anchor = unsafe { core::slice::from_raw_parts(ptr, 4) };
    if legacy_anchor == b"_SM_" {
        // SAFETY: same mapped SMBIOS entry-point invariant; length byte is at offset 5.
        let length = unsafe { ptr.add(5).read() };
        return validate_smbios_checksum(ptr, length, 0x1F);
    }
    Err(BootMetadataError::BadSmbios)
}

fn validate_smbios_checksum(
    ptr: *const u8,
    length: u8,
    min_length: u8,
) -> Result<(), BootMetadataError> {
    if length < min_length {
        return Err(BootMetadataError::BadSmbios);
    }
    // SAFETY:
    // 1. Invariant: `ptr` names a mapped SMBIOS entry point with a declared length.
    // 2. Established by: anchor detection before this call.
    // 3. Lifetime: firmware metadata remains reserved in early boot.
    // 4. Pointer ownership: firmware owns the table; PythCore reads it.
    // 5. Alignment: byte reads require no stronger alignment.
    // 6. Mapped length: VM setup maps the declared entry-point length.
    // 7. Concurrency: immutable firmware metadata.
    // 8. Violation: a bad length faults or fails checksum.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, usize::from(length)) };
    if checksum(bytes) == 0 {
        Ok(())
    } else {
        Err(BootMetadataError::BadSmbios)
    }
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte))
}

fn read_u32(ptr: *const u8, offset: usize) -> u32 {
    // SAFETY:
    // 1. Invariant: caller established that `ptr + offset..offset+4` is mapped.
    // 2. Established by: ACPI RSDP or SDT length checks before each call site.
    // 3. Lifetime: firmware metadata remains reserved in early boot.
    // 4. Pointer ownership: firmware owns the table; PythCore reads it.
    // 5. Alignment: `read_unaligned` handles firmware table packing.
    // 6. Mapped length: 4 bytes at `offset`.
    // 7. Concurrency: immutable firmware metadata.
    // 8. Violation: bad caller bounds would fault through diagnostics.
    unsafe { ptr.add(offset).cast::<u32>().read_unaligned() }
}

fn read_u64(ptr: *const u8, offset: usize) -> u64 {
    // SAFETY:
    // 1. Invariant: caller established that `ptr + offset..offset+8` is mapped.
    // 2. Established by: ACPI RSDP revision and length checks before call site.
    // 3. Lifetime: firmware metadata remains reserved in early boot.
    // 4. Pointer ownership: firmware owns the table; PythCore reads it.
    // 5. Alignment: `read_unaligned` handles firmware table packing.
    // 6. Mapped length: 8 bytes at `offset`.
    // 7. Concurrency: immutable firmware metadata.
    // 8. Violation: bad caller bounds would fault through diagnostics.
    unsafe { ptr.add(offset).cast::<u64>().read_unaligned() }
}
