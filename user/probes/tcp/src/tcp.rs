#[cfg(test)]
mod tests {
    use super::{
        DecodeError, EncodeError, TCP_FLAG_ACK, TCP_FLAG_FIN, TCP_FLAG_RST, TCP_FLAG_SYN,
        TCP_HEADER_BYTES, TCP_MSS, TCP_SYN_HEADER_BYTES, TcpSegment, decode_segment,
        encode_segment, next_sequence_number, sequence_space_len, tcp_checksum,
    };

    const LOCAL: [u8; 4] = [192, 168, 14, 2];
    const PEER: [u8; 4] = [192, 168, 14, 1];

    const SEGMENTS: [TcpSegment<'static>; 10] = [
        TcpSegment {
            source_port: 0x1505,
            destination_port: 0x1506,
            sequence: 0x1505_0000,
            acknowledgment: 0,
            flags: TCP_FLAG_SYN,
            window: 0x1000,
            urgent_pointer: 0,
            mss: Some(TCP_MSS),
            data: b"",
        },
        TcpSegment {
            source_port: 0x1506,
            destination_port: 0x1505,
            sequence: 0x2506_0000,
            acknowledgment: 0x1505_0001,
            flags: TCP_FLAG_SYN | TCP_FLAG_ACK,
            window: 0x1000,
            urgent_pointer: 0,
            mss: Some(TCP_MSS),
            data: b"",
        },
        TcpSegment {
            source_port: 0x1505,
            destination_port: 0x1506,
            sequence: 0x1505_0001,
            acknowledgment: 0x2506_0001,
            flags: TCP_FLAG_ACK,
            window: 0x1000,
            urgent_pointer: 0,
            mss: None,
            data: b"",
        },
        TcpSegment {
            source_port: 0x1505,
            destination_port: 0x1506,
            sequence: 0x1505_0001,
            acknowledgment: 0x2506_0001,
            flags: TCP_FLAG_ACK,
            window: 0x1000,
            urgent_pointer: 0,
            mss: None,
            data: b"PYTCPQ",
        },
        TcpSegment {
            source_port: 0x1506,
            destination_port: 0x1505,
            sequence: 0x2506_0001,
            acknowledgment: 0x1505_0007,
            flags: TCP_FLAG_ACK,
            window: 0x1000,
            urgent_pointer: 0,
            mss: None,
            data: b"PYTCPR",
        },
        TcpSegment {
            source_port: 0x1505,
            destination_port: 0x1506,
            sequence: 0x1505_0007,
            acknowledgment: 0x2506_0007,
            flags: TCP_FLAG_ACK,
            window: 0x1000,
            urgent_pointer: 0,
            mss: None,
            data: b"",
        },
        TcpSegment {
            source_port: 0x1505,
            destination_port: 0x1506,
            sequence: 0x1505_0007,
            acknowledgment: 0x2506_0007,
            flags: TCP_FLAG_FIN | TCP_FLAG_ACK,
            window: 0x1000,
            urgent_pointer: 0,
            mss: None,
            data: b"",
        },
        TcpSegment {
            source_port: 0x1506,
            destination_port: 0x1505,
            sequence: 0x2506_0007,
            acknowledgment: 0x1505_0008,
            flags: TCP_FLAG_ACK,
            window: 0x1000,
            urgent_pointer: 0,
            mss: None,
            data: b"",
        },
        TcpSegment {
            source_port: 0x1506,
            destination_port: 0x1505,
            sequence: 0x2506_0007,
            acknowledgment: 0x1505_0008,
            flags: TCP_FLAG_FIN | TCP_FLAG_ACK,
            window: 0x1000,
            urgent_pointer: 0,
            mss: None,
            data: b"",
        },
        TcpSegment {
            source_port: 0x1505,
            destination_port: 0x1506,
            sequence: 0x1505_0008,
            acknowledgment: 0x2506_0008,
            flags: TCP_FLAG_ACK,
            window: 0x1000,
            urgent_pointer: 0,
            mss: None,
            data: b"",
        },
    ];

