//! Phase 8 syscall-entry proof.
//!
//! This defines the first syscall ABI contract. It intentionally accepts no
//! user pointers yet; Phase 8 copy-in/copy-out belongs to later slices.
#![cfg_attr(test, allow(dead_code, unused_imports))]

use crate::architecture::x86_64::gdt;
use crate::capabilities::{CapabilityError, CapabilityTable, ResourceId, RightsMask};
use crate::ipc_channels::{IpcChannel, IpcError, IpcMessage};
use crate::permission_validation::{self, PermissionError};
#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityTable};
use crate::system_api::{SystemApiError, SystemApiHost};
use crate::tasks::TaskId;
use crate::user_mode;
use crate::value_validation::{HostCallResult, UntrustedRuntimeValue};
#[cfg(not(test))]
use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub const SYSCALL_SYSTEM_LOG_PROOF: u64 = 0x5059_0001;

const SYSCALL_OK: u64 = 0x5059_004F;
const SYSCALL_ERROR_BAD_NUMBER: u64 = 0xBAD0_0001;
const SYSCALL_ERROR_DISPATCH: u64 = 0xBAD0_0002;
const SYSCALL_ERROR_UNEXPECTED: u64 = 0xBAD0_0003;

const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;
const EFER_SYSCALL_ENABLE: u64 = 1 << 0;
const RFLAGS_INTERRUPT_ENABLE: u64 = 1 << 9;
const RFLAGS_DIRECTION: u64 = 1 << 10;
const SYSCALL_RFLAGS_MASK: u64 = RFLAGS_INTERRUPT_ENABLE | RFLAGS_DIRECTION;

const IPC_SYSCALL_RESOURCE: ResourceId = ResourceId::new(0x5359_5343_4950_4300);
const SYSCALL_MESSAGE_TYPE: u16 = 0x88;
const SYSCALL_PAYLOAD: [u8; 4] = [0x53, 0x43, 0x41, 0x4C];
const SYSCALL_LOG_MESSAGE: &[u8] = b"PythOS [HISS] We Are Woken";

static EXPECTED_SYSCALL: AtomicBool = AtomicBool::new(false);
static SYSCALL_RETURNED: AtomicBool = AtomicBool::new(false);
static SYSCALL_LAST_RESULT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyscallError {
    BadNumber,
    Capability(CapabilityError),
    Ipc(IpcError),
    Permission(PermissionError),
    System(SystemApiError),
    UnexpectedSyscall,
    UserMode(user_mode::UserModeError),
    DidNotReturn,
    BadResult,
}

impl From<CapabilityError> for SyscallError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

impl From<IpcError> for SyscallError {
    fn from(error: IpcError) -> Self {
        Self::Ipc(error)
    }
}

impl From<PermissionError> for SyscallError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl From<SystemApiError> for SyscallError {
    fn from(error: SystemApiError) -> Self {
        Self::System(error)
    }
}

impl From<user_mode::UserModeError> for SyscallError {
    fn from(error: user_mode::UserModeError) -> Self {
        Self::UserMode(error)
    }
}

#[cfg(not(test))]
global_asm!(
    r#"
    .section .bss
    .balign 16
    syscall_kernel_stack:
        .zero 16384
    syscall_kernel_stack_end:
    .balign 8
    syscall_saved_user_rsp:
        .quad 0

    .section .text
    .global syscall_entry_abi
    syscall_entry_abi:
        mov qword ptr [rip + syscall_saved_user_rsp], rsp
        lea rsp, [rip + syscall_kernel_stack_end]
        push rcx
        push r11
        push rbx
        push rbp
        push r12
        push r13
        push r14
        push r15
        cld
        mov rdi, rax
        call syscall_dispatch_abi
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbp
        pop rbx
        pop r11
        pop rcx
        mov rsp, qword ptr [rip + syscall_saved_user_rsp]
        sysretq
    "#
);

#[cfg(not(test))]
unsafe extern "C" {
    fn syscall_entry_abi();
}

