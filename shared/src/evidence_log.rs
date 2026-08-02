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
    BufferTooSmall,
    InvalidMagic,
    UnsupportedVersion,
    CapacityMismatch,
    UsedOutOfRange,
    LineTooLong,
    NonAscii,
    LengthOverflow,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceLogSnapshot<'a> {
    pub header: EvidenceLogHeader,
    pub payload: &'a [u8],
}

pub fn initialize(buffer: &mut [u8]) -> Result<(), EvidenceLogError> {
    if buffer.len() < header_len() {
        return Err(EvidenceLogError::BufferTooSmall);
    }

    buffer.fill(0);
    write_header(
        buffer,
        EvidenceLogHeader {
            magic: EVIDENCE_LOG_MAGIC,
            version: EVIDENCE_LOG_VERSION,
            capacity: payload_capacity(buffer.len())?,
            used: 0,
            lines: 0,
            dropped: 0,
            crc32: 0,
        },
    );
    Ok(())
}

pub fn append_line(buffer: &mut [u8], line: &str) -> Result<(), EvidenceLogError> {
    if !line.is_ascii() {
        return Err(EvidenceLogError::NonAscii);
    }
    if line.len() > MAX_EVIDENCE_LINE_BYTES {
        return Err(EvidenceLogError::LineTooLong);
    }

    let header = read_header(buffer)?;
    let line_len = line.len();
    let needed = line_len
        .checked_add(1)
        .ok_or(EvidenceLogError::LengthOverflow)?;
    let used = usize::try_from(header.used).map_err(|_| EvidenceLogError::UsedOutOfRange)?;
    let capacity = usize::try_from(header.capacity).map_err(|_| EvidenceLogError::CapacityMismatch)?;
    let next_used = used
        .checked_add(needed)
        .ok_or(EvidenceLogError::LengthOverflow)?;
    if next_used > capacity {
        let mut updated = header;
        updated.dropped = updated.dropped.saturating_add(1);
        write_header(buffer, updated);
        return Err(EvidenceLogError::Full);
    }

    let payload_start = header_len();
    let payload_end = payload_start + used;
    let line_bytes = line.as_bytes();
    buffer[payload_end..payload_end + line_len].copy_from_slice(line_bytes);
    buffer[payload_end + line_len] = b'\n';

    let mut updated = header;
    updated.used = u32::try_from(next_used).map_err(|_| EvidenceLogError::LengthOverflow)?;
    updated.lines = updated.lines.saturating_add(1);
    updated.crc32 = crc32_iso_hdlc(&buffer[payload_start..payload_start + next_used]);
    write_header(buffer, updated);
    Ok(())
}

pub fn snapshot(buffer: &[u8]) -> Result<EvidenceLogSnapshot<'_>, EvidenceLogError> {
    let header = read_header(buffer)?;
    let used = usize::try_from(header.used).map_err(|_| EvidenceLogError::UsedOutOfRange)?;
    let capacity = usize::try_from(header.capacity).map_err(|_| EvidenceLogError::CapacityMismatch)?;
    if capacity != payload_capacity(buffer.len())? as usize {
        return Err(EvidenceLogError::CapacityMismatch);
    }
    if used > capacity {
        return Err(EvidenceLogError::UsedOutOfRange);
    }

    let payload_start = header_len();
    let payload_end = payload_start
        .checked_add(used)
        .ok_or(EvidenceLogError::LengthOverflow)?;
    let payload = buffer
        .get(payload_start..payload_end)
        .ok_or(EvidenceLogError::UsedOutOfRange)?;
    if crc32_iso_hdlc(payload) != header.crc32 {
        return Err(EvidenceLogError::InvalidMagic);
    }

    Ok(EvidenceLogSnapshot { header, payload })
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
    crc ^ 0xFFFF_FFFF
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
    fn append_line_records_newline_and_crc32_iso_hdlc() {
        let mut buffer = [0u8; EVIDENCE_LOG_TOTAL_BYTES];
        initialize(&mut buffer).unwrap();
        append_line(&mut buffer, "PYTHOS:LOADER:ENTER").unwrap();
        let snapshot = snapshot(&buffer).unwrap();
        assert_eq!(snapshot.payload, b"PYTHOS:LOADER:ENTER\n");
        assert_eq!(snapshot.header.used as usize, snapshot.payload.len());
        assert_eq!(snapshot.header.lines, 1);
        assert_eq!(snapshot.header.dropped, 0);
        assert_eq!(snapshot.header.crc32, crc32_iso_hdlc(snapshot.payload));
    }

    #[test]
    fn crc32_iso_hdlc_matches_check_value() {
        assert_eq!(crc32_iso_hdlc(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn full_buffer_increments_dropped_without_mutating_payload() {
        let mut buffer = [0u8; 96];
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
}

fn header_len() -> usize {
    core::mem::size_of::<EvidenceLogHeader>()
}

fn payload_capacity(total_len: usize) -> Result<u32, EvidenceLogError> {
    let capacity = total_len
        .checked_sub(header_len())
        .ok_or(EvidenceLogError::BufferTooSmall)?;
    u32::try_from(capacity).map_err(|_| EvidenceLogError::CapacityMismatch)
}

fn read_header(buffer: &[u8]) -> Result<EvidenceLogHeader, EvidenceLogError> {
    if buffer.len() < header_len() {
        return Err(EvidenceLogError::BufferTooSmall);
    }

    let header = EvidenceLogHeader {
        magic: read_array::<8>(buffer, 0)?,
        version: read_u32(buffer, 8)?,
        capacity: read_u32(buffer, 12)?,
        used: read_u32(buffer, 16)?,
        lines: read_u32(buffer, 20)?,
        dropped: read_u32(buffer, 24)?,
        crc32: read_u32(buffer, 28)?,
    };

    if header.magic != EVIDENCE_LOG_MAGIC {
        return Err(EvidenceLogError::InvalidMagic);
    }
    if header.version != EVIDENCE_LOG_VERSION {
        return Err(EvidenceLogError::UnsupportedVersion);
    }
    if header.capacity != payload_capacity(buffer.len())? {
        return Err(EvidenceLogError::CapacityMismatch);
    }

    Ok(header)
}

fn write_header(buffer: &mut [u8], header: EvidenceLogHeader) {
    buffer[..8].copy_from_slice(&header.magic);
    buffer[8..12].copy_from_slice(&header.version.to_le_bytes());
    buffer[12..16].copy_from_slice(&header.capacity.to_le_bytes());
    buffer[16..20].copy_from_slice(&header.used.to_le_bytes());
    buffer[20..24].copy_from_slice(&header.lines.to_le_bytes());
    buffer[24..28].copy_from_slice(&header.dropped.to_le_bytes());
    buffer[28..32].copy_from_slice(&header.crc32.to_le_bytes());
}

fn read_u32(buffer: &[u8], offset: usize) -> Result<u32, EvidenceLogError> {
    let bytes = buffer
        .get(offset..offset + 4)
        .ok_or(EvidenceLogError::BufferTooSmall)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_array<const N: usize>(buffer: &[u8], offset: usize) -> Result<[u8; N], EvidenceLogError> {
    let bytes = buffer
        .get(offset..offset + N)
        .ok_or(EvidenceLogError::BufferTooSmall)?;
    let mut array = [0u8; N];
    array.copy_from_slice(bytes);
    Ok(array)
}
