pub const ETHERNET_HEADER_BYTES: usize = 14;
pub const MIN_FRAME_BYTES: usize = 60;
pub const MAX_FRAME_BYTES: usize = 1514;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EthernetFrame<'a> {
    pub destination: [u8; 6],
    pub source: [u8; 6],
    pub ether_type: u16,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    TooShort,
    TooLong,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    PayloadTooLong,
}

pub fn parse(frame: &[u8]) -> Result<EthernetFrame<'_>, ParseError> {
    if frame.len() < MIN_FRAME_BYTES {
        return Err(ParseError::TooShort);
    }
    if frame.len() > MAX_FRAME_BYTES {
        return Err(ParseError::TooLong);
    }

    let mut destination = [0; 6];
    destination.copy_from_slice(&frame[0..6]);
    let mut source = [0; 6];
    source.copy_from_slice(&frame[6..12]);

    Ok(EthernetFrame {
        destination,
        source,
        ether_type: u16::from_be_bytes([frame[12], frame[13]]),
        payload: &frame[ETHERNET_HEADER_BYTES..],
    })
}

pub fn encode_minimum_frame(
    destination: [u8; 6],
    source: [u8; 6],
    ether_type: u16,
    payload: &[u8],
) -> Result<[u8; MIN_FRAME_BYTES], EncodeError> {
    if payload.len() > MIN_FRAME_BYTES - ETHERNET_HEADER_BYTES {
        return Err(EncodeError::PayloadTooLong);
    }

    let mut frame = [0; MIN_FRAME_BYTES];
    frame[0..6].copy_from_slice(&destination);
    frame[6..12].copy_from_slice(&source);
    frame[12..14].copy_from_slice(&ether_type.to_be_bytes());
    frame[ETHERNET_HEADER_BYTES..ETHERNET_HEADER_BYTES + payload.len()].copy_from_slice(payload);
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::{EncodeError, EthernetFrame, ParseError, encode_minimum_frame, parse};

    const DESTINATION: [u8; 6] = [0, 1, 2, 3, 4, 5];
    const SOURCE: [u8; 6] = [6, 7, 8, 9, 10, 11];

    fn frame_with_length(length: usize) -> std::vec::Vec<u8> {
        let mut frame = std::vec![0; length];
        if length >= 6 {
            frame[0..6].copy_from_slice(&DESTINATION);
        }
        if length >= 12 {
            frame[6..12].copy_from_slice(&SOURCE);
        }
        if length >= 14 {
            frame[12..14].copy_from_slice(&0x88B5_u16.to_be_bytes());
        }
        frame
    }

    #[test]
    fn rejects_frames_shorter_than_the_ethernet_minimum() {
        assert_eq!(parse(&frame_with_length(13)), Err(ParseError::TooShort));
        assert_eq!(parse(&frame_with_length(59)), Err(ParseError::TooShort));
    }

    #[test]
    fn accepts_the_minimum_and_maximum_frame_lengths() {
        assert!(parse(&frame_with_length(60)).is_ok());
        assert!(parse(&frame_with_length(1514)).is_ok());
    }

    #[test]
    fn rejects_frames_longer_than_the_bounded_maximum() {
        assert_eq!(parse(&frame_with_length(1515)), Err(ParseError::TooLong));
    }

    #[test]
    fn decodes_ethertype_in_network_byte_order() {
        assert_eq!(parse(&frame_with_length(60)).unwrap().ether_type, 0x88B5);
    }

    #[test]
    fn borrows_the_payload_from_the_input_frame() {
        let mut frame = frame_with_length(60);
        frame[14] = 0xA5;
        let parsed = parse(&frame).unwrap();
        assert_eq!(parsed.payload, &frame[14..]);
        assert_eq!(parsed.payload.as_ptr(), frame[14..].as_ptr());
    }

    #[test]
    fn encodes_headers_and_zero_padding_into_a_minimum_frame() {
        let frame = encode_minimum_frame(DESTINATION, SOURCE, 0x88B5, b"hello").unwrap();
        assert_eq!(&frame[0..6], &DESTINATION);
        assert_eq!(&frame[6..12], &SOURCE);
        assert_eq!(&frame[12..14], &0x88B5_u16.to_be_bytes());
        assert_eq!(&frame[14..19], b"hello");
        assert!(frame[19..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn accepts_payloads_up_to_the_minimum_frame_payload_capacity() {
        let payload = [0x5A; 46];
        let frame = encode_minimum_frame(DESTINATION, SOURCE, 0x88B5, &payload).unwrap();
        assert_eq!(&frame[14..], &payload);
    }

    #[test]
    fn rejects_payloads_longer_than_the_minimum_frame_payload_capacity() {
        assert_eq!(
            encode_minimum_frame(DESTINATION, SOURCE, 0x88B5, &[0; 47]),
            Err(EncodeError::PayloadTooLong)
        );
    }

    #[test]
    fn parsed_frame_contains_the_two_mac_addresses_and_payload() {
        let frame = frame_with_length(60);
        assert_eq!(
            parse(&frame).unwrap(),
            EthernetFrame {
                destination: DESTINATION,
                source: SOURCE,
                ether_type: 0x88B5,
                payload: &frame[14..],
            }
        );
    }
}
