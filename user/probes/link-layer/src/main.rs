#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(dead_code, unused_imports))]

mod ethernet;

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
};

use pythos_shared::link_layer_markers::{
    LINK_LAYER_BOOTSTRAPPED_MARKER, LINK_LAYER_DESCRIBE_OK_MARKER, LINK_LAYER_RX_OK_MARKER,
    LINK_LAYER_TX_OK_MARKER, LINK_LAYER_WRONG_DESTINATION_DENIED_MARKER,
    LINK_LAYER_WRONG_ETHERTYPE_DENIED_MARKER,
};

const PEER_MAC: [u8; 6] = [2, 0, 0, 0, 0, 2];
const ETHERTYPE: u16 = 0x88B5;
const WRONG_ETHERTYPE: u16 = 0x88B6;
const TX_PAYLOAD: &[u8] = b"PYTHOS:LINK:TX";
const RX_PAYLOAD: &[u8] = b"PYTHOS:LINK:RX";

struct ProbeStorage(UnsafeCell<ProbeBuffers>);

struct ProbeBuffers {
    request: NetworkPortRequestV1,
    response: NetworkPortResponseV1,
    description: NetworkPortDescriptionV1,
    tx: [u8; NETWORK_PORT_MIN_FRAME_BYTES],
    rx: [u8; NETWORK_PORT_MAX_FRAME_BYTES],
}

// SAFETY: the finite user probe has one thread and accesses this storage serially.
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
    // SAFETY: PythCore supplies one aligned, read-only NetworkPortBootstrapV1 mapping.
    let bootstrap = unsafe { (bootstrap_ptr as *const NetworkPortBootstrapV1).read() };
    if !valid_bootstrap(bootstrap) {
        error(console);
    }
    write_marker(console, LINK_LAYER_BOOTSTRAPPED_MARKER);

    let capability = bootstrap.port_capability;
    if describe(capability) != NETWORK_PORT_STATUS_OK || !valid_description() {
        error(console);
    }
    write_marker(console, LINK_LAYER_DESCRIBE_OK_MARKER);

    if send(capability) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    write_marker(console, LINK_LAYER_TX_OK_MARKER);

    receive_exchange(capability, console);
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
    // SAFETY: this single-threaded probe has exclusive access between syscalls.
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

fn payload_matches(payload: &[u8], token: &[u8]) -> bool {
    payload.len() >= token.len()
        && payload[..token.len()] == *token
        && payload[token.len()..].iter().all(|byte| *byte == 0)
}

fn describe(capability: PackedCapability) -> u16 {
    // SAFETY: this single-threaded probe has exclusive access between syscalls.
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.description = NetworkPortDescriptionV1::empty();
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_DESCRIBE, capability);
    buffers.request.output_ptr = &mut buffers.description as *mut NetworkPortDescriptionV1 as u64;
    buffers.request.output_len = size_of::<NetworkPortDescriptionV1>() as u64;
    request(buffers)
}

fn send(capability: PackedCapability) -> u16 {
    // SAFETY: this single-threaded probe has exclusive access between syscalls.
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.tx = match ethernet::encode_minimum_frame(
        PEER_MAC,
        buffers.description.mac,
        ETHERTYPE,
        TX_PAYLOAD,
    ) {
        Ok(frame) => frame,
        Err(_) => return u16::MAX,
    };
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_SEND, capability);
    buffers.request.input_ptr = buffers.tx.as_ptr() as u64;
    buffers.request.input_len = buffers.tx.len() as u64;
    request(buffers)
}

