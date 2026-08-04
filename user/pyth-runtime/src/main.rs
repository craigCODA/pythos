#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::cell::UnsafeCell;
#[cfg(not(test))]
use core::panic::PanicInfo;
#[cfg(not(test))]
use pythos_shared::pyth_tig::{format::PythGraphPackage, verify::VerifiedGraph};
#[cfg(not(test))]
use pythos_shared::{
    object_shell_abi::PackedCapability,
    pyth_runtime_abi::{MAX_PYTH_GRAPH_IMPORTS, PythGraphBootstrapBlock},
    pyth_tig::format::MAX_RUNTIME_VALUES,
};
#[cfg(not(test))]
use pythos_user_pyth_runtime::{interpreter::Interpreter, syscalls, value::Value};

#[cfg(not(test))]
struct RuntimeValueStorage(UnsafeCell<[Option<Value>; MAX_RUNTIME_VALUES]>);

#[cfg(not(test))]
struct RuntimeBootstrapStorage(UnsafeCell<PythGraphBootstrapBlock>);

#[cfg(not(test))]
struct RuntimeImportStorage(UnsafeCell<[PackedCapability; MAX_PYTH_GRAPH_IMPORTS]>);

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: the Phase 2 graph runtime executes one graph on one thread.
// 2. Established by: PythCore launches a single ring-3 runtime process and the
//    runtime exposes no API for spawning threads or sharing this table.
// 3. Lifetime: the table is static for the runtime process lifetime.
// 4. Pointer ownership: `_start` takes the only mutable borrow and does not
//    retain aliases outside the interpreter invocation.
// 5. Alignment: `UnsafeCell` preserves the array's alignment.
// 6. Mapped length: exactly one `[Option<Value>; MAX_RUNTIME_VALUES]` table is
//    accessed.
// 7. Concurrency: no concurrent runtime thread exists in Phase 2.
// 8. Violation: concurrent access would corrupt graph value state.
unsafe impl Sync for RuntimeValueStorage {}

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: the Phase 2 graph runtime copies exactly one bootstrap block
//    before interpretation and never shares that cell with another thread.
// 2. Established by: PythCore launches one runtime process and passes one
//    bootstrap pointer to `_start`.
// 3. Lifetime: the copied block is static for the runtime process lifetime.
// 4. Pointer ownership: `_start` takes the only mutable borrow and uses it to
//    derive package and import metadata.
// 5. Alignment: `UnsafeCell` preserves `PythGraphBootstrapBlock` alignment.
// 6. Mapped length: exactly one `PythGraphBootstrapBlock` is accessed.
// 7. Concurrency: Phase 2 runtime code has no thread creation or reentrant
//    callback path.
// 8. Violation: concurrent access could corrupt the launch ABI view.
unsafe impl Sync for RuntimeBootstrapStorage {}

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: the Phase 2 graph runtime fills one fixed import capability
//    table before interpretation and does not mutate it afterward.
// 2. Established by: imports are derived from the copied bootstrap block inside
//    the single `_start` invocation.
// 3. Lifetime: the table is static for the runtime process lifetime.
// 4. Pointer ownership: `_start` takes the only mutable borrow and then passes
//    the table by shared reference to the interpreter.
// 5. Alignment: `UnsafeCell` preserves the capability array's alignment.
// 6. Mapped length: exactly one `[PackedCapability; MAX_PYTH_GRAPH_IMPORTS]`
//    table is accessed.
// 7. Concurrency: no concurrent runtime thread can race table initialization or
//    interpretation in Phase 2.
// 8. Violation: aliasing could swap capability handles during graph execution.
unsafe impl Sync for RuntimeImportStorage {}

#[cfg(not(test))]
static RUNTIME_VALUES: RuntimeValueStorage =
    RuntimeValueStorage(UnsafeCell::new([None; MAX_RUNTIME_VALUES]));

#[cfg(not(test))]
static RUNTIME_BOOTSTRAP: RuntimeBootstrapStorage =
    RuntimeBootstrapStorage(UnsafeCell::new(syscalls::empty_bootstrap_block()));

#[cfg(not(test))]
static RUNTIME_IMPORTS: RuntimeImportStorage = RuntimeImportStorage(UnsafeCell::new(
    [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS],
));

