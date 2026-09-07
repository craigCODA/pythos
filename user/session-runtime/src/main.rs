#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
mod syscalls;

#[cfg(not(test))]
use core::{cell::UnsafeCell, panic::PanicInfo, ptr};
#[cfg(not(test))]
use pythos_shared::{
    object_shell_abi::PackedCapability,
    pyth_command_abi::{COMMAND_RESULT_STATUS_OK, PythCommand, PythCommandResult},
    pyth_runtime_abi::{
        GRAPH_EXIT_OK, GRAPH_RESULT_UNIT, GraphExitRecord, HostCallResult, MAX_PYTH_GRAPH_IMPORTS,
        PythGraphBootstrapBlock,
    },
    pyth_tig::{format::MAX_RUNTIME_VALUES, verify::VerifiedGraph},
    session_input_abi::{
        SESSION_INPUT_RESULT_EMPTY, SESSION_INPUT_RESULT_EVENT, SessionInputEventV1,
    },
    session_runtime_abi::{
        SESSION_RUNTIME_COMMAND_COUNT, SESSION_RUNTIME_EMPTY_POLL_LIMIT,
        SESSION_RUNTIME_LIFECYCLE_REINVOKE, SESSION_RUNTIME_LIFECYCLE_REQUEST_RECOVERY,
        SESSION_RUNTIME_RESULT_COMPLETE, SESSION_RUNTIME_RESULT_MAGIC,
        SESSION_RUNTIME_RESULT_REQUEST_RECOVERY, SessionRuntimeBootstrapV1,
        SessionRuntimeFixtureV1, SessionRuntimeResultV1, validate_session_runtime_result,
    },
};
#[cfg(not(test))]
use pythos_user_pyth_runtime::{interpreter::Interpreter, value::Value};
#[cfg(not(test))]
use pythos_user_session_runtime::{
    InputSequenceValidator, SessionGraphLifecycleAction, SessionRuntimeEffectError,
    SessionRuntimeEffects, SessionRuntimeState, SessionRuntimeTerminalResult,
    run_session_runtime_orchestration, session_command_host::SessionCommandHost,
    validate_session_runtime_package,
};

#[cfg(not(test))]
struct RuntimeOwnedStorage {
    bootstrap: SessionRuntimeBootstrapV1,
    fixture: SessionRuntimeFixtureV1,
    graph: PythGraphBootstrapBlock,
    imports: [PackedCapability; MAX_PYTH_GRAPH_IMPORTS],
    values: [Option<Value>; MAX_RUNTIME_VALUES],
    host_results: [Option<HostCallResult>; MAX_RUNTIME_VALUES],
}

#[cfg(not(test))]
struct RuntimeStorage(UnsafeCell<RuntimeOwnedStorage>);

#[cfg(not(test))]
#[repr(align(8))]
struct InputOutputSlot(UnsafeCell<SessionInputEventV1>);

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: one retained ring-3 runtime thread owns this storage for one finite execution.
// 2. Established by: the session-runtime ELF has no thread, callback, or interrupt-entry API.
// 3. Lifetime: the static storage outlives the complete retained runtime execution.
// 4. Pointer ownership: `_start` obtains the sole mutable reference and never publishes it.
// 5. Alignment: `UnsafeCell` preserves every contained ABI record and array alignment.
// 6. Mapped length: accesses stay within exactly one `RuntimeOwnedStorage` value.
// 7. Concurrency: no second ring-3 execution can race this process-local static.
// 8. Violation: aliasing or concurrent access could corrupt authenticated launch or invocation state.
unsafe impl Sync for RuntimeStorage {}

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: only the one finite ring-3 runtime execution accesses this input output slot.
// 2. Established by: this user ELF has no threads, callbacks, or interrupt handler.
// 3. Lifetime: the static belongs to the complete retained runtime execution.
// 4. Pointer ownership: polling owns the sole mutable reference for each synchronous syscall.
// 5. Alignment: `repr(align(8))` satisfies `SessionInputEventV1` alignment.
// 6. Mapped length: exactly one writable 40-byte BSS record is exposed to PythCore.
// 7. Concurrency: input reads are strictly sequential in `_start`.
// 8. Violation: concurrent access could race PythCore's synchronous copy-out.
unsafe impl Sync for InputOutputSlot {}

