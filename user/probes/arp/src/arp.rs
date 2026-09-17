pub const ARP_PAYLOAD_BYTES: usize = 28;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArpPacket {
    pub hardware_type: u16,
    pub protocol_type: u16,
    pub hardware_len: u8,
    pub protocol_len: u8,
    pub operation: u16,
    pub sender_hardware: [u8; 6],
    pub sender_protocol: [u8; 4],
    pub target_hardware: [u8; 6],
    pub target_protocol: [u8; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    TooShort,
}

pub fn parse(payload: &[u8]) -> Result<ArpPacket, ParseError> {
    if payload.len() < ARP_PAYLOAD_BYTES {
        return Err(ParseError::TooShort);
    }

    let mut sender_hardware = [0; 6];
    sender_hardware.copy_from_slice(&payload[8..14]);
    let mut sender_protocol = [0; 4];
    sender_protocol.copy_from_slice(&payload[14..18]);
    let mut target_hardware = [0; 6];
    target_hardware.copy_from_slice(&payload[18..24]);
    let mut target_protocol = [0; 4];
    target_protocol.copy_from_slice(&payload[24..28]);

    Ok(ArpPacket {
        hardware_type: u16::from_be_bytes([payload[0], payload[1]]),
        protocol_type: u16::from_be_bytes([payload[2], payload[3]]),
        hardware_len: payload[4],
        protocol_len: payload[5],
        operation: u16::from_be_bytes([payload[6], payload[7]]),
        sender_hardware,
        sender_protocol,
        target_hardware,
        target_protocol,
    })
}

pub fn encode(packet: ArpPacket) -> [u8; ARP_PAYLOAD_BYTES] {
    let mut payload = [0; ARP_PAYLOAD_BYTES];
    payload[0..2].copy_from_slice(&packet.hardware_type.to_be_bytes());
    payload[2..4].copy_from_slice(&packet.protocol_type.to_be_bytes());
    payload[4] = packet.hardware_len;
    payload[5] = packet.protocol_len;
    payload[6..8].copy_from_slice(&packet.operation.to_be_bytes());
    payload[8..14].copy_from_slice(&packet.sender_hardware);
    payload[14..18].copy_from_slice(&packet.sender_protocol);
    payload[18..24].copy_from_slice(&packet.target_hardware);
    payload[24..28].copy_from_slice(&packet.target_protocol);
    payload
}

#[cfg(test)]
mod tests {
    use super::{ARP_PAYLOAD_BYTES, ArpPacket, ParseError, encode, parse};

    const REQUEST_BYTES: [u8; 28] = [
        0x00, 0x01, 0x08, 0x00, 0x06, 0x04, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0xc0,
        0x00, 0x02, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0x00, 0x02, 0x01,
    ];

    const REQUEST_PACKET: ArpPacket = ArpPacket {
        hardware_type: 1,
        protocol_type: 0x0800,
        hardware_len: 6,
        protocol_len: 4,
        operation: 1,
        sender_hardware: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
        sender_protocol: [192, 0, 2, 2],
        target_hardware: [0; 6],
        target_protocol: [192, 0, 2, 1],
    };

    #[test]
    fn parse_decodes_network_byte_order_and_all_addresses() {
        assert_eq!(parse(&REQUEST_BYTES), Ok(REQUEST_PACKET));
    }

    #[test]
    fn encode_writes_the_28_byte_wire_layout() {
        assert_eq!(ARP_PAYLOAD_BYTES, REQUEST_BYTES.len());
        assert_eq!(encode(REQUEST_PACKET), REQUEST_BYTES);
    }

    #[test]
    fn parse_rejects_a_payload_shorter_than_28_bytes() {
        assert_eq!(parse(&REQUEST_BYTES[..27]), Err(ParseError::TooShort));
    }

    #[test]
    fn parse_ignores_ethernet_padding_after_the_28_byte_payload() {
        let mut padded = [0xa5; 46];
        padded[..REQUEST_BYTES.len()].copy_from_slice(&REQUEST_BYTES);

        assert_eq!(parse(&padded), Ok(REQUEST_PACKET));
    }
}
