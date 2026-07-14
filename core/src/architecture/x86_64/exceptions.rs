//! Allocation-free x86-64 exception diagnostics.

use crate::serial;
use core::arch::asm;

#[cfg(not(test))]
use core::arch::global_asm;

#[repr(C)]
pub struct ExceptionFrame {
    vector: u64,
    error_code: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

#[cfg(not(test))]
global_asm!(
    r#"
    .macro EXC_NOERR vector
    .global exception_stub_\vector
    exception_stub_\vector:
        push 0
        push \vector
        jmp exception_common
    .endm

    .macro EXC_ERR vector
    .global exception_stub_\vector
    exception_stub_\vector:
        push \vector
        jmp exception_common
    .endm

    EXC_NOERR 0
    EXC_NOERR 1
    EXC_NOERR 2
    EXC_NOERR 3
    EXC_NOERR 4
    EXC_NOERR 5
    EXC_NOERR 6
    EXC_NOERR 7
    EXC_ERR   8
    EXC_NOERR 9
    EXC_ERR   10
    EXC_ERR   11
    EXC_ERR   12
    EXC_ERR   13
    EXC_ERR   14
    EXC_NOERR 15
    EXC_NOERR 16
    EXC_ERR   17
    EXC_NOERR 18
    EXC_NOERR 19
    EXC_NOERR 20
    EXC_ERR   21
    EXC_NOERR 22
    EXC_NOERR 23
    EXC_NOERR 24
    EXC_NOERR 25
    EXC_NOERR 26
    EXC_NOERR 27
    EXC_NOERR 28
    EXC_ERR   29
    EXC_ERR   30
    EXC_NOERR 31

    .global exception_common
    exception_common:
        mov rdi, rsp
        call exception_handler
    1:
        hlt
        jmp 1b
    "#
);

#[cfg(not(test))]
unsafe extern "C" {
    fn exception_stub_0();
    fn exception_stub_1();
    fn exception_stub_2();
    fn exception_stub_3();
    fn exception_stub_4();
    fn exception_stub_5();
    fn exception_stub_6();
    fn exception_stub_7();
    fn exception_stub_8();
    fn exception_stub_9();
    fn exception_stub_10();
    fn exception_stub_11();
    fn exception_stub_12();
    fn exception_stub_13();
    fn exception_stub_14();
    fn exception_stub_15();
    fn exception_stub_16();
    fn exception_stub_17();
    fn exception_stub_18();
    fn exception_stub_19();
    fn exception_stub_20();
    fn exception_stub_21();
    fn exception_stub_22();
    fn exception_stub_23();
    fn exception_stub_24();
    fn exception_stub_25();
    fn exception_stub_26();
    fn exception_stub_27();
    fn exception_stub_28();
    fn exception_stub_29();
    fn exception_stub_30();
    fn exception_stub_31();
}

pub fn handler_for_vector(vector: usize) -> u64 {
    #[cfg(test)]
    {
        let _ = vector;
        panic_stub as *const () as usize as u64
    }

    #[cfg(not(test))]
    {
        match vector {
            0 => exception_stub_0 as *const () as usize as u64,
            1 => exception_stub_1 as *const () as usize as u64,
            2 => exception_stub_2 as *const () as usize as u64,
            3 => exception_stub_3 as *const () as usize as u64,
            4 => exception_stub_4 as *const () as usize as u64,
            5 => exception_stub_5 as *const () as usize as u64,
            6 => exception_stub_6 as *const () as usize as u64,
            7 => exception_stub_7 as *const () as usize as u64,
            8 => exception_stub_8 as *const () as usize as u64,
            9 => exception_stub_9 as *const () as usize as u64,
            10 => exception_stub_10 as *const () as usize as u64,
            11 => exception_stub_11 as *const () as usize as u64,
            12 => exception_stub_12 as *const () as usize as u64,
            13 => exception_stub_13 as *const () as usize as u64,
            14 => exception_stub_14 as *const () as usize as u64,
            15 => exception_stub_15 as *const () as usize as u64,
            16 => exception_stub_16 as *const () as usize as u64,
            17 => exception_stub_17 as *const () as usize as u64,
            18 => exception_stub_18 as *const () as usize as u64,
            19 => exception_stub_19 as *const () as usize as u64,
            20 => exception_stub_20 as *const () as usize as u64,
            21 => exception_stub_21 as *const () as usize as u64,
            22 => exception_stub_22 as *const () as usize as u64,
            23 => exception_stub_23 as *const () as usize as u64,
            24 => exception_stub_24 as *const () as usize as u64,
            25 => exception_stub_25 as *const () as usize as u64,
            26 => exception_stub_26 as *const () as usize as u64,
            27 => exception_stub_27 as *const () as usize as u64,
            28 => exception_stub_28 as *const () as usize as u64,
            29 => exception_stub_29 as *const () as usize as u64,
            30 => exception_stub_30 as *const () as usize as u64,
            31 => exception_stub_31 as *const () as usize as u64,
            _ => panic_stub as *const () as usize as u64,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn exception_handler(frame: &ExceptionFrame) -> ! {
    serial::write_line("PYTHOS:EXCEPTION");
    serial::write_hex_u64("vector=", frame.vector);
    serial::write_hex_u64("error_code=", frame.error_code);
    serial::write_hex_u64("rip=", frame.rip);
    serial::write_hex_u64("cs=", frame.cs);
    serial::write_hex_u64("rflags=", frame.rflags);
    serial::write_hex_u64("rsp=", frame.rsp);
    serial::write_hex_u64("ss=", frame.ss);
    if frame.vector == 14 {
        serial::write_hex_u64("cr2=", read_cr2());
    }
    serial::write_hex_u64("cr3=", read_cr3());
    serial::write_line("PYTHOS:PANIC");
    halt();
}

pub extern "C" fn panic_stub() -> ! {
    serial::write_line("PYTHOS:PANIC");
    halt();
}

fn read_cr2() -> u64 {
    let cr2: u64;
    // SAFETY:
    // 1. Invariant: reading CR2 is valid in x86-64 ring 0.
    // 2. Established by: PythCore executes as the native kernel.
    // 3. Lifetime: this instruction has no borrowed memory lifetime.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable.
    // 6. Mapped length: not applicable.
    // 7. Concurrency: single-core exception handling with interrupts disabled.
    // 8. Violation: outside ring 0 this instruction would fault.
    unsafe {
        asm!("mov {out}, cr2", out = out(reg) cr2, options(nomem, nostack, preserves_flags));
    }
    cr2
}

fn read_cr3() -> u64 {
    let cr3: u64;
    // SAFETY:
    // 1. Invariant: reading CR3 is valid in x86-64 ring 0.
    // 2. Established by: PythCore executes as the native kernel.
    // 3. Lifetime: this instruction has no borrowed memory lifetime.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable.
    // 6. Mapped length: not applicable.
    // 7. Concurrency: single-core exception handling with interrupts disabled.
    // 8. Violation: outside ring 0 this instruction would fault.
    unsafe {
        asm!("mov {out}, cr3", out = out(reg) cr3, options(nomem, nostack, preserves_flags));
    }
    cr3
}

fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_frame_layout_starts_with_vector_and_error_code() {
        assert_eq!(core::mem::offset_of!(ExceptionFrame, vector), 0);
        assert_eq!(core::mem::offset_of!(ExceptionFrame, error_code), 8);
        assert_eq!(core::mem::offset_of!(ExceptionFrame, rip), 16);
    }
}
