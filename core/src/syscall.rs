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
#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
use crate::object_service::{ObjectService, ObjectServiceError, PackageDefinedCreateInput};
#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
use crate::package_service;
#[cfg(test)]
use crate::package_service::PackageService;
use crate::permission_validation::{self, PermissionError};
use crate::process_context::{self, ActiveUserProcess, ProcessContextError};
#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
use crate::retained_services::{self, RetainedServiceError};
#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityTable};
use crate::session_input::{self, SessionInputError};
#[cfg(any(
    test,
    feature = "session-viewing-probe",
    all(feature = "normal-session", not(feature = "verify"))
))]
use crate::session_presentation::{self, PresentationError};
#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
use crate::shell_objects::{ObjectId, ObjectKind};
use crate::system_api::{SystemApiError, SystemApiHost};
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use crate::task_context::TaskContextEvent;
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use crate::task_service::{self, TaskServiceError};
use crate::tasks::TaskId;
use crate::user_copy::{UserCopyAccess, UserCopyError, UserCopyMap};
use crate::user_mode;
use crate::value_validation::{HostCallResult, UntrustedRuntimeValue};
#[cfg(not(test))]
use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::mem::{align_of, size_of};
#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe",
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
use core::slice;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
use pythos_shared::network_port_abi::{
    NETWORK_PORT_ABI_MAJOR, NETWORK_PORT_ABI_MINOR, NETWORK_PORT_MAX_FRAME_BYTES,
    NETWORK_PORT_MIN_FRAME_BYTES, NETWORK_PORT_OP_DESCRIBE, NETWORK_PORT_OP_RESET,
    NETWORK_PORT_OP_SEND, NETWORK_PORT_OP_TRY_RECEIVE, NETWORK_PORT_STATUS_BAD_REQUEST,
    NETWORK_PORT_STATUS_BUFFER_TOO_SMALL, NETWORK_PORT_STATUS_DENIED, NETWORK_PORT_STATUS_OK,
    NetworkPortDescriptionV1, NetworkPortRequestV1, NetworkPortResponseV1,
};
use pythos_shared::normal_session_abi::{
    SESSION_WAIT_CONSOLE_READY, SESSION_WAIT_INPUT_READY, SYSCALL_SESSION_WAIT,
};
#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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
#[cfg(any(
    test,
    all(not(test), not(feature = "verify")),
    all(not(test), feature = "phase13-package-test")
))]
use pythos_shared::package_abi::OP_PACKAGE_CONTEXT_SCHEMA;
#[cfg(any(
    test,
    all(not(test), not(feature = "verify")),
    all(not(test), feature = "phase13-package-test")
))]
use pythos_shared::package_abi::PackageRuntimeSchemaBindingV0;
#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
use pythos_shared::package_abi::{
    OBJECT_KIND_PACKAGE_DEFINED_OBJECT, PACKAGE_DEFINED_MAX_INITIAL_STATE_BYTES,
    PACKAGE_DEFINED_OBJECT_CREATE_ABI_MAJOR, PACKAGE_DEFINED_OBJECT_CREATE_ABI_MINOR,
    PACKAGE_DEFINED_STATE_FORMAT_EMPTY, PACKAGE_DEFINED_STATE_FORMAT_INLINE_BYTES_V0,
    PackageDefinedObjectCreateV0,
};
use pythos_shared::package_abi::{PackageStatus, SYSCALL_PACKAGE_CONTEXT};
#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
use pythos_shared::pyth_runtime_abi::{
    GRAPH_EXIT_BUDGET_EXHAUSTED, GRAPH_EXIT_OK, GRAPH_EXIT_RUNTIME_ERROR, GRAPH_MAX_LOG_BYTES,
    GRAPH_RESULT_UNIT, GraphExitRecord,
};
use pythos_shared::pyth_runtime_abi::{SYSCALL_PYTH_GRAPH_EXIT, SYSCALL_PYTH_GRAPH_LOG};
use pythos_shared::session_input_abi::{
    SESSION_INPUT_RESOURCE_ID, SESSION_INPUT_RESULT_EMPTY, SESSION_INPUT_RESULT_EVENT,
    SYSCALL_SESSION_INPUT_TRY_READ, SessionInputEventV1,
};
#[cfg(any(test, feature = "session-runtime-probe"))]
use pythos_shared::session_runtime_abi::SESSION_COMMAND_RESOURCE_ID;
#[cfg(any(
    test,
    feature = "session-viewing-probe",
    all(feature = "normal-session", not(feature = "verify"))
))]
use pythos_shared::session_viewing_abi::{
    SESSION_VIEWING_RESOURCE_ID, SYSCALL_SESSION_VIEWING_PRESENT,
};
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use pythos_shared::task_abi::{
    MAX_TASK_PROPOSAL_RESULTS, OP_ABANDON_TASK, OP_APPEND_TASK_EVENT, OP_APPROVE_PROPOSAL,
    OP_COMPLETE_TASK, OP_CREATE_PROPOSAL, OP_CREATE_TASK, OP_LIST_PROPOSALS, OP_READ_ACTIVE_TASK,
    OP_READ_CONTEXT_SUMMARY, OP_REJECT_PROPOSAL, OP_REVIVE_TASK, OP_SUSPEND_TASK,
    SYSCALL_TASK_REQUEST, TASK_ABI_MAJOR, TASK_ABI_MINOR, TASK_REQUEST_SUSPEND_CURRENT,
    TaskContextSummary, TaskEventInput, TaskProposalListEntry, TaskRequest, TaskResponse,
};

pub const SYSCALL_ABI_MAJOR: u16 = 1;
pub const SYSCALL_ABI_MINOR: u16 = 2;
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
const USER_VIRT_MIN: u64 = 0x0020_0000;
const USER_VIRT_MAX: u64 = 0x0000_8000_0000_0000;
#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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
    SessionInput(SessionInputError),
    #[cfg(any(
        test,
        feature = "session-viewing-probe",
        all(feature = "normal-session", not(feature = "verify"))
    ))]
    SessionPresentation(PresentationError),
    UserCopy(UserCopyError),
    #[cfg(any(
        test,
        all(
            not(test),
            any(not(feature = "verify"), feature = "phase13-package-test")
        )
    ))]
    ObjectService(ObjectServiceError),
    #[cfg(any(
        test,
        all(
            not(test),
            any(not(feature = "verify"), feature = "phase13-package-test")
        )
    ))]
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
    PackageContext,
    SessionInputTryRead,
    SessionWait,
    NetworkPort,
    #[cfg(any(
        test,
        feature = "session-viewing-probe",
        all(feature = "normal-session", not(feature = "verify"))
    ))]
    SessionViewingPresent,
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
        number: SYSCALL_SESSION_INPUT_TRY_READ,
        name: "SYSCALL_SESSION_INPUT_TRY_READ",
        introduced_major: 1,
        introduced_minor: 1,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::SessionInputTryRead,
    },
    #[cfg(any(
        test,
        feature = "session-viewing-probe",
        all(feature = "normal-session", not(feature = "verify"))
    ))]
    SyscallEntry {
        number: SYSCALL_SESSION_VIEWING_PRESENT,
        name: "SYSCALL_SESSION_VIEWING_PRESENT",
        introduced_major: 1,
        introduced_minor: 1,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::SessionViewingPresent,
    },
    SyscallEntry {
        number: SYSCALL_SESSION_WAIT,
        name: "SYSCALL_SESSION_WAIT",
        introduced_major: pythos_shared::normal_session_abi::SYSCALL_ABI_MAJOR,
        introduced_minor: pythos_shared::normal_session_abi::SYSCALL_ABI_MINOR,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::SessionWait,
    },
    SyscallEntry {
        number: pythos_shared::network_port_abi::SYSCALL_NETWORK_PORT_REQUEST,
        name: "SYSCALL_NETWORK_PORT_REQUEST",
        introduced_major: 1,
        introduced_minor: 2,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::NetworkPort,
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
    SyscallEntry {
        number: SYSCALL_PACKAGE_CONTEXT,
        name: "SYSCALL_PACKAGE_CONTEXT",
        introduced_major: 1,
        introduced_minor: 0,
        proof_only: false,
        dispatch_kind: SyscallDispatchKind::PackageContext,
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

impl From<SessionInputError> for SyscallError {
    fn from(error: SessionInputError) -> Self {
        Self::SessionInput(error)
    }
}

impl From<UserCopyError> for SyscallError {
    fn from(error: UserCopyError) -> Self {
        Self::UserCopy(error)
    }
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
impl From<ObjectServiceError> for SyscallError {
    fn from(error: ObjectServiceError) -> Self {
        Self::ObjectService(error)
    }
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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
        SyscallDispatchKind::PackageContext => dispatch_package_context(args),
        SyscallDispatchKind::SessionInputTryRead => dispatch_session_input_try_read(args),
        SyscallDispatchKind::SessionWait => dispatch_session_wait(args),
        SyscallDispatchKind::NetworkPort => dispatch_network_port(args),
        #[cfg(any(
            test,
            feature = "session-viewing-probe",
            all(feature = "normal-session", not(feature = "verify"))
        ))]
        SyscallDispatchKind::SessionViewingPresent => with_syscall_capabilities(|table| {
            dispatch_session_viewing_present_with_table(args, table, session_presentation::present)
        }),
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

#[cfg(any(
    test,
    feature = "session-input-bridge-probe",
    feature = "session-runtime-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe",
    all(not(test), not(feature = "verify"))
))]
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

#[cfg(any(test, feature = "session-runtime-probe"))]
pub fn grant_session_command_capability(
    process: ActiveUserProcess,
) -> Result<PackedCapability, SyscallError> {
    with_syscall_capabilities(|table| grant_session_command_capability_with_table(table, process))
}

#[cfg(any(test, feature = "session-runtime-probe"))]
fn grant_session_command_capability_with_table(
    table: &mut CapabilityTable,
    process: ActiveUserProcess,
) -> Result<PackedCapability, SyscallError> {
    let handle = table.grant(
        process.service_id(),
        ResourceId::new(SESSION_COMMAND_RESOURCE_ID),
        RightsMask::new(RightsMask::READ | RightsMask::APPEND),
    )?;
    Ok(pack_syscall_capability(handle))
}

/// Grants the sole session-input capability and binds its holder to the
/// device-neutral queue while producers are quiescent.
pub fn bind_session_input_capability(
    process: ActiveUserProcess,
) -> Result<PackedCapability, SyscallError> {
    with_syscall_capabilities(|table| {
        bind_session_input_capability_with_table(
            table,
            process,
            session_input::bind_session_consumer_quiescent,
        )
    })
}

fn bind_session_input_capability_with_table(
    table: &mut CapabilityTable,
    process: ActiveUserProcess,
    bind_session_consumer: impl FnOnce(ServiceId) -> Result<(), SessionInputError>,
) -> Result<PackedCapability, SyscallError> {
    let grant = table.grant_with_provenance(
        process.service_id(),
        ResourceId::new(SESSION_INPUT_RESOURCE_ID),
        RightsMask::new(RightsMask::INPUT),
    )?;
    let handle = grant.handle();
    if let Err(error) = bind_session_consumer(process.service_id()) {
        if grant.is_created()
            && let Err(rollback_error) = table.revoke(handle)
        {
            // A freshly-created handle is exclusively borrowed here, so this
            // is unreachable under CapabilityTable's contract. Do not report
            // a misleading session-bind failure if that contract is broken.
            return Err(SyscallError::Capability(rollback_error));
        }
        return Err(SyscallError::SessionInput(error));
    }
    Ok(pack_syscall_capability(handle))
}

// The Session Manager composition slice calls this exported bootstrap hook;
// retain the function item in this delivery-only slice without invoking a
// producer-side bind from an unrelated syscall path.
const _: fn(ActiveUserProcess) -> Result<PackedCapability, SyscallError> =
    bind_session_input_capability;

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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

#[cfg(any(test, all(not(test), feature = "phase13-package-test")))]
/// Issues the Phase 13 package-launch graph-log grant and exposes the same
/// table for immediate `PackageService::launch` validation.
pub fn with_pyth_graph_system_log_launch_capability<R>(
    process: ActiveUserProcess,
    f: impl FnOnce(CapabilityHandle, &CapabilityTable) -> R,
) -> Result<R, SyscallError> {
    with_syscall_capabilities(|table| {
        let handle = table.grant(
            process.service_id(),
            PYTH_GRAPH_SYSTEM_LOG_RESOURCE,
            RightsMask::new(RightsMask::LOG),
        )?;
        Ok(f(handle, table))
    })
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
        let com2 = serial::try_read_byte_com2();
        Ok(console_read_result(com2, || {
            #[cfg(feature = "physical-keyboard-console")]
            {
                crate::physical_keyboard_console::poll_console_byte()
            }
            #[cfg(not(feature = "physical-keyboard-console"))]
            {
                None
            }
        }))
    }
    #[cfg(test)]
    {
        Ok(console_read_result(None, || None))
    }
}

#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
fn dispatch_network_port_with_port<T: crate::network_port::NetworkTransport>(
    args: SyscallArgs,
    caller: ActiveUserProcess,
    capabilities: &mut CapabilityTable,
    port: &mut crate::network_port::NetworkPort<T>,
) -> Result<u64, SyscallError> {
    if args.arg1 != size_of::<NetworkPortRequestV1>() as u64
        || args.arg3 != size_of::<NetworkPortResponseV1>() as u64
        || args.arg4 != 0
    {
        return Err(SyscallError::BadResult);
    }
    let copy_map = caller.copy_map();
    validate_user_buffer(
        &copy_map,
        args.arg0,
        args.arg1,
        align_of::<NetworkPortRequestV1>(),
        UserCopyAccess::Read,
    )?;
    validate_user_buffer(
        &copy_map,
        args.arg2,
        args.arg3,
        align_of::<NetworkPortResponseV1>(),
        UserCopyAccess::Write,
    )?;

    let request_ptr = args.arg0 as *const NetworkPortRequestV1;
    let response_ptr = args.arg2 as *mut NetworkPortResponseV1;
    // SAFETY:
    // 1. Invariant: request_ptr names one readable NetworkPortRequestV1 in
    //    the active caller's UserCopyMap.
    // 2. Established by: exact syscall length, natural alignment, and
    //    readable mapping validation above.
    // 3. Lifetime: copied by value for this synchronous syscall only.
    // 4. Pointer ownership: caller owns the request; PythCore retains none.
    // 5. Alignment: checked against NetworkPortRequestV1 alignment.
    // 6. Mapped length: exactly size_of::<NetworkPortRequestV1>().
    // 7. Concurrency: this slice admits one active syscall on one CPU.
    // 8. Violation: stale map authority could read unrelated user memory.
    let request = unsafe { request_ptr.read() };
    let response = dispatch_network_port_request(capabilities, caller, &copy_map, port, request);
    // SAFETY:
    // 1. Invariant: response_ptr names one writable NetworkPortResponseV1 in
    //    the active caller's UserCopyMap.
    // 2. Established by: exact syscall length, natural alignment, and writable
    //    mapping validation above before any response is constructed.
    // 3. Lifetime: one by-value copy-out occurs before the syscall returns.
    // 4. Pointer ownership: caller owns the response buffer; PythCore retains none.
    // 5. Alignment: checked against NetworkPortResponseV1 alignment.
    // 6. Mapped length: exactly size_of::<NetworkPortResponseV1>().
    // 7. Concurrency: this slice admits one active syscall on one CPU.
    // 8. Violation: stale map authority could corrupt unrelated user memory.
    unsafe { response_ptr.write(response) };
    Ok(SYSCALL_OK)
}

#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
fn dispatch_network_port(args: SyscallArgs) -> Result<u64, SyscallError> {
    let caller = process_context::current_caller()?;
    with_syscall_capabilities(|capabilities| {
        crate::network_port::with_active_port(|port| {
            dispatch_network_port_with_port(args, caller, capabilities, port)
        })
        .unwrap_or(Err(SyscallError::BadResult))
    })
}

#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
pub(crate) fn bind_network_port_capabilities(
    consumer_holder: ActiveUserProcess,
    consumer: PackedCapability,
    owner_holder: ServiceId,
    owner: PackedCapability,
) -> Result<(), SyscallError> {
    with_syscall_capabilities(|capabilities| {
        crate::network_port::with_active_port(|port| {
            bind_network_port_capabilities_with_table(
                capabilities,
                port,
                consumer_holder,
                consumer,
                owner_holder,
                owner,
            )
        })
        .unwrap_or(Err(SyscallError::BadResult))
    })
}

/// Issue the two boot-only grants for a NetworkPort consumer. The consumer
/// receives one combined READ|SEND handle; the kernel-only owner receives the
/// sole WRITE handle used for terminal teardown.
#[cfg(any(
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
pub(crate) fn grant_network_port_consumer_capabilities(
    consumer: ActiveUserProcess,
    owner: ServiceId,
) -> Result<(PackedCapability, PackedCapability), SyscallError> {
    with_syscall_capabilities(|capabilities| {
        crate::network_port::with_active_port(|port| {
            let consumer = capabilities.grant(
                consumer.service_id(),
                port.resource(),
                RightsMask::new(RightsMask::READ | RightsMask::SEND),
            )?;
            let owner =
                capabilities.grant(owner, port.resource(), RightsMask::new(RightsMask::WRITE))?;
            Ok((
                pack_syscall_capability(consumer),
                pack_syscall_capability(owner),
            ))
        })
        .unwrap_or(Err(SyscallError::BadResult))
    })
}