    const EXPECTED: [&[u8]; 10] = [
        &[
            0x15, 0x05, 0x15, 0x06, 0x15, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x60, 0x02,
            0x10, 0x00, 0xad, 0x76, 0x00, 0x00, 0x02, 0x04, 0x04, 0x00,
        ],
        &[
            0x15, 0x06, 0x15, 0x05, 0x25, 0x06, 0x00, 0x00, 0x15, 0x05, 0x00, 0x01, 0x60, 0x12,
            0x10, 0x00, 0x88, 0x5f, 0x00, 0x00, 0x02, 0x04, 0x04, 0x00,
        ],
        &[
            0x15, 0x05, 0x15, 0x06, 0x15, 0x05, 0x00, 0x01, 0x25, 0x06, 0x00, 0x01, 0x50, 0x10,
            0x10, 0x00, 0x9e, 0x68, 0x00, 0x00,
        ],
        &[
            0x15, 0x05, 0x15, 0x06, 0x15, 0x05, 0x00, 0x01, 0x25, 0x06, 0x00, 0x01, 0x50, 0x10,
            0x10, 0x00, 0xa9, 0x74, 0x00, 0x00, 0x50, 0x59, 0x54, 0x43, 0x50, 0x51,
        ],
        &[
            0x15, 0x06, 0x15, 0x05, 0x25, 0x06, 0x00, 0x01, 0x15, 0x05, 0x00, 0x07, 0x50, 0x10,
            0x10, 0x00, 0xa9, 0x6d, 0x00, 0x00, 0x50, 0x59, 0x54, 0x43, 0x50, 0x52,
        ],
        &[
            0x15, 0x05, 0x15, 0x06, 0x15, 0x05, 0x00, 0x07, 0x25, 0x06, 0x00, 0x07, 0x50, 0x10,
            0x10, 0x00, 0x9e, 0x5c, 0x00, 0x00,
        ],
        &[
            0x15, 0x05, 0x15, 0x06, 0x15, 0x05, 0x00, 0x07, 0x25, 0x06, 0x00, 0x07, 0x50, 0x11,
            0x10, 0x00, 0x9e, 0x5b, 0x00, 0x00,
        ],
        &[
            0x15, 0x06, 0x15, 0x05, 0x25, 0x06, 0x00, 0x07, 0x15, 0x05, 0x00, 0x08, 0x50, 0x10,
            0x10, 0x00, 0x9e, 0x5b, 0x00, 0x00,
        ],
        &[
            0x15, 0x06, 0x15, 0x05, 0x25, 0x06, 0x00, 0x07, 0x15, 0x05, 0x00, 0x08, 0x50, 0x11,
            0x10, 0x00, 0x9e, 0x5a, 0x00, 0x00,
        ],
        &[
            0x15, 0x05, 0x15, 0x06, 0x15, 0x05, 0x00, 0x08, 0x25, 0x06, 0x00, 0x08, 0x50, 0x10,
            0x10, 0x00, 0x9e, 0x5a, 0x00, 0x00,
        ],
    ];

    fn addresses(index: usize) -> ([u8; 4], [u8; 4]) {
        if index == 1 || index == 4 || index == 7 || index == 8 {
            (PEER, LOCAL)
        } else {
            (LOCAL, PEER)
        }
    }

    fn encode(index: usize) -> std::vec::Vec<u8> {
        let (source, destination) = addresses(index);
        let mut output = std::vec![0; EXPECTED[index].len()];
        assert_eq!(
            encode_segment(SEGMENTS[index], source, destination, &mut output),
            Ok(output.len())
        );
        output
    }

    fn rewrite_checksum(segment: &mut [u8], source: [u8; 4], destination: [u8; 4]) {
        segment[16..18].fill(0);
        let checksum = tcp_checksum(source, destination, segment);
        segment[16..18].copy_from_slice(&checksum.to_be_bytes());
    }

