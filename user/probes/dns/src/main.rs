#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(dead_code, unused_imports))]

#[cfg(not(test))]
use core::panic::PanicInfo;
use core::{arch::asm, cell::UnsafeCell, mem::size_of};
use pythos_shared::{
    capability_abi::PackedCapability,
    dns_markers::{
        DNS_ARP_SETUP_OK_MARKER, DNS_BOOTSTRAPPED_MARKER, DNS_DESCRIBE_OK_MARKER,
        DNS_QUERY_OK_MARKER, DNS_RESPONSE_OK_MARKER,
    },
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
};
use pythos_user_arp_probe::arp::{
    ARP_PAYLOAD_BYTES, ArpPacket, encode as encode_arp, parse as parse_arp,
};
use pythos_user_dns_probe::dns::{
    DNS_QUERY_BYTES, DNS_RESPONSE_BYTES, decode_response, encode_query,
};
use pythos_user_ipv4_probe::ipv4::{Ipv4Header, decode_ipv4_packet, encode_ipv4_packet};
use pythos_user_link_layer_probe::ethernet;
use pythos_user_udp_probe::udp::udp_checksum;

const BROADCAST_MAC: [u8; 6] = [0xff; 6];
const LOCAL_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
const PEER_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x02];
const LOCAL_IPV4: [u8; 4] = [192, 168, 14, 2];
const PEER_IPV4: [u8; 4] = [192, 168, 14, 1];
const ARP_ETHER_TYPE: u16 = 0x0806;
const IPV4_ETHER_TYPE: u16 = 0x0800;
const UDP_PROTOCOL: u8 = 17;
const DNS_SOURCE_PORT: u16 = 0x1605;
const DNS_SERVER_PORT: u16 = 53;
const DNS_QUERY_IPV4_ID: u16 = 0x1601;
const DNS_RESPONSE_IPV4_ID: u16 = 0x1602;
const IPV4_TTL: u8 = 64;
const DNS_QUERY_UDP_BYTES: usize = 8 + DNS_QUERY_BYTES;
const DNS_RESPONSE_UDP_BYTES: usize = 8 + DNS_RESPONSE_BYTES;
const DNS_QUERY_IPV4_BYTES: usize = 20 + DNS_QUERY_UDP_BYTES;
const DNS_RESPONSE_IPV4_BYTES: usize = 20 + DNS_RESPONSE_UDP_BYTES;
const DNS_QUERY_FRAME_BYTES: usize = 14 + DNS_QUERY_IPV4_BYTES;
const DNS_RESPONSE_FRAME_BYTES: usize = 14 + DNS_RESPONSE_IPV4_BYTES;
const MAX_RECEIVE_POLL_ATTEMPTS: usize = 1024;
const DNS_ERROR_MARKER: &str = "PYTHOS:CORE:DNS:ERROR";

struct ProbeStorage(UnsafeCell<ProbeBuffers>);

struct ProbeBuffers {
    request: NetworkPortRequestV1,
    response: NetworkPortResponseV1,
    description: NetworkPortDescriptionV1,
    tx: [u8; NETWORK_PORT_MAX_FRAME_BYTES],
    rx: [u8; NETWORK_PORT_MAX_FRAME_BYTES],
}

// SAFETY: the finite native probe has one thread and accesses this storage serially.
unsafe impl Sync for ProbeStorage {}

