#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(dead_code, unused_imports))]

#[cfg(not(test))]
use core::panic::PanicInfo;
use core::{arch::asm, cell::UnsafeCell, mem::size_of};
use pythos_shared::{
    arp_markers::{
        ARP_BOOTSTRAPPED_MARKER, ARP_DESCRIBE_OK_MARKER, ARP_REPLY_OK_MARKER, ARP_REQUEST_OK_MARKER,
    },
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
};
use pythos_user_arp_probe::arp::{ARP_PAYLOAD_BYTES, ArpPacket, encode, parse};
use pythos_user_link_layer_probe::ethernet;

const BROADCAST_MAC: [u8; 6] = [0xff; 6];
const PEER_MAC: [u8; 6] = [2, 0, 0, 0, 0, 2];
const LOCAL_IPV4: [u8; 4] = [192, 0, 2, 2];
const PEER_IPV4: [u8; 4] = [192, 0, 2, 1];
const ARP_ETHER_TYPE: u16 = 0x0806;

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
    write_marker(console, ARP_BOOTSTRAPPED_MARKER);

    let capability = bootstrap.port_capability;
    if describe(capability) != NETWORK_PORT_STATUS_OK || !valid_description() {
        error(console);
    }
    write_marker(console, ARP_DESCRIBE_OK_MARKER);

    if send_request(capability) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    write_marker(console, ARP_REQUEST_OK_MARKER);

    receive_reply(capability, console)
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

fn request_packet(local_mac: [u8; 6]) -> ArpPacket {
    ArpPacket {
        hardware_type: 1,
        protocol_type: 0x0800,
        hardware_len: 6,
        protocol_len: 4,
        operation: 1,
        sender_hardware: local_mac,
        sender_protocol: LOCAL_IPV4,
        target_hardware: [0; 6],
        target_protocol: PEER_IPV4,
    }
}

fn request_frame(local_mac: [u8; 6]) -> [u8; NETWORK_PORT_MIN_FRAME_BYTES] {
    let mut frame = [0; NETWORK_PORT_MIN_FRAME_BYTES];
    frame[0..6].copy_from_slice(&BROADCAST_MAC);
    frame[6..12].copy_from_slice(&local_mac);
    frame[12..14].copy_from_slice(&ARP_ETHER_TYPE.to_be_bytes());
    frame[14..14 + ARP_PAYLOAD_BYTES].copy_from_slice(&encode(request_packet(local_mac)));
    frame
}

fn reply_matches(packet: ArpPacket, local_mac: [u8; 6]) -> bool {
    packet.hardware_type == 1
        && packet.protocol_type == 0x0800
        && packet.hardware_len == 6
        && packet.protocol_len == 4
        && packet.operation == 2
        && packet.sender_hardware == PEER_MAC
        && packet.sender_protocol == PEER_IPV4
        && packet.target_hardware == local_mac
        && packet.target_protocol == LOCAL_IPV4
}

