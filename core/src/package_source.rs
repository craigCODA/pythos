#![cfg_attr(test, allow(dead_code))]

use crate::{
    capabilities::{CapabilityHandle, CapabilityTable, ResourceId, RightsMask},
    service_identity::ServiceId,
};
use pythos_shared::{
    init_bundle::{InitBundle, RecordType},
    package_abi::{
        MAX_PACKAGE_ARTIFACT_BYTES, MAX_PACKAGE_SOURCE_LABEL_BYTES, MAX_PACKAGE_SOURCES,
        PACKAGE_SOURCE_READ_RIGHT, PACKAGE_SOURCE_RESOURCE_ID, PackageSourceHandle, PackageStatus,
    },
    sha256::sha256,
};

const PACKAGE_SOURCE_MAGIC: &[u8; 8] = b"PYPKGS01";
const PACKAGE_SOURCE_HEADER_LEN: usize = 64;
const PACKAGE_SOURCE_GENERATION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageSourceService<'a> {
    sources: [Option<PackageSourceEntry<'a>>; MAX_PACKAGE_SOURCES],
    count: usize,
    generation: u16,
}

impl<'a> PackageSourceService<'a> {
    pub const fn empty() -> Self {
        Self {
            sources: [None; MAX_PACKAGE_SOURCES],
            count: 0,
            generation: PACKAGE_SOURCE_GENERATION,
        }
    }

    pub fn from_init_bundle(bundle: &'a InitBundle<'a>) -> Result<Self, PackageStatus> {
        let mut service = Self::empty();
        service.load_from_init_bundle(bundle)?;
        Ok(service)
    }

    pub fn load_from_init_bundle(
        &mut self,
        bundle: &'a InitBundle<'a>,
    ) -> Result<(), PackageStatus> {
        self.sources = [None; MAX_PACKAGE_SOURCES];
        self.count = 0;
        self.generation = PACKAGE_SOURCE_GENERATION;
        let mut ordinal = 0usize;
        while let Some(record) = bundle.record_at(RecordType::PackageSource, ordinal) {
            if ordinal >= MAX_PACKAGE_SOURCES {
                return Err(PackageStatus::BoundsExceeded);
            }
            self.sources[ordinal] = Some(parse_source_record(record.bytes(), ordinal)?);
            self.count += 1;
            ordinal += 1;
        }
        if self.count == 0 {
            return Err(PackageStatus::SourceMissing);
        }
        Ok(())
    }

    pub fn handle_at(&self, ordinal: usize) -> Option<PackageSourceHandle> {
        if ordinal >= self.count {
            return None;
        }
        Some(PackageSourceHandle::from_parts(
            self.sources[ordinal]?.source_id,
            self.generation,
        ))
    }

    pub fn read(
        &self,
        caller: ServiceId,
        capabilities: &CapabilityTable,
        handle: PackageSourceHandle,
        read_capability: CapabilityHandle,
        out: &mut [u8],
    ) -> Result<usize, PackageStatus> {
        let source = self.source_for_handle(handle)?;
        capabilities
            .validate(
                caller,
                read_capability,
                ResourceId::new(PACKAGE_SOURCE_RESOURCE_ID),
                RightsMask::new(PACKAGE_SOURCE_READ_RIGHT as u32),
            )
            .map_err(|_| PackageStatus::SourceReadDenied)?;
        if out.len() < source.artifact.len() {
            return Err(PackageStatus::BufferTooSmall);
        }
        out[..source.artifact.len()].copy_from_slice(source.artifact);
        Ok(source.artifact.len())
    }

    fn source_for_handle(
        &self,
        handle: PackageSourceHandle,
    ) -> Result<PackageSourceEntry<'a>, PackageStatus> {
        if !handle.has_magic() || handle.generation() != self.generation {
            return Err(PackageStatus::SourceHandleInvalid);
        }
        let index = usize::from(handle.source_id());
        if index >= self.count {
            return Err(PackageStatus::SourceHandleInvalid);
        }
        self.sources[index].ok_or(PackageStatus::SourceHandleInvalid)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PackageSourceEntry<'a> {
    source_id: u16,
    _label: &'a [u8],
    artifact: &'a [u8],
}