static STORAGE: ProbeStorage = ProbeStorage(UnsafeCell::new(ProbeBuffers {
    request: NetworkPortRequestV1::new(0, PackedCapability::from_raw(0)),
    response: NetworkPortResponseV1::new(0, 0),
    description: NetworkPortDescriptionV1::empty(),
    tx: [0; NETWORK_PORT_MAX_FRAME_BYTES],
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
    write_marker(console, DNS_BOOTSTRAPPED_MARKER);

    let capability = bootstrap.port_capability;
    if describe(capability) != NETWORK_PORT_STATUS_OK || !valid_description() {
        error(console);
    }
    write_marker(console, DNS_DESCRIBE_OK_MARKER);

    if send_frame(capability, &arp_request_frame(LOCAL_MAC)) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    receive_matching_frame(
        capability,
        console,
        arp_reply_frame_matches,
        NETWORK_PORT_MIN_FRAME_BYTES,
    );
    write_marker(console, DNS_ARP_SETUP_OK_MARKER);

    let query = match dns_query_frame(LOCAL_MAC) {
        Some(frame) => frame,
        None => error(console),
    };
    if send_frame(capability, &query) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    write_marker(console, DNS_QUERY_OK_MARKER);

    receive_matching_frame(
        capability,
        console,
        dns_response_frame_matches,
        DNS_RESPONSE_FRAME_BYTES,
    );
    write_marker(console, DNS_RESPONSE_OK_MARKER);
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

fn arp_request_frame(local_mac: [u8; 6]) -> [u8; NETWORK_PORT_MIN_FRAME_BYTES] {
    let packet = ArpPacket {
        hardware_type: 1,
        protocol_type: IPV4_ETHER_TYPE,
        hardware_len: 6,
        protocol_len: 4,
        operation: 1,
        sender_hardware: local_mac,
        sender_protocol: LOCAL_IPV4,
        target_hardware: [0; 6],
        target_protocol: PEER_IPV4,
    };
    let mut frame = [0; NETWORK_PORT_MIN_FRAME_BYTES];
    frame[..6].copy_from_slice(&BROADCAST_MAC);
    frame[6..12].copy_from_slice(&local_mac);
    frame[12..14].copy_from_slice(&ARP_ETHER_TYPE.to_be_bytes());
    frame[14..14 + ARP_PAYLOAD_BYTES].copy_from_slice(&encode_arp(packet));
    frame
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
    match parse_arp(&frame.payload[..ARP_PAYLOAD_BYTES]) {
        Ok(packet) => {
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
        Err(_) => false,
    }
}

fn dns_query_bytes() -> Option<[u8; DNS_QUERY_BYTES]> {
    let mut dns = [0; DNS_QUERY_BYTES];
    if encode_query(&mut dns).ok()? != DNS_QUERY_BYTES {
        return None;
    }
    Some(dns)
}

fn dns_query_frame(local_mac: [u8; 6]) -> Option<[u8; DNS_QUERY_FRAME_BYTES]> {
    let dns = dns_query_bytes()?;
    let mut udp = [0; DNS_QUERY_UDP_BYTES];
    udp[..2].copy_from_slice(&DNS_SOURCE_PORT.to_be_bytes());
    udp[2..4].copy_from_slice(&DNS_SERVER_PORT.to_be_bytes());
    udp[4..6].copy_from_slice(&(DNS_QUERY_UDP_BYTES as u16).to_be_bytes());
    udp[8..].copy_from_slice(&dns);
    let checksum = udp_checksum(LOCAL_IPV4, PEER_IPV4, &udp);
    udp[6..8].copy_from_slice(&checksum.to_be_bytes());

    let header = Ipv4Header {
        version: 4,
        ihl: 5,
        dscp_ecn: 0,
        identification: DNS_QUERY_IPV4_ID,
        flags_fragment_offset: 0,
        ttl: IPV4_TTL,
        protocol: UDP_PROTOCOL,
        source: LOCAL_IPV4,
        destination: PEER_IPV4,
    };
    let mut ipv4 = [0; DNS_QUERY_IPV4_BYTES];
    if encode_ipv4_packet(header, &udp, &mut ipv4).ok()? != DNS_QUERY_IPV4_BYTES {
        return None;
    }

    let mut frame = [0; DNS_QUERY_FRAME_BYTES];
    frame[..6].copy_from_slice(&PEER_MAC);
    frame[6..12].copy_from_slice(&local_mac);
    frame[12..14].copy_from_slice(&IPV4_ETHER_TYPE.to_be_bytes());
    frame[14..].copy_from_slice(&ipv4);
    Some(frame)
}

fn dns_response_frame_matches(frame_bytes: &[u8], local_mac: [u8; 6]) -> bool {
    if frame_bytes.len() != DNS_RESPONSE_FRAME_BYTES {
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
    let packet = match decode_ipv4_packet(frame.payload) {
        Ok(packet) => packet,
        Err(_) => return false,
    };
    if packet.total_length as usize != DNS_RESPONSE_IPV4_BYTES
        || packet.header
            != (Ipv4Header {
                version: 4,
                ihl: 5,
                dscp_ecn: 0,
                identification: DNS_RESPONSE_IPV4_ID,
                flags_fragment_offset: 0,
                ttl: IPV4_TTL,
                protocol: UDP_PROTOCOL,
                source: PEER_IPV4,
                destination: LOCAL_IPV4,
            })
        || packet.header_checksum != 0xc74b
    {
        return false;
    }
    let udp = packet.payload;
    if udp.len() != DNS_RESPONSE_UDP_BYTES
        || u16::from_be_bytes([udp[0], udp[1]]) != DNS_SERVER_PORT
        || u16::from_be_bytes([udp[2], udp[3]]) != DNS_SOURCE_PORT
        || u16::from_be_bytes([udp[4], udp[5]]) != DNS_RESPONSE_UDP_BYTES as u16
        || u16::from_be_bytes([udp[6], udp[7]]) != 0x7f11
        || udp_checksum(PEER_IPV4, LOCAL_IPV4, udp) != 0
    {
        return false;
    }
    let query = match dns_query_bytes() {
        Some(query) => query,
        None => return false,
    };
    decode_response(&udp[8..], &query).is_ok()
}

fn empty_receive_poll_exhausted(empty_polls: &mut usize) -> bool {
    *empty_polls = empty_polls.saturating_add(1);
    *empty_polls >= MAX_RECEIVE_POLL_ATTEMPTS
}

fn received_frame_is_terminal_failure(
    frame_bytes: &[u8],
    local_mac: [u8; 6],
    expected_length: usize,
    matches: fn(&[u8], [u8; 6]) -> bool,
) -> bool {
    frame_bytes.len() != expected_length || !matches(frame_bytes, local_mac)
}

fn receive_matching_frame(
    capability: PackedCapability,
    console: PackedCapability,
    matches: fn(&[u8], [u8; 6]) -> bool,
    expected_length: usize,
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
        if received_frame_is_terminal_failure(
            &buffers.rx[..frame_len.min(NETWORK_PORT_MAX_FRAME_BYTES)],
            buffers.description.mac,
            expected_length,
            matches,
        ) {
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

fn send_frame(capability: PackedCapability, frame: &[u8]) -> u16 {
    if frame.is_empty() || frame.len() > NETWORK_PORT_MAX_FRAME_BYTES {
        return u16::MAX;
    }
    // SAFETY: the single-threaded probe exclusively accesses fixed storage between syscalls.
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.tx[..frame.len()].copy_from_slice(frame);
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_SEND, capability);
    buffers.request.input_ptr = buffers.tx.as_ptr() as u64;
    buffers.request.input_len = frame.len() as u64;
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
    write_marker(console, DNS_ERROR_MARKER);
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
        DNS_QUERY_FRAME_BYTES, DNS_QUERY_IPV4_BYTES, DNS_QUERY_UDP_BYTES, DNS_RESPONSE_FRAME_BYTES,
        MAX_RECEIVE_POLL_ATTEMPTS, PEER_MAC, arp_reply_frame_matches, arp_request_frame,
        dns_query_bytes, dns_query_frame, dns_response_frame_matches, empty_receive_poll_exhausted,
        received_frame_is_terminal_failure,
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

    const QUERY_DNS: [u8; 32] = [
        0xd1, 0x4e, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x70, 0x79,
        0x74, 0x68, 0x6f, 0x73, 0x07, 0x65, 0x78, 0x61, 0x6d, 0x70, 0x6c, 0x65, 0x00, 0x00, 0x01,
        0x00, 0x01,
    ];

    const QUERY_FRAME: [u8; DNS_QUERY_FRAME_BYTES] = [
        0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x52, 0x54, 0x00, 0x12, 0x34, 0x56, 0x08, 0x00, 0x45,
        0x00, 0x00, 0x3c, 0x16, 0x01, 0x00, 0x00, 0x40, 0x11, 0xc7, 0x5c, 0xc0, 0xa8, 0x0e, 0x02,
        0xc0, 0xa8, 0x0e, 0x01, 0x16, 0x05, 0x00, 0x35, 0x00, 0x28, 0x82, 0x10, 0xd1, 0x4e, 0x01,
        0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x70, 0x79, 0x74, 0x68, 0x6f,
        0x73, 0x07, 0x65, 0x78, 0x61, 0x6d, 0x70, 0x6c, 0x65, 0x00, 0x00, 0x01, 0x00, 0x01,
    ];

    const RESPONSE_FRAME: [u8; DNS_RESPONSE_FRAME_BYTES] = [
        0x52, 0x54, 0x00, 0x12, 0x34, 0x56, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x08, 0x00, 0x45,
        0x00, 0x00, 0x4c, 0x16, 0x02, 0x00, 0x00, 0x40, 0x11, 0xc7, 0x4b, 0xc0, 0xa8, 0x0e, 0x01,
        0xc0, 0xa8, 0x0e, 0x02, 0x00, 0x35, 0x16, 0x05, 0x00, 0x38, 0x7f, 0x11, 0xd1, 0x4e, 0x81,
        0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x06, 0x70, 0x79, 0x74, 0x68, 0x6f,
        0x73, 0x07, 0x65, 0x78, 0x61, 0x6d, 0x70, 0x6c, 0x65, 0x00, 0x00, 0x01, 0x00, 0x01, 0xc0,
        0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x04, 0xc0, 0x00, 0x02, 0x0e,
    ];

    #[test]
    fn exact_four_frame_policy_has_two_arp_frames_then_query_and_response() {
        assert_eq!(arp_request_frame(LOCAL_MAC), ARP_REQUEST_FRAME);
        assert!(arp_reply_frame_matches(&ARP_REPLY_FRAME, LOCAL_MAC));
        assert_eq!(dns_query_frame(LOCAL_MAC), Some(QUERY_FRAME));
        assert!(dns_response_frame_matches(&RESPONSE_FRAME, LOCAL_MAC));
    }

    #[test]
    fn query_has_exact_dns_bytes_and_network_lengths_and_checksums() {
        assert_eq!(dns_query_bytes(), Some(QUERY_DNS));
        let frame = dns_query_frame(LOCAL_MAC).unwrap();
        assert_eq!(frame.len(), 74);
        assert_eq!(u16::from_be_bytes([frame[24], frame[25]]), 0xc75c);
        assert_eq!(u16::from_be_bytes([frame[40], frame[41]]), 0x8210);
        assert_eq!(u16::from_be_bytes([frame[16], frame[17]]), 60);
        assert_eq!(u16::from_be_bytes([frame[38], frame[39]]), 40);
    }

    #[test]
    fn response_has_exact_dns_bytes_and_network_lengths_and_checksums() {
        assert_eq!(RESPONSE_FRAME.len(), 90);
        assert_eq!(
            u16::from_be_bytes([RESPONSE_FRAME[24], RESPONSE_FRAME[25]]),
            0xc74b
        );
        assert_eq!(
            u16::from_be_bytes([RESPONSE_FRAME[40], RESPONSE_FRAME[41]]),
            0x7f11
        );
        assert_eq!(
            u16::from_be_bytes([RESPONSE_FRAME[16], RESPONSE_FRAME[17]]),
            76
        );
        assert_eq!(
            u16::from_be_bytes([RESPONSE_FRAME[38], RESPONSE_FRAME[39]]),
            56
        );
        assert!(dns_response_frame_matches(&RESPONSE_FRAME, LOCAL_MAC));
    }

    #[test]
    fn response_rejects_wrong_fields_lengths_directions_pointer_and_answer() {
        for index in [
            0, 5, 11, 13, 14, 20, 22, 23, 24, 33, 35, 37, 39, 40, 42, 44, 75, 89,
        ] {
            let mut wrong = RESPONSE_FRAME;
            wrong[index] ^= 1;
            assert!(
                !dns_response_frame_matches(&wrong, LOCAL_MAC),
                "byte {index}"
            );
        }

        for length in [0, 13, 59, 73, 74, 89] {
            assert!(!dns_response_frame_matches(
                &RESPONSE_FRAME[..length],
                LOCAL_MAC
            ));
        }
        let mut overlong = [0; DNS_RESPONSE_FRAME_BYTES + 1];
        overlong[..DNS_RESPONSE_FRAME_BYTES].copy_from_slice(&RESPONSE_FRAME);
        assert!(!dns_response_frame_matches(&overlong, LOCAL_MAC));

        let mut bad_pointer = RESPONSE_FRAME;
        bad_pointer[74] = 0x0d;
        assert!(!dns_response_frame_matches(&bad_pointer, LOCAL_MAC));

        let mut bad_answer = RESPONSE_FRAME;
        bad_answer[89] ^= 1;
        assert!(!dns_response_frame_matches(&bad_answer, LOCAL_MAC));
    }

    #[test]
    fn first_nonmatching_frame_is_terminal_and_no_extra_frame_is_admitted() {
        assert!(received_frame_is_terminal_failure(
            &QUERY_FRAME,
            LOCAL_MAC,
            DNS_RESPONSE_FRAME_BYTES,
            dns_response_frame_matches,
        ));
        assert!(received_frame_is_terminal_failure(
            &RESPONSE_FRAME,
            PEER_MAC,
            DNS_RESPONSE_FRAME_BYTES,
            dns_response_frame_matches,
        ));
        assert_eq!(DNS_QUERY_FRAME_BYTES, 74);
        assert_eq!(DNS_QUERY_UDP_BYTES, 40);
        assert_eq!(DNS_QUERY_IPV4_BYTES, 60);
    }

    #[test]
    fn empty_receive_responses_exhaust_at_the_fixed_poll_bound() {
        let mut empty_polls = 0;
        for _ in 0..MAX_RECEIVE_POLL_ATTEMPTS - 1 {
            assert!(!empty_receive_poll_exhausted(&mut empty_polls));
        }
        assert!(empty_receive_poll_exhausted(&mut empty_polls));
    }
}
