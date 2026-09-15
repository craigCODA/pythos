#![no_std]
#![no_main]

use core::{
    arch::asm,
    cell::UnsafeCell,
    mem::size_of,
    panic::PanicInfo,
    sync::atomic::{AtomicU8, Ordering},
};
use pythos_shared::{
    capability_abi::PackedCapability,
    network_port_abi::{
        NETWORK_PORT_ABI_MAJOR, NETWORK_PORT_ABI_MINOR, NETWORK_PORT_BOOTSTRAP_MAGIC,
        NETWORK_PORT_FLAG_MAC_ONLY, NETWORK_PORT_FLAG_NO_OFFLOAD, NETWORK_PORT_MAX_FRAME_BYTES,
        NETWORK_PORT_MIN_FRAME_BYTES, NETWORK_PORT_OP_DESCRIBE, NETWORK_PORT_OP_SEND,
        NETWORK_PORT_OP_TRY_RECEIVE, NETWORK_PORT_STATE_READY, NETWORK_PORT_STATUS_BAD_REQUEST,
        NETWORK_PORT_STATUS_DENIED, NETWORK_PORT_STATUS_EMPTY, NETWORK_PORT_STATUS_OK,
        NetworkPortBootstrapV1, NetworkPortDescriptionV1, NetworkPortRequestV1,
        NetworkPortResponseV1, SYSCALL_NETWORK_PORT_REQUEST,
    },
    object_shell_abi::{SYSCALL_CONSOLE_WRITE_BYTE, SYSCALL_OK},
};

const PEER_MAC: [u8; 6] = [2, 0, 0, 0, 0, 2];
const ETHER_TYPE: u16 = 0x88B5;

struct ProbeStorage(UnsafeCell<ProbeBuffers>);

struct ProbeBuffers {
    request: NetworkPortRequestV1,
    response: NetworkPortResponseV1,
    description: NetworkPortDescriptionV1,
    tx: [u8; NETWORK_PORT_MIN_FRAME_BYTES],
    rx: [u8; NETWORK_PORT_MAX_FRAME_BYTES],
}

unsafe impl Sync for ProbeStorage {}

static STORAGE: ProbeStorage = ProbeStorage(UnsafeCell::new(ProbeBuffers {
    request: NetworkPortRequestV1::new(0, PackedCapability::from_raw(0)),
    response: NetworkPortResponseV1::new(0, 0),
    description: NetworkPortDescriptionV1::empty(),
    tx: [0; NETWORK_PORT_MIN_FRAME_BYTES],
    rx: [0; NETWORK_PORT_MAX_FRAME_BYTES],
}));

static EXECUTION_PHASE: AtomicU8 = AtomicU8::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn _start(bootstrap_ptr: u64, console_raw: u64) -> ! {
    let console = PackedCapability::from_raw(console_raw);
    let bootstrap = unsafe { (bootstrap_ptr as *const NetworkPortBootstrapV1).read() };
    if !valid_bootstrap(bootstrap) {
        error(console);
    }
    let capability = bootstrap.port_capability;

    match EXECUTION_PHASE.load(Ordering::SeqCst) {
        0 => consumer_phase(capability, console),
        1 => wrong_holder_phase(capability, console),
        2 => bad_buffer_phase(capability, console),
        _ => error(console),
    }
}

fn consumer_phase(capability: PackedCapability, console: PackedCapability) -> ! {
    let describe_status = describe(capability);
    if describe_status != NETWORK_PORT_STATUS_OK || !valid_description() {
        error(console);
    }
    write_str(console, "PYTHOS:CORE:NETWORK_PORT:BOOTSTRAPPED\r\n");
    write_str(console, "PYTHOS:CORE:NETWORK_PORT:DESCRIBE_OK\r\n");

    if send(capability) != NETWORK_PORT_STATUS_OK {
        error(console);
    }
    write_str(console, "PYTHOS:CORE:NETWORK_PORT:TX_OK\r\n");

    loop {
        match receive(capability) {
            NETWORK_PORT_STATUS_EMPTY => core::hint::spin_loop(),
            NETWORK_PORT_STATUS_OK if valid_received_frame() => break,
            _ => error(console),
        }
    }
    write_str(console, "PYTHOS:CORE:NETWORK_PORT:RX_OK\r\n");

    let forged = PackedCapability::from_parts(capability.slot(), capability.generation() ^ 1);
    if describe(forged) != NETWORK_PORT_STATUS_DENIED {
        error(console);
    }
    write_str(console, "PYTHOS:CORE:NETWORK_PORT:FORGED_DENIED\r\n");

    EXECUTION_PHASE.store(1, Ordering::SeqCst);
    success_breakpoint();
}