#[cfg(not(test))]
pub fn run_self_test() -> Result<(), SyscallError> {
    configure_gate();
    serial::write_line("PYTHOS:CORE:SYSCALL:MSRS_READY");
    EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
    SYSCALL_RETURNED.store(false, Ordering::SeqCst);
    SYSCALL_LAST_RESULT.store(0, Ordering::SeqCst);

    user_mode::run_syscall_test()?;

    if !SYSCALL_RETURNED.load(Ordering::SeqCst) {
        return Err(SyscallError::DidNotReturn);
    }
    if SYSCALL_LAST_RESULT.load(Ordering::SeqCst) != SYSCALL_OK {
        return Err(SyscallError::BadResult);
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch_abi(number: u64) -> u64 {
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:SYSCALL:ENTER");

    let result = dispatch(number);
    let code = match result {
        Ok(()) => SYSCALL_OK,
        Err(SyscallError::BadNumber) => SYSCALL_ERROR_BAD_NUMBER,
        Err(SyscallError::UnexpectedSyscall) => SYSCALL_ERROR_UNEXPECTED,
        Err(_) => SYSCALL_ERROR_DISPATCH,
    };
    SYSCALL_LAST_RESULT.store(code, Ordering::SeqCst);
    SYSCALL_RETURNED.store(true, Ordering::SeqCst);

    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:SYSCALL:RETURN");
    code
}

fn dispatch(number: u64) -> Result<(), SyscallError> {
    if !EXPECTED_SYSCALL.swap(false, Ordering::SeqCst) {
        return Err(SyscallError::UnexpectedSyscall);
    }
    if number != SYSCALL_SYSTEM_LOG_PROOF {
        return Err(SyscallError::BadNumber);
    }

    run_capability_gated_ipc_bridge()?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK");

    run_system_log_bridge()?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:SYSCALL:SYSTEM_LOG");
    Ok(())
}

fn run_capability_gated_ipc_bridge() -> Result<(), SyscallError> {
    let mut identities = ServiceIdentityTable::new();
    let caller = service(&mut identities, 80)?;
    let receiver = service(&mut identities, 81)?;
    let mut table = CapabilityTable::new();
    let handle = table.grant(
        caller,
        IPC_SYSCALL_RESOURCE,
        RightsMask::new(RightsMask::SEND),
    )?;
    let mut channel = IpcChannel::new(caller, receiver);
    let message = IpcMessage::new(SYSCALL_MESSAGE_TYPE, &SYSCALL_PAYLOAD)?;

    permission_validation::send_with_capability(
        &table,
        caller,
        handle,
        IPC_SYSCALL_RESOURCE,
        &mut channel,
        receiver,
        message,
    )?;
    if channel.receive(receiver)? != message {
        return Err(SyscallError::Ipc(IpcError::PayloadCorrupt));
    }
    Ok(())
}

fn run_system_log_bridge() -> Result<(), SyscallError> {
    let mut identities = ServiceIdentityTable::new();
    let runtime = service(&mut identities, 82)?;
    let mut host = SystemApiHost::new();
    let handle = host.grant_log(runtime)?;

    match host.log(
        runtime,
        handle,
        UntrustedRuntimeValue::StringBytes(SYSCALL_LOG_MESSAGE),
    )? {
        HostCallResult::Returned => Ok(()),
        HostCallResult::Rejected(error) => Err(SyscallError::System(SystemApiError::Value(error))),
    }
}

fn service(
    identities: &mut ServiceIdentityTable,
    task_id: u64,
) -> Result<ServiceId, CapabilityError> {
    identities
        .register_task(TaskId::new(task_id))
        .map_err(|_| CapabilityError::InvalidHandle)
}

#[cfg(not(test))]
fn configure_gate() {
    let efer = read_msr(IA32_EFER);
    write_msr(IA32_EFER, efer | EFER_SYSCALL_ENABLE);
    write_msr(IA32_STAR, syscall_star_value());
    write_msr(IA32_LSTAR, syscall_entry_abi as *const () as u64);
    write_msr(IA32_FMASK, SYSCALL_RFLAGS_MASK);
}

fn syscall_star_value() -> u64 {
    let kernel_selector = u64::from(gdt::KERNEL_CODE_SELECTOR);
    let user_selector_base = u64::from(gdt::USER_DATA_SELECTOR - 8);
    (user_selector_base << 48) | (kernel_selector << 32)
}

#[cfg(not(test))]
fn read_msr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY:
    // 1. Invariant: `msr` names an architectural x86-64 model-specific
    //    register used for syscall setup.
    // 2. Established by: callers pass only IA32_EFER/STAR/LSTAR/FMASK constants.
    // 3. Lifetime: the instruction has no borrowed memory lifetime.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable.
    // 6. Mapped length: not applicable.
    // 7. Concurrency: Phase 8 boot remains single-core during setup.
    // 8. Violation: reading an invalid MSR causes a general-protection fault.
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    u64::from(low) | (u64::from(high) << 32)
}

#[cfg(not(test))]
fn write_msr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    // SAFETY:
    // 1. Invariant: `msr` names an architectural syscall MSR and `value`
    //    encodes selectors, handler address, or flag mask per ADR 0028.
    // 2. Established by: `configure_gate` computes values from loaded GDT
    //    selectors and the mapped `syscall_entry_abi` symbol.
    // 3. Lifetime: MSR state remains active for the Phase 8 syscall proof.
    // 4. Pointer ownership: LSTAR borrows executable PythCore text; other MSRs
    //    carry integer configuration.
    // 5. Alignment: LSTAR is a canonical function address; other values are
    //    CPU-defined bitfields.
    // 6. Mapped length: the handler text page remains mapped in kernel and
    //    user proof roots.
    // 7. Concurrency: single-core setup with interrupts disabled.
    // 8. Violation: bad MSR state faults or returns to the wrong privilege
    //    context during the proof.
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") low,
            in("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syscall_star_value_selects_kernel_and_ring3_segments() {
        assert_eq!(
            (syscall_star_value() >> 32) & 0xFFFF,
            u64::from(gdt::KERNEL_CODE_SELECTOR)
        );
        assert_eq!(
            (syscall_star_value() >> 48) & 0xFFFF,
            u64::from(gdt::USER_DATA_SELECTOR - 8)
        );
        assert_eq!(
            ((syscall_star_value() >> 48) & 0xFFFF) + 16,
            u64::from(gdt::USER_CODE_SELECTOR)
        );
        assert_eq!(
            ((syscall_star_value() >> 48) & 0xFFFF) + 8,
            u64::from(gdt::USER_DATA_SELECTOR)
        );
    }

    #[test]
    fn dispatch_rejects_unexpected_or_unknown_syscalls() {
        EXPECTED_SYSCALL.store(false, Ordering::SeqCst);
        assert_eq!(
            dispatch(SYSCALL_SYSTEM_LOG_PROOF),
            Err(SyscallError::UnexpectedSyscall)
        );

        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(
            dispatch(SYSCALL_SYSTEM_LOG_PROOF + 1),
            Err(SyscallError::BadNumber)
        );
    }

    #[test]
    fn dispatch_system_log_proof_uses_capability_and_log_surfaces() {
        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(dispatch(SYSCALL_SYSTEM_LOG_PROOF), Ok(()));
    }
}
