use crate::interpreter::{Host, HostError};
use core::{mem::size_of, ptr};
use pythos_shared::{
    object_shell_abi::{
        FIELD_TEXT, MAX_QUERY_RESULTS, OBJECT_KIND_NOTE, OBJECT_SHELL_ABI_MAJOR,
        OBJECT_SHELL_ABI_MINOR, OP_CREATE_OBJECT, OP_GET_HISTORY, OP_INSPECT_OBJECT,
        OP_QUERY_OBJECTS, OP_REVISE_FIELD, ObjectListEntry, ObjectShellRequest,
        ObjectShellResponse, PackedCapability, STATUS_OK, SYSCALL_OBJECT_REQUEST, SYSCALL_OK,
    },
    pyth_runtime_abi::{
        GraphExitRecord, HostCallResult, MAX_HOST_RESULT_BYTES, MAX_PYTH_GRAPH_IMPORTS,
        PYTH_GRAPH_BOOTSTRAP_MAGIC, PYTH_GRAPH_RUNTIME_ABI_MAJOR, PYTH_GRAPH_RUNTIME_ABI_MINOR,
        PythGraphBootstrapBlock, SYSCALL_PYTH_GRAPH_EXIT, SYSCALL_PYTH_GRAPH_LOG,
    },
    pyth_tig::format::MAX_PACKAGE_BYTES,
    task_abi::{
        OP_CREATE_PROPOSAL, OP_READ_CONTEXT_SUMMARY, SYSCALL_TASK_REQUEST, TASK_ABI_MAJOR,
        TASK_ABI_MINOR, TaskContextSummary, TaskProposalKind, TaskRequest, TaskResponse,
    },
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

    fn object_create(
        &mut self,
        capability: PackedCapability,
        kind: &[u8],
    ) -> Result<HostCallResult, HostError> {
        let mut request = base_object_request(OP_CREATE_OBJECT, capability);
        request.object_kind = object_kind_from_graph(kind)?;
        let response = send_object_request(&mut request, &mut empty_query_output())?;
        Ok(result_from_response(response, request.object_id, None))
    }

    fn object_query(
        &mut self,
        capability: PackedCapability,
        kind: &[u8],
    ) -> Result<HostCallResult, HostError> {
        let mut request = base_object_request(OP_QUERY_OBJECTS, capability);
        request.object_kind = object_kind_from_graph(kind)?;
        let mut output = empty_query_output();
        request.output_ptr = output.as_mut_ptr() as u64;
        request.output_len = size_of::<[ObjectListEntry; MAX_QUERY_RESULTS]>() as u64;
        let response = send_object_request(&mut request, &mut output)?;
        Ok(result_from_response(response, 0, Some(output[0])))
    }

    fn object_inspect(
        &mut self,
        capability: PackedCapability,
        object_id: u64,
    ) -> Result<HostCallResult, HostError> {
        let mut request = base_object_request(OP_INSPECT_OBJECT, capability);
        request.object_id = object_id;
        let response = send_object_request(&mut request, &mut empty_query_output())?;
        Ok(result_from_response(response, object_id, None))
    }

    fn object_revise(
        &mut self,
        capability: PackedCapability,
        object_id: u64,
        text: &[u8],
    ) -> Result<HostCallResult, HostError> {
        if text.len() > MAX_HOST_RESULT_BYTES || text.len() > 16 {
            return Err(HostError::Failed);
        }
        let mut request = base_object_request(OP_REVISE_FIELD, capability);
        request.object_id = object_id;
        request.field_id = FIELD_TEXT;
        request.input_ptr = text.as_ptr() as u64;
        request.input_len = text.len() as u64;
        let response = send_object_request(&mut request, &mut empty_query_output())?;
        Ok(result_from_response(response, object_id, None))
    }

    fn object_history(
        &mut self,
        capability: PackedCapability,
        object_id: u64,
    ) -> Result<HostCallResult, HostError> {
        let mut request = base_object_request(OP_GET_HISTORY, capability);
        request.object_id = object_id;
        let response = send_object_request(&mut request, &mut empty_query_output())?;
        let mut result = result_from_response(response, object_id, None);
        result.revision = response.revision_count;
        Ok(result)
    }

    fn task_context(&mut self, capability: PackedCapability) -> Result<HostCallResult, HostError> {
        let mut request = base_task_request(OP_READ_CONTEXT_SUMMARY, capability);
        let mut summary = empty_task_context_summary();
        request.output_ptr = &mut summary as *mut TaskContextSummary as u64;
        request.output_len = size_of::<TaskContextSummary>() as u64;
        let response = send_task_request(&mut request)?;
        result_from_task_context(response, summary)
    }

    fn task_propose(
        &mut self,
        capability: PackedCapability,
        candidate_task_id: u64,
        score: u64,
    ) -> Result<(), HostError> {
        let mut request = base_task_request(OP_CREATE_PROPOSAL, capability);
        request.proposal_kind = TaskProposalKind::Related.code();
        request.target_task_id = candidate_task_id;
        request.score = score;
        let response = send_task_request(&mut request)?;
        if response.status == STATUS_OK {
            Ok(())
        } else {
            Err(HostError::Failed)
        }
    }
}

