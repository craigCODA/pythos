#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(dead_code, unused_imports))]
#![cfg_attr(feature = "secure-transport", allow(dead_code, unused_imports))]

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
    socket_markers::{
        SOCKET_BOOTSTRAPPED_MARKER, SOCKET_CLOSE_OK_MARKER, SOCKET_DENIED_BOOTSTRAPPED_MARKER,
        SOCKET_HANDSHAKE_OK_MARKER, SOCKET_OPEN_GRANTED_MARKER,
        SOCKET_OPEN_WITHOUT_CAP_DENIED_MARKER, SOCKET_REQUEST_OK_MARKER, SOCKET_RESPONSE_OK_MARKER,
    },
};
use pythos_user_arp_probe::arp::{
    ARP_PAYLOAD_BYTES, ArpPacket, encode as encode_arp, parse as parse_arp,
};
use pythos_user_ipv4_probe::ipv4::{Ipv4Header, decode_ipv4_packet, encode_ipv4_packet};
use pythos_user_link_layer_probe::ethernet;
use pythos_user_socket_probe::service::{
    ACCEPTED_ENDPOINT, CapabilityAuthority, REQUEST_PAYLOAD, RESPONSE_PAYLOAD,
    SOCKET_PAYLOAD_BYTES, SocketService,
};
use pythos_user_tcp_probe::tcp::{
    TCP_FLAG_ACK, TCP_FLAG_FIN_ACK, TCP_FLAG_SYN, TCP_FLAG_SYN_ACK, TCP_HEADER_BYTES, TCP_MSS,
    TCP_PROTOCOL, TCP_SYN_HEADER_BYTES, TcpSegment, decode_segment, encode_segment,
};

#[cfg(feature = "secure-transport")]
mod secure;

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
const TCP_IP_IDENTIFICATIONS: [u16; 10] = [
    0x1501, 0x1502, 0x1503, 0x1504, 0x1505, 0x1506, 0x1507, 0x1508, 0x1509, 0x150a,
];
const MAX_RECEIVE_POLL_ATTEMPTS: usize = 1024;
const SOCKET_ERROR_MARKER: &str = "PYTHOS:CORE:SOCKET:ERROR";

struct ProbeStorage(UnsafeCell<ProbeBuffers>);
struct ProbeBuffers {
    request: NetworkPortRequestV1,
    response: NetworkPortResponseV1,
    description: NetworkPortDescriptionV1,
    tx: [u8; NETWORK_PORT_MAX_FRAME_BYTES],
    rx: [u8; NETWORK_PORT_MAX_FRAME_BYTES],
}

unsafe impl Sync for ProbeStorage {}

static STORAGE: ProbeStorage = ProbeStorage(UnsafeCell::new(ProbeBuffers {
    request: NetworkPortRequestV1::new(0, PackedCapability::from_raw(0)),
    response: NetworkPortResponseV1::new(0, 0),
    description: NetworkPortDescriptionV1::empty(),
    tx: [0; NETWORK_PORT_MAX_FRAME_BYTES],
    rx: [0; NETWORK_PORT_MAX_FRAME_BYTES],
}));

