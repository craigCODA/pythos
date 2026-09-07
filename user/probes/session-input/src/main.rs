#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::{cell::UnsafeCell, panic::PanicInfo};
#[cfg(not(test))]
use pythos_shared::{
    capability_abi::PackedCapability,
    session_input_abi::{
        SESSION_INPUT_RESULT_EMPTY, SESSION_INPUT_RESULT_EVENT, SessionInputEventV1,
    },
};
#[cfg(not(test))]
use pythos_user_session_input_probe::{EventSequenceValidator, syscalls};

#[cfg(not(test))]
#[repr(align(8))]
struct OutputSlot(UnsafeCell<SessionInputEventV1>);

// SAFETY:
// 1. Invariant: only the one finite ring-3 probe execution accesses this slot.
// 2. Established by: this user ELF has no threads, callbacks, or interrupt handler.
// 3. Lifetime: the static belongs to the complete probe execution.
// 4. Pointer ownership: the probe owns the sole mutable reference for each syscall.
// 5. Alignment: `repr(align(8))` satisfies `SessionInputEventV1` alignment.
// 6. Mapped length: exactly one writable BSS record is exposed to PythCore.
// 7. Concurrency: forged and real reads are strictly sequential in `_start`.
// 8. Violation: concurrent access could race the kernel's synchronous copy-out.
#[cfg(not(test))]
unsafe impl Sync for OutputSlot {}

#[cfg(not(test))]
static OUTPUT_SLOT: OutputSlot = OutputSlot(UnsafeCell::new(SessionInputEventV1::empty()));

#[cfg(not(test))]
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
    write_output(sentinel);
    let forged = PackedCapability::from_parts(input.slot(), input.generation() ^ 1);
    let forged_result = try_read_output(forged);
    if forged_result == SESSION_INPUT_RESULT_EVENT
        || forged_result == SESSION_INPUT_RESULT_EMPTY
        || read_output() != sentinel
    {
        error(console);
    }
    syscalls::write_str(
        console,
        "PYTHOS:SESSION_INPUT_PROBE:FORGED_DENIED_OUTPUT_UNCHANGED\r\n",
    );

    let mut validator = EventSequenceValidator::new();
    while !validator.is_complete() {
        write_output(SessionInputEventV1::empty());
        match try_read_output(input) {
            SESSION_INPUT_RESULT_EMPTY => core::hint::spin_loop(),
            SESSION_INPUT_RESULT_EVENT => match validator.accept(read_output()) {
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

#[cfg(not(test))]
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

#[cfg(not(test))]
fn write_output(event: SessionInputEventV1) {
    // SAFETY:
    // 1. Invariant: the slot is the probe's only syscall output record.
    // 2. Established by: `_start` calls this only before a synchronous syscall.
    // 3. Lifetime: the static outlives every use in the finite probe.
    // 4. Pointer ownership: this function creates the only mutable access.
    // 5. Alignment: `OutputSlot` is explicitly aligned for this record.
    // 6. Mapped length: exactly one `SessionInputEventV1` is written.
    // 7. Concurrency: the probe is single-threaded and sequential.
    // 8. Violation: a concurrent writer could corrupt the syscall result.
    unsafe { *OUTPUT_SLOT.0.get() = event };
}

#[cfg(not(test))]
fn try_read_output(input: PackedCapability) -> u64 {
    // SAFETY:
    // 1. Invariant: PythCore receives this one writable BSS record as output.
    // 2. Established by: the validated ELF copy map includes writable data/BSS.
    // 3. Lifetime: the static outlives the synchronous syscall.
    // 4. Pointer ownership: no other reference exists during the syscall.
    // 5. Alignment: `OutputSlot` satisfies the ABI record alignment.
    // 6. Mapped length: the syscall receives exactly one full record.
    // 7. Concurrency: one ring-3 thread issues one syscall at a time.
    // 8. Violation: aliasing during copy-out could corrupt the record.
    unsafe { syscalls::try_read(input, &mut *OUTPUT_SLOT.0.get()) }
}

#[cfg(not(test))]
fn read_output() -> SessionInputEventV1 {
    // SAFETY:
    // 1. Invariant: this reads the probe's one completed syscall output record.
    // 2. Established by: the preceding write or synchronous syscall completes first.
    // 3. Lifetime: the static outlives the returned by-value copy.
    // 4. Pointer ownership: this creates only a short shared read.
    // 5. Alignment: `OutputSlot` is explicitly aligned for this record.
    // 6. Mapped length: exactly one initialized record is copied.
    // 7. Concurrency: no concurrent writer exists in this single-threaded probe.
    // 8. Violation: a racing writer could make the copied record inconsistent.
    unsafe { *OUTPUT_SLOT.0.get() }
}

#[cfg(not(test))]
fn error(console: PackedCapability) -> ! {
    syscalls::write_str(console, "PYTHOS:SESSION_INPUT_PROBE:ERROR\r\n");
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
fn main() {}
