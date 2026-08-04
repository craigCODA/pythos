use crate::interpreter::{Host, HostError};
use core::{mem::size_of, ptr};
use pythos_shared::{
    object_shell_abi::{PackedCapability, SYSCALL_OK},
    pyth_runtime_abi::{
        GraphExitRecord, MAX_PYTH_GRAPH_IMPORTS, PYTH_GRAPH_BOOTSTRAP_MAGIC,
        PYTH_GRAPH_RUNTIME_ABI_MAJOR, PYTH_GRAPH_RUNTIME_ABI_MINOR, PythGraphBootstrapBlock,
        SYSCALL_PYTH_GRAPH_EXIT, SYSCALL_PYTH_GRAPH_LOG,
    },
    pyth_tig::format::MAX_PACKAGE_BYTES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapError {
    NullPointer,
    BadMagic,
    UnsupportedAbi,
    NonZeroReserved,
    TooManyImports,
    NullPackage,
    EmptyPackage,
    PackageTooLarge,
    ZeroBudget,
    NullResult,
    MissingImport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitNotifyError {
    Failed,
}

pub struct GraphSyscallHost;

impl Host for GraphSyscallHost {
    fn system_log(&mut self, capability: PackedCapability, text: &[u8]) -> Result<(), HostError> {
        // SAFETY: matches syscall5's contract. `text` is a live package-backed
        // UTF-8 slice for the duration of the syscall and PythCore validates
        // the active process copy map before reading it.
        let result = unsafe {
            syscall5(
                SYSCALL_PYTH_GRAPH_LOG,
                capability.raw(),
                text.as_ptr() as u64,
                text.len() as u64,
                0,
                0,
            )
        };
        if result == SYSCALL_OK {
            Ok(())
        } else {
            Err(HostError::Failed)
        }
    }
}

/// Validate and copy the read-only graph bootstrap block from PythCore.
///
/// # Safety
///
/// The caller must pass exactly the pointer PythCore placed in the runtime's
/// entry register. It must point to a readable, page-mapped
/// `PythGraphBootstrapBlock` initialized for this process and valid for the
/// duration of this call.
pub unsafe fn bootstrap_graph(
    ptr: *const PythGraphBootstrapBlock,
) -> Result<PythGraphBootstrapBlock, BootstrapError> {
    let mut block = empty_bootstrap_block();
    unsafe {
        bootstrap_graph_into(ptr, &mut block)?;
    }
    Ok(block)
}

/// Validate and copy the read-only graph bootstrap block into caller-owned
/// storage without placing the 816-byte ABI block on the user stack.
///
/// # Safety
///
/// The caller must pass exactly the pointer PythCore placed in the runtime's
/// entry register. It must point to a readable, page-mapped
/// `PythGraphBootstrapBlock` initialized for this process and valid for the
/// duration of this call. `out` must be the runtime's exclusive bootstrap
/// storage for this invocation.
pub unsafe fn bootstrap_graph_into(
    ptr: *const PythGraphBootstrapBlock,
    out: &mut PythGraphBootstrapBlock,
) -> Result<(), BootstrapError> {
    if ptr.is_null() {
        return Err(BootstrapError::NullPointer);
    }
    // SAFETY:
    // 1. Invariant: `ptr` is the PythCore-provided, readable, initialized
    //    `PythGraphBootstrapBlock` pointer described by this function's
    //    `# Safety` contract.
    // 2. Established by: the graph runtime launch ABI maps the block
    //    read-only and passes it as `_start`'s first argument.
    // 3. Lifetime: this is a single copy; `out` owns the copied block.
    // 4. Pointer ownership: PythCore owns the mapping; this runtime only
    //    reads it and writes its own storage.
    // 5. Alignment: PythCore maps the block at page alignment, which satisfies
    //    `PythGraphBootstrapBlock`'s 8-byte alignment.
    // 6. Mapped length: at least `size_of::<PythGraphBootstrapBlock>()` bytes
    //    are readable by launch contract.
    // 7. Concurrency: the v1 runtime is single-threaded and does not mutate
    //    the bootstrap mapping.
    // 8. Violation: invalid pointers fault or produce a copied block rejected
    //    by the magic/version/reserved/bounds checks below.
    unsafe {
        ptr::copy_nonoverlapping(ptr, out as *mut PythGraphBootstrapBlock, 1);
    }
    validate_bootstrap(out)
}

fn validate_bootstrap(block: &PythGraphBootstrapBlock) -> Result<(), BootstrapError> {
    if block.magic != PYTH_GRAPH_BOOTSTRAP_MAGIC {
        return Err(BootstrapError::BadMagic);
    }
    if block.abi_major != PYTH_GRAPH_RUNTIME_ABI_MAJOR
        || block.abi_minor != PYTH_GRAPH_RUNTIME_ABI_MINOR
    {
        return Err(BootstrapError::UnsupportedAbi);
    }
    if block.reserved0 != 0 {
        return Err(BootstrapError::NonZeroReserved);
    }
    if usize::from(block.import_count) > MAX_PYTH_GRAPH_IMPORTS {
        return Err(BootstrapError::TooManyImports);
    }
    if block.package_ptr == 0 {
        return Err(BootstrapError::NullPackage);
    }
    if block.package_len == 0 {
        return Err(BootstrapError::EmptyPackage);
    }
    if block.package_len > MAX_PACKAGE_BYTES as u64 {
        return Err(BootstrapError::PackageTooLarge);
    }
    if block.instruction_budget == 0 {
        return Err(BootstrapError::ZeroBudget);
    }
    if block.result_ptr == 0 {
        return Err(BootstrapError::NullResult);
    }
    for binding in block.imports[..usize::from(block.import_count)].iter() {
        if binding.reserved0 != 0 {
            return Err(BootstrapError::NonZeroReserved);
        }
        if usize::from(binding.import_slot) >= MAX_PYTH_GRAPH_IMPORTS
            || binding.capability.raw() == 0
        {
            return Err(BootstrapError::MissingImport);
        }
    }
    Ok(())
}

pub fn import_table(
    block: &PythGraphBootstrapBlock,
) -> Result<[PackedCapability; MAX_PYTH_GRAPH_IMPORTS], BootstrapError> {
    let mut imports = [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS];
    import_table_into(block, &mut imports)?;
    Ok(imports)
}

pub fn import_table_into(
    block: &PythGraphBootstrapBlock,
    imports: &mut [PackedCapability; MAX_PYTH_GRAPH_IMPORTS],
) -> Result<(), BootstrapError> {
    imports.fill(PackedCapability::from_raw(0));
    for binding in block.imports[..usize::from(block.import_count)].iter() {
        let slot = usize::from(binding.import_slot);
        if slot >= MAX_PYTH_GRAPH_IMPORTS || binding.capability.raw() == 0 {
            return Err(BootstrapError::MissingImport);
        }
        imports[slot] = binding.capability;
    }
    Ok(())
}

/// Build a read-only package byte slice from a validated bootstrap block.
///
/// # Safety
///
/// The caller must pass a block returned by `bootstrap_graph`; its package
/// pointer and length must still refer to an immutable, user-readable mapping.
pub unsafe fn package_bytes<'a>(
    block: &PythGraphBootstrapBlock,
) -> Result<&'a [u8], BootstrapError> {
    let len = usize::try_from(block.package_len).map_err(|_| BootstrapError::PackageTooLarge)?;
    if len > MAX_PACKAGE_BYTES {
        return Err(BootstrapError::PackageTooLarge);
    }
    // SAFETY:
    // 1. Invariant: `block` was accepted by `bootstrap_graph`, and its
    //    package range is still mapped read-only and readable for `len` bytes
    //    as required by this function's `# Safety` contract.
    // 2. Established by: PythCore launch maps the verified package before
    //    transferring control and never grants writable access to this runtime.
    // 3. Lifetime: the returned slice is used only while the runtime process
    //    is executing this package.
    // 4. Pointer ownership: PythCore owns package bytes; this runtime only
    //    reads them.
    // 5. Alignment: byte slices have alignment 1.
    // 6. Mapped length: `len` was checked against the bootstrap length and
    //    `MAX_PACKAGE_BYTES`.
    // 7. Concurrency: no writer is mapped into this process for the package.
    // 8. Violation: bad mappings fault during decode/verify and are contained
    //    by the user-fault path added by the launch task.
    Ok(unsafe { core::slice::from_raw_parts(block.package_ptr as *const u8, len) })
}

