//! Versioned named PythTIG graph-package manifest.
//!
//! The layout intentionally mirrors `user_program_manifest`: a trusted
//! boot-bundle integrity envelope around an already canonical graph package.
//! The digest is corruption/mismatch evidence, not a signature or authority.

pub use crate::user_program_manifest::digest64;

pub const NAMED_PYTH_GRAPH_MAGIC: &[u8; 8] = b"PYTIGM01";
pub const NAMED_PYTH_GRAPH_MAJOR: u16 = 1;
pub const NAMED_PYTH_GRAPH_MINOR: u16 = 0;
pub const NAMED_PYTH_GRAPH_HEADER_LEN: usize = 40;
pub const MAX_NAMED_PYTH_GRAPH_NAME_LEN: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythGraphManifestError {
    TooShort,
    BadMagic,
    UnsupportedVersion,
    NameTooLong,
    LengthOverflow,
    BadDigest,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NamedPythGraphManifest<'a> {
    name: &'a [u8],
    principal_id: u64,
    package_digest: u64,
    package: &'a [u8],
}

impl<'a> NamedPythGraphManifest<'a> {
    pub const fn name(&self) -> &'a [u8] {
        self.name
    }

    pub const fn principal_id(&self) -> u64 {
        self.principal_id
    }

    pub const fn package_digest(&self) -> u64 {
        self.package_digest
    }

    pub const fn package(&self) -> &'a [u8] {
        self.package
    }
}

pub fn encode_named_pyth_graph(
    out: &mut [u8],
    name: &[u8],
    principal_id: u64,
    package: &[u8],
) -> Result<usize, PythGraphManifestError> {
    if name.len() > MAX_NAMED_PYTH_GRAPH_NAME_LEN {
        return Err(PythGraphManifestError::NameTooLong);
    }
    let package_len_u32 =
        u32::try_from(package.len()).map_err(|_| PythGraphManifestError::LengthOverflow)?;
    let total = NAMED_PYTH_GRAPH_HEADER_LEN
        .checked_add(name.len())
        .and_then(|value| value.checked_add(package.len()))
        .ok_or(PythGraphManifestError::LengthOverflow)?;
    if out.len() < total {
        return Err(PythGraphManifestError::OutputTooSmall);
    }

    out[..8].copy_from_slice(NAMED_PYTH_GRAPH_MAGIC);
    out[8..10].copy_from_slice(&NAMED_PYTH_GRAPH_MAJOR.to_le_bytes());
    out[10..12].copy_from_slice(&NAMED_PYTH_GRAPH_MINOR.to_le_bytes());
    out[12..14].copy_from_slice(&(name.len() as u16).to_le_bytes());
    out[14..16].copy_from_slice(&0u16.to_le_bytes());
    out[16..24].copy_from_slice(&principal_id.to_le_bytes());
    out[24..32].copy_from_slice(&digest64(package).to_le_bytes());
    out[32..36].copy_from_slice(&package_len_u32.to_le_bytes());
    out[36..40].copy_from_slice(&0u32.to_le_bytes());
    out[NAMED_PYTH_GRAPH_HEADER_LEN..NAMED_PYTH_GRAPH_HEADER_LEN + name.len()]
        .copy_from_slice(name);
    out[NAMED_PYTH_GRAPH_HEADER_LEN + name.len()..total].copy_from_slice(package);
    Ok(total)
}

pub fn validate_named_pyth_graph(
    bytes: &[u8],
) -> Result<NamedPythGraphManifest<'_>, PythGraphManifestError> {
    if bytes.len() < NAMED_PYTH_GRAPH_HEADER_LEN {
        return Err(PythGraphManifestError::TooShort);
    }
    if &bytes[..8] != NAMED_PYTH_GRAPH_MAGIC {
        return Err(PythGraphManifestError::BadMagic);
    }

    let major = u16::from_le_bytes([bytes[8], bytes[9]]);
    let minor = u16::from_le_bytes([bytes[10], bytes[11]]);
    if major != NAMED_PYTH_GRAPH_MAJOR || minor != NAMED_PYTH_GRAPH_MINOR {
        return Err(PythGraphManifestError::UnsupportedVersion);
    }
    let name_len = u16::from_le_bytes([bytes[12], bytes[13]]) as usize;
    let reserved0 = u16::from_le_bytes([bytes[14], bytes[15]]);
    let principal_id = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let package_digest = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
    let package_len = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
    let reserved1 = u32::from_le_bytes(bytes[36..40].try_into().unwrap());
    if reserved0 != 0 || reserved1 != 0 {
        return Err(PythGraphManifestError::UnsupportedVersion);
    }
    if name_len > MAX_NAMED_PYTH_GRAPH_NAME_LEN {
        return Err(PythGraphManifestError::NameTooLong);
    }

    let name_start = NAMED_PYTH_GRAPH_HEADER_LEN;
    let name_end = name_start
        .checked_add(name_len)
        .ok_or(PythGraphManifestError::LengthOverflow)?;
    let package_end = name_end
        .checked_add(package_len)
        .ok_or(PythGraphManifestError::LengthOverflow)?;
    if bytes.len() != package_end {
        return Err(PythGraphManifestError::TooShort);
    }

    let name = &bytes[name_start..name_end];
    let package = &bytes[name_end..package_end];
    if digest64(package) != package_digest {
        return Err(PythGraphManifestError::BadDigest);
    }

    Ok(NamedPythGraphManifest {
        name,
        principal_id,
        package_digest,
        package,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_manifest_round_trips_name_principal_digest_and_package() {
        let package = b"PYTHTIG1fixture";
        let mut output = [0u8; 256];
        let len =
            encode_named_pyth_graph(&mut output, b"hello.tig", 0x5059_5448_4752_0001, package)
                .unwrap();

        let manifest = validate_named_pyth_graph(&output[..len]).unwrap();

        assert_eq!(manifest.name(), b"hello.tig");
        assert_eq!(manifest.principal_id(), 0x5059_5448_4752_0001);
        assert_eq!(manifest.package(), package);
        assert_eq!(manifest.package_digest(), digest64(package));
    }
}
