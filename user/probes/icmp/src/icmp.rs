pub const ICMP_ECHO_BYTES: usize = 16;

const ICMP_HEADER_BYTES: usize = 8;
const ICMP_IDENTIFIER: u16 = 0x1403;
const ICMP_SEQUENCE: u16 = 0x0001;
const ICMP_DATA: &[u8; 8] = b"PYTHICMP";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IcmpEcho<'a> {
    pub icmp_type: u8,
    pub code: u8,
    pub identifier: u16,
    pub sequence: u16,
    pub data: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    UnsupportedType,
    InvalidCode,
    InvalidIdentifier,
    InvalidSequence,
    InvalidData,
    OutputTooSmall,
    OutputTooLarge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    InputTooShort,
    InputTooLarge,
    UnsupportedType,
    InvalidCode,
    InvalidIdentifier,
    InvalidSequence,
    InvalidData,
    BadChecksum,
}

pub fn icmp_checksum(message: &[u8]) -> u16 {
    let mut sum = 0_u32;
    let mut words = message.chunks_exact(2);

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

pub fn encode_echo(message: IcmpEcho<'_>, output: &mut [u8]) -> Result<usize, EncodeError> {
    validate_encode_profile(message)?;

    let message_len = ICMP_HEADER_BYTES
        .checked_add(ICMP_DATA.len())
        .ok_or(EncodeError::OutputTooLarge)?;
    if output.len() < message_len {
        return Err(EncodeError::OutputTooSmall);
    }
    if output.len() > message_len {
        return Err(EncodeError::OutputTooLarge);
    }

    let encoded = output
        .get_mut(..message_len)
        .ok_or(EncodeError::OutputTooSmall)?;
    encoded[0] = message.icmp_type;
    encoded[1] = message.code;
    encoded[2..4].fill(0);
    encoded[4..6].copy_from_slice(&message.identifier.to_be_bytes());
    encoded[6..8].copy_from_slice(&message.sequence.to_be_bytes());
    encoded[ICMP_HEADER_BYTES..message_len].copy_from_slice(message.data);

    let checksum = icmp_checksum(encoded);
    encoded[2..4].copy_from_slice(&checksum.to_be_bytes());
    Ok(message_len)
}

pub fn decode_echo(message: &[u8]) -> Result<IcmpEcho<'_>, DecodeError> {
    let message_len = ICMP_HEADER_BYTES
        .checked_add(ICMP_DATA.len())
        .ok_or(DecodeError::InputTooLarge)?;
    if message.len() < message_len {
        return Err(DecodeError::InputTooShort);
    }
    if message.len() > message_len {
        return Err(DecodeError::InputTooLarge);
    }

    let icmp_type = message[0];
    if icmp_type != 8 && icmp_type != 0 {
        return Err(DecodeError::UnsupportedType);
    }
    let code = message[1];
    if code != 0 {
        return Err(DecodeError::InvalidCode);
    }

    let identifier = u16::from_be_bytes([message[4], message[5]]);
    if identifier != ICMP_IDENTIFIER {
        return Err(DecodeError::InvalidIdentifier);
    }
    let sequence = u16::from_be_bytes([message[6], message[7]]);
    if sequence != ICMP_SEQUENCE {
        return Err(DecodeError::InvalidSequence);
    }
    let data = message
        .get(ICMP_HEADER_BYTES..message_len)
        .ok_or(DecodeError::InputTooShort)?;
    if data != ICMP_DATA {
        return Err(DecodeError::InvalidData);
    }
    if icmp_checksum(message) != 0 {
        return Err(DecodeError::BadChecksum);
    }

    Ok(IcmpEcho {
        icmp_type,
        code,
        identifier,
        sequence,
        data,
    })
}

