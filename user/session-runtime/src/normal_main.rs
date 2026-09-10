#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::{cell::UnsafeCell, panic::PanicInfo, ptr};
#[cfg(not(test))]
use pythos_shared::{
    capability_abi::PackedCapability,
    normal_session_abi::{
        NORMAL_SESSION_ABI_MAJOR, NORMAL_SESSION_ABI_MINOR, NORMAL_SESSION_RETURN_MAGIC,
        NormalSessionBootstrapV1, NormalSessionReturnReason, NormalSessionReturnV1,
    },
    pyth_runtime_abi::{GraphExitRecord, HostCallResult, MAX_PYTH_GRAPH_IMPORTS},
    pyth_tig::{format::MAX_RUNTIME_VALUES, verify::VerifiedGraph},
    session_input_abi::SessionInputEventV1,
    viewing::{ViewingExtent, ViewingSnapshot},
};
#[cfg(not(test))]
use pythos_user_pyth_runtime::value::Value;
#[cfg(not(test))]
use pythos_user_session_runtime::{
    normal_graph::{NormalGraphExitSink, NormalGraphRunner},
    normal_session::{
        NormalSession, NormalSessionEffects, validate_normal_session_bootstrap_address,
        validate_normal_session_launch, validate_normal_session_package,
    },
    normal_syscalls,
};

#[cfg(not(test))]
struct NormalOwnedStorage {
    bootstrap: NormalSessionBootstrapV1,
    imports: [PackedCapability; MAX_PYTH_GRAPH_IMPORTS],
    values: [Option<Value>; MAX_RUNTIME_VALUES],
    host_results: [Option<HostCallResult>; MAX_RUNTIME_VALUES],
    input_event: SessionInputEventV1,
}

#[cfg(not(test))]
struct NormalStorage(UnsafeCell<NormalOwnedStorage>);

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: one normal-session ring-3 thread owns this storage for one process lifetime.
// 2. Established by: the ELF has one entry and exposes no thread, callback, or interrupt API.
// 3. Lifetime: the static outlives the continuous normal session and every graph invocation.
// 4. Pointer ownership: `_start` obtains the sole mutable reference and never publishes it.
// 5. Alignment: UnsafeCell preserves all contained ABI records and interpreter-array alignment.
// 6. Mapped length: accesses remain within exactly one complete NormalOwnedStorage allocation.
// 7. Concurrency: PythCore runs this process single-core with no second user execution context.
// 8. Violation: aliasing could corrupt authenticated metadata or invocation-local graph state.
unsafe impl Sync for NormalStorage {}

#[cfg(not(test))]
static NORMAL_STORAGE: NormalStorage = NormalStorage(UnsafeCell::new(NormalOwnedStorage {
    bootstrap: NormalSessionBootstrapV1::empty(),
    imports: [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS],
    values: [None; MAX_RUNTIME_VALUES],
    host_results: [None; MAX_RUNTIME_VALUES],
    input_event: SessionInputEventV1::empty(),
}));

#[cfg(not(test))]
struct RuntimeEffects<'package, 'storage> {
    console: PackedCapability,
    input: PackedCapability,
    presentation: PackedCapability,
    input_event: &'storage mut SessionInputEventV1,
    graph: NormalGraphRunner<'package, 'storage>,
    graph_exit: MappedGraphExitSink,
}

#[cfg(not(test))]
struct MappedGraphExitSink {
    output: *mut GraphExitRecord,
}

#[cfg(not(test))]
impl NormalGraphExitSink for MappedGraphExitSink {
    fn write_exit(&mut self, exit: GraphExitRecord) {
        // SAFETY:
        // 1. Invariant: output is the exact graph-result address from the validated bootstrap.
        // 2. Established by: validate_normal_session_launch accepted the fixed offset-64 range.
        // 3. Lifetime: PythCore retains the writable result page through contained return.
        // 4. Pointer ownership: this runtime is the sole writer; PythCore observes the exchange.
        // 5. Alignment: the fixed address satisfies GraphExitRecord's alignment requirement.
        // 6. Mapped length: exactly one 32-byte graph-exit record is written at offset 64.
        // 7. Concurrency: one ring-3 thread completes each interpreter invocation serially.
        // 8. Violation: a broken mapping faults this process instead of forging graph success.
        unsafe { self.output.write_volatile(exit) };
    }
}

