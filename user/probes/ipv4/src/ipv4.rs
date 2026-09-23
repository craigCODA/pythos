pub const IPV4_HEADER_BYTES: usize = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv4Header {
    pub version: u8,
    pub ihl: u8,
    pub dscp_ecn: u8,
    pub identification: u16,
    pub flags_fragment_offset: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub source: [u8; 4],
    pub destination: [u8; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv4Packet<'a> {
    pub header: Ipv4Header,
    pub total_length: u16,
    pub header_checksum: u16,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    NonCanonicalHeader,
    PayloadTooLong,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    HeaderTooShort,
    UnsupportedVersion,
    InvalidIhl,
    HeaderLengthExceedsInput,
    TotalLengthBelowHeader,
    TotalLengthExceedsInput,
    BadChecksum,
}

pub fn ipv4_header_checksum(header: &[u8]) -> u16 {
    let mut sum = 0_u32;
    let mut words = header.chunks_exact(2);

    for word in &mut words {
        sum += u32::from(u16::from_be_bytes([word[0], word[1]]));
        sum = (sum & 0xffff) + (sum >> 16);
    }

    if let Some(byte) = words.remainder().first() {
        sum += u32::from(*byte) << 8;
        sum = (sum & 0xffff) + (sum >> 16);
    }

    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }

    !(sum as u16)
}

pub fn encode_ipv4_packet(
    header: Ipv4Header,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    if header.version != 4 || header.ihl != 5 {
        return Err(EncodeError::NonCanonicalHeader);
    }

    let total_length = IPV4_HEADER_BYTES
        .checked_add(payload.len())
        .ok_or(EncodeError::PayloadTooLong)?;
    let total_length_u16 = u16::try_from(total_length).map_err(|_| EncodeError::PayloadTooLong)?;
    if output.len() < total_length {
        return Err(EncodeError::OutputTooSmall);
    }

    let datagram = &mut output[..total_length];
    datagram[0] = 0x45;
    datagram[1] = header.dscp_ecn;
    datagram[2..4].copy_from_slice(&total_length_u16.to_be_bytes());
    datagram[4..6].copy_from_slice(&header.identification.to_be_bytes());
    datagram[6..8].copy_from_slice(&header.flags_fragment_offset.to_be_bytes());
    datagram[8] = header.ttl;
    datagram[9] = header.protocol;
    datagram[10..12].fill(0);
    datagram[12..16].copy_from_slice(&header.source);
    datagram[16..20].copy_from_slice(&header.destination);

    let checksum = ipv4_header_checksum(&datagram[..IPV4_HEADER_BYTES]);
    datagram[10..12].copy_from_slice(&checksum.to_be_bytes());
    datagram[IPV4_HEADER_BYTES..].copy_from_slice(payload);

    Ok(total_length)
}