/// Write the final graph exit record to PythCore-owned result memory.
///
/// # Safety
///
/// The result pointer in `block` must refer to a writable, user-mapped
/// `GraphExitRecord` slot provided by PythCore for this invocation.
pub unsafe fn write_exit_record(block: &PythGraphBootstrapBlock, exit: &GraphExitRecord) {
    // SAFETY:
    // 1. Invariant: `result_ptr` identifies a writable
    //    `GraphExitRecord`-sized mapping for this process.
    // 2. Established by: PythCore builds the graph runtime bootstrap/result
    //    mappings before launch.
    // 3. Lifetime: the pointer is used for this single write only.
    // 4. Pointer ownership: PythCore owns the result slot; the runtime writes
    //    exactly one final result.
    // 5. Alignment: PythCore maps the slot at page alignment, satisfying the
    //    record's 8-byte alignment.
    // 6. Mapped length: exactly `size_of::<GraphExitRecord>()` bytes are
    //    writable for this record.
    // 7. Concurrency: the v1 runtime is single-threaded; there is no
    //    concurrent writer to the result slot.
    // 8. Violation: a bad result pointer faults and is contained by the user
    //    fault path.
    unsafe {
        (block.result_ptr as *mut GraphExitRecord).write(*exit);
    }
}

pub fn notify_exit(block: &PythGraphBootstrapBlock) -> Result<(), ExitNotifyError> {
    // SAFETY: matches syscall5's contract. `result_ptr` names the writable
    // exit-record mapping from the already validated bootstrap block, and the
    // runtime wrote exactly one `GraphExitRecord` there immediately before
    // notifying PythCore.
    let result = unsafe {
        syscall5(
            SYSCALL_PYTH_GRAPH_EXIT,
            block.result_ptr,
            size_of::<GraphExitRecord>() as u64,
            0,
            0,
            0,
        )
    };
    if result == SYSCALL_OK {
        Ok(())
    } else {
        Err(ExitNotifyError::Failed)
    }
}

