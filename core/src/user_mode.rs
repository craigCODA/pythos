//! Minimal Phase 8 ring-3 execution proof.
//!
//! This is not the syscall ABI. It is a one-shot proof that PythCore can enter
//! CPL3 code, take a user-originated trap, and resume kernel execution.
#![cfg_attr(test, allow(dead_code, unused_imports))]
// The persistent ring-3 launch/fault items are only used by the normal-boot
// persistent-launch path (a `verify`-excluded set of modules), so they are
// legitimately unused under `--features verify`.
#![cfg_attr(feature = "verify", allow(dead_code))]

use crate::{
    architecture::x86_64::gdt, process_context, process_context::ActiveUserProcess, user_stacks,
};
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
const USER_CODE_FAULT_OFFSET: usize = 32;
const USER_CODE_BAD_POINTER_OFFSET: usize = 48;
const USER_CODE_BREAKPOINT: u8 = 0xCC;
const USER_CODE_HALT: u8 = 0xF4;
const USER_CODE_SYSCALL_PREFIX: u8 = 0x0F;
const USER_CODE_SYSCALL_OPCODE: u8 = 0x05;
const USER_CODE_UD2_PREFIX: u8 = 0x0F;
const USER_CODE_UD2_OPCODE: u8 = 0x0B;
const USER_CODE_MOV_RAX_IMM64: u8 = 0x48;
const USER_CODE_MOV_RAX_IMM64_OPCODE: u8 = 0xB8;
const USER_CODE_LOAD_AL_FROM_RAX: u8 = 0x8A;
const USER_CODE_LOAD_AL_FROM_RAX_MODRM: u8 = 0x00;
const USER_INVALID_OPCODE_VECTOR: u64 = 6;
const USER_GENERAL_PROTECTION_VECTOR: u64 = 13;
const USER_PAGE_FAULT_VECTOR: u64 = 14;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserModeError {
    DidNotReturn,
}

#[repr(align(4096))]
struct UserCodePage([u8; USER_PAGE_SIZE]);

#[repr(align(16))]
struct KernelTrapStack(UnsafeCell<[u8; KERNEL_TRAP_STACK_SIZE]>);

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
static KERNEL_TRAP_STACK: KernelTrapStack =
    KernelTrapStack(UnsafeCell::new([0; KERNEL_TRAP_STACK_SIZE]));

static EXPECTED_USER_BREAKPOINT: AtomicBool = AtomicBool::new(false);
static EXPECTED_USER_FAULT_VECTOR: AtomicU64 = AtomicU64::new(0);
static USER_RETURNED: AtomicBool = AtomicBool::new(false);
static KERNEL_RECOVERY_RIP: AtomicU64 = AtomicU64::new(0);
static KERNEL_RECOVERY_RSP: AtomicU64 = AtomicU64::new(0);
static PERSISTENT_USER_PROCESS_ACTIVE: AtomicBool = AtomicBool::new(false);
static PERSISTENT_USER_PROCESS_KIND: AtomicU64 = AtomicU64::new(PERSISTENT_KIND_NONE);

const PERSISTENT_KIND_NONE: u64 = 0;
const PERSISTENT_KIND_OBJECT_SHELL: u64 = 1;
const PERSISTENT_KIND_PYTH_GRAPH_RUNTIME: u64 = 2;
const PERSISTENT_KIND_PYTH_NATIVE_GRAPH: u64 = 3;

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

    bytes[USER_CODE_FAULT_OFFSET] = USER_CODE_UD2_PREFIX;
    bytes[USER_CODE_FAULT_OFFSET + 1] = USER_CODE_UD2_OPCODE;
    bytes[USER_CODE_FAULT_OFFSET + 2] = USER_CODE_HALT;

    bytes[USER_CODE_BAD_POINTER_OFFSET] = USER_CODE_MOV_RAX_IMM64;
    bytes[USER_CODE_BAD_POINTER_OFFSET + 1] = USER_CODE_MOV_RAX_IMM64_OPCODE;
    bytes[USER_CODE_BAD_POINTER_OFFSET + 10] = USER_CODE_LOAD_AL_FROM_RAX;
    bytes[USER_CODE_BAD_POINTER_OFFSET + 11] = USER_CODE_LOAD_AL_FROM_RAX_MODRM;
    bytes[USER_CODE_BAD_POINTER_OFFSET + 12] = USER_CODE_HALT;
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

    .global ring3_enter_forever_abi
    ring3_enter_forever_abi:
        mov rbx, rdi
        mov rdi, rdx
        mov ax, 0x2B
        mov ds, ax
        mov es, ax
        push 0x2B
        push rsi
        pushfq
        pop rax
        or rax, 0x200
        push rax
        push 0x33
        push rbx
        iretq
    "#
);