/// Continuous normal-session ring-3 entry.
///
/// # Safety
///
/// PythCore must enter with the fixed authenticated bootstrap mapping and retain
/// its read-only graph page, writable return page, and user stack until `int3`.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(bootstrap_ptr: *const NormalSessionBootstrapV1) -> ! {
    let bootstrap_address = bootstrap_ptr as u64;
    if validate_normal_session_bootstrap_address(bootstrap_address).is_err() {
        invalid_instruction();
    }

    let storage = normal_storage();
    copy_bootstrap(bootstrap_ptr, &mut storage.bootstrap);
    if validate_normal_session_launch(bootstrap_address, &storage.bootstrap).is_err() {
        invalid_instruction();
    }

    let package_bytes = package_bytes(&storage.bootstrap);
    let package = match validate_normal_session_package(&storage.bootstrap, package_bytes) {
        Ok(package) => package,
        Err(_) => return_with_reason(&storage.bootstrap, NormalSessionReturnReason::Graph),
    };
    // SAFETY:
    // 1. Invariant: this exact package was verified by PythCore before ring-3 entry.
    // 2. Established by: trusted launch admission plus the immediate length/digest/decode checks.
    // 3. Lifetime: PythCore retains the package's read-only page until contained return.
    // 4. Pointer ownership: PythCore owns the page; the runtime holds read-only decoded slices.
    // 5. Alignment: the decoder materializes encoded fields by value from byte-aligned storage.
    // 6. Mapped length: nested bootstrap validation bounds the nonempty package to one page.
    // 7. Concurrency: there is no writable user alias or concurrent package mutation.
    // 8. Violation: bypassing admission could execute an unverified graph in this process.
    let verified = unsafe { VerifiedGraph::assume_kernel_verified_package(&package) };

    storage.imports.fill(PackedCapability::from_raw(0));
    let graph_import = storage.bootstrap.graph.imports[0];
    storage.imports[usize::from(graph_import.import_slot)] = graph_import.capability;
    let extent = match ViewingExtent::new(storage.bootstrap.width, storage.bootstrap.height) {
        Ok(extent) => extent,
        Err(_) => return_with_reason(&storage.bootstrap, NormalSessionReturnReason::Bootstrap),
    };
    let mut session = match NormalSession::new(storage.bootstrap.session_service_id, extent) {
        Ok(session) => session,
        Err(reason) => return_with_reason(&storage.bootstrap, reason),
    };

    let mut effects = RuntimeEffects {
        console: storage.bootstrap.console_capability,
        input: storage.bootstrap.input_capability,
        presentation: storage.bootstrap.presentation_capability,
        input_event: &mut storage.input_event,
        graph: NormalGraphRunner::new(
            verified,
            graph_import.capability,
            storage.bootstrap.graph.instruction_budget,
            &storage.imports,
            &mut storage.values,
            &mut storage.host_results,
        ),
        graph_exit: MappedGraphExitSink {
            output: storage.bootstrap.graph.result_ptr as *mut GraphExitRecord,
        },
    };
    if let Err(reason) = session.initialize(&mut effects) {
        return_with_reason(&storage.bootstrap, reason);
    }

    loop {
        if let Err(reason) = session.step(&mut effects) {
            return_with_reason(&storage.bootstrap, reason);
        }
        #[cfg(feature = "normal-session-fault-test")]
        fault_after_acceptance_state(&session);
    }
}

#[cfg(not(test))]
impl NormalSessionEffects for RuntimeEffects<'_, '_> {
    fn try_input(&mut self) -> Result<Option<SessionInputEventV1>, NormalSessionReturnReason> {
        normal_syscalls::try_input(self.input, self.input_event)
    }

    fn try_console(&mut self) -> Result<Option<u8>, NormalSessionReturnReason> {
        normal_syscalls::try_console(self.console)
    }

    fn wait(&mut self) -> Result<u64, NormalSessionReturnReason> {
        normal_syscalls::wait(self.input, self.console)
    }

