#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(dead_code, unused_imports))]

#[cfg(not(test))]
use core::panic::PanicInfo;
use core::{arch::asm, cell::UnsafeCell, mem::size_of};
use pythos_shared::{
    capability_abi::PackedCapability,
    network_port_abi::{
        NETWORK_PORT_ABI_MAJOR, NETWORK_PORT_ABI_MINOR, NETWORK_PORT_BOOTSTRAP_MAGIC,
        NETWORK_PORT_FLAG_MAC_ONLY, NETWORK_PORT_FLAG_NO_OFFLOAD, NETWORK_PORT_MAX_FRAME_BYTES,
        NETWORK_PORT_MIN_FRAME_BYTES, NETWORK_PORT_OP_DESCRIBE, NETWORK_PORT_OP_SEND,
        NETWORK_PORT_OP_TRY_RECEIVE, NETWORK_PORT_STATE_READY, NETWORK_PORT_STATUS_EMPTY,
        NETWORK_PORT_STATUS_OK, NETWORK_PORT_STATUS_TRANSPORT_ERROR, NetworkPortBootstrapV1,
        NetworkPortDescriptionV1, NetworkPortRequestV1, NetworkPortResponseV1,
        SYSCALL_NETWORK_PORT_REQUEST,
    },
    object_shell_abi::{SYSCALL_CONSOLE_WRITE_BYTE, SYSCALL_OK},
    udp_markers::{
        UDP_ARP_SETUP_OK_MARKER, UDP_BOOTSTRAPPED_MARKER, UDP_DESCRIBE_OK_MARKER, UDP_RX_OK_MARKER,
        UDP_TX_OK_MARKER,
    },
};
use pythos_user_arp_probe::arp::{ARP_PAYLOAD_BYTES, ArpPacket, encode, parse};
use pythos_user_ipv4_probe::ipv4::{Ipv4Header, decode_ipv4_packet, encode_ipv4_packet};
use pythos_user_link_layer_probe::ethernet;
use pythos_user_udp_probe::udp::{UdpDatagram, decode_datagram, encode_datagram};

const BROADCAST_MAC: [u8; 6] = [0xff; 6];
const LOCAL_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
const PEER_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x02];
const LOCAL_IPV4: [u8; 4] = [192, 168, 14, 2];
const PEER_IPV4: [u8; 4] = [192, 168, 14, 1];
const ARP_ETHER_TYPE: u16 = 0x0806;
const IPV4_ETHER_TYPE: u16 = 0x0800;
const UDP_PROTOCOL: u8 = 17;
const UDP_SOURCE_PORT: u16 = 0x1405;
const UDP_DESTINATION_PORT: u16 = 0x1406;
const UDP_DATA: &[u8; 7] = b"PYTHUDP";
const IPV4_REQUEST_ID: u16 = 0x1405;
const IPV4_REPLY_ID: u16 = 0x1406;
const IPV4_TTL: u8 = 64;
const UDP_DATAGRAM_BYTES: usize = 15;
const IPV4_DATAGRAM_BYTES: usize = 20 + UDP_DATAGRAM_BYTES;
const MAX_RECEIVE_POLL_ATTEMPTS: usize = 1024;
const UDP_ERROR_MARKER: &str = "PYTHOS:CORE:UDP:ERROR";

struct ProbeStorage(UnsafeCell<ProbeBuffers>);

struct ProbeBuffers {
    request: NetworkPortRequestV1,
    response: NetworkPortResponseV1,
    description: NetworkPortDescriptionV1,
    tx: [u8; NETWORK_PORT_MIN_FRAME_BYTES],
    rx: [u8; NETWORK_PORT_MAX_FRAME_BYTES],
}

// SAFETY: the finite native probe has one thread and accesses this storage serially.
unsafe impl Sync for ProbeStorage {}