fn validate_encode_profile(message: IcmpEcho<'_>) -> Result<(), EncodeError> {
    if message.icmp_type != 8 && message.icmp_type != 0 {
        return Err(EncodeError::UnsupportedType);
    }
    if message.code != 0 {
        return Err(EncodeError::InvalidCode);
    }
    if message.identifier != ICMP_IDENTIFIER {
        return Err(EncodeError::InvalidIdentifier);
    }
    if message.sequence != ICMP_SEQUENCE {
        return Err(EncodeError::InvalidSequence);
    }
    if message.data != ICMP_DATA {
        return Err(EncodeError::InvalidData);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DecodeError, EncodeError, IcmpEcho, decode_echo, encode_echo, icmp_checksum};

    const REQUEST_BYTES: [u8; 16] = [
        0x08, 0x00, 0xa8, 0xc6, 0x14, 0x03, 0x00, 0x01, 0x50, 0x59, 0x54, 0x48, 0x49, 0x43, 0x4d,
        0x50,
    ];
    const REPLY_BYTES: [u8; 16] = [
        0x00, 0x00, 0xb0, 0xc6, 0x14, 0x03, 0x00, 0x01, 0x50, 0x59, 0x54, 0x48, 0x49, 0x43, 0x4d,
        0x50,
    ];

    const REQUEST: IcmpEcho<'static> = IcmpEcho {
        icmp_type: 8,
        code: 0,
        identifier: 0x1403,
        sequence: 1,
        data: b"PYTHICMP",
    };
    const REPLY: IcmpEcho<'static> = IcmpEcho {
        icmp_type: 0,
        ..REQUEST
    };

    fn encode(message: IcmpEcho<'_>) -> [u8; 16] {
        let mut output = [0; 16];
        assert_eq!(encode_echo(message, &mut output), Ok(output.len()));
        output
    }

    fn rewrite_checksum(message: &mut [u8; 16]) {
        message[2..4].fill(0);
        let checksum = icmp_checksum(message);
        message[2..4].copy_from_slice(&checksum.to_be_bytes());
    }

    #[test]
    fn encodes_the_exact_request_with_a8c6_checksum() {
        let request = encode(REQUEST);
        assert_eq!(request, REQUEST_BYTES);
        assert_eq!(icmp_checksum(&request), 0);
    }

    #[test]
    fn encodes_the_exact_reply_with_b0c6_checksum() {
        let reply = encode(REPLY);
        assert_eq!(reply, REPLY_BYTES);
        assert_eq!(icmp_checksum(&reply), 0);
    }

    #[test]
    fn round_trip_returns_a_borrowed_data_view() {
        let request = encode(REQUEST);
        let decoded = decode_echo(&request).unwrap();

        assert_eq!(decoded, REQUEST);
        assert_eq!(decoded.data.as_ptr(), request[8..].as_ptr());
    }

    #[test]
    fn checksum_handles_an_odd_trailing_byte_as_the_high_octet() {
        assert_eq!(icmp_checksum(&[0x12, 0x34, 0x56]), 0x97cb);
    }

    #[test]
    fn encoder_rejects_non_echo_type_and_nonzero_code() {
        let mut output = [0; 16];
        assert_eq!(
            encode_echo(
                IcmpEcho {
                    icmp_type: 3,
                    ..REQUEST
                },
                &mut output
            ),
            Err(EncodeError::UnsupportedType)
        );
        assert_eq!(
            encode_echo(IcmpEcho { code: 1, ..REQUEST }, &mut output),
            Err(EncodeError::InvalidCode)
        );
    }

    #[test]
    fn encoder_rejects_wrong_identity_and_data() {
        let mut output = [0; 16];
        assert_eq!(
            encode_echo(
                IcmpEcho {
                    identifier: 0x1404,
                    ..REQUEST
                },
                &mut output
            ),
            Err(EncodeError::InvalidIdentifier)
        );
        assert_eq!(
            encode_echo(
                IcmpEcho {
                    sequence: 2,
                    ..REQUEST
                },
                &mut output
            ),
            Err(EncodeError::InvalidSequence)
        );
        assert_eq!(
            encode_echo(
                IcmpEcho {
                    data: b"PYTHICM?",
                    ..REQUEST
                },
                &mut output
            ),
            Err(EncodeError::InvalidData)
        );
    }

    #[test]
    fn encoder_rejects_short_and_oversized_output() {
        assert_eq!(
            encode_echo(REQUEST, &mut [0; 15]),
            Err(EncodeError::OutputTooSmall)
        );
        assert_eq!(
            encode_echo(REQUEST, &mut [0; 17]),
            Err(EncodeError::OutputTooLarge)
        );
    }

    #[test]
    fn decoder_rejects_short_and_oversized_input() {
        assert_eq!(
            decode_echo(&REQUEST_BYTES[..15]),
            Err(DecodeError::InputTooShort)
        );
        let mut oversized = [0; 17];
        oversized[..16].copy_from_slice(&REQUEST_BYTES);
        assert_eq!(decode_echo(&oversized), Err(DecodeError::InputTooLarge));
    }

    #[test]
    fn decoder_rejects_non_echo_type_and_nonzero_code() {
        let mut wrong_type = REQUEST_BYTES;
        wrong_type[0] = 3;
        rewrite_checksum(&mut wrong_type);
        assert_eq!(decode_echo(&wrong_type), Err(DecodeError::UnsupportedType));

        let mut wrong_code = REQUEST_BYTES;
        wrong_code[1] = 1;
        rewrite_checksum(&mut wrong_code);
        assert_eq!(decode_echo(&wrong_code), Err(DecodeError::InvalidCode));
    }

    #[test]
    fn decoder_rejects_wrong_identity_and_data() {
        let mut wrong_identifier = REQUEST_BYTES;
        wrong_identifier[4..6].copy_from_slice(&0x1404_u16.to_be_bytes());
        rewrite_checksum(&mut wrong_identifier);
        assert_eq!(
            decode_echo(&wrong_identifier),
            Err(DecodeError::InvalidIdentifier)
        );

        let mut wrong_sequence = REQUEST_BYTES;
        wrong_sequence[6..8].copy_from_slice(&2_u16.to_be_bytes());
        rewrite_checksum(&mut wrong_sequence);
        assert_eq!(
            decode_echo(&wrong_sequence),
            Err(DecodeError::InvalidSequence)
        );

        let mut wrong_data = REQUEST_BYTES;
        wrong_data[15] = b'?';
        rewrite_checksum(&mut wrong_data);
        assert_eq!(decode_echo(&wrong_data), Err(DecodeError::InvalidData));
    }

    #[test]
    fn decoder_rejects_an_invalid_complete_checksum() {
        let mut request = REQUEST_BYTES;
        request[2] ^= 1;
        assert_eq!(decode_echo(&request), Err(DecodeError::BadChecksum));
    }
}
