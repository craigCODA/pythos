//! Versioned named user-program manifest (ADR 0051/0052).
//!
//! Bundles a user ELF with its stable program name, a principal id PythCore
//! policy binds only when the loader-validated name/digest match (identity
//! binding for the trusted boot bundle, not a general code-signing chain),
//! and an integrity digest over the ELF bytes.
//!
//! Layout (little-endian, all offsets from the start of the manifest):
//!
//! ```text
//! 0..8    magic            b"PYUPGM01"
//! 8..10   major            u16
//! 10..12  minor            u16
//! 12..14  name_len         u16
//! 14..16  reserved0        u16 (must be zero)
//! 16..24  principal_id     u64
//! 24..32  elf_digest       u64 (digest64 of the ELF bytes)
//! 32..36  elf_len          u32
//! 36..40  reserved1        u32 (must be zero)
//! 40..40+name_len          name bytes
//! 40+name_len..+elf_len    ELF bytes
//! ```

pub const NAMED_USER_PROGRAM_MAGIC: &[u8; 8] = b"PYUPGM01";
pub const NAMED_USER_PROGRAM_MAJOR: u16 = 1;
pub const NAMED_USER_PROGRAM_MINOR: u16 = 0;
pub const NAMED_USER_PROGRAM_HEADER_LEN: usize = 40;
pub const MAX_NAMED_PROGRAM_NAME_LEN: usize = 32;

pub const SHELL_PRINCIPAL_ID: u64 = 0x5059_5348_454C_4C01;
pub const INTRUDER_PRINCIPAL_ID: u64 = 0x5059_494E_5452_4401;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserProgramManifestError {
    TooShort,
    BadMagic,
    UnsupportedVersion,
    NameTooLong,
    LengthOverflow,
    BadDigest,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NamedUserProgramManifest<'a> {
    name: &'a [u8],
    principal_id: u64,
    elf_digest: u64,
    elf: &'a [u8],
}

impl<'a> NamedUserProgramManifest<'a> {
    pub const fn name(&self) -> &'a [u8] {
        self.name
    }

    pub const fn principal_id(&self) -> u64 {
        self.principal_id
    }

    pub const fn elf_digest(&self) -> u64 {
        self.elf_digest
    }

    pub const fn elf(&self) -> &'a [u8] {
        self.elf
    }
}

/// FNV-1a 64-bit digest. An integrity binding for the trusted boot bundle
/// (detects corruption/mismatch), not a cryptographic signature.
pub fn digest64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut hash = FNV_OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Encode a named user-program manifest into `out`, returning the total
/// encoded length.
pub fn encode_named_user_program(
    out: &mut [u8],
    name: &[u8],
    principal_id: u64,
    elf: &[u8],
) -> Result<usize, UserProgramManifestError> {
    if name.len() > MAX_NAMED_PROGRAM_NAME_LEN {
        return Err(UserProgramManifestError::NameTooLong);
    }
    let name_len = name.len();
    let elf_len_u32 =
        u32::try_from(elf.len()).map_err(|_| UserProgramManifestError::LengthOverflow)?;
    let total = NAMED_USER_PROGRAM_HEADER_LEN
        .checked_add(name_len)
        .and_then(|value| value.checked_add(elf.len()))
        .ok_or(UserProgramManifestError::LengthOverflow)?;
    if out.len() < total {
        return Err(UserProgramManifestError::OutputTooSmall);
    }

    let digest = digest64(elf);
    out[0..8].copy_from_slice(NAMED_USER_PROGRAM_MAGIC);
    out[8..10].copy_from_slice(&NAMED_USER_PROGRAM_MAJOR.to_le_bytes());
    out[10..12].copy_from_slice(&NAMED_USER_PROGRAM_MINOR.to_le_bytes());
    out[12..14].copy_from_slice(&(name_len as u16).to_le_bytes());
    out[14..16].copy_from_slice(&0u16.to_le_bytes());
    out[16..24].copy_from_slice(&principal_id.to_le_bytes());
    out[24..32].copy_from_slice(&digest.to_le_bytes());
    out[32..36].copy_from_slice(&elf_len_u32.to_le_bytes());
    out[36..40].copy_from_slice(&0u32.to_le_bytes());
    out[NAMED_USER_PROGRAM_HEADER_LEN..NAMED_USER_PROGRAM_HEADER_LEN + name_len]
        .copy_from_slice(name);
    out[NAMED_USER_PROGRAM_HEADER_LEN + name_len..total].copy_from_slice(elf);
    Ok(total)
}

