#![no_std]

#[cfg(test)]
extern crate std;

pub mod udp;

#[cfg(test)]
mod tests {
    use super::udp::{
        DecodeError, EncodeError, UdpDatagram, decode_datagram, encode_datagram, udp_checksum,
    };

    const REQUEST_SOURCE: [u8; 4] = [192, 168, 14, 2];
    const REQUEST_DESTINATION: [u8; 4] = [192, 168, 14, 1];
    const REQUEST: UdpDatagram<'static> = UdpDatagram {
        source_port: 0x1405,
        destination_port: 0x1406,
        data: b"PYTHUDP",
    };
    const REPLY: UdpDatagram<'static> = UdpDatagram {
        source_port: 0x1406,
        destination_port: 0x1405,
        data: b"PYTHUDP",
    };
    const REQUEST_BYTES: [u8; 15] = [
        0x14, 0x05, 0x14, 0x06, 0x00, 0x0f, 0xf0, 0x8a, 0x50, 0x59, 0x54, 0x48, 0x55, 0x44, 0x50,
    ];
    const REPLY_BYTES: [u8; 15] = [
        0x14, 0x06, 0x14, 0x05, 0x00, 0x0f, 0xf0, 0x8a, 0x50, 0x59, 0x54, 0x48, 0x55, 0x44, 0x50,
    ];

    fn encode(datagram: UdpDatagram<'_>, source: [u8; 4], destination: [u8; 4]) -> [u8; 15] {
        let mut output = [0; 15];
        assert_eq!(
            encode_datagram(datagram, source, destination, &mut output),
            Ok(output.len())
        );
        output
    }

    #[test]
    fn encodes_the_exact_request_with_f08a_checksum() {
        let request = encode(REQUEST, REQUEST_SOURCE, REQUEST_DESTINATION);

        assert_eq!(request, REQUEST_BYTES);
        assert_eq!(
            udp_checksum(REQUEST_SOURCE, REQUEST_DESTINATION, &request),
            0
        );
    }

    #[test]
    fn encodes_the_exact_reply_with_f08a_checksum() {
        let reply = encode(REPLY, REQUEST_DESTINATION, REQUEST_SOURCE);

        assert_eq!(reply, REPLY_BYTES);
        assert_eq!(udp_checksum(REQUEST_DESTINATION, REQUEST_SOURCE, &reply), 0);
    }

    #[test]
    fn checksum_is_f08a_and_pads_the_odd_data_octet_for_arithmetic() {
        let mut unchecked = REQUEST_BYTES;
        unchecked[6..8].fill(0);

        assert_eq!(
            udp_checksum(REQUEST_SOURCE, REQUEST_DESTINATION, &unchecked),
            0xf08a
        );
        assert_eq!(
            udp_checksum(REQUEST_SOURCE, REQUEST_DESTINATION, &unchecked[..14]),
            0x408c
        );
    }

    #[test]
    fn round_trip_returns_a_borrowed_data_view() {
        let request = encode(REQUEST, REQUEST_SOURCE, REQUEST_DESTINATION);
        let decoded = decode_datagram(&request, REQUEST_SOURCE, REQUEST_DESTINATION).unwrap();

        assert_eq!(decoded, REQUEST);
        assert_eq!(decoded.data.as_ptr(), request[8..].as_ptr());
    }

    #[test]
    fn fields_use_network_byte_order() {
        let request = encode(REQUEST, REQUEST_SOURCE, REQUEST_DESTINATION);

        assert_eq!(&request[..2], &[0x14, 0x05]);
        assert_eq!(&request[2..4], &[0x14, 0x06]);
        assert_eq!(&request[4..6], &[0x00, 0x0f]);
        assert_eq!(&request[6..8], &[0xf0, 0x8a]);
    }

    #[test]
    fn encoder_rejects_short_and_overlong_output() {
        assert_eq!(
            encode_datagram(REQUEST, REQUEST_SOURCE, REQUEST_DESTINATION, &mut [0; 14]),
            Err(EncodeError::OutputTooSmall)
        );
        assert_eq!(
            encode_datagram(REQUEST, REQUEST_SOURCE, REQUEST_DESTINATION, &mut [0; 16]),
            Err(EncodeError::OutputTooLarge)
        );
    }

    #[test]
    fn decoder_rejects_short_and_overlong_input() {
        assert_eq!(
            decode_datagram(&REQUEST_BYTES[..14], REQUEST_SOURCE, REQUEST_DESTINATION),
            Err(DecodeError::InputTooShort)
        );
        let mut overlong = [0; 16];
        overlong[..15].copy_from_slice(&REQUEST_BYTES);
        assert_eq!(
            decode_datagram(&overlong, REQUEST_SOURCE, REQUEST_DESTINATION),
            Err(DecodeError::InputTooLarge)
        );
    }

    #[test]
    fn decoder_rejects_a_udp_length_mismatch() {
        let mut request = REQUEST_BYTES;
        request[4..6].copy_from_slice(&14_u16.to_be_bytes());

        assert_eq!(
            decode_datagram(&request, REQUEST_SOURCE, REQUEST_DESTINATION),
            Err(DecodeError::InvalidLength)
        );
    }

    #[test]
    fn decoder_rejects_bad_and_zero_checksums() {
        let mut bad_checksum = REQUEST_BYTES;
        bad_checksum[6] ^= 1;
        assert_eq!(
            decode_datagram(&bad_checksum, REQUEST_SOURCE, REQUEST_DESTINATION),
            Err(DecodeError::BadChecksum)
        );

        let mut zero_checksum = REQUEST_BYTES;
        zero_checksum[6..8].fill(0);
        assert_eq!(
            decode_datagram(&zero_checksum, REQUEST_SOURCE, REQUEST_DESTINATION),
            Err(DecodeError::ZeroChecksum)
        );
    }

    #[test]
    fn decoder_rejects_checksums_for_wrong_pseudo_header_addresses() {
        assert_eq!(
            decode_datagram(&REQUEST_BYTES, [192, 168, 14, 3], REQUEST_DESTINATION),
            Err(DecodeError::BadChecksum)
        );
        assert_eq!(
            decode_datagram(&REQUEST_BYTES, REQUEST_SOURCE, [192, 168, 14, 3]),
            Err(DecodeError::BadChecksum)
        );
    }

    #[test]
    fn decoder_rejects_a_checksum_computed_with_the_wrong_protocol() {
        let mut wrong_protocol = REQUEST_BYTES;
        wrong_protocol[6..8].fill(0);
        let checksum =
            checksum_with_protocol(REQUEST_SOURCE, REQUEST_DESTINATION, &wrong_protocol, 16);
        wrong_protocol[6..8].copy_from_slice(&checksum.to_be_bytes());

        assert_eq!(
            decode_datagram(&wrong_protocol, REQUEST_SOURCE, REQUEST_DESTINATION),
            Err(DecodeError::BadChecksum)
        );
    }

    #[test]
    fn decoder_rejects_a_udp_length_other_than_fifteen() {
        let mut wrong_length = REQUEST_BYTES;
        wrong_length[4..6].copy_from_slice(&16_u16.to_be_bytes());

        assert_eq!(
            decode_datagram(&wrong_length, REQUEST_SOURCE, REQUEST_DESTINATION),
            Err(DecodeError::InvalidLength)
        );
    }

    fn checksum_with_protocol(
        source: [u8; 4],
        destination: [u8; 4],
        datagram: &[u8],
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
        add(u16::from_be_bytes([0, datagram.len() as u8]));

        let mut words = datagram.chunks_exact(2);
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
