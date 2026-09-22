const UDP_HEADER_BYTES: usize = 8;
const UDP_DATAGRAM_BYTES: usize = 15;
const UDP_LENGTH: u16 = UDP_DATAGRAM_BYTES as u16;
const UDP_PROTOCOL: u8 = 17;
const REQUEST_SOURCE_PORT: u16 = 0x1405;
const REQUEST_DESTINATION_PORT: u16 = 0x1406;
const REQUEST_SOURCE_IPV4: [u8; 4] = [192, 168, 14, 2];
const REQUEST_DESTINATION_IPV4: [u8; 4] = [192, 168, 14, 1];
const UDP_DATA: &[u8; 7] = b"PYTHUDP";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UdpDatagram<'a> {
    pub source_port: u16,
    pub destination_port: u16,
    pub data: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    InvalidProfile,
    OutputTooSmall,
    OutputTooLarge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    InputTooShort,
    InputTooLarge,
    InvalidLength,
    ZeroChecksum,
    BadChecksum,
    InvalidProfile,
}

pub fn udp_checksum(source_ipv4: [u8; 4], destination_ipv4: [u8; 4], datagram: &[u8]) -> u16 {
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
    add_word(&mut sum, u16::from(UDP_PROTOCOL));
    add_word(&mut sum, u16::try_from(datagram.len()).unwrap_or(u16::MAX));

    let mut words = datagram.chunks_exact(2);
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

pub fn encode_datagram(
    datagram: UdpDatagram<'_>,
    source_ipv4: [u8; 4],
    destination_ipv4: [u8; 4],
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    validate_profile(datagram, source_ipv4, destination_ipv4)
        .map_err(|_| EncodeError::InvalidProfile)?;
    if output.len() < UDP_DATAGRAM_BYTES {
        return Err(EncodeError::OutputTooSmall);
    }
    if output.len() > UDP_DATAGRAM_BYTES {
        return Err(EncodeError::OutputTooLarge);
    }

    output[..2].copy_from_slice(&datagram.source_port.to_be_bytes());
    output[2..4].copy_from_slice(&datagram.destination_port.to_be_bytes());
    output[4..6].copy_from_slice(&UDP_LENGTH.to_be_bytes());
    output[6..8].fill(0);
    output[UDP_HEADER_BYTES..].copy_from_slice(datagram.data);

    let checksum = udp_checksum(source_ipv4, destination_ipv4, output);
    output[6..8].copy_from_slice(&checksum.to_be_bytes());
    Ok(UDP_DATAGRAM_BYTES)
}

pub fn decode_datagram(
    bytes: &[u8],
    source_ipv4: [u8; 4],
    destination_ipv4: [u8; 4],
) -> Result<UdpDatagram<'_>, DecodeError> {
    if bytes.len() < UDP_DATAGRAM_BYTES {
        return Err(DecodeError::InputTooShort);
    }
    if bytes.len() > UDP_DATAGRAM_BYTES {
        return Err(DecodeError::InputTooLarge);
    }

    let length = u16::from_be_bytes([bytes[4], bytes[5]]);
    if length != UDP_LENGTH {
        return Err(DecodeError::InvalidLength);
    }
    let checksum = u16::from_be_bytes([bytes[6], bytes[7]]);
    if checksum == 0 {
        return Err(DecodeError::ZeroChecksum);
    }
    if udp_checksum(source_ipv4, destination_ipv4, bytes) != 0 {
        return Err(DecodeError::BadChecksum);
    }

    let datagram = UdpDatagram {
        source_port: u16::from_be_bytes([bytes[0], bytes[1]]),
        destination_port: u16::from_be_bytes([bytes[2], bytes[3]]),
        data: bytes
            .get(UDP_HEADER_BYTES..UDP_DATAGRAM_BYTES)
            .ok_or(DecodeError::InputTooShort)?,
    };
    validate_profile(datagram, source_ipv4, destination_ipv4)
        .map_err(|_| DecodeError::InvalidProfile)?;

    Ok(datagram)
}

fn add_word(sum: &mut u32, word: u16) {
    *sum += u32::from(word);
    *sum = (*sum & u32::from(u16::MAX)) + (*sum >> 16);
}

fn validate_profile(
    datagram: UdpDatagram<'_>,
    source_ipv4: [u8; 4],
    destination_ipv4: [u8; 4],
) -> Result<(), ()> {
    let is_request = datagram.source_port == REQUEST_SOURCE_PORT
        && datagram.destination_port == REQUEST_DESTINATION_PORT
        && source_ipv4 == REQUEST_SOURCE_IPV4
        && destination_ipv4 == REQUEST_DESTINATION_IPV4;
    let is_reply = datagram.source_port == REQUEST_DESTINATION_PORT
        && datagram.destination_port == REQUEST_SOURCE_PORT
        && source_ipv4 == REQUEST_DESTINATION_IPV4
        && destination_ipv4 == REQUEST_SOURCE_IPV4;

    if (is_request || is_reply) && datagram.data == UDP_DATA {
        Ok(())
    } else {
        Err(())
    }
}
