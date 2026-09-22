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
    tcp_markers::{
        TCP_ARP_SETUP_OK_MARKER, TCP_BOOTSTRAPPED_MARKER, TCP_CLOSE_OK_MARKER,
        TCP_DESCRIBE_OK_MARKER, TCP_HANDSHAKE_OK_MARKER, TCP_RX_OK_MARKER, TCP_TX_OK_MARKER,
    },
};
use pythos_user_arp_probe::arp::{ARP_PAYLOAD_BYTES, ArpPacket, encode, parse};
use pythos_user_ipv4_probe::ipv4::{Ipv4Header, decode_ipv4_packet, encode_ipv4_packet};
use pythos_user_link_layer_probe::ethernet;
use pythos_user_tcp_probe::tcp::{
    TCP_FLAG_ACK, TCP_FLAG_FIN_ACK, TCP_FLAG_SYN, TCP_FLAG_SYN_ACK, TCP_HEADER_BYTES, TCP_MSS,
    TCP_PROTOCOL, TCP_SYN_HEADER_BYTES, TcpSegment, decode_segment, encode_segment,
};

const BROADCAST_MAC: [u8; 6] = [0xff; 6];
const LOCAL_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
const PEER_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x02];
const LOCAL_IPV4: [u8; 4] = [192, 168, 14, 2];
const PEER_IPV4: [u8; 4] = [192, 168, 14, 1];
const ARP_ETHER_TYPE: u16 = 0x0806;
const IPV4_ETHER_TYPE: u16 = 0x0800;
const TCP_SOURCE_PORT: u16 = 0x1505;
const TCP_DESTINATION_PORT: u16 = 0x1506;
const TCP_WINDOW: u16 = 0x1000;
const TCP_LOCAL_ISS: u32 = 0x1505_0000;
const TCP_PEER_ISS: u32 = 0x2506_0000;
const TCP_DATA_REQUEST: &[u8; 6] = b"PYTCPQ";
const TCP_DATA_REPLY: &[u8; 6] = b"PYTCPR";
const TCP_IP_IDENTIFICATIONS: [u16; 10] = [
    0x1501, 0x1502, 0x1503, 0x1504, 0x1505, 0x1506, 0x1507, 0x1508, 0x1509, 0x150a,
];
#[cfg(test)]
const TCP_TRANSMIT_INDICES: [usize; 6] = [0, 2, 3, 5, 6, 9];
#[cfg(test)]
const TCP_RECEIVE_INDICES: [usize; 4] = [1, 4, 7, 8];
const MAX_RECEIVE_POLL_ATTEMPTS: usize = 1024;
const TCP_ERROR_MARKER: &str = "PYTHOS:CORE:TCP:ERROR";

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
    write_marker(console, TCP_BOOTSTRAPPED_MARKER);

    let capability = bootstrap.port_capability;
    if describe(capability) != NETWORK_PORT_STATUS_OK || !valid_description() {
        error(console);
    }
    write_marker(console, TCP_DESCRIBE_OK_MARKER);

    if send_frame(capability, arp_request_frame(LOCAL_MAC)) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    receive_matching_frame(capability, console, arp_reply_frame_matches, 0);
    write_marker(console, TCP_ARP_SETUP_OK_MARKER);

    if send_tcp_frame(capability, LOCAL_MAC, 0).is_err() {
        error(console);
    }
    receive_matching_frame(capability, console, tcp_frame_matches, 1);
    if send_tcp_frame(capability, LOCAL_MAC, 2).is_err() {
        error(console);
    }
    write_marker(console, TCP_HANDSHAKE_OK_MARKER);

    if send_tcp_frame(capability, LOCAL_MAC, 3).is_err() {
        error(console);
    }
    write_marker(console, TCP_TX_OK_MARKER);
    receive_matching_frame(capability, console, tcp_frame_matches, 4);
    write_marker(console, TCP_RX_OK_MARKER);

    if send_tcp_frame(capability, LOCAL_MAC, 5).is_err()
        || send_tcp_frame(capability, LOCAL_MAC, 6).is_err()
    {
        error(console);
    }
    receive_matching_frame(capability, console, tcp_frame_matches, 7);
    receive_matching_frame(capability, console, tcp_frame_matches, 8);
    if send_tcp_frame(capability, LOCAL_MAC, 9).is_err() {
        error(console);
    }
    write_marker(console, TCP_CLOSE_OK_MARKER);
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

