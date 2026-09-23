//! Allocation-free codec for the single Phase 14 DNS proof exchange.

pub const DNS_HEADER_BYTES: usize = 12;
pub const DNS_QUESTION_BYTES: usize = 20;
pub const DNS_QUERY_BYTES: usize = DNS_HEADER_BYTES + DNS_QUESTION_BYTES;
pub const DNS_ANSWER_BYTES: usize = 16;
pub const DNS_RESPONSE_BYTES: usize = DNS_QUERY_BYTES + DNS_ANSWER_BYTES;

pub const DNS_TRANSACTION_ID: u16 = 0xD14E;
pub const DNS_QUERY_FLAGS: u16 = 0x0100;
pub const DNS_RESPONSE_FLAGS: u16 = 0x8180;
pub const DNS_TYPE_A: u16 = 0x0001;
pub const DNS_CLASS_IN: u16 = 0x0001;
pub const DNS_ANSWER_POINTER: u16 = 0xC00C;
pub const DNS_TTL: u32 = 60;
pub const DNS_ANSWER_IPV4: [u8; 4] = [192, 0, 2, 14];

const QUERY_QNAME: &[u8] = b"\x06pythos\x07example\x00";
const QUESTION: [u8; DNS_QUESTION_BYTES] = [
    0x06, b'p', b'y', b't', b'h', b'o', b's', 0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x00,
    0x00, 0x01, 0x00, 0x01,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DnsQuery<'a> {
    question: &'a [u8],
}

impl<'a> DnsQuery<'a> {
    pub const fn question(self) -> &'a [u8] {
        self.question
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DnsResponse<'a> {
    question: &'a [u8],
    answer_ipv4: &'a [u8],
}

impl<'a> DnsResponse<'a> {
    pub const fn question(self) -> &'a [u8] {
        self.question
    }

    pub const fn answer_ipv4(self) -> &'a [u8] {
        self.answer_ipv4
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    OutputTooSmall,
    OutputTooLarge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    InputTooShort,
    InputTooLarge,
    WrongTransactionId,
    WrongFlags,
    WrongCounts,
    MalformedLabels,
    UnsupportedType,
    UnsupportedClass,
    InvalidQuestion,
    InvalidAnswerPointer,
    InvalidTtl,
    InvalidAddress,
    InvalidQuestionMatch,
}

pub fn encode_query(output: &mut [u8]) -> Result<usize, EncodeError> {
    if output.len() < DNS_QUERY_BYTES {
        return Err(EncodeError::OutputTooSmall);
    }
    if output.len() > DNS_QUERY_BYTES {
        return Err(EncodeError::OutputTooLarge);
    }

    output[..2].copy_from_slice(&DNS_TRANSACTION_ID.to_be_bytes());
    output[2..4].copy_from_slice(&DNS_QUERY_FLAGS.to_be_bytes());
    output[4..6].copy_from_slice(&1_u16.to_be_bytes());
    output[6..12].fill(0);
    output[DNS_HEADER_BYTES..].copy_from_slice(&QUESTION);
    Ok(DNS_QUERY_BYTES)
}

pub fn decode_query(bytes: &[u8]) -> Result<DnsQuery<'_>, DecodeError> {
    if bytes.len() < DNS_QUERY_BYTES {
        return Err(DecodeError::InputTooShort);
    }
    if bytes.len() > DNS_QUERY_BYTES {
        return Err(DecodeError::InputTooLarge);
    }

    validate_header(bytes, DNS_QUERY_FLAGS, 0, 0, 0)?;
    let question = validate_question(bytes, DNS_HEADER_BYTES)?;
    Ok(DnsQuery { question })
}

pub fn decode_response<'a>(
    bytes: &'a [u8],
    expected_query: &[u8],
) -> Result<DnsResponse<'a>, DecodeError> {
    if bytes.len() < DNS_RESPONSE_BYTES {
        return Err(DecodeError::InputTooShort);
    }
    if bytes.len() > DNS_RESPONSE_BYTES {
        return Err(DecodeError::InputTooLarge);
    }
    let query = decode_query(expected_query)?;

    validate_header(bytes, DNS_RESPONSE_FLAGS, 1, 0, 0)?;
    let question = validate_question(bytes, DNS_HEADER_BYTES)?;
    if question != query.question() {
        return Err(DecodeError::InvalidQuestionMatch);
    }

    if read_u16(bytes, 32) != DNS_ANSWER_POINTER {
        return Err(DecodeError::InvalidAnswerPointer);
    }
    if read_u16(bytes, 34) != DNS_TYPE_A {
        return Err(DecodeError::UnsupportedType);
    }
    if read_u16(bytes, 36) != DNS_CLASS_IN {
        return Err(DecodeError::UnsupportedClass);
    }
    if read_u32(bytes, 38) != DNS_TTL {
        return Err(DecodeError::InvalidTtl);
    }
    if read_u16(bytes, 42) != DNS_ANSWER_IPV4.len() as u16 {
        return Err(DecodeError::InvalidAddress);
    }
    let answer_ipv4 = bytes.get(44..48).ok_or(DecodeError::InputTooShort)?;
    if answer_ipv4 != DNS_ANSWER_IPV4 {
        return Err(DecodeError::InvalidAddress);
    }

    Ok(DnsResponse {
        question,
        answer_ipv4,
    })
}