#[cfg(not(test))]
static RUNTIME_STORAGE: RuntimeStorage = RuntimeStorage(UnsafeCell::new(RuntimeOwnedStorage {
    bootstrap: SessionRuntimeBootstrapV1::empty(),
    fixture: SessionRuntimeFixtureV1::empty(),
    graph: SessionRuntimeBootstrapV1::empty().graph,
    imports: [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS],
    values: [None; MAX_RUNTIME_VALUES],
    host_results: [None; MAX_RUNTIME_VALUES],
}));

#[cfg(not(test))]
static INPUT_OUTPUT: InputOutputSlot =
    InputOutputSlot(UnsafeCell::new(SessionInputEventV1::empty()));

#[cfg(not(test))]
struct ExecutionEvidence {
    state: SessionRuntimeState,
    retained_state_before_second: u64,
    retained_state_final: u64,
    command_results: [PythCommandResult; SESSION_RUNTIME_COMMAND_COUNT],
    graph_exits: [GraphExitRecord; SESSION_RUNTIME_COMMAND_COUNT],
}

#[cfg(not(test))]
impl ExecutionEvidence {
    const fn new(session_service_id: u64) -> Self {
        Self {
            state: SessionRuntimeState::new(session_service_id),
            retained_state_before_second: 0,
            retained_state_final: 0,
            command_results: [PythCommandResult::empty(0, 0); SESSION_RUNTIME_COMMAND_COUNT],
            graph_exits: [empty_graph_exit(); SESSION_RUNTIME_COMMAND_COUNT],
        }
    }
}

#[cfg(not(test))]
struct RuntimeEffects {
    bootstrap_ptr: *const SessionRuntimeBootstrapV1,
    storage: &'static mut RuntimeOwnedStorage,
    evidence: ExecutionEvidence,
    input_validator: InputSequenceValidator,
    verified: Option<VerifiedGraph<'static>>,
}

#[cfg(not(test))]
impl RuntimeEffects {
    fn new(bootstrap_ptr: *const SessionRuntimeBootstrapV1) -> Self {
        Self {
            bootstrap_ptr,
            storage: runtime_storage(),
            evidence: ExecutionEvidence::new(0),
            input_validator: InputSequenceValidator::new(),
            verified: None,
        }
    }
}

/// Entry for the bounded retained ring-3 session runtime.
///
/// # Safety
///
/// PythCore must map the exact authenticated bootstrap address read-only before
/// entry and retain the matching package, fixture, result, and stack mappings
/// until the expected breakpoint returns control to the kernel.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(bootstrap_ptr: *const SessionRuntimeBootstrapV1) -> ! {
    let mut effects = RuntimeEffects::new(bootstrap_ptr);
    run_session_runtime_orchestration(bootstrap_ptr as u64, &mut effects);
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(test))]
impl SessionRuntimeEffects for RuntimeEffects {
    fn copy_bootstrap(&mut self) -> SessionRuntimeBootstrapV1 {
        copy_bootstrap(self.bootstrap_ptr, &mut self.storage.bootstrap);
        self.evidence = ExecutionEvidence::new(self.storage.bootstrap.session_service_id);
        self.storage.bootstrap
    }

    fn copy_fixture(&mut self) -> SessionRuntimeFixtureV1 {
        copy_fixture(
            self.storage.bootstrap.fixture_ptr as *const SessionRuntimeFixtureV1,
            &mut self.storage.fixture,
        );
        self.storage.fixture
    }

