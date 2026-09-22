#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(dead_code, unused_imports))]

#[cfg(not(test))]
use core::panic::PanicInfo;
use core::{arch::asm, cell::UnsafeCell, mem::size_of};
use pythos_shared::{
    capability_abi::PackedCapability,
    icmp_markers::{
        ICMP_ARP_SETUP_OK_MARKER, ICMP_BOOTSTRAPPED_MARKER, ICMP_DESCRIBE_OK_MARKER,
        ICMP_RX_OK_MARKER, ICMP_TX_OK_MARKER,
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
use pythos_user_arp_probe::arp::{ARP_PAYLOAD_BYTES, ArpPacket, encode, parse};
use pythos_user_icmp_probe::icmp::{ICMP_ECHO_BYTES, IcmpEcho, decode_echo, encode_echo};
use pythos_user_ipv4_probe::ipv4::{Ipv4Header, decode_ipv4_packet, encode_ipv4_packet};
use pythos_user_link_layer_probe::ethernet;

const BROADCAST_MAC: [u8; 6] = [0xff; 6];
const PEER_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x02];
const LOCAL_IPV4: [u8; 4] = [192, 168, 14, 2];
const PEER_IPV4: [u8; 4] = [192, 168, 14, 1];
const ARP_ETHER_TYPE: u16 = 0x0806;
const IPV4_ETHER_TYPE: u16 = 0x0800;
const ICMP_PROTOCOL: u8 = 1;
const ICMP_REQUEST_TYPE: u8 = 8;
const ICMP_REPLY_TYPE: u8 = 0;
const ICMP_CODE: u8 = 0;
const ICMP_IDENTIFIER: u16 = 0x1403;
const ICMP_SEQUENCE: u16 = 0x0001;
const ICMP_DATA: &[u8; 8] = b"PYTHICMP";
const IPV4_REQUEST_ID: u16 = 0x1403;
const IPV4_REPLY_ID: u16 = 0x1404;
const IPV4_TTL: u8 = 64;
const IPV4_DATAGRAM_BYTES: usize = 36;
const MAX_RECEIVE_POLL_ATTEMPTS: usize = 1024;
const ICMP_ERROR_MARKER: &str = "PYTHOS:CORE:ICMP:ERROR";

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
    write_marker(console, ICMP_BOOTSTRAPPED_MARKER);

    let capability = bootstrap.port_capability;
    if describe(capability) != NETWORK_PORT_STATUS_OK || !valid_description() {
        error(console);
    }
    write_marker(console, ICMP_DESCRIBE_OK_MARKER);

    let local_mac = current_local_mac();
    if send_frame(capability, arp_request_frame(local_mac)) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    receive_matching_frame(capability, console, arp_reply_frame_matches);
    write_marker(console, ICMP_ARP_SETUP_OK_MARKER);

    let icmp_frame = match icmp_request_frame(local_mac) {
        Some(frame) => frame,
        None => error(console),
    };
    if send_frame(capability, icmp_frame) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    write_marker(console, ICMP_TX_OK_MARKER);

    receive_matching_frame(capability, console, icmp_reply_frame_matches);
    write_marker(console, ICMP_RX_OK_MARKER);
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
    description.reserved0 == [0; 2]
        && description.reserved1 == 0
        && description.min_frame_bytes as usize == NETWORK_PORT_MIN_FRAME_BYTES
        && description.max_frame_bytes as usize == NETWORK_PORT_MAX_FRAME_BYTES
        && description.transport_flags == NETWORK_PORT_FLAG_MAC_ONLY | NETWORK_PORT_FLAG_NO_OFFLOAD
        && description.state == u32::from(NETWORK_PORT_STATE_READY)
}