fn base_object_request(operation: u16, authority: PackedCapability) -> ObjectShellRequest {
    ObjectShellRequest {
        abi_major: OBJECT_SHELL_ABI_MAJOR,
        abi_minor: OBJECT_SHELL_ABI_MINOR,
        operation,
        object_kind: 0,
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

fn object_kind_from_graph(kind: &[u8]) -> Result<u16, HostError> {
    if kind == b"note" {
        Ok(OBJECT_KIND_NOTE)
    } else {
        Err(HostError::Failed)
    }
}

fn base_task_request(operation: u16, authority: PackedCapability) -> TaskRequest {
    TaskRequest {
        abi_major: TASK_ABI_MAJOR,
        abi_minor: TASK_ABI_MINOR,
        operation,
        proposal_kind: 0,
        authority: authority.raw(),
        task_id: 0,
        proposal_id: 0,
        target_task_id: 0,
        input_ptr: 0,
        input_len: 0,
        output_ptr: 0,
        output_len: 0,
        flags: 0,
        score: 0,
        reserved0: 0,
    }
}

fn send_task_request(request: &mut TaskRequest) -> Result<TaskResponse, HostError> {
    let mut response = empty_task_response();
    // SAFETY: matches syscall5's contract. `request` and `response` are live
    // stack objects for the duration of the synchronous syscall. Optional
    // context output is named by fields already written into `request` and is
    // likewise a live stack object. PythCore validates the active process copy
    // map before reading or writing user buffers.
    let result = unsafe {
        syscall5(
            SYSCALL_TASK_REQUEST,
            request as *const TaskRequest as u64,
            size_of::<TaskRequest>() as u64,
            &mut response as *mut TaskResponse as u64,
            size_of::<TaskResponse>() as u64,
            0,
        )
    };
    if result == SYSCALL_OK {
        Ok(response)
    } else {
        Err(HostError::Failed)
    }
}

fn result_from_task_context(
    response: TaskResponse,
    summary: TaskContextSummary,
) -> Result<HostCallResult, HostError> {
    if response.status != STATUS_OK {
        return Err(HostError::Failed);
    }
    let mut result = HostCallResult::empty(response.status);
    result.object_id = summary.active_task_id;
    result.revision = summary.matching_suspended_task_id;
    result.capability = PackedCapability::from_raw(summary.confidence_score);
    result.reserved0 = u32::from(summary.proposal_kind);
    let reason = context_reason_bytes(summary);
    result.bytes_len = reason.len() as u16;
    result.bytes[..reason.len()].copy_from_slice(reason);
    Ok(result)
}

fn context_reason_bytes(summary: TaskContextSummary) -> &'static [u8] {
    if summary.confidence_score >= 70 {
        b"context-shift"
    } else {
        b"context-stable"
    }
}

fn send_object_request(
    request: &mut ObjectShellRequest,
    _query_output: &mut [ObjectListEntry; MAX_QUERY_RESULTS],
) -> Result<ObjectShellResponse, HostError> {
    let mut response = empty_response();
    // SAFETY: matches syscall5's contract. `request` and `response` are live
    // stack objects for the duration of the synchronous syscall. Query output,
    // when requested, is named by fields already written into `request` and is
    // likewise a live stack object. PythCore validates the active process copy
    // map before reading or writing any of these user buffers.
    let result = unsafe {
        syscall5(
            SYSCALL_OBJECT_REQUEST,
            request as *const ObjectShellRequest as u64,
            size_of::<ObjectShellRequest>() as u64,
            &mut response as *mut ObjectShellResponse as u64,
            size_of::<ObjectShellResponse>() as u64,
            0,
        )
    };
    if result == SYSCALL_OK {
        Ok(response)
    } else {
        Err(HostError::Failed)
    }
}

fn result_from_response(
    response: ObjectShellResponse,
    fallback_object_id: u64,
    query_entry: Option<ObjectListEntry>,
) -> HostCallResult {
    let mut result = HostCallResult::empty(response.status);
    result.object_id = if response.object_id != 0 {
        response.object_id
    } else {
        fallback_object_id
    };
    result.revision = response.revision;
    result.capability = response.capability;

    if response.status == STATUS_OK {
        if let Some(entry) = query_entry
            && entry.object_id != 0
        {
            result.object_id = entry.object_id;
            result.capability = entry.capability;
        }
        if response.bytes_written <= 16 {
            result.bytes_len = response.bytes_written as u16;
            result.bytes[..response.bytes_written as usize]
                .copy_from_slice(&response.field_bytes[..response.bytes_written as usize]);
        }
    }

    result
}

const fn empty_query_output() -> [ObjectListEntry; MAX_QUERY_RESULTS] {
    [ObjectListEntry {
        object_id: 0,
        capability: PackedCapability::from_raw(0),
    }; MAX_QUERY_RESULTS]
}

const fn empty_response() -> ObjectShellResponse {
    ObjectShellResponse {
        status: pythos_shared::object_shell_abi::STATUS_BAD_REQUEST,
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

const fn empty_task_response() -> TaskResponse {
    TaskResponse {
        status: pythos_shared::object_shell_abi::STATUS_BAD_REQUEST,
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

const fn empty_task_context_summary() -> TaskContextSummary {
    TaskContextSummary {
        active_task_id: 0,
        matching_suspended_task_id: 0,
        dominant_object_kind: 0,
        dominant_tool_domain: 0,
        proposal_kind: 0,
        event_count: 0,
        active_match_count: 0,
        candidate_match_count: 0,
        tool_domain_changed: 0,
        reserved0: 0,
        confidence_score: 0,
        candidate_tag_hash: 0,
        source_event_ids: [0; 4],
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

    #[test]
    fn task_context_response_requires_ok_status_before_projecting_summary() {
        let response = empty_task_response();
        let mut summary = empty_task_context_summary();
        summary.active_task_id = 0x1001;
        summary.matching_suspended_task_id = 0x2001;
        summary.confidence_score = 85;

        assert_eq!(
            result_from_task_context(response, summary),
            Err(HostError::Failed)
        );
    }
}