    fn prepare_graph_package(
        &mut self,
        bootstrap: &SessionRuntimeBootstrapV1,
    ) -> Result<(), SessionRuntimeEffectError> {
        self.storage.graph = bootstrap.graph;
        self.storage.imports.fill(PackedCapability::from_raw(0));
        let import = self.storage.graph.imports[0];
        self.storage.imports[usize::from(import.import_slot)] = import.capability;

        let package_bytes = package_bytes(&self.storage.graph);
        let package = validate_session_runtime_package(bootstrap, package_bytes)
            .map_err(|_| SessionRuntimeEffectError::Package)?;
        // SAFETY:
        // 1. Invariant: this exact package was authenticated and verified by PythCore before entry.
        // 2. Established by: the coordinator accepted the exact launch boundary and nested graph
        //    metadata, while package length, digest, and decode all succeeded immediately above.
        // 3. Lifetime: PythCore retains the read-only package page until this runtime traps once.
        // 4. Pointer ownership: PythCore owns the mapping; this runtime holds only read-only slices.
        // 5. Alignment: PythTIG decoding reads byte records and materializes aligned values by value.
        // 6. Mapped length: the package is nonempty, one-page bounded, and all sections decoded.
        // 7. Concurrency: no writer is mapped into this single-threaded runtime process.
        // 8. Violation: bypassing this authenticated boundary would skip graph admission verification.
        self.verified = Some(unsafe { VerifiedGraph::assume_kernel_verified_package(&package) });
        Ok(())
    }

    fn poll_input(&mut self, ordinal: usize) -> Result<(), SessionRuntimeEffectError> {
        poll_input_event(
            self.storage.bootstrap.input_capability,
            &mut self.input_validator,
            ordinal + 1,
        )
        .map_err(|_| SessionRuntimeEffectError::Poll)?;
        self.evidence.state.record_input_event();
        Ok(())
    }

    fn run_command_host_and_graph(
        &mut self,
        ordinal: usize,
    ) -> Result<(), SessionRuntimeEffectError> {
        let command = self.storage.fixture.commands[ordinal];
        let payload = fixture_payload(&self.storage.fixture, ordinal);
        let import = self.storage.graph.imports[0];
        let mut command_host = SessionCommandHost::new(import.capability, &command, payload)
            .map_err(|_| SessionRuntimeEffectError::CommandHost)?;
        let verified = self.verified.ok_or(SessionRuntimeEffectError::Graph)?;
        let exit = Interpreter::new(
            verified,
            &self.storage.imports,
            self.storage.graph.instruction_budget,
            &mut self.storage.values,
            &mut self.storage.host_results,
        )
        .execute(&mut command_host);
        let result = command_host.result();

        self.evidence.graph_exits[ordinal] = exit;
        if self.evidence.state.record_graph_exit(exit.status)
            != SessionGraphLifecycleAction::Reinvoke
        {
            return Err(SessionRuntimeEffectError::Graph);
        }
        let result = result.ok_or(SessionRuntimeEffectError::Result)?;
        if !invocation_is_valid(&command, payload, result, exit) {
            return Err(SessionRuntimeEffectError::Result);
        }
        self.evidence.command_results[ordinal] = result;
        if ordinal == 1 {
            self.evidence.retained_state_final = self.evidence.state.input_event_count;
        }
        Ok(())
    }

    fn reset_invocation_local(&mut self) -> Result<(), SessionRuntimeEffectError> {
        self.storage.values.fill(Some(Value::U64(u64::MAX)));
        self.storage
            .host_results
            .fill(Some(HostCallResult::empty(u16::MAX)));
        {
            let _reset_proof = Interpreter::new(
                self.verified.ok_or(SessionRuntimeEffectError::Reset)?,
                &self.storage.imports,
                self.storage.graph.instruction_budget,
                &mut self.storage.values,
                &mut self.storage.host_results,
            );
        }
        if self.storage.values.iter().any(Option::is_some)
            || self.storage.host_results.iter().any(Option::is_some)
        {
            return Err(SessionRuntimeEffectError::Reset);
        }
        self.evidence.retained_state_before_second = self.evidence.state.input_event_count;
        if self.evidence.retained_state_before_second != 1 {
            return Err(SessionRuntimeEffectError::Reset);
        }
        Ok(())
    }