/// Exercise the owner-only terminal transition after both ring-3 launches.
#[cfg(any(
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
pub(crate) fn teardown_network_port_capabilities(
    owner_holder: ServiceId,
    owner: PackedCapability,
) -> Result<NetworkPortResponseV1, SyscallError> {
    with_syscall_capabilities(|capabilities| {
        crate::network_port::with_active_port(|port| {
            capabilities.validate(
                owner_holder,
                unpack_syscall_capability(owner),
                port.resource(),
                RightsMask::new(RightsMask::WRITE),
            )?;
            Ok(port.reset(capabilities))
        })
        .unwrap_or(Err(SyscallError::BadResult))
    })
}

/// Confirms that a terminal port transition revoked the consumer authority.
#[cfg(any(
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
pub(crate) fn network_port_consumer_revoked(
    consumer: ActiveUserProcess,
    capability: PackedCapability,
) -> Result<bool, SyscallError> {
    with_syscall_capabilities(|capabilities| {
        crate::network_port::with_active_port(|port| {
            Ok(capabilities
                .validate(
                    consumer.service_id(),
                    unpack_syscall_capability(capability),
                    port.resource(),
                    RightsMask::new(RightsMask::READ | RightsMask::SEND),
                )
                .is_err())
        })
        .unwrap_or(Err(SyscallError::BadResult))
    })
}

#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
fn bind_network_port_capabilities_with_table<T: crate::network_port::NetworkTransport>(
    capabilities: &CapabilityTable,
    port: &mut crate::network_port::NetworkPort<T>,
    consumer_holder: ActiveUserProcess,
    consumer: PackedCapability,
    owner_holder: ServiceId,
    owner: PackedCapability,
) -> Result<(), SyscallError> {
    validate_syscall_capability_with_table(
        capabilities,
        consumer_holder,
        consumer,
        port.resource(),
        RightsMask::new(RightsMask::READ | RightsMask::SEND),
    )?;
    capabilities.validate(
        owner_holder,
        unpack_syscall_capability(owner),
        port.resource(),
        RightsMask::new(RightsMask::WRITE),
    )?;
    port.bind_capabilities(
        unpack_syscall_capability(consumer),
        unpack_syscall_capability(owner),
    )
    .map_err(|_| SyscallError::BadResult)
}

#[cfg(not(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
)))]
fn dispatch_network_port(_args: SyscallArgs) -> Result<u64, SyscallError> {
    // The capability is never granted and no transport is installed outside
    // the opt-in profile; Task 4 supplies that profile without changing the
    // default or normal-session boot path.
    Err(SyscallError::BadResult)
}

#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
fn dispatch_network_port_request<T: crate::network_port::NetworkTransport>(
    capabilities: &mut CapabilityTable,
    caller: ActiveUserProcess,
    copy_map: &UserCopyMap,
    port: &mut crate::network_port::NetworkPort<T>,
    request: NetworkPortRequestV1,
) -> NetworkPortResponseV1 {
    if !valid_network_port_request_header(&request) {
        return network_port_response(NETWORK_PORT_STATUS_BAD_REQUEST, port.state());
    }
    let required_right = match request.operation {
        NETWORK_PORT_OP_DESCRIBE | NETWORK_PORT_OP_TRY_RECEIVE => RightsMask::new(RightsMask::READ),
        NETWORK_PORT_OP_SEND => RightsMask::new(RightsMask::SEND),
        NETWORK_PORT_OP_RESET => RightsMask::new(RightsMask::WRITE),
        _ => return network_port_response(NETWORK_PORT_STATUS_BAD_REQUEST, port.state()),
    };
    if validate_syscall_capability_with_table(
        capabilities,
        caller,
        request.authority,
        port.resource(),
        required_right,
    )
    .is_err()
    {
        return network_port_response(NETWORK_PORT_STATUS_DENIED, port.state());
    }
    if let Some(response) = port.service_response() {
        return response;
    }

    match request.operation {
        NETWORK_PORT_OP_DESCRIBE => dispatch_network_port_describe(copy_map, port, &request),
        NETWORK_PORT_OP_SEND => dispatch_network_port_send(copy_map, capabilities, port, &request),
        NETWORK_PORT_OP_TRY_RECEIVE => {
            dispatch_network_port_receive(copy_map, capabilities, port, &request)
        }
        NETWORK_PORT_OP_RESET => {
            if request.input_ptr != 0
                || request.input_len != 0
                || request.output_ptr != 0
                || request.output_len != 0
            {
                network_port_response(NETWORK_PORT_STATUS_BAD_REQUEST, port.state())
            } else {
                port.reset(capabilities)
            }
        }
        _ => network_port_response(NETWORK_PORT_STATUS_BAD_REQUEST, port.state()),
    }
}

#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
fn valid_network_port_request_header(request: &NetworkPortRequestV1) -> bool {
    request.abi_major == NETWORK_PORT_ABI_MAJOR
        && request.abi_minor == NETWORK_PORT_ABI_MINOR
        && request.flags == 0
        && request.reserved0 == 0
        && request.reserved1 == 0
        && request.reserved2 == 0
        && request.reserved3 == 0
}

#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
fn dispatch_network_port_describe<T: crate::network_port::NetworkTransport>(
    copy_map: &UserCopyMap,
    port: &mut crate::network_port::NetworkPort<T>,
    request: &NetworkPortRequestV1,
) -> NetworkPortResponseV1 {
    let description_len = size_of::<NetworkPortDescriptionV1>() as u64;
    if request.input_ptr != 0 || request.input_len != 0 {
        return network_port_response(NETWORK_PORT_STATUS_BAD_REQUEST, port.state());
    }
    if request.output_len < description_len {
        let mut response =
            network_port_response(NETWORK_PORT_STATUS_BUFFER_TOO_SMALL, port.state());
        response.required_len = description_len;
        return response;
    }
    if request.output_len != description_len
        || validate_user_buffer(
            copy_map,
            request.output_ptr,
            request.output_len,
            align_of::<NetworkPortDescriptionV1>(),
            UserCopyAccess::Write,
        )
        .is_err()
    {
        return network_port_response(NETWORK_PORT_STATUS_BAD_REQUEST, port.state());
    }
    let description_ptr = request.output_ptr as *mut NetworkPortDescriptionV1;
    let description = port.describe();
    // SAFETY: output_ptr is an exactly-sized, aligned writable description
    // mapping validated immediately above; the value is copied once and the
    // pointer is not retained.
    unsafe { description_ptr.write(description) };
    network_port_response(NETWORK_PORT_STATUS_OK, port.state())
}

#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
fn dispatch_network_port_send<T: crate::network_port::NetworkTransport>(
    copy_map: &UserCopyMap,
    capabilities: &mut CapabilityTable,
    port: &mut crate::network_port::NetworkPort<T>,
    request: &NetworkPortRequestV1,
) -> NetworkPortResponseV1 {
    if request.output_ptr != 0
        || request.output_len != 0
        || !(NETWORK_PORT_MIN_FRAME_BYTES as u64..=NETWORK_PORT_MAX_FRAME_BYTES as u64)
            .contains(&request.input_len)
        || validate_user_buffer(
            copy_map,
            request.input_ptr,
            request.input_len,
            1,
            UserCopyAccess::Read,
        )
        .is_err()
    {
        return network_port_response(NETWORK_PORT_STATUS_BAD_REQUEST, port.state());
    }
    // SAFETY: input_ptr is a nonzero readable byte range of the validated
    // frame length in the caller's one mapping; PythCore borrows it only for
    // this synchronous copy-oriented transport operation and retains nothing.
    let frame = unsafe {
        slice::from_raw_parts(request.input_ptr as *const u8, request.input_len as usize)
    };
    port.send(frame, capabilities)
}

#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
fn dispatch_network_port_receive<T: crate::network_port::NetworkTransport>(
    copy_map: &UserCopyMap,
    capabilities: &mut CapabilityTable,
    port: &mut crate::network_port::NetworkPort<T>,
    request: &NetworkPortRequestV1,
) -> NetworkPortResponseV1 {
    if request.input_ptr != 0 || request.input_len != 0 {
        return network_port_response(NETWORK_PORT_STATUS_BAD_REQUEST, port.state());
    }
    if request.output_len == 0 {
        return network_port_response(NETWORK_PORT_STATUS_BAD_REQUEST, port.state());
    }
    if request.output_len < NETWORK_PORT_MAX_FRAME_BYTES as u64 {
        let mut response =
            network_port_response(NETWORK_PORT_STATUS_BUFFER_TOO_SMALL, port.state());
        response.required_len = NETWORK_PORT_MAX_FRAME_BYTES as u64;
        return response;
    }
    if request.output_len != NETWORK_PORT_MAX_FRAME_BYTES as u64
        || validate_user_buffer(
            copy_map,
            request.output_ptr,
            request.output_len,
            1,
            UserCopyAccess::Write,
        )
        .is_err()
    {
        return network_port_response(NETWORK_PORT_STATUS_BAD_REQUEST, port.state());
    }
    // SAFETY: output_ptr is exactly the ABI's 1514-byte writable single-map
    // range. The port copies one Ethernet frame synchronously and does not
    // retain this pointer; capacity validation occurred before it can consume
    // or recycle a receive completion.
    let output = unsafe {
        slice::from_raw_parts_mut(request.output_ptr as *mut u8, request.output_len as usize)
    };
    port.try_receive_into(output, capabilities)
}

#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe"
))]
const fn network_port_response(status: u16, state: u16) -> NetworkPortResponseV1 {
    NetworkPortResponseV1::new(status, state)
}

#[cfg(any(
    test,
    feature = "session-viewing-probe",
    all(feature = "normal-session", not(feature = "verify"))
))]
fn dispatch_session_viewing_present_with_table(
    args: SyscallArgs,
    capabilities: &CapabilityTable,
    present: impl FnOnce(ServiceId, u64, u64, u64, u64) -> Result<(), PresentationError>,
) -> Result<u64, SyscallError> {
    let caller = process_context::current_caller()?;
    validate_syscall_capability_with_table(
        capabilities,
        caller,
        PackedCapability::from_raw(args.arg0),
        ResourceId::new(SESSION_VIEWING_RESOURCE_ID),
        RightsMask::new(RightsMask::SEND),
    )?;
    if caller.principal_id() != pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID {
        return Err(SyscallError::SessionPresentation(
            PresentationError::WrongHolder,
        ));
    }
    present(
        caller.service_id(),
        args.arg1,
        args.arg2,
        args.arg3,
        args.arg4,
    )
    .map_err(SyscallError::SessionPresentation)?;
    Ok(SYSCALL_OK)
}

#[cfg(any(test, feature = "session-viewing-probe"))]
fn grant_session_presentation_capability_with_table(
    table: &mut CapabilityTable,
    process: ActiveUserProcess,
) -> Result<PackedCapability, SyscallError> {
    if process.principal_id() != pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID
    {
        return Err(SyscallError::SessionPresentation(
            PresentationError::WrongHolder,
        ));
    }
    let handle = table.grant(
        process.service_id(),
        ResourceId::new(SESSION_VIEWING_RESOURCE_ID),
        RightsMask::new(RightsMask::SEND),
    )?;
    Ok(pack_syscall_capability(handle))
}

/// The opt-in runtime bootstrap is the sole grant site; graph bindings never
/// receive this distinct presentation authority.
#[cfg(any(test, feature = "session-viewing-probe"))]
pub(crate) fn grant_session_presentation_capability(
    process: ActiveUserProcess,
) -> Result<PackedCapability, SyscallError> {
    with_syscall_capabilities(|table| {
        grant_session_presentation_capability_with_table(table, process)
    })
}

fn dispatch_session_input_try_read(args: SyscallArgs) -> Result<u64, SyscallError> {
    with_syscall_capabilities(|table| {
        dispatch_session_input_try_read_with_table(args, table, session_input::try_read_session)
    })
}

fn session_wait_with(
    args: SyscallArgs,
    mut caller: impl FnMut() -> Result<ActiveUserProcess, SyscallError>,
    mut validate: impl FnMut(ActiveUserProcess) -> Result<(), SyscallError>,
    mut input_ready: impl FnMut(ServiceId) -> Result<bool, SessionInputError>,
    mut console_ready: impl FnMut() -> bool,
    sleep: impl FnOnce(),
) -> Result<u64, SyscallError> {
    if args.arg2 != 0 || args.arg3 != 0 || args.arg4 != 0 {
        return Err(SyscallError::BadResult);
    }
    let identity = caller()?;
    validate(identity)?;
    let mut ready = || -> Result<u64, SyscallError> {
        // Owner validation must run even when console input is pending.
        let input = input_ready(identity.service_id())?;
        let console = console_ready();
        Ok(if input { SESSION_WAIT_INPUT_READY } else { 0 }
            | if console {
                SESSION_WAIT_CONSOLE_READY
            } else {
                0
            })
    };
    let pending = ready()?;
    if pending != 0 {
        return Ok(pending);
    }
    // Only copied identity/scalars and callbacks survive. In production IF is
    // still clear from FMASK; validation's table borrow has already ended.
    sleep();
    if caller()? != identity {
        return Err(SyscallError::BadResult);
    }
    validate(identity)?;
    ready()
}

fn dispatch_session_wait(args: SyscallArgs) -> Result<u64, SyscallError> {
    session_wait_with(
        args,
        || process_context::current_caller().map_err(SyscallError::from),
        |caller| {
            with_syscall_capabilities(|table| validate_session_wait_with_table(table, caller, args))
        },
        session_input::session_ready,
        || {
            #[cfg(not(test))]
            {
                serial::com2_receive_ready()
            }
            #[cfg(test)]
            {
                false
            }
        },
        || {
            #[cfg(not(test))]
            // SAFETY: FMASK cleared IF; both capability borrows ended, readiness
            // used no queue slot, and the single-core retained root maps the
            // syscall stack/IRQ/TSS/continuation. Normal boot does not arm proof
            // scheduling. No borrowed state or user pointer crosses this call.
            unsafe {
                crate::architecture::x86_64::interrupts::enable_halt_disable();
            }
        },
    )
}

#[cfg(any(test, all(feature = "normal-session", not(feature = "verify"))))]
#[derive(Debug)]
pub struct NormalSessionGrants {
    holder: ServiceId,
    handles: [PackedCapability; 4],
    owned: [bool; 4],
}

#[cfg(any(test, all(feature = "normal-session", not(feature = "verify"))))]
fn grant_normal_session_capabilities_with_table(
    table: &mut CapabilityTable,
    process: ActiveUserProcess,
    bind: impl FnOnce(ServiceId) -> Result<(), SessionInputError>,
) -> Result<NormalSessionGrants, SyscallError> {
    if process.principal_id() != pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID
    {
        return Err(SyscallError::Capability(CapabilityError::WrongHolder));
    }
    let requests = [
        (
            CONSOLE_COM2_RESOURCE,
            RightsMask::new(RightsMask::READ | RightsMask::WRITE),
        ),
        (
            ResourceId::new(SESSION_INPUT_RESOURCE_ID),
            RightsMask::new(RightsMask::INPUT),
        ),
        (
            ResourceId::new(pythos_shared::session_runtime_abi::SESSION_COMMAND_RESOURCE_ID),
            RightsMask::new(RightsMask::READ | RightsMask::APPEND),
        ),
        (
            ResourceId::new(pythos_shared::session_viewing_abi::SESSION_VIEWING_RESOURCE_ID),
            RightsMask::new(RightsMask::SEND),
        ),
    ];
    let mut grants = NormalSessionGrants {
        holder: process.service_id(),
        handles: [PackedCapability::from_raw(0); 4],
        owned: [false; 4],
    };
    let acquired = (|| {
        for (index, (resource, rights)) in requests.into_iter().enumerate() {
            let grant = table.grant_with_provenance(process.service_id(), resource, rights)?;
            if !grant.is_created() {
                return Err(SyscallError::Capability(CapabilityError::InvalidHandle));
            }
            grants.handles[index] = pack_syscall_capability(grant.handle());
            grants.owned[index] = true;
        }
        bind(process.service_id()).map_err(SyscallError::from)
    })();
    if let Err(error) = acquired {
        // No borrowed table/IRQ publication can replace these fresh grants.
        // Still attempt all owned handles if a future table change breaks that.
        revoke_normal_session_capabilities_with_table(table, &mut grants)?;
        return Err(error);
    }
    Ok(grants)
}

#[cfg(any(test, all(feature = "normal-session", not(feature = "verify"))))]
impl NormalSessionGrants {
    pub const fn console(&self) -> PackedCapability {
        self.handles[0]
    }
    pub const fn input(&self) -> PackedCapability {
        self.handles[1]
    }
    pub const fn command(&self) -> PackedCapability {
        self.handles[2]
    }
    pub const fn presentation(&self) -> PackedCapability {
        self.handles[3]
    }
    pub fn is_revoked(&self) -> bool {
        self.owned.iter().all(|owned| !owned)
    }
}

