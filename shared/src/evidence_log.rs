pub const EVIDENCE_LOG_TOTAL_BYTES: usize = 64 * 1024;
pub const EVIDENCE_LOG_MAGIC: [u8; 8] = *b"PYLOG001";
pub const EVIDENCE_LOG_VERSION: u32 = 1;
pub const MAX_EVIDENCE_LINE_BYTES: usize = 128;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceLogHeader {
    pub magic: [u8; 8],
    pub version: u32,
    pub capacity: u32,
    pub used: u32,
    pub lines: u32,
    pub dropped: u32,
    pub crc32: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceLogError {
    InvalidBufferLength,
    InvalidHeader,
    LengthOverflow,
    LineTooLong,
    EmbeddedLineBreak,
    NonAscii,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceLogSnapshot<'a> {
    pub header: EvidenceLogHeader,
    pub payload: &'a [u8],
}

pub fn initialize(buffer: &mut [u8]) -> Result<(), EvidenceLogError> {
    ensure_exact_buffer_length(buffer.len())?;
    buffer.fill(0);
    write_header(
        buffer,
        EvidenceLogHeader {
            magic: EVIDENCE_LOG_MAGIC,
            version: EVIDENCE_LOG_VERSION,
            capacity: payload_capacity() as u32,
            used: 0,
            lines: 0,
            dropped: 0,
            crc32: 0,
        },
    );
    Ok(())
}

pub fn append_line(buffer: &mut [u8], line: &str) -> Result<(), EvidenceLogError> {
    ensure_exact_buffer_length(buffer.len())?;
    if !line.is_ascii() {
        return Err(EvidenceLogError::NonAscii);
    }
    if line.bytes().any(|byte| byte == b'\n' || byte == b'\r') {
        return Err(EvidenceLogError::EmbeddedLineBreak);
    }
    if line.len() > MAX_EVIDENCE_LINE_BYTES {
        return Err(EvidenceLogError::LineTooLong);
    }

    let mut header = read_header(buffer)?;
    let payload = payload_mut(buffer);
    let needed = line
        .len()
        .checked_add(1)
        .ok_or(EvidenceLogError::LengthOverflow)?;
    let needed_u32 = u32::try_from(needed).map_err(|_| EvidenceLogError::LengthOverflow)?;

    let new_used = header
        .used
        .checked_add(needed_u32)
        .ok_or(EvidenceLogError::LengthOverflow)?;
    if new_used > header.capacity {
        header.dropped = header.dropped.saturating_add(1);
        write_header(buffer, header);
        return Err(EvidenceLogError::Full);
    }

    let start = header.used as usize;
    let end = start + needed;
    payload[start..start + line.len()].copy_from_slice(line.as_bytes());
    payload[start + line.len()] = b'\n';
    header.used = new_used;
    header.lines = header.lines.saturating_add(1);
    header.crc32 = crc32_iso_hdlc(&payload[..end]);
    write_header(buffer, header);
    Ok(())
}

pub fn snapshot(buffer: &[u8]) -> Result<EvidenceLogSnapshot<'_>, EvidenceLogError> {
    ensure_exact_buffer_length(buffer.len())?;
    let header = read_header(buffer)?;
    validate_header(&header)?;

    let payload = payload(buffer);
    let used = header.used as usize;
    if used > payload.len() {
        return Err(EvidenceLogError::InvalidHeader);
    }
    if crc32_iso_hdlc(&payload[..used]) != header.crc32 {
        return Err(EvidenceLogError::InvalidHeader);
    }

    Ok(EvidenceLogSnapshot {
        header,
        payload: &payload[..used],
    })
}

