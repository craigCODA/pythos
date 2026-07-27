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
use crate::tasks::TaskId;
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
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:SYSCALL:ENTER");

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

    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:SYSCALL:RETURN");
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
            run_capability_gated_ipc_bridge()?;
            #[cfg(not(test))]
            serial::write_line("PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK");

            run_system_log_bridge()?;
            #[cfg(not(test))]
            serial::write_line("PYTHOS:CORE:SYSCALL:SYSTEM_LOG");
            Ok(SYSCALL_OK)
        }
        SyscallDispatchKind::ConsoleReadByte => dispatch_console_read(args),
        SyscallDispatchKind::ConsoleWriteByte => dispatch_console_write(args),
        SyscallDispatchKind::ObjectRequest => dispatch_object_request(args),
        SyscallDispatchKind::SystemReboot => dispatch_system_reboot(args),
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
        || !is_aligned(args.arg0, align_of::<ObjectShellRequest>())
        || !is_aligned(args.arg2, align_of::<ObjectShellResponse>())
    {
        return Err(SyscallError::BadResult);
    }
    let request_ptr = args.arg0 as *const ObjectShellRequest;
    let response_ptr = args.arg2 as *mut ObjectShellResponse;
    if request_ptr.is_null() || response_ptr.is_null() {
        return Err(SyscallError::BadResult);
    }
    // SAFETY:
    // 1. Invariant: `request_ptr` names a live, readable
    //    ObjectShellRequest supplied by the active user process for this
    //    syscall; `response_ptr` names a live, writable ObjectShellResponse.
    // 2. Established by: Task 7's fixed ABI length/alignment/null checks above
    //    and the ADR 0051 launch contract that maps these shell buffers in the
    //    active user address space. Full address-space map lookup lands with
    //    the shell execution slice.
    // 3. Lifetime: both buffers remain valid only for this syscall; neither
    //    pointer is retained after returning.
    // 4. Pointer ownership: the user process owns the buffers, PythCore only
    //    copies in/out.
    // 5. Alignment: checked against repr(C) alignment before dereference.
    // 6. Mapped length: checked against the exact ABI struct sizes.
    // 7. Concurrency: ADR 0051 shell is single-threaded and one syscall is
    //    handled at a time.
    // 8. Violation: a forged mapping could fault or corrupt user memory; Task 9
    //    retains fault containment and the upcoming launch slice supplies the
    //    scheduler-owned mapping source.
    let request = unsafe { request_ptr.read() };
    let response = dispatch_object_request_with_raw_buffers(caller, request)?;
    // SAFETY: see the invariant block above; this is the matching copy-out to
    // the already length/alignment/null-validated response pointer.
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
    request: ObjectShellRequest,
) -> Result<ObjectShellResponse, SyscallError> {
    if !valid_object_request_header(&request) {
        return Ok(bad_request_response());
    }
    let input = if request.operation == OP_REVISE_FIELD {
        checked_request_input(&request)?
    } else {
        &[]
    };
    retained_services::with_object_service(|service| {
        if request.operation == OP_QUERY_OBJECTS {
            let output_len = size_of::<[ObjectListEntry; MAX_QUERY_RESULTS]>() as u64;
            if request.output_len < output_len
                || !is_aligned(request.output_ptr, align_of::<ObjectListEntry>())
            {
                return buffer_too_small_response();
            }
            let output_ptr = request.output_ptr as *mut ObjectListEntry;
            if output_ptr.is_null() {
                return buffer_too_small_response();
            }
            // SAFETY:
            // 1. Invariant: query output points at a writable array of
            //    MAX_QUERY_RESULTS ObjectListEntry values in the caller's
            //    address space.
            // 2. Established by: fixed Task 7 ABI requires `output_len` at
            //    least the full array size and natural ObjectListEntry
            //    alignment; the shell supplies this buffer immediately before
            //    invoking SYSCALL_OBJECT_REQUEST.
            // 3. Lifetime: the slice is used only inside this syscall.
            // 4. Pointer ownership: user space owns the memory; PythCore writes
            //    bounded result entries and does not retain the pointer.
            // 5. Alignment: checked above.
            // 6. Mapped length: checked above against the full fixed array.
            // 7. Concurrency: shell is single-threaded in ADR 0051.
            // 8. Violation: a forged output mapping could fault; the future
            //    launch path supplies the authoritative user mapping.
            let output = unsafe { slice::from_raw_parts_mut(output_ptr, MAX_QUERY_RESULTS) };
            dispatch_object_request_to_service(service, caller, request, input, output)
        } else {
            dispatch_object_request_to_service(service, caller, request, input, &mut [])
        }
    })
    .map_err(SyscallError::from)
}