static STORAGE: ProbeStorage = ProbeStorage(UnsafeCell::new(ProbeBuffers {
    request: NetworkPortRequestV1::new(0, PackedCapability::from_raw(0)),
    response: NetworkPortResponseV1::new(0, 0),
    description: NetworkPortDescriptionV1::empty(),
    tx: [0; NETWORK_PORT_MIN_FRAME_BYTES],
    rx: [0; NETWORK_PORT_MAX_FRAME_BYTES],
}));

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn _start(bootstrap_ptr: u64, console_raw: u64) -> ! {
    let console = PackedCapability::from_raw(console_raw);
    // SAFETY: PythCore supplies this aligned, read-only NetworkPortBootstrapV1 page.
    let bootstrap = unsafe { (bootstrap_ptr as *const NetworkPortBootstrapV1).read() };
    if !valid_bootstrap(bootstrap) {
        error(console);
    }
    write_marker(console, UDP_BOOTSTRAPPED_MARKER);

    let capability = bootstrap.port_capability;
    if describe(capability) != NETWORK_PORT_STATUS_OK || !valid_description() {
        error(console);
    }
    write_marker(console, UDP_DESCRIBE_OK_MARKER);

    if send_frame(capability, arp_request_frame(LOCAL_MAC)) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    receive_matching_frame(capability, console, arp_reply_frame_matches);
    write_marker(console, UDP_ARP_SETUP_OK_MARKER);

    let udp_frame = match udp_request_frame(LOCAL_MAC) {
        Some(frame) => frame,
        None => error(console),
    };
    if send_frame(capability, udp_frame) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    write_marker(console, UDP_TX_OK_MARKER);

    receive_matching_frame(capability, console, udp_reply_frame_matches);
    write_marker(console, UDP_RX_OK_MARKER);
    success_breakpoint();
}

fn valid_bootstrap(bootstrap: NetworkPortBootstrapV1) -> bool {
    bootstrap.magic == NETWORK_PORT_BOOTSTRAP_MAGIC
        && bootstrap.abi_major == NETWORK_PORT_ABI_MAJOR
        && bootstrap.abi_minor == NETWORK_PORT_ABI_MINOR
        && bootstrap.reserved0 == 0
        && bootstrap.reserved == [0; 5]
        && bootstrap.port_capability.raw() != 0
}

fn valid_description() -> bool {
    // SAFETY: the single-threaded probe exclusively accesses fixed storage between syscalls.
    let description = unsafe { &*STORAGE.0.get() }.description;
    valid_description_fields(description)
}

fn valid_description_fields(description: NetworkPortDescriptionV1) -> bool {
    description.mac == LOCAL_MAC
        && description.reserved0 == [0; 2]
        && description.reserved1 == 0
        && description.min_frame_bytes as usize == NETWORK_PORT_MIN_FRAME_BYTES
        && description.max_frame_bytes as usize == NETWORK_PORT_MAX_FRAME_BYTES
        && description.transport_flags == NETWORK_PORT_FLAG_MAC_ONLY | NETWORK_PORT_FLAG_NO_OFFLOAD
        && description.state == u32::from(NETWORK_PORT_STATE_READY)
}

fn arp_request_packet(local_mac: [u8; 6]) -> ArpPacket {
    ArpPacket {
        hardware_type: 1,
        protocol_type: IPV4_ETHER_TYPE,
        hardware_len: 6,
        protocol_len: 4,
        operation: 1,
        sender_hardware: local_mac,
        sender_protocol: LOCAL_IPV4,
        target_hardware: [0; 6],
        target_protocol: PEER_IPV4,
    }
}

fn arp_request_frame(local_mac: [u8; 6]) -> [u8; NETWORK_PORT_MIN_FRAME_BYTES] {
    let mut frame = [0; NETWORK_PORT_MIN_FRAME_BYTES];
    frame[..6].copy_from_slice(&BROADCAST_MAC);
    frame[6..12].copy_from_slice(&local_mac);
    frame[12..14].copy_from_slice(&ARP_ETHER_TYPE.to_be_bytes());
    frame[14..14 + ARP_PAYLOAD_BYTES].copy_from_slice(&encode(arp_request_packet(local_mac)));
    frame
}

