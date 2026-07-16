//! Minimal Phase 8 ring-3 execution proof.
//!
//! This is not the syscall ABI. It is a one-shot proof that PythCore can enter
//! CPL3 code, take a user-originated trap, and resume kernel execution.
#![cfg_attr(test, allow(dead_code, unused_imports))]

use crate::architecture::x86_64::gdt;
#[cfg(not(test))]
use crate::{architecture::x86_64::tss, serial};
#[cfg(not(test))]
use core::arch::{asm, global_asm};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

const USER_PAGE_SIZE: usize = 4096;
const KERNEL_TRAP_STACK_SIZE: usize = 16 * 4096;
const USER_CODE_BREAKPOINT_OFFSET: usize = 0;
const USER_CODE_SYSCALL_OFFSET: usize = 16;
const USER_CODE_BREAKPOINT: u8 = 0xCC;
const USER_CODE_HALT: u8 = 0xF4;
const USER_CODE_SYSCALL_PREFIX: u8 = 0x0F;
const USER_CODE_SYSCALL_OPCODE: u8 = 0x05;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserModeError {
    DidNotReturn,
}

#[repr(align(4096))]
struct UserCodePage([u8; USER_PAGE_SIZE]);

#[repr(align(4096))]
struct UserDataPage(UnsafeCell<[u8; USER_PAGE_SIZE]>);

#[repr(align(16))]
struct KernelTrapStack(UnsafeCell<[u8; KERNEL_TRAP_STACK_SIZE]>);

// SAFETY:
// 1. Invariant: the user stack page is private to the single ring-3 proof.
// 2. Established by: Phase 8 has no concurrent user tasks yet.
// 3. Lifetime: the page is static for all of PythCore.
// 4. Pointer ownership: this module owns the backing storage.
// 5. Alignment: the wrapper gives page alignment.
// 6. Mapped length: exactly `USER_PAGE_SIZE` bytes.
// 7. Concurrency: single-core boot with one user proof at a time.
// 8. Violation: concurrent mutation would corrupt the proof stack.
unsafe impl Sync for UserDataPage {}

// SAFETY:
// 1. Invariant: the trap stack is used only as TSS.RSP0 for the one-shot
//    user-to-kernel transition in this slice.
// 2. Established by: `run_self_test` writes TSS.RSP0 immediately before entry.
// 3. Lifetime: the stack is static for all of PythCore.
// 4. Pointer ownership: the CPU owns pushes while handling the trap; this
//    module restores the original kernel stack before resuming.
// 5. Alignment: the wrapper gives 16-byte stack alignment.
// 6. Mapped length: exactly `KERNEL_TRAP_STACK_SIZE` bytes.
// 7. Concurrency: single-core proof, no nested user transitions.
// 8. Violation: an undersized or aliased stack would corrupt kernel state.
unsafe impl Sync for KernelTrapStack {}

static USER_CODE_PAGE: UserCodePage = UserCodePage(user_code_bytes());
static USER_STACK_PAGE: UserDataPage = UserDataPage(UnsafeCell::new([0; USER_PAGE_SIZE]));
static KERNEL_TRAP_STACK: KernelTrapStack =
    KernelTrapStack(UnsafeCell::new([0; KERNEL_TRAP_STACK_SIZE]));

static EXPECTED_USER_BREAKPOINT: AtomicBool = AtomicBool::new(false);
static USER_RETURNED: AtomicBool = AtomicBool::new(false);
static KERNEL_RECOVERY_RIP: AtomicU64 = AtomicU64::new(0);
static KERNEL_RECOVERY_RSP: AtomicU64 = AtomicU64::new(0);

const fn user_code_bytes() -> [u8; USER_PAGE_SIZE] {
    let mut bytes = [0; USER_PAGE_SIZE];
    bytes[USER_CODE_BREAKPOINT_OFFSET] = USER_CODE_BREAKPOINT;
    bytes[USER_CODE_BREAKPOINT_OFFSET + 1] = USER_CODE_HALT;

    let syscall_number = crate::syscall::SYSCALL_SYSTEM_LOG_PROOF as u32;
    bytes[USER_CODE_SYSCALL_OFFSET] = 0xB8;
    bytes[USER_CODE_SYSCALL_OFFSET + 1] = syscall_number as u8;
    bytes[USER_CODE_SYSCALL_OFFSET + 2] = (syscall_number >> 8) as u8;
    bytes[USER_CODE_SYSCALL_OFFSET + 3] = (syscall_number >> 16) as u8;
    bytes[USER_CODE_SYSCALL_OFFSET + 4] = (syscall_number >> 24) as u8;
    bytes[USER_CODE_SYSCALL_OFFSET + 5] = USER_CODE_SYSCALL_PREFIX;
    bytes[USER_CODE_SYSCALL_OFFSET + 6] = USER_CODE_SYSCALL_OPCODE;
    bytes[USER_CODE_SYSCALL_OFFSET + 7] = USER_CODE_BREAKPOINT;
    bytes[USER_CODE_SYSCALL_OFFSET + 8] = USER_CODE_HALT;
    bytes
}