#[cfg(any(test, all(not(test), not(feature = "verify"))))]
fn checked_request_input(request: &ObjectShellRequest) -> Result<&[u8], SyscallError> {
    if request.input_len == 0 {
        return Ok(&[]);
    }
    if request.input_len > 16 || request.input_ptr == 0 {
        return Err(SyscallError::BadResult);
    }
    // SAFETY:
    // 1. Invariant: non-empty request input points at at most 16 readable bytes
    //    in the active shell process.
    // 2. Established by: Task 7 caps input_len at one TypedObjectField payload
    //    and rejects a null pointer before slicing.
    // 3. Lifetime: the returned slice is consumed during the current syscall
    //    and never stored.
    // 4. Pointer ownership: user space owns the input bytes; PythCore reads
    //    them only for typed object mutation.
    // 5. Alignment: byte slices require no stricter alignment than 1.
    // 6. Mapped length: bounded to 16 bytes by the check above.
    // 7. Concurrency: shell is single-threaded in ADR 0051.
    // 8. Violation: a forged input mapping could fault; the upcoming launch
    //    slice provides scheduler-owned user mapping validation.
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
    }
    Ok(SYSCALL_OK)
}

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
            Ok(created) => ObjectShellResponse {
                status: STATUS_OK,
                object_kind: request.object_kind,
                object_id: created.object_id.raw(),
                revision: created.revision,
                capability: created.object_capability,
                ..empty_response()
            },
            Err(error) => error_response(error),
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
                    ObjectShellResponse {
                        status: STATUS_OK,
                        object_kind: request.object_kind,
                        bytes_written: (count * size_of::<ObjectListEntry>()) as u64,
                        ..empty_response()
                    }
                }
                Err(error) => error_response(error),
            }
        }
        OP_INSPECT_OBJECT => match service.inspect_object(
            caller,
            request.authority,
            ObjectId::new(request.object_id),
        ) {
            Ok(inspection) => ObjectShellResponse {
                status: STATUS_OK,
                object_kind: OBJECT_KIND_NOTE,
                field_id: FIELD_TEXT,
                object_id: request.object_id,
                revision: inspection.revision,
                ..empty_response()
            },
            Err(error) => error_response(error),
        },
        OP_REVISE_FIELD => match service.revise_field(
            caller,
            request.authority,
            ObjectId::new(request.object_id),
            request.field_id,
            input,
        ) {
            Ok(revision) => ObjectShellResponse {
                status: STATUS_OK,
                field_id: request.field_id,
                object_id: request.object_id,
                revision,
                ..empty_response()
            },
            Err(error) => error_response(error),
        },
        OP_GET_HISTORY => {
            match service.history(caller, request.authority, ObjectId::new(request.object_id)) {
                Ok(revision_count) => ObjectShellResponse {
                    status: STATUS_OK,
                    object_id: request.object_id,
                    revision_count,
                    ..empty_response()
                },
                Err(error) => error_response(error),
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
        reserved1: 0,
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
        reserved1: 0,
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
    use pythos_shared::object_shell_abi::{
        OBJECT_KIND_NOTE, OBJECT_SHELL_ABI_MAJOR, OBJECT_SHELL_ABI_MINOR, OP_CREATE_OBJECT,
        OP_QUERY_OBJECTS, ObjectListEntry, ObjectShellRequest, STATUS_DENIED, STATUS_OK,
    };

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
    fn abi_info_dispatch_does_not_require_proof_expectation() {
        EXPECTED_SYSCALL.store(false, Ordering::SeqCst);

        assert_eq!(
            dispatch(SyscallArgs::for_number(SYSCALL_ABI_INFO)),
            Ok(abi_info_result())
        );
    }

    #[test]
    fn abi_info_dispatch_returns_version_metadata() {
        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(
            dispatch(SyscallArgs::for_number(SYSCALL_ABI_INFO)),
            Ok(abi_info_result())
        );
    }

    #[test]
    fn unknown_syscall_number_is_denied_by_registry() {
        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(
            dispatch(SyscallArgs::for_number(0x5059_FFFF)),
            Err(SyscallError::UnsupportedNumber)
        );
    }

    #[test]
    fn dispatch_rejects_unexpected_or_unknown_syscalls() {
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
        EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
        assert_eq!(
            dispatch(SyscallArgs::for_number(SYSCALL_SYSTEM_LOG_PROOF)),
            Ok(SYSCALL_OK)
        );
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
        let proof = run_general_abi_self_test().unwrap();

        assert!(proof.versioned);
        assert!(proof.known_dispatch);
        assert!(proof.unknown_denied);
    }
}