fn arp_reply_frame_matches(frame_bytes: &[u8], local_mac: [u8; 6], _: usize) -> bool {
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

fn expected_tcp_segment(index: usize) -> Option<TcpSegment<'static>> {
    let local_to_peer = !matches!(index, 1 | 4 | 7 | 8);
    let (source_port, destination_port) = if local_to_peer {
        (TCP_SOURCE_PORT, TCP_DESTINATION_PORT)
    } else {
        (TCP_DESTINATION_PORT, TCP_SOURCE_PORT)
    };
    let (sequence, acknowledgment, flags, mss, data) = match index {
        0 => (
            TCP_LOCAL_ISS,
            0,
            TCP_FLAG_SYN,
            Some(TCP_MSS),
            b"".as_slice(),
        ),
        1 => (
            TCP_PEER_ISS,
            TCP_LOCAL_ISS + 1,
            TCP_FLAG_SYN_ACK,
            Some(TCP_MSS),
            b"".as_slice(),
        ),
        2 => (
            TCP_LOCAL_ISS + 1,
            TCP_PEER_ISS + 1,
            TCP_FLAG_ACK,
            None,
            b"".as_slice(),
        ),
        3 => (
            TCP_LOCAL_ISS + 1,
            TCP_PEER_ISS + 1,
            TCP_FLAG_ACK,
            None,
            TCP_DATA_REQUEST.as_slice(),
        ),
        4 => (
            TCP_PEER_ISS + 1,
            TCP_LOCAL_ISS + 7,
            TCP_FLAG_ACK,
            None,
            TCP_DATA_REPLY.as_slice(),
        ),
        5 => (
            TCP_LOCAL_ISS + 7,
            TCP_PEER_ISS + 7,
            TCP_FLAG_ACK,
            None,
            b"".as_slice(),
        ),
        6 => (
            TCP_LOCAL_ISS + 7,
            TCP_PEER_ISS + 7,
            TCP_FLAG_FIN_ACK,
            None,
            b"".as_slice(),
        ),
        7 => (
            TCP_PEER_ISS + 7,
            TCP_LOCAL_ISS + 8,
            TCP_FLAG_ACK,
            None,
            b"".as_slice(),
        ),
        8 => (
            TCP_PEER_ISS + 7,
            TCP_LOCAL_ISS + 8,
            TCP_FLAG_FIN_ACK,
            None,
            b"".as_slice(),
        ),
        9 => (
            TCP_LOCAL_ISS + 8,
            TCP_PEER_ISS + 8,
            TCP_FLAG_ACK,
            None,
            b"".as_slice(),
        ),
        _ => return None,
    };
    Some(TcpSegment {
        source_port,
        destination_port,
        sequence,
        acknowledgment,
        flags,
        window: TCP_WINDOW,
        urgent_pointer: 0,
        mss,
        data,
    })
}

fn tcp_addresses(index: usize) -> Option<([u8; 4], [u8; 4])> {
    if index < 10 {
        if matches!(index, 1 | 4 | 7 | 8) {
            Some((PEER_IPV4, LOCAL_IPV4))
        } else {
            Some((LOCAL_IPV4, PEER_IPV4))
        }
    } else {
        None
    }
}

