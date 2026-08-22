use crate::package_abi::{
    MAX_CONTENT_BYTES, MAX_CONTENT_TABLE_BYTES, MAX_MANIFEST_BYTES, MAX_PACKAGE_ARTIFACT_BYTES,
};
use crate::sha256::{Sha256, sha256};

pub const PACKAGE_ARTIFACT_MAGIC: &[u8; 8] = b"PYTHPKG0";
pub const PACKAGE_ARTIFACT_MAJOR: u16 = 0;
pub const PACKAGE_ARTIFACT_MINOR: u16 = 1;
pub const PACKAGE_ARTIFACT_HEADER_LEN: usize = 160;

const MAGIC_OFFSET: usize = 0;
const MAJOR_OFFSET: usize = 8;
const MINOR_OFFSET: usize = 10;
const HEADER_LEN_OFFSET: usize = 12;
const MANIFEST_OFFSET_OFFSET: usize = 16;
const MANIFEST_LENGTH_OFFSET: usize = 24;
const CONTENT_TABLE_OFFSET_OFFSET: usize = 32;
const CONTENT_TABLE_LENGTH_OFFSET: usize = 40;
const CONTENT_BYTES_OFFSET_OFFSET: usize = 48;
const CONTENT_BYTES_LENGTH_OFFSET: usize = 56;
const MANIFEST_SHA256_OFFSET: usize = 64;
const ARTIFACT_SHA256_OFFSET: usize = 96;
const RESERVED_OFFSET: usize = 128;
const RESERVED_END: usize = PACKAGE_ARTIFACT_HEADER_LEN;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageFormatError {
    TooShort,
    InvalidMagic,
    UnsupportedMajor,
    UnsupportedRequiredMinor,
    InvalidHeaderLength,
    NonZeroReserved,
    LengthOverflow,
    BoundsExceeded,
    ManifestDigestMismatch,
    ArtifactDigestMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManifestV0<'a> {
    bytes: &'a [u8],
}

impl<'a> ManifestV0<'a> {
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageArtifactV0<'a> {
    bytes: &'a [u8],
    manifest_range: Range,
    content_table_range: Range,
    content_bytes_range: Range,
    manifest_sha256: [u8; 32],
    artifact_sha256: [u8; 32],
}

impl<'a> PackageArtifactV0<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, PackageFormatError> {
        if bytes.len() < PACKAGE_ARTIFACT_HEADER_LEN {
            return Err(PackageFormatError::TooShort);
        }
        if bytes.len() > MAX_PACKAGE_ARTIFACT_BYTES {
            return Err(PackageFormatError::BoundsExceeded);
        }
        if &bytes[MAGIC_OFFSET..MAGIC_OFFSET + 8] != PACKAGE_ARTIFACT_MAGIC {
            return Err(PackageFormatError::InvalidMagic);
        }

        let major = read_u16(bytes, MAJOR_OFFSET);
        let minor = read_u16(bytes, MINOR_OFFSET);
        if major != PACKAGE_ARTIFACT_MAJOR {
            return Err(PackageFormatError::UnsupportedMajor);
        }
        if minor > PACKAGE_ARTIFACT_MINOR {
            return Err(PackageFormatError::UnsupportedRequiredMinor);
        }
        if read_u32(bytes, HEADER_LEN_OFFSET) as usize != PACKAGE_ARTIFACT_HEADER_LEN {
            return Err(PackageFormatError::InvalidHeaderLength);
        }
        if bytes[RESERVED_OFFSET..RESERVED_END]
            .iter()
            .any(|&byte| byte != 0)
        {
            return Err(PackageFormatError::NonZeroReserved);
        }

        let manifest_range = checked_range(
            read_u64(bytes, MANIFEST_OFFSET_OFFSET),
            read_u64(bytes, MANIFEST_LENGTH_OFFSET),
            bytes.len(),
            MAX_MANIFEST_BYTES,
        )?;
        let content_table_range = checked_range(
            read_u64(bytes, CONTENT_TABLE_OFFSET_OFFSET),
            read_u64(bytes, CONTENT_TABLE_LENGTH_OFFSET),
            bytes.len(),
            MAX_CONTENT_TABLE_BYTES,
        )?;
        let content_bytes_range = checked_range(
            read_u64(bytes, CONTENT_BYTES_OFFSET_OFFSET),
            read_u64(bytes, CONTENT_BYTES_LENGTH_OFFSET),
            bytes.len(),
            MAX_CONTENT_BYTES,
        )?;

        let manifest_sha256 = read_sha256(bytes, MANIFEST_SHA256_OFFSET);
        if sha256(manifest_range.slice(bytes)) != manifest_sha256 {
            return Err(PackageFormatError::ManifestDigestMismatch);
        }

        let artifact_sha256 = read_sha256(bytes, ARTIFACT_SHA256_OFFSET);
        if artifact_digest(bytes) != artifact_sha256 {
            return Err(PackageFormatError::ArtifactDigestMismatch);
        }

        Ok(Self {
            bytes,
            manifest_range,
            content_table_range,
            content_bytes_range,
            manifest_sha256,
            artifact_sha256,
        })
    }

    pub const fn artifact_sha256(&self) -> [u8; 32] {
        self.artifact_sha256
    }

    pub const fn manifest_sha256(&self) -> [u8; 32] {
        self.manifest_sha256
    }

    pub fn manifest(&self) -> ManifestV0<'a> {
        ManifestV0 {
            bytes: self.manifest_range.slice(self.bytes),
        }
    }

    pub fn content_table_bytes(&self) -> &'a [u8] {
        self.content_table_range.slice(self.bytes)
    }

    pub fn content_payload_bytes(&self) -> &'a [u8] {
        self.content_bytes_range.slice(self.bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Range {
    start: usize,
    end: usize,
}

impl Range {
    pub fn slice<'a>(self, bytes: &'a [u8]) -> &'a [u8] {
        &bytes[self.start..self.end]
    }
}