    fn final_state_is_valid(&self) -> bool {
        self.evidence.retained_state_final == 2
            && self.input_validator.is_complete()
            && self.evidence.state.session_service_id == self.storage.bootstrap.session_service_id
    }

    fn write_terminal_result(
        &mut self,
        terminal: SessionRuntimeTerminalResult,
    ) -> Result<(), SessionRuntimeEffectError> {
        let (terminal_status, lifecycle) = match terminal {
            SessionRuntimeTerminalResult::Complete => (
                SESSION_RUNTIME_RESULT_COMPLETE,
                SESSION_RUNTIME_LIFECYCLE_REINVOKE,
            ),
            SessionRuntimeTerminalResult::RequestRecovery => (
                SESSION_RUNTIME_RESULT_REQUEST_RECOVERY,
                SESSION_RUNTIME_LIFECYCLE_REQUEST_RECOVERY,
            ),
        };
        let result = terminal_result(
            &self.storage.bootstrap,
            &self.evidence,
            terminal_status,
            lifecycle,
        );
        if terminal == SessionRuntimeTerminalResult::Complete
            && validate_session_runtime_result(&self.storage.bootstrap, &result).is_err()
        {
            return Err(SessionRuntimeEffectError::Result);
        }
        write_result(self.storage.bootstrap.result_ptr, result);
        Ok(())
    }

    fn emit_marker(&mut self, marker: &'static str) {
        syscalls::write_str(self.storage.bootstrap.console_capability, marker);
    }

    fn trap(&mut self) {
        trap_and_spin()
    }
}

#[cfg(not(test))]
fn runtime_storage() -> &'static mut RuntimeOwnedStorage {
    // SAFETY:
    // 1. Invariant: `_start` calls this once and owns the storage until its terminal trap.
    // 2. Established by: one retained process has one entry and no thread-creation surface.
    // 3. Lifetime: the static outlives every interpreter invocation and terminal write.
    // 4. Pointer ownership: this is the sole mutable reference to `RUNTIME_STORAGE`.
    // 5. Alignment: `UnsafeCell` preserves `RuntimeOwnedStorage` alignment.
    // 6. Mapped length: exactly one complete `RuntimeOwnedStorage` value is returned.
    // 7. Concurrency: no callback, interrupt handler, or second runtime thread accesses it.
    // 8. Violation: another reference could alias bootstrap, fixture, or invocation tables.
    unsafe { &mut *RUNTIME_STORAGE.0.get() }
}

#[cfg(not(test))]
fn copy_bootstrap(
    source: *const SessionRuntimeBootstrapV1,
    destination: &mut SessionRuntimeBootstrapV1,
) {
    // SAFETY:
    // 1. Invariant: `source` is the exact aligned fixed bootstrap address checked before this call.
    // 2. Established by: `validate_session_runtime_bootstrap_address` accepted the entry value.
    // 3. Lifetime: PythCore retains the read-only page through this one copy.
    // 4. Pointer ownership: PythCore owns source; the runtime exclusively owns destination.
    // 5. Alignment: the fixed page address and destination both satisfy ABI alignment.
    // 6. Mapped length: the kernel contract maps a full page, larger than the 944-byte record.
    // 7. Concurrency: PythCore never mutates the authenticated page after ring-3 entry.
    // 8. Violation: an absent mapping faults the user process without authorizing pointer writes.
    unsafe { ptr::copy_nonoverlapping(source, destination, 1) };
}

#[cfg(not(test))]
fn copy_fixture(source: *const SessionRuntimeFixtureV1, destination: &mut SessionRuntimeFixtureV1) {
    // SAFETY:
    // 1. Invariant: outer validation accepted the exact fixed read-only fixture address and length.
    // 2. Established by: `validate_session_runtime_outer_bootstrap` completed before this call.
    // 3. Lifetime: PythCore retains the fixture mapping through this one copy.
    // 4. Pointer ownership: PythCore owns source; the runtime exclusively owns destination.
    // 5. Alignment: the fixed page address and destination satisfy the fixture's alignment.
    // 6. Mapped length: exactly one 224-byte fixture is copied from a full mapped page.
    // 7. Concurrency: no writer is mapped into this process for the fixture page.
    // 8. Violation: violating the kernel map contract faults before input or graph invocation.
    unsafe { ptr::copy_nonoverlapping(source, destination, 1) };
}