fn tcp_frame(local_mac: [u8; 6], index: usize) -> Option<[u8; NETWORK_PORT_MIN_FRAME_BYTES]> {
    let segment = expected_tcp_segment(index)?;
    let (source, destination) = tcp_addresses(index)?;
    let tcp_bytes_len = if segment.mss.is_some() {
        TCP_SYN_HEADER_BYTES
    } else {
        TCP_HEADER_BYTES
    } + segment.data.len();
    let mut tcp_bytes = [0; TCP_SYN_HEADER_BYTES + 6];
    if encode_segment(
        segment,
        source,
        destination,
        &mut tcp_bytes[..tcp_bytes_len],
    )
    .ok()?
        != tcp_bytes_len
    {
        return None;
    }

    let ipv4_bytes_len = 20 + tcp_bytes_len;
    let mut ipv4_bytes = [0; 20 + TCP_SYN_HEADER_BYTES + 6];
    let header = Ipv4Header {
        version: 4,
        ihl: 5,
        dscp_ecn: 0,
        identification: TCP_IP_IDENTIFICATIONS[index],
        flags_fragment_offset: 0,
        ttl: 64,
        protocol: TCP_PROTOCOL,
        source,
        destination,
    };
    if encode_ipv4_packet(
        header,
        &tcp_bytes[..tcp_bytes_len],
        &mut ipv4_bytes[..ipv4_bytes_len],
    )
    .ok()?
        != ipv4_bytes_len
    {
        return None;
    }
    let ethernet_destination = if source == LOCAL_IPV4 {
        PEER_MAC
    } else {
        local_mac
    };
    let ethernet_source = if source == LOCAL_IPV4 {
        local_mac
    } else {
        PEER_MAC
    };
    ethernet::encode_minimum_frame(
        ethernet_destination,
        ethernet_source,
        IPV4_ETHER_TYPE,
        &ipv4_bytes[..ipv4_bytes_len],
    )
    .ok()
}

fn tcp_frame_matches(frame_bytes: &[u8], local_mac: [u8; 6], index: usize) -> bool {
    let Some(expected_segment) = expected_tcp_segment(index) else {
        return false;
    };
    let Some((source, destination)) = tcp_addresses(index) else {
        return false;
    };
    if frame_bytes.len() != NETWORK_PORT_MIN_FRAME_BYTES {
        return false;
    }
    let frame = match ethernet::parse(frame_bytes) {
        Ok(frame) => frame,
        Err(_) => return false,
    };
    let expected_destination = if source == LOCAL_IPV4 {
        PEER_MAC
    } else {
        local_mac
    };
    let expected_source = if source == LOCAL_IPV4 {
        local_mac
    } else {
        PEER_MAC
    };
    if frame.destination != expected_destination
        || frame.source != expected_source
        || frame.ether_type != IPV4_ETHER_TYPE
    {
        return false;
    }
    let packet = match decode_ipv4_packet(frame.payload) {
        Ok(packet) => packet,
        Err(_) => return false,
    };
    let segment_bytes = if expected_segment.mss.is_some() {
        TCP_SYN_HEADER_BYTES
    } else {
        TCP_HEADER_BYTES
    } + expected_segment.data.len();
    let ipv4_bytes_len = 20 + segment_bytes;
    if packet.total_length as usize != ipv4_bytes_len
        || packet.header
            != (Ipv4Header {
                version: 4,
                ihl: 5,
                dscp_ecn: 0,
                identification: TCP_IP_IDENTIFICATIONS[index],
                flags_fragment_offset: 0,
                ttl: 64,
                protocol: TCP_PROTOCOL,
                source,
                destination,
            })
        || !frame.payload[ipv4_bytes_len..]
            .iter()
            .all(|byte| *byte == 0)
    {
        return false;
    }
    match decode_segment(packet.payload, source, destination) {
        Ok(segment) => segment == expected_segment,
        Err(_) => false,
    }
}

fn send_tcp_frame(
    capability: PackedCapability,
    local_mac: [u8; 6],
    index: usize,
) -> Result<(), ()> {
    let frame = tcp_frame(local_mac, index).ok_or(())?;
    if send_frame(capability, frame) == NETWORK_PORT_STATUS_OK {
        Ok(())
    } else {
        Err(())
    }
}