    #[test]
    fn encodes_all_ten_exact_tcp_headers_options_payloads_and_checksums() {
        for index in 0..SEGMENTS.len() {
            let encoded = encode(index);
            assert_eq!(encoded, EXPECTED[index]);
            let (source, destination) = addresses(index);
            assert_eq!(tcp_checksum(source, destination, &encoded), 0);
        }
    }

    #[test]
    fn decodes_all_ten_exact_segments_and_borrows_payload() {
        for index in 0..SEGMENTS.len() {
            let encoded = encode(index);
            let (source, destination) = addresses(index);
            let decoded = decode_segment(&encoded, source, destination).unwrap();
            assert_eq!(decoded, SEGMENTS[index]);
            if !decoded.data.is_empty() {
                assert_eq!(decoded.data.as_ptr(), encoded[TCP_HEADER_BYTES..].as_ptr());
            }
        }
    }

    #[test]
    fn checksum_zeroing_and_odd_padding_are_arithmetic_only() {
        let mut data_segment = EXPECTED[3].to_vec();
        data_segment[16..18].fill(0);
        assert_eq!(tcp_checksum(LOCAL, PEER, &data_segment), 0xa974);
        assert_eq!(tcp_checksum([0; 4], [0; 4], &[0x12, 0x34, 0x56]), 0x97c2);
    }

    #[test]
    fn mss_option_is_exactly_four_bytes_and_advertises_1024() {
        assert_eq!(&EXPECTED[0][TCP_HEADER_BYTES..], &[0x02, 0x04, 0x04, 0x00]);
        assert_eq!(&EXPECTED[1][TCP_HEADER_BYTES..], &[0x02, 0x04, 0x04, 0x00]);
        assert_eq!(SEGMENTS[0].mss, Some(1024));
        assert_eq!(SEGMENTS[1].mss, Some(1024));
    }

    #[test]
    fn sequence_helpers_account_for_syn_data_fin_and_ack() {
        assert_eq!(sequence_space_len(&SEGMENTS[0]), Some(1));
        assert_eq!(sequence_space_len(&SEGMENTS[3]), Some(6));
        assert_eq!(sequence_space_len(&SEGMENTS[6]), Some(1));
        assert_eq!(sequence_space_len(&SEGMENTS[2]), Some(0));
        assert_eq!(
            next_sequence_number(0x1505_0000, &SEGMENTS[0]),
            Some(0x1505_0001)
        );
        assert_eq!(
            next_sequence_number(0x1505_0001, &SEGMENTS[3]),
            Some(0x1505_0007)
        );
        assert_eq!(next_sequence_number(0xffff_ffff, &SEGMENTS[6]), None);
    }

    #[test]
    fn encoder_rejects_short_and_overlong_output() {
        assert_eq!(
            encode_segment(SEGMENTS[0], LOCAL, PEER, &mut [0; TCP_SYN_HEADER_BYTES - 1]),
            Err(EncodeError::OutputTooSmall)
        );
        assert_eq!(
            encode_segment(SEGMENTS[0], LOCAL, PEER, &mut [0; TCP_SYN_HEADER_BYTES + 1]),
            Err(EncodeError::OutputTooLarge)
        );
    }

    #[test]
    fn decoder_rejects_short_and_overlong_inputs() {
        let encoded = encode(0);
        assert_eq!(
            decode_segment(&encoded[..TCP_SYN_HEADER_BYTES - 1], LOCAL, PEER),
            Err(DecodeError::InputTooShort)
        );
        let mut overlong = std::vec![0; TCP_SYN_HEADER_BYTES + 1];
        overlong[..TCP_SYN_HEADER_BYTES].copy_from_slice(&encoded);
        assert_eq!(
            decode_segment(&overlong, LOCAL, PEER),
            Err(DecodeError::InputTooLarge)
        );
    }

    #[test]
    fn decoder_rejects_truncated_headers_without_panicking() {
        for length in 0..TCP_HEADER_BYTES {
            assert_eq!(
                decode_segment(&EXPECTED[2][..length], LOCAL, PEER),
                Err(DecodeError::InputTooShort)
            );
        }
        let mut truncated = EXPECTED[0][..TCP_HEADER_BYTES].to_vec();
        truncated[12] = 0x60;
        assert_eq!(
            decode_segment(&truncated, LOCAL, PEER),
            Err(DecodeError::InputTooShort)
        );
    }