#[cfg(not(test))]
fn package_bytes(graph: &PythGraphBootstrapBlock) -> &'static [u8] {
    // SAFETY:
    // 1. Invariant: outer validation accepted the fixed read-only package address and one-page bound.
    // 2. Established by: `validate_session_runtime_outer_bootstrap` checked pointer and length.
    // 3. Lifetime: PythCore retains the immutable package page until the terminal trap.
    // 4. Pointer ownership: PythCore owns the bytes; the runtime only reads the returned slice.
    // 5. Alignment: a byte slice requires alignment one.
    // 6. Mapped length: `package_len` is nonzero and no greater than the mapped 4096-byte page.
    // 7. Concurrency: the page has no writable user alias and the runtime is single-threaded.
    // 8. Violation: a broken map contract faults this process and cannot produce readiness.
    unsafe {
        core::slice::from_raw_parts(graph.package_ptr as *const u8, graph.package_len as usize)
    }
}

#[cfg(not(test))]
fn fixture_payload(fixture: &SessionRuntimeFixtureV1, ordinal: usize) -> &[u8] {
    let len = fixture.commands[ordinal].payload_len as usize;
    &fixture.payloads[ordinal][..len]
}

#[cfg(not(test))]
fn poll_input_event(
    input: PackedCapability,
    validator: &mut InputSequenceValidator,
    expected_ordinal: usize,
) -> Result<(), ()> {
    let mut empty_polls = 0u64;
    loop {
        let result = try_read_input(input);
        match result {
            SESSION_INPUT_RESULT_EVENT => {
                return match validator.accept(read_input()) {
                    Ok(ordinal) if ordinal == expected_ordinal => Ok(()),
                    _ => Err(()),
                };
            }
            SESSION_INPUT_RESULT_EMPTY => {
                empty_polls += 1;
                if empty_polls >= SESSION_RUNTIME_EMPTY_POLL_LIMIT {
                    return Err(());
                }
                core::hint::spin_loop();
            }
            _ => return Err(()),
        }
    }
}

#[cfg(not(test))]
fn try_read_input(input: PackedCapability) -> u64 {
    // SAFETY:
    // 1. Invariant: `INPUT_OUTPUT` is the runtime's only session-input syscall output slot.
    // 2. Established by: no bootstrap, fixture, package, or result pointer is passed here.
    // 3. Lifetime: the static outlives this synchronous syscall and subsequent by-value read.
    // 4. Pointer ownership: this block creates the only mutable access until the syscall returns.
    // 5. Alignment: `InputOutputSlot` is explicitly aligned to eight bytes.
    // 6. Mapped length: exactly one full 40-byte `SessionInputEventV1` record is exposed.
    // 7. Concurrency: polling is sequential on the runtime's only thread.
    // 8. Violation: aliasing during copy-out could corrupt input validation and force recovery.
    unsafe {
        *INPUT_OUTPUT.0.get() = SessionInputEventV1::empty();
        syscalls::try_read(input, &mut *INPUT_OUTPUT.0.get())
    }
}

#[cfg(not(test))]
fn read_input() -> SessionInputEventV1 {
    // SAFETY:
    // 1. Invariant: this copies the output of the immediately completed synchronous input syscall.
    // 2. Established by: `poll_input_event` calls it only after `SESSION_INPUT_RESULT_EVENT`.
    // 3. Lifetime: the static outlives the returned by-value event.
    // 4. Pointer ownership: this creates one short read after mutable syscall access ended.
    // 5. Alignment: `InputOutputSlot` is explicitly aligned to eight bytes.
    // 6. Mapped length: exactly one initialized 40-byte event record is copied.
    // 7. Concurrency: no concurrent poll or writer exists.
    // 8. Violation: a racing writer could produce a torn event and force recovery.
    unsafe { *INPUT_OUTPUT.0.get() }
}