#[cfg(not(test))]
global_asm!(
    r#"
    .global ring3_enter_abi
    ring3_enter_abi:
        push rbx
        push r12
        push r13
        mov rbx, rdi
        mov r12, rsi
        lea rdi, [rip + .Lring3_recovered]
        mov rsi, rsp
        call prepare_ring3_return_abi

        mov ax, 0x2B
        mov ds, ax
        mov es, ax
        push 0x2B
        push r12
        pushfq
        pop rax
        or rax, 0x200
        push rax
        push 0x33
        push rbx
        iretq

    .Lring3_recovered:
        mov ax, 0x10
        mov ds, ax
        mov es, ax
        mov eax, 1
        pop r13
        pop r12
        pop rbx
        ret
    "#
);

#[cfg(not(test))]
unsafe extern "C" {
    fn ring3_enter_abi(entry: u64, user_stack_top: u64) -> u64;
}

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn prepare_ring3_return_abi(recovery_rip: u64, kernel_rsp: u64) {
    USER_RETURNED.store(false, Ordering::SeqCst);
    KERNEL_RECOVERY_RIP.store(recovery_rip, Ordering::SeqCst);
    KERNEL_RECOVERY_RSP.store(kernel_rsp, Ordering::SeqCst);
    EXPECTED_USER_BREAKPOINT.store(true, Ordering::SeqCst);
}

#[cfg(not(test))]
pub fn run_self_test() -> Result<(), UserModeError> {
    run_user_entry(user_code_start())
}

#[cfg(not(test))]
pub fn run_syscall_test() -> Result<(), UserModeError> {
    run_user_entry(user_syscall_entry())
}

#[cfg(not(test))]
fn run_user_entry(entry: u64) -> Result<(), UserModeError> {
    tss::set_ring0_stack(kernel_trap_stack_top());
    serial::write_line("PYTHOS:CORE:USER_MODE:ENTER");
    // SAFETY:
    // 1. Invariant: user code and stack pages are mapped with user access, the
    //    GDT has DPL3 code/data descriptors, IDT vector 3 is callable from
    //    CPL3, and TSS.RSP0 points to a mapped kernel trap stack.
    // 2. Established by: GDT/IDT initialization, PythCore page-table build,
    //    and `tss::set_ring0_stack` immediately above.
    // 3. Lifetime: code, stack, recovery label, and trap stack are static or
    //    active until the one-shot proof returns.
    // 4. Pointer ownership: the CPU consumes the user entry/stack and trap
    //    stack; this module owns the backing storage.
    // 5. Alignment: user entry is page aligned; stacks are 16-byte aligned.
    // 6. Mapped length: one user code page, one user stack page, and a 16 KiB
    //    kernel trap stack are mapped.
    // 7. Concurrency: single-core boot, one ring-3 proof in flight.
    // 8. Violation: bad descriptors or mappings fault through diagnostics.
    let returned = unsafe { ring3_enter_abi(entry, user_stack_top()) };
    if returned == 1 && USER_RETURNED.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err(UserModeError::DidNotReturn)
    }
}

pub fn handle_user_breakpoint(cs: u64, ss: u64) -> bool {
    if !EXPECTED_USER_BREAKPOINT.load(Ordering::SeqCst) || !is_user_frame(cs, ss) {
        return false;
    }
    EXPECTED_USER_BREAKPOINT.store(false, Ordering::SeqCst);
    USER_RETURNED.store(true, Ordering::SeqCst);
    #[cfg(not(test))]
    {
        serial::write_line("PYTHOS:CORE:USER_MODE:RETURN");
        let recovery_rip = KERNEL_RECOVERY_RIP.load(Ordering::SeqCst);
        let recovery_rsp = KERNEL_RECOVERY_RSP.load(Ordering::SeqCst);
        jump_to_kernel_recovery(recovery_rip, recovery_rsp);
    }
    #[cfg(test)]
    {
        true
    }
}