fn validate_header(
    bytes: &[u8],
    expected_flags: u16,
    expected_answers: u16,
    expected_authority: u16,
    expected_additional: u16,
) -> Result<(), DecodeError> {
    if read_u16(bytes, 0) != DNS_TRANSACTION_ID {
        return Err(DecodeError::WrongTransactionId);
    }
    if read_u16(bytes, 2) != expected_flags {
        return Err(DecodeError::WrongFlags);
    }
    if read_u16(bytes, 4) != 1
        || read_u16(bytes, 6) != expected_answers
        || read_u16(bytes, 8) != expected_authority
        || read_u16(bytes, 10) != expected_additional
    {
        return Err(DecodeError::WrongCounts);
    }
    Ok(())
}

fn validate_question(bytes: &[u8], offset: usize) -> Result<&[u8], DecodeError> {
    let end = offset
        .checked_add(DNS_QUESTION_BYTES)
        .ok_or(DecodeError::MalformedLabels)?;
    let question = bytes.get(offset..end).ok_or(DecodeError::InputTooShort)?;
    if question[..QUERY_QNAME.len()] != *QUERY_QNAME {
        return Err(DecodeError::MalformedLabels);
    }
    if read_u16(question, QUERY_QNAME.len()) != DNS_TYPE_A {
        return Err(DecodeError::UnsupportedType);
    }
    if read_u16(question, QUERY_QNAME.len() + 2) != DNS_CLASS_IN {
        return Err(DecodeError::UnsupportedClass);
    }
    if question.len() != DNS_QUESTION_BYTES {
        return Err(DecodeError::InvalidQuestion);
    }
    Ok(question)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUERY_BYTES: [u8; DNS_QUERY_BYTES] = [
        0xd1, 0x4e, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x70, 0x79,
        0x74, 0x68, 0x6f, 0x73, 0x07, 0x65, 0x78, 0x61, 0x6d, 0x70, 0x6c, 0x65, 0x00, 0x00, 0x01,
        0x00, 0x01,
    ];
    const RESPONSE_BYTES: [u8; DNS_RESPONSE_BYTES] = [
        0xd1, 0x4e, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x06, 0x70, 0x79,
        0x74, 0x68, 0x6f, 0x73, 0x07, 0x65, 0x78, 0x61, 0x6d, 0x70, 0x6c, 0x65, 0x00, 0x00, 0x01,
        0x00, 0x01, 0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x04, 0xc0,
        0x00, 0x02, 0x0e,
    ];

    #[test]
    fn encodes_exact_query_bytes_and_rejects_wrong_bounds() {
        let mut output = [0u8; DNS_QUERY_BYTES];
        assert_eq!(encode_query(&mut output), Ok(DNS_QUERY_BYTES));
        assert_eq!(output, QUERY_BYTES);
        assert_eq!(
            encode_query(&mut [0u8; DNS_QUERY_BYTES - 1]),
            Err(EncodeError::OutputTooSmall)
        );
        assert_eq!(
            encode_query(&mut [0u8; DNS_QUERY_BYTES + 1]),
            Err(EncodeError::OutputTooLarge)
        );
    }

    #[test]
    fn decodes_query_and_response_with_borrowed_question() {
        let mut query_bytes = [0u8; DNS_QUERY_BYTES];
        encode_query(&mut query_bytes).unwrap();
        let query = decode_query(&query_bytes).unwrap();
        assert_eq!(query.question(), &QUERY_BYTES[DNS_HEADER_BYTES..]);
        assert_eq!(
            query.question().as_ptr(),
            query_bytes[DNS_HEADER_BYTES..].as_ptr()
        );

        let response = decode_response(&RESPONSE_BYTES, &query_bytes).unwrap();
        assert_eq!(response.question(), &RESPONSE_BYTES[DNS_HEADER_BYTES..32]);
        assert_eq!(
            response.question().as_ptr(),
            RESPONSE_BYTES[DNS_HEADER_BYTES..].as_ptr()
        );
        assert_eq!(response.answer_ipv4(), &DNS_ANSWER_IPV4);
        assert_eq!(
            response.answer_ipv4().as_ptr(),
            RESPONSE_BYTES[44..].as_ptr()
        );
    }

    #[test]
    fn response_rejects_short_and_overlong_messages() {
        assert_eq!(
            decode_response(&RESPONSE_BYTES[..DNS_RESPONSE_BYTES - 1], &QUERY_BYTES),
            Err(DecodeError::InputTooShort)
        );
        let mut overlong = [0u8; DNS_RESPONSE_BYTES + 1];
        overlong[..DNS_RESPONSE_BYTES].copy_from_slice(&RESPONSE_BYTES);
        assert_eq!(
            decode_response(&overlong, &QUERY_BYTES),
            Err(DecodeError::InputTooLarge)
        );
    }

    #[test]
    fn response_rejects_header_flags_counts_and_transaction_id() {
        let mut cases = RESPONSE_BYTES;
        cases[0] ^= 1;
        assert_eq!(
            decode_response(&cases, &QUERY_BYTES),
            Err(DecodeError::WrongTransactionId)
        );

        let mut cases = RESPONSE_BYTES;
        cases[2] ^= 1;
        assert_eq!(
            decode_response(&cases, &QUERY_BYTES),
            Err(DecodeError::WrongFlags)
        );

        let mut cases = RESPONSE_BYTES;
        cases[7] = 0;
        assert_eq!(
            decode_response(&cases, &QUERY_BYTES),
            Err(DecodeError::WrongCounts)
        );
    }

    #[test]
    fn response_rejects_malformed_labels_unexpected_compression_and_question_mismatch() {
        let mut malformed = RESPONSE_BYTES;
        malformed[12] = 0xc0;
        assert_eq!(
            decode_response(&malformed, &QUERY_BYTES),
            Err(DecodeError::MalformedLabels)
        );

        let mut mismatch = RESPONSE_BYTES;
        mismatch[19] = b'X';
        assert_eq!(
            decode_response(&mismatch, &QUERY_BYTES),
            Err(DecodeError::MalformedLabels)
        );

        let mut query_mismatch = QUERY_BYTES;
        query_mismatch[19] = b'X';
        assert_eq!(
            decode_response(&RESPONSE_BYTES, &query_mismatch),
            Err(DecodeError::MalformedLabels)
        );
    }

    #[test]
    fn response_rejects_unsupported_types_classes_pointer_ttl_and_address() {
        let mut unsupported = RESPONSE_BYTES;
        unsupported[35] = 0;
        assert_eq!(
            decode_response(&unsupported, &QUERY_BYTES),
            Err(DecodeError::UnsupportedType)
        );

        let mut unsupported = RESPONSE_BYTES;
        unsupported[37] = 0;
        assert_eq!(
            decode_response(&unsupported, &QUERY_BYTES),
            Err(DecodeError::UnsupportedClass)
        );

        let mut bad_pointer = RESPONSE_BYTES;
        bad_pointer[32] = 0xc0;
        bad_pointer[33] = 0x0d;
        assert_eq!(
            decode_response(&bad_pointer, &QUERY_BYTES),
            Err(DecodeError::InvalidAnswerPointer)
        );

        let mut bad_ttl = RESPONSE_BYTES;
        bad_ttl[41] = 0x3d;
        assert_eq!(
            decode_response(&bad_ttl, &QUERY_BYTES),
            Err(DecodeError::InvalidTtl)
        );

        let mut bad_address = RESPONSE_BYTES;
        bad_address[47] = 0x0f;
        assert_eq!(
            decode_response(&bad_address, &QUERY_BYTES),
            Err(DecodeError::InvalidAddress)
        );
    }

    #[test]
    fn query_decoder_rejects_unsupported_types_and_classes() {
        let mut unsupported = QUERY_BYTES;
        unsupported[29] = 0;
        assert_eq!(
            decode_query(&unsupported),
            Err(DecodeError::UnsupportedType)
        );

        let mut unsupported = QUERY_BYTES;
        unsupported[31] = 0;
        assert_eq!(
            decode_query(&unsupported),
            Err(DecodeError::UnsupportedClass)
        );
    }
}
