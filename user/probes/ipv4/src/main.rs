#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(dead_code, unused_imports))]

#[cfg(not(test))]
use core::panic::PanicInfo;
use core::{arch::asm, cell::UnsafeCell, mem::size_of};
use pythos_shared::{
    capability_abi::PackedCapability,
    ipv4_markers::{
        IPV4_ARP_SETUP_OK_MARKER, IPV4_BOOTSTRAPPED_MARKER, IPV4_DESCRIBE_OK_MARKER,
        IPV4_RX_OK_MARKER, IPV4_TX_OK_MARKER,
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
use pythos_user_ipv4_probe::ipv4::{Ipv4Header, decode_ipv4_packet, encode_ipv4_packet};
use pythos_user_link_layer_probe::ethernet;

const BROADCAST_MAC: [u8; 6] = [0xff; 6];
const PEER_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x02];
const LOCAL_IPV4: [u8; 4] = [192, 168, 14, 2];
const PEER_IPV4: [u8; 4] = [192, 168, 14, 1];
const ARP_ETHER_TYPE: u16 = 0x0806;
const IPV4_ETHER_TYPE: u16 = 0x0800;
const IPV4_PROTOCOL: u8 = 253;
const IPV4_REQUEST_ID: u16 = 0x1401;
const IPV4_REPLY_ID: u16 = 0x1402;
const IPV4_TTL: u8 = 64;
const IPV4_DATAGRAM_BYTES: usize = 28;
const IPV4_REQUEST_PAYLOAD: &[u8; 8] = b"PYTHIPRQ";
const IPV4_REPLY_PAYLOAD: &[u8; 8] = b"PYTHIPRP";
const MAX_RECEIVE_POLL_ATTEMPTS: usize = 1024;
const IPV4_ERROR_MARKER: &str = "PYTHOS:CORE:IPV4:ERROR";

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
    write_marker(console, IPV4_BOOTSTRAPPED_MARKER);

    let capability = bootstrap.port_capability;
    if describe(capability) != NETWORK_PORT_STATUS_OK || !valid_description() {
        error(console);
    }
    write_marker(console, IPV4_DESCRIBE_OK_MARKER);

    let local_mac = current_local_mac();
    if send_frame(capability, arp_request_frame(local_mac)) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    receive_matching_frame(capability, console, arp_reply_frame_matches);
    write_marker(console, IPV4_ARP_SETUP_OK_MARKER);

    let ipv4_frame = match ipv4_request_frame(local_mac) {
        Some(frame) => frame,
        None => error(console),
    };
    if send_frame(capability, ipv4_frame) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    write_marker(console, IPV4_TX_OK_MARKER);

    receive_matching_frame(capability, console, ipv4_reply_frame_matches);
    write_marker(console, IPV4_RX_OK_MARKER);
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

fn ipv4_request_frame(local_mac: [u8; 6]) -> Option<[u8; NETWORK_PORT_MIN_FRAME_BYTES]> {
    let header = Ipv4Header {
        version: 4,
        ihl: 5,
        dscp_ecn: 0,
        identification: IPV4_REQUEST_ID,
        flags_fragment_offset: 0,
        ttl: IPV4_TTL,
        protocol: IPV4_PROTOCOL,
        source: LOCAL_IPV4,
        destination: PEER_IPV4,
    };
    let mut datagram = [0; IPV4_DATAGRAM_BYTES];
    let encoded = encode_ipv4_packet(header, IPV4_REQUEST_PAYLOAD, &mut datagram).ok()?;
    if encoded != IPV4_DATAGRAM_BYTES {
        return None;
    }
    ethernet::encode_minimum_frame(PEER_MAC, local_mac, IPV4_ETHER_TYPE, &datagram).ok()
}

fn ipv4_reply_frame_matches(frame_bytes: &[u8], local_mac: [u8; 6]) -> bool {
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
    packet.header.version == 4
        && packet.header.ihl == 5
        && packet.header.dscp_ecn == 0
        && packet.total_length as usize == IPV4_DATAGRAM_BYTES
        && packet.header.identification == IPV4_REPLY_ID
        && packet.header.flags_fragment_offset == 0
        && packet.header.ttl == IPV4_TTL
        && packet.header.protocol == IPV4_PROTOCOL
        && packet.header.source == PEER_IPV4
        && packet.header.destination == LOCAL_IPV4
        && packet.header_checksum == 0xc88f
        && packet.payload == IPV4_REPLY_PAYLOAD
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
    write_marker(console, IPV4_ERROR_MARKER);
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
        IPV4_ERROR_MARKER, MAX_RECEIVE_POLL_ATTEMPTS, arp_reply_frame_matches, arp_request_frame,
        empty_receive_poll_exhausted, ipv4_reply_frame_matches, ipv4_request_frame,
    };
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

    const IPV4_REQUEST_FRAME: [u8; 60] = [
        0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x45,
        0x00, 0x00, 0x1c, 0x14, 0x01, 0x00, 0x00, 0x40, 0xfd, 0xc8, 0x90, 0xc0, 0xa8, 0x0e, 0x02,
        0xc0, 0xa8, 0x0e, 0x01, 0x50, 0x59, 0x54, 0x48, 0x49, 0x50, 0x52, 0x51, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const IPV4_REPLY_FRAME: [u8; 60] = [
        0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x08, 0x00, 0x45,
        0x00, 0x00, 0x1c, 0x14, 0x02, 0x00, 0x00, 0x40, 0xfd, 0xc8, 0x8f, 0xc0, 0xa8, 0x0e, 0x01,
        0xc0, 0xa8, 0x0e, 0x02, 0x50, 0x59, 0x54, 0x48, 0x49, 0x50, 0x52, 0x50, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    fn repair_ipv4_checksum(frame: &mut [u8; 60]) {
        frame[24..26].fill(0);
        let checksum = ipv4_header_checksum(&frame[14..34]);
        frame[24..26].copy_from_slice(&checksum.to_be_bytes());
    }

    #[test]
    fn empty_receive_responses_exhaust_at_the_bound_and_use_terminal_error_marker() {
        assert_eq!(IPV4_ERROR_MARKER, "PYTHOS:CORE:IPV4:ERROR");

        let mut empty_polls = 0;
        for _ in 0..MAX_RECEIVE_POLL_ATTEMPTS - 1 {
            assert!(!empty_receive_poll_exhausted(&mut empty_polls));
        }
        assert_eq!(empty_polls, MAX_RECEIVE_POLL_ATTEMPTS - 1);
        assert!(empty_receive_poll_exhausted(&mut empty_polls));
        assert_eq!(empty_polls, MAX_RECEIVE_POLL_ATTEMPTS);
    }

    #[test]
    fn arp_request_uses_the_exact_private_address_relationship() {
        assert_eq!(arp_request_frame(LOCAL_MAC), ARP_REQUEST_FRAME);
    }

    #[test]
    fn arp_reply_accepts_only_the_exact_private_address_relationship() {
        assert!(arp_reply_frame_matches(&ARP_REPLY_FRAME, LOCAL_MAC));

        let mut wrong_sender_ip = ARP_REPLY_FRAME;
        wrong_sender_ip[31] = 0x02;
        assert!(!arp_reply_frame_matches(&wrong_sender_ip, LOCAL_MAC));

        let mut wrong_target_ip = ARP_REPLY_FRAME;
        wrong_target_ip[41] = 0x01;
        assert!(!arp_reply_frame_matches(&wrong_target_ip, LOCAL_MAC));
    }

    #[test]
    fn arp_reply_rejects_wrong_link_or_packet_relationships() {
        let mut wrong_destination = ARP_REPLY_FRAME;
        wrong_destination[5] ^= 1;
        assert!(!arp_reply_frame_matches(&wrong_destination, LOCAL_MAC));

        let mut wrong_source = ARP_REPLY_FRAME;
        wrong_source[11] ^= 1;
        assert!(!arp_reply_frame_matches(&wrong_source, LOCAL_MAC));

        let mut wrong_operation = ARP_REPLY_FRAME;
        wrong_operation[21] = 1;
        assert!(!arp_reply_frame_matches(&wrong_operation, LOCAL_MAC));

        let mut wrong_target_hardware = ARP_REPLY_FRAME;
        wrong_target_hardware[37] ^= 1;
        assert!(!arp_reply_frame_matches(&wrong_target_hardware, LOCAL_MAC));

        let mut nonzero_padding = ARP_REPLY_FRAME;
        nonzero_padding[42] = 1;
        assert!(!arp_reply_frame_matches(&nonzero_padding, LOCAL_MAC));
    }

    #[test]
    fn ipv4_request_is_the_exact_sixty_byte_frame() {
        assert_eq!(ipv4_request_frame(LOCAL_MAC), Some(IPV4_REQUEST_FRAME));
    }

    #[test]
    fn ipv4_reply_accepts_the_exact_sixty_byte_frame() {
        assert!(ipv4_reply_frame_matches(&IPV4_REPLY_FRAME, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_nonzero_ethernet_padding() {
        let mut frame = IPV4_REPLY_FRAME;
        frame[42] = 1;
        assert!(!ipv4_reply_frame_matches(&frame, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_mac_addresses() {
        let mut wrong_destination = IPV4_REPLY_FRAME;
        wrong_destination[5] ^= 1;
        assert!(!ipv4_reply_frame_matches(&wrong_destination, LOCAL_MAC));

        let mut wrong_source = IPV4_REPLY_FRAME;
        wrong_source[11] ^= 1;
        assert!(!ipv4_reply_frame_matches(&wrong_source, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_ether_type() {
        let mut frame = IPV4_REPLY_FRAME;
        frame[13] = 0x06;
        assert!(!ipv4_reply_frame_matches(&frame, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_ipv4_addresses() {
        let mut wrong_source = IPV4_REPLY_FRAME;
        wrong_source[29] ^= 1;
        repair_ipv4_checksum(&mut wrong_source);
        assert!(!ipv4_reply_frame_matches(&wrong_source, LOCAL_MAC));

        let mut wrong_destination = IPV4_REPLY_FRAME;
        wrong_destination[33] ^= 1;
        repair_ipv4_checksum(&mut wrong_destination);
        assert!(!ipv4_reply_frame_matches(&wrong_destination, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_protocol() {
        let mut frame = IPV4_REPLY_FRAME;
        frame[23] = 17;
        repair_ipv4_checksum(&mut frame);
        assert!(!ipv4_reply_frame_matches(&frame, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_nonzero_fragment_fields() {
        let mut frame = IPV4_REPLY_FRAME;
        frame[21] = 1;
        repair_ipv4_checksum(&mut frame);
        assert!(!ipv4_reply_frame_matches(&frame, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_identification() {
        let mut frame = IPV4_REPLY_FRAME;
        frame[19] = 1;
        repair_ipv4_checksum(&mut frame);
        assert!(!ipv4_reply_frame_matches(&frame, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_payload() {
        let mut frame = IPV4_REPLY_FRAME;
        frame[41] = b'Q';
        assert!(!ipv4_reply_frame_matches(&frame, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_bad_header_checksum() {
        let mut frame = IPV4_REPLY_FRAME;
        frame[24] ^= 1;
        assert!(!ipv4_reply_frame_matches(&frame, LOCAL_MAC));
    }
}