fn arp_reply_packet_matches(packet: ArpPacket, local_mac: [u8; 6]) -> bool {
    packet.hardware_type == 1
        && packet.protocol_type == IPV4_ETHER_TYPE
        && packet.hardware_len == 6
        && packet.protocol_len == 4
        && packet.operation == 2
        && packet.sender_hardware == PEER_MAC
        && packet.sender_protocol == PEER_IPV4
        && packet.target_hardware == local_mac
        && packet.target_protocol == LOCAL_IPV4
}

fn arp_reply_frame_matches(frame_bytes: &[u8], local_mac: [u8; 6]) -> bool {
    if frame_bytes.len() != NETWORK_PORT_MIN_FRAME_BYTES
        || !frame_bytes[42..].iter().all(|byte| *byte == 0)
    {
        return false;
    }
    let frame = match ethernet::parse(frame_bytes) {
        Ok(frame) => frame,
        Err(_) => return false,
    };
    if frame.destination != local_mac
        || frame.source != PEER_MAC
        || frame.ether_type != ARP_ETHER_TYPE
    {
        return false;
    }
    match parse(&frame.payload[..ARP_PAYLOAD_BYTES]) {
        Ok(packet) => arp_reply_packet_matches(packet, local_mac),
        Err(_) => false,
    }
}

fn udp_request_frame(local_mac: [u8; 6]) -> Option<[u8; NETWORK_PORT_MIN_FRAME_BYTES]> {
    let datagram = UdpDatagram {
        source_port: UDP_SOURCE_PORT,
        destination_port: UDP_DESTINATION_PORT,
        data: UDP_DATA,
    };
    let mut udp_bytes = [0; UDP_DATAGRAM_BYTES];
    if encode_datagram(datagram, LOCAL_IPV4, PEER_IPV4, &mut udp_bytes).ok()? != UDP_DATAGRAM_BYTES
    {
        return None;
    }

    let header = Ipv4Header {
        version: 4,
        ihl: 5,
        dscp_ecn: 0,
        identification: IPV4_REQUEST_ID,
        flags_fragment_offset: 0,
        ttl: IPV4_TTL,
        protocol: UDP_PROTOCOL,
        source: LOCAL_IPV4,
        destination: PEER_IPV4,
    };
    let mut ipv4_bytes = [0; IPV4_DATAGRAM_BYTES];
    if encode_ipv4_packet(header, &udp_bytes, &mut ipv4_bytes).ok()? != IPV4_DATAGRAM_BYTES {
        return None;
    }
    ethernet::encode_minimum_frame(PEER_MAC, local_mac, IPV4_ETHER_TYPE, &ipv4_bytes).ok()
}

fn udp_reply_frame_matches(frame_bytes: &[u8], local_mac: [u8; 6]) -> bool {
    if frame_bytes.len() != NETWORK_PORT_MIN_FRAME_BYTES
        || !frame_bytes[14 + IPV4_DATAGRAM_BYTES..]
            .iter()
            .all(|byte| *byte == 0)
    {
        return false;
    }
    let frame = match ethernet::parse(frame_bytes) {
        Ok(frame) => frame,
        Err(_) => return false,
    };
    if frame.destination != local_mac
        || frame.source != PEER_MAC
        || frame.ether_type != IPV4_ETHER_TYPE
    {
        return false;
    }
    let packet = match decode_ipv4_packet(&frame.payload[..IPV4_DATAGRAM_BYTES]) {
        Ok(packet) => packet,
        Err(_) => return false,
    };
    if packet.header.version != 4
        || packet.header.ihl != 5
        || packet.header.dscp_ecn != 0
        || packet.total_length as usize != IPV4_DATAGRAM_BYTES
        || packet.header.identification != IPV4_REPLY_ID
        || packet.header.flags_fragment_offset != 0
        || packet.header.ttl != IPV4_TTL
        || packet.header.protocol != UDP_PROTOCOL
        || packet.header.source != PEER_IPV4
        || packet.header.destination != LOCAL_IPV4
        || packet.header_checksum != 0xc970
    {
        return false;
    }
    match decode_datagram(packet.payload, PEER_IPV4, LOCAL_IPV4) {
        Ok(datagram) => {
            datagram.source_port == UDP_DESTINATION_PORT
                && datagram.destination_port == UDP_SOURCE_PORT
                && datagram.data == UDP_DATA
        }
        Err(_) => false,
    }
}