    fn present(
        &mut self,
        revision: u64,
        snapshot: ViewingSnapshot,
    ) -> Result<(), NormalSessionReturnReason> {
        normal_syscalls::present(self.presentation, revision, snapshot)
    }

    fn run_status(&mut self, payload: &[u8]) -> Result<(), NormalSessionReturnReason> {
        self.graph
            .run_status(payload, &mut self.graph_exit)
            .map(|_| ())
    }

    fn write_console(&mut self, bytes: &[u8]) -> Result<(), NormalSessionReturnReason> {
        normal_syscalls::write_console(self.console, bytes)
    }
}

#[cfg(any(test, feature = "normal-session-fault-test"))]
const fn fault_acceptance_ready(
    command_count: u64,
    event_count: u64,
    presentation_revision: u64,
    active: bool,
) -> bool {
    command_count == 3 && event_count >= 8 && presentation_revision == event_count && active
}

#[cfg(all(not(test), feature = "normal-session-fault-test"))]
fn fault_after_acceptance_state(session: &NormalSession) {
    if fault_acceptance_ready(
        session.command_count(),
        session.event_count(),
        session.presentation_revision(),
        session.snapshot().focus_mark.is_some(),
    ) {
        pythos_normal_session_fault_acceptance_ud2();
    }
}

#[cfg(all(not(test), feature = "normal-session-fault-test"))]
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn pythos_normal_session_fault_acceptance_ud2() -> ! {
    // SAFETY:
    // 1. Invariant: this acceptance-only image must fault after its proven live session state.
    // 2. Established by: the compile-time feature and fault_after_acceptance_state predicate.
    // 3. Lifetime: the invariant applies to this terminal instruction.
    // 4. Pointer ownership: UD2 accesses no pointer.
    // 5. Alignment: no pointer or alignment requirement applies.
    // 6. Mapped length: no memory range is accessed.
    // 7. Concurrency: the acceptance runtime has one execution context.
    // 8. Violation: returning would prevent the harness from proving genuine ring-3 containment.
    unsafe { core::arch::asm!("ud2", options(noreturn)) }
}

#[cfg(not(test))]
fn normal_storage() -> &'static mut NormalOwnedStorage {
    // SAFETY:
    // 1. Invariant: `_start` calls this once and owns the storage until contained return.
    // 2. Established by: one process entry and no thread or callback creation surface.
    // 3. Lifetime: static storage outlives the controller and all graph invocations.
    // 4. Pointer ownership: this creates the only mutable reference to NORMAL_STORAGE.
    // 5. Alignment: UnsafeCell preserves NormalOwnedStorage's natural alignment.
    // 6. Mapped length: exactly one complete NormalOwnedStorage value is returned.
    // 7. Concurrency: no other ring-3 execution context accesses this process-local static.
    // 8. Violation: another reference would alias bootstrap or interpreter tables.
    unsafe { &mut *NORMAL_STORAGE.0.get() }
}

#[cfg(not(test))]
fn copy_bootstrap(
    source: *const NormalSessionBootstrapV1,
    destination: &mut NormalSessionBootstrapV1,
) {
    // SAFETY:
    // 1. Invariant: source is the exact fixed bootstrap address validated before this call.
    // 2. Established by: validate_normal_session_bootstrap_address accepted the scalar address.
    // 3. Lifetime: PythCore retains the read-only bootstrap page through this one copy.
    // 4. Pointer ownership: PythCore owns source; the runtime exclusively owns destination.
    // 5. Alignment: the fixed page and destination satisfy NormalSessionBootstrapV1 alignment.
    // 6. Mapped length: PythCore maps a full page, exceeding the exact 944-byte record.
    // 7. Concurrency: no writable user alias or kernel mutation exists after ring-3 entry.
    // 8. Violation: an absent source mapping faults and is contained without pointer authority.
    unsafe { ptr::copy_nonoverlapping(source, destination, 1) };
}