pub fn user_code_region() -> (u64, u64) {
    (user_code_start(), USER_PAGE_SIZE as u64)
}

pub fn user_stack_region() -> (u64, u64) {
    (user_stack_start(), USER_PAGE_SIZE as u64)
}

pub fn user_code_start() -> u64 {
    USER_CODE_PAGE.0.as_ptr() as u64
}

fn user_stack_start() -> u64 {
    USER_STACK_PAGE.0.get().cast::<u8>() as u64
}

fn user_syscall_entry() -> u64 {
    user_code_start() + USER_CODE_SYSCALL_OFFSET as u64
}

#[cfg(not(test))]
fn user_stack_top() -> u64 {
    user_stack_start() + USER_PAGE_SIZE as u64 - 16
}

#[cfg(not(test))]
fn kernel_trap_stack_top() -> u64 {
    KERNEL_TRAP_STACK.0.get().cast::<u8>() as u64 + KERNEL_TRAP_STACK_SIZE as u64
}

fn is_user_frame(cs: u64, ss: u64) -> bool {
    cs == u64::from(gdt::USER_CODE_SELECTOR) && ss == u64::from(gdt::USER_DATA_SELECTOR)
}

#[cfg(not(test))]
fn jump_to_kernel_recovery(recovery_rip: u64, recovery_rsp: u64) -> ! {
    // SAFETY:
    // 1. Invariant: `recovery_rsp` is the saved stack pointer inside
    //    `ring3_enter_abi`, and `recovery_rip` is its internal recovery label.
    // 2. Established by: `prepare_ring3_return_abi` was called by
    //    `ring3_enter_abi` immediately before `iretq` to user mode.
    // 3. Lifetime: the assembly frame remains live until this jump resumes it.
    // 4. Pointer ownership: this restores the kernel-owned stack to its saved
    //    position; no Rust references survive across the jump.
    // 5. Alignment: the saved stack pointer is the aligned assembly frame.
    // 6. Mapped length: the saved frame covers three pushed registers and the
    //    caller return address.
    // 7. Concurrency: single-core one-shot ring-3 proof.
    // 8. Violation: a bad RIP/RSP would jump to unmapped code or corrupt stack.
    unsafe {
        asm!(
            "mov rsp, {recovery_rsp}",
            "jmp {recovery_rip}",
            recovery_rsp = in(reg) recovery_rsp,
            recovery_rip = in(reg) recovery_rip,
            options(noreturn)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_code_page_starts_with_breakpoint_then_halt() {
        assert_eq!(USER_CODE_PAGE.0[0], USER_CODE_BREAKPOINT);
        assert_eq!(USER_CODE_PAGE.0[1], USER_CODE_HALT);
    }

    #[test]
    fn user_code_page_contains_syscall_probe_entry() {
        assert_eq!(USER_CODE_PAGE.0[USER_CODE_SYSCALL_OFFSET], 0xB8);
        assert_eq!(
            USER_CODE_PAGE.0[USER_CODE_SYSCALL_OFFSET + 5],
            USER_CODE_SYSCALL_PREFIX
        );
        assert_eq!(
            USER_CODE_PAGE.0[USER_CODE_SYSCALL_OFFSET + 6],
            USER_CODE_SYSCALL_OPCODE
        );
        assert_eq!(
            USER_CODE_PAGE.0[USER_CODE_SYSCALL_OFFSET + 7],
            USER_CODE_BREAKPOINT
        );
    }

    #[test]
    fn user_regions_are_page_sized_and_aligned() {
        let (code_start, code_len) = user_code_region();
        let (stack_start, stack_len) = user_stack_region();

        assert_eq!(code_len, USER_PAGE_SIZE as u64);
        assert_eq!(stack_len, USER_PAGE_SIZE as u64);
        assert_eq!(code_start % USER_PAGE_SIZE as u64, 0);
        assert_eq!(stack_start % USER_PAGE_SIZE as u64, 0);
    }

    #[test]
    fn user_frame_requires_ring3_code_and_stack_selectors() {
        assert!(is_user_frame(
            u64::from(gdt::USER_CODE_SELECTOR),
            u64::from(gdt::USER_DATA_SELECTOR)
        ));
        assert!(!is_user_frame(
            u64::from(gdt::KERNEL_CODE_SELECTOR),
            u64::from(gdt::USER_DATA_SELECTOR)
        ));
    }
}