#[cfg(all(not(test), not(feature = "secure-transport")))]
#[unsafe(no_mangle)]
pub extern "C" fn _start(bootstrap_ptr: u64, console_raw: u64) -> ! {
    let console = PackedCapability::from_raw(console_raw);
    // SAFETY: PythCore supplies one aligned bootstrap page for this probe.
    let bootstrap = unsafe { (bootstrap_ptr as *const NetworkPortBootstrapV1).read() };
    let mut service = SocketService::new();
    if !valid_bootstrap_header(bootstrap) {
        error(console);
    }
    if bootstrap.port_capability.raw() == 0 {
        if service.open(None, ACCEPTED_ENDPOINT).is_ok() || service.handle().is_some() {
            error(console);
        }
        write_marker(console, SOCKET_DENIED_BOOTSTRAPPED_MARKER);
        write_marker(console, SOCKET_OPEN_WITHOUT_CAP_DENIED_MARKER);
        success_breakpoint();
    }
    if !valid_bootstrap(bootstrap) {
        error(console);
    }
    write_marker(console, SOCKET_BOOTSTRAPPED_MARKER);

    let capability = bootstrap.port_capability;
    if describe(capability) != NETWORK_PORT_STATUS_OK || !valid_description() {
        error(console);
    }
    let authority = CapabilityAuthority::new(capability, true);
    let handle = match service.open(Some(authority), ACCEPTED_ENDPOINT) {
        Ok(handle) => handle,
        Err(_) => error(console),
    };
    write_marker(console, SOCKET_OPEN_GRANTED_MARKER);

    if send_frame(capability, arp_request_frame()).is_err()
        || !receive_matching_frame(capability, arp_reply_matches, 0, console)
    {
        error(console);
    }
    if send_tcp_frame(capability, 0).is_err()
        || !receive_matching_frame(capability, tcp_frame_matches, 1, console)
        || send_tcp_frame(capability, 2).is_err()
    {
        error(console);
    }
    if service.establish(handle, authority).is_err() {
        error(console);
    }
    write_marker(console, SOCKET_HANDSHAKE_OK_MARKER);

    if service.send(handle, authority, REQUEST_PAYLOAD).is_err()
        || send_tcp_frame(capability, 3).is_err()
    {
        error(console);
    }
    write_marker(console, SOCKET_REQUEST_OK_MARKER);
    if !receive_matching_frame(capability, tcp_frame_matches, 4, console) {
        error(console);
    }
    let mut response = [0u8; SOCKET_PAYLOAD_BYTES];
    if service
        .receive(handle, authority, RESPONSE_PAYLOAD, &mut response)
        .is_err()
    {
        error(console);
    }
    write_marker(console, SOCKET_RESPONSE_OK_MARKER);

    if send_tcp_frame(capability, 5).is_err()
        || send_tcp_frame(capability, 6).is_err()
        || !receive_matching_frame(capability, tcp_frame_matches, 7, console)
        || !receive_matching_frame(capability, tcp_frame_matches, 8, console)
        || send_tcp_frame(capability, 9).is_err()
    {
        error(console);
    }
    if service.close(handle, authority).is_err() || service.handle().is_some() {
        error(console);
    }
    write_marker(console, SOCKET_CLOSE_OK_MARKER);
    success_breakpoint();
}

#[cfg(all(not(test), feature = "secure-transport"))]
#[unsafe(no_mangle)]
pub extern "C" fn _start(bootstrap_ptr: u64, console_raw: u64) -> ! {
    secure::start(bootstrap_ptr, console_raw)
}

fn valid_bootstrap(bootstrap: NetworkPortBootstrapV1) -> bool {
    valid_bootstrap_header(bootstrap) && bootstrap.port_capability.raw() != 0
}

fn valid_bootstrap_header(bootstrap: NetworkPortBootstrapV1) -> bool {
    bootstrap.magic == NETWORK_PORT_BOOTSTRAP_MAGIC
        && bootstrap.abi_major == NETWORK_PORT_ABI_MAJOR
        && bootstrap.abi_minor == NETWORK_PORT_ABI_MINOR
        && bootstrap.reserved0 == 0
        && bootstrap.reserved == [0; 5]
}

fn valid_description() -> bool {
    // SAFETY: one probe thread accesses fixed storage serially.
    let description = unsafe { &*STORAGE.0.get() }.description;
    description.mac == LOCAL_MAC
        && description.reserved0 == [0; 2]
        && description.reserved1 == 0
        && description.min_frame_bytes as usize == NETWORK_PORT_MIN_FRAME_BYTES
        && description.max_frame_bytes as usize == NETWORK_PORT_MAX_FRAME_BYTES
        && description.transport_flags == NETWORK_PORT_FLAG_MAC_ONLY | NETWORK_PORT_FLAG_NO_OFFLOAD
        && description.state == u32::from(NETWORK_PORT_STATE_READY)
}

fn arp_request_frame() -> [u8; NETWORK_PORT_MIN_FRAME_BYTES] {
    let packet = ArpPacket {
        hardware_type: 1,
        protocol_type: IPV4_ETHER_TYPE,
        hardware_len: 6,
        protocol_len: 4,
        operation: 1,
        sender_hardware: LOCAL_MAC,
        sender_protocol: LOCAL_IPV4,
        target_hardware: [0; 6],
        target_protocol: PEER_IPV4,
    };
    let mut frame = [0; NETWORK_PORT_MIN_FRAME_BYTES];
    frame[..6].copy_from_slice(&BROADCAST_MAC);
    frame[6..12].copy_from_slice(&LOCAL_MAC);
    frame[12..14].copy_from_slice(&ARP_ETHER_TYPE.to_be_bytes());
    frame[14..14 + ARP_PAYLOAD_BYTES].copy_from_slice(&encode_arp(packet));
    frame
}