/// Validate and parse a named user-program manifest.
pub fn validate_named_user_program(
    bytes: &[u8],
) -> Result<NamedUserProgramManifest<'_>, UserProgramManifestError> {
    if bytes.len() < NAMED_USER_PROGRAM_HEADER_LEN {
        return Err(UserProgramManifestError::TooShort);
    }
    if &bytes[0..8] != NAMED_USER_PROGRAM_MAGIC {
        return Err(UserProgramManifestError::BadMagic);
    }
    let major = u16::from_le_bytes([bytes[8], bytes[9]]);
    let minor = u16::from_le_bytes([bytes[10], bytes[11]]);
    // Exact match for this first version of the ABI: simplest correct policy,
    // and there is no defined meaning yet for "newer/older but compatible".
    if major != NAMED_USER_PROGRAM_MAJOR || minor != NAMED_USER_PROGRAM_MINOR {
        return Err(UserProgramManifestError::UnsupportedVersion);
    }
    let name_len = u16::from_le_bytes([bytes[12], bytes[13]]) as usize;
    let reserved0 = u16::from_le_bytes([bytes[14], bytes[15]]);
    let principal_id = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let elf_digest = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
    let elf_len = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
    let reserved1 = u32::from_le_bytes(bytes[36..40].try_into().unwrap());
    // Reject nonzero reserved fields: a forward-compatibility signal to not
    // silently ignore, not a distinct error case in this ABI's error set, so
    // it is reported as an unsupported (newer, unknown) manifest version.
    if reserved0 != 0 || reserved1 != 0 {
        return Err(UserProgramManifestError::UnsupportedVersion);
    }
    if name_len > MAX_NAMED_PROGRAM_NAME_LEN {
        return Err(UserProgramManifestError::NameTooLong);
    }

    let name_start = NAMED_USER_PROGRAM_HEADER_LEN;
    let name_end = name_start
        .checked_add(name_len)
        .ok_or(UserProgramManifestError::LengthOverflow)?;
    let elf_end = name_end
        .checked_add(elf_len)
        .ok_or(UserProgramManifestError::LengthOverflow)?;
    if bytes.len() != elf_end {
        return Err(UserProgramManifestError::TooShort);
    }

    let name = &bytes[name_start..name_end];
    let elf = &bytes[name_end..elf_end];
    if digest64(elf) != elf_digest {
        return Err(UserProgramManifestError::BadDigest);
    }

    Ok(NamedUserProgramManifest {
        name,
        principal_id,
        elf_digest,
        elf,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_manifest_round_trips_identity_digest_and_elf() {
        let mut bytes = [0u8; 96];
        let len = encode_named_user_program(
            &mut bytes,
            b"shell.elf",
            0x5059_5348_454C_4C01,
            b"\x7FELFpayload",
        )
        .unwrap();
        let manifest = validate_named_user_program(&bytes[..len]).unwrap();

        assert_eq!(manifest.name(), b"shell.elf");
        assert_eq!(manifest.principal_id(), 0x5059_5348_454C_4C01);
        assert_eq!(manifest.elf(), b"\x7FELFpayload");
        assert_eq!(manifest.elf_digest(), digest64(b"\x7FELFpayload"));
    }

    #[test]
    fn name_too_long_is_rejected() {
        let mut bytes = [0u8; 128];
        let long_name = [b'a'; MAX_NAMED_PROGRAM_NAME_LEN + 1];
        assert_eq!(
            encode_named_user_program(&mut bytes, &long_name, 1, b"elf"),
            Err(UserProgramManifestError::NameTooLong)
        );
    }

    #[test]
    fn mismatched_minor_version_is_rejected() {
        let mut bytes = [0u8; 96];
        let len = encode_named_user_program(&mut bytes, b"shell.elf", 1, b"elf").unwrap();
        bytes[10..12].copy_from_slice(&(NAMED_USER_PROGRAM_MINOR + 1).to_le_bytes());
        assert_eq!(
            validate_named_user_program(&bytes[..len]),
            Err(UserProgramManifestError::UnsupportedVersion)
        );
    }

    #[test]
    fn output_buffer_too_small_is_rejected() {
        let mut bytes = [0u8; 10];
        assert_eq!(
            encode_named_user_program(&mut bytes, b"shell.elf", 1, b"elf"),
            Err(UserProgramManifestError::OutputTooSmall)
        );
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = [0u8; 96];
        let len = encode_named_user_program(&mut bytes, b"shell.elf", 1, b"elf").unwrap();
        bytes[0] ^= 0xFF;
        assert_eq!(
            validate_named_user_program(&bytes[..len]),
            Err(UserProgramManifestError::BadMagic)
        );
    }

    #[test]
    fn tampered_elf_bytes_fail_digest_check() {
        let mut bytes = [0u8; 96];
        let len = encode_named_user_program(&mut bytes, b"shell.elf", 1, b"elf").unwrap();
        let last = len - 1;
        bytes[last] ^= 0xFF;
        assert_eq!(
            validate_named_user_program(&bytes[..len]),
            Err(UserProgramManifestError::BadDigest)
        );
    }

    #[test]
    fn truncated_manifest_is_too_short() {
        let mut bytes = [0u8; 96];
        let len = encode_named_user_program(&mut bytes, b"shell.elf", 1, b"elf").unwrap();
        assert_eq!(
            validate_named_user_program(&bytes[..len - 1]),
            Err(UserProgramManifestError::TooShort)
        );
    }
}