#[cfg(not(test))]
unsafe extern "C" {
    fn ring3_enter_abi(entry: u64, user_stack_top: u64) -> u64;
    fn ring3_enter_forever_abi(entry: u64, user_stack_top: u64, bootstrap_user_ptr: u64) -> !;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentUserProcessKind {
    ObjectShell,
    PythGraphRuntime,
    PythNativeGraph,
}

impl PersistentUserProcessKind {
    const fn raw(self) -> u64 {
        match self {
            Self::ObjectShell => PERSISTENT_KIND_OBJECT_SHELL,
            Self::PythGraphRuntime => PERSISTENT_KIND_PYTH_GRAPH_RUNTIME,
            Self::PythNativeGraph => PERSISTENT_KIND_PYTH_NATIVE_GRAPH,
        }
    }

    const fn from_raw(raw: u64) -> Option<Self> {
        match raw {
            PERSISTENT_KIND_OBJECT_SHELL => Some(Self::ObjectShell),
            PERSISTENT_KIND_PYTH_GRAPH_RUNTIME => Some(Self::PythGraphRuntime),
            PERSISTENT_KIND_PYTH_NATIVE_GRAPH => Some(Self::PythNativeGraph),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistentUserTerminationOutcome {
    pub terminated_principal: Option<u64>,
    pub process_kind: Option<PersistentUserProcessKind>,
    pub was_active: bool,
    pub safe_idle_ready: bool,
}

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn prepare_ring3_return_abi(recovery_rip: u64, kernel_rsp: u64) {
    USER_RETURNED.store(false, Ordering::SeqCst);
    KERNEL_RECOVERY_RIP.store(recovery_rip, Ordering::SeqCst);
    KERNEL_RECOVERY_RSP.store(kernel_rsp, Ordering::SeqCst);
}

#[cfg(not(test))]
pub fn run_self_test() -> Result<(), UserModeError> {
    run_user_entry(user_code_start(), ExpectedUserTrap::Breakpoint)
}

#[cfg(not(test))]
pub fn run_syscall_test() -> Result<(), UserModeError> {
    run_user_entry(user_syscall_entry(), ExpectedUserTrap::Breakpoint)
}

#[cfg(not(test))]
pub fn run_illegal_instruction_fault_test() -> Result<(), UserModeError> {
    run_user_entry(
        user_fault_entry(),
        ExpectedUserTrap::Fault(USER_INVALID_OPCODE_VECTOR),
    )
}

#[cfg(not(test))]
pub fn run_bad_pointer_fault_test() -> Result<(), UserModeError> {
    run_user_entry(
        user_bad_pointer_entry(),
        ExpectedUserTrap::Fault(USER_PAGE_FAULT_VECTOR),
    )
}

#[cfg(not(test))]
pub fn run_dynamic_breakpoint_test(entry: u64) -> Result<(), UserModeError> {
    run_user_entry(entry, ExpectedUserTrap::Breakpoint)
}

#[cfg(not(test))]
pub fn run_dynamic_illegal_instruction_fault_test(entry: u64) -> Result<(), UserModeError> {
    run_user_entry(entry, ExpectedUserTrap::Fault(USER_INVALID_OPCODE_VECTOR))
}

#[cfg(not(test))]
pub fn run_dynamic_bad_pointer_fault_test(entry: u64) -> Result<(), UserModeError> {
    run_user_entry(entry, ExpectedUserTrap::Fault(USER_PAGE_FAULT_VECTOR))
}

#[cfg(not(test))]
pub fn run_dynamic_hardware_fault_test(entry: u64) -> Result<(), UserModeError> {
    run_user_entry(
        entry,
        ExpectedUserTrap::Fault(USER_GENERAL_PROTECTION_VECTOR),
    )
}

#[cfg(not(test))]
pub fn enter_persistent_user_process(
    process: ActiveUserProcess,
    entry: u64,
    user_stack_top: u64,
    bootstrap_user_ptr: u64,
) -> ! {
    process_context::bind_current_process(process);
    tss::set_ring0_stack(kernel_trap_stack_top());
    serial::write_line("PYTHOS:SHELL:RING3_ENTER");
    PERSISTENT_USER_PROCESS_ACTIVE.store(true, Ordering::SeqCst);
    PERSISTENT_USER_PROCESS_KIND.store(
        PersistentUserProcessKind::ObjectShell.raw(),
        Ordering::SeqCst,
    );
    // SAFETY:
    // 1. Invariant: `entry` is the validated `shell.elf` entry point in a
    //    retained user address space, `user_stack_top` names a user-writable
    //    stack page, and `bootstrap_user_ptr` names a read-only user mapping.
    // 2. Established by: normal boot validates the ELF image, builds the shell
    //    user root with ELF/stack/bootstrap mappings, and binds its copy map.
    // 3. Lifetime: the shell root, user stack, and bootstrap page are retained
    //    for the full persistent shell run.
    // 4. Pointer ownership: the CPU consumes user RIP/RSP and passes the
    //    bootstrap pointer by value in `rdi`; Rust retains no references.
    // 5. Alignment: entry and stack are canonical user addresses, and the stack
    //    top is 16-byte aligned by the guarded-stack allocator.
    // 6. Mapped length: the root maps every validated ELF page, one stack page,
    //    one bootstrap block page, kernel trap stack, and kernel fault path.
    // 7. Concurrency: ADR 0051 runs one persistent user process on one CPU.
    // 8. Violation: bad descriptors or mappings fault through user containment.
    unsafe {
        ring3_enter_forever_abi(entry, user_stack_top, bootstrap_user_ptr);
    }
}

#[cfg(not(test))]
pub fn enter_pyth_graph_runtime(
    process: ActiveUserProcess,
    entry: u64,
    user_stack_top: u64,
    bootstrap_user_ptr: u64,
    package_digest: u64,
) -> ! {
    process_context::bind_current_process(process);
    tss::set_ring0_stack(kernel_trap_stack_top());
    serial::write_str("PYTHOS:PYTHTIG:RUNTIME_ENTER package:");
    serial::write_hex_u64_value(package_digest);
    serial::write_str("\r\n");
    PERSISTENT_USER_PROCESS_ACTIVE.store(true, Ordering::SeqCst);
    PERSISTENT_USER_PROCESS_KIND.store(
        PersistentUserProcessKind::PythGraphRuntime.raw(),
        Ordering::SeqCst,
    );
    // SAFETY:
    // 1. Invariant: `entry` is the validated `pyth-runtime.elf` entry point,
    //    `user_stack_top` is a guarded user stack, and `bootstrap_user_ptr`
    //    names the read-only Pyth graph bootstrap block.
    // 2. Established by: PythTIG normal init validates the runtime ELF,
    //    package, payload mappings, and retained user root before activation.
    // 3. Lifetime: the graph runtime root and payload pages are retained for
    //    the one-shot runtime invocation.
    // 4. Pointer ownership: the CPU consumes user RIP/RSP/RDI by value.
    // 5. Alignment: entry and stack are canonical user addresses; bootstrap is
    //    page-aligned and satisfies the block's ABI alignment.
    // 6. Mapped length: the root maps the runtime ELF, one stack, bootstrap,
    //    package, result page, kernel trap stack, and syscall path.
    // 7. Concurrency: Phase 2 launches one graph runtime process on one CPU.
    // 8. Violation: bad descriptors or mappings fault through containment.
    unsafe {
        ring3_enter_forever_abi(entry, user_stack_top, bootstrap_user_ptr);
    }
}

#[cfg(not(test))]
pub fn enter_pyth_native_graph(
    process: ActiveUserProcess,
    entry: u64,
    user_stack_top: u64,
    bootstrap_user_ptr: u64,
    package_digest: u64,
) -> ! {
    process_context::bind_current_process(process);
    tss::set_ring0_stack(kernel_trap_stack_top());
    serial::write_str("PYTHOS:PYTHTIG:NATIVE_ENTER package:");
    serial::write_hex_u64_value(package_digest);
    serial::write_str("\r\n");
    PERSISTENT_USER_PROCESS_ACTIVE.store(true, Ordering::SeqCst);
    PERSISTENT_USER_PROCESS_KIND.store(
        PersistentUserProcessKind::PythNativeGraph.raw(),
        Ordering::SeqCst,
    );
    // SAFETY:
    // 1. Invariant: `entry` is the validated native PythTIG ELF entry point,
    //    `user_stack_top` is a guarded user stack, and `bootstrap_user_ptr`
    //    names the read-only Pyth graph bootstrap block.
    // 2. Established by: PythTIG native normal init validates the native ELF,
    //    graph package, native binding record, payload mappings, and retained
    //    user root before activation.
    // 3. Lifetime: the native graph root and payload pages are retained for
    //    the one-shot invocation.
    // 4. Pointer ownership: the CPU consumes user RIP/RSP/RDI by value.
    // 5. Alignment: entry and stack are canonical user addresses; bootstrap is
    //    page-aligned and satisfies the block's ABI alignment.
    // 6. Mapped length: the root maps the native ELF, one stack, bootstrap,
    //    package, result page, kernel trap stack, and syscall path.
    // 7. Concurrency: Phase 6 launches one native graph process on one CPU.
    // 8. Violation: bad descriptors or mappings fault through containment.
    unsafe {
        ring3_enter_forever_abi(entry, user_stack_top, bootstrap_user_ptr);
    }
}

pub fn is_active_pyth_graph_process(expected_principal: u64) -> bool {
    PERSISTENT_USER_PROCESS_ACTIVE.load(Ordering::SeqCst)
        && matches!(
            PersistentUserProcessKind::from_raw(
                PERSISTENT_USER_PROCESS_KIND.load(Ordering::SeqCst)
            ),
            Some(
                PersistentUserProcessKind::PythGraphRuntime
                    | PersistentUserProcessKind::PythNativeGraph
            )
        )
        && process_context::current_caller()
            .map(|process| process.principal_id() == expected_principal)
            .unwrap_or(false)
}

pub fn is_active_pyth_native_graph() -> bool {
    PERSISTENT_USER_PROCESS_ACTIVE.load(Ordering::SeqCst)
        && PersistentUserProcessKind::from_raw(PERSISTENT_USER_PROCESS_KIND.load(Ordering::SeqCst))
            == Some(PersistentUserProcessKind::PythNativeGraph)
}

#[cfg(not(test))]
enum ExpectedUserTrap {
    Breakpoint,
    Fault(u64),
}

#[cfg(not(test))]
fn run_user_entry(entry: u64, trap: ExpectedUserTrap) -> Result<(), UserModeError> {
    match trap {
        ExpectedUserTrap::Breakpoint => {
            EXPECTED_USER_BREAKPOINT.store(true, Ordering::SeqCst);
            EXPECTED_USER_FAULT_VECTOR.store(0, Ordering::SeqCst);
        }
        ExpectedUserTrap::Fault(vector) => {
            EXPECTED_USER_BREAKPOINT.store(false, Ordering::SeqCst);
            EXPECTED_USER_FAULT_VECTOR.store(vector, Ordering::SeqCst);
        }
    }
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
    // 5. Alignment: user entry is a canonical executable user address; stacks
    //    are 16-byte aligned.
    // 6. Mapped length: the active user code page or dynamic ELF page, one
    //    user stack page, and a 16 KiB kernel trap stack are mapped.
    // 7. Concurrency: single-core boot, one ring-3 proof in flight.
    // 8. Violation: bad descriptors or mappings fault through diagnostics.
    let returned = unsafe { ring3_enter_abi(entry, user_stack_top()) };
    if returned == 1 && USER_RETURNED.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err(UserModeError::DidNotReturn)
    }
}

pub fn handle_user_fault(
    vector: u64,
    cs: u64,
    ss: u64,
    rip: u64,
    rsp: u64,
    fault_address: u64,
) -> bool {
    let expected_vector = EXPECTED_USER_FAULT_VECTOR.load(Ordering::SeqCst);
    if expected_vector != 0 && expected_vector == vector && is_user_frame(cs, ss) {
        EXPECTED_USER_FAULT_VECTOR.store(0, Ordering::SeqCst);
        USER_RETURNED.store(true, Ordering::SeqCst);
        #[cfg(not(test))]
        {
            serial::write_line("PYTHOS:CORE:CRASH:USER_FAULT");
            let recovery_rip = KERNEL_RECOVERY_RIP.load(Ordering::SeqCst);
            let recovery_rsp = KERNEL_RECOVERY_RSP.load(Ordering::SeqCst);
            jump_to_kernel_recovery(recovery_rip, recovery_rsp);
        }
        #[cfg(test)]
        {
            return true;
        }
    }
    if PERSISTENT_USER_PROCESS_ACTIVE.load(Ordering::SeqCst) && is_user_frame(cs, ss) {
        return handle_persistent_user_fault(vector, rip, rsp, fault_address);
    }
    false
}

fn transition_persistent_user_process_to_safe_idle() -> PersistentUserTerminationOutcome {
    let was_active = PERSISTENT_USER_PROCESS_ACTIVE.swap(false, Ordering::SeqCst);
    let process_kind = PersistentUserProcessKind::from_raw(
        PERSISTENT_USER_PROCESS_KIND.swap(PERSISTENT_KIND_NONE, Ordering::SeqCst),
    );
    let terminated_principal = process_context::current_caller()
        .ok()
        .map(|process| process.principal_id());
    process_context::clear_current_process();
    let safe_idle_ready = !PERSISTENT_USER_PROCESS_ACTIVE.load(Ordering::SeqCst)
        && PERSISTENT_USER_PROCESS_KIND.load(Ordering::SeqCst) == PERSISTENT_KIND_NONE
        && process_context::current_caller().is_err();
    PersistentUserTerminationOutcome {
        terminated_principal,
        process_kind,
        was_active,
        safe_idle_ready,
    }
}

pub fn transition_pyth_graph_runtime_exit(expected_principal: u64) -> bool {
    let outcome = transition_persistent_user_process_to_safe_idle();
    outcome.was_active
        && outcome.safe_idle_ready
        && matches!(
            outcome.process_kind,
            Some(
                PersistentUserProcessKind::PythGraphRuntime
                    | PersistentUserProcessKind::PythNativeGraph
            )
        )
        && outcome.terminated_principal == Some(expected_principal)
}

#[cfg(not(test))]
pub fn complete_pyth_graph_runtime_exit(expected_principal: u64) -> ! {
    if transition_pyth_graph_runtime_exit(expected_principal) {
        serial::write_str("PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:");
        serial::write_hex_u64_value(expected_principal);
        serial::write_str("\r\n");
    } else {
        serial::write_line("PYTHOS:PYTHTIG:RUNTIME_TERMINATION_FAILED");
    }
    persistent_fault_idle();
}

#[cfg(test)]
pub(crate) fn activate_persistent_user_process_for_test(
    process: ActiveUserProcess,
    process_kind: PersistentUserProcessKind,
) {
    process_context::bind_current_process(process);
    PERSISTENT_USER_PROCESS_KIND.store(process_kind.raw(), Ordering::SeqCst);
    PERSISTENT_USER_PROCESS_ACTIVE.store(true, Ordering::SeqCst);
}

fn handle_persistent_user_fault(_vector: u64, _rip: u64, _rsp: u64, _fault_address: u64) -> bool {
    let _outcome = transition_persistent_user_process_to_safe_idle();
    #[cfg(not(test))]
    {
        let principal = _outcome.terminated_principal.unwrap_or(0);
        serial::write_line("PYTHOS:CORE:CRASH:USER_FAULT");
        match _outcome.process_kind {
            Some(PersistentUserProcessKind::ObjectShell) => {
                serial::write_line("PYTHOS:SHELL:FAULT_TERMINATED");
            }
            Some(PersistentUserProcessKind::PythGraphRuntime) => {
                serial::write_str("PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED principal:");
                serial::write_hex_u64_value(principal);
                serial::write_str(" vector:");
                serial::write_dec_u64_value(_vector);
                serial::write_str(" rip:");
                serial::write_hex_u64_value(_rip);
                serial::write_str(" rsp:");
                serial::write_hex_u64_value(_rsp);
                serial::write_str(" cr2:");
                serial::write_hex_u64_value(_fault_address);
                serial::write_str("\r\n");
                if _outcome.was_active && _outcome.safe_idle_ready {
                    serial::write_line("PYTHOS:PYTHTIG:RUNTIME_FAULT_SAFE_IDLE");
                }
            }
            Some(PersistentUserProcessKind::PythNativeGraph) => {
                serial::write_str("PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED principal:");
                serial::write_hex_u64_value(principal);
                serial::write_str(" vector:");
                serial::write_dec_u64_value(_vector);
                serial::write_str(" rip:");
                serial::write_hex_u64_value(_rip);
                serial::write_str(" rsp:");
                serial::write_hex_u64_value(_rsp);
                serial::write_str(" cr2:");
                serial::write_hex_u64_value(_fault_address);
                serial::write_str("\r\n");
                if _outcome.was_active && _outcome.safe_idle_ready {
                    serial::write_line("PYTHOS:PYTHTIG:RUNTIME_FAULT_SAFE_IDLE");
                }
            }
            None => {}
        }
        persistent_fault_idle();
    }
    #[cfg(test)]
    {
        true
    }
}

#[cfg(not(test))]
fn persistent_fault_idle() -> ! {
    loop {
        core::hint::spin_loop();
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
    user_stacks::proof_stack_region()
}

pub fn user_code_start() -> u64 {
    USER_CODE_PAGE.0.as_ptr() as u64
}

fn user_syscall_entry() -> u64 {
    user_code_start() + USER_CODE_SYSCALL_OFFSET as u64
}

fn user_fault_entry() -> u64 {
    user_code_start() + USER_CODE_FAULT_OFFSET as u64
}

#[cfg(not(test))]
fn user_bad_pointer_entry() -> u64 {
    user_code_start() + USER_CODE_BAD_POINTER_OFFSET as u64
}

#[cfg(not(test))]
fn user_stack_top() -> u64 {
    user_stacks::proof_stack_top()
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
    fn user_code_page_contains_illegal_instruction_probe_entry() {
        assert_eq!(
            USER_CODE_PAGE.0[USER_CODE_FAULT_OFFSET],
            USER_CODE_UD2_PREFIX
        );
        assert_eq!(
            USER_CODE_PAGE.0[USER_CODE_FAULT_OFFSET + 1],
            USER_CODE_UD2_OPCODE
        );
    }

    #[test]
    fn user_code_page_contains_bad_pointer_probe_entry() {
        assert_eq!(
            USER_CODE_PAGE.0[USER_CODE_BAD_POINTER_OFFSET],
            USER_CODE_MOV_RAX_IMM64
        );
        assert_eq!(
            USER_CODE_PAGE.0[USER_CODE_BAD_POINTER_OFFSET + 10],
            USER_CODE_LOAD_AL_FROM_RAX
        );
    }

    #[test]
    fn user_regions_are_page_sized_and_aligned() {
        let (code_start, code_len) = user_code_region();
        let (stack_start, stack_len) = user_stack_region();

        assert_eq!(code_len, USER_PAGE_SIZE as u64);
        assert_eq!(
            stack_len,
            crate::user_stacks::USER_STACK_USABLE_BYTES as u64
        );
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

    #[test]
    fn persistent_user_fault_transitions_to_safe_idle() {
        let _process_guard = crate::process_context::PROCESS_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut identities = crate::service_identity::ServiceIdentityTable::new();
        let shell_service = identities
            .register_task(crate::tasks::TaskId::new(180))
            .unwrap();
        let shell = crate::process_context::ActiveUserProcess::new(
            shell_service,
            pythos_shared::user_program_manifest::SHELL_PRINCIPAL_ID,
            0xAA,
        );
        activate_persistent_user_process_for_test(shell, PersistentUserProcessKind::ObjectShell);
        let outcome = transition_persistent_user_process_to_safe_idle();

        assert_eq!(
            outcome.terminated_principal,
            Some(pythos_shared::user_program_manifest::SHELL_PRINCIPAL_ID)
        );
        assert!(outcome.was_active);
        assert!(outcome.safe_idle_ready);
        assert_eq!(
            outcome.process_kind,
            Some(PersistentUserProcessKind::ObjectShell)
        );
    }

    #[test]
    fn persistent_pyth_graph_fault_transitions_to_safe_idle() {
        let _process_guard = crate::process_context::PROCESS_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut identities = crate::service_identity::ServiceIdentityTable::new();
        let graph_service = identities
            .register_task(crate::tasks::TaskId::new(190))
            .unwrap();
        let graph = crate::process_context::ActiveUserProcess::new(
            graph_service,
            crate::pyth_runtime_launch::PYTH_RUNTIME_PRINCIPAL_ID,
            0xDD,
        );
        activate_persistent_user_process_for_test(
            graph,
            PersistentUserProcessKind::PythGraphRuntime,
        );
        let outcome = transition_persistent_user_process_to_safe_idle();

        assert_eq!(
            outcome.terminated_principal,
            Some(crate::pyth_runtime_launch::PYTH_RUNTIME_PRINCIPAL_ID)
        );
        assert!(outcome.was_active);
        assert!(outcome.safe_idle_ready);
        assert_eq!(
            outcome.process_kind,
            Some(PersistentUserProcessKind::PythGraphRuntime)
        );
    }
}