    #[test]
    fn decoder_rejects_malformed_data_offsets_and_options() {
        let mut short_offset = EXPECTED[2].to_vec();
        short_offset[12] = 0x40;
        assert_eq!(
            decode_segment(&short_offset, LOCAL, PEER),
            Err(DecodeError::InvalidDataOffset)
        );

        let mut long_offset = EXPECTED[2].to_vec();
        long_offset[12] = 0x70;
        assert_eq!(
            decode_segment(&long_offset, LOCAL, PEER),
            Err(DecodeError::InvalidDataOffset)
        );

        let mut reserved_bits = EXPECTED[2].to_vec();
        reserved_bits[12] = 0x51;
        rewrite_checksum(&mut reserved_bits, LOCAL, PEER);
        assert_eq!(
            decode_segment(&reserved_bits, LOCAL, PEER),
            Err(DecodeError::InvalidProfile)
        );

        let mut wrong_mss = EXPECTED[0].to_vec();
        wrong_mss[22] = 0x05;
        rewrite_checksum(&mut wrong_mss, LOCAL, PEER);
        assert_eq!(
            decode_segment(&wrong_mss, LOCAL, PEER),
            Err(DecodeError::InvalidOptions)
        );

        let mut ordinary_option = EXPECTED[2].to_vec();
        ordinary_option[12] = 0x60;
        ordinary_option.extend_from_slice(&[0x02, 0x04, 0x04, 0x00]);
        assert_eq!(
            decode_segment(&ordinary_option, LOCAL, PEER),
            Err(DecodeError::InvalidOptions)
        );
    }

    #[test]
    fn decoder_rejects_bad_zero_and_wrong_pseudo_header_checksums() {
        let encoded = encode(3);
        let mut bad = encoded.clone();
        bad[16] ^= 1;
        assert_eq!(
            decode_segment(&bad, LOCAL, PEER),
            Err(DecodeError::BadChecksum)
        );

        let mut zero = encoded.clone();
        zero[16..18].fill(0);
        assert_eq!(
            decode_segment(&zero, LOCAL, PEER),
            Err(DecodeError::ZeroChecksum)
        );
        assert_eq!(
            decode_segment(&encoded, [192, 168, 14, 3], PEER),
            Err(DecodeError::BadChecksum)
        );
        assert_eq!(
            decode_segment(&encoded, LOCAL, [192, 168, 14, 3]),
            Err(DecodeError::BadChecksum)
        );
    }

    #[test]
    fn decoder_rejects_a_checksum_computed_with_the_wrong_protocol() {
        let mut wrong_protocol = encode(3);
        wrong_protocol[16..18].fill(0);
        let checksum = checksum_with_protocol(LOCAL, PEER, &wrong_protocol, 17);
        wrong_protocol[16..18].copy_from_slice(&checksum.to_be_bytes());
        assert_eq!(
            decode_segment(&wrong_protocol, LOCAL, PEER),
            Err(DecodeError::BadChecksum)
        );
    }

    #[test]
    fn encoder_and_decoder_reject_invalid_flags_and_profile_fields() {
        let mut output = [0; TCP_HEADER_BYTES];
        assert_eq!(
            encode_segment(
                TcpSegment {
                    flags: TCP_FLAG_RST,
                    ..SEGMENTS[2]
                },
                LOCAL,
                PEER,
                &mut output
            ),
            Err(EncodeError::InvalidProfile)
        );

        for (offset, value) in [(0, 0x04), (13, 0x14)] {
            let mut invalid = EXPECTED[2].to_vec();
            invalid[offset] = value;
            rewrite_checksum(&mut invalid, LOCAL, PEER);
            assert_eq!(
                decode_segment(&invalid, LOCAL, PEER),
                Err(DecodeError::InvalidProfile)
            );
        }

        let mut invalid_flags = EXPECTED[2].to_vec();
        invalid_flags[13] = TCP_FLAG_RST;
        rewrite_checksum(&mut invalid_flags, LOCAL, PEER);
        assert_eq!(
            decode_segment(&invalid_flags, LOCAL, PEER),
            Err(DecodeError::InvalidProfile)
        );
    }

