//! PythTIG native graph-to-ELF binding payload (ADR 0037 / ADR 0064).
//!
//! This is integrity metadata for the trusted boot bundle. It is not a
//! signature and does not grant launch authority by itself.

use crate::{
    pyth_graph_manifest::MAX_NAMED_PYTH_GRAPH_NAME_LEN,
    user_program_manifest::{MAX_NAMED_PROGRAM_NAME_LEN, digest64},
};

pub const PYTH_NATIVE_BINDING_MAGIC: &[u8; 8] = b"PYTNAT01";
pub const PYTH_NATIVE_BINDING_MAJOR: u16 = 1;
pub const PYTH_NATIVE_BINDING_MINOR: u16 = 0;
pub const PYTH_NATIVE_BINDING_HEADER_LEN: usize = 48;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythNativeBindingError {
    TooShort,
    BadMagic,
    UnsupportedVersion,
    GraphNameTooLong,
    ElfNameTooLong,
    LengthOverflow,
    BadGraphDigest,
    BadElfDigest,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythNativeBinding<'a> {
    graph_name: &'a [u8],
    elf_name: &'a [u8],
    principal_id: u64,
    graph_digest: u64,
    elf_digest: u64,
}

impl<'a> PythNativeBinding<'a> {
    pub const fn graph_name(&self) -> &'a [u8] {
        self.graph_name
    }

    pub const fn elf_name(&self) -> &'a [u8] {
        self.elf_name
    }

    pub const fn principal_id(&self) -> u64 {
        self.principal_id
    }

    pub const fn graph_digest(&self) -> u64 {
        self.graph_digest
    }

    pub const fn elf_digest(&self) -> u64 {
        self.elf_digest
    }
}

pub fn encode_pyth_native_binding(
    out: &mut [u8],
    graph_name: &[u8],
    elf_name: &[u8],
    principal_id: u64,
    graph: &[u8],
    elf: &[u8],
) -> Result<usize, PythNativeBindingError> {
    if graph_name.len() > MAX_NAMED_PYTH_GRAPH_NAME_LEN {
        return Err(PythNativeBindingError::GraphNameTooLong);
    }
    if elf_name.len() > MAX_NAMED_PROGRAM_NAME_LEN {
        return Err(PythNativeBindingError::ElfNameTooLong);
    }
    let total = PYTH_NATIVE_BINDING_HEADER_LEN
        .checked_add(graph_name.len())
        .and_then(|value| value.checked_add(elf_name.len()))
        .ok_or(PythNativeBindingError::LengthOverflow)?;
    if out.len() < total {
        return Err(PythNativeBindingError::OutputTooSmall);
    }

    out[..8].copy_from_slice(PYTH_NATIVE_BINDING_MAGIC);
    out[8..10].copy_from_slice(&PYTH_NATIVE_BINDING_MAJOR.to_le_bytes());
    out[10..12].copy_from_slice(&PYTH_NATIVE_BINDING_MINOR.to_le_bytes());
    out[12..14].copy_from_slice(&(graph_name.len() as u16).to_le_bytes());
    out[14..16].copy_from_slice(&(elf_name.len() as u16).to_le_bytes());
    out[16..24].copy_from_slice(&principal_id.to_le_bytes());
    out[24..32].copy_from_slice(&digest64(graph).to_le_bytes());
    out[32..40].copy_from_slice(&digest64(elf).to_le_bytes());
    out[40..48].copy_from_slice(&0u64.to_le_bytes());
    out[PYTH_NATIVE_BINDING_HEADER_LEN..PYTH_NATIVE_BINDING_HEADER_LEN + graph_name.len()]
        .copy_from_slice(graph_name);
    out[PYTH_NATIVE_BINDING_HEADER_LEN + graph_name.len()..total].copy_from_slice(elf_name);
    Ok(total)
}

pub fn validate_pyth_native_binding(
    bytes: &[u8],
) -> Result<PythNativeBinding<'_>, PythNativeBindingError> {
    if bytes.len() < PYTH_NATIVE_BINDING_HEADER_LEN {
        return Err(PythNativeBindingError::TooShort);
    }
    if &bytes[..8] != PYTH_NATIVE_BINDING_MAGIC {
        return Err(PythNativeBindingError::BadMagic);
    }
    let major = u16::from_le_bytes([bytes[8], bytes[9]]);
    let minor = u16::from_le_bytes([bytes[10], bytes[11]]);
    if major != PYTH_NATIVE_BINDING_MAJOR || minor != PYTH_NATIVE_BINDING_MINOR {
        return Err(PythNativeBindingError::UnsupportedVersion);
    }
    let graph_name_len = u16::from_le_bytes([bytes[12], bytes[13]]) as usize;
    let elf_name_len = u16::from_le_bytes([bytes[14], bytes[15]]) as usize;
    if graph_name_len > MAX_NAMED_PYTH_GRAPH_NAME_LEN {
        return Err(PythNativeBindingError::GraphNameTooLong);
    }
    if elf_name_len > MAX_NAMED_PROGRAM_NAME_LEN {
        return Err(PythNativeBindingError::ElfNameTooLong);
    }
    if u64::from_le_bytes(bytes[40..48].try_into().unwrap()) != 0 {
        return Err(PythNativeBindingError::UnsupportedVersion);
    }
    let names_end = PYTH_NATIVE_BINDING_HEADER_LEN
        .checked_add(graph_name_len)
        .and_then(|value| value.checked_add(elf_name_len))
        .ok_or(PythNativeBindingError::LengthOverflow)?;
    if bytes.len() != names_end {
        return Err(PythNativeBindingError::TooShort);
    }

    let principal_id = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let graph_digest = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
    let elf_digest = u64::from_le_bytes(bytes[32..40].try_into().unwrap());
    let graph_name_start = PYTH_NATIVE_BINDING_HEADER_LEN;
    let graph_name_end = graph_name_start + graph_name_len;
    let graph_name = &bytes[graph_name_start..graph_name_end];
    let elf_name = &bytes[graph_name_end..names_end];

    Ok(PythNativeBinding {
        graph_name,
        elf_name,
        principal_id,
        graph_digest,
        elf_digest,
    })
}

pub fn validate_pyth_native_binding_for_artifacts<'a>(
    bytes: &'a [u8],
    graph_name: &[u8],
    elf_name: &[u8],
    principal_id: u64,
    graph: &[u8],
    elf: &[u8],
) -> Result<PythNativeBinding<'a>, PythNativeBindingError> {
    let binding = validate_pyth_native_binding(bytes)?;
    if binding.graph_name != graph_name {
        return Err(PythNativeBindingError::BadGraphDigest);
    }
    if binding.elf_name != elf_name || binding.principal_id != principal_id {
        return Err(PythNativeBindingError::BadElfDigest);
    }
    if binding.graph_digest != digest64(graph) {
        return Err(PythNativeBindingError::BadGraphDigest);
    }
    if binding.elf_digest != digest64(elf) {
        return Err(PythNativeBindingError::BadElfDigest);
    }
    Ok(binding)
}