#[cfg(not(test))]
fn invocation_is_valid(
    command: &PythCommand,
    payload: &[u8],
    result: PythCommandResult,
    exit: GraphExitRecord,
) -> bool {
    exit.status == GRAPH_EXIT_OK
        && exit.error_code == 0
        && exit.result_type == GRAPH_RESULT_UNIT
        && exit.reserved0 == 0
        && exit.reserved1 == 0
        && exit.result_raw == 0
        && result.status == COMMAND_RESULT_STATUS_OK
        && result.kind == command.kind
        && result.reserved0 == 0
        && result.object_id == command.object_id
        && result.task_id == command.task_id
        && result.proposal_id == command.proposal_id
        && result.bytes_written == payload.len() as u64
        && result.reserved1 == 0
}

#[cfg(not(test))]
fn terminal_result(
    bootstrap: &SessionRuntimeBootstrapV1,
    evidence: &ExecutionEvidence,
    terminal_status: u16,
    lifecycle: u16,
) -> SessionRuntimeResultV1 {
    let mut result = SessionRuntimeResultV1::empty();
    result.magic = SESSION_RUNTIME_RESULT_MAGIC;
    result.abi_major = bootstrap.abi_major;
    result.abi_minor = bootstrap.abi_minor;
    result.terminal_status = terminal_status;
    result.last_lifecycle_action = lifecycle;
    result.session_service_id = bootstrap.session_service_id;
    result.runtime_principal_id = bootstrap.runtime_principal_id;
    result.graph_principal_id = bootstrap.graph_principal_id;
    result.input_event_count = evidence.state.input_event_count;
    result.invocation_count = evidence.state.graph_invocation_count;
    result.retained_state_before_second = evidence.retained_state_before_second;
    result.retained_state_final = evidence.retained_state_final;
    result.command_results = evidence.command_results;
    result.graph_exits = evidence.graph_exits;
    result
}

#[cfg(not(test))]
fn write_result(result_address: u64, result: SessionRuntimeResultV1) {
    // SAFETY:
    // 1. Invariant: the recovery boundary accepted the exact writable result address and size.
    // 2. Established by: the coordinator validates that boundary before every terminal write path.
    // 3. Lifetime: PythCore retains the result page until the expected breakpoint returns.
    // 4. Pointer ownership: this runtime performs one terminal write; PythCore reads afterward.
    // 5. Alignment: the page-aligned fixed address satisfies the result record alignment.
    // 6. Mapped length: exactly one complete 256-byte `SessionRuntimeResultV1` is written.
    // 7. Concurrency: the runtime is single-threaded and PythCore waits for the trap.
    // 8. Violation: a broken writable-map contract faults and suppresses readiness.
    unsafe { (result_address as *mut SessionRuntimeResultV1).write(result) };
}

#[cfg(not(test))]
fn trap_and_spin() -> ! {
    // SAFETY:
    // 1. Invariant: each terminal path executes this expected breakpoint exactly once.
    // 2. Established by: every success/failure branch tail-calls this diverging function.
    // 3. Lifetime: the condition applies to this single instruction.
    // 4. Pointer ownership: the instruction accesses no pointer.
    // 5. Alignment: no pointer or alignment requirement applies.
    // 6. Mapped length: no memory range is accessed.
    // 7. Concurrency: this retained runtime has one thread and one terminal path.
    // 8. Violation: an unexpected breakpoint fails containment rather than issuing graph exit.
    unsafe { core::arch::asm!("int3", options(nomem, nostack)) };
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(test))]
const fn empty_graph_exit() -> GraphExitRecord {
    GraphExitRecord {
        status: 0,
        error_code: 0,
        last_node: 0,
        executed_nodes: 0,
        result_type: GRAPH_RESULT_UNIT,
        reserved0: 0,
        reserved1: 0,
        result_raw: 0,
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
fn main() {}