    #[test]
    fn decoder_rejects_data_lengths_outside_the_six_byte_profile() {
        let mut extra = EXPECTED[2].to_vec();
        extra.extend_from_slice(b"X");
        assert_eq!(
            decode_segment(&extra, LOCAL, PEER),
            Err(DecodeError::InputTooLarge)
        );

        let mut wrong_data = EXPECTED[3].to_vec();
        wrong_data[25] = b'X';
        rewrite_checksum(&mut wrong_data, LOCAL, PEER);
        assert_eq!(
            decode_segment(&wrong_data, LOCAL, PEER),
            Err(DecodeError::InvalidProfile)
        );
    }

    fn checksum_with_protocol(
        source: [u8; 4],
        destination: [u8; 4],
        segment: &[u8],
        protocol: u8,
    ) -> u16 {
        let mut sum = 0_u32;
        let mut add = |word: u16| {
            sum += u32::from(word);
            sum = (sum & 0xffff) + (sum >> 16);
        };
        add(u16::from_be_bytes([source[0], source[1]]));
        add(u16::from_be_bytes([source[2], source[3]]));
        add(u16::from_be_bytes([destination[0], destination[1]]));
        add(u16::from_be_bytes([destination[2], destination[3]]));
        add(u16::from(protocol));
        add(u16::try_from(segment.len()).unwrap());
        let mut words = segment.chunks_exact(2);
        for word in &mut words {
            add(u16::from_be_bytes([word[0], word[1]]));
        }
        if let Some(byte) = words.remainder().first() {
            add(u16::from(*byte) << 8);
        }
        while sum > 0xffff {
            sum = (sum & 0xffff) + (sum >> 16);
        }
        !(sum as u16)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TcpSegment<'a> {
    pub source_port: u16,
    pub destination_port: u16,
    pub sequence: u32,
    pub acknowledgment: u32,
    pub flags: u8,
    pub window: u16,
    pub urgent_pointer: u16,
    pub mss: Option<u16>,
    pub data: &'a [u8],
}

pub const TCP_FLAG_FIN: u8 = 0x01;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_RST: u8 = 0x04;
pub const TCP_FLAG_ACK: u8 = 0x10;
pub const TCP_FLAG_SYN_ACK: u8 = TCP_FLAG_SYN | TCP_FLAG_ACK;
pub const TCP_FLAG_FIN_ACK: u8 = TCP_FLAG_FIN | TCP_FLAG_ACK;

pub const TCP_HEADER_BYTES: usize = 20;
pub const TCP_SYN_HEADER_BYTES: usize = 24;
pub const TCP_PROTOCOL: u8 = 6;
pub const TCP_MSS: u16 = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    InvalidProfile,
    PayloadTooLong,
    OutputTooSmall,
    OutputTooLarge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    InputTooShort,
    InputTooLarge,
    InvalidDataOffset,
    InvalidOptions,
    ZeroChecksum,
    BadChecksum,
    InvalidProfile,
}

pub fn tcp_checksum(source_ipv4: [u8; 4], destination_ipv4: [u8; 4], segment: &[u8]) -> u16 {
    let mut sum = 0_u32;

    add_word(
        &mut sum,
        u16::from_be_bytes([source_ipv4[0], source_ipv4[1]]),
    );
    add_word(
        &mut sum,
        u16::from_be_bytes([source_ipv4[2], source_ipv4[3]]),
    );
    add_word(
        &mut sum,
        u16::from_be_bytes([destination_ipv4[0], destination_ipv4[1]]),
    );
    add_word(
        &mut sum,
        u16::from_be_bytes([destination_ipv4[2], destination_ipv4[3]]),
    );
    add_word(&mut sum, u16::from(TCP_PROTOCOL));
    add_word(&mut sum, u16::try_from(segment.len()).unwrap_or(u16::MAX));

    let mut words = segment.chunks_exact(2);
    for word in &mut words {
        add_word(&mut sum, u16::from_be_bytes([word[0], word[1]]));
    }
    if let Some(byte) = words.remainder().first() {
        add_word(&mut sum, u16::from(*byte) << 8);
    }

    while sum > u32::from(u16::MAX) {
        sum = (sum & u32::from(u16::MAX)) + (sum >> 16);
    }

    !(sum as u16)
}