fn checked_range(
    offset: u64,
    length: u64,
    bytes_len: usize,
    max_len: usize,
) -> Result<Range, PackageFormatError> {
    let start = usize::try_from(offset).map_err(|_| PackageFormatError::BoundsExceeded)?;
    let len = usize::try_from(length).map_err(|_| PackageFormatError::BoundsExceeded)?;
    if len > max_len {
        return Err(PackageFormatError::BoundsExceeded);
    }
    let end = start
        .checked_add(len)
        .ok_or(PackageFormatError::LengthOverflow)?;
    if start < PACKAGE_ARTIFACT_HEADER_LEN || end > bytes_len {
        return Err(PackageFormatError::BoundsExceeded);
    }
    Ok(Range { start, end })
}

fn artifact_digest(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(&bytes[..ARTIFACT_SHA256_OFFSET]);
    hasher.update(&[0u8; 32]);
    hasher.update(&bytes[ARTIFACT_SHA256_OFFSET + 32..]);
    hasher.finalize()
}

fn read_sha256(bytes: &[u8], offset: usize) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes[offset..offset + 32]);
    out
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256::sha256;

    const HEADER_LEN: usize = 160;
    const MANIFEST: &[u8; 12] = b"PYTHMAN0\0\0\0\0";
    const MANIFEST_SHA256_OFFSET: usize = 64;
    const ARTIFACT_SHA256_OFFSET: usize = 96;

    fn write_u16(out: &mut [u8], offset: usize, value: u16) {
        out[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(out: &mut [u8], offset: usize, value: u32) {
        out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u64(out: &mut [u8], offset: usize, value: u64) {
        out[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn minimal_artifact() -> ([u8; HEADER_LEN + MANIFEST.len()], [u8; 32]) {
        let mut bytes = [0u8; HEADER_LEN + MANIFEST.len()];
        bytes[0..8].copy_from_slice(b"PYTHPKG0");
        write_u16(&mut bytes, 8, 0);
        write_u16(&mut bytes, 10, 1);
        write_u32(&mut bytes, 12, HEADER_LEN as u32);
        write_u64(&mut bytes, 16, HEADER_LEN as u64);
        write_u64(&mut bytes, 24, MANIFEST.len() as u64);
        write_u64(&mut bytes, 32, (HEADER_LEN + MANIFEST.len()) as u64);
        write_u64(&mut bytes, 40, 0);
        write_u64(&mut bytes, 48, (HEADER_LEN + MANIFEST.len()) as u64);
        write_u64(&mut bytes, 56, 0);
        bytes[MANIFEST_SHA256_OFFSET..MANIFEST_SHA256_OFFSET + 32]
            .copy_from_slice(&sha256(MANIFEST));
        bytes[HEADER_LEN..].copy_from_slice(MANIFEST);

        let mut zeroed = bytes;
        zeroed[ARTIFACT_SHA256_OFFSET..ARTIFACT_SHA256_OFFSET + 32].fill(0);
        let artifact_digest = sha256(&zeroed);
        bytes[ARTIFACT_SHA256_OFFSET..ARTIFACT_SHA256_OFFSET + 32]
            .copy_from_slice(&artifact_digest);
        (bytes, artifact_digest)
    }

    #[test]
    fn package_header_validates_zero_filled_artifact_digest_domain() {
        let (bytes, artifact_digest) = minimal_artifact();

        let artifact = PackageArtifactV0::parse(&bytes).unwrap();

        assert_eq!(artifact.artifact_sha256(), artifact_digest);
        assert_eq!(artifact.manifest_sha256(), sha256(MANIFEST));
        assert_eq!(artifact.manifest().bytes(), MANIFEST);
    }
}