fn reply_frame_matches(frame_bytes: &[u8], local_mac: [u8; 6]) -> bool {
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
    if frame.payload.len() < ARP_PAYLOAD_BYTES {
        return false;
    }
    match parse(&frame.payload[..ARP_PAYLOAD_BYTES]) {
        Ok(packet) => reply_matches(packet, local_mac),
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

fn send_request(capability: PackedCapability) -> u16 {
    // SAFETY: the single-threaded probe exclusively accesses fixed storage between syscalls.
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.tx = request_frame(buffers.description.mac);
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_SEND, capability);
    buffers.request.input_ptr = buffers.tx.as_ptr() as u64;
    buffers.request.input_len = buffers.tx.len() as u64;
    request(buffers)
}

fn receive_reply(capability: PackedCapability, console: PackedCapability) -> ! {
    loop {
        let status = receive(capability);
        if status == NETWORK_PORT_STATUS_EMPTY {
            core::hint::spin_loop();
            continue;
        }
        if status != NETWORK_PORT_STATUS_OK {
            error(console);
        }

        // SAFETY: `receive` writes only the fixed RX buffer and its response is bounded below.
        let buffers = unsafe { &*STORAGE.0.get() };
        let frame_len = buffers.response.frame_len as usize;
        if frame_len != NETWORK_PORT_MIN_FRAME_BYTES {
            error(console);
        }
        // The NetworkPort frame length has been checked against the RX buffer before slicing.
        if !reply_frame_matches(&buffers.rx[..frame_len], buffers.description.mac) {
            error(console);
        }
        write_marker(console, ARP_REPLY_OK_MARKER);
        success_breakpoint();
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
    write_marker(console, "PYTHOS:CORE:ARP:ERROR");
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
    use super::{reply_frame_matches, reply_matches, request_frame};
    use pythos_user_arp_probe::arp::{ArpPacket, encode, parse};
    use pythos_user_link_layer_probe::ethernet;

    const LOCAL_MAC: [u8; 6] = [2, 0, 0, 0, 0, 1];
    const PEER_MAC: [u8; 6] = [2, 0, 0, 0, 0, 2];
    const LOCAL_IPV4: [u8; 4] = [192, 0, 2, 2];
    const PEER_IPV4: [u8; 4] = [192, 0, 2, 1];

    fn reply() -> ArpPacket {
        ArpPacket {
            hardware_type: 1,
            protocol_type: 0x0800,
            hardware_len: 6,
            protocol_len: 4,
            operation: 2,
            sender_hardware: PEER_MAC,
            sender_protocol: PEER_IPV4,
            target_hardware: LOCAL_MAC,
            target_protocol: LOCAL_IPV4,
        }
    }

    #[test]
    fn request_policy_uses_broadcast_and_zero_target_hardware() {
        let bytes = request_frame(LOCAL_MAC);
        let frame = ethernet::parse(&bytes).unwrap();
        assert_eq!(
            (frame.destination, frame.source, frame.ether_type),
            ([0xff; 6], LOCAL_MAC, 0x0806)
        );
        assert_eq!(
            parse(frame.payload).unwrap(),
            ArpPacket {
                hardware_type: 1,
                protocol_type: 0x0800,
                hardware_len: 6,
                protocol_len: 4,
                operation: 1,
                sender_hardware: LOCAL_MAC,
                sender_protocol: LOCAL_IPV4,
                target_hardware: [0; 6],
                target_protocol: PEER_IPV4,
            }
        );
        assert!(frame.payload[28..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn reply_policy_accepts_only_the_exact_peer_relationship() {
        assert!(reply_matches(reply(), LOCAL_MAC));
        let bytes =
            ethernet::encode_minimum_frame(LOCAL_MAC, PEER_MAC, 0x0806, &encode(reply())).unwrap();
        assert!(reply_frame_matches(&bytes, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_nonzero_ethernet_padding() {
        let mut bytes =
            ethernet::encode_minimum_frame(LOCAL_MAC, PEER_MAC, 0x0806, &encode(reply())).unwrap();
        bytes[42] = 1;

        assert!(!reply_frame_matches(&bytes, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_frames_longer_than_the_exact_minimum() {
        let bytes =
            ethernet::encode_minimum_frame(LOCAL_MAC, PEER_MAC, 0x0806, &encode(reply())).unwrap();
        let mut longer = [0; 61];
        longer[..bytes.len()].copy_from_slice(&bytes);

        assert!(!reply_frame_matches(&longer, LOCAL_MAC));
    }

    #[test]
    fn reply_policy_rejects_wrong_operation_sender_target_or_address_pair() {
        let mut packet = reply();
        packet.operation = 1;
        assert!(!reply_matches(packet, LOCAL_MAC));

        let mut packet = reply();
        packet.sender_hardware = LOCAL_MAC;
        assert!(!reply_matches(packet, LOCAL_MAC));

        let mut packet = reply();
        packet.target_hardware = PEER_MAC;
        assert!(!reply_matches(packet, LOCAL_MAC));

        let mut packet = reply();
        packet.sender_protocol = LOCAL_IPV4;
        assert!(!reply_matches(packet, LOCAL_MAC));

        let mut packet = reply();
        packet.target_protocol = PEER_IPV4;
        assert!(!reply_matches(packet, LOCAL_MAC));
    }

    #[test]
    fn policy_rejects_non_ethernet_or_non_ipv4_arp_fields() {
        let mut packet = reply();
        packet.hardware_type = 2;
        assert!(!reply_matches(packet, LOCAL_MAC));

        let mut packet = reply();
        packet.protocol_type = 0x86dd;
        assert!(!reply_matches(packet, LOCAL_MAC));

        let mut packet = reply();
        packet.hardware_len = 8;
        assert!(!reply_matches(packet, LOCAL_MAC));

        let mut packet = reply();
        packet.protocol_len = 16;
        assert!(!reply_matches(packet, LOCAL_MAC));
    }

    #[test]
    fn reply_marker_policy_rejects_short_or_wrong_ethernet_frames() {
        assert!(!reply_frame_matches(&[0; 59], LOCAL_MAC));

        let wrong_destination =
            ethernet::encode_minimum_frame(PEER_MAC, PEER_MAC, 0x0806, &encode(reply())).unwrap();
        assert!(!reply_frame_matches(&wrong_destination, LOCAL_MAC));

        let wrong_source =
            ethernet::encode_minimum_frame(LOCAL_MAC, LOCAL_MAC, 0x0806, &encode(reply())).unwrap();
        assert!(!reply_frame_matches(&wrong_source, LOCAL_MAC));

        let wrong_type =
            ethernet::encode_minimum_frame(LOCAL_MAC, PEER_MAC, 0x0800, &encode(reply())).unwrap();
        assert!(!reply_frame_matches(&wrong_type, LOCAL_MAC));
    }
}
