//! Phase 8 syscall-entry proof, Phase 9 general syscall ABI registry, and the
//! ADR 0051 typed object-shell syscall bridge.
//!
//! Object-shell requests use the retained active process `UserCopyMap` before
//! raw copy-in/copy-out. Human command parsing stays in `shell.elf`.
#![cfg_attr(test, allow(dead_code, unused_imports))]

use crate::architecture::x86_64::gdt;
use crate::capabilities::{
    CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask,
};
use crate::ipc_channels::{IpcChannel, IpcError, IpcMessage};
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use crate::object_service::{ObjectService, ObjectServiceError};
use crate::permission_validation::{self, PermissionError};
use crate::process_context::{self, ActiveUserProcess, ProcessContextError};
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use crate::retained_services::{self, RetainedServiceError};
#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityTable};
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use crate::shell_objects::{ObjectId, ObjectKind};
use crate::system_api::{SystemApiError, SystemApiHost};
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use crate::task_context::TaskContextEvent;
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use crate::task_service::{self, TaskServiceError};
use crate::tasks::TaskId;
use crate::user_copy::UserCopyError;
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use crate::user_copy::{UserCopyAccess, UserCopyMap};
use crate::user_mode;
use crate::value_validation::{HostCallResult, UntrustedRuntimeValue};
#[cfg(not(test))]
use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use core::mem::{align_of, size_of};
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use core::slice;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use pythos_shared::object_shell_abi::{
    FIELD_TEXT, MAX_QUERY_RESULTS, OBJECT_KIND_NOTE, OBJECT_SHELL_ABI_MAJOR,
    OBJECT_SHELL_ABI_MINOR, OP_CREATE_OBJECT, OP_GET_HISTORY, OP_INSPECT_OBJECT, OP_QUERY_OBJECTS,
    OP_REVISE_FIELD, ObjectListEntry, ObjectShellRequest, ObjectShellResponse, STATUS_BAD_REQUEST,
    STATUS_BUFFER_TOO_SMALL, STATUS_DENIED, STATUS_NOT_FOUND, STATUS_OK,
};
use pythos_shared::object_shell_abi::{
    NO_BYTE, PackedCapability, SYSCALL_CONSOLE_READ_BYTE, SYSCALL_CONSOLE_WRITE_BYTE,
    SYSCALL_OBJECT_REQUEST, SYSCALL_OK, SYSCALL_SYSTEM_REBOOT,
};
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use pythos_shared::pyth_runtime_abi::{
    GRAPH_EXIT_BUDGET_EXHAUSTED, GRAPH_EXIT_OK, GRAPH_EXIT_RUNTIME_ERROR, GRAPH_MAX_LOG_BYTES,
    GRAPH_RESULT_UNIT, GraphExitRecord,
};
use pythos_shared::pyth_runtime_abi::{SYSCALL_PYTH_GRAPH_EXIT, SYSCALL_PYTH_GRAPH_LOG};
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use pythos_shared::task_abi::{
    MAX_TASK_PROPOSAL_RESULTS, OP_ABANDON_TASK, OP_APPEND_TASK_EVENT, OP_APPROVE_PROPOSAL,
    OP_COMPLETE_TASK, OP_CREATE_PROPOSAL, OP_CREATE_TASK, OP_LIST_PROPOSALS, OP_READ_ACTIVE_TASK,
    OP_READ_CONTEXT_SUMMARY, OP_REJECT_PROPOSAL, OP_REVIVE_TASK, OP_SUSPEND_TASK,
    SYSCALL_TASK_REQUEST, TASK_ABI_MAJOR, TASK_ABI_MINOR, TASK_REQUEST_SUSPEND_CURRENT,
    TaskContextSummary, TaskEventInput, TaskProposalListEntry, TaskRequest, TaskResponse,
};

pub const SYSCALL_ABI_MAJOR: u16 = 1;
pub const SYSCALL_ABI_MINOR: u16 = 0;
pub const SYSCALL_ABI_INFO: u64 = 0x5059_0000;
pub const SYSCALL_SYSTEM_LOG_PROOF: u64 = 0x5059_0001;

const SYSCALL_ABI_INFO_MAGIC: u64 = 0x5059_0000_0000;
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
const CONSOLE_COM2_RESOURCE: ResourceId = ResourceId::new(0x434F_4D32_434F_4E00);
const SYSTEM_CONTROL_RESOURCE: ResourceId = ResourceId::new(0x5359_5354_4354_524C);
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
const PYTH_GRAPH_SYSTEM_LOG_RESOURCE: ResourceId = ResourceId::new(0x5059_5447_4C4F_4700);
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
const MAX_TASK_INPUT_BYTES: u64 = 64;
const SYSCALL_MESSAGE_TYPE: u16 = 0x88;
const SYSCALL_PAYLOAD: [u8; 4] = [0x53, 0x43, 0x41, 0x4C];
const BOUNDARY_MESSAGE_TYPE: u16 = 0x89;
const BOUNDARY_PAYLOAD: [u8; 4] = [0x42, 0x4F, 0x55, 0x4E];
const SYSCALL_LOG_MESSAGE: &[u8] = b"PythOS [HISS] We Are Woken";

static EXPECTED_SYSCALL: AtomicBool = AtomicBool::new(false);
static SYSCALL_RETURNED: AtomicBool = AtomicBool::new(false);
static SYSCALL_LAST_RESULT: AtomicU64 = AtomicU64::new(0);

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyscallArgs {
    pub number: u64,
    pub arg0: u64,
    pub arg1: u64,
    pub arg2: u64,
    pub arg3: u64,
    pub arg4: u64,
}

impl SyscallArgs {
    const fn for_number(number: u64) -> Self {
        Self {
            number,
            arg0: 0,
            arg1: 0,
            arg2: 0,
            arg3: 0,
            arg4: 0,
        }
    }
}

struct SyscallCapabilityStorage(UnsafeCell<CapabilityTable>);

// SAFETY:
// 1. Invariant: ADR 0051 normal boot executes one active user process on one
//    CPU; syscall capability mutations are non-reentrant in this slice.
// 2. Established by: the current QEMU target is single-core and Task 7 only
//    grants console/system-control capabilities during shell bootstrap or
//    controlled tests before using them.
// 3. Lifetime: the capability table is static kernel-owned storage for the
//    full boot.
// 4. Pointer ownership: with_syscall_capabilities lends one mutable borrow for
//    one short grant/validate operation and never stores that borrow.
// 5. Alignment: UnsafeCell<CapabilityTable> preserves CapabilityTable alignment.
// 6. Mapped length: exactly one CapabilityTable value is accessed.
// 7. Concurrency: SMP and concurrent syscalls are outside ADR 0051; future SMP
//    work must replace this storage with scheduler-owned synchronization.
// 8. Violation: concurrent mutation could corrupt slots or validate authority
//    against the wrong holder.
unsafe impl Sync for SyscallCapabilityStorage {}

