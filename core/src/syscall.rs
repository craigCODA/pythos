//! Phase 8 syscall-entry proof and Phase 9 general syscall ABI registry.
//!
//! This defines the first syscall ABI contract. It intentionally accepts no
//! user pointers yet; Phase 9 copy-in/copy-out belongs to the next slice.
#![cfg_attr(test, allow(dead_code, unused_imports))]

use crate::architecture::x86_64::gdt;
use crate::capabilities::{
    CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask,
};
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

pub const SYSCALL_ABI_MAJOR: u16 = 1;
pub const SYSCALL_ABI_MINOR: u16 = 0;
pub const SYSCALL_ABI_INFO: u64 = 0x5059_0000;
pub const SYSCALL_SYSTEM_LOG_PROOF: u64 = 0x5059_0001;

const SYSCALL_ABI_INFO_MAGIC: u64 = 0x5059_0000_0000;
const SYSCALL_OK: u64 = 0x5059_004F;
const SYSCALL_ERROR_UNSUPPORTED_NUMBER: u64 = 0xBAD0_0001;
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
const HARDWARE_PORT_RESOURCE: ResourceId = ResourceId::new(0x4841_5244_504F_5254);
const SYSCALL_MESSAGE_TYPE: u16 = 0x88;
const SYSCALL_PAYLOAD: [u8; 4] = [0x53, 0x43, 0x41, 0x4C];
const BOUNDARY_MESSAGE_TYPE: u16 = 0x89;
const BOUNDARY_PAYLOAD: [u8; 4] = [0x42, 0x4F, 0x55, 0x4E];
const SYSCALL_LOG_MESSAGE: &[u8] = b"PythOS [HISS] We Are Woken";