#[cfg(not(test))]
fn package_bytes(bootstrap: &NormalSessionBootstrapV1) -> &'static [u8] {
    // SAFETY:
    // 1. Invariant: full bootstrap validation accepted the fixed read-only package range.
    // 2. Established by: validate_normal_session_launch checked nested pointer and length fields.
    // 3. Lifetime: PythCore retains the immutable package page until contained return.
    // 4. Pointer ownership: PythCore owns the bytes; the runtime only borrows the slice.
    // 5. Alignment: a byte slice requires alignment one.
    // 6. Mapped length: package_len is nonzero and no greater than the mapped 4096-byte page.
    // 7. Concurrency: no writer can race this single-threaded runtime's reads.
    // 8. Violation: a broken mapping contract faults before graph execution.
    unsafe {
        core::slice::from_raw_parts(
            bootstrap.graph.package_ptr as *const u8,
            bootstrap.graph.package_len as usize,
        )
    }
}

#[cfg(not(test))]
fn return_with_reason(
    bootstrap: &NormalSessionBootstrapV1,
    reason: NormalSessionReturnReason,
) -> ! {
    let record = NormalSessionReturnV1 {
        magic: NORMAL_SESSION_RETURN_MAGIC,
        abi_major: NORMAL_SESSION_ABI_MAJOR,
        abi_minor: NORMAL_SESSION_ABI_MINOR,
        reason: reason.as_wire(),
        reserved0: 0,
        service_id: bootstrap.session_service_id,
        reserved1: 0,
    };
    write_return(bootstrap.return_ptr, record);
    contained_return();
}

#[cfg(not(test))]
fn write_return(address: u64, record: NormalSessionReturnV1) {
    // SAFETY:
    // 1. Invariant: full bootstrap validation accepted this exact writable return address.
    // 2. Established by: validate_normal_session_launch completed before any caller reaches here.
    // 3. Lifetime: PythCore retains the result page until the following contained breakpoint.
    // 4. Pointer ownership: the runtime writes once; PythCore reads only after process return.
    // 5. Alignment: the page-aligned fixed address satisfies NormalSessionReturnV1 alignment.
    // 6. Mapped length: exactly the separate 32-byte return record is written at offset zero.
    // 7. Concurrency: the single runtime thread has exclusive write access before `int3`.
    // 8. Violation: a broken map faults the process and cannot fabricate a successful return.
    unsafe { (address as *mut NormalSessionReturnV1).write(record) };
}

#[cfg(not(test))]
fn contained_return() -> ! {
    // SAFETY:
    // 1. Invariant: a validated typed return record was written immediately before this trap.
    // 2. Established by: return_with_reason is the only caller.
    // 3. Lifetime: the invariant applies to this one contained breakpoint instruction.
    // 4. Pointer ownership: the instruction accesses no user pointer.
    // 5. Alignment: no pointer or alignment requirement applies.
    // 6. Mapped length: no memory range is accessed.
    // 7. Concurrency: the runtime has one thread and one terminal path.
    // 8. Violation: if PythCore unexpectedly returns, UD2 contains the invalid continuation.
    unsafe { core::arch::asm!("int3", options(nomem, nostack)) };
    invalid_instruction();
}

#[cfg(not(test))]
fn invalid_instruction() -> ! {
    // SAFETY:
    // 1. Invariant: this path must fault rather than continue with untrusted state.
    // 2. Established by: bootstrap rejection, panic, or the compile-time acceptance trigger.
    // 3. Lifetime: the invariant applies to this terminal instruction.
    // 4. Pointer ownership: UD2 accesses no pointer.
    // 5. Alignment: no pointer or alignment requirement applies.
    // 6. Mapped length: no memory range is accessed.
    // 7. Concurrency: the runtime has one execution context.
    // 8. Violation: returning would continue after a fatal trust-boundary failure.
    unsafe { core::arch::asm!("ud2", options(noreturn)) }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    invalid_instruction()
}

#[cfg(test)]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fault_acceptance_requires_third_completed_status_and_active_eight_event_projection() {
        assert!(!fault_acceptance_ready(2, 8, 8, true));
        assert!(!fault_acceptance_ready(3, 7, 7, true));
        assert!(!fault_acceptance_ready(3, 8, 7, true));
        assert!(!fault_acceptance_ready(3, 8, 8, false));
        assert!(fault_acceptance_ready(3, 8, 8, true));
    }
}
