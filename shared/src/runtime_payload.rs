pub const RUNTIME_PAYLOAD_MAGIC: &[u8; 16] = b"PYTHOS_MINRT_V00";
pub const RUNTIME_PAYLOAD_MAJOR: u16 = 0;
pub const RUNTIME_PAYLOAD_MINOR: u16 = 0;
pub const RUNTIME_PAYLOAD_HEADER_LEN: u32 = 32;
pub const MAX_RUNTIME_SOURCE_LEN: u32 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimePayloadError {
    TooShort,
    BadMagic,
    UnsupportedMajor,
    BadHeaderLength,
    LengthOverflow,
    BadTotalLength,
    EmptySource,
    SourceTooLarge,
    BadChecksum,
    BadUtf8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimePayloadHeader {
    pub major: u16,
    pub minor: u16,
    pub header_len: u32,
    pub source_len: u32,
    pub source_checksum: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimePayload<'a> {
    pub header: RuntimePayloadHeader,
    pub source: &'a str,
}

pub fn validate(bytes: &[u8]) -> Result<RuntimePayload<'_>, RuntimePayloadError> {
    if bytes.len() < RUNTIME_PAYLOAD_HEADER_LEN as usize {
        return Err(RuntimePayloadError::TooShort);
    }
    if &bytes[..RUNTIME_PAYLOAD_MAGIC.len()] != RUNTIME_PAYLOAD_MAGIC {
        return Err(RuntimePayloadError::BadMagic);
    }

    let major = read_u16(bytes, 16)?;
    let minor = read_u16(bytes, 18)?;
    let header_len = read_u32(bytes, 20)?;
    let source_len = read_u32(bytes, 24)?;
    let source_checksum = read_u32(bytes, 28)?;

    if major != RUNTIME_PAYLOAD_MAJOR {
        return Err(RuntimePayloadError::UnsupportedMajor);
    }
    if header_len != RUNTIME_PAYLOAD_HEADER_LEN || header_len as usize > bytes.len() {
        return Err(RuntimePayloadError::BadHeaderLength);
    }
    if source_len == 0 {
        return Err(RuntimePayloadError::EmptySource);
    }
    if source_len > MAX_RUNTIME_SOURCE_LEN {
        return Err(RuntimePayloadError::SourceTooLarge);
    }

    let total_len = header_len
        .checked_add(source_len)
        .ok_or(RuntimePayloadError::LengthOverflow)?;
    if total_len as usize != bytes.len() {
        return Err(RuntimePayloadError::BadTotalLength);
    }

    let source_start = header_len as usize;
    let source_end = source_start
        .checked_add(source_len as usize)
        .ok_or(RuntimePayloadError::LengthOverflow)?;
    let source_bytes = bytes
        .get(source_start..source_end)
        .ok_or(RuntimePayloadError::BadTotalLength)?;
    if checksum(source_bytes) != source_checksum {
        return Err(RuntimePayloadError::BadChecksum);
    }
    let source = core::str::from_utf8(source_bytes).map_err(|_| RuntimePayloadError::BadUtf8)?;

    Ok(RuntimePayload {
        header: RuntimePayloadHeader {
            major,
            minor,
            header_len,
            source_len,
            source_checksum,
        },
        source,
    })
}

pub fn checksum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |sum, &byte| sum.wrapping_add(u32::from(byte)))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, RuntimePayloadError> {
    let raw = bytes
        .get(offset..offset + 2)
        .ok_or(RuntimePayloadError::TooShort)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, RuntimePayloadError> {
    let raw = bytes
        .get(offset..offset + 4)
        .ok_or(RuntimePayloadError::TooShort)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::vec;
    use std::vec::Vec;

    const HELLO_SERVICE: &[u8] = b"class HelloService(Service):\n    async def start(self):\n        system.log(\"hello from Python\")\n        self.ready()\n";

    fn bundle(source: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; RUNTIME_PAYLOAD_HEADER_LEN as usize];
        bytes[..RUNTIME_PAYLOAD_MAGIC.len()].copy_from_slice(RUNTIME_PAYLOAD_MAGIC);
        bytes[16..18].copy_from_slice(&RUNTIME_PAYLOAD_MAJOR.to_le_bytes());
        bytes[18..20].copy_from_slice(&RUNTIME_PAYLOAD_MINOR.to_le_bytes());
        bytes[20..24].copy_from_slice(&RUNTIME_PAYLOAD_HEADER_LEN.to_le_bytes());
        bytes[24..28].copy_from_slice(&(source.len() as u32).to_le_bytes());
        bytes[28..32].copy_from_slice(&checksum(source).to_le_bytes());
        bytes.extend_from_slice(source);
        bytes
    }

    #[test]
    fn valid_minimal_service_source_passes() {
        let bytes = bundle(HELLO_SERVICE);
        let payload = validate(&bytes).unwrap();

        assert_eq!(payload.header.major, RUNTIME_PAYLOAD_MAJOR);
        assert_eq!(payload.header.source_len, HELLO_SERVICE.len() as u32);
        assert!(payload.source.contains("HelloService"));
    }

    #[test]
    fn empty_source_is_rejected() {
        assert_eq!(
            validate(&bundle(&[])),
            Err(RuntimePayloadError::EmptySource)
        );
    }

    #[test]
    fn bad_checksum_is_rejected() {
        let mut bytes = bundle(HELLO_SERVICE);
        bytes[28] ^= 0xFF;

        assert_eq!(validate(&bytes), Err(RuntimePayloadError::BadChecksum));
    }

    #[test]
    fn non_utf8_source_is_rejected() {
        let bytes = bundle(&[0xFF]);

        assert_eq!(validate(&bytes), Err(RuntimePayloadError::BadUtf8));
    }

    #[test]
    fn mismatched_total_length_is_rejected() {
        let mut bytes = bundle(HELLO_SERVICE);
        bytes.pop();

        assert_eq!(validate(&bytes), Err(RuntimePayloadError::BadTotalLength));
    }
}