fn empty_receive_poll_exhausted(empty_polls: &mut usize) -> bool {
    *empty_polls = empty_polls.saturating_add(1);
    *empty_polls >= MAX_RECEIVE_POLL_ATTEMPTS
}

fn receive_matching_frame(
    capability: PackedCapability,
    console: PackedCapability,
    matches: fn(&[u8], [u8; 6], usize) -> bool,
    expected_index: usize,
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
            || !matches(
                &buffers.rx[..frame_len],
                buffers.description.mac,
                expected_index,
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
    write_marker(console, TCP_ERROR_MARKER);
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
mod tests {
    use super::{
        TCP_RECEIVE_INDICES, TCP_TRANSMIT_INDICES, arp_reply_frame_matches, arp_request_frame,
        empty_receive_poll_exhausted, tcp_frame, tcp_frame_matches,
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

    #[test]
    fn arp_setup_uses_the_exact_minimum_request_and_reply_frames() {
        let request = arp_request_frame(LOCAL_MAC);
        assert_eq!(request, ARP_REQUEST_FRAME);
        assert!(arp_reply_frame_matches(&ARP_REPLY_FRAME, LOCAL_MAC, 0));
    }

    #[test]
    fn all_ten_tcp_frames_have_the_exact_finite_exchange_policy() {
        for index in 0..10 {
            let frame = tcp_frame(LOCAL_MAC, index).expect("canonical TCP frame");
            assert_eq!(frame.len(), 60);
            assert!(tcp_frame_matches(&frame, LOCAL_MAC, index), "index {index}");
        }
    }

    #[test]
    fn transmit_and_receive_sequences_are_exact_and_have_no_extra_frames() {
        assert_eq!(TCP_TRANSMIT_INDICES, [0, 2, 3, 5, 6, 9]);
        assert_eq!(TCP_RECEIVE_INDICES, [1, 4, 7, 8]);
        assert_eq!(TCP_TRANSMIT_INDICES.len() + TCP_RECEIVE_INDICES.len(), 10);
    }

    #[test]
    fn tcp_policy_rejects_wrong_fields_options_checksums_padding_and_lengths() {
        let canonical = tcp_frame(LOCAL_MAC, 1).unwrap();
        for index in [0, 5, 11, 13, 14, 15, 16, 24, 34, 35, 47, 59] {
            let mut wrong = canonical;
            wrong[index] ^= 1;
            assert!(!tcp_frame_matches(&wrong, LOCAL_MAC, 1));
        }

        assert!(!tcp_frame_matches(&canonical[..59], LOCAL_MAC, 1));
        let mut overlong = [0; 61];
        overlong[..60].copy_from_slice(&canonical);
        assert!(!tcp_frame_matches(&overlong, LOCAL_MAC, 1));
    }

    #[test]
    fn tcp_policy_rejects_rst_duplicate_and_reordered_receives() {
        let syn_ack = tcp_frame(LOCAL_MAC, 1).unwrap();
        let data_reply = tcp_frame(LOCAL_MAC, 4).unwrap();
        let mut rst = syn_ack;
        rst[14 + 20 + 13] = 0x14 | 0x04;
        assert!(!tcp_frame_matches(&rst, LOCAL_MAC, 1));
        assert!(!tcp_frame_matches(&syn_ack, LOCAL_MAC, 4));
        assert!(!tcp_frame_matches(&data_reply, LOCAL_MAC, 1));
    }

    #[test]
    fn bounded_empty_polling_exhausts_at_the_finite_limit() {
        let mut empty_polls = 0;
        for _ in 0..1023 {
            assert!(!empty_receive_poll_exhausted(&mut empty_polls));
        }
        assert!(empty_receive_poll_exhausted(&mut empty_polls));
    }
}
