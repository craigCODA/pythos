#![cfg_attr(not(any(test, clippy)), no_std)]
#![cfg_attr(not(any(test, clippy)), no_main)]

#[cfg(not(any(test, clippy)))]
use core::panic::PanicInfo;
#[cfg(not(any(test, clippy)))]
use pythos_shared::{
    capability_abi::PackedCapability,
    session_input_abi::{
        SESSION_INPUT_RESULT_EMPTY, SESSION_INPUT_RESULT_EVENT, SessionInputEventV1,
    },
};
#[cfg(not(any(test, clippy)))]
use pythos_user_session_input_probe::{EventSequenceValidator, syscalls};

#[cfg(not(any(test, clippy)))]
#[unsafe(no_mangle)]
pub extern "C" fn _start(input_raw: u64, console_raw: u64) -> ! {
    let input = PackedCapability::from_raw(input_raw);
    let console = PackedCapability::from_raw(console_raw);
    syscalls::write_str(console, "PYTHOS:SESSION_INPUT_PROBE:READY_FOR_INPUT\r\n");

    loop {
        if syscalls::read_byte(console) == Some(b'G') {
            break;
        }
        core::hint::spin_loop();
    }

    let sentinel = sentinel_event();
    let mut forged_output = sentinel;
    let forged = PackedCapability::from_parts(input.slot(), input.generation() ^ 1);
    let forged_result = syscalls::try_read(forged, &mut forged_output);
    if forged_result == SESSION_INPUT_RESULT_EVENT
        || forged_result == SESSION_INPUT_RESULT_EMPTY
        || forged_output != sentinel
    {
        error(console);
    }
    syscalls::write_str(
        console,
        "PYTHOS:SESSION_INPUT_PROBE:FORGED_DENIED_OUTPUT_UNCHANGED\r\n",
    );

    let mut validator = EventSequenceValidator::new();
    while !validator.is_complete() {
        let mut event = SessionInputEventV1::empty();
        match syscalls::try_read(input, &mut event) {
            SESSION_INPUT_RESULT_EMPTY => core::hint::spin_loop(),
            SESSION_INPUT_RESULT_EVENT => match validator.accept(event) {
                Ok(1) => {
                    syscalls::write_str(console, "PYTHOS:SESSION_INPUT_PROBE:EVENT_1_SPACE\r\n")
                }
                Ok(2) => {
                    syscalls::write_str(console, "PYTHOS:SESSION_INPUT_PROBE:EVENT_2_SPACE\r\n")
                }
                Ok(3) => {
                    syscalls::write_str(console, "PYTHOS:SESSION_INPUT_PROBE:EVENT_3_BACKSPACE\r\n")
                }
                Ok(4) => {
                    syscalls::write_str(console, "PYTHOS:SESSION_INPUT_PROBE:EVENT_4_BACKSPACE\r\n")
                }
                Ok(5) => syscalls::write_str(
                    console,
                    "PYTHOS:SESSION_INPUT_PROBE:EVENT_5_RELATIVE_MOTION_DX_7_DY_NEG_7\r\n",
                ),
                _ => error(console),
            },
            _ => error(console),
        }
    }

    syscalls::write_str(console, "PYTHOS:SESSION_INPUT_PROBE:CONTIGUOUS\r\n");
    syscalls::write_str(console, "PYTHOS:SESSION_INPUT_PROBE:READY\r\n");
    // SAFETY:
    // 1. Invariant: all five required records have validated before this trap.
    // 2. Established by: `EventSequenceValidator` above.
    // 3. Lifetime: the condition applies to this one instruction.
    // 4. Pointer ownership: this instruction uses no pointer.
    // 5. Alignment: no pointer or alignment requirement applies.
    // 6. Mapped length: no memory range is accessed.
    // 7. Concurrency: this single-threaded probe has no concurrent path to the trap.
    // 8. Violation: an unexpected trap would fail the delivery proof rather than signal success.
    unsafe { core::arch::asm!("int3", options(nomem, nostack)) };
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(any(test, clippy)))]
fn sentinel_event() -> SessionInputEventV1 {
    SessionInputEventV1 {
        sequence: 0xA5A5_A5A5_A5A5_A5A5,
        kind: 0xA5A5,
        source: 0xA5A5,
        flags: 0xA5A5_A5A5,
        value0: 0xA5A5_A5A5u32 as i32,
        value1: 0xA5A5_A5A5u32 as i32,
        reserved0: 0xA5A5_A5A5_A5A5_A5A5,
        reserved1: 0xA5A5_A5A5_A5A5_A5A5,
    }
}

#[cfg(not(any(test, clippy)))]
fn error(console: PackedCapability) -> ! {
    syscalls::write_str(console, "PYTHOS:SESSION_INPUT_PROBE:ERROR\r\n");
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(any(test, clippy)))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(any(test, clippy))]
fn main() {}