pub fn decode_ipv4_packet(datagram: &[u8]) -> Result<Ipv4Packet<'_>, DecodeError> {
    if datagram.len() < IPV4_HEADER_BYTES {
        return Err(DecodeError::HeaderTooShort);
    }

    let version_ihl = datagram[0];
    let version = version_ihl >> 4;
    if version != 4 {
        return Err(DecodeError::UnsupportedVersion);
    }

    let ihl = version_ihl & 0x0f;
    if ihl < 5 {
        return Err(DecodeError::InvalidIhl);
    }
    let header_length = usize::from(ihl)
        .checked_mul(4)
        .ok_or(DecodeError::HeaderLengthExceedsInput)?;
    if header_length > datagram.len() {
        return Err(DecodeError::HeaderLengthExceedsInput);
    }
    if ihl != 5 {
        return Err(DecodeError::InvalidIhl);
    }

    let total_length = u16::from_be_bytes([datagram[2], datagram[3]]);
    let total_length_usize = usize::from(total_length);
    if total_length_usize < header_length {
        return Err(DecodeError::TotalLengthBelowHeader);
    }
    if total_length_usize > datagram.len() {
        return Err(DecodeError::TotalLengthExceedsInput);
    }
    if ipv4_header_checksum(&datagram[..header_length]) != 0 {
        return Err(DecodeError::BadChecksum);
    }

    Ok(Ipv4Packet {
        header: Ipv4Header {
            version,
            ihl,
            dscp_ecn: datagram[1],
            identification: u16::from_be_bytes([datagram[4], datagram[5]]),
            flags_fragment_offset: u16::from_be_bytes([datagram[6], datagram[7]]),
            ttl: datagram[8],
            protocol: datagram[9],
            source: [datagram[12], datagram[13], datagram[14], datagram[15]],
            destination: [datagram[16], datagram[17], datagram[18], datagram[19]],
        },
        total_length,
        header_checksum: u16::from_be_bytes([datagram[10], datagram[11]]),
        payload: &datagram[header_length..total_length_usize],
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DecodeError, EncodeError, IPV4_HEADER_BYTES, Ipv4Header, decode_ipv4_packet,
        encode_ipv4_packet, ipv4_header_checksum,
    };

    const REQUEST_HEADER: Ipv4Header = Ipv4Header {
        version: 4,
        ihl: 5,
        dscp_ecn: 0,
        identification: 0x1401,
        flags_fragment_offset: 0,
        ttl: 64,
        protocol: 253,
        source: [192, 168, 14, 2],
        destination: [192, 168, 14, 1],
    };

    const REPLY_HEADER: Ipv4Header = Ipv4Header {
        identification: 0x1402,
        source: [192, 168, 14, 1],
        destination: [192, 168, 14, 2],
        ..REQUEST_HEADER
    };

    const REQUEST_BYTES: [u8; 20] = [
        0x45, 0x00, 0x00, 0x1c, 0x14, 0x01, 0x00, 0x00, 0x40, 0xfd, 0xc8, 0x90, 0xc0, 0xa8, 0x0e,
        0x02, 0xc0, 0xa8, 0x0e, 0x01,
    ];

    const REPLY_BYTES: [u8; 20] = [
        0x45, 0x00, 0x00, 0x1c, 0x14, 0x02, 0x00, 0x00, 0x40, 0xfd, 0xc8, 0x8f, 0xc0, 0xa8, 0x0e,
        0x01, 0xc0, 0xa8, 0x0e, 0x02,
    ];

    fn encode(header: Ipv4Header, payload: &[u8]) -> std::vec::Vec<u8> {
        let mut datagram = std::vec![0; IPV4_HEADER_BYTES + payload.len()];
        let encoded = encode_ipv4_packet(header, payload, &mut datagram).unwrap();
        assert_eq!(encoded, datagram.len());
        datagram
    }

    fn request_datagram() -> std::vec::Vec<u8> {
        encode(REQUEST_HEADER, b"PYTHIPRQ")
    }

    #[test]
    fn encodes_the_exact_request_header_with_c890_checksum() {
        let datagram = request_datagram();
        assert_eq!(&datagram[..IPV4_HEADER_BYTES], &REQUEST_BYTES);
        assert_eq!(ipv4_header_checksum(&datagram[..IPV4_HEADER_BYTES]), 0);
    }

    #[test]
    fn encodes_the_exact_reply_header_with_c88f_checksum() {
        let datagram = encode(REPLY_HEADER, b"PYTHIPRP");
        assert_eq!(&datagram[..IPV4_HEADER_BYTES], &REPLY_BYTES);
        assert_eq!(ipv4_header_checksum(&datagram[..IPV4_HEADER_BYTES]), 0);
    }

    #[test]
    fn round_trip_decodes_all_fields_and_borrows_only_the_declared_payload() {
        let mut datagram = request_datagram();
        datagram.extend_from_slice(&[0xa5; 4]);

        let packet = decode_ipv4_packet(&datagram).unwrap();

        assert_eq!(packet.header, REQUEST_HEADER);
        assert_eq!(packet.total_length, 28);
        assert_eq!(packet.header_checksum, 0xc890);
        assert_eq!(packet.payload, b"PYTHIPRQ");
        assert_eq!(
            packet.payload.as_ptr(),
            datagram[IPV4_HEADER_BYTES..].as_ptr()
        );
    }

    #[test]
    fn rejects_headers_shorter_than_twenty_bytes() {
        assert_eq!(
            decode_ipv4_packet(&[0; IPV4_HEADER_BYTES - 1]),
            Err(DecodeError::HeaderTooShort)
        );
    }

    #[test]
    fn rejects_versions_other_than_four() {
        let mut datagram = request_datagram();
        datagram[0] = 0x55;
        assert_eq!(
            decode_ipv4_packet(&datagram),
            Err(DecodeError::UnsupportedVersion)
        );
    }

    #[test]
    fn rejects_ihl_below_five() {
        let mut datagram = request_datagram();
        datagram[0] = 0x44;
        assert_eq!(decode_ipv4_packet(&datagram), Err(DecodeError::InvalidIhl));
    }

    #[test]
    fn rejects_header_length_beyond_the_input() {
        let mut datagram = [0; IPV4_HEADER_BYTES];
        datagram[0] = 0x46;
        assert_eq!(
            decode_ipv4_packet(&datagram),
            Err(DecodeError::HeaderLengthExceedsInput)
        );
    }

    #[test]
    fn rejects_total_length_below_header_length() {
        let mut datagram = request_datagram();
        datagram[2..4].copy_from_slice(&19_u16.to_be_bytes());
        assert_eq!(
            decode_ipv4_packet(&datagram),
            Err(DecodeError::TotalLengthBelowHeader)
        );
    }

    #[test]
    fn rejects_total_length_beyond_the_input() {
        let mut datagram = request_datagram();
        datagram[2..4].copy_from_slice(&29_u16.to_be_bytes());
        assert_eq!(
            decode_ipv4_packet(&datagram),
            Err(DecodeError::TotalLengthExceedsInput)
        );
    }

    #[test]
    fn rejects_a_bad_header_checksum() {
        let mut datagram = request_datagram();
        datagram[8] ^= 1;
        assert_eq!(decode_ipv4_packet(&datagram), Err(DecodeError::BadChecksum));
    }

    #[test]
    fn rejects_a_complete_noncanonical_options_header() {
        let mut datagram = [0_u8; 24];
        datagram[0] = 0x46;
        datagram[2..4].copy_from_slice(&24_u16.to_be_bytes());
        assert_eq!(decode_ipv4_packet(&datagram), Err(DecodeError::InvalidIhl));
    }

    #[test]
    fn rejects_an_output_buffer_smaller_than_the_datagram() {
        let mut output = [0; 27];
        assert_eq!(
            encode_ipv4_packet(REQUEST_HEADER, b"PYTHIPRQ", &mut output),
            Err(EncodeError::OutputTooSmall)
        );
    }

    #[test]
    fn rejects_payload_length_that_overflows_ipv4_total_length() {
        let payload = std::vec![0; usize::from(u16::MAX) + 1 - IPV4_HEADER_BYTES];
        let mut output = std::vec![0; IPV4_HEADER_BYTES];
        assert_eq!(
            encode_ipv4_packet(REQUEST_HEADER, &payload, &mut output),
            Err(EncodeError::PayloadTooLong)
        );
    }

    #[test]
    fn canonical_encoder_rejects_noncanonical_options() {
        let mut output = [0; 24];
        let header = Ipv4Header {
            ihl: 6,
            ..REQUEST_HEADER
        };
        assert_eq!(
            encode_ipv4_packet(header, &[], &mut output),
            Err(EncodeError::NonCanonicalHeader)
        );
    }
}