fn wrong_holder_phase(capability: PackedCapability, console: PackedCapability) -> ! {
    if describe(capability) != NETWORK_PORT_STATUS_DENIED {
        error(console);
    }
    write_str(console, "PYTHOS:CORE:NETWORK_PORT:WRONG_HOLDER_DENIED\r\n");
    EXECUTION_PHASE.store(2, Ordering::SeqCst);
    success_breakpoint();
}

fn bad_buffer_phase(capability: PackedCapability, console: PackedCapability) -> ! {
    if bad_buffer(capability) != NETWORK_PORT_STATUS_BAD_REQUEST {
        error(console);
    }
    write_str(console, "PYTHOS:CORE:NETWORK_PORT:BAD_BUFFER_DENIED\r\n");
    EXECUTION_PHASE.store(3, Ordering::SeqCst);
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
    let buffers = unsafe { &*STORAGE.0.get() };
    buffers.description.min_frame_bytes as usize == NETWORK_PORT_MIN_FRAME_BYTES
        && buffers.description.max_frame_bytes as usize == NETWORK_PORT_MAX_FRAME_BYTES
        && buffers.description.transport_flags
            == NETWORK_PORT_FLAG_MAC_ONLY | NETWORK_PORT_FLAG_NO_OFFLOAD
        && buffers.description.state == u32::from(NETWORK_PORT_STATE_READY)
}

fn send(capability: PackedCapability) -> u16 {
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.tx[0..6].copy_from_slice(&PEER_MAC);
    buffers.tx[6..12].copy_from_slice(&buffers.description.mac);
    buffers.tx[12..14].copy_from_slice(&ETHER_TYPE.to_be_bytes());
    buffers.tx[14..27].copy_from_slice(b"PYTHOS:NIC:TX");
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_SEND, capability);
    buffers.request.input_ptr = buffers.tx.as_ptr() as u64;
    buffers.request.input_len = buffers.tx.len() as u64;
    request(buffers)
}

fn receive(capability: PackedCapability) -> u16 {
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_TRY_RECEIVE, capability);
    buffers.request.output_ptr = buffers.rx.as_mut_ptr() as u64;
    buffers.request.output_len = buffers.rx.len() as u64;
    request(buffers)
}

fn describe(capability: PackedCapability) -> u16 {
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.description = NetworkPortDescriptionV1::empty();
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_DESCRIBE, capability);
    buffers.request.output_ptr = &mut buffers.description as *mut NetworkPortDescriptionV1 as u64;
    buffers.request.output_len = size_of::<NetworkPortDescriptionV1>() as u64;
    request(buffers)
}

fn bad_buffer(capability: PackedCapability) -> u16 {
    let buffers = unsafe { &mut *STORAGE.0.get() };
    buffers.request = NetworkPortRequestV1::new(NETWORK_PORT_OP_SEND, capability);
    buffers.request.input_ptr = 0x0000_0000_7400_0000;
    buffers.request.input_len = NETWORK_PORT_MIN_FRAME_BYTES as u64;
    request(buffers)
}

fn request(buffers: &mut ProbeBuffers) -> u16 {
    buffers.response = NetworkPortResponseV1::new(0xFFFF, 0);
    let result = syscall5(
        SYSCALL_NETWORK_PORT_REQUEST,
        &buffers.request as *const NetworkPortRequestV1 as u64,
        size_of::<NetworkPortRequestV1>() as u64,
        &mut buffers.response as *mut NetworkPortResponseV1 as u64,
        size_of::<NetworkPortResponseV1>() as u64,
        0,
    );
    if result != SYSCALL_OK {
        0xFFFF
    } else {
        buffers.response.status
    }
}

fn valid_received_frame() -> bool {
    let buffers = unsafe { &*STORAGE.0.get() };
    buffers.response.frame_len == NETWORK_PORT_MIN_FRAME_BYTES as u64
        && buffers.rx[0..6] == buffers.description.mac
        && buffers.rx[6..12] == PEER_MAC
        && buffers.rx[12..14] == ETHER_TYPE.to_be_bytes()
        && buffers.rx[14..27] == *b"PYTHOS:NIC:RX"
}

fn syscall5(number: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
    let result: u64;
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

fn write_str(console: PackedCapability, text: &str) {
    for byte in text.bytes() {
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
    unsafe { asm!("int3", options(nomem, nostack)) };
    loop {
        core::hint::spin_loop();
    }
}

fn error(console: PackedCapability) -> ! {
    write_str(console, "PYTHOS:CORE:NETWORK_PORT:ERROR\r\n");
    unsafe { asm!("ud2", options(noreturn, nomem, nostack)) }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    unsafe { asm!("ud2", options(noreturn, nomem, nostack)) }
}