/// Acquire four fresh grants and bind input last, before PS/2 publication.
#[cfg(any(test, all(feature = "normal-session", not(feature = "verify"))))]
pub fn grant_normal_session_capabilities(
    process: ActiveUserProcess,
) -> Result<NormalSessionGrants, SyscallError> {
    with_syscall_capabilities(|table| {
        grant_normal_session_capabilities_with_table(
            table,
            process,
            session_input::bind_session_consumer_quiescent,
        )
    })
}

/// Revoke precisely the supplied slot/generation; never revoke a replacement.
#[cfg(test)]
pub fn revoke_syscall_capability(capability: PackedCapability) -> Result<(), SyscallError> {
    with_syscall_capabilities(|table| revoke_syscall_capability_with_table(table, capability))
}

#[cfg(any(test, all(feature = "normal-session", not(feature = "verify"))))]
fn revoke_syscall_capability_with_table(
    table: &mut CapabilityTable,
    capability: PackedCapability,
) -> Result<(), SyscallError> {
    table
        .revoke(unpack_syscall_capability(capability))
        .map_err(SyscallError::from)
}

#[cfg(any(test, all(feature = "normal-session", not(feature = "verify"))))]
pub fn revoke_normal_session_capabilities(
    grants: &mut NormalSessionGrants,
) -> Result<(), SyscallError> {
    with_syscall_capabilities(|table| revoke_normal_session_capabilities_with_table(table, grants))
}

#[cfg(any(test, all(feature = "normal-session", not(feature = "verify"))))]
fn revoke_normal_session_capabilities_with_table(
    table: &mut CapabilityTable,
    grants: &mut NormalSessionGrants,
) -> Result<(), SyscallError> {
    let mut failure = None;
    for (capability, owned) in grants.handles.iter().zip(grants.owned.iter_mut()) {
        if *owned {
            match revoke_syscall_capability_with_table(table, *capability) {
                Ok(()) => *owned = false,
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    // Check the actual table, not only the ownership ledger: each old exact
    // generation must now be rejected before holder/resource/rights checks.
    for capability in grants.handles {
        if !matches!(
            table.validate(
                grants.holder,
                unpack_syscall_capability(capability),
                CONSOLE_COM2_RESOURCE,
                RightsMask::new(RightsMask::READ)
            ),
            Err(CapabilityError::InvalidHandle | CapabilityError::Revoked)
        ) {
            return Err(SyscallError::BadResult);
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod normal_grant_test_support {
    use super::*;
    pub(crate) fn grant(
        table: &mut CapabilityTable,
        process: ActiveUserProcess,
        queue: &session_input::SessionInputQueue,
    ) -> Result<NormalSessionGrants, SyscallError> {
        grant_normal_session_capabilities_with_table(table, process, |holder| {
            queue.bind_session_consumer_quiescent(holder)
        })
    }
    pub(crate) fn revoke(
        table: &mut CapabilityTable,
        grants: &mut NormalSessionGrants,
    ) -> Result<(), SyscallError> {
        revoke_normal_session_capabilities_with_table(table, grants)
    }
}

fn validate_session_wait_with_table(
    table: &CapabilityTable,
    caller: ActiveUserProcess,
    args: SyscallArgs,
) -> Result<(), SyscallError> {
    if caller.principal_id() != pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID {
        return Err(SyscallError::Capability(CapabilityError::WrongHolder));
    }
    validate_syscall_capability_with_table(
        table,
        caller,
        PackedCapability::from_raw(args.arg0),
        ResourceId::new(SESSION_INPUT_RESOURCE_ID),
        RightsMask::new(RightsMask::INPUT),
    )?;
    validate_syscall_capability_with_table(
        table,
        caller,
        PackedCapability::from_raw(args.arg1),
        CONSOLE_COM2_RESOURCE,
        RightsMask::new(RightsMask::READ),
    )
}

fn dispatch_session_input_try_read_with_table(
    args: SyscallArgs,
    capabilities: &CapabilityTable,
    try_read_session: impl FnOnce(ServiceId) -> Result<Option<SessionInputEventV1>, SessionInputError>,
) -> Result<u64, SyscallError> {
    let caller = process_context::current_caller()?;
    let input_resource = ResourceId::new(SESSION_INPUT_RESOURCE_ID);
    let input_right = RightsMask::new(RightsMask::INPUT);
    validate_syscall_capability_with_table(
        capabilities,
        caller,
        PackedCapability::from_raw(args.arg0),
        input_resource,
        input_right,
    )?;
    if args.arg2 != size_of::<SessionInputEventV1>() as u64 {
        return Err(SyscallError::BadResult);
    }
    if args.arg3 != 0 || args.arg4 != 0 {
        return Err(SyscallError::BadResult);
    }
    validate_session_input_output(
        &caller.copy_map(),
        args.arg1,
        args.arg2,
        align_of::<SessionInputEventV1>(),
        UserCopyAccess::Write,
    )?;
    let Some(event) = try_read_session(caller.service_id())? else {
        return Ok(SESSION_INPUT_RESULT_EMPTY);
    };
    let output = args.arg1 as *mut SessionInputEventV1;
    // SAFETY:
    // 1. Invariant: `output` names exactly one writable SessionInputEventV1
    //    in the active caller's user copy map.
    // 2. Established by: capability validation, exact ABI length and reserved
    //    argument checks, alignment, canonical user-range, and writable-map
    //    validation above all complete before the reader can dequeue.
    // 3. Lifetime: `event` is copied by value and `output` is used only for
    //    this synchronous syscall copy-out.
    // 4. Pointer ownership: the caller owns the mapped output memory; PythCore
    //    writes one record without retaining the pointer.
    // 5. Alignment: validate_user_buffer checked SessionInputEventV1 alignment.
    // 6. Mapped length: validation checked exactly size_of::<SessionInputEventV1>().
    // 7. Concurrency: one syscall executes at a time in this slice and the
    //    output mapping remains valid for the syscall duration.
    // 8. Violation: stale copy-map state could write unrelated user memory.
    unsafe { output.write(event) };
    Ok(SESSION_INPUT_RESULT_EVENT)
}

fn console_read_result(
    com2: Option<u8>,
    poll_physical_keyboard: impl FnOnce() -> Option<u8>,
) -> u64 {
    if let Some(byte) = com2 {
        return u64::from(byte);
    }
    poll_physical_keyboard().map_or(NO_BYTE, u64::from)
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

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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

#[cfg(all(not(test), feature = "verify", not(feature = "phase13-package-test")))]
fn dispatch_object_request(_args: SyscallArgs) -> Result<u64, SyscallError> {
    Err(SyscallError::BadResult)
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
fn dispatch_object_request_with_raw_buffers(
    caller: ActiveUserProcess,
    copy_map: &UserCopyMap,
    request: ObjectShellRequest,
) -> Result<ObjectShellResponse, SyscallError> {
    if !valid_object_request_header(&request) {
        return Ok(bad_request_response());
    }
    let package_defined_create_input = if request.operation == OP_CREATE_OBJECT
        && request.object_kind == OBJECT_KIND_PACKAGE_DEFINED_OBJECT
    {
        match checked_package_defined_create_input(copy_map, &request)? {
            Some(input) => Some(input),
            None => return Ok(bad_request_response()),
        }
    } else {
        None
    };
    let input = if request.operation == OP_REVISE_FIELD {
        checked_request_input(copy_map, &request)?
    } else {
        &[]
    };
    let mut query_marker_entry = None;
    let response = if let Some(input) = package_defined_create_input {
        retained_services::with_object_service(|service| {
            dispatch_package_defined_create_to_service(service, caller, request, input)
        })
        .map_err(SyscallError::from)?
    } else if request.operation == OP_QUERY_OBJECTS {
        if request.output_len < size_of::<[ObjectListEntry; MAX_QUERY_RESULTS]>() as u64 {
            return Ok(buffer_too_small_response());
        }
        let output = checked_query_output(copy_map, &request)?;
        let response = retained_services::with_object_service(|service| {
            dispatch_object_request_to_service(service, caller, request, input, output)
        })
        .map_err(SyscallError::from)?;
        if response.status == STATUS_OK
            && response.bytes_written >= size_of::<ObjectListEntry>() as u64
        {
            query_marker_entry = Some(output[0]);
        }
        response
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
    if response.status == STATUS_OK && object_operation_mutates(request.operation) {
        retained_services::persist_object_service().map_err(SyscallError::from)?;
    }

    emit_pythtig_object_success_marker(caller, request.operation, response, query_marker_entry);

    Ok(response)
}

#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
fn reconcile_package_schema_references_from_object_service(
    object_service: &ObjectService,
) -> Result<(), PackageStatus> {
    package_service::with_retained_package_service_for_phase13(|package_service| {
        package_service.reconcile_schema_references_from_object_service(object_service)
    })
    .unwrap_or(Err(PackageStatus::Denied))
}

#[cfg(all(
    not(test),
    not(feature = "verify"),
    feature = "hardware-probe",
    not(feature = "phase13-package-test")
))]
fn reconcile_package_schema_references_from_object_service(
    _object_service: &ObjectService,
) -> Result<(), PackageStatus> {
    Err(PackageStatus::Denied)
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
const fn object_operation_mutates(operation: u16) -> bool {
    matches!(operation, OP_CREATE_OBJECT | OP_REVISE_FIELD)
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
    let context_output = if request.operation == OP_READ_CONTEXT_SUMMARY {
        if request.output_len < size_of::<TaskContextSummary>() as u64 {
            return Ok(task_buffer_too_small_response());
        }
        Some(checked_task_context_output(copy_map, &request)?)
    } else {
        None
    };
    let proposal_output = if request.operation == OP_LIST_PROPOSALS {
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
            context_output,
            proposal_output,
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

#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe",
    all(not(test), not(feature = "verify")),
    all(not(test), feature = "phase13-package-test")
))]
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

fn validate_session_input_output(
    copy_map: &UserCopyMap,
    ptr: u64,
    len: u64,
    alignment: usize,
    access: UserCopyAccess,
) -> Result<(), SyscallError> {
    if !is_aligned(ptr, alignment) {
        return Err(SyscallError::BadResult);
    }
    validate_canonical_user_range(ptr, len)?;
    copy_map.validate_range(ptr, len, access)?;
    Ok(())
}

fn validate_canonical_user_range(ptr: u64, len: u64) -> Result<(), SyscallError> {
    let end = ptr.checked_add(len).ok_or(SyscallError::BadResult)?;
    if ptr < USER_VIRT_MIN || end > USER_VIRT_MAX {
        return Err(SyscallError::BadResult);
    }
    Ok(())
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
fn checked_package_defined_create_input<'a>(
    copy_map: &UserCopyMap,
    request: &ObjectShellRequest,
) -> Result<Option<PackageDefinedCreateInput<'a>>, SyscallError> {
    if request.input_len != size_of::<PackageDefinedObjectCreateV0>() as u64 {
        return Ok(None);
    }
    validate_user_buffer(
        copy_map,
        request.input_ptr,
        request.input_len,
        align_of::<PackageDefinedObjectCreateV0>(),
        UserCopyAccess::Read,
    )?;
    let create_ptr = request.input_ptr as *const PackageDefinedObjectCreateV0;
    // SAFETY:
    // 1. Invariant: `create_ptr` names one readable
    //    PackageDefinedObjectCreateV0 in the active caller's user copy map.
    // 2. Established by: exact input_len check, repr(C) alignment check, and
    //    UserCopyMap readable-range validation above.
    // 3. Lifetime: the record is copied by value and not retained.
    // 4. Pointer ownership: user space owns the buffer; PythCore only reads it.
    // 5. Alignment: checked against PackageDefinedObjectCreateV0 alignment.
    // 6. Mapped length: UserCopyMap validated the full ABI record size.
    // 7. Concurrency: object-shell syscalls are handled one at a time here.
    // 8. Violation: stale copy-map state could read unrelated user memory.
    let create = unsafe { create_ptr.read() };
    if create.abi_major != PACKAGE_DEFINED_OBJECT_CREATE_ABI_MAJOR
        || create.abi_minor != PACKAGE_DEFINED_OBJECT_CREATE_ABI_MINOR
        || create.flags != 0
        || create.reserved0 != 0
        || create.reserved1 != 0
        || create.reserved2 != 0
    {
        return Ok(None);
    }
    let initial_state = match create.state_format {
        PACKAGE_DEFINED_STATE_FORMAT_EMPTY => {
            if create.initial_state_ptr != 0 || create.initial_state_len != 0 {
                return Ok(None);
            }
            &[]
        }
        PACKAGE_DEFINED_STATE_FORMAT_INLINE_BYTES_V0 => {
            if create.initial_state_ptr == 0
                || create.initial_state_len == 0
                || create.initial_state_len > PACKAGE_DEFINED_MAX_INITIAL_STATE_BYTES
            {
                return Ok(None);
            }
            copy_map.validate_range(
                create.initial_state_ptr,
                create.initial_state_len,
                UserCopyAccess::Read,
            )?;
            // SAFETY:
            // 1. Invariant: non-empty inline package-defined state names at
            //    most PACKAGE_DEFINED_MAX_INITIAL_STATE_BYTES readable bytes
            //    in the active caller's user copy map.
            // 2. Established by: nonzero pointer/length checks, bounded length
            //    check, and UserCopyMap readable-range validation above.
            // 3. Lifetime: the slice is consumed synchronously while creating
            //    the object and is not retained.
            // 4. Pointer ownership: user space owns the bytes; PythCore copies
            //    them into an object-owned typed field.
            // 5. Alignment: byte slices require no stricter alignment than 1.
            // 6. Mapped length: UserCopyMap validated the exact state range.
            // 7. Concurrency: object-shell syscalls are handled one at a time.
            // 8. Violation: stale copy-map state could read unrelated memory.
            unsafe {
                slice::from_raw_parts(
                    create.initial_state_ptr as *const u8,
                    create.initial_state_len as usize,
                )
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(PackageDefinedCreateInput {
        schema_object_id: ObjectId::new(create.schema_object_id),
        schema_revision: create.schema_revision,
        state_format: create.state_format,
        initial_state,
    }))
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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

#[cfg(all(not(test), feature = "verify", not(feature = "phase13-package-test")))]
fn dispatch_pyth_graph_log(_args: SyscallArgs) -> Result<u64, SyscallError> {
    Err(SyscallError::BadResult)
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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

#[cfg(all(not(test), feature = "verify", not(feature = "phase13-package-test")))]
fn dispatch_pyth_graph_exit(_args: SyscallArgs) -> Result<u64, SyscallError> {
    Err(SyscallError::BadResult)
}

#[cfg(any(
    test,
    all(not(test), not(feature = "verify")),
    all(not(test), feature = "phase13-package-test")
))]
fn dispatch_package_context(args: SyscallArgs) -> Result<u64, SyscallError> {
    if args.arg0 != u64::from(OP_PACKAGE_CONTEXT_SCHEMA) || args.arg4 != 0 {
        return Ok(u64::from(PackageStatus::BadRequest as u16));
    }
    if args.arg1 > u64::from(u16::MAX) {
        return Ok(u64::from(PackageStatus::BadRequest as u16));
    }
    if args.arg3 != size_of::<PackageRuntimeSchemaBindingV0>() as u64 {
        return Ok(u64::from(PackageStatus::BufferTooSmall as u16));
    }

    let caller = process_context::current_caller()?;
    let copy_map = caller.copy_map();
    validate_user_buffer(
        &copy_map,
        args.arg2,
        args.arg3,
        align_of::<PackageRuntimeSchemaBindingV0>(),
        UserCopyAccess::Write,
    )?;
    let binding = match package_runtime_schema_binding(caller, args.arg1 as u16) {
        Ok(binding) => binding,
        Err(status) => return Ok(u64::from(status as u16)),
    };
    let output_ptr = args.arg2 as *mut PackageRuntimeSchemaBindingV0;
    // SAFETY:
    // 1. Invariant: `output_ptr` names one writable
    //    PackageRuntimeSchemaBindingV0 in the active caller's user copy map.
    // 2. Established by: exact output length, natural alignment, and
    //    UserCopyMap writable-range validation above.
    // 3. Lifetime: the pointer is consumed only for this syscall copy-out.
    // 4. Pointer ownership: user space owns the output buffer; PythCore writes
    //    exactly one ABI record and retains no reference.
    // 5. Alignment: checked against PackageRuntimeSchemaBindingV0 alignment.
    // 6. Mapped length: UserCopyMap validated the exact ABI record size.
    // 7. Concurrency: one package-context syscall is handled at a time in this
    //    Phase 13 slice.
    // 8. Violation: stale copy-map state could corrupt user memory or fault.
    unsafe {
        output_ptr.write(binding);
    }
    Ok(u64::from(PackageStatus::Ok as u16))
}

#[cfg(all(not(test), feature = "verify", not(feature = "phase13-package-test")))]
fn dispatch_package_context(_args: SyscallArgs) -> Result<u64, SyscallError> {
    Ok(u64::from(PackageStatus::Denied as u16))
}

#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
fn package_runtime_schema_binding(
    caller: ActiveUserProcess,
    schema_slot: u16,
) -> Result<PackageRuntimeSchemaBindingV0, PackageStatus> {
    package_service::with_retained_package_service_for_phase13(|service| {
        service.runtime_schema_binding(caller, schema_slot)
    })
    .unwrap_or(Err(PackageStatus::Denied))
}

#[cfg(all(
    not(test),
    not(feature = "verify"),
    feature = "hardware-probe",
    not(feature = "phase13-package-test")
))]
fn package_runtime_schema_binding(
    _caller: ActiveUserProcess,
    _schema_slot: u16,
) -> Result<PackageRuntimeSchemaBindingV0, PackageStatus> {
    Err(PackageStatus::Denied)
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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

#[cfg(all(
    not(test),
    any(not(feature = "verify"), feature = "phase13-package-test")
))]
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

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
fn dispatch_package_defined_create_to_service(
    service: &mut ObjectService,
    caller: ActiveUserProcess,
    request: ObjectShellRequest,
    input: PackageDefinedCreateInput<'_>,
) -> ObjectShellResponse {
    let mut staged_service = *service;
    match staged_service.create_package_defined_object(caller, request.authority, input) {
        Ok(created) => {
            if let Err(status) =
                reconcile_package_schema_references_from_object_service(&staged_service)
            {
                return package_retention_error_response(caller, request, status);
            }
            *service = staged_service;
            ObjectShellResponse {
                status: STATUS_OK,
                object_kind: request.object_kind,
                object_id: created.object_id.raw(),
                revision: created.revision,
                capability: created.object_capability,
                ..empty_response()
            }
        }
        Err(error) => object_error_response(caller, request, error),
    }
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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
                    ObjectShellResponse {
                        status: STATUS_OK,
                        object_kind: request.object_kind,
                        bytes_written: (count * size_of::<ObjectListEntry>()) as u64,
                        ..empty_response()
                    }
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
                ObjectShellResponse {
                    status: STATUS_OK,
                    object_kind: OBJECT_KIND_NOTE,
                    field_id: FIELD_TEXT,
                    object_id: request.object_id,
                    revision: inspection.revision,
                    bytes_written,
                    field_bytes,
                    ..empty_response()
                }
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
            Ok(revision) => ObjectShellResponse {
                status: STATUS_OK,
                field_id: request.field_id,
                object_id: request.object_id,
                revision,
                ..empty_response()
            },
            Err(error) => object_error_response(caller, request, error),
        },
        OP_GET_HISTORY => {
            match service.history(caller, request.authority, ObjectId::new(request.object_id)) {
                Ok(revision_count) => ObjectShellResponse {
                    status: STATUS_OK,
                    object_id: request.object_id,
                    revision_count,
                    ..empty_response()
                },
                Err(error) => object_error_response(caller, request, error),
            }
        }
        _ => bad_request_response(),
    }
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
fn valid_object_request_header(request: &ObjectShellRequest) -> bool {
    request.abi_major == OBJECT_SHELL_ABI_MAJOR
        && request.abi_minor == OBJECT_SHELL_ABI_MINOR
        && request.reserved0 == 0
        && request.reserved1 == 0
        && request.reserved2 == 0
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
fn request_object_kind(kind: u16) -> Result<ObjectKind, ObjectServiceError> {
    if kind == OBJECT_KIND_NOTE {
        Ok(ObjectKind::Note)
    } else {
        Err(ObjectServiceError::UnsupportedKind)
    }
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
fn package_retention_error_response(
    caller: ActiveUserProcess,
    request: ObjectShellRequest,
    status: PackageStatus,
) -> ObjectShellResponse {
    match status {
        PackageStatus::Denied => object_error_response(caller, request, ObjectServiceError::Denied),
        PackageStatus::NotFound => {
            object_error_response(caller, request, ObjectServiceError::NotFound)
        }
        PackageStatus::QuotaDenied => {
            object_error_response(caller, request, ObjectServiceError::Denied)
        }
        _ => bad_request_response(),
    }
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
fn object_error_response(
    caller: ActiveUserProcess,
    request: ObjectShellRequest,
    error: ObjectServiceError,
) -> ObjectShellResponse {
    emit_pythtig_object_denial_marker(caller, request, error);
    error_response(error)
}

#[cfg(all(
    not(test),
    any(not(feature = "verify"), feature = "phase13-package-test")
))]
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