pub fn crc32_iso_hdlc(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn ensure_exact_buffer_length(length: usize) -> Result<(), EvidenceLogError> {
    if length == EVIDENCE_LOG_TOTAL_BYTES {
        Ok(())
    } else {
        Err(EvidenceLogError::InvalidBufferLength)
    }
}

fn payload_capacity() -> usize {
    EVIDENCE_LOG_TOTAL_BYTES - core::mem::size_of::<EvidenceLogHeader>()
}

fn validate_header(header: &EvidenceLogHeader) -> Result<(), EvidenceLogError> {
    if header.magic != EVIDENCE_LOG_MAGIC
        || header.version != EVIDENCE_LOG_VERSION
        || header.capacity as usize != payload_capacity()
    {
        return Err(EvidenceLogError::InvalidHeader);
    }
    if header.used > header.capacity {
        return Err(EvidenceLogError::InvalidHeader);
    }
    Ok(())
}

fn read_header(buffer: &[u8]) -> Result<EvidenceLogHeader, EvidenceLogError> {
    let header_bytes = buffer
        .get(..core::mem::size_of::<EvidenceLogHeader>())
        .ok_or(EvidenceLogError::InvalidHeader)?;
    Ok(EvidenceLogHeader {
        magic: header_bytes[0..8].try_into().expect("slice length is fixed"),
        version: u32::from_le_bytes(header_bytes[8..12].try_into().expect("slice length is fixed")),
        capacity: u32::from_le_bytes(
            header_bytes[12..16].try_into().expect("slice length is fixed"),
        ),
        used: u32::from_le_bytes(header_bytes[16..20].try_into().expect("slice length is fixed")),
        lines: u32::from_le_bytes(header_bytes[20..24].try_into().expect("slice length is fixed")),
        dropped: u32::from_le_bytes(
            header_bytes[24..28].try_into().expect("slice length is fixed"),
        ),
        crc32: u32::from_le_bytes(header_bytes[28..32].try_into().expect("slice length is fixed")),
    })
}

fn write_header(buffer: &mut [u8], header: EvidenceLogHeader) {
    let header_bytes = &mut buffer[..core::mem::size_of::<EvidenceLogHeader>()];
    header_bytes[0..8].copy_from_slice(&header.magic);
    header_bytes[8..12].copy_from_slice(&header.version.to_le_bytes());
    header_bytes[12..16].copy_from_slice(&header.capacity.to_le_bytes());
    header_bytes[16..20].copy_from_slice(&header.used.to_le_bytes());
    header_bytes[20..24].copy_from_slice(&header.lines.to_le_bytes());
    header_bytes[24..28].copy_from_slice(&header.dropped.to_le_bytes());
    header_bytes[28..32].copy_from_slice(&header.crc32.to_le_bytes());
}

fn payload(buffer: &[u8]) -> &[u8] {
    &buffer[core::mem::size_of::<EvidenceLogHeader>()..]
}

fn payload_mut(buffer: &mut [u8]) -> &mut [u8] {
    &mut buffer[core::mem::size_of::<EvidenceLogHeader>()..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_header_and_empty_payload() {
        let mut buffer = [0xA5u8; EVIDENCE_LOG_TOTAL_BYTES];
        initialize(&mut buffer).unwrap();

        let snapshot = snapshot(&buffer).unwrap();

        assert_eq!(snapshot.header.magic, EVIDENCE_LOG_MAGIC);
        assert_eq!(snapshot.header.version, EVIDENCE_LOG_VERSION);
        assert_eq!(
            snapshot.header.capacity as usize,
            EVIDENCE_LOG_TOTAL_BYTES - core::mem::size_of::<EvidenceLogHeader>()
        );
        assert_eq!(snapshot.header.used, 0);
        assert_eq!(snapshot.header.lines, 0);
        assert_eq!(snapshot.header.dropped, 0);
        assert_eq!(snapshot.header.crc32, 0);
        assert_eq!(snapshot.payload, b"");
    }

    #[test]
    fn initialize_rejects_short_and_long_buffers() {
        let mut short_buffer = [0u8; EVIDENCE_LOG_TOTAL_BYTES - 1];
        let mut long_buffer = [0u8; EVIDENCE_LOG_TOTAL_BYTES + 1];

        assert_eq!(
            initialize(&mut short_buffer),
            Err(EvidenceLogError::InvalidBufferLength)
        );
        assert_eq!(
            initialize(&mut long_buffer),
            Err(EvidenceLogError::InvalidBufferLength)
        );
    }

    #[test]
    fn append_line_rejects_short_and_long_buffers() {
        let mut short_buffer = [0u8; EVIDENCE_LOG_TOTAL_BYTES - 1];
        let mut long_buffer = [0u8; EVIDENCE_LOG_TOTAL_BYTES + 1];

        assert_eq!(
            append_line(&mut short_buffer, "PYTHOS:LOADER:ENTER"),
            Err(EvidenceLogError::InvalidBufferLength)
        );
        assert_eq!(
            append_line(&mut long_buffer, "PYTHOS:LOADER:ENTER"),
            Err(EvidenceLogError::InvalidBufferLength)
        );
    }

    #[test]
    fn snapshot_rejects_short_and_long_buffers() {
        let short_buffer = [0u8; EVIDENCE_LOG_TOTAL_BYTES - 1];
        let long_buffer = [0u8; EVIDENCE_LOG_TOTAL_BYTES + 1];

        assert_eq!(
            snapshot(&short_buffer),
            Err(EvidenceLogError::InvalidBufferLength)
        );
        assert_eq!(
            snapshot(&long_buffer),
            Err(EvidenceLogError::InvalidBufferLength)
        );
    }

    #[test]
    fn append_line_rejects_embedded_cr_or_lf() {
        let mut buffer = [0u8; EVIDENCE_LOG_TOTAL_BYTES];
        initialize(&mut buffer).unwrap();

        assert_eq!(
            append_line(&mut buffer, "bad\nline"),
            Err(EvidenceLogError::EmbeddedLineBreak)
        );
        assert_eq!(
            append_line(&mut buffer, "bad\rline"),
            Err(EvidenceLogError::EmbeddedLineBreak)
        );
    }

    #[test]
    fn full_buffer_increments_dropped_without_mutating_payload() {
        let mut buffer = [0u8; EVIDENCE_LOG_TOTAL_BYTES];
        initialize(&mut buffer).unwrap();
        append_line(&mut buffer, "PYTHOS:CORE:PHASE_10_COMPLETE").unwrap();
        let expected_prefix = b"PYTHOS:CORE:PHASE_10_COMPLETE\n";
        let (before_used, before_crc) = {
            let before = snapshot(&buffer).unwrap();
            assert_eq!(&before.payload[..expected_prefix.len()], expected_prefix);
            (before.header.used, before.header.crc32)
        };

        while append_line(&mut buffer, "PYTHOS:CORE:FRAMEBUFFER_READY").is_ok() {}

        let after = snapshot(&buffer).unwrap();
        assert!(after.header.dropped > 0);
        assert_eq!(&after.payload[..expected_prefix.len()], expected_prefix);
        assert_ne!(after.header.crc32, 0);
        assert_eq!(before_used as usize, expected_prefix.len());
        assert_eq!(before_crc, crc32_iso_hdlc(expected_prefix));
    }

    #[test]
    fn crc32_iso_hdlc_matches_check_value() {
        assert_eq!(crc32_iso_hdlc(b"123456789"), 0xCBF4_3926);
    }
}