fn receive_exchange(capability: PackedCapability, console: PackedCapability) -> ! {
    let mut rejected_destination = false;
    let mut rejected_ethertype = false;

    loop {
        let status = receive(capability);
        if status == NETWORK_PORT_STATUS_EMPTY {
            core::hint::spin_loop();
            continue;
        }
        if status != NETWORK_PORT_STATUS_OK {
            error(console);
        }

        // SAFETY: `receive` accepts only the fixed receive buffer and returns a bounded length.
        let buffers = unsafe { &*STORAGE.0.get() };
        let frame_len = buffers.response.frame_len as usize;
        if !(NETWORK_PORT_MIN_FRAME_BYTES..=NETWORK_PORT_MAX_FRAME_BYTES).contains(&frame_len) {
            error(console);
        }
        let frame = match ethernet::parse(&buffers.rx[..frame_len]) {
            Ok(frame) => frame,
            Err(_) => error(console),
        };

        if !rejected_destination {
            if frame.source != PEER_MAC
                || frame.ether_type != ETHERTYPE
                || !payload_matches(frame.payload, RX_PAYLOAD)
            {
                error(console);
            }
            if frame.destination == buffers.description.mac {
                error(console);
            }
            rejected_destination = true;
            write_marker(console, LINK_LAYER_WRONG_DESTINATION_DENIED_MARKER);
            continue;
        }

        if !rejected_ethertype {
            if frame.destination != buffers.description.mac
                || frame.source != PEER_MAC
                || frame.ether_type != WRONG_ETHERTYPE
                || !payload_matches(frame.payload, RX_PAYLOAD)
            {
                error(console);
            }
            rejected_ethertype = true;
            write_marker(console, LINK_LAYER_WRONG_ETHERTYPE_DENIED_MARKER);
            continue;
        }

        if frame.destination != buffers.description.mac
            || frame.source != PEER_MAC
            || frame.ether_type != ETHERTYPE
            || !payload_matches(frame.payload, RX_PAYLOAD)
        {
            error(console);
        }
        write_marker(console, LINK_LAYER_RX_OK_MARKER);
        success_breakpoint();
    }
}

#[cfg(test)]
fn frame_matches_policy(frame: &ethernet::EthernetFrame<'_>, local: [u8; 6]) -> bool {
    frame.destination == local
        && frame.source == PEER_MAC
        && frame.ether_type == ETHERTYPE
        && payload_matches(frame.payload, RX_PAYLOAD)
}

fn receive(capability: PackedCapability) -> u16 {
    // SAFETY: this single-threaded probe has exclusive access between syscalls.
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
    write_marker(console, "PYTHOS:CORE:LINK_LAYER:ERROR");
    // SAFETY: the bounded probe error path terminates the process on malformed input or syscall failure.
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
        ETHERTYPE, PEER_MAC, frame_matches_policy, valid_description_fields, valid_response,
    };
    use crate::ethernet::{encode_minimum_frame, parse};
    use pythos_shared::network_port_abi::{
        NETWORK_PORT_STATE_READY, NETWORK_PORT_STATUS_EMPTY, NETWORK_PORT_STATUS_OK,
        NetworkPortDescriptionV1, NetworkPortResponseV1,
    };

    #[test]
    fn receive_policy_accepts_a_real_padded_minimum_frame() {
        let bytes =
            encode_minimum_frame([2, 0, 0, 0, 0, 1], PEER_MAC, ETHERTYPE, b"PYTHOS:LINK:RX")
                .unwrap();
        let frame = parse(&bytes).unwrap();
        assert!(frame_matches_policy(&frame, [2, 0, 0, 0, 0, 1]));
    }

    #[test]
    fn response_validation_requires_ready_state_for_success_and_empty() {
        assert!(valid_response(NetworkPortResponseV1::new(
            NETWORK_PORT_STATUS_OK,
            NETWORK_PORT_STATE_READY,
        )));
        assert!(valid_response(NetworkPortResponseV1::new(
            NETWORK_PORT_STATUS_EMPTY,
            NETWORK_PORT_STATE_READY,
        )));
        assert!(!valid_response(NetworkPortResponseV1::new(
            NETWORK_PORT_STATUS_OK,
            3,
        )));
    }

    #[test]
    fn description_validation_requires_zero_reserved_fields() {
        let mut description = NetworkPortDescriptionV1 {
            min_frame_bytes: 60,
            max_frame_bytes: 1514,
            transport_flags: 3,
            state: 1,
            ..NetworkPortDescriptionV1::empty()
        };
        assert!(valid_description_fields(description));
        description.reserved0 = [1, 0];
        assert!(!valid_description_fields(description));
        description.reserved0 = [0; 2];
        description.reserved1 = 1;
        assert!(!valid_description_fields(description));
    }
}