fn current_local_mac() -> [u8; 6] {
    // SAFETY: description is initialized before this single-threaded read.
    unsafe { &*STORAGE.0.get() }.description.mac
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
    frame[0..6].copy_from_slice(&BROADCAST_MAC);
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

fn icmp_request_frame(local_mac: [u8; 6]) -> Option<[u8; NETWORK_PORT_MIN_FRAME_BYTES]> {
    let echo = IcmpEcho {
        icmp_type: ICMP_REQUEST_TYPE,
        code: ICMP_CODE,
        identifier: ICMP_IDENTIFIER,
        sequence: ICMP_SEQUENCE,
        data: ICMP_DATA,
    };
    let mut icmp_message = [0; ICMP_ECHO_BYTES];
    if encode_echo(echo, &mut icmp_message).ok()? != ICMP_ECHO_BYTES {
        return None;
    }

    let header = Ipv4Header {
        version: 4,
        ihl: 5,
        dscp_ecn: 0,
        identification: IPV4_REQUEST_ID,
        flags_fragment_offset: 0,
        ttl: IPV4_TTL,
        protocol: ICMP_PROTOCOL,
        source: LOCAL_IPV4,
        destination: PEER_IPV4,
    };
    let mut datagram = [0; IPV4_DATAGRAM_BYTES];
    if encode_ipv4_packet(header, &icmp_message, &mut datagram).ok()? != IPV4_DATAGRAM_BYTES {
        return None;
    }
    ethernet::encode_minimum_frame(PEER_MAC, local_mac, IPV4_ETHER_TYPE, &datagram).ok()
}

fn icmp_reply_frame_matches(frame_bytes: &[u8], local_mac: [u8; 6]) -> bool {
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
        || packet.header.protocol != ICMP_PROTOCOL
        || packet.header.source != PEER_IPV4
        || packet.header.destination != LOCAL_IPV4
        || packet.header_checksum != 0xc981
    {
        return false;
    }
    match decode_echo(packet.payload) {
        Ok(echo) => {
            echo.icmp_type == ICMP_REPLY_TYPE
                && echo.code == ICMP_CODE
                && echo.identifier == ICMP_IDENTIFIER
                && echo.sequence == ICMP_SEQUENCE
                && echo.data == ICMP_DATA
        }
        Err(_) => false,
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

fn empty_receive_poll_exhausted(empty_polls: &mut usize) -> bool {
    *empty_polls = empty_polls.saturating_add(1);
    *empty_polls >= MAX_RECEIVE_POLL_ATTEMPTS
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
            || !matches(&buffers.rx[..frame_len], buffers.description.mac)
        {
            error(console);
        }
        return;
    }
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
    write_marker(console, ICMP_ERROR_MARKER);
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
        ICMP_ERROR_MARKER, MAX_RECEIVE_POLL_ATTEMPTS, arp_reply_frame_matches, arp_request_frame,
        empty_receive_poll_exhausted, icmp_reply_frame_matches, icmp_request_frame,
    };
    use pythos_user_icmp_probe::icmp::icmp_checksum;
    use pythos_user_ipv4_probe::ipv4::ipv4_header_checksum;

    const LOCAL_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];

    const ARP_REQUEST_FRAME: [u8; 60] = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00,
        0x01, 0x08, 0x00, 0x06, 0x04, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0xc0, 0xa8,
        0x0e, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0xa8, 0x0e, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const ARP_REPLY_FRAME: [u8; 60] = [
        0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00,
        0x01, 0x08, 0x00, 0x06, 0x04, 0x00, 0x02, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0xc0, 0xa8,
        0x0e, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0xc0, 0xa8, 0x0e, 0x02, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const ICMP_REQUEST_FRAME: [u8; 60] = [
        0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x45,
        0x00, 0x00, 0x24, 0x14, 0x03, 0x00, 0x00, 0x40, 0x01, 0xc9, 0x82, 0xc0, 0xa8, 0x0e, 0x02,
        0xc0, 0xa8, 0x0e, 0x01, 0x08, 0x00, 0xa8, 0xc6, 0x14, 0x03, 0x00, 0x01, 0x50, 0x59, 0x54,
        0x48, 0x49, 0x43, 0x4d, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const ICMP_REPLY_FRAME: [u8; 60] = [
        0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x08, 0x00, 0x45,
        0x00, 0x00, 0x24, 0x14, 0x04, 0x00, 0x00, 0x40, 0x01, 0xc9, 0x81, 0xc0, 0xa8, 0x0e, 0x01,
        0xc0, 0xa8, 0x0e, 0x02, 0x00, 0x00, 0xb0, 0xc6, 0x14, 0x03, 0x00, 0x01, 0x50, 0x59, 0x54,
        0x48, 0x49, 0x43, 0x4d, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    fn repair_ipv4_checksum(frame: &mut [u8; 60]) {
        frame[24..26].fill(0);
        let checksum = ipv4_header_checksum(&frame[14..34]);
        frame[24..26].copy_from_slice(&checksum.to_be_bytes());
    }

    fn repair_icmp_checksum(frame: &mut [u8; 60]) {
        frame[36..38].fill(0);
        let checksum = icmp_checksum(&frame[34..50]);
        frame[36..38].copy_from_slice(&checksum.to_be_bytes());
    }

    #[test]
    fn empty_receive_responses_exhaust_at_the_bound_and_use_terminal_error_marker() {
        assert_eq!(ICMP_ERROR_MARKER, "PYTHOS:CORE:ICMP:ERROR");

        let mut empty_polls = 0;
        for _ in 0..MAX_RECEIVE_POLL_ATTEMPTS - 1 {
            assert!(!empty_receive_poll_exhausted(&mut empty_polls));
        }
        assert_eq!(empty_polls, MAX_RECEIVE_POLL_ATTEMPTS - 1);
        assert!(empty_receive_poll_exhausted(&mut empty_polls));
        assert_eq!(empty_polls, MAX_RECEIVE_POLL_ATTEMPTS);
    }

    #[test]
    fn arp_setup_uses_the_exact_private_address_relationship() {
        assert_eq!(arp_request_frame(LOCAL_MAC), ARP_REQUEST_FRAME);
        assert!(arp_reply_frame_matches(&ARP_REPLY_FRAME, LOCAL_MAC));

        let mut wrong_sender_ip = ARP_REPLY_FRAME;
        wrong_sender_ip[31] ^= 1;
        assert!(!arp_reply_frame_matches(&wrong_sender_ip, LOCAL_MAC));
    }

    #[test]
    fn icmp_request_is_the_exact_sixty_byte_frame() {
        assert_eq!(icmp_request_frame(LOCAL_MAC), Some(ICMP_REQUEST_FRAME));
    }

    #[test]
    fn icmp_reply_accepts_only_the_exact_sixty_byte_frame() {
        assert!(icmp_reply_frame_matches(&ICMP_REPLY_FRAME, LOCAL_MAC));
        assert!(!icmp_reply_frame_matches(
            &ICMP_REPLY_FRAME[..59],
            LOCAL_MAC
        ));

        let mut extra_frame_byte = [0; 61];
        extra_frame_byte[..60].copy_from_slice(&ICMP_REPLY_FRAME);
        assert!(!icmp_reply_frame_matches(&extra_frame_byte, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_mac_addresses_and_ether_type() {
        let mut wrong_destination = ICMP_REPLY_FRAME;
        wrong_destination[5] ^= 1;
        assert!(!icmp_reply_frame_matches(&wrong_destination, LOCAL_MAC));

        let mut wrong_source = ICMP_REPLY_FRAME;
        wrong_source[11] ^= 1;
        assert!(!icmp_reply_frame_matches(&wrong_source, LOCAL_MAC));

        let mut wrong_ether_type = ICMP_REPLY_FRAME;
        wrong_ether_type[13] = 0x06;
        assert!(!icmp_reply_frame_matches(&wrong_ether_type, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_ipv4_fields() {
        for index in [14, 15, 19, 22, 23, 29, 33] {
            let mut frame = ICMP_REPLY_FRAME;
            frame[index] ^= 1;
            repair_ipv4_checksum(&mut frame);
            assert!(!icmp_reply_frame_matches(&frame, LOCAL_MAC));
        }

        let mut wrong_total_length = ICMP_REPLY_FRAME;
        wrong_total_length[17] = 0x23;
        repair_ipv4_checksum(&mut wrong_total_length);
        assert!(!icmp_reply_frame_matches(&wrong_total_length, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_fragmentation() {
        let mut frame = ICMP_REPLY_FRAME;
        frame[21] = 1;
        repair_ipv4_checksum(&mut frame);
        assert!(!icmp_reply_frame_matches(&frame, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_icmp_type_or_code() {
        for index in [34, 35] {
            let mut frame = ICMP_REPLY_FRAME;
            frame[index] ^= 1;
            repair_icmp_checksum(&mut frame);
            assert!(!icmp_reply_frame_matches(&frame, LOCAL_MAC));
        }
    }

    #[test]
    fn reply_policy_rejects_wrong_icmp_identity_sequence_or_data() {
        for index in [39, 41, 49] {
            let mut frame = ICMP_REPLY_FRAME;
            frame[index] ^= 1;
            repair_icmp_checksum(&mut frame);
            assert!(!icmp_reply_frame_matches(&frame, LOCAL_MAC));
        }
    }

    #[test]
    fn reply_policy_rejects_bad_ipv4_or_icmp_checksum() {
        let mut bad_ipv4_checksum = ICMP_REPLY_FRAME;
        bad_ipv4_checksum[24] ^= 1;
        assert!(!icmp_reply_frame_matches(&bad_ipv4_checksum, LOCAL_MAC));

        let mut bad_icmp_checksum = ICMP_REPLY_FRAME;
        bad_icmp_checksum[36] ^= 1;
        assert!(!icmp_reply_frame_matches(&bad_icmp_checksum, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_nonzero_ethernet_padding() {
        for index in 50..60 {
            let mut frame = ICMP_REPLY_FRAME;
            frame[index] = 1;
            assert!(!icmp_reply_frame_matches(&frame, LOCAL_MAC));
        }
    }
}
