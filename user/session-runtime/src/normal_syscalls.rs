//! Direct normal-session syscall adapters with typed result classification.

use core::{arch::asm, mem::size_of};
use pythos_shared::{
    capability_abi::PackedCapability,
    normal_session_abi::{
        NormalSessionReturnReason, SESSION_WAIT_READY_MASK, SYSCALL_SESSION_WAIT,
    },
    object_shell_abi::{
        NO_BYTE, SYSCALL_CONSOLE_READ_BYTE, SYSCALL_CONSOLE_WRITE_BYTE, SYSCALL_OK,
    },
    session_input_abi::{
        SESSION_INPUT_RESULT_EMPTY, SESSION_INPUT_RESULT_EVENT, SYSCALL_SESSION_INPUT_TRY_READ,
        SessionInputEventV1,
    },
    session_viewing_abi::SYSCALL_SESSION_VIEWING_PRESENT,
    viewing::ViewingSnapshot,
};

fn syscall5(number: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
    let result: u64;
    // SAFETY:
    // 1. Invariant: the number and five arguments use the shared PythCore syscall ABI.
    // 2. Established by: the fixed wrappers below and shared ABI constants.
    // 3. Lifetime: any pointer argument remains live until this synchronous syscall returns.
    // 4. Pointer ownership: callers retain their buffers; PythCore borrows only during dispatch.
    // 5. Alignment: the only pointer is a naturally aligned SessionInputEventV1 output record.
    // 6. Mapped length: input copy-out advertises exactly the complete 40-byte record.
    // 7. Concurrency: the normal session has one ring-3 thread and one syscall in flight.
    // 8. Violation: PythCore rejects invalid arguments or contains the faulting process.
    unsafe {
        asm!(
            "syscall",
            inout("rax") number => result,
            inout("rdi") arg1 => _,
            inout("rsi") arg2 => _,
            inout("rdx") arg3 => _,
            inout("r10") arg4 => _,
            inout("r8") arg5 => _,
            lateout("r9") _,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

pub fn try_input(
    capability: PackedCapability,
    output: &mut SessionInputEventV1,
) -> Result<Option<SessionInputEventV1>, NormalSessionReturnReason> {
    *output = SessionInputEventV1::empty();
    let result = syscall5(
        SYSCALL_SESSION_INPUT_TRY_READ,
        capability.raw(),
        output as *mut SessionInputEventV1 as u64,
        size_of::<SessionInputEventV1>() as u64,
        0,
        0,
    );
    if classify_input(result)? {
        Ok(Some(*output))
    } else {
        Ok(None)
    }
}

pub fn try_console(capability: PackedCapability) -> Result<Option<u8>, NormalSessionReturnReason> {
    classify_console_read(syscall5(
        SYSCALL_CONSOLE_READ_BYTE,
        capability.raw(),
        0,
        0,
        0,
        0,
    ))
}

pub fn wait(
    input: PackedCapability,
    console: PackedCapability,
) -> Result<u64, NormalSessionReturnReason> {
    classify_wait(syscall5(
        SYSCALL_SESSION_WAIT,
        input.raw(),
        console.raw(),
        0,
        0,
        0,
    ))
}

pub fn present(
    capability: PackedCapability,
    revision: u64,
    snapshot: ViewingSnapshot,
) -> Result<(), NormalSessionReturnReason> {
    let (flags, coordinates) = match snapshot.focus_mark {
        None => (0, 0),
        Some(position) => (1, u64::from(position.x) | (u64::from(position.y) << 32)),
    };
    classify_effect(
        syscall5(
            SYSCALL_SESSION_VIEWING_PRESENT,
            capability.raw(),
            revision,
            flags,
            coordinates,
            0,
        ),
        NormalSessionReturnReason::Presentation,
    )
}

pub fn write_console(
    capability: PackedCapability,
    bytes: &[u8],
) -> Result<(), NormalSessionReturnReason> {
    for byte in bytes {
        classify_effect(
            syscall5(
                SYSCALL_CONSOLE_WRITE_BYTE,
                capability.raw(),
                u64::from(*byte),
                0,
                0,
                0,
            ),
            NormalSessionReturnReason::Console,
        )?;
    }
    Ok(())
}

fn classify_console_read(result: u64) -> Result<Option<u8>, NormalSessionReturnReason> {
    if result == NO_BYTE {
        Ok(None)
    } else if let Ok(byte) = u8::try_from(result) {
        Ok(Some(byte))
    } else {
        Err(NormalSessionReturnReason::Console)
    }
}

fn classify_input(result: u64) -> Result<bool, NormalSessionReturnReason> {
    match result {
        SESSION_INPUT_RESULT_EVENT => Ok(true),
        SESSION_INPUT_RESULT_EMPTY => Ok(false),
        _ => Err(NormalSessionReturnReason::Input),
    }
}

fn classify_effect(
    result: u64,
    reason: NormalSessionReturnReason,
) -> Result<(), NormalSessionReturnReason> {
    if result == SYSCALL_OK {
        Ok(())
    } else {
        Err(reason)
    }
}

fn classify_wait(result: u64) -> Result<u64, NormalSessionReturnReason> {
    if result & !SESSION_WAIT_READY_MASK == 0 {
        Ok(result)
    } else {
        Err(NormalSessionReturnReason::Input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::{
        normal_session_abi::NormalSessionReturnReason,
        object_shell_abi::{NO_BYTE, SYSCALL_OK},
        session_input_abi::{SESSION_INPUT_RESULT_EMPTY, SESSION_INPUT_RESULT_EVENT},
    };

    #[test]
    fn console_read_keeps_bytes_distinct_from_no_byte_and_error_words() {
        // Catches truncating a non-byte syscall result into a command byte.
        assert_eq!(classify_console_read(NO_BYTE), Ok(None));
        assert_eq!(classify_console_read(0), Ok(Some(0)));
        assert_eq!(classify_console_read(255), Ok(Some(255)));
        assert_eq!(
            classify_console_read(SYSCALL_OK),
            Err(NormalSessionReturnReason::Console)
        );
    }

    #[test]
    fn typed_syscall_results_reject_unknown_words() {
        // Catches accepting failed input, presentation, console-write, or wait syscalls.
        assert_eq!(classify_input(SESSION_INPUT_RESULT_EMPTY), Ok(false));
        assert_eq!(classify_input(SESSION_INPUT_RESULT_EVENT), Ok(true));
        assert_eq!(
            classify_input(u64::MAX - 1),
            Err(NormalSessionReturnReason::Input)
        );
        assert_eq!(
            classify_effect(SYSCALL_OK, NormalSessionReturnReason::Presentation),
            Ok(())
        );
        assert_eq!(
            classify_effect(0, NormalSessionReturnReason::Presentation),
            Err(NormalSessionReturnReason::Presentation)
        );
        for readiness in 0..=3 {
            assert_eq!(classify_wait(readiness), Ok(readiness));
        }
        assert_eq!(classify_wait(4), Err(NormalSessionReturnReason::Input));
    }
}