pub fn encode_segment(
    segment: TcpSegment<'_>,
    source_ipv4: [u8; 4],
    destination_ipv4: [u8; 4],
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    validate_profile(segment, source_ipv4, destination_ipv4)?;

    let header_bytes = if is_syn(segment.flags) {
        TCP_SYN_HEADER_BYTES
    } else {
        TCP_HEADER_BYTES
    };
    let segment_bytes = header_bytes
        .checked_add(segment.data.len())
        .ok_or(EncodeError::PayloadTooLong)?;
    if u16::try_from(segment_bytes).is_err() {
        return Err(EncodeError::PayloadTooLong);
    }
    if output.len() < segment_bytes {
        return Err(EncodeError::OutputTooSmall);
    }
    if output.len() > segment_bytes {
        return Err(EncodeError::OutputTooLarge);
    }

    let encoded = output
        .get_mut(..segment_bytes)
        .ok_or(EncodeError::OutputTooSmall)?;
    encoded[0..2].copy_from_slice(&segment.source_port.to_be_bytes());
    encoded[2..4].copy_from_slice(&segment.destination_port.to_be_bytes());
    encoded[4..8].copy_from_slice(&segment.sequence.to_be_bytes());
    encoded[8..12].copy_from_slice(&segment.acknowledgment.to_be_bytes());
    encoded[12] = ((header_bytes / 4) as u8) << 4;
    encoded[13] = segment.flags;
    encoded[14..16].copy_from_slice(&segment.window.to_be_bytes());
    encoded[16..18].fill(0);
    encoded[18..20].copy_from_slice(&segment.urgent_pointer.to_be_bytes());
    if is_syn(segment.flags) {
        encoded[TCP_HEADER_BYTES..TCP_SYN_HEADER_BYTES].copy_from_slice(&[0x02, 0x04, 0x04, 0x00]);
    }
    encoded[header_bytes..segment_bytes].copy_from_slice(segment.data);

    let checksum = tcp_checksum(source_ipv4, destination_ipv4, encoded);
    encoded[16..18].copy_from_slice(&checksum.to_be_bytes());
    Ok(segment_bytes)
}

pub fn decode_segment(
    bytes: &[u8],
    source_ipv4: [u8; 4],
    destination_ipv4: [u8; 4],
) -> Result<TcpSegment<'_>, DecodeError> {
    if bytes.len() < TCP_HEADER_BYTES {
        return Err(DecodeError::InputTooShort);
    }

    let data_offset = bytes[12] >> 4;
    if !(5..=6).contains(&data_offset) {
        return Err(DecodeError::InvalidDataOffset);
    }
    if bytes[12] & 0x0f != 0 {
        return Err(DecodeError::InvalidProfile);
    }
    let header_bytes = usize::from(data_offset)
        .checked_mul(4)
        .ok_or(DecodeError::InvalidDataOffset)?;
    if header_bytes > bytes.len() {
        return Err(DecodeError::InputTooShort);
    }

    let flags = bytes[13];
    if data_offset == 6 {
        if !is_syn(flags) || bytes[20..24] != [0x02, 0x04, 0x04, 0x00] {
            return Err(DecodeError::InvalidOptions);
        }
        if bytes.len() != TCP_SYN_HEADER_BYTES {
            return if bytes.len() < TCP_SYN_HEADER_BYTES {
                Err(DecodeError::InputTooShort)
            } else {
                Err(DecodeError::InputTooLarge)
            };
        }
    } else if bytes.len() != TCP_HEADER_BYTES && bytes.len() != TCP_HEADER_BYTES + 6 {
        return Err(DecodeError::InputTooLarge);
    }

    let checksum = u16::from_be_bytes([bytes[16], bytes[17]]);
    if checksum == 0 {
        return Err(DecodeError::ZeroChecksum);
    }
    if tcp_checksum(source_ipv4, destination_ipv4, bytes) != 0 {
        return Err(DecodeError::BadChecksum);
    }

    let data = bytes
        .get(header_bytes..)
        .ok_or(DecodeError::InputTooShort)?;
    let segment = TcpSegment {
        source_port: u16::from_be_bytes([bytes[0], bytes[1]]),
        destination_port: u16::from_be_bytes([bytes[2], bytes[3]]),
        sequence: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        acknowledgment: u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        flags,
        window: u16::from_be_bytes([bytes[14], bytes[15]]),
        urgent_pointer: u16::from_be_bytes([bytes[18], bytes[19]]),
        mss: if data_offset == 6 {
            Some(TCP_MSS)
        } else {
            None
        },
        data,
    };
    validate_profile(segment, source_ipv4, destination_ipv4)
        .map_err(|_| DecodeError::InvalidProfile)?;
    Ok(segment)
}