unsafe fn syscall5(number: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
    let result: u64;
    // SAFETY:
    // 1. Invariant: `number` names the syscall documented by this module and
    //    `arg1..arg5` are placed in PythCore's x86-64 syscall ABI order.
    // 2. Established by: every call site in this module passes fixed ABI
    //    arguments for that syscall.
    // 3. Lifetime: pointer arguments passed by graph log/exit calls remain
    //    valid for the synchronous syscall duration.
    // 4. Pointer ownership: this wrapper does not dereference pointers.
    // 5. Alignment: pointer alignment is a caller/kernel validation concern;
    //    this wrapper only places raw argument values into registers.
    // 6. Mapped length: PythCore validates pointer lengths for syscalls that
    //    accept pointers; no pointer length is forwarded here.
    // 7. Concurrency: the v1 runtime is single-threaded.
    // 8. Violation: `syscall` clobbers rcx/r11 and PythCore consumes argument
    //    registers; all are declared clobbered and memory is not marked
    //    readonly/nomem.
    unsafe {
        core::arch::asm!(
            "syscall",
            inout("rax") number => result,
            inout("rdi") arg1 => _,
            inout("rsi") arg2 => _,
            inout("rdx") arg3 => _,
            inout("r10") arg4 => _,
            inout("r8") arg5 => _,
            lateout("r9") _,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

pub const fn expected_result_size() -> usize {
    size_of::<GraphExitRecord>()
}

pub const fn empty_capability_binding()
-> pythos_shared::pyth_runtime_abi::PythGraphCapabilityBinding {
    pythos_shared::pyth_runtime_abi::PythGraphCapabilityBinding {
        import_slot: 0,
        resource_kind: 0,
        reserved0: 0,
        rights: 0,
        capability: PackedCapability::from_raw(0),
    }
}

pub const fn empty_bootstrap_block() -> PythGraphBootstrapBlock {
    PythGraphBootstrapBlock {
        magic: 0,
        abi_major: 0,
        abi_minor: 0,
        import_count: 0,
        reserved0: 0,
        package_ptr: 0,
        package_len: 0,
        instruction_budget: 0,
        result_ptr: 0,
        imports: [empty_capability_binding(); MAX_PYTH_GRAPH_IMPORTS],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::pyth_runtime_abi::{PythGraphBootstrapBlock, PythGraphCapabilityBinding};

    fn valid_bootstrap() -> PythGraphBootstrapBlock {
        let mut imports = [PythGraphCapabilityBinding {
            import_slot: 0,
            resource_kind: 0,
            reserved0: 0,
            rights: 0,
            capability: PackedCapability::from_raw(0),
        }; MAX_PYTH_GRAPH_IMPORTS];
        imports[0] = PythGraphCapabilityBinding {
            import_slot: 0,
            resource_kind: 1,
            reserved0: 0,
            rights: 1,
            capability: PackedCapability::from_parts(7, 1),
        };

        PythGraphBootstrapBlock {
            magic: PYTH_GRAPH_BOOTSTRAP_MAGIC,
            abi_major: PYTH_GRAPH_RUNTIME_ABI_MAJOR,
            abi_minor: PYTH_GRAPH_RUNTIME_ABI_MINOR,
            import_count: 1,
            reserved0: 0,
            package_ptr: 0x1000,
            package_len: 96,
            instruction_budget: 64,
            result_ptr: 0x2000,
            imports,
        }
    }

    #[test]
    fn bootstrap_validation_rejects_reserved_import_fields() {
        let mut bootstrap = valid_bootstrap();
        bootstrap.imports[0].reserved0 = 1;

        assert_eq!(
            unsafe { bootstrap_graph(&raw const bootstrap) },
            Err(BootstrapError::NonZeroReserved)
        );
    }

    #[test]
    fn bootstrap_validation_can_copy_into_caller_storage() {
        let bootstrap = valid_bootstrap();
        let mut copied = empty_bootstrap_block();

        unsafe { bootstrap_graph_into(&raw const bootstrap, &mut copied) }.unwrap();

        assert_eq!(copied, bootstrap);
    }

    #[test]
    fn import_table_maps_bindings_by_import_slot() {
        let bootstrap = valid_bootstrap();
        let mut imports = [PackedCapability::from_raw(99); MAX_PYTH_GRAPH_IMPORTS];

        import_table_into(&bootstrap, &mut imports).unwrap();

        assert_eq!(imports[0], PackedCapability::from_parts(7, 1));
        assert_eq!(imports[1], PackedCapability::from_raw(0));
    }
}