static EXPECTED_SYSCALL: AtomicBool = AtomicBool::new(false);
static SYSCALL_RETURNED: AtomicBool = AtomicBool::new(false);
static SYSCALL_LAST_RESULT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyscallError {
    UnsupportedNumber,
    Capability(CapabilityError),
    Ipc(IpcError),
    Permission(PermissionError),
    System(SystemApiError),
    UnexpectedSyscall,
    UserMode(user_mode::UserModeError),
    DidNotReturn,
    BadResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundaryCapabilityProof {
    pub allowed_call: bool,
    pub forged_handle_denied: bool,
    pub direct_hardware_denied: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneralSyscallAbiProof {
    pub versioned: bool,
    pub known_dispatch: bool,
    pub unknown_denied: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyscallDispatchKind {
    AbiInfo,
    SystemLogProof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SyscallEntry {
    number: u64,
    name: &'static str,
    introduced_major: u16,
    introduced_minor: u16,
    dispatch_kind: SyscallDispatchKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyscallTableError {
    Empty,
    NotSortedOrDuplicate,
    InvalidIntroducedVersion,
}

const SYSCALL_TABLE: &[SyscallEntry] = &[
    SyscallEntry {
        number: SYSCALL_ABI_INFO,
        name: "SYSCALL_ABI_INFO",
        introduced_major: 1,
        introduced_minor: 0,
        dispatch_kind: SyscallDispatchKind::AbiInfo,
    },
    SyscallEntry {
        number: SYSCALL_SYSTEM_LOG_PROOF,
        name: "SYSCALL_SYSTEM_LOG_PROOF",
        introduced_major: 1,
        introduced_minor: 0,
        dispatch_kind: SyscallDispatchKind::SystemLogProof,
    },
];

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
        Ok(code) => code,
        Err(SyscallError::UnsupportedNumber) => SYSCALL_ERROR_UNSUPPORTED_NUMBER,
        Err(SyscallError::UnexpectedSyscall) => SYSCALL_ERROR_UNEXPECTED,
        Err(_) => SYSCALL_ERROR_DISPATCH,
    };
    SYSCALL_LAST_RESULT.store(code, Ordering::SeqCst);
    SYSCALL_RETURNED.store(true, Ordering::SeqCst);

    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:SYSCALL:RETURN");
    code
}

fn dispatch(number: u64) -> Result<u64, SyscallError> {
    if !EXPECTED_SYSCALL.swap(false, Ordering::SeqCst) {
        return Err(SyscallError::UnexpectedSyscall);
    }
    let entry = lookup_syscall(number).ok_or(SyscallError::UnsupportedNumber)?;

    match entry.dispatch_kind {
        SyscallDispatchKind::AbiInfo => Ok(abi_info_result()),
        SyscallDispatchKind::SystemLogProof => {
            run_capability_gated_ipc_bridge()?;
            #[cfg(not(test))]
            serial::write_line("PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK");

            run_system_log_bridge()?;
            #[cfg(not(test))]
            serial::write_line("PYTHOS:CORE:SYSCALL:SYSTEM_LOG");
            Ok(SYSCALL_OK)
        }
    }
}

fn abi_info_result() -> u64 {
    SYSCALL_ABI_INFO_MAGIC | (u64::from(SYSCALL_ABI_MAJOR) << 16) | u64::from(SYSCALL_ABI_MINOR)
}

fn lookup_syscall(number: u64) -> Option<&'static SyscallEntry> {
    SYSCALL_TABLE.iter().find(|entry| entry.number == number)
}

fn validate_syscall_table(table: &[SyscallEntry]) -> Result<(), SyscallTableError> {
    if table.is_empty() {
        return Err(SyscallTableError::Empty);
    }

    let mut previous = None;
    for entry in table {
        if entry.introduced_major == 0
            || entry.introduced_major > SYSCALL_ABI_MAJOR
            || (entry.introduced_major == SYSCALL_ABI_MAJOR
                && entry.introduced_minor > SYSCALL_ABI_MINOR)
        {
            return Err(SyscallTableError::InvalidIntroducedVersion);
        }
        if let Some(previous_number) = previous
            && entry.number <= previous_number
        {
            return Err(SyscallTableError::NotSortedOrDuplicate);
        }
        previous = Some(entry.number);
    }

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

pub fn run_boundary_capability_self_test() -> Result<BoundaryCapabilityProof, SyscallError> {
    let mut identities = ServiceIdentityTable::new();
    let caller = service(&mut identities, 83)?;
    let receiver = service(&mut identities, 84)?;
    let intruder = service(&mut identities, 85)?;
    let mut table = CapabilityTable::new();
    let handle = table.grant(
        caller,
        IPC_SYSCALL_RESOURCE,
        RightsMask::new(RightsMask::SEND),
    )?;
    let mut channel = IpcChannel::new(caller, receiver);
    let allowed_message = IpcMessage::new(BOUNDARY_MESSAGE_TYPE, &BOUNDARY_PAYLOAD)?;

    syscall_gate_send_with_capability(
        &table,
        caller,
        handle,
        IPC_SYSCALL_RESOURCE,
        &mut channel,
        receiver,
        allowed_message,
    )?;
    if channel.receive(receiver)? != allowed_message {
        return Err(SyscallError::Ipc(IpcError::PayloadCorrupt));
    }

    let forged_message = IpcMessage::new(BOUNDARY_MESSAGE_TYPE, &BOUNDARY_PAYLOAD)?;
    let forged_handle_denied = syscall_gate_send_with_capability(
        &table,
        intruder,
        handle,
        IPC_SYSCALL_RESOURCE,
        &mut channel,
        receiver,
        forged_message,
    ) == Err(SyscallError::Capability(CapabilityError::WrongHolder));
    if !forged_handle_denied {
        return Err(SyscallError::Capability(CapabilityError::WrongHolder));
    }
    if channel.receive(receiver) != Err(IpcError::QueueEmpty) {
        return Err(SyscallError::Ipc(IpcError::PayloadCorrupt));
    }

    let hardware_message = IpcMessage::new(BOUNDARY_MESSAGE_TYPE, &BOUNDARY_PAYLOAD)?;
    let direct_hardware_denied = syscall_gate_send_with_capability(
        &table,
        caller,
        handle,
        HARDWARE_PORT_RESOURCE,
        &mut channel,
        receiver,
        hardware_message,
    ) == Err(SyscallError::Capability(CapabilityError::WrongResource));
    if !direct_hardware_denied {
        return Err(SyscallError::Capability(CapabilityError::WrongResource));
    }
    if channel.receive(receiver) != Err(IpcError::QueueEmpty) {
        return Err(SyscallError::Ipc(IpcError::PayloadCorrupt));
    }

    Ok(BoundaryCapabilityProof {
        allowed_call: true,
        forged_handle_denied,
        direct_hardware_denied,
    })
}

pub fn run_general_abi_self_test() -> Result<GeneralSyscallAbiProof, SyscallError> {
    if SYSCALL_ABI_MAJOR != 1 || SYSCALL_ABI_MINOR != 0 {
        return Err(SyscallError::BadResult);
    }
    if validate_syscall_table(SYSCALL_TABLE).is_err() {
        return Err(SyscallError::BadResult);
    }

    EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
    if dispatch(SYSCALL_ABI_INFO)? != abi_info_result() {
        return Err(SyscallError::BadResult);
    }

    EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
    if dispatch(SYSCALL_SYSTEM_LOG_PROOF)? != SYSCALL_OK {
        return Err(SyscallError::BadResult);
    }

    EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
    let unknown_denied = dispatch(0x5059_FFFF) == Err(SyscallError::UnsupportedNumber);
    if !unknown_denied {
        return Err(SyscallError::BadResult);
    }

    Ok(GeneralSyscallAbiProof {
        versioned: true,
        known_dispatch: true,
        unknown_denied,
    })
}

fn syscall_gate_send_with_capability(
    table: &CapabilityTable,
    caller: ServiceId,
    handle: CapabilityHandle,
    resource: ResourceId,
    channel: &mut IpcChannel,
    to: ServiceId,
    message: IpcMessage,
) -> Result<(), SyscallError> {
    table.validate(caller, handle, resource, RightsMask::new(RightsMask::SEND))?;
    channel.send(caller, to, message)?;
    Ok(())
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
    fn abi_version_and_info_result_are_stable() {
        assert_eq!(SYSCALL_ABI_MAJOR, 1);
        assert_eq!(SYSCALL_ABI_MINOR, 0);
        assert_eq!(SYSCALL_ABI_INFO, 0x5059_0000);
        assert_eq!(abi_info_result(), 0x5059_0001_0000);
    }

    #[test]
    fn syscall_registry_is_sorted_and_duplicate_free() {
        assert_eq!(validate_syscall_table(SYSCALL_TABLE), Ok(()));
    }

    #[test]
    fn system_log_proof_number_is_permanent() {
        assert_eq!(SYSCALL_SYSTEM_LOG_PROOF, 0x5059_0001);
        let entry = lookup_syscall(SYSCALL_SYSTEM_LOG_PROOF).unwrap();
        assert_eq!(entry.name, "SYSCALL_SYSTEM_LOG_PROOF");
    }

    #[test]
    fn abi_info_dispatch_returns_version_metadata() {
        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(dispatch(SYSCALL_ABI_INFO), Ok(abi_info_result()));
    }

    #[test]
    fn unknown_syscall_number_is_denied_by_registry() {
        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(dispatch(0x5059_FFFF), Err(SyscallError::UnsupportedNumber));
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
            Err(SyscallError::UnsupportedNumber)
        );
    }

    #[test]
    fn dispatch_system_log_proof_uses_capability_and_log_surfaces() {
        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(dispatch(SYSCALL_SYSTEM_LOG_PROOF), Ok(SYSCALL_OK));
    }

    #[test]
    fn boundary_capability_self_test_denies_forged_and_hardware_requests() {
        let proof = run_boundary_capability_self_test().unwrap();

        assert!(proof.allowed_call);
        assert!(proof.forged_handle_denied);
        assert!(proof.direct_hardware_denied);
    }

    #[test]
    fn general_abi_self_test_proves_version_known_dispatch_and_unknown_denial() {
        let proof = run_general_abi_self_test().unwrap();

        assert!(proof.versioned);
        assert!(proof.known_dispatch);
        assert!(proof.unknown_denied);
    }
}