fn arp_reply_matches(frame_bytes: &[u8], _: usize) -> bool {
    if frame_bytes.len() != NETWORK_PORT_MIN_FRAME_BYTES {
        return false;
    }
    let Ok(frame) = ethernet::parse(frame_bytes) else {
        return false;
    };
    if frame.destination != LOCAL_MAC
        || frame.source != PEER_MAC
        || frame.ether_type != ARP_ETHER_TYPE
        || !frame.payload[ARP_PAYLOAD_BYTES..]
            .iter()
            .all(|byte| *byte == 0)
    {
        return false;
    }
    let Ok(packet) = parse_arp(&frame.payload[..ARP_PAYLOAD_BYTES]) else {
        return false;
    };
    packet
        == (ArpPacket {
            hardware_type: 1,
            protocol_type: IPV4_ETHER_TYPE,
            hardware_len: 6,
            protocol_len: 4,
            operation: 2,
            sender_hardware: PEER_MAC,
            sender_protocol: PEER_IPV4,
            target_hardware: LOCAL_MAC,
            target_protocol: LOCAL_IPV4,
        })
}

fn expected_tcp(index: usize) -> Option<TcpSegment<'static>> {
    let inbound = matches!(index, 1 | 4 | 7 | 8);
    let (source_port, destination_port) = if inbound {
        (TCP_DESTINATION_PORT, TCP_SOURCE_PORT)
    } else {
        (TCP_SOURCE_PORT, TCP_DESTINATION_PORT)
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
            REQUEST_PAYLOAD.as_slice(),
        ),
        4 => (
            TCP_PEER_ISS + 1,
            TCP_LOCAL_ISS + 7,
            TCP_FLAG_ACK,
            None,
            RESPONSE_PAYLOAD.as_slice(),
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

fn tcp_frame(index: usize) -> Option<[u8; NETWORK_PORT_MIN_FRAME_BYTES]> {
    let segment = expected_tcp(index)?;
    let (source, destination) = if matches!(index, 1 | 4 | 7 | 8) {
        (PEER_IPV4, LOCAL_IPV4)
    } else {
        (LOCAL_IPV4, PEER_IPV4)
    };
    let tcp_len = if segment.mss.is_some() {
        TCP_SYN_HEADER_BYTES
    } else {
        TCP_HEADER_BYTES
    } + segment.data.len();
    let mut tcp_bytes = [0; TCP_SYN_HEADER_BYTES + SOCKET_PAYLOAD_BYTES];
    if encode_segment(segment, source, destination, &mut tcp_bytes[..tcp_len]).ok()? != tcp_len {
        return None;
    }
    let ip_len = 20 + tcp_len;
    let mut ip_bytes = [0; 20 + TCP_SYN_HEADER_BYTES + SOCKET_PAYLOAD_BYTES];
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
    if encode_ipv4_packet(header, &tcp_bytes[..tcp_len], &mut ip_bytes[..ip_len]).ok()? != ip_len {
        return None;
    }
    let (destination_mac, source_mac) = if source == LOCAL_IPV4 {
        (PEER_MAC, LOCAL_MAC)
    } else {
        (LOCAL_MAC, PEER_MAC)
    };
    ethernet::encode_minimum_frame(
        destination_mac,
        source_mac,
        IPV4_ETHER_TYPE,
        &ip_bytes[..ip_len],
    )
    .ok()
}

fn tcp_frame_matches(frame_bytes: &[u8], index: usize) -> bool {
    let Some(expected) = expected_tcp(index) else {
        return false;
    };
    if frame_bytes.len() != NETWORK_PORT_MIN_FRAME_BYTES {
        return false;
    }
    let Ok(frame) = ethernet::parse(frame_bytes) else {
        return false;
    };
    let (source, destination) = if matches!(index, 1 | 4 | 7 | 8) {
        (PEER_IPV4, LOCAL_IPV4)
    } else {
        (LOCAL_IPV4, PEER_IPV4)
    };
    let (expected_destination, expected_source) = if source == LOCAL_IPV4 {
        (PEER_MAC, LOCAL_MAC)
    } else {
        (LOCAL_MAC, PEER_MAC)
    };
    if frame.destination != expected_destination
        || frame.source != expected_source
        || frame.ether_type != IPV4_ETHER_TYPE
    {
        return false;
    }
    let Ok(packet) = decode_ipv4_packet(frame.payload) else {
        return false;
    };
    let tcp_len = if expected.mss.is_some() {
        TCP_SYN_HEADER_BYTES
    } else {
        TCP_HEADER_BYTES
    } + expected.data.len();
    let expected_header = Ipv4Header {
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
    let ip_len = 20 + tcp_len;
    if packet.total_length as usize != ip_len
        || packet.header != expected_header
        || !frame.payload[ip_len..].iter().all(|byte| *byte == 0)
    {
        return false;
    }
    decode_segment(packet.payload, source, destination)
        .map(|actual| actual == expected)
        .unwrap_or(false)
}

fn send_tcp_frame(capability: PackedCapability, index: usize) -> Result<(), ()> {
    send_frame(capability, tcp_frame(index).ok_or(())?)
}

fn receive_matching_frame(
    capability: PackedCapability,
    matches: fn(&[u8], usize) -> bool,
    index: usize,
    console: PackedCapability,
) -> bool {
    let mut empty_polls: usize = 0;
    loop {
        let status = receive(capability);
        if status == NETWORK_PORT_STATUS_EMPTY {
            empty_polls = empty_polls.saturating_add(1);
            if empty_polls >= MAX_RECEIVE_POLL_ATTEMPTS {
                error(console)
            }
            core::hint::spin_loop();
            continue;
        }
        if status != NETWORK_PORT_STATUS_OK {
            return false;
        }
        // SAFETY: receive wrote only the fixed RX buffer and bounded response length.
        let buffers = unsafe { &*STORAGE.0.get() };
        let frame_len = buffers.response.frame_len as usize;
        return frame_len == NETWORK_PORT_MIN_FRAME_BYTES
            && matches(&buffers.rx[..frame_len], index);
    }
}

fn describe(capability: PackedCapability) -> u16 {
    // SAFETY: one probe thread accesses fixed storage serially.
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.description = NetworkPortDescriptionV1::empty();
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_DESCRIBE, capability);
    buffers.request.output_ptr = &mut buffers.description as *mut _ as u64;
    buffers.request.output_len = size_of::<NetworkPortDescriptionV1>() as u64;
    request(buffers)
}

fn send_frame(
    capability: PackedCapability,
    frame: [u8; NETWORK_PORT_MIN_FRAME_BYTES],
) -> Result<(), ()> {
    send_frame_bytes(capability, &frame)
}

fn send_frame_bytes(capability: PackedCapability, frame: &[u8]) -> Result<(), ()> {
    if !(NETWORK_PORT_MIN_FRAME_BYTES..=NETWORK_PORT_MAX_FRAME_BYTES).contains(&frame.len()) {
        return Err(());
    }
    // SAFETY: one probe thread accesses fixed storage serially.
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.tx.fill(0);
    buffers.tx[..frame.len()].copy_from_slice(frame);
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_SEND, capability);
    buffers.request.input_ptr = buffers.tx.as_ptr() as u64;
    buffers.request.input_len = frame.len() as u64;
    (request(buffers) == NETWORK_PORT_STATUS_OK)
        .then_some(())
        .ok_or(())
}