fn empty_receive_poll_exhausted(empty_polls: &mut usize) -> bool {
    *empty_polls = empty_polls.saturating_add(1);
    *empty_polls >= MAX_RECEIVE_POLL_ATTEMPTS
}

fn received_frame_is_terminal_failure(
    frame_bytes: &[u8],
    local_mac: [u8; 6],
    matches: fn(&[u8], [u8; 6]) -> bool,
) -> bool {
    !matches(frame_bytes, local_mac)
}

fn receive_matching_frame(
    capability: PackedCapability,
    console: PackedCapability,
    matches: fn(&[u8], [u8; 6]) -> bool,
) {
    let mut empty_polls = 0;
    loop {
        let status = receive(capability);
        if status == NETWORK_PORT_STATUS_EMPTY {
            if empty_receive_poll_exhausted(&mut empty_polls) {
                error(console);
            }
            core::hint::spin_loop();
            continue;
        }
        if status != NETWORK_PORT_STATUS_OK {
            error(console);
        }

        // SAFETY: `receive` writes only the fixed RX buffer and returns a bounded frame length.
        let buffers = unsafe { &*STORAGE.0.get() };
        let frame_len = buffers.response.frame_len as usize;
        if frame_len != NETWORK_PORT_MIN_FRAME_BYTES
            || received_frame_is_terminal_failure(
                &buffers.rx[..frame_len],
                buffers.description.mac,
                matches,
            )
        {
            error(console);
        }
        return;
    }
}

fn describe(capability: PackedCapability) -> u16 {
    // SAFETY: the single-threaded probe exclusively accesses fixed storage between syscalls.
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.description = NetworkPortDescriptionV1::empty();
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_DESCRIBE, capability);
    buffers.request.output_ptr = &mut buffers.description as *mut NetworkPortDescriptionV1 as u64;
    buffers.request.output_len = size_of::<NetworkPortDescriptionV1>() as u64;
    request(buffers)
}

fn send_frame(capability: PackedCapability, frame: [u8; NETWORK_PORT_MIN_FRAME_BYTES]) -> u16 {
    // SAFETY: the single-threaded probe exclusively accesses fixed storage between syscalls.
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.tx = frame;
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_SEND, capability);
    buffers.request.input_ptr = buffers.tx.as_ptr() as u64;
    buffers.request.input_len = buffers.tx.len() as u64;
    request(buffers)
}

fn receive(capability: PackedCapability) -> u16 {
    // SAFETY: the single-threaded probe exclusively accesses fixed storage between syscalls.
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_TRY_RECEIVE, capability);
    buffers.request.output_ptr = buffers.rx.as_mut_ptr() as u64;
    buffers.request.output_len = buffers.rx.len() as u64;
    request(buffers)
}

fn request(buffers: &mut ProbeBuffers) -> u16 {
    buffers.response = NetworkPortResponseV1::new(u16::MAX, 0);
    let result = syscall5(
        SYSCALL_NETWORK_PORT_REQUEST,
        &buffers.request as *const NetworkPortRequestV1 as u64,
        size_of::<NetworkPortRequestV1>() as u64,
        &mut buffers.response as *mut NetworkPortResponseV1 as u64,
        size_of::<NetworkPortResponseV1>() as u64,
        0,
    );
    if result == SYSCALL_OK && valid_response(buffers.response) {
        buffers.response.status
    } else {
        u16::MAX
    }
}

fn valid_response(response: NetworkPortResponseV1) -> bool {
    response.status <= NETWORK_PORT_STATUS_TRANSPORT_ERROR
        && response.state == NETWORK_PORT_STATE_READY
        && response.reserved0 == 0
        && response.reserved1 == 0
        && response.reserved2 == 0
        && response.reserved3 == 0
        && response.reserved4 == 0
}

fn syscall5(number: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
    let result: u64;
    // SAFETY: this is the established x86-64 five-argument user syscall ABI.
    unsafe {
        asm!(
            "syscall",
            inout("rax") number => result,
            inout("rdi") arg1 => _, inout("rsi") arg2 => _, inout("rdx") arg3 => _,
            inout("r10") arg4 => _, inout("r8") arg5 => _, lateout("r9") _,
            lateout("rcx") _, lateout("r11") _, options(nostack),
        );
    }
    result
}

