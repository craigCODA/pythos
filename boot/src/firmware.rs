use crate::uefi::{self, EfiGuid, EfiSystemTable};

const ACPI_20_TABLE_GUID: EfiGuid = EfiGuid {
    data1: 0x8868_E871,
    data2: 0xE4F1,
    data3: 0x11D3,
    data4: [0xBC, 0x22, 0x00, 0x80, 0xC7, 0x3C, 0x88, 0x81],
};

const ACPI_TABLE_GUID: EfiGuid = EfiGuid {
    data1: 0xEB9D_2D30,
    data2: 0x2D88,
    data3: 0x11D3,
    data4: [0x9A, 0x16, 0x00, 0x90, 0x27, 0x3F, 0xC1, 0x4D],
};

const SMBIOS_TABLE_GUID: EfiGuid = EfiGuid {
    data1: 0xEB9D_2D31,
    data2: 0x2D88,
    data3: 0x11D3,
    data4: [0x9A, 0x16, 0x00, 0x90, 0x27, 0x3F, 0xC1, 0x4D],
};

const SMBIOS3_TABLE_GUID: EfiGuid = EfiGuid {
    data1: 0xF2FD_1544,
    data2: 0x9794,
    data3: 0x4A2C,
    data4: [0x99, 0x2E, 0xE5, 0xBB, 0xCF, 0x20, 0xE3, 0x94],
};

#[derive(Clone, Copy)]
pub(crate) struct FirmwareTables {
    pub(crate) acpi_rsdp: u64,
    pub(crate) smbios_entry: u64,
}

pub(crate) fn discover(system_table: *mut EfiSystemTable) -> Result<FirmwareTables, ()> {
    let acpi_rsdp = find_config_table(system_table, &ACPI_20_TABLE_GUID)
        .or_else(|| find_config_table(system_table, &ACPI_TABLE_GUID))
        .ok_or(())?;
    let smbios_entry = find_config_table(system_table, &SMBIOS3_TABLE_GUID)
        .or_else(|| find_config_table(system_table, &SMBIOS_TABLE_GUID))
        .ok_or(())?;

    if !validate_rsdp(acpi_rsdp) || !validate_smbios(smbios_entry) {
        return Err(());
    }
    Ok(FirmwareTables {
        acpi_rsdp,
        smbios_entry,
    })
}

fn find_config_table(system_table: *mut EfiSystemTable, guid: &EfiGuid) -> Option<u64> {
    let (tables, count) = uefi::configuration_tables(system_table).ok()?;
    for index in 0..count {
        // SAFETY:
        // 1. Invariant: `tables` points to `count` UEFI configuration-table entries.
        // 2. Established by: `uefi::configuration_tables()` reading the firmware system table.
        // 3. Lifetime: entries remain valid until `ExitBootServices()`.
        // 4. Pointer ownership: firmware owns the array; loader reads entries.
        // 5. Alignment: UEFI configuration entries are naturally aligned.
        // 6. Mapped length: `index < count`, one full configuration-table entry.
        // 7. Concurrency: no concurrent firmware table mutation is expected here.
        // 8. Violation: a bogus count or pointer would read invalid memory.
        let entry = unsafe { &*tables.add(index) };
        if guid_eq(&entry.vendor_guid, guid) && !entry.vendor_table.is_null() {
            return Some(entry.vendor_table as u64);
        }
    }
    None
}

