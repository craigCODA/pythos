use core::{arch::asm, mem::size_of};
use pythos_shared::{
    object_shell_abi::{NO_BYTE, SYSCALL_CONSOLE_READ_BYTE, SYSCALL_CONSOLE_WRITE_BYTE},
    capability_abi::PackedCapability,
    session_input_abi::{SYSCALL_SESSION_INPUT_TRY_READ, SessionInputEventV1},
};

fn syscall5(number: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
    let result: u64;
    // SAFETY:
    // 1. Invariant: `number` and all five arguments use the PythCore syscall ABI.
    // 2. Established by: this module's fixed wrappers and shared ABI constants.
    // 3. Lifetime: pointer arguments remain live for this synchronous instruction.
    // 4. Pointer ownership: callers retain their buffers; PythCore only reads or writes them during dispatch.
    // 5. Alignment: `try_read` receives a naturally aligned `SessionInputEventV1`.
    // 6. Mapped length: `try_read` passes exactly its full 40-byte record length.
    // 7. Concurrency: this probe is single-threaded and issues one syscall at a time.
    // 8. Violation: invalid registers or memory are rejected by PythCore or may fault this process.
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

pub fn write_str(console: PackedCapability, text: &str) {
    for byte in text.bytes() {
        syscall5(SYSCALL_CONSOLE_WRITE_BYTE, console.raw(), u64::from(byte), 0, 0, 0);
    }
}

pub fn read_byte(console: PackedCapability) -> Option<u8> {
    let result = syscall5(SYSCALL_CONSOLE_READ_BYTE, console.raw(), 0, 0, 0, 0);
    if result == NO_BYTE { None } else { Some(result as u8) }
}

pub fn try_read(input: PackedCapability, output: &mut SessionInputEventV1) -> u64 {
    syscall5(
        SYSCALL_SESSION_INPUT_TRY_READ,
        input.raw(),
        output as *mut SessionInputEventV1 as u64,
        size_of::<SessionInputEventV1>() as u64,
        0,
        0,
    )
}
