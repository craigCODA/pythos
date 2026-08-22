#![cfg_attr(test, allow(dead_code))]

use pythos_shared::{
    boot_protocol::PythBootInfo,
    init_bundle, init_pak,
    package_abi::{MAX_PACKAGE_ARTIFACT_BYTES, MAX_PACKAGE_SOURCE_LABEL_BYTES, MAX_PACKAGE_SOURCES},
    package_format::PackageArtifactV0,
    sha256::sha256,
};

use crate::serial;

const PACKAGE_SOURCE_MAGIC: &[u8; 8] = b"PYPKGS01";
const PACKAGE_SOURCE_HEADER_LEN: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageAcceptanceError {
    BadInitPak,
    BadInitBundle,
    MissingSource,
    TooManySources,
    BadSource,
    BadSourceDigest,
    BadPackageFormat,
    InvalidPackageWasAccepted,
}

pub fn run_package_format_acceptance(
    boot_info: &PythBootInfo,
) -> Result<(), PackageAcceptanceError> {
    let bytes = init_pak_bytes(boot_info)?;
    validate_package_sources(bytes)?;

    serial::write_line("PYTHOS:CORE:PACKAGE_SOURCE_READY");
    serial::write_line("PYTHOS:CORE:PACKAGE_FORMAT:VALID");
    if PackageArtifactV0::parse(b"not-a-package").is_err() {
        serial::write_line("PYTHOS:CORE:PACKAGE_FORMAT:INVALID_DENIED");
    } else {
        return Err(PackageAcceptanceError::InvalidPackageWasAccepted);
    }
    serial::write_line("PYTHOS:CORE:PACKAGE_FORMAT_READY");
    Ok(())
}

fn validate_package_sources(bytes: &[u8]) -> Result<(), PackageAcceptanceError> {
    let payload = init_pak_payload(bytes)?;
    let bundle =
        init_bundle::validate(payload).map_err(|_| PackageAcceptanceError::BadInitBundle)?;
    let mut ordinal = 0usize;
    while let Some(record) = bundle.record_at(init_bundle::RecordType::PackageSource, ordinal) {
        if ordinal >= MAX_PACKAGE_SOURCES {
            return Err(PackageAcceptanceError::TooManySources);
        }
        let artifact = package_artifact_from_source(record.bytes(), ordinal)?;
        PackageArtifactV0::parse(artifact).map_err(|_| PackageAcceptanceError::BadPackageFormat)?;
        ordinal += 1;
    }
    if ordinal == 0 {
        return Err(PackageAcceptanceError::MissingSource);
    }
    Ok(())
}

fn package_artifact_from_source(
    bytes: &[u8],
    ordinal: usize,
) -> Result<&[u8], PackageAcceptanceError> {
    if bytes.len() < PACKAGE_SOURCE_HEADER_LEN {
        return Err(PackageAcceptanceError::BadSource);
    }
    if &bytes[..PACKAGE_SOURCE_MAGIC.len()] != PACKAGE_SOURCE_MAGIC {
        return Err(PackageAcceptanceError::BadSource);
    }
    let source_id = usize::from(read_u16(bytes, 8)?);
    if source_id != ordinal {
        return Err(PackageAcceptanceError::BadSource);
    }
    let label_len = usize::from(read_u16(bytes, 10)?);
    if label_len == 0 || label_len > MAX_PACKAGE_SOURCE_LABEL_BYTES {
        return Err(PackageAcceptanceError::BadSource);
    }
    if bytes[12..16].iter().any(|&byte| byte != 0) {
        return Err(PackageAcceptanceError::BadSource);
    }
    let artifact_offset =
        usize::try_from(read_u64(bytes, 16)?).map_err(|_| PackageAcceptanceError::BadSource)?;
    let artifact_len =
        usize::try_from(read_u64(bytes, 24)?).map_err(|_| PackageAcceptanceError::BadSource)?;
    if artifact_len == 0 || artifact_len > MAX_PACKAGE_ARTIFACT_BYTES {
        return Err(PackageAcceptanceError::BadSource);
    }
    if artifact_offset != PACKAGE_SOURCE_HEADER_LEN + label_len {
        return Err(PackageAcceptanceError::BadSource);
    }
    let artifact_end = artifact_offset
        .checked_add(artifact_len)
        .ok_or(PackageAcceptanceError::BadSource)?;
    if artifact_end != bytes.len() {
        return Err(PackageAcceptanceError::BadSource);
    }
    let artifact = bytes
        .get(artifact_offset..artifact_end)
        .ok_or(PackageAcceptanceError::BadSource)?;
    let mut expected_digest = [0u8; 32];
    expected_digest.copy_from_slice(&bytes[32..64]);
    if sha256(artifact) != expected_digest {
        return Err(PackageAcceptanceError::BadSourceDigest);
    }
    Ok(artifact)
}

fn init_pak_payload(bytes: &[u8]) -> Result<&[u8], PackageAcceptanceError> {
    let header = init_pak::validate(bytes).map_err(|_| PackageAcceptanceError::BadInitPak)?;
    let payload_start =
        usize::try_from(header.header_len).map_err(|_| PackageAcceptanceError::BadInitPak)?;
    let payload_len =
        usize::try_from(header.payload_len).map_err(|_| PackageAcceptanceError::BadInitPak)?;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or(PackageAcceptanceError::BadInitPak)?;
    bytes
        .get(payload_start..payload_end)
        .ok_or(PackageAcceptanceError::BadInitPak)
}

fn init_pak_bytes(boot_info: &PythBootInfo) -> Result<&[u8], PackageAcceptanceError> {
    let len =
        usize::try_from(boot_info.init_bundle_len).map_err(|_| PackageAcceptanceError::BadInitPak)?;
    if boot_info.init_bundle_phys == 0 || len == 0 {
        return Err(PackageAcceptanceError::BadInitPak);
    }
    // SAFETY:
    // 1. Invariant: `init_bundle_phys` points to the loader-retained INIT.PAK
    //    byte range already mapped readable by the active verify-path page
    //    tables.
    // 2. Established by: boot metadata validation and the VM switch completed
    //    before Phase 13 package-format acceptance runs.
    // 3. Lifetime: the loader allocation is retained for the whole boot.
    // 4. Pointer ownership: PythCore owns the allocation and reads it
    //    immutably here.
    // 5. Alignment: byte-slice reads require only `u8` alignment.
    // 6. Mapped length: `init_bundle_len` bytes were mapped into the kernel
    //    address space.
    // 7. Concurrency: single-core verify path; no writer exists.
    // 8. Violation: an invalid mapping faults or fails package-source checks.
    Ok(unsafe { core::slice::from_raw_parts(boot_info.init_bundle_phys as *const u8, len) })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, PackageAcceptanceError> {
    let raw = bytes
        .get(offset..offset + 2)
        .ok_or(PackageAcceptanceError::BadSource)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, PackageAcceptanceError> {
    let raw = bytes
        .get(offset..offset + 8)
        .ok_or(PackageAcceptanceError::BadSource)?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}