fn parse_source_record(
    bytes: &[u8],
    ordinal: usize,
) -> Result<PackageSourceEntry<'_>, PackageStatus> {
    if bytes.len() < PACKAGE_SOURCE_HEADER_LEN {
        return Err(PackageStatus::SourceMissing);
    }
    if &bytes[..PACKAGE_SOURCE_MAGIC.len()] != PACKAGE_SOURCE_MAGIC {
        return Err(PackageStatus::InvalidMagic);
    }
    let source_id = read_u16(bytes, 8)?;
    if usize::from(source_id) != ordinal {
        return Err(PackageStatus::SourceHandleInvalid);
    }
    let label_len = usize::from(read_u16(bytes, 10)?);
    if label_len == 0 || label_len > MAX_PACKAGE_SOURCE_LABEL_BYTES {
        return Err(PackageStatus::BoundsExceeded);
    }
    if bytes[12..16].iter().any(|&byte| byte != 0) {
        return Err(PackageStatus::BadRequest);
    }
    let artifact_offset =
        usize::try_from(read_u64(bytes, 16)?).map_err(|_| PackageStatus::BoundsExceeded)?;
    let artifact_len =
        usize::try_from(read_u64(bytes, 24)?).map_err(|_| PackageStatus::BoundsExceeded)?;
    if artifact_len == 0 || artifact_len > MAX_PACKAGE_ARTIFACT_BYTES {
        return Err(PackageStatus::BoundsExceeded);
    }
    if artifact_offset != PACKAGE_SOURCE_HEADER_LEN + label_len {
        return Err(PackageStatus::InvalidOffset);
    }
    let artifact_end = artifact_offset
        .checked_add(artifact_len)
        .ok_or(PackageStatus::LengthOverflow)?;
    if artifact_end != bytes.len() {
        return Err(PackageStatus::BoundsExceeded);
    }
    let label = bytes
        .get(PACKAGE_SOURCE_HEADER_LEN..artifact_offset)
        .ok_or(PackageStatus::BoundsExceeded)?;
    let artifact = bytes
        .get(artifact_offset..artifact_end)
        .ok_or(PackageStatus::BoundsExceeded)?;
    let mut expected_digest = [0u8; 32];
    expected_digest.copy_from_slice(&bytes[32..64]);
    if sha256(artifact) != expected_digest {
        return Err(PackageStatus::DigestMismatch);
    }
    Ok(PackageSourceEntry {
        source_id,
        _label: label,
        artifact,
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, PackageStatus> {
    let raw = bytes
        .get(offset..offset + 2)
        .ok_or(PackageStatus::BoundsExceeded)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, PackageStatus> {
    let raw = bytes
        .get(offset..offset + 8)
        .ok_or(PackageStatus::BoundsExceeded)?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::{
        capabilities::{CapabilityHandle, CapabilityTable, ResourceId, RightsMask},
        service_identity::ServiceId,
    };
    use pythos_shared::{
        init_bundle::{self, INIT_BUNDLE_HEADER_LEN, RECORD_ENTRY_LEN, TYPE_PACKAGE_SOURCE},
        package_abi::{
            PACKAGE_SOURCE_READ_RIGHT, PACKAGE_SOURCE_RESOURCE_ID, PackageSourceHandle,
            PackageStatus,
        },
        sha256::sha256,
    };
    use std::vec;
    use std::vec::Vec;

    const SOURCE_MAGIC: &[u8; 8] = b"PYPKGS01";
    const SOURCE_HEADER_LEN: usize = 64;

    fn build_bundle(records: &[(u32, &[u8])]) -> Vec<u8> {
        let header_len = INIT_BUNDLE_HEADER_LEN as usize;
        let table_len = records.len() * RECORD_ENTRY_LEN;
        let mut bytes = vec![0u8; header_len + table_len];
        bytes[..init_bundle::INIT_BUNDLE_MAGIC.len()]
            .copy_from_slice(init_bundle::INIT_BUNDLE_MAGIC);
        bytes[16..18].copy_from_slice(&init_bundle::INIT_BUNDLE_MAJOR.to_le_bytes());
        bytes[18..20].copy_from_slice(&init_bundle::INIT_BUNDLE_MINOR.to_le_bytes());
        bytes[20..24].copy_from_slice(&INIT_BUNDLE_HEADER_LEN.to_le_bytes());
        bytes[24..26].copy_from_slice(&(records.len() as u16).to_le_bytes());

        let mut cursor = header_len + table_len;
        for (index, (record_type, payload)) in records.iter().enumerate() {
            let entry = header_len + index * RECORD_ENTRY_LEN;
            bytes[entry..entry + 4].copy_from_slice(&record_type.to_le_bytes());
            bytes[entry + 8..entry + 16].copy_from_slice(&(cursor as u64).to_le_bytes());
            bytes[entry + 16..entry + 24].copy_from_slice(&(payload.len() as u64).to_le_bytes());
            bytes[entry + 24..entry + 28]
                .copy_from_slice(&init_bundle::checksum(payload).to_le_bytes());
            bytes.extend_from_slice(payload);
            cursor += payload.len();
        }
        bytes
    }

    fn build_source_record(source_id: u16, label: &[u8], artifact: &[u8]) -> Vec<u8> {
        let artifact_offset = SOURCE_HEADER_LEN + label.len();
        let mut header = vec![0u8; SOURCE_HEADER_LEN];
        header[0..8].copy_from_slice(SOURCE_MAGIC);
        header[8..10].copy_from_slice(&source_id.to_le_bytes());
        header[10..12].copy_from_slice(&(label.len() as u16).to_le_bytes());
        header[16..24].copy_from_slice(&(artifact_offset as u64).to_le_bytes());
        header[24..32].copy_from_slice(&(artifact.len() as u64).to_le_bytes());
        header[32..64].copy_from_slice(&sha256(artifact));
        header.extend_from_slice(label);
        header.extend_from_slice(artifact);
        header
    }

    #[test]
    fn source_handle_without_read_capability_is_denied() {
        let artifact = b"PYTHPKG0-package";
        let source = build_source_record(0, b"package.pkg", artifact);
        let bundle_bytes = build_bundle(&[(TYPE_PACKAGE_SOURCE, source.as_slice())]);
        let bundle = init_bundle::validate(&bundle_bytes).unwrap();
        let service = PackageSourceService::from_init_bundle(&bundle).unwrap();
        let caller = ServiceId::from_raw(7);
        let table = CapabilityTable::new();
        let source_handle = service.handle_at(0).unwrap();
        let forged_read_capability = CapabilityHandle::from_parts(0, 1);
        let mut out = [0u8; 32];

        assert_eq!(
            service.read(
                caller,
                &table,
                source_handle,
                forged_read_capability,
                &mut out,
            ),
            Err(PackageStatus::SourceReadDenied)
        );
    }

    #[test]
    fn source_read_copies_exact_bounded_bytes() {
        let artifact = b"PYTHPKG0-package";
        let source = build_source_record(0, b"package.pkg", artifact);
        let bundle_bytes = build_bundle(&[(TYPE_PACKAGE_SOURCE, source.as_slice())]);
        let bundle = init_bundle::validate(&bundle_bytes).unwrap();
        let service = PackageSourceService::from_init_bundle(&bundle).unwrap();
        let caller = ServiceId::from_raw(7);
        let mut table = CapabilityTable::new();
        let read_capability = table
            .grant(
                caller,
                ResourceId::new(PACKAGE_SOURCE_RESOURCE_ID),
                RightsMask::new(PACKAGE_SOURCE_READ_RIGHT as u32),
            )
            .unwrap();
        let mut out = [0u8; 32];

        let copied = service
            .read(
                caller,
                &table,
                PackageSourceHandle::from_parts(0, 1),
                read_capability,
                &mut out,
            )
            .unwrap();

        assert_eq!(copied, artifact.len());
        assert_eq!(&out[..copied], artifact);
    }
}
