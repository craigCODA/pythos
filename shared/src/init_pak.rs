pub const INIT_PAK_MAGIC: &[u8; 18] = b"PYTHOS_INIT_PAK_V0";
pub const INIT_PAK_MAJOR: u16 = 0;
pub const INIT_PAK_MINOR: u16 = 0;
pub const INIT_PAK_HEADER_LEN: u32 = 64;
const RESERVED_OFFSET: usize = 46;
const RESERVED_LEN: usize = 18;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitPakError {
    TooShort,
    BadMagic,
    UnsupportedMajor,
    BadHeaderLength,
    LengthOverflow,
    BadTotalLength,
    NonZeroReserved,
    BadChecksum,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitPakHeader {
    pub major: u16,
    pub minor: u16,
    pub header_len: u32,
    pub total_len: u64,
    pub payload_len: u64,
    pub payload_checksum: u32,
}

pub fn validate(bytes: &[u8]) -> Result<InitPakHeader, InitPakError> {
    if bytes.len() < INIT_PAK_HEADER_LEN as usize {
        return Err(InitPakError::TooShort);
    }
    if &bytes[..INIT_PAK_MAGIC.len()] != INIT_PAK_MAGIC {
        return Err(InitPakError::BadMagic);
    }

    let major = read_u16(bytes, 18)?;
    let minor = read_u16(bytes, 20)?;
    let header_len = read_u32(bytes, 22)?;
    let total_len = read_u64(bytes, 26)?;
    let payload_len = read_u64(bytes, 34)?;
    let payload_checksum = read_u32(bytes, 42)?;

    if major != INIT_PAK_MAJOR {
        return Err(InitPakError::UnsupportedMajor);
    }
    if header_len != INIT_PAK_HEADER_LEN || (header_len as usize) > bytes.len() {
        return Err(InitPakError::BadHeaderLength);
    }
    let expected_total = u64::from(header_len)
        .checked_add(payload_len)
        .ok_or(InitPakError::LengthOverflow)?;
    if total_len != expected_total || total_len as usize != bytes.len() {
        return Err(InitPakError::BadTotalLength);
    }
    if bytes[RESERVED_OFFSET..RESERVED_OFFSET + RESERVED_LEN]
        .iter()
        .any(|&byte| byte != 0)
    {
        return Err(InitPakError::NonZeroReserved);
    }

    let payload_start = header_len as usize;
    let payload_end = payload_start
        .checked_add(payload_len as usize)
        .ok_or(InitPakError::LengthOverflow)?;
    let payload = bytes
        .get(payload_start..payload_end)
        .ok_or(InitPakError::BadTotalLength)?;
    if checksum(payload) != payload_checksum {
        return Err(InitPakError::BadChecksum);
    }

    Ok(InitPakHeader {
        major,
        minor,
        header_len,
        total_len,
        payload_len,
        payload_checksum,
    })
}

pub fn checksum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |sum, &byte| sum.wrapping_add(u32::from(byte)))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, InitPakError> {
    let raw = bytes
        .get(offset..offset + 2)
        .ok_or(InitPakError::TooShort)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, InitPakError> {
    let raw = bytes
        .get(offset..offset + 4)
        .ok_or(InitPakError::TooShort)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, InitPakError> {
    let raw = bytes
        .get(offset..offset + 8)
        .ok_or(InitPakError::TooShort)?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_bundle(payload: &[u8]) -> [u8; INIT_PAK_HEADER_LEN as usize] {
        assert!(payload.is_empty());
        let mut bytes = [0u8; INIT_PAK_HEADER_LEN as usize];
        bytes[..INIT_PAK_MAGIC.len()].copy_from_slice(INIT_PAK_MAGIC);
        bytes[18..20].copy_from_slice(&INIT_PAK_MAJOR.to_le_bytes());
        bytes[20..22].copy_from_slice(&INIT_PAK_MINOR.to_le_bytes());
        bytes[22..26].copy_from_slice(&INIT_PAK_HEADER_LEN.to_le_bytes());
        bytes[26..34].copy_from_slice(&(INIT_PAK_HEADER_LEN as u64).to_le_bytes());
        bytes[34..42].copy_from_slice(&0u64.to_le_bytes());
        bytes[42..46].copy_from_slice(&checksum(payload).to_le_bytes());
        bytes
    }

    #[test]
    fn valid_empty_bundle_passes() {
        let bytes = valid_bundle(&[]);

        let header = validate(&bytes).unwrap();

        assert_eq!(header.major, INIT_PAK_MAJOR);
        assert_eq!(header.header_len, INIT_PAK_HEADER_LEN);
        assert_eq!(header.total_len, u64::from(INIT_PAK_HEADER_LEN));
        assert_eq!(header.payload_len, 0);
    }

    #[test]
    fn nonzero_reserved_bytes_are_rejected() {
        let mut bytes = valid_bundle(&[]);
        bytes[RESERVED_OFFSET] = 1;

        assert_eq!(validate(&bytes), Err(InitPakError::NonZeroReserved));
    }

    #[test]
    fn mismatched_total_length_is_rejected() {
        let mut bytes = valid_bundle(&[]);
        bytes[26..34].copy_from_slice(&65u64.to_le_bytes());

        assert_eq!(validate(&bytes), Err(InitPakError::BadTotalLength));
    }
}