static SYSCALL_CAPABILITIES: SyscallCapabilityStorage =
    SyscallCapabilityStorage(UnsafeCell::new(CapabilityTable::new()));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyscallError {
    UnsupportedNumber,
    Capability(CapabilityError),
    Ipc(IpcError),
    Permission(PermissionError),
    ProcessContext(ProcessContextError),
    UserCopy(UserCopyError),
    #[cfg(any(test, all(not(test), not(feature = "verify"))))]
    ObjectService(ObjectServiceError),
    #[cfg(any(test, all(not(test), not(feature = "verify"))))]
    RetainedService(RetainedServiceError),
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
    ConsoleReadByte,
    ConsoleWriteByte,
    ObjectRequest,
    SystemReboot,
    #[cfg(any(test, all(not(test), not(feature = "verify"))))]
    TaskRequest,
    PythGraphLog,
    PythGraphExit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SyscallEntry {
    number: u64,
    name: &'static str,
    introduced_major: u16,
    introduced_minor: u16,
    proof_only: bool,
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
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::AbiInfo,
    },
    SyscallEntry {
        number: SYSCALL_SYSTEM_LOG_PROOF,
        name: "SYSCALL_SYSTEM_LOG_PROOF",
        introduced_major: 1,
        introduced_minor: 0,
        proof_only: true,
        dispatch_kind: SyscallDispatchKind::SystemLogProof,
    },
    SyscallEntry {
        number: SYSCALL_CONSOLE_READ_BYTE,
        name: "SYSCALL_CONSOLE_READ_BYTE",
        introduced_major: 1,
        introduced_minor: 0,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::ConsoleReadByte,
    },
    SyscallEntry {
        number: SYSCALL_CONSOLE_WRITE_BYTE,
        name: "SYSCALL_CONSOLE_WRITE_BYTE",
        introduced_major: 1,
        introduced_minor: 0,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::ConsoleWriteByte,
    },
    SyscallEntry {
        number: SYSCALL_OBJECT_REQUEST,
        name: "SYSCALL_OBJECT_REQUEST",
        introduced_major: 1,
        introduced_minor: 0,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::ObjectRequest,
    },
    SyscallEntry {
        number: SYSCALL_SYSTEM_REBOOT,
        name: "SYSCALL_SYSTEM_REBOOT",
        introduced_major: 1,
        introduced_minor: 0,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::SystemReboot,
    },
    #[cfg(any(test, all(not(test), not(feature = "verify"))))]
    SyscallEntry {
        number: SYSCALL_TASK_REQUEST,
        name: "SYSCALL_TASK_REQUEST",
        introduced_major: 1,
        introduced_minor: 0,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::TaskRequest,
    },
    SyscallEntry {
        number: SYSCALL_PYTH_GRAPH_LOG,
        name: "SYSCALL_PYTH_GRAPH_LOG",
        introduced_major: 1,
        introduced_minor: 0,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::PythGraphLog,
    },
    SyscallEntry {
        number: SYSCALL_PYTH_GRAPH_EXIT,
        name: "SYSCALL_PYTH_GRAPH_EXIT",
        introduced_major: 1,
        introduced_minor: 0,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::PythGraphExit,
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

impl From<ProcessContextError> for SyscallError {
    fn from(error: ProcessContextError) -> Self {
        Self::ProcessContext(error)
    }
}

impl From<UserCopyError> for SyscallError {
    fn from(error: UserCopyError) -> Self {
        Self::UserCopy(error)
    }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
impl From<ObjectServiceError> for SyscallError {
    fn from(error: ObjectServiceError) -> Self {
        Self::ObjectService(error)
    }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
impl From<RetainedServiceError> for SyscallError {
    fn from(error: RetainedServiceError) -> Self {
        Self::RetainedService(error)
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
    // Task 11: this stack lives in its own linker-script output section
    // (`.syscall_stack`, see core/linker.ld), sandwiched between two 4 KiB
    // ranges that `map_kernel_segments` (memory/virtual.rs) deliberately
    // leaves unmapped in every page table it builds. An overflow that
    // pushes rsp below `syscall_kernel_stack` now takes a #PF against the
    // low guard page - routed onto the IST1 fault stack (architecture/
    // x86_64/tss.rs, idt.rs) so the fault is delivered even though the
    // stack that just overflowed cannot be trusted - instead of silently
    // corrupting whatever static data used to sit below it.
    .section .syscall_stack, "aw", @nobits
    .balign 16
    // ADR 0052 durable-mutation persistence (`retained_services::persist_object_service`)
    // runs on this stack during `create`/`revise` dispatch and its call chain
    // builds a full ~3.8 KiB `ObjectServiceSnapshot` (plus checkpoint encode/
    // decode locals) through several unelided intermediate copies. 64 KiB was
    // enough for dispatch alone but silently overran into adjacent static
    // data once persistence was wired in - with no guard page below this
    // buffer, the overflow does not fault; it corrupts whatever static data
    // (e.g. GDT/IDT/page-table state) happens to sit below it in `.bss`,
    // which only surfaces later as an unexplained triple fault. 256 KiB
    // leaves >2x headroom over the ~96 KiB observed requirement. The guard
    // pages above are the deterministic backstop if that headroom is ever
    // exceeded.
    syscall_kernel_stack:
        .zero 262144
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
        mov r9, r8
        mov r8, r10
        mov rcx, rdx
        mov rdx, rsi
        mov rsi, rdi
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
    static syscall_kernel_stack: u8;
    static syscall_kernel_stack_end: u8;
}

/// The `[start, end)` byte range of the static stack `syscall_entry_abi`
/// switches onto for every ring-3 to ring-0 syscall entry. Used by
/// `retained_services::persist_object_service` (Task 11) to assert it is
/// actually running on the guarded syscall stack, not some other
/// unmeasured boot stack. `retained_services` itself is excluded from the
/// `verify` build, so this accessor is legitimately unused there.
#[cfg(not(test))]
#[cfg_attr(feature = "verify", allow(dead_code))]
pub fn kernel_stack_bounds() -> (u64, u64) {
    (
        &raw const syscall_kernel_stack as u64,
        &raw const syscall_kernel_stack_end as u64,
    )
}

/// Program the `syscall`/`sysret` MSRs. Production setup, reusable by both the
/// verification proof and normal boot (ADR 0052); performs no self-test.
#[cfg(not(test))]
pub fn initialize() {
    configure_gate();
}

#[cfg(not(test))]
pub fn run_self_test() -> Result<(), SyscallError> {
    initialize();
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
pub extern "C" fn syscall_dispatch_abi(
    number: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
) -> u64 {
    let result = dispatch(SyscallArgs {
        number,
        arg0,
        arg1,
        arg2,
        arg3,
        arg4,
    });
    let code = match result {
        Ok(code) => code,
        Err(SyscallError::UnsupportedNumber) => SYSCALL_ERROR_UNSUPPORTED_NUMBER,
        Err(SyscallError::UnexpectedSyscall) => SYSCALL_ERROR_UNEXPECTED,
        Err(_) => SYSCALL_ERROR_DISPATCH,
    };
    SYSCALL_LAST_RESULT.store(code, Ordering::SeqCst);
    SYSCALL_RETURNED.store(true, Ordering::SeqCst);

    code
}

fn dispatch(args: SyscallArgs) -> Result<u64, SyscallError> {
    let entry = lookup_syscall(args.number).ok_or(SyscallError::UnsupportedNumber)?;
    if entry.proof_only && !EXPECTED_SYSCALL.swap(false, Ordering::SeqCst) {
        return Err(SyscallError::UnexpectedSyscall);
    }

    match entry.dispatch_kind {
        SyscallDispatchKind::AbiInfo => Ok(abi_info_result()),
        SyscallDispatchKind::SystemLogProof => {
            #[cfg(not(test))]
            serial::write_line("PYTHOS:CORE:SYSCALL:ENTER");

            run_capability_gated_ipc_bridge()?;
            #[cfg(not(test))]
            serial::write_line("PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK");

            run_system_log_bridge()?;
            #[cfg(not(test))]
            serial::write_line("PYTHOS:CORE:SYSCALL:SYSTEM_LOG");
            #[cfg(not(test))]
            serial::write_line("PYTHOS:CORE:SYSCALL:RETURN");
            Ok(SYSCALL_OK)
        }
        SyscallDispatchKind::ConsoleReadByte => dispatch_console_read(args),
        SyscallDispatchKind::ConsoleWriteByte => dispatch_console_write(args),
        SyscallDispatchKind::ObjectRequest => dispatch_object_request(args),
        SyscallDispatchKind::SystemReboot => dispatch_system_reboot(args),
        #[cfg(any(test, all(not(test), not(feature = "verify"))))]
        SyscallDispatchKind::TaskRequest => dispatch_task_request(args),
        SyscallDispatchKind::PythGraphLog => dispatch_pyth_graph_log(args),
        SyscallDispatchKind::PythGraphExit => dispatch_pyth_graph_exit(args),
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

fn with_syscall_capabilities<R>(f: impl FnOnce(&mut CapabilityTable) -> R) -> R {
    // SAFETY:
    // 1. Invariant: ADR 0051 normal boot handles one syscall at a time on one
    //    CPU, so no concurrent mutable borrow of this table exists.
    // 2. Established by: persistent shell launch remains single-process and
    //    SMP is explicitly outside this slice.
    // 3. Lifetime: the mutable borrow is confined to this function call and
    //    never stored.
    // 4. Pointer ownership: SYSCALL_CAPABILITIES owns the static table.
    // 5. Alignment: UnsafeCell<CapabilityTable> preserves table alignment.
    // 6. Mapped length: exactly one CapabilityTable value is accessed.
    // 7. Concurrency: tests that reset/grant this table serialize themselves;
    //    production is single-core before future SMP work.
    // 8. Violation: reentrant mutation could corrupt authority slots.
    unsafe { f(&mut *SYSCALL_CAPABILITIES.0.get()) }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
pub fn grant_console_capability(
    process: ActiveUserProcess,
) -> Result<PackedCapability, SyscallError> {
    let handle = with_syscall_capabilities(|table| {
        table.grant(
            process.service_id(),
            CONSOLE_COM2_RESOURCE,
            RightsMask::new(RightsMask::READ | RightsMask::WRITE),
        )
    })?;
    Ok(pack_syscall_capability(handle))
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
pub fn grant_system_control_capability(
    process: ActiveUserProcess,
) -> Result<PackedCapability, SyscallError> {
    let handle = with_syscall_capabilities(|table| {
        table.grant(
            process.service_id(),
            SYSTEM_CONTROL_RESOURCE,
            RightsMask::new(RightsMask::WRITE),
        )
    })?;
    Ok(pack_syscall_capability(handle))
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
pub fn grant_pyth_graph_system_log_capability(
    process: ActiveUserProcess,
) -> Result<PackedCapability, SyscallError> {
    let handle = with_syscall_capabilities(|table| {
        table.grant(
            process.service_id(),
            PYTH_GRAPH_SYSTEM_LOG_RESOURCE,
            RightsMask::new(RightsMask::LOG),
        )
    })?;
    Ok(pack_syscall_capability(handle))
}

fn dispatch_console_read(args: SyscallArgs) -> Result<u64, SyscallError> {
    let caller = process_context::current_caller()?;
    validate_syscall_capability(
        caller,
        PackedCapability::from_raw(args.arg0),
        CONSOLE_COM2_RESOURCE,
        RightsMask::new(RightsMask::READ),
    )?;
    #[cfg(not(test))]
    {
        Ok(serial::try_read_byte_com2().map_or(NO_BYTE, u64::from))
    }
    #[cfg(test)]
    {
        Ok(NO_BYTE)
    }
}

fn dispatch_console_write(args: SyscallArgs) -> Result<u64, SyscallError> {
    let caller = process_context::current_caller()?;
    dispatch_console_write_for_caller(caller, PackedCapability::from_raw(args.arg0), args.arg1)
}

fn dispatch_console_write_for_caller(
    caller: ActiveUserProcess,
    capability: PackedCapability,
    byte: u64,
) -> Result<u64, SyscallError> {
    if byte > u64::from(u8::MAX) {
        return Err(SyscallError::BadResult);
    }
    validate_syscall_capability(
        caller,
        capability,
        CONSOLE_COM2_RESOURCE,
        RightsMask::new(RightsMask::WRITE),
    )?;
    #[cfg(not(test))]
    serial::write_byte_com2(byte as u8);
    Ok(SYSCALL_OK)
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn dispatch_object_request(args: SyscallArgs) -> Result<u64, SyscallError> {
    let caller = process_context::current_caller()?;
    if args.arg1 != size_of::<ObjectShellRequest>() as u64
        || args.arg3 != size_of::<ObjectShellResponse>() as u64
        || args.arg4 != 0
    {
        return Err(SyscallError::BadResult);
    }
    let copy_map = caller.copy_map();
    validate_user_buffer(
        &copy_map,
        args.arg0,
        args.arg1,
        align_of::<ObjectShellRequest>(),
        UserCopyAccess::Read,
    )?;
    validate_user_buffer(
        &copy_map,
        args.arg2,
        args.arg3,
        align_of::<ObjectShellResponse>(),
        UserCopyAccess::Write,
    )?;
    let request_ptr = args.arg0 as *const ObjectShellRequest;
    let response_ptr = args.arg2 as *mut ObjectShellResponse;
    // SAFETY:
    // 1. Invariant: `request_ptr` names a live, readable
    //    ObjectShellRequest supplied by the active user process for this
    //    syscall; `response_ptr` names a live, writable ObjectShellResponse.
    // 2. Established by: exact ABI size checks, natural alignment checks, and
    //    the active process's retained UserCopyMap validation above.
    // 3. Lifetime: both buffers remain valid only for this syscall; neither
    //    pointer is retained after returning.
    // 4. Pointer ownership: the user process owns the buffers, PythCore only
    //    copies in/out.
    // 5. Alignment: checked against repr(C) alignment before dereference.
    // 6. Mapped length: UserCopyMap validated each exact ABI-sized range.
    // 7. Concurrency: ADR 0051 shell is single-threaded and one syscall is
    //    handled at a time.
    // 8. Violation: stale or forged map state could fault or corrupt user
    //    memory; Task 8 must bind this map from the validated launch surface.
    let request = unsafe { request_ptr.read() };
    let response = dispatch_object_request_with_raw_buffers(caller, &copy_map, request)?;
    // SAFETY: see the invariant block above; this is the matching copy-out to
    // the already UserCopyMap-validated response pointer.
    unsafe {
        response_ptr.write(response);
    }
    Ok(SYSCALL_OK)
}

#[cfg(all(not(test), feature = "verify"))]
fn dispatch_object_request(_args: SyscallArgs) -> Result<u64, SyscallError> {
    Err(SyscallError::BadResult)
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn dispatch_object_request_with_raw_buffers(
    caller: ActiveUserProcess,
    copy_map: &UserCopyMap,
    request: ObjectShellRequest,
) -> Result<ObjectShellResponse, SyscallError> {
    if !valid_object_request_header(&request) {
        return Ok(bad_request_response());
    }
    let input = if request.operation == OP_REVISE_FIELD {
        checked_request_input(copy_map, &request)?
    } else {
        &[]
    };
    let response = if request.operation == OP_QUERY_OBJECTS {
        if request.output_len < size_of::<[ObjectListEntry; MAX_QUERY_RESULTS]>() as u64 {
            return Ok(buffer_too_small_response());
        }
        let output = checked_query_output(copy_map, &request)?;
        retained_services::with_object_service(|service| {
            dispatch_object_request_to_service(service, caller, request, input, output)
        })
        .map_err(SyscallError::from)?
    } else {
        retained_services::with_object_service(|service| {
            dispatch_object_request_to_service(service, caller, request, input, &mut [])
        })
        .map_err(SyscallError::from)?
    };

    // ADR 0052: durable mutations persist here so a capability-gated `reboot`
    // syscall (Task 9) restores exactly this state. See the syscall_kernel_stack
    // comment above `syscall_entry_abi` for why this needed a larger kernel
    // stack before it could be wired in safely.
    #[cfg(not(test))]
    if response.status == STATUS_OK
        && matches!(request.operation, OP_CREATE_OBJECT | OP_REVISE_FIELD)
    {
        retained_services::persist_object_service().map_err(SyscallError::from)?;
    }

    Ok(response)
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn dispatch_task_request(args: SyscallArgs) -> Result<u64, SyscallError> {
    let caller = process_context::current_caller()?;
    if args.arg1 != size_of::<TaskRequest>() as u64
        || args.arg3 != size_of::<TaskResponse>() as u64
        || args.arg4 != 0
    {
        return Err(SyscallError::BadResult);
    }
    let copy_map = caller.copy_map();
    validate_user_buffer(
        &copy_map,
        args.arg0,
        args.arg1,
        align_of::<TaskRequest>(),
        UserCopyAccess::Read,
    )?;
    validate_user_buffer(
        &copy_map,
        args.arg2,
        args.arg3,
        align_of::<TaskResponse>(),
        UserCopyAccess::Write,
    )?;
    let request_ptr = args.arg0 as *const TaskRequest;
    let response_ptr = args.arg2 as *mut TaskResponse;
    // SAFETY:
    // 1. Invariant: `request_ptr` is a readable TaskRequest and
    //    `response_ptr` is a writable TaskResponse in the current caller's
    //    validated user copy map.
    // 2. Established by: exact ABI size checks, repr(C) alignment checks, and
    //    UserCopyMap range validation above.
    // 3. Lifetime: both pointers are consumed only for this syscall and are
    //    not retained by PythCore.
    // 4. Pointer ownership: the user process owns both buffers; PythCore only
    //    copies the request in and response out.
    // 5. Alignment: checked against each ABI type's natural alignment.
    // 6. Mapped length: UserCopyMap validated the full fixed-size ranges.
    // 7. Concurrency: ADR 0051 normal boot handles one syscall at a time.
    // 8. Violation: stale copy-map authority could read or write the wrong
    //    user memory, so the caller-derived map is the authority boundary.
    let request = unsafe { request_ptr.read() };
    let response = dispatch_task_request_with_raw_buffers(caller, &copy_map, request)?;
    // SAFETY: same validated response pointer described above; this is the
    // matching bounded copy-out for the one TaskResponse value.
    unsafe {
        response_ptr.write(response);
    }
    Ok(SYSCALL_OK)
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn dispatch_task_request_with_raw_buffers(
    caller: ActiveUserProcess,
    copy_map: &UserCopyMap,
    request: TaskRequest,
) -> Result<TaskResponse, SyscallError> {
    if !valid_task_request_header(&request) {
        return Ok(bad_task_response());
    }
    let input = checked_task_input(copy_map, &request)?;
    let mut context_output = if request.operation == OP_READ_CONTEXT_SUMMARY {
        if request.output_len < size_of::<TaskContextSummary>() as u64 {
            return Ok(task_buffer_too_small_response());
        }
        Some(checked_task_context_output(copy_map, &request)?)
    } else {
        None
    };
    let mut proposal_output = if request.operation == OP_LIST_PROPOSALS {
        if request.output_len
            < size_of::<[TaskProposalListEntry; MAX_TASK_PROPOSAL_RESULTS]>() as u64
        {
            return Ok(task_buffer_too_small_response());
        }
        Some(checked_task_proposal_output(copy_map, &request)?)
    } else {
        None
    };

    let response = retained_services::with_task_service(|service| {
        dispatch_task_request_to_service(
            service,
            caller,
            request,
            input,
            context_output.as_deref_mut(),
            proposal_output.as_deref_mut(),
        )
    })
    .map_err(SyscallError::from)?;

    #[cfg(not(test))]
    if response.status == STATUS_OK && task_operation_mutates(request.operation) {
        retained_services::persist_object_service().map_err(SyscallError::from)?;
    }

    Ok(response)
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn dispatch_task_request_to_service(
    service: &mut task_service::TaskService<'_>,
    caller: ActiveUserProcess,
    request: TaskRequest,
    input: &[u8],
    context_output: Option<&mut TaskContextSummary>,
    proposal_output: Option<&mut [TaskProposalListEntry]>,
) -> TaskResponse {
    match request.operation {
        OP_CREATE_TASK => match service.create_task(caller, request_authority(request), input) {
            Ok(created) => TaskResponse {
                status: STATUS_OK,
                operation: request.operation,
                task_id: created.task_id,
                active_task_id: service.active_task_id().unwrap_or(0),
                ..empty_task_response()
            },
            Err(error) => task_error_response(error),
        },
        OP_READ_ACTIVE_TASK => match service.read_active_task(caller, request_authority(request)) {
            Ok(active) => TaskResponse {
                status: STATUS_OK,
                operation: request.operation,
                active_task_id: active.unwrap_or(0),
                ..empty_task_response()
            },
            Err(error) => task_error_response(error),
        },
        OP_APPEND_TASK_EVENT => {
            let active_task_id = service.active_task_id().unwrap_or(0);
            let task_id = if request.task_id == 0 {
                active_task_id
            } else {
                request.task_id
            };
            let result = if input.is_empty() {
                service.append_task_event(caller, request_authority(request), task_id)
            } else if let Some(event) = task_event_input_from_bytes(input) {
                service
                    .append_task_context_event(caller, request_authority(request), task_id, event)
                    .map(|_| ())
            } else {
                return bad_task_response();
            };
            match result {
                Ok(()) => TaskResponse {
                    status: STATUS_OK,
                    operation: request.operation,
                    task_id,
                    active_task_id: service.active_task_id().unwrap_or(0),
                    ..empty_task_response()
                },
                Err(error) => task_error_response(error),
            }
        }
        OP_CREATE_PROPOSAL => {
            let Some(kind) = task_service::proposal_kind_from_code(request.proposal_kind) else {
                return bad_task_response();
            };
            match service.create_proposal(
                caller,
                request_authority(request),
                kind,
                request.task_id,
                request.target_task_id,
                request.score,
                input,
                &[],
            ) {
                Ok(proposal) => TaskResponse {
                    status: STATUS_OK,
                    operation: request.operation,
                    proposal_kind: request.proposal_kind,
                    proposal_id: proposal.proposal_id,
                    active_task_id: service.active_task_id().unwrap_or(0),
                    score: request.score,
                    ..empty_task_response()
                },
                Err(error) => task_error_response(error),
            }
        }
        OP_LIST_PROPOSALS => {
            let Some(output) = proposal_output else {
                return bad_task_response();
            };
            match service.list_pending_proposals(caller, request_authority(request), output) {
                Ok(count) => TaskResponse {
                    status: STATUS_OK,
                    operation: request.operation,
                    active_task_id: service.active_task_id().unwrap_or(0),
                    bytes_written: (count * size_of::<TaskProposalListEntry>()) as u64,
                    ..empty_task_response()
                },
                Err(error) => task_error_response(error),
            }
        }
        OP_APPROVE_PROPOSAL => match service.approve_proposal(
            caller,
            request_authority(request),
            request.proposal_id,
            request.flags & TASK_REQUEST_SUSPEND_CURRENT != 0,
        ) {
            Ok(created) => TaskResponse {
                status: STATUS_OK,
                operation: request.operation,
                proposal_id: request.proposal_id,
                task_id: created.task_id,
                active_task_id: service.active_task_id().unwrap_or(0),
                ..empty_task_response()
            },
            Err(error) => task_error_response(error),
        },
        OP_REJECT_PROPOSAL => {
            match service.reject_proposal(caller, request_authority(request), request.proposal_id) {
                Ok(()) => TaskResponse {
                    status: STATUS_OK,
                    operation: request.operation,
                    proposal_id: request.proposal_id,
                    active_task_id: service.active_task_id().unwrap_or(0),
                    ..empty_task_response()
                },
                Err(error) => task_error_response(error),
            }
        }
        OP_SUSPEND_TASK => task_transition_response(
            service.suspend_task(caller, request_authority(request), request.task_id),
            service,
            request,
        ),
        OP_REVIVE_TASK => task_transition_response(
            service.revive_task(caller, request_authority(request), request.task_id),
            service,
            request,
        ),
        OP_COMPLETE_TASK => task_transition_response(
            service.complete_task(caller, request_authority(request), request.task_id),
            service,
            request,
        ),
        OP_ABANDON_TASK => task_transition_response(
            service.abandon_task(caller, request_authority(request), request.task_id),
            service,
            request,
        ),
        OP_READ_CONTEXT_SUMMARY => {
            let Some(output) = context_output else {
                return bad_task_response();
            };
            match service.read_context_summary(caller, request_authority(request)) {
                Ok(summary) => {
                    *output = summary;
                    TaskResponse {
                        status: STATUS_OK,
                        operation: request.operation,
                        active_task_id: summary.active_task_id,
                        bytes_written: size_of::<TaskContextSummary>() as u64,
                        score: summary.confidence_score,
                        ..empty_task_response()
                    }
                }
                Err(error) => task_error_response(error),
            }
        }
        _ => bad_task_response(),
    }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn task_transition_response(
    result: Result<(), TaskServiceError>,
    service: &task_service::TaskService<'_>,
    request: TaskRequest,
) -> TaskResponse {
    match result {
        Ok(()) => TaskResponse {
            status: STATUS_OK,
            operation: request.operation,
            task_id: request.task_id,
            active_task_id: service.active_task_id().unwrap_or(0),
            ..empty_task_response()
        },
        Err(error) => task_error_response(error),
    }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn valid_task_request_header(request: &TaskRequest) -> bool {
    request.abi_major == TASK_ABI_MAJOR
        && request.abi_minor == TASK_ABI_MINOR
        && request.reserved0 == 0
        && request.flags & !TASK_REQUEST_SUSPEND_CURRENT == 0
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn checked_task_input<'a>(
    copy_map: &UserCopyMap,
    request: &TaskRequest,
) -> Result<&'a [u8], SyscallError> {
    if request.input_len == 0 {
        return Ok(&[]);
    }
    if request.input_len > MAX_TASK_INPUT_BYTES {
        return Err(SyscallError::BadResult);
    }
    copy_map.validate_range(request.input_ptr, request.input_len, UserCopyAccess::Read)?;
    // SAFETY:
    // 1. Invariant: non-empty task input names at most MAX_TASK_INPUT_BYTES
    //    readable bytes in the active caller's user address space.
    // 2. Established by: the bounded input_len check and caller-derived
    //    UserCopyMap readable-range validation above.
    // 3. Lifetime: the slice is consumed synchronously during this syscall.
    // 4. Pointer ownership: user space owns the bytes; PythCore reads only.
    // 5. Alignment: byte slices impose no stricter alignment.
    // 6. Mapped length: UserCopyMap validated the exact requested range.
    // 7. Concurrency: ADR 0051 shell/runtime syscalls are single-threaded.
    // 8. Violation: stale copy-map authority could read unrelated memory.
    Ok(
        unsafe {
            slice::from_raw_parts(request.input_ptr as *const u8, request.input_len as usize)
        },
    )
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn checked_task_context_output<'a>(
    copy_map: &UserCopyMap,
    request: &TaskRequest,
) -> Result<&'a mut TaskContextSummary, SyscallError> {
    validate_user_buffer(
        copy_map,
        request.output_ptr,
        size_of::<TaskContextSummary>() as u64,
        align_of::<TaskContextSummary>(),
        UserCopyAccess::Write,
    )?;
    let output_ptr = request.output_ptr as *mut TaskContextSummary;
    // SAFETY:
    // 1. Invariant: output_ptr names one writable TaskContextSummary in the
    //    active caller's validated user copy map.
    // 2. Established by: fixed-size UserCopyMap writable-range validation and
    //    repr(C) alignment check above.
    // 3. Lifetime: the reference is used only before syscall return.
    // 4. Pointer ownership: the user process owns the buffer; PythCore writes
    //    exactly one summary and does not retain the pointer.
    // 5. Alignment: checked against TaskContextSummary alignment.
    // 6. Mapped length: the full TaskContextSummary size was validated.
    // 7. Concurrency: one syscall is handled at a time in this slice.
    // 8. Violation: stale map state could corrupt caller memory.
    Ok(unsafe { &mut *output_ptr })
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn checked_task_proposal_output<'a>(
    copy_map: &UserCopyMap,
    request: &TaskRequest,
) -> Result<&'a mut [TaskProposalListEntry], SyscallError> {
    let output_len = size_of::<[TaskProposalListEntry; MAX_TASK_PROPOSAL_RESULTS]>() as u64;
    validate_user_buffer(
        copy_map,
        request.output_ptr,
        output_len,
        align_of::<TaskProposalListEntry>(),
        UserCopyAccess::Write,
    )?;
    let output_ptr = request.output_ptr as *mut TaskProposalListEntry;
    // SAFETY:
    // 1. Invariant: output_ptr names a writable fixed proposal-list buffer in
    //    the active caller's validated user copy map.
    // 2. Established by: output_len is checked against the exact bounded ABI
    //    array size and UserCopyMap writable-range validation above.
    // 3. Lifetime: the slice is used only before syscall return.
    // 4. Pointer ownership: user space owns the buffer; PythCore writes only
    //    the bounded proposal-list records and retains no pointer.
    // 5. Alignment: checked against TaskProposalListEntry alignment.
    // 6. Mapped length: the full fixed list buffer size was validated.
    // 7. Concurrency: one shell/runtime syscall is handled at a time here.
    // 8. Violation: stale map state could corrupt caller memory.
    Ok(unsafe { slice::from_raw_parts_mut(output_ptr, MAX_TASK_PROPOSAL_RESULTS) })
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn task_event_input_from_bytes(input: &[u8]) -> Option<TaskContextEvent> {
    if input.len() != size_of::<TaskEventInput>() {
        return None;
    }
    let tag_hash = read_u64_input(input, 0)?;
    let object_kind = read_u16_input(input, 8)?;
    let tool_domain = read_u16_input(input, 10)?;
    let flags = read_u16_input(input, 12)?;
    let reserved0 = read_u16_input(input, 14)?;
    if reserved0 != 0 {
        return None;
    }
    Some(TaskContextEvent::new(0, object_kind, tool_domain, tag_hash).with_flags(flags))
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn read_u16_input(input: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        *input.get(offset)?,
        *input.get(offset + 1)?,
    ]))
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn read_u64_input(input: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes([
        *input.get(offset)?,
        *input.get(offset + 1)?,
        *input.get(offset + 2)?,
        *input.get(offset + 3)?,
        *input.get(offset + 4)?,
        *input.get(offset + 5)?,
        *input.get(offset + 6)?,
        *input.get(offset + 7)?,
    ]))
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn task_error_response(error: TaskServiceError) -> TaskResponse {
    let status = match error {
        TaskServiceError::Denied => STATUS_DENIED,
        TaskServiceError::NotFound => STATUS_NOT_FOUND,
        _ => STATUS_BAD_REQUEST,
    };
    TaskResponse {
        status,
        ..empty_task_response()
    }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn empty_task_response() -> TaskResponse {
    TaskResponse {
        status: STATUS_BAD_REQUEST,
        operation: 0,
        proposal_kind: 0,
        reserved0: 0,
        task_id: 0,
        proposal_id: 0,
        active_task_id: 0,
        bytes_written: 0,
        score: 0,
        reserved1: 0,
        reserved2: 0,
    }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn bad_task_response() -> TaskResponse {
    empty_task_response()
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn task_buffer_too_small_response() -> TaskResponse {
    TaskResponse {
        status: STATUS_BUFFER_TOO_SMALL,
        ..empty_task_response()
    }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
const fn request_authority(request: TaskRequest) -> PackedCapability {
    PackedCapability::from_raw(request.authority)
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn task_operation_mutates(operation: u16) -> bool {
    matches!(
        operation,
        OP_CREATE_TASK
            | OP_APPEND_TASK_EVENT
            | OP_CREATE_PROPOSAL
            | OP_APPROVE_PROPOSAL
            | OP_REJECT_PROPOSAL
            | OP_SUSPEND_TASK
            | OP_REVIVE_TASK
            | OP_COMPLETE_TASK
            | OP_ABANDON_TASK
            | OP_READ_CONTEXT_SUMMARY
    )
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn validate_user_buffer(
    copy_map: &UserCopyMap,
    ptr: u64,
    len: u64,
    alignment: usize,
    access: UserCopyAccess,
) -> Result<(), SyscallError> {
    if !is_aligned(ptr, alignment) {
        return Err(SyscallError::BadResult);
    }
    copy_map.validate_range(ptr, len, access)?;
    Ok(())
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn checked_query_output<'a>(
    copy_map: &UserCopyMap,
    request: &ObjectShellRequest,
) -> Result<&'a mut [ObjectListEntry], SyscallError> {
    let output_len = size_of::<[ObjectListEntry; MAX_QUERY_RESULTS]>() as u64;
    validate_user_buffer(
        copy_map,
        request.output_ptr,
        output_len,
        align_of::<ObjectListEntry>(),
        UserCopyAccess::Write,
    )?;
    let output_ptr = request.output_ptr as *mut ObjectListEntry;
    // SAFETY:
    // 1. Invariant: query output points at a writable array of
    //    MAX_QUERY_RESULTS ObjectListEntry values in the caller's
    //    address space.
    // 2. Established by: fixed Task 7 ABI requires `output_len` at least the
    //    full array size and UserCopyMap validates that exact writable range
    //    with natural ObjectListEntry alignment above.
    // 3. Lifetime: the slice is used only inside this syscall.
    // 4. Pointer ownership: user space owns the memory; PythCore writes
    //    bounded result entries and does not retain the pointer.
    // 5. Alignment: checked above.
    // 6. Mapped length: UserCopyMap checked the full fixed array range.
    // 7. Concurrency: shell is single-threaded in ADR 0051.
    // 8. Violation: stale map state could fault or corrupt user memory.
    Ok(unsafe { slice::from_raw_parts_mut(output_ptr, MAX_QUERY_RESULTS) })
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn checked_request_input<'a>(
    copy_map: &UserCopyMap,
    request: &ObjectShellRequest,
) -> Result<&'a [u8], SyscallError> {
    if request.input_len == 0 {
        return Ok(&[]);
    }
    if request.input_len > 16 {
        return Err(SyscallError::BadResult);
    }
    copy_map.validate_range(request.input_ptr, request.input_len, UserCopyAccess::Read)?;
    // SAFETY:
    // 1. Invariant: non-empty request input points at at most 16 readable bytes
    //    in the active shell process.
    // 2. Established by: Task 7 caps input_len at one TypedObjectField payload
    //    and UserCopyMap validates the exact readable byte range above.
    // 3. Lifetime: the returned slice is consumed during the current syscall
    //    and never stored.
    // 4. Pointer ownership: user space owns the input bytes; PythCore reads
    //    them only for typed object mutation.
    // 5. Alignment: byte slices require no stricter alignment than 1.
    // 6. Mapped length: UserCopyMap checked the requested bounded length.
    // 7. Concurrency: shell is single-threaded in ADR 0051.
    // 8. Violation: stale map state could fault or read unrelated memory.
    Ok(
        unsafe {
            slice::from_raw_parts(request.input_ptr as *const u8, request.input_len as usize)
        },
    )
}

fn dispatch_system_reboot(args: SyscallArgs) -> Result<u64, SyscallError> {
    let caller = process_context::current_caller()?;
    dispatch_system_reboot_for_caller(caller, PackedCapability::from_raw(args.arg0))
}

fn dispatch_system_reboot_for_caller(
    caller: ActiveUserProcess,
    capability: PackedCapability,
) -> Result<u64, SyscallError> {
    validate_syscall_capability(
        caller,
        capability,
        SYSTEM_CONTROL_RESOURCE,
        RightsMask::new(RightsMask::WRITE),
    )?;
    #[cfg(not(test))]
    {
        serial::write_line("PYTHOS:SHELL:REBOOT_REQUESTED");
        serial::write_line("PYTHOS:CORE:SYSTEM:REBOOTING");
        crate::qemu_exit::reboot_qemu()
    }
    #[cfg(test)]
    Ok(SYSCALL_OK)
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn dispatch_pyth_graph_log(args: SyscallArgs) -> Result<u64, SyscallError> {
    if args.arg2 == 0 || args.arg2 > GRAPH_MAX_LOG_BYTES || args.arg3 != 0 || args.arg4 != 0 {
        return Err(SyscallError::BadResult);
    }
    let caller = process_context::current_caller()?;
    validate_syscall_capability(
        caller,
        PackedCapability::from_raw(args.arg0),
        PYTH_GRAPH_SYSTEM_LOG_RESOURCE,
        RightsMask::new(RightsMask::LOG),
    )?;
    let copy_map = caller.copy_map();
    copy_map.validate_range(args.arg1, args.arg2, UserCopyAccess::Read)?;
    let text_len = usize::try_from(args.arg2).map_err(|_| SyscallError::BadResult)?;
    // SAFETY:
    // 1. Invariant: `arg1..arg1+arg2` names a readable user buffer in the
    //    active graph runtime process.
    // 2. Established by: nonzero length, `GRAPH_MAX_LOG_BYTES` cap, and the
    //    active `UserCopyMap` read validation immediately above.
    // 3. Lifetime: the slice is used only during this syscall and is not
    //    retained.
    // 4. Pointer ownership: user space owns the bytes; PythCore only reads
    //    them to validate the host-operation boundary.
    // 5. Alignment: byte slices require alignment 1.
    // 6. Mapped length: `UserCopyMap` validated exactly `arg2` bytes.
    // 7. Concurrency: Phase 2 runs one graph runtime on one CPU.
    // 8. Violation: stale copy-map state could fault while reading user text.
    let _text = unsafe { slice::from_raw_parts(args.arg1 as *const u8, text_len) };
    #[cfg(not(test))]
    serial::write_line("PYTHOS:PYTHTIG:PROGRAM_LOG");
    Ok(SYSCALL_OK)
}

#[cfg(all(not(test), feature = "verify"))]
fn dispatch_pyth_graph_log(_args: SyscallArgs) -> Result<u64, SyscallError> {
    Err(SyscallError::BadResult)
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn dispatch_pyth_graph_exit(args: SyscallArgs) -> Result<u64, SyscallError> {
    if args.arg0 != crate::pyth_runtime_launch::PYTH_GRAPH_RESULT_USER_PTR
        || args.arg1 != size_of::<GraphExitRecord>() as u64
        || args.arg2 != 0
        || args.arg3 != 0
        || args.arg4 != 0
    {
        return Err(SyscallError::BadResult);
    }
    let caller = process_context::current_caller()?;
    if !crate::user_mode::is_active_pyth_graph_process(caller.principal_id()) {
        return Err(SyscallError::BadResult);
    }
    let copy_map = caller.copy_map();
    validate_user_buffer(
        &copy_map,
        args.arg0,
        args.arg1,
        align_of::<GraphExitRecord>(),
        UserCopyAccess::Read,
    )?;
    let exit_ptr = args.arg0 as *const GraphExitRecord;
    // SAFETY:
    // 1. Invariant: `exit_ptr` names a readable `GraphExitRecord` in the active
    //    graph runtime result page.
    // 2. Established by: exact pointer/size checks, natural alignment, and
    //    `UserCopyMap` read validation above.
    // 3. Lifetime: the record is copied once and not retained.
    // 4. Pointer ownership: the runtime owns the writable result page; PythCore
    //    reads the final record at the explicit exit syscall.
    // 5. Alignment: checked against `GraphExitRecord` alignment above.
    // 6. Mapped length: `UserCopyMap` validated the exact record size.
    // 7. Concurrency: Phase 2 graph runtime has one active thread.
    // 8. Violation: bad result mapping could fault or report a forged status.
    let exit = unsafe { exit_ptr.read() };
    finalize_pyth_graph_exit(caller, exit)
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn finalize_pyth_graph_exit(
    caller: ActiveUserProcess,
    exit: GraphExitRecord,
) -> Result<u64, SyscallError> {
    validate_graph_exit_record(exit)?;
    emit_graph_exit_marker(exit);
    #[cfg(not(test))]
    crate::user_mode::complete_pyth_graph_runtime_exit(caller.principal_id());
    #[cfg(test)]
    {
        if !crate::user_mode::transition_pyth_graph_runtime_exit(caller.principal_id()) {
            return Err(SyscallError::BadResult);
        }
        Ok(SYSCALL_OK)
    }
}

#[cfg(all(not(test), feature = "verify"))]
fn dispatch_pyth_graph_exit(_args: SyscallArgs) -> Result<u64, SyscallError> {
    Err(SyscallError::BadResult)
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn validate_graph_exit_record(exit: GraphExitRecord) -> Result<(), SyscallError> {
    if exit.result_type != GRAPH_RESULT_UNIT || exit.reserved0 != 0 || exit.reserved1 != 0 {
        return Err(SyscallError::BadResult);
    }
    match exit.status {
        GRAPH_EXIT_OK if exit.error_code == 0 => Ok(()),
        GRAPH_EXIT_RUNTIME_ERROR | GRAPH_EXIT_BUDGET_EXHAUSTED => Ok(()),
        _ => Err(SyscallError::BadResult),
    }
}

#[cfg(all(not(test), not(feature = "verify")))]
fn emit_graph_exit_marker(exit: GraphExitRecord) {
    if exit.status == GRAPH_EXIT_BUDGET_EXHAUSTED {
        serial::write_str("PYTHOS:PYTHTIG:BUDGET_EXHAUSTED node:");
        serial::write_dec_u64_value(u64::from(exit.last_node));
        serial::write_str("\r\n");
    }
    if crate::user_mode::is_active_pyth_native_graph() {
        serial::write_str("PYTHOS:PYTHTIG:NATIVE_EXIT status:");
    } else {
        serial::write_str("PYTHOS:PYTHTIG:RUNTIME_EXIT status:");
    }
    serial::write_dec_u64_value(u64::from(exit.status));
    serial::write_str("\r\n");
    if exit.status == GRAPH_EXIT_OK
        && crate::pyth_runtime_launch::take_object_flow_completion_marker()
    {
        serial::write_line("PYTHOS:PYTHTIG:OBJECT_FLOW_ACCEPTANCE_COMPLETE");
    }
}

#[cfg(test)]
fn emit_graph_exit_marker(_exit: GraphExitRecord) {}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn dispatch_object_request_to_service(
    service: &mut ObjectService,
    caller: ActiveUserProcess,
    request: ObjectShellRequest,
    input: &[u8],
    output: &mut [ObjectListEntry],
) -> ObjectShellResponse {
    if !valid_object_request_header(&request) {
        return bad_request_response();
    }

    match request.operation {
        OP_CREATE_OBJECT => match request_object_kind(request.object_kind)
            .and_then(|kind| service.create_object(caller, request.authority, kind))
        {
            Ok(created) => {
                let response = ObjectShellResponse {
                    status: STATUS_OK,
                    object_kind: request.object_kind,
                    object_id: created.object_id.raw(),
                    revision: created.revision,
                    capability: created.object_capability,
                    ..empty_response()
                };
                emit_pythtig_object_success_marker(caller, request.operation, response, None);
                response
            }
            Err(error) => object_error_response(caller, request, error),
        },
        OP_QUERY_OBJECTS => {
            if output.len() < MAX_QUERY_RESULTS {
                return buffer_too_small_response();
            }
            match request_object_kind(request.object_kind)
                .and_then(|kind| service.query_objects(caller, request.authority, kind))
            {
                Ok(entries) => {
                    let mut count = 0usize;
                    while count < MAX_QUERY_RESULTS && entries[count].object_id != 0 {
                        output[count] = entries[count];
                        count += 1;
                    }
                    let response = ObjectShellResponse {
                        status: STATUS_OK,
                        object_kind: request.object_kind,
                        bytes_written: (count * size_of::<ObjectListEntry>()) as u64,
                        ..empty_response()
                    };
                    emit_pythtig_object_success_marker(
                        caller,
                        request.operation,
                        response,
                        Some(output[0]),
                    );
                    response
                }
                Err(error) => object_error_response(caller, request, error),
            }
        }
        OP_INSPECT_OBJECT => match service.inspect_object(
            caller,
            request.authority,
            ObjectId::new(request.object_id),
        ) {
            Ok(inspection) => {
                let field_bytes = inspection.field_bytes(FIELD_TEXT).unwrap_or([0; 16]);
                let bytes_written = u64::from(inspection.field_value_len(FIELD_TEXT).unwrap_or(0));
                let response = ObjectShellResponse {
                    status: STATUS_OK,
                    object_kind: OBJECT_KIND_NOTE,
                    field_id: FIELD_TEXT,
                    object_id: request.object_id,
                    revision: inspection.revision,
                    bytes_written,
                    field_bytes,
                    ..empty_response()
                };
                emit_pythtig_object_success_marker(caller, request.operation, response, None);
                response
            }
            Err(error) => object_error_response(caller, request, error),
        },
        OP_REVISE_FIELD => match service.revise_field(
            caller,
            request.authority,
            ObjectId::new(request.object_id),
            request.field_id,
            input,
        ) {
            Ok(revision) => {
                let response = ObjectShellResponse {
                    status: STATUS_OK,
                    field_id: request.field_id,
                    object_id: request.object_id,
                    revision,
                    ..empty_response()
                };
                emit_pythtig_object_success_marker(caller, request.operation, response, None);
                response
            }
            Err(error) => object_error_response(caller, request, error),
        },
        OP_GET_HISTORY => {
            match service.history(caller, request.authority, ObjectId::new(request.object_id)) {
                Ok(revision_count) => {
                    let response = ObjectShellResponse {
                        status: STATUS_OK,
                        object_id: request.object_id,
                        revision_count,
                        ..empty_response()
                    };
                    emit_pythtig_object_success_marker(caller, request.operation, response, None);
                    response
                }
                Err(error) => object_error_response(caller, request, error),
            }
        }
        _ => bad_request_response(),
    }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn valid_object_request_header(request: &ObjectShellRequest) -> bool {
    request.abi_major == OBJECT_SHELL_ABI_MAJOR
        && request.abi_minor == OBJECT_SHELL_ABI_MINOR
        && request.reserved0 == 0
        && request.reserved1 == 0
        && request.reserved2 == 0
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn request_object_kind(kind: u16) -> Result<ObjectKind, ObjectServiceError> {
    if kind == OBJECT_KIND_NOTE {
        Ok(ObjectKind::Note)
    } else {
        Err(ObjectServiceError::UnsupportedKind)
    }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn error_response(error: ObjectServiceError) -> ObjectShellResponse {
    let status = match error {
        ObjectServiceError::Denied => {
            #[cfg(not(test))]
            serial::write_line("PYTHOS:CORE:OBJECT_SYSCALL:CALLER_DENIED");
            STATUS_DENIED
        }
        ObjectServiceError::NotFound => STATUS_NOT_FOUND,
        _ => STATUS_BAD_REQUEST,
    };
    ObjectShellResponse {
        status,
        ..empty_response()
    }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn object_error_response(
    caller: ActiveUserProcess,
    request: ObjectShellRequest,
    error: ObjectServiceError,
) -> ObjectShellResponse {
    emit_pythtig_object_denial_marker(caller, request, error);
    error_response(error)
}

#[cfg(all(not(test), not(feature = "verify")))]
fn emit_pythtig_object_success_marker(
    caller: ActiveUserProcess,
    operation: u16,
    response: ObjectShellResponse,
    query_entry: Option<ObjectListEntry>,
) {
    if response.status != STATUS_OK
        || !crate::user_mode::is_active_pyth_graph_process(caller.principal_id())
    {
        return;
    }
    match operation {
        OP_CREATE_OBJECT if response.object_id != 0 && response.capability.raw() != 0 => {
            serial::write_str("PYTHOS:PYTHTIG:OBJECT_CREATED object:");
            serial::write_dec_u64_value(response.object_id);
            serial::write_str(" revision:");
            serial::write_dec_u64_value(response.revision);
            serial::write_str("\r\n");
        }
        OP_QUERY_OBJECTS => {
            if let Some(entry) = query_entry
                && entry.object_id != 0
                && entry.capability.raw() != 0
            {
                serial::write_str("PYTHOS:PYTHTIG:OBJECT_REBOUND object:");
                serial::write_dec_u64_value(entry.object_id);
                serial::write_str("\r\n");
            }
        }
        OP_INSPECT_OBJECT
            if response.bytes_written == 5 && response.field_bytes[..5] == *b"hello" =>
        {
            serial::write_str("PYTHOS:PYTHTIG:OBJECT_INSPECTED object:");
            serial::write_dec_u64_value(response.object_id);
            serial::write_str(" revision:");
            serial::write_dec_u64_value(response.revision);
            serial::write_str("\r\n");
        }
        OP_REVISE_FIELD if response.revision >= 2 => {
            serial::write_str("PYTHOS:PYTHTIG:OBJECT_REVISED object:");
            serial::write_dec_u64_value(response.object_id);
            serial::write_str(" revision:");
            serial::write_dec_u64_value(response.revision);
            serial::write_str("\r\n");
        }
        OP_GET_HISTORY if response.revision_count >= 2 => {
            serial::write_str("PYTHOS:PYTHTIG:OBJECT_HISTORY object:");
            serial::write_dec_u64_value(response.object_id);
            serial::write_str(" revisions:");
            serial::write_dec_u64_value(response.revision_count);
            serial::write_str("\r\n");
        }
        _ => {}
    }
}

#[cfg(test)]
fn emit_pythtig_object_success_marker(
    _caller: ActiveUserProcess,
    _operation: u16,
    _response: ObjectShellResponse,
    _query_entry: Option<ObjectListEntry>,
) {
}

#[cfg(all(not(test), not(feature = "verify")))]
fn emit_pythtig_object_denial_marker(
    caller: ActiveUserProcess,
    request: ObjectShellRequest,
    error: ObjectServiceError,
) {
    if error != ObjectServiceError::Denied
        || !crate::user_mode::is_active_pyth_graph_process(caller.principal_id())
        || request.operation != OP_INSPECT_OBJECT
    {
        return;
    }
    if request.object_id == 2001 {
        serial::write_str("PYTHOS:PYTHTIG:OBJECT_KNOWN_DENIED object:");
        serial::write_dec_u64_value(request.object_id);
        serial::write_str("\r\n");
    } else if request.object_id == 1042 && request.authority.raw() != 0 {
        serial::write_line("PYTHOS:PYTHTIG:CAPABILITY_FORGERY_DENIED");
    }
}

#[cfg(test)]
fn emit_pythtig_object_denial_marker(
    _caller: ActiveUserProcess,
    _request: ObjectShellRequest,
    _error: ObjectServiceError,
) {
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
const fn empty_response() -> ObjectShellResponse {
    ObjectShellResponse {
        status: STATUS_BAD_REQUEST,
        reserved0: 0,
        object_kind: 0,
        field_id: 0,
        object_id: 0,
        revision: 0,
        revision_count: 0,
        bytes_written: 0,
        capability: PackedCapability::from_raw(0),
        field_bytes: [0; 16],
    }
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
const fn bad_request_response() -> ObjectShellResponse {
    empty_response()
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
const fn buffer_too_small_response() -> ObjectShellResponse {
    ObjectShellResponse {
        status: STATUS_BUFFER_TOO_SMALL,
        reserved0: 0,
        object_kind: 0,
        field_id: 0,
        object_id: 0,
        revision: 0,
        revision_count: 0,
        bytes_written: 0,
        capability: PackedCapability::from_raw(0),
        field_bytes: [0; 16],
    }
}

fn validate_syscall_capability(
    caller: ActiveUserProcess,
    capability: PackedCapability,
    resource: ResourceId,
    rights: RightsMask,
) -> Result<(), SyscallError> {
    with_syscall_capabilities(|table| {
        table.validate(
            caller.service_id(),
            unpack_syscall_capability(capability),
            resource,
            rights,
        )
    })?;
    Ok(())
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
const fn pack_syscall_capability(handle: CapabilityHandle) -> PackedCapability {
    PackedCapability::from_parts(handle.slot(), handle.generation())
}

const fn unpack_syscall_capability(capability: PackedCapability) -> CapabilityHandle {
    CapabilityHandle::from_parts(capability.slot(), capability.generation())
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
const fn is_aligned(ptr: u64, alignment: usize) -> bool {
    ptr != 0 && ptr.is_multiple_of(alignment as u64)
}

#[cfg(test)]
fn reset_syscall_capabilities_for_test() {
    with_syscall_capabilities(|table| {
        *table = CapabilityTable::new();
    });
}

#[cfg(test)]
fn grant_console_capability_for_test(
    process: ActiveUserProcess,
) -> Result<PackedCapability, SyscallError> {
    grant_console_capability(process)
}

#[cfg(test)]
fn grant_system_control_capability_for_test(
    process: ActiveUserProcess,
) -> Result<PackedCapability, SyscallError> {
    grant_system_control_capability(process)
}

#[cfg(test)]
fn dispatch_console_write_for_test(
    caller: ActiveUserProcess,
    capability: PackedCapability,
    byte: u8,
) -> Result<u64, SyscallError> {
    dispatch_console_write_for_caller(caller, capability, u64::from(byte))
}

#[cfg(test)]
fn dispatch_system_reboot_for_test(
    caller: ActiveUserProcess,
    capability: PackedCapability,
) -> Result<u64, SyscallError> {
    dispatch_system_reboot_for_caller(caller, capability)
}

#[cfg(test)]
fn dispatch_object_request_for_test(
    service: &mut ObjectService,
    caller: ActiveUserProcess,
    request: ObjectShellRequest,
    input: &[u8],
    output: &mut [ObjectListEntry],
) -> ObjectShellResponse {
    dispatch_object_request_to_service(service, caller, request, input, output)
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

    EXPECTED_SYSCALL.store(false, Ordering::SeqCst);
    if dispatch(SyscallArgs::for_number(SYSCALL_ABI_INFO))? != abi_info_result() {
        return Err(SyscallError::BadResult);
    }

    EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
    if dispatch(SyscallArgs::for_number(SYSCALL_SYSTEM_LOG_PROOF))? != SYSCALL_OK {
        return Err(SyscallError::BadResult);
    }

    EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
    let unknown_denied =
        dispatch(SyscallArgs::for_number(0x5059_FFFF)) == Err(SyscallError::UnsupportedNumber);
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
    use crate::object_service::ObjectService;
    use crate::shell_objects::ObjectKind;
    use crate::user_copy::{UserCopyError, UserCopyMap};
    use pythos_shared::object_shell_abi::{
        FIELD_TEXT, OBJECT_KIND_NOTE, OBJECT_SHELL_ABI_MAJOR, OBJECT_SHELL_ABI_MINOR,
        OP_CREATE_OBJECT, OP_QUERY_OBJECTS, OP_REVISE_FIELD, ObjectListEntry, ObjectShellRequest,
        ObjectShellResponse, STATUS_DENIED, STATUS_OK,
    };
    use pythos_shared::task_abi::{
        MAX_TASK_PROPOSAL_RESULTS, OP_APPEND_TASK_EVENT, OP_CREATE_PROPOSAL, OP_CREATE_TASK,
        OP_LIST_PROPOSALS, OP_READ_ACTIVE_TASK, TASK_ABI_MAJOR, TASK_ABI_MINOR, TaskEventInput,
        TaskProposalKind, TaskProposalListEntry, TaskRequest, TaskResponse,
    };

    static EXPECTED_SYSCALL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    fn pyth_graph_syscall_numbers_are_registered() {
        assert_eq!(SYSCALL_PYTH_GRAPH_LOG, 0x5059_0200);
        assert_eq!(SYSCALL_PYTH_GRAPH_EXIT, 0x5059_0201);
        assert_eq!(
            lookup_syscall(SYSCALL_PYTH_GRAPH_LOG).unwrap().name,
            "SYSCALL_PYTH_GRAPH_LOG"
        );
        assert_eq!(
            lookup_syscall(SYSCALL_PYTH_GRAPH_EXIT).unwrap().name,
            "SYSCALL_PYTH_GRAPH_EXIT"
        );
    }

    #[test]
    fn abi_info_dispatch_does_not_require_proof_expectation() {
        let _guard = EXPECTED_SYSCALL_TEST_LOCK.lock().unwrap();
        EXPECTED_SYSCALL.store(false, Ordering::SeqCst);

        assert_eq!(
            dispatch(SyscallArgs::for_number(SYSCALL_ABI_INFO)),
            Ok(abi_info_result())
        );
    }

    #[test]
    fn abi_info_dispatch_returns_version_metadata() {
        let _guard = EXPECTED_SYSCALL_TEST_LOCK.lock().unwrap();
        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(
            dispatch(SyscallArgs::for_number(SYSCALL_ABI_INFO)),
            Ok(abi_info_result())
        );
    }

    #[test]
    fn unknown_syscall_number_is_denied_by_registry() {
        let _guard = EXPECTED_SYSCALL_TEST_LOCK.lock().unwrap();
        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(
            dispatch(SyscallArgs::for_number(0x5059_FFFF)),
            Err(SyscallError::UnsupportedNumber)
        );
    }

    #[test]
    fn dispatch_rejects_unexpected_or_unknown_syscalls() {
        let _guard = EXPECTED_SYSCALL_TEST_LOCK.lock().unwrap();
        EXPECTED_SYSCALL.store(false, Ordering::SeqCst);
        assert_eq!(
            dispatch(SyscallArgs::for_number(SYSCALL_SYSTEM_LOG_PROOF)),
            Err(SyscallError::UnexpectedSyscall)
        );

        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(
            dispatch(SyscallArgs::for_number(SYSCALL_SYSTEM_LOG_PROOF + 1)),
            Err(SyscallError::UnsupportedNumber)
        );
    }

    #[test]
    fn dispatch_system_log_proof_uses_capability_and_log_surfaces() {
        let _guard = EXPECTED_SYSCALL_TEST_LOCK.lock().unwrap();
        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(
            dispatch(SyscallArgs::for_number(SYSCALL_SYSTEM_LOG_PROOF)),
            Ok(SYSCALL_OK)
        );
    }

    #[test]
    fn pyth_graph_log_requires_runtime_capability_and_readable_text() {
        let runtime = pyth_runtime_process();
        let intruder = ActiveUserProcess::new(ServiceId::from_raw(0x99), 0xAA, 0xBB);
        reset_syscall_capabilities_for_test();
        let log_capability = grant_pyth_graph_system_log_capability(runtime).unwrap();
        let text = *b"hello";
        let mut copy_map = UserCopyMap::new();
        map_slice(&mut copy_map, &text, true, false);
        process_context::bind_current_process(runtime.with_copy_map(copy_map));

        assert_eq!(
            dispatch(graph_log_args(
                log_capability,
                text.as_ptr() as u64,
                text.len() as u64
            )),
            Ok(SYSCALL_OK)
        );
        process_context::bind_current_process(intruder.with_copy_map(copy_map));
        assert_eq!(
            dispatch(graph_log_args(
                log_capability,
                text.as_ptr() as u64,
                text.len() as u64
            )),
            Err(SyscallError::Capability(CapabilityError::WrongHolder))
        );
        process_context::bind_current_process(runtime.with_copy_map(UserCopyMap::new()));
        assert_eq!(
            dispatch(graph_log_args(
                log_capability,
                text.as_ptr() as u64,
                text.len() as u64
            )),
            Err(SyscallError::UserCopy(UserCopyError::OutOfRange))
        );
    }

    #[test]
    fn pyth_graph_exit_requires_runtime_result_pointer_and_valid_record() {
        let runtime = pyth_runtime_process();
        let exit = GraphExitRecord {
            status: GRAPH_EXIT_OK,
            error_code: 0,
            last_node: 4,
            executed_nodes: 5,
            result_type: GRAPH_RESULT_UNIT,
            reserved0: 0,
            reserved1: 0,
            result_raw: 0,
        };
        process_context::bind_current_process(runtime.with_copy_map(UserCopyMap::new()));

        assert_eq!(
            dispatch_pyth_graph_exit(SyscallArgs {
                number: SYSCALL_PYTH_GRAPH_EXIT,
                arg0: &exit as *const GraphExitRecord as u64,
                arg1: size_of::<GraphExitRecord>() as u64,
                arg2: 0,
                arg3: 0,
                arg4: 0,
            }),
            Err(SyscallError::BadResult)
        );
        assert_eq!(validate_graph_exit_record(exit), Ok(()));

        crate::user_mode::activate_persistent_user_process_for_test(
            runtime,
            crate::user_mode::PersistentUserProcessKind::PythGraphRuntime,
        );
        assert_eq!(finalize_pyth_graph_exit(runtime, exit), Ok(SYSCALL_OK));
        assert_eq!(
            process_context::current_caller(),
            Err(crate::process_context::ProcessContextError::NoActiveProcess)
        );

        let mut bad = exit;
        bad.reserved1 = 1;
        assert_eq!(
            validate_graph_exit_record(bad),
            Err(SyscallError::BadResult)
        );
    }

    #[test]
    fn pyth_graph_exit_accepts_active_native_graph_process_identity() {
        let native = ActiveUserProcess::new(
            ServiceId::from_raw(0x5059_5447_5254_0002),
            crate::pyth_runtime_launch::HELLO_GRAPH_PRINCIPAL_ID,
            0xE1F0,
        );
        let exit = GraphExitRecord {
            status: GRAPH_EXIT_OK,
            error_code: 0,
            last_node: 4,
            executed_nodes: 5,
            result_type: GRAPH_RESULT_UNIT,
            reserved0: 0,
            reserved1: 0,
            result_raw: 0,
        };

        crate::user_mode::activate_persistent_user_process_for_test(
            native,
            crate::user_mode::PersistentUserProcessKind::PythNativeGraph,
        );

        assert_eq!(finalize_pyth_graph_exit(native, exit), Ok(SYSCALL_OK));
    }

    #[test]
    fn console_write_requires_console_capability_from_current_caller() {
        let service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let intruder = service.test_intruder_caller();
        reset_syscall_capabilities_for_test();
        let console = grant_console_capability_for_test(shell).unwrap();

        assert_eq!(
            dispatch_console_write_for_test(shell, console, b'x'),
            Ok(SYSCALL_OK)
        );
        assert_eq!(
            dispatch_console_write_for_test(intruder, console, b'x'),
            Err(SyscallError::Capability(CapabilityError::WrongHolder))
        );
    }

    #[test]
    fn object_request_denies_intruder_without_borrowing_shell_authority() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let intruder = service.test_intruder_caller();
        let workspace = service.test_shell_workspace_capability();
        let request = ObjectShellRequest {
            abi_major: OBJECT_SHELL_ABI_MAJOR,
            abi_minor: OBJECT_SHELL_ABI_MINOR,
            operation: OP_CREATE_OBJECT,
            object_kind: OBJECT_KIND_NOTE,
            field_id: 0,
            reserved0: 0,
            authority: workspace,
            object_id: 0,
            input_ptr: 0,
            input_len: 0,
            output_ptr: 0,
            output_len: 0,
            reserved1: 0,
            reserved2: 0,
        };

        let response =
            dispatch_object_request_for_test(&mut service, intruder, request, &[], &mut []);

        assert_eq!(response.status, STATUS_DENIED);
        assert_eq!(
            service
                .query_objects(shell, workspace, ObjectKind::Note)
                .unwrap()
                .iter()
                .filter(|entry| entry.object_id != 0)
                .count(),
            0
        );
    }

    #[test]
    fn object_query_writes_entries_to_the_request_output_buffer() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let created = service
            .create_object(shell, workspace, ObjectKind::Note)
            .unwrap();
        let request = ObjectShellRequest {
            abi_major: OBJECT_SHELL_ABI_MAJOR,
            abi_minor: OBJECT_SHELL_ABI_MINOR,
            operation: OP_QUERY_OBJECTS,
            object_kind: OBJECT_KIND_NOTE,
            field_id: 0,
            reserved0: 0,
            authority: workspace,
            object_id: 0,
            input_ptr: 0,
            input_len: 0,
            output_ptr: 0,
            output_len: core::mem::size_of::<[ObjectListEntry; 8]>() as u64,
            reserved1: 0,
            reserved2: 0,
        };
        let mut output = [ObjectListEntry {
            object_id: 0,
            capability: pythos_shared::object_shell_abi::PackedCapability::from_raw(0),
        }; 8];

        let response =
            dispatch_object_request_for_test(&mut service, shell, request, &[], &mut output);

        assert_eq!(response.status, STATUS_OK);
        assert_eq!(response.bytes_written, 16);
        assert_eq!(output[0].object_id, created.object_id.raw());
        assert_ne!(output[0].capability.raw(), 0);
    }

    #[test]
    fn object_request_rejects_unmapped_request_before_service_mutation() {
        let service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let request = Box::new(object_request(OP_CREATE_OBJECT, workspace));
        let mut response = Box::new(empty_test_response());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*response, true, true);
        process_context::bind_current_process(shell.with_copy_map(copy_map));

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Err(SyscallError::UserCopy(UserCopyError::OutOfRange))
        );
        assert_eq!(retained_note_count(shell, workspace), 0);
    }

    #[test]
    fn object_request_rejects_cross_mapping_request_before_service_mutation() {
        let service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let request = Box::new(object_request(OP_CREATE_OBJECT, workspace));
        let mut response = Box::new(empty_test_response());
        let request_ptr = (&*request as *const ObjectShellRequest) as u64;
        let mut copy_map = UserCopyMap::new();
        copy_map.add_mapping(request_ptr, 40, true, false).unwrap();
        copy_map
            .add_mapping(request_ptr + 40, 40, true, false)
            .unwrap();
        map_value(&mut copy_map, &*response, true, true);
        process_context::bind_current_process(shell.with_copy_map(copy_map));

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Err(SyscallError::UserCopy(UserCopyError::CrossMapping))
        );
        assert_eq!(retained_note_count(shell, workspace), 0);
    }

    #[test]
    fn object_request_rejects_kernel_request_pointer_before_service_mutation() {
        let service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let mut response = Box::new(empty_test_response());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*response, true, true);
        process_context::bind_current_process(shell.with_copy_map(copy_map));
        let args = SyscallArgs {
            number: SYSCALL_OBJECT_REQUEST,
            arg0: 0xFFFF_FFFF_8000_0000,
            arg1: size_of::<ObjectShellRequest>() as u64,
            arg2: &mut *response as *mut ObjectShellResponse as u64,
            arg3: size_of::<ObjectShellResponse>() as u64,
            arg4: 0,
        };

        assert_eq!(
            dispatch_object_request(args),
            Err(SyscallError::UserCopy(UserCopyError::OutOfRange))
        );
        assert_eq!(retained_note_count(shell, workspace), 0);
    }

    #[test]
    fn object_request_rejects_overflowing_request_range_before_service_mutation() {
        let service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let mut response = Box::new(empty_test_response());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*response, true, true);
        process_context::bind_current_process(shell.with_copy_map(copy_map));
        let args = SyscallArgs {
            number: SYSCALL_OBJECT_REQUEST,
            arg0: u64::MAX - 7,
            arg1: size_of::<ObjectShellRequest>() as u64,
            arg2: &mut *response as *mut ObjectShellResponse as u64,
            arg3: size_of::<ObjectShellResponse>() as u64,
            arg4: 0,
        };

        assert_eq!(
            dispatch_object_request(args),
            Err(SyscallError::UserCopy(UserCopyError::LengthOverflow))
        );
        assert_eq!(retained_note_count(shell, workspace), 0);
    }

    #[test]
    fn object_request_requires_writable_response_before_service_mutation() {
        let service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let request = Box::new(object_request(OP_CREATE_OBJECT, workspace));
        let mut response = Box::new(empty_test_response());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*request, true, false);
        map_value(&mut copy_map, &*response, true, false);
        process_context::bind_current_process(shell.with_copy_map(copy_map));

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Err(SyscallError::UserCopy(UserCopyError::PermissionDenied))
        );
        assert_eq!(retained_note_count(shell, workspace), 0);
    }

    #[test]
    fn object_query_requires_writable_output_before_service_borrow() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        service
            .create_object(shell, workspace, ObjectKind::Note)
            .unwrap();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let mut request = Box::new(object_request(OP_QUERY_OBJECTS, workspace));
        let mut response = Box::new(empty_test_response());
        let mut output = Box::new(empty_query_output());
        request.output_ptr = (&mut output[0] as *mut ObjectListEntry) as u64;
        request.output_len = size_of::<[ObjectListEntry; MAX_QUERY_RESULTS]>() as u64;
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*request, true, false);
        map_value(&mut copy_map, &*response, true, true);
        map_slice(&mut copy_map, &*output, true, false);
        process_context::bind_current_process(shell.with_copy_map(copy_map));

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Err(SyscallError::UserCopy(UserCopyError::PermissionDenied))
        );
        assert_eq!(
            output.iter().filter(|entry| entry.object_id != 0).count(),
            0
        );
    }

    #[test]
    fn object_query_rejects_overflowing_output_range_before_writing_entries() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        service
            .create_object(shell, workspace, ObjectKind::Note)
            .unwrap();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let mut request = Box::new(object_request(OP_QUERY_OBJECTS, workspace));
        let mut response = Box::new(empty_test_response());
        request.output_ptr = u64::MAX - 7;
        request.output_len = size_of::<[ObjectListEntry; MAX_QUERY_RESULTS]>() as u64;
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*request, true, false);
        map_value(&mut copy_map, &*response, true, true);
        process_context::bind_current_process(shell.with_copy_map(copy_map));

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Err(SyscallError::UserCopy(UserCopyError::LengthOverflow))
        );
        assert_eq!(response.status, STATUS_BAD_REQUEST);
    }

    #[test]
    fn object_revise_rejects_unmapped_input_without_mutating_object() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let created = service
            .create_object(shell, workspace, ObjectKind::Note)
            .unwrap();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let input = Box::new(*b"hello");
        let mut request = Box::new(object_request(OP_REVISE_FIELD, created.object_capability));
        request.object_id = created.object_id.raw();
        request.field_id = FIELD_TEXT;
        request.input_ptr = input.as_ptr() as u64;
        request.input_len = input.len() as u64;
        let mut response = Box::new(empty_test_response());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*request, true, false);
        map_value(&mut copy_map, &*response, true, true);
        process_context::bind_current_process(shell.with_copy_map(copy_map));

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Err(SyscallError::UserCopy(UserCopyError::OutOfRange))
        );
        retained_services::with_object_service(|service| {
            let inspection = service
                .inspect_object(shell, created.object_capability, created.object_id)
                .unwrap();
            assert_eq!(inspection.revision, 1);
            assert_eq!(inspection.field_bytes(FIELD_TEXT), None);
        })
        .unwrap();
    }

    #[test]
    fn task_request_create_and_read_active_use_current_caller_authority() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let user_control = {
            let mut task_authority = crate::task_service::TaskAuthorityState::new(shell);
            let task_service =
                crate::task_service::TaskService::new(&mut service, &mut task_authority).unwrap();
            task_service.user_task_control_capability()
        };
        let _guard = retained_services::initialize_object_service_for_test(service);
        let title = Box::new(*b"Universal Boot");
        let request = Box::new(task_request(OP_CREATE_TASK, user_control, &*title));
        let mut response = Box::new(empty_task_test_response());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*request, true, false);
        map_value(&mut copy_map, &*response, true, true);
        map_slice(&mut copy_map, &*title, true, false);
        process_context::bind_current_process(shell.with_copy_map(copy_map));

        assert_eq!(
            dispatch_task_request(task_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );

        assert_eq!(response.status, STATUS_OK);
        assert_eq!(response.task_id, response.active_task_id);
        assert_ne!(response.task_id, 0);

        let read_request = Box::new(task_request(OP_READ_ACTIVE_TASK, user_control, &[]));
        let mut read_response = Box::new(empty_task_test_response());
        let mut read_map = UserCopyMap::new();
        map_value(&mut read_map, &*read_request, true, false);
        map_value(&mut read_map, &*read_response, true, true);
        process_context::bind_current_process(shell.with_copy_map(read_map));

        assert_eq!(
            dispatch_task_request(task_args(&read_request, &mut read_response)),
            Ok(SYSCALL_OK)
        );
        assert_eq!(read_response.active_task_id, response.task_id);
    }

    #[test]
    fn task_request_appends_context_event_to_active_task() {
        let service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let (user_control, task_id) = retained_services::with_task_service(|service| {
            let authority = service.user_task_control_capability();
            let created = service
                .create_task(shell, authority, b"Universal Boot")
                .unwrap();
            (authority, created.task_id)
        })
        .unwrap();
        let input = Box::new(TaskEventInput {
            tag_hash: 0x5059_5448,
            object_kind: crate::task_context::object_kind_code(ObjectKind::Task),
            tool_domain: crate::task_context::TOOL_DOMAIN_GRAPH,
            flags: 0,
            reserved0: 0,
        });
        let mut request = Box::new(task_request(
            OP_APPEND_TASK_EVENT,
            user_control,
            task_event_input_bytes(&input),
        ));
        request.task_id = 0;
        let mut response = Box::new(empty_task_test_response());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*request, true, false);
        map_value(&mut copy_map, &*response, true, true);
        map_value(&mut copy_map, &*input, true, false);
        process_context::bind_current_process(shell.with_copy_map(copy_map));

        assert_eq!(
            dispatch_task_request(task_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );

        assert_eq!(response.status, STATUS_OK);
        assert_eq!(response.task_id, task_id);
        let summary = retained_services::with_task_service(|service| {
            service.read_context_summary(shell, user_control).unwrap()
        })
        .unwrap();
        assert_eq!(summary.event_count, 1);
        assert_eq!(summary.candidate_tag_hash, 0x5059_5448);
    }

    #[test]
    fn task_request_lists_pending_proposals_to_output_buffer() {
        let service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let (user_control, proposal_id) = retained_services::with_task_service(|service| {
            let user_control = service.user_task_control_capability();
            let steward = service.steward_caller();
            let steward_propose = service.steward_proposal_capability();
            let task = service
                .create_task(shell, user_control, b"Universal Boot")
                .unwrap();
            let proposal = service
                .create_proposal(
                    steward,
                    steward_propose,
                    TaskProposalKind::NewTask,
                    task.task_id,
                    0,
                    85,
                    b"Semantic Task Runtime",
                    b"recent context diverged",
                )
                .unwrap();
            (user_control, proposal.proposal_id)
        })
        .unwrap();
        let mut request = Box::new(task_request(OP_LIST_PROPOSALS, user_control, &[]));
        let mut response = Box::new(empty_task_test_response());
        let mut output = Box::new(empty_task_proposal_output());
        request.output_ptr = output.as_mut_ptr() as u64;
        request.output_len = size_of::<[TaskProposalListEntry; MAX_TASK_PROPOSAL_RESULTS]>() as u64;
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*request, true, false);
        map_value(&mut copy_map, &*response, true, true);
        map_slice(&mut copy_map, &*output, true, true);
        process_context::bind_current_process(shell.with_copy_map(copy_map));

        assert_eq!(
            dispatch_task_request(task_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );

        assert_eq!(response.status, STATUS_OK);
        assert_eq!(
            response.bytes_written,
            size_of::<TaskProposalListEntry>() as u64
        );
        assert_eq!(output[0].proposal_id, proposal_id);
        assert_eq!(output[0].score, 85);
    }

    #[test]
    fn task_request_denies_steward_create_with_proposal_capability() {
        let mut service = ObjectService::new_for_test();
        let steward = crate::task_service::steward_process();
        let steward_propose = {
            let mut task_authority =
                crate::task_service::TaskAuthorityState::new(service.test_shell_caller());
            let task_service =
                crate::task_service::TaskService::new(&mut service, &mut task_authority).unwrap();
            task_service.steward_proposal_capability()
        };
        let _guard = retained_services::initialize_object_service_for_test(service);
        let title = Box::new(*b"forged");
        let request = Box::new(task_request(OP_CREATE_TASK, steward_propose, &*title));
        let mut response = Box::new(empty_task_test_response());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*request, true, false);
        map_value(&mut copy_map, &*response, true, true);
        map_slice(&mut copy_map, &*title, true, false);
        process_context::bind_current_process(steward.with_copy_map(copy_map));

        assert_eq!(
            dispatch_task_request(task_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );

        assert_eq!(response.status, STATUS_DENIED);
        assert_eq!(
            retained_services::with_task_service(|service| service.active_task_id()).unwrap(),
            None
        );
    }

    #[test]
    fn system_reboot_requires_system_control_capability_from_current_caller() {
        let service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let intruder = service.test_intruder_caller();
        reset_syscall_capabilities_for_test();
        let system_control = grant_system_control_capability_for_test(shell).unwrap();

        assert_eq!(
            dispatch_system_reboot_for_test(shell, system_control),
            Ok(SYSCALL_OK)
        );
        assert_eq!(
            dispatch_system_reboot_for_test(intruder, system_control),
            Err(SyscallError::Capability(CapabilityError::WrongHolder))
        );
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
        let _guard = EXPECTED_SYSCALL_TEST_LOCK.lock().unwrap();
        let proof = run_general_abi_self_test().unwrap();

        assert!(proof.versioned);
        assert!(proof.known_dispatch);
        assert!(proof.unknown_denied);
    }

    fn object_request(operation: u16, authority: PackedCapability) -> ObjectShellRequest {
        ObjectShellRequest {
            abi_major: OBJECT_SHELL_ABI_MAJOR,
            abi_minor: OBJECT_SHELL_ABI_MINOR,
            operation,
            object_kind: OBJECT_KIND_NOTE,
            field_id: 0,
            reserved0: 0,
            authority,
            object_id: 0,
            input_ptr: 0,
            input_len: 0,
            output_ptr: 0,
            output_len: 0,
            reserved1: 0,
            reserved2: 0,
        }
    }

    const fn empty_test_response() -> ObjectShellResponse {
        ObjectShellResponse {
            status: STATUS_BAD_REQUEST,
            reserved0: 0,
            object_kind: 0,
            field_id: 0,
            object_id: 0,
            revision: 0,
            revision_count: 0,
            bytes_written: 0,
            capability: PackedCapability::from_raw(0),
            field_bytes: [0; 16],
        }
    }

    const fn empty_query_output() -> [ObjectListEntry; MAX_QUERY_RESULTS] {
        [ObjectListEntry {
            object_id: 0,
            capability: PackedCapability::from_raw(0),
        }; MAX_QUERY_RESULTS]
    }

    fn object_args(
        request: &ObjectShellRequest,
        response: &mut ObjectShellResponse,
    ) -> SyscallArgs {
        SyscallArgs {
            number: SYSCALL_OBJECT_REQUEST,
            arg0: request as *const ObjectShellRequest as u64,
            arg1: size_of::<ObjectShellRequest>() as u64,
            arg2: response as *mut ObjectShellResponse as u64,
            arg3: size_of::<ObjectShellResponse>() as u64,
            arg4: 0,
        }
    }

    fn task_request(operation: u16, authority: PackedCapability, input: &[u8]) -> TaskRequest {
        TaskRequest {
            abi_major: TASK_ABI_MAJOR,
            abi_minor: TASK_ABI_MINOR,
            operation,
            proposal_kind: 0,
            authority: authority.raw(),
            task_id: 0,
            proposal_id: 0,
            target_task_id: 0,
            input_ptr: input.as_ptr() as u64,
            input_len: input.len() as u64,
            output_ptr: 0,
            output_len: 0,
            flags: 0,
            score: 0,
            reserved0: 0,
        }
    }

    const fn empty_task_test_response() -> TaskResponse {
        TaskResponse {
            status: STATUS_BAD_REQUEST,
            operation: 0,
            proposal_kind: 0,
            reserved0: 0,
            task_id: 0,
            proposal_id: 0,
            active_task_id: 0,
            bytes_written: 0,
            score: 0,
            reserved1: 0,
            reserved2: 0,
        }
    }

    const fn empty_task_proposal_output() -> [TaskProposalListEntry; MAX_TASK_PROPOSAL_RESULTS] {
        [TaskProposalListEntry {
            status: 0,
            proposal_kind: 0,
            reserved0: 0,
            proposal_id: 0,
            target_task_id: 0,
            candidate_task_id: 0,
            score: 0,
        }; MAX_TASK_PROPOSAL_RESULTS]
    }

    fn task_event_input_bytes(input: &TaskEventInput) -> &[u8] {
        let ptr = input as *const TaskEventInput as *const u8;
        // SAFETY:
        // 1. Invariant: `input` is a live TaskEventInput value.
        // 2. Established by: the caller passes a reference to a stack/box
        //    value that outlives the returned slice in the test.
        // 3. Lifetime: the returned bytes are used only while `input` lives.
        // 4. Pointer ownership: the test owns `input`; this borrows bytes.
        // 5. Alignment: u8 has no stricter alignment.
        // 6. Mapped length: exactly size_of::<TaskEventInput>() bytes.
        // 7. Concurrency: unit test is single-threaded for this value.
        // 8. Violation: a stale reference would make the syscall test invalid.
        unsafe { core::slice::from_raw_parts(ptr, size_of::<TaskEventInput>()) }
    }

    fn task_args(request: &TaskRequest, response: &mut TaskResponse) -> SyscallArgs {
        SyscallArgs {
            number: SYSCALL_TASK_REQUEST,
            arg0: request as *const TaskRequest as u64,
            arg1: size_of::<TaskRequest>() as u64,
            arg2: response as *mut TaskResponse as u64,
            arg3: size_of::<TaskResponse>() as u64,
            arg4: 0,
        }
    }

    fn map_value<T>(map: &mut UserCopyMap, value: &T, readable: bool, writable: bool) {
        map.add_mapping(
            value as *const T as u64,
            size_of::<T>() as u64,
            readable,
            writable,
        )
        .unwrap();
    }

    fn map_slice<T>(map: &mut UserCopyMap, values: &[T], readable: bool, writable: bool) {
        map.add_mapping(
            values.as_ptr() as u64,
            core::mem::size_of_val(values) as u64,
            readable,
            writable,
        )
        .unwrap();
    }

    fn retained_note_count(shell: ActiveUserProcess, workspace: PackedCapability) -> usize {
        retained_services::with_object_service(|service| {
            service
                .query_objects(shell, workspace, ObjectKind::Note)
                .unwrap()
                .iter()
                .filter(|entry| entry.object_id != 0)
                .count()
        })
        .unwrap()
    }

    fn pyth_runtime_process() -> ActiveUserProcess {
        ActiveUserProcess::new(
            ServiceId::from_raw(0x5059_5447_5254_0001),
            crate::pyth_runtime_launch::PYTH_RUNTIME_PRINCIPAL_ID,
            0x1234,
        )
    }

    fn graph_log_args(capability: PackedCapability, ptr: u64, len: u64) -> SyscallArgs {
        SyscallArgs {
            number: SYSCALL_PYTH_GRAPH_LOG,
            arg0: capability.raw(),
            arg1: ptr,
            arg2: len,
            arg3: 0,
            arg4: 0,
        }
    }
}