fn write_marker(console: PackedCapability, marker: &str) {
    for byte in marker
        .bytes()
        .chain(core::iter::once(b'\r'))
        .chain(core::iter::once(b'\n'))
    {
        let _ = syscall5(
            SYSCALL_CONSOLE_WRITE_BYTE,
            console.raw(),
            u64::from(byte),
            0,
            0,
            0,
        );
    }
}

fn success_breakpoint() -> ! {
    // SAFETY: PythCore's returnable-user-process path catches this terminal probe breakpoint.
    unsafe { asm!("int3", options(nomem, nostack)) };
    loop {
        core::hint::spin_loop();
    }
}

fn error(console: PackedCapability) -> ! {
    write_marker(console, UDP_ERROR_MARKER);
    // SAFETY: this terminal error path must not return after malformed input or syscall failure.
    unsafe { asm!("ud2", options(noreturn, nomem, nostack)) }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    // SAFETY: panic is terminal for this no-std probe and must not return to user code.
    unsafe { asm!("ud2", options(noreturn, nomem, nostack)) }
}

#[cfg(test)]
fn main() {}

#[cfg(test)]
mod tests {
    use super::{
        MAX_RECEIVE_POLL_ATTEMPTS, arp_reply_frame_matches, arp_request_frame,
        empty_receive_poll_exhausted, received_frame_is_terminal_failure, udp_reply_frame_matches,
        udp_request_frame,
    };
    const LOCAL_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];

    const ARP_REQUEST_FRAME: [u8; 60] = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x52, 0x54, 0x00, 0x12, 0x34, 0x56, 0x08, 0x06, 0x00,
        0x01, 0x08, 0x00, 0x06, 0x04, 0x00, 0x01, 0x52, 0x54, 0x00, 0x12, 0x34, 0x56, 0xc0, 0xa8,
        0x0e, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0xa8, 0x0e, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const ARP_REPLY_FRAME: [u8; 60] = [
        0x52, 0x54, 0x00, 0x12, 0x34, 0x56, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00,
        0x01, 0x08, 0x00, 0x06, 0x04, 0x00, 0x02, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0xc0, 0xa8,
        0x0e, 0x01, 0x52, 0x54, 0x00, 0x12, 0x34, 0x56, 0xc0, 0xa8, 0x0e, 0x02, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const UDP_REQUEST_FRAME: [u8; 60] = [
        0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x52, 0x54, 0x00, 0x12, 0x34, 0x56, 0x08, 0x00, 0x45,
        0x00, 0x00, 0x23, 0x14, 0x05, 0x00, 0x00, 0x40, 0x11, 0xc9, 0x71, 0xc0, 0xa8, 0x0e, 0x02,
        0xc0, 0xa8, 0x0e, 0x01, 0x14, 0x05, 0x14, 0x06, 0x00, 0x0f, 0xf0, 0x8a, 0x50, 0x59, 0x54,
        0x48, 0x55, 0x44, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const UDP_REPLY_FRAME: [u8; 60] = [
        0x52, 0x54, 0x00, 0x12, 0x34, 0x56, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x08, 0x00, 0x45,
        0x00, 0x00, 0x23, 0x14, 0x06, 0x00, 0x00, 0x40, 0x11, 0xc9, 0x70, 0xc0, 0xa8, 0x0e, 0x01,
        0xc0, 0xa8, 0x0e, 0x02, 0x14, 0x06, 0x14, 0x05, 0x00, 0x0f, 0xf0, 0x8a, 0x50, 0x59, 0x54,
        0x48, 0x55, 0x44, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn arp_setup_uses_exact_request_and_reply_frames() {
        assert_eq!(arp_request_frame(LOCAL_MAC), ARP_REQUEST_FRAME);
        assert!(arp_reply_frame_matches(&ARP_REPLY_FRAME, LOCAL_MAC));
    }

    #[test]
    fn udp_request_is_the_exact_sixty_byte_frame() {
        assert_eq!(udp_request_frame(LOCAL_MAC), Some(UDP_REQUEST_FRAME));
    }

    #[test]
    fn udp_reply_accepts_only_the_exact_sixty_byte_frame() {
        assert!(udp_reply_frame_matches(&UDP_REPLY_FRAME, LOCAL_MAC));
        assert!(!udp_reply_frame_matches(&UDP_REPLY_FRAME[..59], LOCAL_MAC));

        let mut extra_frame_byte = [0; 61];
        extra_frame_byte[..60].copy_from_slice(&UDP_REPLY_FRAME);
        assert!(!udp_reply_frame_matches(&extra_frame_byte, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_mac_addresses_and_ether_type() {
        for index in [5, 11, 13] {
            let mut frame = UDP_REPLY_FRAME;
            frame[index] ^= 1;
            assert!(!udp_reply_frame_matches(&frame, LOCAL_MAC));
        }
    }

    #[test]
    fn reply_policy_rejects_wrong_ipv4_version_ihl_tos_length_id_flags_ttl_protocol_checksum_and_addresses()
     {
        let mut wrong_version = UDP_REPLY_FRAME;
        wrong_version[14] = 0x55;
        repair_ipv4_checksum(&mut wrong_version);
        assert!(!udp_reply_frame_matches(&wrong_version, LOCAL_MAC));

        for index in [14, 15, 16, 19, 20, 22, 23, 29, 33] {
            let mut frame = UDP_REPLY_FRAME;
            frame[index] ^= 1;
            repair_ipv4_checksum(&mut frame);
            assert!(!udp_reply_frame_matches(&frame, LOCAL_MAC));
        }

        let mut bad_checksum = UDP_REPLY_FRAME;
        bad_checksum[24] ^= 1;
        assert!(!udp_reply_frame_matches(&bad_checksum, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_udp_ports_length_checksum_data_and_odd_padding() {
        for index in [35, 37, 39, 42, 49] {
            let mut frame = UDP_REPLY_FRAME;
            frame[index] ^= 1;
            if index != 49 {
                repair_udp_checksum(&mut frame);
            }
            assert!(!udp_reply_frame_matches(&frame, LOCAL_MAC));
        }

        let mut bad_checksum = UDP_REPLY_FRAME;
        bad_checksum[40] ^= 1;
        assert!(!udp_reply_frame_matches(&bad_checksum, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_short_and_truncated_frames() {
        for length in [0, 13, 14, 33, 48, 59] {
            assert!(!udp_reply_frame_matches(
                &UDP_REPLY_FRAME[..length],
                LOCAL_MAC
            ));
        }
    }

    #[test]
    fn first_nonmatching_received_frame_is_terminal() {
        let mut frame = UDP_REPLY_FRAME;
        frame[48] = b'Q';
        repair_udp_checksum(&mut frame);

        assert!(received_frame_is_terminal_failure(
            &frame,
            LOCAL_MAC,
            udp_reply_frame_matches,
        ));
    }

    #[test]
    fn empty_receive_responses_exhaust_at_the_fixed_poll_bound() {
        let mut empty_polls = 0;
        for _ in 0..MAX_RECEIVE_POLL_ATTEMPTS - 1 {
            assert!(!empty_receive_poll_exhausted(&mut empty_polls));
        }
        assert!(empty_receive_poll_exhausted(&mut empty_polls));
    }

    fn repair_ipv4_checksum(frame: &mut [u8; 60]) {
        frame[24..26].fill(0);
        let checksum = pythos_user_ipv4_probe::ipv4::ipv4_header_checksum(&frame[14..34]);
        frame[24..26].copy_from_slice(&checksum.to_be_bytes());
    }

    fn repair_udp_checksum(frame: &mut [u8; 60]) {
        frame[40..42].fill(0);
        let checksum = pythos_user_udp_probe::udp::udp_checksum(
            [192, 168, 14, 1],
            [192, 168, 14, 2],
            &frame[34..49],
        );
        frame[40..42].copy_from_slice(&checksum.to_be_bytes());
    }
}