fn receive(capability: PackedCapability) -> u16 {
    // SAFETY: one probe thread accesses fixed storage serially.
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
        &buffers.request as *const _ as u64,
        size_of::<NetworkPortRequestV1>() as u64,
        &mut buffers.response as *mut _ as u64,
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
    // SAFETY: established x86-64 five-argument user syscall ABI.
    unsafe {
        asm!("syscall", inout("rax") number => result, inout("rdi") arg1 => _, inout("rsi") arg2 => _, inout("rdx") arg3 => _, inout("r10") arg4 => _, inout("r8") arg5 => _, lateout("r9") _, lateout("rcx") _, lateout("r11") _, options(nostack));
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
    // SAFETY: bounded probe completion is caught by PythCore.
    unsafe { asm!("int3", options(nomem, nostack)) }
    loop {
        core::hint::spin_loop()
    }
}

fn error(console: PackedCapability) -> ! {
    write_marker(console, SOCKET_ERROR_MARKER);
    // SAFETY: malformed input and syscall failures are terminal.
    unsafe { asm!("ud2", options(noreturn, nomem, nostack)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_tcp_validator_rejects_header_and_padding_mutations() {
        let canonical = tcp_frame(1).expect("canonical TCP frame");
        assert!(tcp_frame_matches(&canonical, 1));

        let mut header_mutation = canonical;
        header_mutation[14] ^= 1;
        assert!(!tcp_frame_matches(&header_mutation, 1));

        let mut padding_mutation = canonical;
        *padding_mutation.last_mut().unwrap() = 1;
        assert!(!tcp_frame_matches(&padding_mutation, 1));
    }

    #[test]
    fn native_response_validator_rejects_reserved_bytes() {
        let response = NetworkPortResponseV1::new(NETWORK_PORT_STATUS_OK, NETWORK_PORT_STATE_READY);
        assert!(valid_response(response));
        let mut reserved = response;
        reserved.reserved3 = 1;
        assert!(!valid_response(reserved));
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    unsafe { asm!("ud2", options(noreturn, nomem, nostack)) }
}