#[cfg(all(
    not(test),
    any(not(feature = "verify"), feature = "phase13-package-test")
))]
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

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
const fn bad_request_response() -> ObjectShellResponse {
    empty_response()
}

#[cfg(any(
    test,
    all(
        not(test),
        any(not(feature = "verify"), feature = "phase13-package-test")
    )
))]
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
        validate_syscall_capability_with_table(table, caller, capability, resource, rights)
    })
}

fn validate_syscall_capability_with_table(
    table: &CapabilityTable,
    caller: ActiveUserProcess,
    capability: PackedCapability,
    resource: ResourceId,
    rights: RightsMask,
) -> Result<(), SyscallError> {
    table.validate(
        caller.service_id(),
        unpack_syscall_capability(capability),
        resource,
        rights,
    )?;
    Ok(())
}

const fn pack_syscall_capability(handle: CapabilityHandle) -> PackedCapability {
    PackedCapability::from_parts(handle.slot(), handle.generation())
}

const fn unpack_syscall_capability(capability: PackedCapability) -> CapabilityHandle {
    CapabilityHandle::from_parts(capability.slot(), capability.generation())
}

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
fn network_port_args(
    request: &NetworkPortRequestV1,
    response: &mut NetworkPortResponseV1,
) -> SyscallArgs {
    SyscallArgs {
        number: pythos_shared::network_port_abi::SYSCALL_NETWORK_PORT_REQUEST,
        arg0: request as *const NetworkPortRequestV1 as u64,
        arg1: size_of::<NetworkPortRequestV1>() as u64,
        arg2: response as *mut NetworkPortResponseV1 as u64,
        arg3: size_of::<NetworkPortResponseV1>() as u64,
        arg4: 0,
    }
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
    if SYSCALL_ABI_MAJOR != 1 || SYSCALL_ABI_MINOR != 2 {
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
    use crate::object_relationships::{PACKAGE_LOCATOR_ROOT_OBJECT_ID, SHELL_WORKSPACE_OBJECT_ID};
    use crate::object_service::ObjectService;
    use crate::package_service::{
        PackageLaunchGrant, PackageLaunchRequest, PackageLaunchRequirement,
    };
    use crate::shell_objects::ObjectId;
    use crate::shell_objects::ObjectKind;
    use crate::user_copy::{UserCopyError, UserCopyMap};
    use pythos_shared::object_shell_abi::{
        FIELD_TEXT, OBJECT_KIND_NOTE, OBJECT_SHELL_ABI_MAJOR, OBJECT_SHELL_ABI_MINOR,
        OP_CREATE_OBJECT, OP_QUERY_OBJECTS, OP_REVISE_FIELD, ObjectListEntry, ObjectShellRequest,
        ObjectShellResponse, STATUS_BAD_REQUEST, STATUS_DENIED, STATUS_NOT_FOUND, STATUS_OK,
    };
    use pythos_shared::package_abi::{
        FIELD_PACKAGE_INLINE_STATE_V0, FIELD_PACKAGE_SCHEMA_REF_V0,
        OBJECT_KIND_PACKAGE_DEFINED_OBJECT, OP_PACKAGE_CONTEXT_SCHEMA,
        PACKAGE_DEFINED_OBJECT_CREATE_ABI_MAJOR, PACKAGE_DEFINED_OBJECT_CREATE_ABI_MINOR,
        PACKAGE_DEFINED_STATE_FORMAT_EMPTY, PACKAGE_DEFINED_STATE_FORMAT_INLINE_BYTES_V0,
        PackageDefinedObjectCreateV0, PackageRuntimeSchemaBindingV0, PackageStatus,
        SYSCALL_PACKAGE_CONTEXT,
    };
    use pythos_shared::session_input_abi::{
        SESSION_INPUT_RESOURCE_ID, SESSION_INPUT_RESULT_EMPTY, SESSION_INPUT_RESULT_EVENT,
        SYSCALL_SESSION_INPUT_TRY_READ, SessionInputEventV1,
    };
    use pythos_shared::session_runtime_abi::SESSION_COMMAND_RESOURCE_ID;
    use pythos_shared::task_abi::{
        MAX_TASK_PROPOSAL_RESULTS, OP_APPEND_TASK_EVENT, OP_CREATE_PROPOSAL, OP_CREATE_TASK,
        OP_LIST_PROPOSALS, OP_READ_ACTIVE_TASK, TASK_ABI_MAJOR, TASK_ABI_MINOR, TaskEventInput,
        TaskProposalKind, TaskProposalListEntry, TaskRequest, TaskResponse,
    };

    static EXPECTED_SYSCALL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[derive(Clone, Copy)]
    struct FakeNetworkTransport {
        sent: usize,
        received: Option<[u8; pythos_shared::network_port_abi::NETWORK_PORT_MIN_FRAME_BYTES]>,
        transmit_error: bool,
    }

    impl FakeNetworkTransport {
        const fn new() -> Self {
            Self {
                sent: 0,
                received: None,
                transmit_error: false,
            }
        }
    }

    impl crate::network_port::NetworkTransport for FakeNetworkTransport {
        fn mac(&self) -> [u8; 6] {
            [2, 0, 0, 0, 0, 1]
        }

        fn transmit(&mut self, _frame: &[u8]) -> Result<(), crate::network_port::TransportError> {
            if self.transmit_error {
                return Err(crate::network_port::TransportError::Fault);
            }
            self.sent += 1;
            Ok(())
        }

        fn try_receive_into(
            &mut self,
            output: &mut [u8],
        ) -> Result<Option<usize>, crate::network_port::TransportError> {
            let Some(frame) = self.received.take() else {
                return Ok(None);
            };
            output[..frame.len()].copy_from_slice(&frame);
            Ok(Some(frame.len()))
        }

        fn reset(&mut self) -> Result<(), crate::network_port::TransportError> {
            Ok(())
        }
    }

    fn network_process(service: u64) -> ActiveUserProcess {
        ActiveUserProcess::new(ServiceId::from_raw(service), 0x5059_4E50_5254_0001, service)
    }

    fn network_request(
        operation: u16,
        authority: PackedCapability,
    ) -> pythos_shared::network_port_abi::NetworkPortRequestV1 {
        pythos_shared::network_port_abi::NetworkPortRequestV1::new(operation, authority)
    }

    fn network_port_for_test() -> crate::network_port::NetworkPort<FakeNetworkTransport> {
        crate::network_port::NetworkPort::new_for_test(FakeNetworkTransport::new())
    }

    #[test]
    fn network_port_forged_capability_is_denied_before_bad_send_buffer() {
        use pythos_shared::network_port_abi::{
            NETWORK_PORT_OP_SEND, NETWORK_PORT_STATUS_DENIED, NetworkPortResponseV1,
        };

        let caller = network_process(0x101);
        let mut table = CapabilityTable::new();
        let mut port = network_port_for_test();
        let request = Box::new(network_request(
            NETWORK_PORT_OP_SEND,
            PackedCapability::from_parts(0, 1),
        ));
        let mut response = Box::new(NetworkPortResponseV1::new(0xFFFF, 0));
        let mut map = UserCopyMap::new();
        map_value(&mut map, &*request, true, false);
        map_value(&mut map, &*response, true, true);

        assert_eq!(
            dispatch_network_port_with_port(
                network_port_args(&request, &mut response),
                caller.with_copy_map(map),
                &mut table,
                &mut port,
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_DENIED);
    }

    fn grant_network_port_right<T: crate::network_port::NetworkTransport>(
        table: &mut CapabilityTable,
        caller: ActiveUserProcess,
        port: &crate::network_port::NetworkPort<T>,
        rights: u32,
    ) -> PackedCapability {
        pack_syscall_capability(
            table
                .grant(
                    caller.service_id(),
                    port.resource(),
                    RightsMask::new(rights),
                )
                .unwrap(),
        )
    }

    fn call_network_port(
        table: &mut CapabilityTable,
        port: &mut crate::network_port::NetworkPort<FakeNetworkTransport>,
        caller: ActiveUserProcess,
        request: &NetworkPortRequestV1,
        response: &mut NetworkPortResponseV1,
        add_buffers: impl FnOnce(&mut UserCopyMap),
    ) -> Result<u64, SyscallError> {
        let mut map = UserCopyMap::new();
        map_value(&mut map, request, true, false);
        map_value(&mut map, response, true, true);
        add_buffers(&mut map);
        dispatch_network_port_with_port(
            network_port_args(request, response),
            caller.with_copy_map(map),
            table,
            port,
        )
    }

    #[test]
    fn network_port_wrong_holder_missing_right_and_stale_generation_are_denied() {
        use pythos_shared::network_port_abi::{
            NETWORK_PORT_OP_DESCRIBE, NETWORK_PORT_OP_SEND, NETWORK_PORT_STATUS_DENIED,
            NetworkPortResponseV1,
        };

        let holder = network_process(0x102);
        let intruder = network_process(0x103);
        let mut table = CapabilityTable::new();
        let mut port = network_port_for_test();
        let send = grant_network_port_right(&mut table, holder, &port, RightsMask::SEND);
        let read = grant_network_port_right(&mut table, holder, &port, RightsMask::READ);
        let frame = Box::new([0; pythos_shared::network_port_abi::NETWORK_PORT_MIN_FRAME_BYTES]);

        let mut wrong_holder = Box::new(network_request(NETWORK_PORT_OP_SEND, send));
        wrong_holder.input_ptr = frame.as_ptr() as u64;
        wrong_holder.input_len = frame.len() as u64;
        let mut response = Box::new(NetworkPortResponseV1::new(0xFFFF, 0));
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                intruder,
                &wrong_holder,
                &mut response,
                |map| { map_slice(map, &*frame, true, false) }
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_DENIED);

        let mut missing_right = Box::new(network_request(NETWORK_PORT_OP_SEND, read));
        missing_right.input_ptr = frame.as_ptr() as u64;
        missing_right.input_len = frame.len() as u64;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                holder,
                &missing_right,
                &mut response,
                |map| { map_slice(map, &*frame, true, false) }
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_DENIED);

        table.revoke(unpack_syscall_capability(read)).unwrap();
        let stale = Box::new(network_request(NETWORK_PORT_OP_DESCRIBE, read));
        assert_eq!(
            call_network_port(&mut table, &mut port, holder, &stale, &mut response, |_| {}),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_DENIED);
    }

    #[test]
    fn network_port_rejects_malformed_shapes_and_bad_user_buffers_without_transport_access() {
        use pythos_shared::network_port_abi::{
            NETWORK_PORT_MAX_FRAME_BYTES, NETWORK_PORT_OP_SEND, NETWORK_PORT_OP_TRY_RECEIVE,
            NETWORK_PORT_STATUS_BAD_REQUEST, NETWORK_PORT_STATUS_BUFFER_TOO_SMALL,
            NetworkPortResponseV1,
        };

        let caller = network_process(0x104);
        let mut table = CapabilityTable::new();
        let mut port = network_port_for_test();
        let send = grant_network_port_right(&mut table, caller, &port, RightsMask::SEND);
        let mut response = Box::new(NetworkPortResponseV1::new(0xFFFF, 0));

        let mut malformed = Box::new(network_request(NETWORK_PORT_OP_SEND, send));
        malformed.flags = 1;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                caller,
                &malformed,
                &mut response,
                |_| {}
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_BAD_REQUEST);

        let mut unmapped = Box::new(network_request(NETWORK_PORT_OP_SEND, send));
        unmapped.input_ptr = 0x0040_0000;
        unmapped.input_len = pythos_shared::network_port_abi::NETWORK_PORT_MIN_FRAME_BYTES as u64;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                caller,
                &unmapped,
                &mut response,
                |_| {}
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_BAD_REQUEST);

        let mut overflow = Box::new(network_request(NETWORK_PORT_OP_SEND, send));
        overflow.input_ptr = u64::MAX - 16;
        overflow.input_len = pythos_shared::network_port_abi::NETWORK_PORT_MIN_FRAME_BYTES as u64;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                caller,
                &overflow,
                &mut response,
                |_| {}
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_BAD_REQUEST);

        let read = grant_network_port_right(&mut table, caller, &port, RightsMask::READ);
        let mut describe = Box::new(network_request(
            pythos_shared::network_port_abi::NETWORK_PORT_OP_DESCRIBE,
            read,
        ));
        let description =
            Box::new(pythos_shared::network_port_abi::NetworkPortDescriptionV1::empty());
        describe.output_ptr = (&*description
            as *const pythos_shared::network_port_abi::NetworkPortDescriptionV1)
            as u64;
        describe.output_len =
            size_of::<pythos_shared::network_port_abi::NetworkPortDescriptionV1>() as u64;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                caller,
                &describe,
                &mut response,
                |map| { map_value(map, &*description, true, false) }
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_BAD_REQUEST);

        let mut zero_receive = Box::new(network_request(NETWORK_PORT_OP_TRY_RECEIVE, read));
        zero_receive.output_ptr = 1;
        zero_receive.output_len = 0;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                caller,
                &zero_receive,
                &mut response,
                |_| {}
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_BAD_REQUEST);

        for capacity in [1_u64, (NETWORK_PORT_MAX_FRAME_BYTES - 1) as u64] {
            let mut short_receive = Box::new(network_request(NETWORK_PORT_OP_TRY_RECEIVE, read));
            short_receive.output_ptr = 1;
            short_receive.output_len = capacity;
            assert_eq!(
                call_network_port(
                    &mut table,
                    &mut port,
                    caller,
                    &short_receive,
                    &mut response,
                    |_| {}
                ),
                Ok(SYSCALL_OK)
            );
            assert_eq!(response.status, NETWORK_PORT_STATUS_BUFFER_TOO_SMALL);
            assert_eq!(response.required_len, NETWORK_PORT_MAX_FRAME_BYTES as u64);
        }

        let mut oversized_receive = Box::new(network_request(NETWORK_PORT_OP_TRY_RECEIVE, read));
        oversized_receive.output_ptr = 1;
        oversized_receive.output_len = (NETWORK_PORT_MAX_FRAME_BYTES + 1) as u64;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                caller,
                &oversized_receive,
                &mut response,
                |_| {}
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_BAD_REQUEST);
    }

    #[test]
    fn network_port_reset_revokes_bound_consumer_and_owner_handles() {
        use pythos_shared::network_port_abi::{
            NETWORK_PORT_MIN_FRAME_BYTES, NETWORK_PORT_OP_RESET, NETWORK_PORT_OP_SEND,
            NETWORK_PORT_STATE_RESET, NETWORK_PORT_STATUS_DENIED, NETWORK_PORT_STATUS_OK,
            NetworkPortResponseV1,
        };

        let consumer_holder = network_process(0x106);
        let owner_holder = network_process(0x107);
        let mut table = CapabilityTable::new();
        let mut port = network_port_for_test();
        let consumer = grant_network_port_right(
            &mut table,
            consumer_holder,
            &port,
            RightsMask::READ | RightsMask::SEND,
        );
        let owner = pack_syscall_capability(
            table
                .grant(
                    owner_holder.service_id(),
                    port.resource(),
                    RightsMask::new(RightsMask::WRITE),
                )
                .unwrap(),
        );
        bind_network_port_capabilities_with_table(
            &table,
            &mut port,
            consumer_holder,
            consumer,
            owner_holder.service_id(),
            owner,
        )
        .unwrap();

        let mut response = Box::new(NetworkPortResponseV1::new(0xFFFF, 0));
        let reset = Box::new(network_request(NETWORK_PORT_OP_RESET, owner));
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                owner_holder,
                &reset,
                &mut response,
                |_| {},
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_OK);
        assert_eq!(response.state, NETWORK_PORT_STATE_RESET);

        let frame = Box::new([0xA5; NETWORK_PORT_MIN_FRAME_BYTES]);
        let mut send = Box::new(network_request(NETWORK_PORT_OP_SEND, consumer));
        send.input_ptr = frame.as_ptr() as u64;
        send.input_len = frame.len() as u64;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                consumer_holder,
                &send,
                &mut response,
                |map| { map_slice(map, &*frame, true, false) },
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_DENIED);

        let reset_again = Box::new(network_request(NETWORK_PORT_OP_RESET, owner));
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                owner_holder,
                &reset_again,
                &mut response,
                |_| {},
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_DENIED);
    }

    #[test]
    fn network_port_transport_failure_revokes_bound_handles_before_returning() {
        use pythos_shared::network_port_abi::{
            NETWORK_PORT_MIN_FRAME_BYTES, NETWORK_PORT_OP_RESET, NETWORK_PORT_OP_SEND,
            NETWORK_PORT_STATE_FAILED, NETWORK_PORT_STATUS_DENIED,
            NETWORK_PORT_STATUS_TRANSPORT_ERROR, NetworkPortResponseV1,
        };

        let consumer_holder = network_process(0x108);
        let owner_holder = network_process(0x109);
        let mut table = CapabilityTable::new();
        let mut transport = FakeNetworkTransport::new();
        transport.transmit_error = true;
        let mut port = crate::network_port::NetworkPort::new_for_test(transport);
        let consumer = grant_network_port_right(
            &mut table,
            consumer_holder,
            &port,
            RightsMask::READ | RightsMask::SEND,
        );
        let owner = pack_syscall_capability(
            table
                .grant(
                    owner_holder.service_id(),
                    port.resource(),
                    RightsMask::new(RightsMask::WRITE),
                )
                .unwrap(),
        );
        bind_network_port_capabilities_with_table(
            &table,
            &mut port,
            consumer_holder,
            consumer,
            owner_holder.service_id(),
            owner,
        )
        .unwrap();

        let frame = Box::new([0xA5; NETWORK_PORT_MIN_FRAME_BYTES]);
        let mut send = Box::new(network_request(NETWORK_PORT_OP_SEND, consumer));
        send.input_ptr = frame.as_ptr() as u64;
        send.input_len = frame.len() as u64;
        let mut response = Box::new(NetworkPortResponseV1::new(0xFFFF, 0));
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                consumer_holder,
                &send,
                &mut response,
                |map| { map_slice(map, &*frame, true, false) },
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_TRANSPORT_ERROR);
        assert_eq!(response.state, NETWORK_PORT_STATE_FAILED);

        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                consumer_holder,
                &send,
                &mut response,
                |_| {},
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_DENIED);

        let reset = Box::new(network_request(NETWORK_PORT_OP_RESET, owner));
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                owner_holder,
                &reset,
                &mut response,
                |_| {},
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_DENIED);
    }

    #[test]
    fn network_port_describe_send_receive_and_reset_follow_the_fixed_abi() {
        use pythos_shared::network_port_abi::{
            NETWORK_PORT_MAX_FRAME_BYTES, NETWORK_PORT_MIN_FRAME_BYTES, NETWORK_PORT_OP_DESCRIBE,
            NETWORK_PORT_OP_RESET, NETWORK_PORT_OP_SEND, NETWORK_PORT_OP_TRY_RECEIVE,
            NETWORK_PORT_STATE_RESET, NETWORK_PORT_STATUS_BUFFER_TOO_SMALL,
            NETWORK_PORT_STATUS_EMPTY, NETWORK_PORT_STATUS_NOT_READY, NETWORK_PORT_STATUS_OK,
            NetworkPortDescriptionV1, NetworkPortResponseV1,
        };

        let caller = network_process(0x105);
        let mut table = CapabilityTable::new();
        let mut port = network_port_for_test();
        let read = grant_network_port_right(&mut table, caller, &port, RightsMask::READ);
        let send = grant_network_port_right(&mut table, caller, &port, RightsMask::SEND);
        let write = grant_network_port_right(&mut table, caller, &port, RightsMask::WRITE);
        let mut response = Box::new(NetworkPortResponseV1::new(0xFFFF, 0));

        let mut description_request = Box::new(network_request(NETWORK_PORT_OP_DESCRIBE, read));
        let mut description = Box::new(NetworkPortDescriptionV1::empty());
        description_request.output_ptr =
            (&mut *description as *mut NetworkPortDescriptionV1) as u64;
        description_request.output_len = size_of::<NetworkPortDescriptionV1>() as u64;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                caller,
                &description_request,
                &mut response,
                |map| { map_value(map, &*description, true, true) }
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_OK);
        assert_eq!(
            description.max_frame_bytes as usize,
            NETWORK_PORT_MAX_FRAME_BYTES
        );

        let frame = Box::new([0xA5; NETWORK_PORT_MIN_FRAME_BYTES]);
        let mut send_request = Box::new(network_request(NETWORK_PORT_OP_SEND, send));
        send_request.input_ptr = frame.as_ptr() as u64;
        send_request.input_len = frame.len() as u64;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                caller,
                &send_request,
                &mut response,
                |map| { map_slice(map, &*frame, true, false) }
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_OK);

        let mut output = Box::new([0; NETWORK_PORT_MAX_FRAME_BYTES]);
        let mut receive_request = Box::new(network_request(NETWORK_PORT_OP_TRY_RECEIVE, read));
        receive_request.output_ptr = output.as_mut_ptr() as u64;
        receive_request.output_len = NETWORK_PORT_MAX_FRAME_BYTES as u64;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                caller,
                &receive_request,
                &mut response,
                |map| { map_slice(map, &*output, true, true) }
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_EMPTY);

        receive_request.output_len = (NETWORK_PORT_MAX_FRAME_BYTES - 1) as u64;
        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                caller,
                &receive_request,
                &mut response,
                |_| {}
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_BUFFER_TOO_SMALL);
        assert_eq!(response.required_len, NETWORK_PORT_MAX_FRAME_BYTES as u64);

        let reset = Box::new(network_request(NETWORK_PORT_OP_RESET, write));
        assert_eq!(
            call_network_port(&mut table, &mut port, caller, &reset, &mut response, |_| {}),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.state, NETWORK_PORT_STATE_RESET);

        assert_eq!(
            call_network_port(
                &mut table,
                &mut port,
                caller,
                &send_request,
                &mut response,
                |_| {}
            ),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, NETWORK_PORT_STATUS_NOT_READY);
    }

    #[test]
    fn session_wait_validates_before_and_after_one_non_consuming_sleep() {
        use crate::input_drivers::RawInputEvent;
        use crate::session_input::SessionInputQueue;
        use core::cell::{Cell, RefCell};
        for scenario in 0..14 {
            let process = ActiveUserProcess::new(
                ServiceId::from_raw(7),
                pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID,
                1,
            );
            let caller = Cell::new(Some(process));
            let table = RefCell::new(CapabilityTable::new());
            let input = table
                .borrow_mut()
                .grant(
                    process.service_id(),
                    ResourceId::new(SESSION_INPUT_RESOURCE_ID),
                    RightsMask::new(RightsMask::INPUT),
                )
                .unwrap();
            let console = table
                .borrow_mut()
                .grant(
                    process.service_id(),
                    CONSOLE_COM2_RESOURCE,
                    RightsMask::new(RightsMask::READ | RightsMask::WRITE),
                )
                .unwrap();
            let args = SyscallArgs {
                number: SYSCALL_SESSION_WAIT,
                arg0: pack_syscall_capability(input).raw(),
                arg1: pack_syscall_capability(console).raw(),
                arg2: 0,
                arg3: 0,
                arg4: 0,
            };
            let queue = SessionInputQueue::new();
            if scenario != 9 {
                queue
                    .bind_session_consumer_quiescent(if scenario == 10 {
                        ServiceId::from_raw(8)
                    } else {
                        process.service_id()
                    })
                    .unwrap();
            }
            let event = RawInputEvent::MouseMoved { dx: 3, dy: -2 };
            if matches!(scenario, 1 | 3) {
                queue.publish(event);
            }
            let console_ready = Cell::new(matches!(scenario, 2 | 3 | 9 | 10));
            let sleeps = Cell::new(0);
            let result = session_wait_with(
                args,
                || caller.get().ok_or(SyscallError::BadResult),
                |current| validate_session_wait_with_table(&table.borrow(), current, args),
                |holder| queue.session_ready(holder),
                || console_ready.get(),
                || {
                    sleeps.set(sleeps.get() + 1);
                    // This mutable borrow must succeed: validation cannot retain a borrow.
                    let mut capabilities = table.borrow_mut();
                    match scenario {
                        4 => {
                            queue.publish(event);
                        }
                        5 => {
                            capabilities.revoke(input).unwrap();
                            queue.publish(event);
                        }
                        6 => {
                            capabilities.revoke(console).unwrap();
                            queue.publish(event);
                        }
                        7 => {
                            caller.set(None);
                            queue.publish(event);
                        }
                        8 => {
                            caller.set(Some(ActiveUserProcess::new(
                                process.service_id(),
                                process.principal_id(),
                                2,
                            )));
                            queue.publish(event);
                        }
                        11 => console_ready.set(true),
                        12 => {
                            console_ready.set(true);
                            queue.publish(event);
                        }
                        13 => {
                            let mut map = UserCopyMap::new();
                            map.add_mapping(0x7200_2000, 4096, true, true).unwrap();
                            caller.set(Some(process.with_copy_map(map)));
                            queue.publish(event);
                        }
                        _ => {}
                    }
                },
            );
            if matches!(scenario, 5..=10 | 13) {
                assert!(result.is_err(), "scenario {scenario}");
            } else {
                assert_eq!(
                    result,
                    Ok(match scenario {
                        1 | 4 => 1,
                        2 | 11 => 2,
                        3 | 12 => 3,
                        _ => 0,
                    }),
                    "scenario {scenario}"
                );
            }
            assert_eq!(
                sleeps.get(),
                if matches!(scenario, 1..=3 | 9 | 10) {
                    0
                } else {
                    1
                }
            );
            if matches!(scenario, 1 | 3..=8 | 12 | 13) {
                let delivered = queue
                    .try_read_session(process.service_id())
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    (delivered.sequence, delivered.value0, delivered.value1),
                    (0, 3, -2)
                );
                assert_eq!(queue.try_read_session(process.service_id()), Ok(None));
            }
        }
    }

    #[test]
    fn session_wait_denials_do_not_sleep_or_consume_pending_data() {
        use core::cell::Cell;
        for invalid in 0..15 {
            let mut table = CapabilityTable::new();
            let process = ActiveUserProcess::new(
                ServiceId::from_raw(7),
                pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID,
                1,
            );
            let input = table
                .grant(
                    if invalid == 4 {
                        ServiceId::from_raw(8)
                    } else {
                        process.service_id()
                    },
                    if invalid == 5 {
                        CONSOLE_COM2_RESOURCE
                    } else {
                        ResourceId::new(SESSION_INPUT_RESOURCE_ID)
                    },
                    RightsMask::new(if invalid == 6 {
                        RightsMask::READ
                    } else {
                        RightsMask::INPUT
                    }),
                )
                .unwrap();
            let console = table
                .grant(
                    if invalid == 10 {
                        ServiceId::from_raw(8)
                    } else {
                        process.service_id()
                    },
                    if invalid == 11 {
                        ResourceId::new(99)
                    } else {
                        CONSOLE_COM2_RESOURCE
                    },
                    RightsMask::new(if invalid == 12 {
                        RightsMask::WRITE
                    } else {
                        RightsMask::READ
                    }),
                )
                .unwrap();
            let mut args = SyscallArgs {
                number: SYSCALL_SESSION_WAIT,
                arg0: pack_syscall_capability(input).raw(),
                arg1: pack_syscall_capability(console).raw(),
                arg2: 0,
                arg3: 0,
                arg4: 0,
            };
            match invalid {
                0 => args.arg2 = 1,
                1 => args.arg3 = 1,
                2 => args.arg4 = 1,
                3 => args.arg0 = PackedCapability::from_parts(input.slot(), 99).raw(),
                7 => args.arg0 = PackedCapability::from_parts(999, 1).raw(),
                8 => args.arg1 = PackedCapability::from_parts(console.slot(), 99).raw(),
                9 => args.arg1 = PackedCapability::from_parts(999, 1).raw(),
                14 => {
                    table.revoke(input).unwrap();
                }
                _ => {}
            }
            let caller = if invalid == 13 {
                ActiveUserProcess::new(process.service_id(), 1, 1)
            } else {
                process
            };
            let reads = Cell::new(0);
            let result = session_wait_with(
                args,
                || Ok(caller),
                |current| validate_session_wait_with_table(&table, current, args),
                |_| {
                    reads.set(reads.get() + 1);
                    Ok(true)
                },
                || true,
                || panic!("denied wait slept"),
            );
            assert!(result.is_err(), "invalid {invalid}");
            assert_eq!(reads.get(), 0, "invalid {invalid}");
        }
    }

    #[test]
    fn session_wait_registry_dispatch_is_additive_and_denies_missing_caller() {
        let _guard = process_context_test_lock();
        process_context::clear_current_process();
        let entry = lookup_syscall(SYSCALL_SESSION_WAIT).expect("registered wait");
        assert_eq!((entry.introduced_major, entry.introduced_minor), (1, 2));
        assert!(!entry.proof_only);
        assert_eq!(
            dispatch(SyscallArgs::for_number(SYSCALL_SESSION_WAIT)),
            Err(SyscallError::ProcessContext(
                ProcessContextError::NoActiveProcess
            ))
        );
    }

    #[test]
    fn normal_grants_rollback_every_partial_stage_and_preserve_preexisting_authority() {
        let process = ActiveUserProcess::new(
            ServiceId::from_raw(7),
            pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID,
            1,
        );
        for available in 0..4 {
            let mut table = CapabilityTable::new();
            let mut existing = std::vec::Vec::new();
            for resource in 0..(32 - available) {
                existing.push(
                    table
                        .grant(
                            process.service_id(),
                            ResourceId::new(resource),
                            RightsMask::new(RightsMask::READ),
                        )
                        .unwrap(),
                );
            }
            assert!(matches!(
                grant_normal_session_capabilities_with_table(&mut table, process, |_| panic!(
                    "partial grant bound queue"
                )),
                Err(SyscallError::Capability(CapabilityError::TableFull))
            ));
            for (resource, handle) in existing.into_iter().enumerate() {
                assert_eq!(
                    table.validate(
                        process.service_id(),
                        handle,
                        ResourceId::new(resource as u64),
                        RightsMask::new(RightsMask::READ)
                    ),
                    Ok(())
                );
            }
            for slot in (32 - available)..32 {
                assert_eq!(
                    table.revoke(CapabilityHandle::from_parts(slot as u32, 1)),
                    Err(CapabilityError::InvalidHandle)
                );
            }
        }
        let requests = [
            (
                CONSOLE_COM2_RESOURCE,
                RightsMask::new(RightsMask::READ | RightsMask::WRITE),
            ),
            (
                ResourceId::new(SESSION_INPUT_RESOURCE_ID),
                RightsMask::new(RightsMask::INPUT),
            ),
            (
                ResourceId::new(SESSION_COMMAND_RESOURCE_ID),
                RightsMask::new(RightsMask::READ | RightsMask::APPEND),
            ),
            (
                ResourceId::new(SESSION_VIEWING_RESOURCE_ID),
                RightsMask::new(RightsMask::SEND),
            ),
        ];
        for (stage, (resource, rights)) in requests.into_iter().enumerate() {
            let mut table = CapabilityTable::new();
            let old = table.grant(process.service_id(), resource, rights).unwrap();
            assert!(
                grant_normal_session_capabilities_with_table(&mut table, process, |_| panic!(
                    "reused grant bound queue"
                ))
                .is_err()
            );
            assert_eq!(
                table.validate(process.service_id(), old, resource, rights),
                Ok(())
            );
            for slot in 1..=stage {
                assert_eq!(
                    table.revoke(CapabilityHandle::from_parts(slot as u32, 1)),
                    Err(CapabilityError::InvalidHandle)
                );
            }
        }
        let mut table = CapabilityTable::new();
        assert!(matches!(
            grant_normal_session_capabilities_with_table(&mut table, process, |_| Err(
                SessionInputError::AlreadyBound
            )),
            Err(SyscallError::SessionInput(SessionInputError::AlreadyBound))
        ));
        for slot in 0..4 {
            assert_eq!(
                table.revoke(CapabilityHandle::from_parts(slot, 1)),
                Err(CapabilityError::InvalidHandle)
            );
        }
        let mut table = CapabilityTable::new();
        assert!(
            grant_normal_session_capabilities_with_table(
                &mut table,
                ActiveUserProcess::new(process.service_id(), 0, 1),
                |_| panic!("wrong principal bound")
            )
            .is_err()
        );
        assert_eq!(table, CapabilityTable::new());
    }

    #[test]
    fn normal_grants_cleanup_checks_table_not_only_retired_ownership_flags() {
        let process = ActiveUserProcess::new(
            ServiceId::from_raw(7),
            pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID,
            1,
        );
        for live in 0..4 {
            let mut table = CapabilityTable::new();
            let mut grants =
                grant_normal_session_capabilities_with_table(&mut table, process, |_| Ok(()))
                    .unwrap();
            for (index, handle) in grants.handles.iter().enumerate() {
                if index != live {
                    table.revoke(unpack_syscall_capability(*handle)).unwrap();
                }
            }
            // Simulate incorrect bookkeeping: even without a current caller,
            // no live exact handle may pass the production post-cleanup gate.
            grants.owned = [false; 4];
            assert!(grants.is_revoked());
            assert_eq!(
                revoke_normal_session_capabilities_with_table(&mut table, &mut grants),
                Err(SyscallError::BadResult)
            );
        }
    }

    #[test]
    fn normal_grants_cleanup_is_exact_idempotent_and_attempts_later_handles_on_error() {
        let process = ActiveUserProcess::new(
            ServiceId::from_raw(7),
            pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID,
            1,
        );
        for scenario in 0..3 {
            let stale = scenario != 0;
            let mut table = CapabilityTable::new();
            let queue = crate::session_input::SessionInputQueue::new();
            let mut grants =
                grant_normal_session_capabilities_with_table(&mut table, process, |holder| {
                    queue.bind_session_consumer_quiescent(holder)
                })
                .unwrap();
            assert!(!grants.is_revoked());
            let requests = [
                (
                    grants.console(),
                    CONSOLE_COM2_RESOURCE,
                    RightsMask::new(RightsMask::READ | RightsMask::WRITE),
                ),
                (
                    grants.input(),
                    ResourceId::new(SESSION_INPUT_RESOURCE_ID),
                    RightsMask::new(RightsMask::INPUT),
                ),
                (
                    grants.command(),
                    ResourceId::new(SESSION_COMMAND_RESOURCE_ID),
                    RightsMask::new(RightsMask::READ | RightsMask::APPEND),
                ),
                (
                    grants.presentation(),
                    ResourceId::new(SESSION_VIEWING_RESOURCE_ID),
                    RightsMask::new(RightsMask::SEND),
                ),
            ];
            for (index, (handle, resource, rights)) in requests.into_iter().enumerate() {
                assert_ne!(handle.raw(), 0);
                assert!(!grants.handles[..index].contains(&handle));
                assert_eq!(
                    table.validate(
                        process.service_id(),
                        unpack_syscall_capability(handle),
                        resource,
                        rights
                    ),
                    Ok(())
                );
            }
            let peer = table
                .grant(
                    ServiceId::from_raw(8),
                    ResourceId::new(99),
                    RightsMask::new(RightsMask::READ),
                )
                .unwrap();
            if scenario == 1 {
                table
                    .revoke(unpack_syscall_capability(grants.handles[0]))
                    .unwrap();
            }
            let console = grants.console();
            if scenario == 2 {
                // Inject the previous generation while the replacement remains live.
                grants.handles[0] =
                    PackedCapability::from_parts(unpack_syscall_capability(console).slot(), 0);
            }
            let result = revoke_normal_session_capabilities_with_table(&mut table, &mut grants);
            assert_eq!(result.is_err(), stale);
            assert_eq!(grants.is_revoked(), !stale);
            assert_eq!(
                queue.bind_session_consumer_quiescent(process.service_id()),
                Err(SessionInputError::AlreadyBound)
            );
            assert_eq!(
                queue.session_ready(ServiceId::from_raw(8)),
                Err(SessionInputError::WrongHolder)
            );
            if scenario == 2 {
                assert_eq!(
                    table.validate(
                        process.service_id(),
                        unpack_syscall_capability(console),
                        CONSOLE_COM2_RESOURCE,
                        RightsMask::new(RightsMask::READ)
                    ),
                    Ok(())
                );
            }
            for handle in grants.handles {
                assert_eq!(
                    table.revoke(unpack_syscall_capability(handle)),
                    Err(CapabilityError::InvalidHandle)
                );
            }
            assert_eq!(
                table.validate(
                    ServiceId::from_raw(8),
                    peer,
                    ResourceId::new(99),
                    RightsMask::new(RightsMask::READ)
                ),
                Ok(())
            );
            if !stale {
                assert!(!grants.owned.into_iter().any(|owned| owned));
                let before = table;
                assert_eq!(
                    revoke_normal_session_capabilities_with_table(&mut table, &mut grants),
                    Ok(())
                );
                assert_eq!(table, before);
            } else {
                // A newer slot generation must not be revoked by stale cleanup.
                let before = table;
                assert!(
                    revoke_normal_session_capabilities_with_table(&mut table, &mut grants).is_err()
                );
                assert_eq!(table, before);
            }
        }
    }

    #[test]
    fn session_presentation_grant_is_separate_and_only_for_runtime_principal() {
        let mut table = CapabilityTable::new();
        let service = ServiceId::from_raw(7);
        for principal in [0, 0x5059_5448_534D_0001] {
            assert!(
                grant_session_presentation_capability_with_table(
                    &mut table,
                    ActiveUserProcess::new(service, principal, 1)
                )
                .is_err()
            );
        }
        let runtime = ActiveUserProcess::new(
            service,
            pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID,
            1,
        );
        let cap = grant_session_presentation_capability_with_table(&mut table, runtime).unwrap();
        assert_eq!(
            validate_syscall_capability_with_table(
                &table,
                runtime,
                cap,
                ResourceId::new(SESSION_VIEWING_RESOURCE_ID),
                RightsMask::new(RightsMask::SEND)
            ),
            Ok(())
        );
        assert!(
            validate_syscall_capability_with_table(
                &table,
                runtime,
                cap,
                ResourceId::new(SESSION_INPUT_RESOURCE_ID),
                RightsMask::new(RightsMask::INPUT)
            )
            .is_err()
        );
    }

    #[test]
    fn session_presentation_syscall_requires_live_caller_generation_holder_resource_and_send() {
        use crate::session_presentation::{PresentationService, tests::fixture};
        use crate::viewing::ViewingExtent;
        let _guard = process_context_test_lock();
        let holder = ServiceId::from_raw(7);
        let process = ActiveUserProcess::new(
            holder,
            pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID,
            1,
        );
        for case in 0..8 {
            let (pixels, info) = fixture();
            let mut service = PresentationService::new();
            // SAFETY:
            // 1. Invariant: fixture metadata names this test's writable pixels.
            // 2. Established by: fixture derives address/length from a real Vec.
            // 3. Lifetime: pixels outlives the service throughout this iteration.
            // 4. Pointer ownership: the local Vec is exclusively test-owned.
            // 5. Alignment: Vec<u32> supplies at least 4-byte pixel alignment.
            // 6. Mapped length: metadata covers all 648 * 484 allocated pixels.
            // 7. Concurrency: process-context lock serializes caller changes;
            //    other tests do not share this service or framebuffer.
            // 8. Violation: dropping/resizing pixels could invalidate the base.
            unsafe {
                service
                    .bind(holder, info, ViewingExtent::new(640, 480).unwrap())
                    .unwrap();
            }
            service.present(holder, 0, 1, (240 << 32) | 320, 0).unwrap();
            let before = pixels.clone();
            let accepted = service.accepted_snapshot();
            let mut table = CapabilityTable::new();
            let resource = if case == 4 {
                SESSION_INPUT_RESOURCE_ID
            } else {
                SESSION_VIEWING_RESOURCE_ID
            };
            let rights = if case == 5 {
                RightsMask::INPUT
            } else {
                RightsMask::SEND
            };
            let handle = table
                .grant(holder, ResourceId::new(resource), RightsMask::new(rights))
                .unwrap();
            let mut cap = pack_syscall_capability(handle);
            let mut caller = process;
            match case {
                1 => cap = PackedCapability::from_parts(31, 1),
                2 => {
                    table.revoke(handle).unwrap();
                }
                3 => {
                    caller =
                        ActiveUserProcess::new(ServiceId::from_raw(8), process.principal_id(), 1)
                }
                6 => caller = ActiveUserProcess::new(holder, 0x5059_5448_534D_0001, 1),
                _ => {}
            }
            if case == 0 {
                process_context::clear_current_process();
            } else {
                process_context::bind_current_process(caller);
            }
            let result = dispatch_session_viewing_present_with_table(
                SyscallArgs {
                    number: SYSCALL_SESSION_VIEWING_PRESENT,
                    arg0: cap.raw(),
                    arg1: 1,
                    arg2: 1,
                    arg3: (233 << 32) | 327,
                    arg4: 0,
                },
                &table,
                |who, revision, flags, coordinates, reserved| {
                    service.present(who, revision, flags, coordinates, reserved)
                },
            );
            if case == 7 {
                assert_eq!(result, Ok(SYSCALL_OK));
                assert_ne!(pixels, before);
                assert_eq!(service.accepted_snapshot().unwrap().0, 1);
            } else {
                assert!(result.is_err(), "case {case}");
                assert_eq!(pixels, before, "case {case}");
                assert_eq!(service.accepted_snapshot(), accepted, "case {case}");
            }
        }
        process_context::clear_current_process();
    }

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
        assert_eq!(SYSCALL_ABI_MINOR, 2);
        assert_eq!(SYSCALL_ABI_INFO, 0x5059_0000);
        assert_eq!(abi_info_result(), 0x5059_0001_0002);
    }

    #[test]
    fn session_input_syscall_is_registry_version_1_1() {
        assert_eq!(SYSCALL_ABI_MAJOR, 1);
        assert_eq!(SYSCALL_ABI_MINOR, 2);
        let entry = lookup_syscall(SYSCALL_SESSION_INPUT_TRY_READ).unwrap();
        assert_eq!((entry.introduced_major, entry.introduced_minor), (1, 1));
        assert!(!entry.proof_only);
    }

    #[test]
    fn session_command_capability_grant_is_exact_and_rejects_invalid_handles() {
        let holder = ActiveUserProcess::new(ServiceId::from_raw(0x5059_5345_5353_0001), 1, 1);
        let stranger = ActiveUserProcess::new(ServiceId::from_raw(0x5059_5345_5353_0002), 2, 2);
        let resource = ResourceId::new(SESSION_COMMAND_RESOURCE_ID);
        let required = RightsMask::new(RightsMask::READ | RightsMask::APPEND);
        let mut table = CapabilityTable::new();

        let capability = grant_session_command_capability_with_table(&mut table, holder).unwrap();
        assert_eq!(
            validate_syscall_capability_with_table(&table, holder, capability, resource, required),
            Ok(())
        );
        assert_eq!(
            validate_syscall_capability_with_table(
                &table, stranger, capability, resource, required
            ),
            Err(SyscallError::Capability(CapabilityError::WrongHolder))
        );
        assert_eq!(
            validate_syscall_capability_with_table(
                &table,
                holder,
                capability,
                ResourceId::new(SESSION_COMMAND_RESOURCE_ID + 1),
                required
            ),
            Err(SyscallError::Capability(CapabilityError::WrongResource))
        );

        let missing_read = pack_syscall_capability(
            table
                .grant(
                    holder.service_id(),
                    resource,
                    RightsMask::new(RightsMask::APPEND),
                )
                .unwrap(),
        );
        let missing_append = pack_syscall_capability(
            table
                .grant(
                    holder.service_id(),
                    resource,
                    RightsMask::new(RightsMask::READ),
                )
                .unwrap(),
        );
        assert_eq!(
            validate_syscall_capability_with_table(
                &table,
                holder,
                missing_read,
                resource,
                required
            ),
            Err(SyscallError::Capability(CapabilityError::MissingRights))
        );
        assert_eq!(
            validate_syscall_capability_with_table(
                &table,
                holder,
                missing_append,
                resource,
                required
            ),
            Err(SyscallError::Capability(CapabilityError::MissingRights))
        );

        let stale = unpack_syscall_capability(capability);
        table.revoke(stale).unwrap();
        assert_eq!(
            validate_syscall_capability_with_table(&table, holder, capability, resource, required),
            Err(SyscallError::Capability(CapabilityError::InvalidHandle))
        );
        let forged_slot = PackedCapability::from_parts(u32::MAX, stale.generation());
        assert_eq!(
            validate_syscall_capability_with_table(&table, holder, forged_slot, resource, required),
            Err(SyscallError::Capability(CapabilityError::InvalidHandle))
        );
    }

    #[test]
    fn session_input_syscall_denial_keeps_the_pending_event_unread() {
        #[derive(Clone, Copy)]
        enum Denial {
            MissingCaller,
            ForgedSlot,
            StaleGeneration,
            WrongHolder,
            WrongResource,
            MissingInputRight,
        }

        let _process_guard = process_context_test_lock();
        let process = session_input_process();
        let intruder = session_input_intruder_process();
        let event = session_input_test_event();
        for (denial, expected) in [
            (
                Denial::MissingCaller,
                SyscallError::ProcessContext(ProcessContextError::NoActiveProcess),
            ),
            (
                Denial::ForgedSlot,
                SyscallError::Capability(CapabilityError::InvalidHandle),
            ),
            (
                Denial::StaleGeneration,
                SyscallError::Capability(CapabilityError::InvalidHandle),
            ),
            (
                Denial::WrongHolder,
                SyscallError::Capability(CapabilityError::WrongHolder),
            ),
            (
                Denial::WrongResource,
                SyscallError::Capability(CapabilityError::WrongResource),
            ),
            (
                Denial::MissingInputRight,
                SyscallError::Capability(CapabilityError::MissingRights),
            ),
        ] {
            let mut capabilities = CapabilityTable::new();
            let mut output = Box::new(session_input_sentinel());
            let sentinel_bytes = session_input_bytes(&output);
            let mut copy_map = UserCopyMap::new();
            map_value(&mut copy_map, &*output, true, true);
            let capability = match denial {
                Denial::MissingCaller => PackedCapability::from_raw(0),
                Denial::ForgedSlot => PackedCapability::from_parts(31, 1),
                Denial::StaleGeneration => {
                    let handle = capabilities
                        .grant(
                            process.service_id(),
                            ResourceId::new(SESSION_INPUT_RESOURCE_ID),
                            RightsMask::new(RightsMask::INPUT),
                        )
                        .unwrap();
                    capabilities.revoke(handle).unwrap();
                    pack_syscall_capability(handle)
                }
                Denial::WrongHolder => grant_session_input_for_test(
                    &mut capabilities,
                    process,
                    ResourceId::new(SESSION_INPUT_RESOURCE_ID),
                    RightsMask::new(RightsMask::INPUT),
                ),
                Denial::WrongResource => grant_session_input_for_test(
                    &mut capabilities,
                    process,
                    ResourceId::new(SESSION_INPUT_RESOURCE_ID ^ 1),
                    RightsMask::new(RightsMask::INPUT),
                ),
                Denial::MissingInputRight => grant_session_input_for_test(
                    &mut capabilities,
                    process,
                    ResourceId::new(SESSION_INPUT_RESOURCE_ID),
                    RightsMask::new(RightsMask::READ),
                ),
            };
            match denial {
                Denial::MissingCaller => process_context::clear_current_process(),
                Denial::WrongHolder => {
                    process_context::bind_current_process(intruder.with_copy_map(copy_map));
                }
                _ => process_context::bind_current_process(process.with_copy_map(copy_map)),
            }

            let mut pending = Some(event);
            let mut reader_calls = 0;
            assert_eq!(
                dispatch_session_input_try_read_with_table(
                    session_input_args(capability, &mut output, session_input_event_len()),
                    &capabilities,
                    |_| {
                        reader_calls += 1;
                        Ok(pending.take())
                    },
                ),
                Err(expected)
            );
            assert_eq!(reader_calls, 0);
            assert_eq!(pending, Some(event));
            assert_eq!(*output, session_input_sentinel());
            assert_eq!(session_input_bytes(&output), sentinel_bytes);
        }
        process_context::clear_current_process();
    }

    #[test]
    fn session_input_syscall_rejects_bad_output_shapes_before_dequeue() {
        #[derive(Clone, Copy)]
        enum Shape {
            WrongLength,
            NonzeroArg3,
            NonzeroArg4,
            Misaligned,
            ReadOnly,
            OutOfMap,
            BelowUserRange,
            LengthOverflow,
            TruncatedMapping,
            CrossMapping,
            NonCanonicalMapped,
        }

        let _process_guard = process_context_test_lock();
        let process = session_input_process();
        let event = session_input_test_event();
        for (shape, expected) in [
            (Shape::WrongLength, SyscallError::BadResult),
            (Shape::NonzeroArg3, SyscallError::BadResult),
            (Shape::NonzeroArg4, SyscallError::BadResult),
            (Shape::Misaligned, SyscallError::BadResult),
            (
                Shape::ReadOnly,
                SyscallError::UserCopy(UserCopyError::PermissionDenied),
            ),
            (
                Shape::OutOfMap,
                SyscallError::UserCopy(UserCopyError::OutOfRange),
            ),
            (Shape::BelowUserRange, SyscallError::BadResult),
            (Shape::LengthOverflow, SyscallError::BadResult),
            (
                Shape::TruncatedMapping,
                SyscallError::UserCopy(UserCopyError::OutOfRange),
            ),
            (
                Shape::CrossMapping,
                SyscallError::UserCopy(UserCopyError::CrossMapping),
            ),
            (Shape::NonCanonicalMapped, SyscallError::BadResult),
        ] {
            let mut capabilities = CapabilityTable::new();
            let capability = grant_session_input_for_test(
                &mut capabilities,
                process,
                ResourceId::new(SESSION_INPUT_RESOURCE_ID),
                RightsMask::new(RightsMask::INPUT),
            );
            let mut output = Box::new(session_input_sentinel());
            let sentinel_bytes = session_input_bytes(&output);
            let mut copy_map = UserCopyMap::new();
            let mut args = session_input_args(capability, &mut output, session_input_event_len());
            match shape {
                Shape::WrongLength => {
                    args.arg2 -= 1;
                    map_value(&mut copy_map, &*output, true, true);
                }
                Shape::NonzeroArg3 => {
                    args.arg3 = 1;
                    map_value(&mut copy_map, &*output, true, true);
                }
                Shape::NonzeroArg4 => {
                    args.arg4 = 1;
                    map_value(&mut copy_map, &*output, true, true);
                }
                Shape::Misaligned => {
                    args.arg1 += 1;
                    copy_map
                        .add_mapping(args.arg1, session_input_event_len(), true, true)
                        .unwrap();
                }
                Shape::ReadOnly => map_value(&mut copy_map, &*output, true, false),
                Shape::OutOfMap => {}
                Shape::BelowUserRange => {
                    args.arg1 = USER_VIRT_MIN - session_input_event_len();
                    copy_map
                        .add_mapping(args.arg1, session_input_event_len(), true, true)
                        .unwrap();
                }
                Shape::LengthOverflow => {
                    args.arg1 = u64::MAX - 7;
                }
                Shape::TruncatedMapping => {
                    copy_map
                        .add_mapping(args.arg1, session_input_event_len() - 1, true, true)
                        .unwrap();
                }
                Shape::CrossMapping => {
                    copy_map.add_mapping(args.arg1, 8, true, true).unwrap();
                    copy_map
                        .add_mapping(args.arg1 + 8, session_input_event_len() - 8, true, true)
                        .unwrap();
                }
                Shape::NonCanonicalMapped => {
                    args.arg1 = USER_VIRT_MAX;
                    copy_map
                        .add_mapping(args.arg1, session_input_event_len(), true, true)
                        .unwrap();
                }
            }
            process_context::bind_current_process(process.with_copy_map(copy_map));

            let mut pending = Some(event);
            let mut reader_calls = 0;
            assert_eq!(
                dispatch_session_input_try_read_with_table(args, &capabilities, |_| {
                    reader_calls += 1;
                    Ok(pending.take())
                }),
                Err(expected)
            );
            assert_eq!(reader_calls, 0);
            assert_eq!(pending, Some(event));
            assert_eq!(*output, session_input_sentinel());
            assert_eq!(session_input_bytes(&output), sentinel_bytes);
        }
        process_context::clear_current_process();
    }

    #[test]
    fn session_input_syscall_returns_empty_without_writing_and_copies_one_exact_event() {
        let _process_guard = process_context_test_lock();
        let process = session_input_process();
        let mut capabilities = CapabilityTable::new();
        let capability = grant_session_input_for_test(
            &mut capabilities,
            process,
            ResourceId::new(SESSION_INPUT_RESOURCE_ID),
            RightsMask::new(RightsMask::INPUT),
        );
        let mut output = Box::new(session_input_sentinel());
        let sentinel_bytes = session_input_bytes(&output);
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*output, true, true);
        process_context::bind_current_process(process.with_copy_map(copy_map));
        let args = session_input_args(capability, &mut output, session_input_event_len());

        assert_eq!(
            dispatch_session_input_try_read_with_table(args, &capabilities, |_| Ok(None)),
            Ok(SESSION_INPUT_RESULT_EMPTY)
        );
        assert_eq!(*output, session_input_sentinel());
        assert_eq!(session_input_bytes(&output), sentinel_bytes);

        let event = session_input_test_event();
        assert_eq!(
            dispatch_session_input_try_read_with_table(args, &capabilities, |_| Ok(Some(event))),
            Ok(SESSION_INPUT_RESULT_EVENT)
        );
        assert_eq!(*output, event);
        process_context::clear_current_process();
    }

    #[test]
    fn session_input_denial_revokes_the_grant_when_binding_fails() {
        let process = session_input_process();
        let mut capabilities = CapabilityTable::new();
        let mut bind_calls = 0;

        assert_eq!(
            bind_session_input_capability_with_table(&mut capabilities, process, |_| {
                bind_calls += 1;
                Err(SessionInputError::AlreadyBound)
            },),
            Err(SyscallError::SessionInput(SessionInputError::AlreadyBound))
        );
        assert_eq!(bind_calls, 1);
        assert_eq!(
            capabilities.validate(
                process.service_id(),
                CapabilityHandle::from_parts(0, 1),
                ResourceId::new(SESSION_INPUT_RESOURCE_ID),
                RightsMask::new(RightsMask::INPUT),
            ),
            Err(CapabilityError::InvalidHandle)
        );
    }

    #[test]
    fn session_input_repeated_bind_failure_preserves_the_committed_capability() {
        let process = session_input_process();
        let mut capabilities = CapabilityTable::new();
        let mut bound = None;

        let first =
            bind_session_input_capability_with_table(&mut capabilities, process, |holder| {
                assert_eq!(bound, None);
                bound = Some(holder);
                Ok(())
            })
            .unwrap();
        assert_eq!(bound, Some(process.service_id()));
        assert_eq!(
            bind_session_input_capability_with_table(&mut capabilities, process, |holder| {
                assert_eq!(bound, Some(holder));
                Err(SessionInputError::AlreadyBound)
            }),
            Err(SyscallError::SessionInput(SessionInputError::AlreadyBound))
        );
        assert_eq!(
            validate_syscall_capability_with_table(
                &capabilities,
                process,
                first,
                ResourceId::new(SESSION_INPUT_RESOURCE_ID),
                RightsMask::new(RightsMask::INPUT),
            ),
            Ok(())
        );
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
    fn package_context_syscall_denies_non_package_process() {
        let _guard = package_context_syscall_test_lock();
        let package_process = package_context_process();
        let non_package_process = package_context_intruder_process();
        let service = package_context_service_with_launch_context(package_process);
        let _service_guard =
            crate::package_service::initialize_retained_package_service_for_phase13_test(service);
        let mut output = Box::new(empty_package_schema_binding_output());
        let original_output = *output;
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*output, true, true);
        process_context::bind_current_process(non_package_process.with_copy_map(copy_map));

        assert_eq!(
            dispatch(package_context_args(0, &mut output, package_binding_len())),
            Ok(u64::from(PackageStatus::Denied as u16))
        );
        assert_eq!(*output, original_output);
    }

    #[test]
    fn package_context_syscall_copies_schema_slot_zero_binding() {
        let _guard = package_context_syscall_test_lock();
        let package_process = package_context_process();
        let service = package_context_service_with_launch_context(package_process);
        let _service_guard =
            crate::package_service::initialize_retained_package_service_for_phase13_test(service);
        let mut output = Box::new(empty_package_schema_binding_output());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*output, true, true);
        process_context::bind_current_process(package_process.with_copy_map(copy_map));

        assert_eq!(
            dispatch(package_context_args(0, &mut output, package_binding_len())),
            Ok(u64::from(PackageStatus::Ok as u16))
        );
        assert_eq!(output.abi_major, 0);
        assert_eq!(output.abi_minor, 1);
        assert_eq!(output.schema_slot, 0);
        assert_eq!(output.reserved0, 0);
        assert_eq!(output.package_object_id, PACKAGE_CONTEXT_PACKAGE_ID);
        assert_eq!(output.package_revision, PACKAGE_CONTEXT_PACKAGE_REVISION);
        assert_eq!(output.schema_object_id, PACKAGE_CONTEXT_SCHEMA_ID);
        assert_eq!(output.schema_revision, PACKAGE_CONTEXT_SCHEMA_REVISION);
        assert_eq!(
            output.schema_descriptor_sha256,
            PACKAGE_CONTEXT_SCHEMA_DESCRIPTOR_DIGEST
        );
        assert_eq!(output.reserved1, [0; 16]);
    }

    #[test]
    fn package_context_syscall_uses_phase13_retained_package_service() {
        let _guard = package_context_syscall_test_lock();
        let package_process = package_context_process();
        let service = package_context_service_with_launch_context(package_process);
        let _service_guard =
            crate::package_service::initialize_retained_package_service_for_phase13_test(service);
        let mut output = Box::new(empty_package_schema_binding_output());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*output, true, true);
        process_context::bind_current_process(package_process.with_copy_map(copy_map));

        assert_eq!(
            dispatch(package_context_args(0, &mut output, package_binding_len())),
            Ok(u64::from(PackageStatus::Ok as u16))
        );
        assert_eq!(output.package_object_id, PACKAGE_CONTEXT_PACKAGE_ID);
        assert_eq!(output.schema_object_id, PACKAGE_CONTEXT_SCHEMA_ID);
        assert_eq!(
            output.schema_descriptor_sha256,
            PACKAGE_CONTEXT_SCHEMA_DESCRIPTOR_DIGEST
        );
    }

    #[test]
    fn package_context_syscall_rejects_wrong_output_length_without_writing() {
        let _guard = package_context_syscall_test_lock();
        let package_process = package_context_process();
        let service = package_context_service_with_launch_context(package_process);
        let _service_guard =
            crate::package_service::initialize_retained_package_service_for_phase13_test(service);
        let mut output = Box::new(empty_package_schema_binding_output());
        let original_output = *output;
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*output, true, true);
        process_context::bind_current_process(package_process.with_copy_map(copy_map));

        assert_eq!(
            dispatch(package_context_args(
                0,
                &mut output,
                package_binding_len() - 1
            )),
            Ok(u64::from(PackageStatus::BufferTooSmall as u16))
        );
        assert_eq!(*output, original_output);
    }

    #[test]
    fn package_context_syscall_does_not_mutate_package_state() {
        let _guard = package_context_syscall_test_lock();
        let package_process = package_context_process();
        let service = package_context_service_with_launch_context(package_process);
        let _service_guard =
            crate::package_service::initialize_retained_package_service_for_phase13_test(service);
        let before = package_context_state_snapshot();
        let mut output = Box::new(empty_package_schema_binding_output());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*output, true, true);
        process_context::bind_current_process(package_process.with_copy_map(copy_map));

        assert_eq!(
            dispatch(package_context_args(0, &mut output, package_binding_len())),
            Ok(u64::from(PackageStatus::Ok as u16))
        );

        let after = package_context_state_snapshot();
        assert_eq!(after, before);
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
    fn console_read_prefers_com2_then_physical_keyboard_then_no_byte() {
        let mut physical_polled = false;
        assert_eq!(
            console_read_result(Some(b'c'), || {
                physical_polled = true;
                Some(b'p')
            }),
            u64::from(b'c')
        );
        assert!(!physical_polled);
        assert_eq!(console_read_result(None, || Some(b'p')), u64::from(b'p'));
        assert_eq!(console_read_result(None, || None), NO_BYTE);
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
    fn package_defined_object_syscall_creates_with_schema_ref_and_inline_state() {
        let _process_guard = process_context_test_lock();
        let (service, shell, workspace, schema) = package_defined_schema_fixture();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let _package_guard =
            retained_package_service_with_schema(schema.object_id, schema.revision);
        let state = Box::new(*b"seed-state");
        let create = Box::new(package_defined_create_record(
            schema.object_id,
            schema.revision,
            PACKAGE_DEFINED_STATE_FORMAT_INLINE_BYTES_V0,
            state.as_ptr() as u64,
            state.len() as u64,
        ));
        let request = Box::new(package_defined_create_request(workspace, &create));
        let mut response = Box::new(empty_test_response());
        bind_package_defined_copy_map(shell, &request, &response, &create, Some(&state[..]));

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, STATUS_OK);
        assert_eq!(response.object_kind, OBJECT_KIND_PACKAGE_DEFINED_OBJECT);
        assert_eq!(response.revision, 1);
        assert_ne!(response.object_id, 0);
        assert_ne!(response.capability.raw(), 0);

        retained_services::with_object_service(|service| {
            let inspection = service
                .inspect_object(
                    shell,
                    response.capability,
                    ObjectId::new(response.object_id),
                )
                .unwrap();
            let schema_ref = inspection
                .field_bytes(FIELD_PACKAGE_SCHEMA_REF_V0)
                .expect("schema ref field");
            let inline_state = inspection
                .field_bytes(FIELD_PACKAGE_INLINE_STATE_V0)
                .expect("inline state field");
            assert_eq!(
                inspection.object.object_kind(),
                ObjectKind::PackageDefinedObject
            );
            assert_eq!(inspection.revision, 1);
            assert_eq!(&schema_ref[..8], &schema.object_id.raw().to_le_bytes());
            assert_eq!(&schema_ref[8..16], &schema.revision.to_le_bytes());
            assert_eq!(
                inspection.field_value_len(FIELD_PACKAGE_SCHEMA_REF_V0),
                Some(16)
            );
            assert_eq!(
                inspection.field_value_len(FIELD_PACKAGE_INLINE_STATE_V0),
                Some(state.len() as u16)
            );
            assert_eq!(&inline_state[..state.len()], &state[..]);
        })
        .unwrap();
    }

    #[test]
    fn package_defined_object_syscall_registers_schema_retention_from_created_object() {
        let _process_guard = process_context_test_lock();
        let (service, shell, workspace, schema) = package_defined_schema_fixture();
        let _object_guard = retained_services::initialize_object_service_for_test(service);
        let mut package_service = PackageService::new_empty_for_test();
        package_service
            .seed_schema_descriptor_content_for_test(
                7000,
                1,
                [0xA5; 32],
                schema.object_id.raw(),
                schema.revision,
                [7; 32],
            )
            .unwrap();
        let _package_guard =
            package_service::initialize_retained_package_service_for_phase13_test(package_service);
        let create = Box::new(package_defined_create_record(
            schema.object_id,
            schema.revision,
            PACKAGE_DEFINED_STATE_FORMAT_EMPTY,
            0,
            0,
        ));
        let request = Box::new(package_defined_create_request(workspace, &create));
        let mut response = Box::new(empty_test_response());
        bind_package_defined_copy_map(shell, &request, &response, &create, None);

        assert_eq!(
            package_service::with_retained_package_service_for_phase13(|service| {
                service
                    .schema_descriptor_retention_count_for_test(schema.object_id, schema.revision)
            }),
            Some(Some(0))
        );

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );
        assert_eq!(response.status, STATUS_OK);
        assert_ne!(response.object_id, 0);

        assert_eq!(
            package_service::with_retained_package_service_for_phase13(|service| {
                service
                    .schema_descriptor_retention_count_for_test(schema.object_id, schema.revision)
            }),
            Some(Some(1))
        );
    }

    #[test]
    fn package_defined_object_syscall_denies_unretainable_schema_without_creation() {
        let _process_guard = process_context_test_lock();
        let (service, shell, workspace, schema) = package_defined_schema_fixture();
        let _object_guard = retained_services::initialize_object_service_for_test(service);
        let _package_guard = package_service::initialize_retained_package_service_for_phase13_test(
            PackageService::new_empty_for_test(),
        );
        let create = Box::new(package_defined_create_record(
            schema.object_id,
            schema.revision,
            PACKAGE_DEFINED_STATE_FORMAT_EMPTY,
            0,
            0,
        ));
        let request = Box::new(package_defined_create_request(workspace, &create));
        let mut response = Box::new(empty_test_response());
        bind_package_defined_copy_map(shell, &request, &response, &create, None);

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );

        assert_eq!(response.status, STATUS_NOT_FOUND);
        assert_eq!(response.object_id, 0);
        let references = retained_services::with_object_service(|service| {
            service.package_defined_schema_references().unwrap()
        })
        .unwrap();
        assert!(references.iter().all(Option::is_none));
    }

    #[test]
    fn package_launch_object_service_grant_reaches_package_defined_object_syscall() {
        let _process_guard = process_context_test_lock();
        let mut object_service = ObjectService::new_for_test();
        let package_process = ActiveUserProcess::new(
            ServiceId::from_raw(0x5059_504B_4C47_5A01),
            0x504B_5A01,
            0x13,
        );
        let workspace = object_service
            .grant_workspace_capability(package_process)
            .unwrap();
        let schema = object_service
            .create_schema_definition_object(
                package_process,
                ObjectId::new(0x5059_5343_484F_5A11),
                ObjectId::new(0x5059_504B_474F_5A11),
                [0x5A; 32],
            )
            .unwrap();
        let mut package_service = PackageService::new_empty_for_test();
        package_service
            .seed_launch_export_for_test(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                b"seed",
                b"object-create",
                0x5059_504B_474F_5A11,
                1,
                [0xA5; 32],
                schema.object_id.raw(),
                schema.revision,
                [0x5A; 32],
            )
            .unwrap();
        let requirement = PackageLaunchRequirement {
            requirement_id: 7,
            graph_import_slot: 0,
            resource: ResourceId::new(SHELL_WORKSPACE_OBJECT_ID),
            rights: RightsMask::new(RightsMask::WRITE),
        };
        package_service
            .record_launch_requirement(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                "seed/object-create",
                requirement,
            )
            .unwrap();
        let supplied_grants = [PackageLaunchGrant::from_packed(
            requirement.requirement_id,
            workspace,
        )];
        let launch = package_service
            .launch_with_validator(
                PackageLaunchRequest {
                    caller: package_process,
                    namespace_root: ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                    locator: "seed/object-create",
                    supplied_grants: &supplied_grants,
                },
                &object_service,
            )
            .unwrap();
        let graph_grant = launch.graph_import_grant(0).unwrap();
        let _guard = retained_services::initialize_object_service_for_test(object_service);
        let _package_guard =
            retained_package_service_with_schema(schema.object_id, schema.revision);
        let create = Box::new(package_defined_create_record(
            schema.object_id,
            schema.revision,
            PACKAGE_DEFINED_STATE_FORMAT_EMPTY,
            0,
            0,
        ));
        let request = Box::new(package_defined_create_request(
            graph_grant.capability,
            &create,
        ));
        let mut response = Box::new(empty_test_response());
        bind_package_defined_copy_map(package_process, &request, &response, &create, None);

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );
        assert_eq!(launch.grant(0), Some(supplied_grants[0]));
        assert_eq!(graph_grant.capability, workspace);
        assert_eq!(response.status, STATUS_OK);
        assert_eq!(response.object_kind, OBJECT_KIND_PACKAGE_DEFINED_OBJECT);
        assert_ne!(response.object_id, 0);
    }

    #[test]
    fn package_defined_object_syscall_denies_invalid_schema_revision_without_creation() {
        let _process_guard = process_context_test_lock();
        let (service, shell, workspace, schema) = package_defined_schema_fixture();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let create = Box::new(package_defined_create_record(
            schema.object_id,
            schema.revision + 1,
            PACKAGE_DEFINED_STATE_FORMAT_EMPTY,
            0,
            0,
        ));
        let request = Box::new(package_defined_create_request(workspace, &create));
        let mut response = Box::new(empty_test_response());
        bind_package_defined_copy_map(shell, &request, &response, &create, None);

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );

        assert_eq!(response.status, STATUS_NOT_FOUND);
        assert_eq!(response.object_id, 0);
        assert!(!retained_object_exists(ObjectId::new(1042)));
    }

    #[test]
    fn package_defined_object_syscall_denies_nonzero_reserved_fields_without_creation() {
        let _process_guard = process_context_test_lock();
        let (service, shell, workspace, schema) = package_defined_schema_fixture();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let mut create = Box::new(package_defined_create_record(
            schema.object_id,
            schema.revision,
            PACKAGE_DEFINED_STATE_FORMAT_EMPTY,
            0,
            0,
        ));
        create.reserved0 = 1;
        let request = Box::new(package_defined_create_request(workspace, &create));
        let mut response = Box::new(empty_test_response());
        bind_package_defined_copy_map(shell, &request, &response, &create, None);

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );

        assert_eq!(response.status, STATUS_BAD_REQUEST);
        assert_eq!(response.object_id, 0);
        assert!(!retained_object_exists(ObjectId::new(1042)));
    }

    #[test]
    fn package_defined_object_syscall_denies_oversized_inline_state_without_creation() {
        let _process_guard = process_context_test_lock();
        let (service, shell, workspace, schema) = package_defined_schema_fixture();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let state = Box::new([0x5Au8; 17]);
        let create = Box::new(package_defined_create_record(
            schema.object_id,
            schema.revision,
            PACKAGE_DEFINED_STATE_FORMAT_INLINE_BYTES_V0,
            state.as_ptr() as u64,
            state.len() as u64,
        ));
        let request = Box::new(package_defined_create_request(workspace, &create));
        let mut response = Box::new(empty_test_response());
        bind_package_defined_copy_map(shell, &request, &response, &create, Some(&state[..]));

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );

        assert_eq!(response.status, STATUS_BAD_REQUEST);
        assert_eq!(response.object_id, 0);
        assert!(!retained_object_exists(ObjectId::new(1042)));
    }

    #[test]
    fn package_defined_object_syscall_preserves_legacy_note_create_behavior() {
        let _process_guard = process_context_test_lock();
        let service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let _guard = retained_services::initialize_object_service_for_test(service);
        let request = Box::new(object_request(OP_CREATE_OBJECT, workspace));
        let mut response = Box::new(empty_test_response());
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, &*request, true, false);
        map_value(&mut copy_map, &*response, true, true);
        process_context::bind_current_process(shell.with_copy_map(copy_map));

        assert_eq!(
            dispatch_object_request(object_args(&request, &mut response)),
            Ok(SYSCALL_OK)
        );

        assert_eq!(response.status, STATUS_OK);
        assert_eq!(response.object_kind, OBJECT_KIND_NOTE);
        assert_eq!(response.revision, 1);
        retained_services::with_object_service(|service| {
            let inspection = service
                .inspect_object(
                    shell,
                    response.capability,
                    ObjectId::new(response.object_id),
                )
                .unwrap();
            assert_eq!(inspection.object.object_kind(), ObjectKind::Note);
        })
        .unwrap();
    }

    #[test]
    fn object_request_rejects_unmapped_request_before_service_mutation() {
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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
        let _process_guard = process_context_test_lock();
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

    fn package_defined_schema_fixture() -> (
        ObjectService,
        ActiveUserProcess,
        PackedCapability,
        crate::object_service::ObjectCreateResult,
    ) {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let schema = service
            .create_schema_definition_object(
                shell,
                ObjectId::new(7001),
                ObjectId::new(7000),
                [7; 32],
            )
            .unwrap();
        (service, shell, workspace, schema)
    }

    fn retained_package_service_with_schema(
        schema_object_id: ObjectId,
        schema_revision: u64,
    ) -> package_service::RetainedPackageServicePhase13TestGuard {
        let mut package_service = PackageService::new_empty_for_test();
        package_service
            .seed_schema_descriptor_content_for_test(
                7000,
                1,
                [0xA5; 32],
                schema_object_id.raw(),
                schema_revision,
                [7; 32],
            )
            .unwrap();
        package_service::initialize_retained_package_service_for_phase13_test(package_service)
    }

    const fn package_defined_create_record(
        schema_object_id: ObjectId,
        schema_revision: u64,
        state_format: u16,
        initial_state_ptr: u64,
        initial_state_len: u64,
    ) -> PackageDefinedObjectCreateV0 {
        PackageDefinedObjectCreateV0 {
            abi_major: PACKAGE_DEFINED_OBJECT_CREATE_ABI_MAJOR,
            abi_minor: PACKAGE_DEFINED_OBJECT_CREATE_ABI_MINOR,
            state_format,
            flags: 0,
            schema_object_id: schema_object_id.raw(),
            schema_revision,
            initial_state_ptr,
            initial_state_len,
            reserved0: 0,
            reserved1: 0,
            reserved2: 0,
        }
    }

    fn package_defined_create_request(
        authority: PackedCapability,
        create: &PackageDefinedObjectCreateV0,
    ) -> ObjectShellRequest {
        let mut request = object_request(OP_CREATE_OBJECT, authority);
        request.object_kind = OBJECT_KIND_PACKAGE_DEFINED_OBJECT;
        request.input_ptr = create as *const PackageDefinedObjectCreateV0 as u64;
        request.input_len = size_of::<PackageDefinedObjectCreateV0>() as u64;
        request
    }

    fn bind_package_defined_copy_map(
        caller: ActiveUserProcess,
        request: &ObjectShellRequest,
        response: &ObjectShellResponse,
        create: &PackageDefinedObjectCreateV0,
        state: Option<&[u8]>,
    ) {
        let mut copy_map = UserCopyMap::new();
        map_value(&mut copy_map, request, true, false);
        map_value(&mut copy_map, response, true, true);
        map_value(&mut copy_map, create, true, false);
        if let Some(state) = state {
            map_slice(&mut copy_map, state, true, false);
        }
        process_context::bind_current_process(caller.with_copy_map(copy_map));
    }

    fn retained_object_exists(object_id: ObjectId) -> bool {
        retained_services::with_object_service(|service| service.object_exists_for_test(object_id))
            .unwrap()
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

    const PACKAGE_CONTEXT_PACKAGE_ID: u64 = 0x5059_504B_474F_1305;
    const PACKAGE_CONTEXT_PACKAGE_REVISION: u64 = 9;
    const PACKAGE_CONTEXT_SCHEMA_ID: u64 = 0x5059_5343_484F_1305;
    const PACKAGE_CONTEXT_SCHEMA_REVISION: u64 = 4;
    const PACKAGE_CONTEXT_RELEASE_DIGEST: [u8; 32] = [
        0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE,
        0xAF, 0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD,
        0xBE, 0xBF,
    ];
    const PACKAGE_CONTEXT_SCHEMA_DESCRIPTOR_DIGEST: [u8; 32] = [
        0xD0, 0xD1, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xDB, 0xDC, 0xDD, 0xDE,
        0xDF, 0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xEB, 0xEC, 0xED,
        0xEE, 0xEF,
    ];

    fn package_context_service_with_launch_context(
        process: ActiveUserProcess,
    ) -> PackageService<'static> {
        let mut service = PackageService::new_empty_for_test();
        service
            .seed_launch_export_for_test(
                ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                b"seed",
                b"launch",
                PACKAGE_CONTEXT_PACKAGE_ID,
                PACKAGE_CONTEXT_PACKAGE_REVISION,
                PACKAGE_CONTEXT_RELEASE_DIGEST,
                PACKAGE_CONTEXT_SCHEMA_ID,
                PACKAGE_CONTEXT_SCHEMA_REVISION,
                PACKAGE_CONTEXT_SCHEMA_DESCRIPTOR_DIGEST,
            )
            .unwrap();
        let supplied_grants = [];
        let capabilities = CapabilityTable::new();
        service
            .launch(
                PackageLaunchRequest {
                    caller: process,
                    namespace_root: ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID),
                    locator: "seed/launch",
                    supplied_grants: &supplied_grants,
                },
                &capabilities,
            )
            .unwrap();
        service
    }

    const fn empty_package_schema_binding_output() -> PackageRuntimeSchemaBindingV0 {
        PackageRuntimeSchemaBindingV0 {
            abi_major: 0xFFFF,
            abi_minor: 0xFFFF,
            schema_slot: 0xFFFF,
            reserved0: 0xFFFF,
            package_object_id: u64::MAX,
            package_revision: u64::MAX,
            schema_object_id: u64::MAX,
            schema_revision: u64::MAX,
            schema_descriptor_sha256: [0xCC; 32],
            reserved1: [0xCC; 16],
        }
    }

    const fn package_binding_len() -> u64 {
        size_of::<PackageRuntimeSchemaBindingV0>() as u64
    }

    fn package_context_args(
        schema_slot: u16,
        output: &mut PackageRuntimeSchemaBindingV0,
        output_len: u64,
    ) -> SyscallArgs {
        SyscallArgs {
            number: SYSCALL_PACKAGE_CONTEXT,
            arg0: u64::from(OP_PACKAGE_CONTEXT_SCHEMA),
            arg1: u64::from(schema_slot),
            arg2: output as *mut PackageRuntimeSchemaBindingV0 as u64,
            arg3: output_len,
            arg4: 0,
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct PackageContextStateSnapshot {
        package_count: usize,
        schema_count: usize,
        runtime_context_count: usize,
        package_object_id: u64,
        package_revision: u64,
        schema_object_id: u64,
        schema_revision: u64,
        schema_descriptor_digest: [u8; 32],
    }

    fn package_context_state_snapshot() -> PackageContextStateSnapshot {
        crate::package_service::with_retained_package_service_for_phase13(|service| {
            let export = service
                .resolve_export(ObjectId::new(PACKAGE_LOCATOR_ROOT_OBJECT_ID), "seed/launch")
                .unwrap();
            PackageContextStateSnapshot {
                package_count: service.registry().package_count(),
                schema_count: service.registry().schema_count(),
                runtime_context_count: service.runtime_context_count_for_test(),
                package_object_id: export.package_object_id,
                package_revision: export.package_revision,
                schema_object_id: export.schema_object_id,
                schema_revision: export.schema_revision,
                schema_descriptor_digest: export.schema_descriptor_digest,
            }
        })
        .expect("service")
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

    fn package_context_process() -> ActiveUserProcess {
        ActiveUserProcess::new(
            ServiceId::from_raw(0x5059_504B_4354_5801),
            0x5059_504B_5254_5801,
            0x1305,
        )
    }

    fn package_context_intruder_process() -> ActiveUserProcess {
        ActiveUserProcess::new(
            ServiceId::from_raw(0x5059_504B_4354_58FF),
            0x5059_504B_5254_58FF,
            0xFFFF,
        )
    }

    fn package_context_syscall_test_lock() -> std::sync::MutexGuard<'static, ()> {
        process_context_test_lock()
    }

    fn process_context_test_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::process_context::PROCESS_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner())
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

    fn session_input_process() -> ActiveUserProcess {
        ActiveUserProcess::new(ServiceId::from_raw(0x5059_5345_5353_0001), 0x1350, 0x0001)
    }

    fn session_input_intruder_process() -> ActiveUserProcess {
        ActiveUserProcess::new(ServiceId::from_raw(0x5059_5345_5353_0002), 0x1350, 0x0002)
    }

    const fn session_input_event_len() -> u64 {
        size_of::<SessionInputEventV1>() as u64
    }

    const fn session_input_sentinel() -> SessionInputEventV1 {
        SessionInputEventV1 {
            sequence: 0xAAAA_AAAA_AAAA_AAAA,
            kind: 0xBBBB,
            source: 0xCCCC,
            flags: 0xDDDD_DDDD,
            value0: -11,
            value1: -22,
            reserved0: u64::MAX,
            reserved1: u64::MAX,
        }
    }

    const fn session_input_test_event() -> SessionInputEventV1 {
        SessionInputEventV1 {
            sequence: 42,
            kind: 3,
            source: 2,
            flags: 1,
            value0: -4,
            value1: 7,
            reserved0: 0,
            reserved1: 0,
        }
    }

    fn session_input_bytes(event: &SessionInputEventV1) -> [u8; 40] {
        let mut bytes = [0; 40];
        bytes[0..8].copy_from_slice(&event.sequence.to_ne_bytes());
        bytes[8..10].copy_from_slice(&event.kind.to_ne_bytes());
        bytes[10..12].copy_from_slice(&event.source.to_ne_bytes());
        bytes[12..16].copy_from_slice(&event.flags.to_ne_bytes());
        bytes[16..20].copy_from_slice(&event.value0.to_ne_bytes());
        bytes[20..24].copy_from_slice(&event.value1.to_ne_bytes());
        bytes[24..32].copy_from_slice(&event.reserved0.to_ne_bytes());
        bytes[32..40].copy_from_slice(&event.reserved1.to_ne_bytes());
        bytes
    }

    fn session_input_args(
        capability: PackedCapability,
        output: &mut SessionInputEventV1,
        output_len: u64,
    ) -> SyscallArgs {
        SyscallArgs {
            number: SYSCALL_SESSION_INPUT_TRY_READ,
            arg0: capability.raw(),
            arg1: output as *mut SessionInputEventV1 as u64,
            arg2: output_len,
            arg3: 0,
            arg4: 0,
        }
    }

    fn grant_session_input_for_test(
        capabilities: &mut CapabilityTable,
        process: ActiveUserProcess,
        resource: ResourceId,
        rights: RightsMask,
    ) -> PackedCapability {
        capabilities
            .grant(process.service_id(), resource, rights)
            .map(pack_syscall_capability)
            .unwrap()
    }
}
