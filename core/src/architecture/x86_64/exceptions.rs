//! Allocation-free x86-64 exception diagnostics.

use crate::{qemu_exit, serial};
use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(not(test))]
use core::arch::global_asm;

#[repr(C)]
pub struct ExceptionFrame {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
    vector: u64,
    error_code: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

const BREAKPOINT_VECTOR: u64 = 3;
const PAGE_FAULT_VECTOR: u64 = 14;

static EXPECTED_BREAKPOINT_RECOVERY_RIP: AtomicU64 = AtomicU64::new(0);
static EXPECTED_PAGE_FAULT_ADDR: AtomicU64 = AtomicU64::new(0);
static EXPECTED_PAGE_FAULT_RECOVERY_RIP: AtomicU64 = AtomicU64::new(0);

#[cfg(not(test))]
extern "C" fn expect_breakpoint(recovery_rip: u64) {
    EXPECTED_BREAKPOINT_RECOVERY_RIP.store(recovery_rip, Ordering::SeqCst);
}

#[cfg_attr(test, allow(dead_code))]
pub fn expect_page_fault(address: u64, recovery_rip: u64) {
    EXPECTED_PAGE_FAULT_ADDR.store(address, Ordering::SeqCst);
    EXPECTED_PAGE_FAULT_RECOVERY_RIP.store(recovery_rip, Ordering::SeqCst);
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
        push rax
        push rbx
        push rcx
        push rdx
        push rbp
        push rdi
        push rsi
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15
        mov rdi, rsp
        mov r12, rsp
        and rsp, -16
        call exception_handler
        mov rsp, r12
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rsi
        pop rdi
        pop rbp
        pop rdx
        pop rcx
        pop rbx
        pop rax
        add rsp, 16
        iretq

    .global exception_entry_self_test
    exception_entry_self_test:
        push rbx
        push rbp
        push r12
        push r13
        push r14
        push r15
        lea rdi, [rip + 1f]
        sub rsp, 8
        call expect_breakpoint_abi
        add rsp, 8

        mov rax, 0x101
        mov rbx, 0x202
        mov rcx, 0x303
        mov rdx, 0x404
        mov rbp, 0x505
        mov rdi, 0x606
        mov rsi, 0x707
        mov r8,  0x808
        mov r9,  0x909
        mov r10, 0xa0a
        mov r11, 0xb0b
        mov r12, 0xc0c
        mov r13, 0xd0d
        mov r14, 0xe0e
        mov r15, 0xf0f
        int3
    1:
        cmp rax, 0x101
        jne 2f
        cmp rbx, 0x202
        jne 2f
        cmp rcx, 0x303
        jne 2f
        cmp rdx, 0x404
        jne 2f
        cmp rbp, 0x505
        jne 2f
        cmp rdi, 0x606
        jne 2f
        cmp rsi, 0x707
        jne 2f
        cmp r8,  0x808
        jne 2f
        cmp r9,  0x909
        jne 2f
        cmp r10, 0xa0a
        jne 2f
        cmp r11, 0xb0b
        jne 2f
        cmp r12, 0xc0c
        jne 2f
        cmp r13, 0xd0d
        jne 2f
        cmp r14, 0xe0e
        jne 2f
        cmp r15, 0xf0f
        jne 2f
        mov rax, 1
        jmp 3f
    2:
        xor rax, rax
    3:
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbp
        pop rbx
        ret
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
    fn exception_entry_self_test() -> u64;
}

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn expect_breakpoint_abi(recovery_rip: u64) {
    expect_breakpoint(recovery_rip);
}

#[cfg(not(test))]
pub fn verify_entry_hardening() -> bool {
    // SAFETY:
    // 1. Invariant: exception stubs are installed in the IDT before this probe runs.
    // 2. Established by: `idt::initialize()` completing before the caller invokes this.
    // 3. Lifetime: the probe owns no borrowed memory and returns before boot continues.
    // 4. Pointer ownership: the recovery RIP is an internal assembly label only.
    // 5. Alignment: the probe aligns its Rust ABI call and does not alter `RSP`.
    // 6. Mapped length: the probe, exception stubs, and bootstrap stack are mapped.
    // 7. Concurrency: boot remains single-core with maskable interrupts disabled.
    // 8. Violation: a broken IDT or unmapped stack faults into the panic path.
    unsafe {
        exception_entry_self_test() == 1
            && EXPECTED_BREAKPOINT_RECOVERY_RIP.load(Ordering::SeqCst) == 0
    }
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
pub extern "C" fn exception_handler(frame: &mut ExceptionFrame) {
    if frame.vector == BREAKPOINT_VECTOR
        && crate::user_mode::handle_user_breakpoint(frame.cs, frame.ss)
    {
        return;
    }
    let fault_address = if frame.vector == PAGE_FAULT_VECTOR {
        read_cr2()
    } else {
        0
    };
    if crate::user_mode::handle_user_fault(
        frame.vector,
        frame.cs,
        frame.ss,
        frame.rip,
        frame.rsp,
        fault_address,
    ) {
        return;
    }
    if frame.vector == BREAKPOINT_VECTOR && handle_expected_breakpoint(frame) {
        return;
    }
    if frame.vector == PAGE_FAULT_VECTOR && handle_expected_page_fault(frame) {
        return;
    }

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
    qemu_exit::panic();
}

pub extern "C" fn panic_stub() -> ! {
    serial::write_line("PYTHOS:PANIC");
    qemu_exit::panic();
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

fn handle_expected_page_fault(frame: &mut ExceptionFrame) -> bool {
    let cr2 = read_cr2();
    let expected = EXPECTED_PAGE_FAULT_ADDR.load(Ordering::SeqCst);
    let recovery = EXPECTED_PAGE_FAULT_RECOVERY_RIP.load(Ordering::SeqCst);
    if expected == 0 || recovery == 0 || cr2 != expected {
        return false;
    }
    EXPECTED_PAGE_FAULT_ADDR.store(0, Ordering::SeqCst);
    EXPECTED_PAGE_FAULT_RECOVERY_RIP.store(0, Ordering::SeqCst);
    frame.rip = recovery;
    serial::write_line("PYTHOS:CORE:EXPECTED_PAGE_FAULT");
    serial::write_hex_u64("cr2=", cr2);
    true
}

fn handle_expected_breakpoint(frame: &mut ExceptionFrame) -> bool {
    let recovery = EXPECTED_BREAKPOINT_RECOVERY_RIP.load(Ordering::SeqCst);
    if recovery == 0 {
        return false;
    }
    EXPECTED_BREAKPOINT_RECOVERY_RIP.store(0, Ordering::SeqCst);
    frame.rip = recovery;
    true
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_frame_layout_preserves_registers_before_cpu_frame() {
        assert_eq!(core::mem::offset_of!(ExceptionFrame, r15), 0);
        assert_eq!(core::mem::offset_of!(ExceptionFrame, rax), 112);
        assert_eq!(core::mem::offset_of!(ExceptionFrame, vector), 120);
        assert_eq!(core::mem::offset_of!(ExceptionFrame, error_code), 128);
        assert_eq!(core::mem::offset_of!(ExceptionFrame, rip), 136);
    }
}