/// Ring-3 PythTIG runtime entry. PythCore passes a read-only
/// `PythGraphBootstrapBlock` pointer in `rdi`.
///
/// # Safety
///
/// The caller must provide a valid graph runtime bootstrap mapping, package
/// mapping, result mapping, and capability import table according to the
/// Phase 2 runtime ABI. This function never returns.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(bootstrap_ptr: *const PythGraphBootstrapBlock) -> ! {
    let bootstrap = runtime_bootstrap();
    if unsafe { syscalls::bootstrap_graph_into(bootstrap_ptr, bootstrap) }.is_err() {
        spin_forever();
    }
    let imports = runtime_imports();
    if syscalls::import_table_into(bootstrap, imports).is_err() {
        spin_forever();
    }
    let Ok(package_bytes) = (unsafe { syscalls::package_bytes(bootstrap) }) else {
        spin_forever();
    };
    let Ok(package) = PythGraphPackage::decode(package_bytes) else {
        spin_forever();
    };
    // SAFETY:
    // 1. Invariant: PythCore already ran the authoritative verifier over
    //    these exact package bytes before launching this ring-3 runtime.
    // 2. Established by: `prepare_pyth_runtime_launch` obtains
    //    `LoadedPythGraph.verified` from `pyth_graph_loader::load_named_pyth_graph`
    //    before allocating the runtime process.
    // 3. Lifetime: the package bytes are mapped read-only for this process
    //    until it reports or faults.
    // 4. Pointer ownership: PythCore owns the package page; this runtime only
    //    decodes and interprets it.
    // 5. Alignment: `PythGraphPackage` stores decoded records by value and
    //    reads byte-backed sections through checked decoders.
    // 6. Mapped length: `bootstrap_graph` bounded `package_len`; `decode`
    //    validated section ranges inside that slice.
    // 7. Concurrency: Phase 2 launches one graph runtime on one CPU.
    // 8. Violation: using this for bytes not accepted by PythCore would bypass
    //    capability, effect-chain, and control-flow verification.
    let verified = unsafe { VerifiedGraph::assume_kernel_verified_package(&package) };

    let mut host = syscalls::GraphSyscallHost;
    let values = runtime_values();
    let exit = Interpreter::new(verified, &imports, bootstrap.instruction_budget, values)
        .execute(&mut host);
    unsafe {
        syscalls::write_exit_record(bootstrap, &exit);
    }
    let _ = syscalls::notify_exit(bootstrap);
    spin_forever();
}

#[cfg(not(test))]
fn runtime_bootstrap() -> &'static mut PythGraphBootstrapBlock {
    // SAFETY:
    // 1. Invariant: `_start` is the only caller and it obtains this storage
    //    once before interpretation.
    // 2. Established by: Phase 2 launches one graph runtime process on one CPU.
    // 3. Lifetime: the block is static and lives through the runtime process.
    // 4. Pointer ownership: this returns the sole mutable bootstrap reference.
    // 5. Alignment: `UnsafeCell` preserves `PythGraphBootstrapBlock` alignment.
    // 6. Mapped length: exactly one bootstrap block is exposed.
    // 7. Concurrency: no runtime thread can race this borrow in Phase 2.
    // 8. Violation: a second mutable borrow could corrupt the launch ABI view.
    unsafe { &mut *RUNTIME_BOOTSTRAP.0.get() }
}

#[cfg(not(test))]
fn runtime_imports() -> &'static mut [PackedCapability; MAX_PYTH_GRAPH_IMPORTS] {
    // SAFETY:
    // 1. Invariant: `_start` is the only caller and fills this table once
    //    before interpretation.
    // 2. Established by: Phase 2 launches one graph runtime process on one CPU.
    // 3. Lifetime: the table is static for the runtime process lifetime.
    // 4. Pointer ownership: this returns the sole mutable imports reference.
    // 5. Alignment: `UnsafeCell` preserves array alignment.
    // 6. Mapped length: exactly one import table is exposed.
    // 7. Concurrency: no runtime thread can race this borrow in Phase 2.
    // 8. Violation: aliasing could swap capability handles mid-execution.
    unsafe { &mut *RUNTIME_IMPORTS.0.get() }
}

#[cfg(not(test))]
fn runtime_values() -> &'static mut [Option<Value>; MAX_RUNTIME_VALUES] {
    // SAFETY:
    // 1. Invariant: the runtime is single-threaded and calls this once per
    //    process invocation before entering the interpreter.
    // 2. Established by: `_start` is the only runtime entry and Phase 2 has no
    //    task creation or reentrant callbacks into runtime code.
    // 3. Lifetime: the returned table is static and used until `_start` spins.
    // 4. Pointer ownership: this function creates the sole mutable reference.
    // 5. Alignment: `UnsafeCell` preserves the table alignment.
    // 6. Mapped length: exactly one table is exposed.
    // 7. Concurrency: no second runtime thread can race this borrow.
    // 8. Violation: a second mutable borrow would allow value-table aliasing.
    unsafe { &mut *RUNTIME_VALUES.0.get() }
}

#[cfg(not(test))]
fn spin_forever() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    spin_forever()
}