fn validate_rsdp(address: u64) -> bool {
    if address == 0 {
        return false;
    }
    let ptr = address as *const u8;
    // SAFETY:
    // 1. Invariant: `address` is the ACPI RSDP pointer from a UEFI configuration table.
    // 2. Established by: matching the ACPI configuration-table GUID.
    // 3. Lifetime: firmware tables remain valid through loader execution.
    // 4. Pointer ownership: firmware owns the table; loader reads it.
    // 5. Alignment: byte reads require no stronger alignment.
    // 6. Mapped length: RSDP revision 1 occupies at least 20 bytes.
    // 7. Concurrency: firmware table contents are immutable for this slice.
    // 8. Violation: an invalid firmware pointer could fault in the loader.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, 20) };
    if &bytes[0..8] != b"RSD PTR " || checksum(bytes) != 0 {
        return false;
    }
    if bytes[15] < 2 {
        return true;
    }
    // SAFETY:
    // Same invariant as above, extended to ACPI 2.0 RSDP length after revision check.
    let length = unsafe { ptr.add(20).cast::<u32>().read_unaligned() };
    if !(36..=4096).contains(&length) {
        return false;
    }
    // SAFETY:
    // 1. Invariant: ACPI 2.0 RSDP declared `length` bytes and was bounded above.
    // 2. Established by: revision and length checks above.
    // 3. Lifetime: firmware table remains valid through loader execution.
    // 4. Pointer ownership: firmware owns the table; loader reads it.
    // 5. Alignment: byte reads require no stronger alignment.
    // 6. Mapped length: `length` bytes as declared by the RSDP.
    // 7. Concurrency: immutable firmware metadata.
    // 8. Violation: a bogus length could read invalid memory; bounded to one page.
    let extended = unsafe { core::slice::from_raw_parts(ptr, length as usize) };
    checksum(extended) == 0
}

fn validate_smbios(address: u64) -> bool {
    if address == 0 {
        return false;
    }
    let ptr = address as *const u8;
    // SAFETY:
    // 1. Invariant: `address` is an SMBIOS entry-point pointer from a UEFI config table.
    // 2. Established by: matching SMBIOS or SMBIOS3 configuration-table GUID.
    // 3. Lifetime: firmware metadata remains valid through loader execution.
    // 4. Pointer ownership: firmware owns the table; loader reads it.
    // 5. Alignment: byte reads require no stronger alignment.
    // 6. Mapped length: the SMBIOS anchor and length field are within the first 6 bytes.
    // 7. Concurrency: immutable firmware metadata.
    // 8. Violation: an invalid firmware pointer could fault in the loader.
    let anchor = unsafe { core::slice::from_raw_parts(ptr, 5) };
    if anchor == b"_SM3_" {
        // SAFETY: same pointer invariant; SMBIOS3 length is byte 6.
        let length = unsafe { ptr.add(6).read() };
        return validate_smbios_checksum(ptr, length, 0x18);
    }

    // SAFETY:
    // 1. Invariant: same SMBIOS pointer, reading the 4-byte legacy anchor.
    // 2. Established by: SMBIOS configuration-table GUID.
    // 3. Lifetime: firmware metadata remains valid through loader execution.
    // 4. Pointer ownership: firmware owns the table; loader reads it.
    // 5. Alignment: byte reads require no stronger alignment.
    // 6. Mapped length: at least 4 bytes for `_SM_`.
    // 7. Concurrency: immutable firmware metadata.
    // 8. Violation: invalid pointer could fault.
    let legacy_anchor = unsafe { core::slice::from_raw_parts(ptr, 4) };
    if legacy_anchor == b"_SM_" {
        // SAFETY: same pointer invariant; legacy SMBIOS entry length is byte 5.
        let length = unsafe { ptr.add(5).read() };
        return validate_smbios_checksum(ptr, length, 0x1F);
    }
    false
}

fn validate_smbios_checksum(ptr: *const u8, length: u8, min_length: u8) -> bool {
    if length < min_length {
        return false;
    }
    // SAFETY:
    // 1. Invariant: `ptr` names an SMBIOS entry point whose length byte was read.
    // 2. Established by: matching a known SMBIOS anchor before calling.
    // 3. Lifetime: firmware metadata remains valid through loader execution.
    // 4. Pointer ownership: firmware owns the entry point; loader reads it.
    // 5. Alignment: byte reads require no stronger alignment.
    // 6. Mapped length: `length` bytes as declared by the entry point.
    // 7. Concurrency: immutable firmware metadata.
    // 8. Violation: a bad length could read invalid memory; SMBIOS lengths are small.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, usize::from(length)) };
    checksum(bytes) == 0
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte))
}

fn guid_eq(left: &EfiGuid, right: &EfiGuid) -> bool {
    left.data1 == right.data1
        && left.data2 == right.data2
        && left.data3 == right.data3
        && left.data4 == right.data4
}