pub fn sequence_space_len(segment: &TcpSegment<'_>) -> Option<u32> {
    if !valid_flags(segment.flags) {
        return None;
    }
    let data_len = u32::try_from(segment.data.len()).ok()?;
    let control_len = u32::from((segment.flags & (TCP_FLAG_SYN | TCP_FLAG_FIN)) != 0);
    data_len.checked_add(control_len)
}

pub fn next_sequence_number(sequence: u32, segment: &TcpSegment<'_>) -> Option<u32> {
    sequence.checked_add(sequence_space_len(segment)?)
}

fn add_word(sum: &mut u32, word: u16) {
    *sum += u32::from(word);
    *sum = (*sum & u32::from(u16::MAX)) + (*sum >> 16);
}

fn is_syn(flags: u8) -> bool {
    flags == TCP_FLAG_SYN || flags == TCP_FLAG_SYN_ACK
}

fn valid_flags(flags: u8) -> bool {
    matches!(
        flags,
        TCP_FLAG_SYN | TCP_FLAG_SYN_ACK | TCP_FLAG_ACK | TCP_FLAG_FIN_ACK
    )
}

fn validate_profile(
    segment: TcpSegment<'_>,
    source_ipv4: [u8; 4],
    destination_ipv4: [u8; 4],
) -> Result<(), EncodeError> {
    let local_to_peer = source_ipv4 == [192, 168, 14, 2]
        && destination_ipv4 == [192, 168, 14, 1]
        && segment.source_port == 0x1505
        && segment.destination_port == 0x1506
        && (segment.data == b"" || segment.data == b"PYTCPQ");
    let peer_to_local = source_ipv4 == [192, 168, 14, 1]
        && destination_ipv4 == [192, 168, 14, 2]
        && segment.source_port == 0x1506
        && segment.destination_port == 0x1505
        && (segment.data == b"" || segment.data == b"PYTCPR");
    let direction_matches = local_to_peer || peer_to_local;
    let data_matches = if segment.data.is_empty() {
        true
    } else if local_to_peer {
        segment.data == b"PYTCPQ"
    } else if peer_to_local {
        segment.data == b"PYTCPR"
    } else {
        false
    };
    let options_match = if is_syn(segment.flags) {
        segment.mss == Some(TCP_MSS)
    } else {
        segment.mss.is_none()
    };
    let flags_match = valid_flags(segment.flags)
        && (!is_syn(segment.flags) || segment.data.is_empty())
        && (segment.flags & TCP_FLAG_FIN == 0 || segment.data.is_empty());

    if direction_matches
        && data_matches
        && options_match
        && flags_match
        && segment.window == 0x1000
        && segment.urgent_pointer == 0
    {
        Ok(())
    } else {
        Err(EncodeError::InvalidProfile)
    }
}
