//! Ring-3 syscall wrappers for the object shell (ADR 0051/0052).
//!
//! These wrappers follow PythCore's x86-64 `syscall` calling convention
//! (number in `rax`; args in `rdi`, `rsi`, `rdx`, `r10`, `r8`; result in
//! `rax`). PythCore's syscall entry consumes all five argument registers and
//! does not preserve them, so `syscall5` declares those registers clobbered.

use crate::capability_map::CapabilityMap;
use core::arch::asm;
use core::mem::size_of;
use pythos_shared::object_shell_abi::{
    BootstrapCapabilityBlock, MAX_QUERY_RESULTS, MAX_SHELL_OBJECT_CAPS, NO_BYTE, OBJECT_KIND_NOTE,
    OBJECT_SHELL_ABI_MAJOR, OBJECT_SHELL_ABI_MINOR, OP_CREATE_OBJECT, OP_GET_HISTORY,
    OP_INSPECT_OBJECT, OP_QUERY_OBJECTS, OP_REVISE_FIELD, ObjectListEntry, ObjectShellRequest,
    ObjectShellResponse, PackedCapability, SHELL_BOOTSTRAP_MAGIC, STATUS_BAD_REQUEST,
    STATUS_BUFFER_TOO_SMALL, STATUS_DENIED, STATUS_NOT_FOUND, STATUS_OK, SYSCALL_CONSOLE_READ_BYTE,
    SYSCALL_CONSOLE_WRITE_BYTE, SYSCALL_OBJECT_REQUEST, SYSCALL_OK, SYSCALL_SYSTEM_REBOOT,
};
use pythos_shared::task_abi::{
    MAX_TASK_PROPOSAL_RESULTS, OP_ABANDON_TASK, OP_APPEND_TASK_EVENT, OP_APPROVE_PROPOSAL,
    OP_COMPLETE_TASK, OP_CREATE_TASK, OP_LIST_PROPOSALS, OP_READ_ACTIVE_TASK, OP_REJECT_PROPOSAL,
    OP_REVIVE_TASK, OP_SUSPEND_TASK, SYSCALL_TASK_REQUEST, TaskProposalListEntry, TaskRequest,
    TaskResponse,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapCapabilityError {
    NullPointer,
    BadMagic,
    UnsupportedAbi,
    NonZeroReserved,
    TooManyObjects,
}

/// Validate and copy the read-only bootstrap block PythCore maps into this
/// process at launch.
///
/// # Safety
///
/// The caller must guarantee `ptr` is exactly the pointer PythCore placed in
/// the process's entry register when it transferred control to `_start`: a
/// page mapped read-only and user-readable, containing a fully-initialized
/// `BootstrapCapabilityBlock`, valid for the duration of this call. This is a
/// distinct invariant from `syscall5`'s below — it is a single plain memory
/// read of a kernel-mapped page, not an instruction that crosses the ring
/// boundary.
pub unsafe fn bootstrap_capabilities(
    ptr: *const BootstrapCapabilityBlock,
) -> Result<BootstrapCapabilityBlock, BootstrapCapabilityError> {
    if ptr.is_null() {
        return Err(BootstrapCapabilityError::NullPointer);
    }
    // SAFETY:
    // 1. Invariant: `ptr` is a valid, readable, fully-initialized
    //    `BootstrapCapabilityBlock` for the duration of this read, per this
    //    function's own `# Safety` contract above.
    // 2. Established by: the PythCore launch ABI maps this block read-only
    //    before jumping to `_start` and passes its address to this process.
    // 3. Lifetime: valid for this single read; the returned value is an
    //    owned copy, so no borrow or pointer is retained afterward.
    // 4. Pointer ownership: PythCore owns the mapped page; this function only
    //    reads through it, never writes or frees it.
    // 5. Alignment: `BootstrapCapabilityBlock` is `#[repr(C)]` with 8-byte
    //    aligned fields; PythCore maps it at a page boundary.
    // 6. Mapped length: exactly `size_of::<BootstrapCapabilityBlock>()` bytes
    //    are mapped and readable, per the launch contract.
    // 7. Concurrency: this shell is single-threaded (ADR 0051); no concurrent
    //    access to the same page from this process.
    // 8. Violation: a bad pointer here would fault or read garbage; the
    //    magic/version/reserved/count checks below reject a garbage read
    //    rather than trust it. The caller must fail closed on `Err`.
    let block = unsafe { ptr.read() };
    if block.magic != SHELL_BOOTSTRAP_MAGIC {
        return Err(BootstrapCapabilityError::BadMagic);
    }
    if block.abi_major != OBJECT_SHELL_ABI_MAJOR || block.abi_minor != OBJECT_SHELL_ABI_MINOR {
        return Err(BootstrapCapabilityError::UnsupportedAbi);
    }
    if block.reserved0 != 0 {
        return Err(BootstrapCapabilityError::NonZeroReserved);
    }
    if usize::from(block.object_count) > MAX_SHELL_OBJECT_CAPS {
        return Err(BootstrapCapabilityError::TooManyObjects);
    }
    Ok(block)
}

/// Issue a syscall with up to 5 arguments (System V `syscall` convention:
/// number in `rax`, args in `rdi`/`rsi`/`rdx`/`r10`/`r8`, result in `rax`).
///
/// # Safety
///
/// The caller must guarantee `number` names a syscall in
/// `pythos_shared::object_shell_abi`, that `arg1..arg5` are exactly that
/// syscall's documented arguments in `rdi`/`rsi`/`rdx`/`r10`/`r8` order, and
/// that any pointer arguments reference memory this process may legally read
/// or write for the syscall's documented duration. PythCore's copy-in/copy-out
/// validation independently checks pointer arguments before use; this wrapper
/// only guarantees it followed the calling convention, not that the callee
/// accepts the call.
unsafe fn syscall5(number: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
    let result: u64;
    // SAFETY:
    // 1. Invariant: `number` names a syscall in `object_shell_abi`, and
    //    `arg1..arg5` are that syscall's documented arguments in
    //    `rdi`/`rsi`/`rdx`/`r10`/`r8` order (this function's `# Safety`
    //    contract above; every call site in this module matches it).
    // 2. Established by: the caller's contract on this function.
    // 3. Lifetime: any pointer arguments must stay valid for the syscall's
    //    documented duration; this asm block does not retain them past the
    //    instruction itself.
    // 4. Pointer ownership: request/response buffer pointers are read and/or
    //    written by PythCore during the call, not by this wrapper.
    // 5. Alignment: pointer arguments must be naturally aligned for their
    //    pointee type; callers construct them from live Rust values.
    // 6. Mapped length: PythCore's copy-in/copy-out validates any
    //    buffer-length arguments before dereferencing; this wrapper only
    //    forwards the byte length the caller supplied.
    // 7. Concurrency: this shell is single-threaded (ADR 0051); no concurrent
    //    syscall from another thread of this process.
    // 8. Violation: the x86-64 `syscall` instruction unconditionally
    //    overwrites `rcx` (old RIP) and `r11` (old RFLAGS), and PythCore's
    //    syscall entry consumes `rdi`/`rsi`/`rdx`/`r10`/`r8`/`r9` while
    //    dispatching. All are therefore declared clobbered rather than
    //    assumed preserved. PythCore also reads and writes caller-supplied
    //    buffers during the call, so this is not marked `nomem`: the compiler
    //    must not reorder, cache, or eliminate memory accesses to those
    //    buffers across this instruction. `rsp` itself is unchanged from this
    //    process's perspective (the kernel switches and restores it
    //    internally before `sysretq`), so `nostack` is accurate.
    unsafe {
        asm!(
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

pub fn write_byte(console: PackedCapability, byte: u8) {
    // SAFETY: matches syscall5's contract - SYSCALL_CONSOLE_WRITE_BYTE takes
    // the console capability and the byte to write; no pointers involved.
    unsafe {
        syscall5(
            SYSCALL_CONSOLE_WRITE_BYTE,
            console.raw(),
            u64::from(byte),
            0,
            0,
            0,
        );
    }
}

pub fn write_str(console: PackedCapability, text: &str) {
    for byte in text.bytes() {
        write_byte(console, byte);
    }
}

/// Non-blocking console read: `None` if no byte is waiting.
pub fn read_byte(console: PackedCapability) -> Option<u8> {
    // SAFETY: matches syscall5's contract - SYSCALL_CONSOLE_READ_BYTE takes
    // only the console capability; no pointers involved.
    let result = unsafe { syscall5(SYSCALL_CONSOLE_READ_BYTE, console.raw(), 0, 0, 0, 0) };
    if result == NO_BYTE {
        None
    } else {
        Some(result as u8)
    }
}

pub fn write_help(console: PackedCapability) {
    write_str(console, "help\r\n");
    write_str(console, "query kind:note\r\n");
    write_str(console, "create kind:note\r\n");
    write_str(console, "inspect object:<id>\r\n");
    write_str(console, "revise object:<id> text=\"...\"\r\n");
    write_str(console, "history object:<id>\r\n");
    write_str(console, "task new \"...\"\r\n");
    write_str(console, "task active\r\n");
    write_str(
        console,
        "task event tag:<hex> tool:<u16> object-kind:<u16>\r\n",
    );
    write_str(console, "task suspend|revive|complete|abandon <id>\r\n");
    write_str(console, "proposal list\r\n");
    write_str(
        console,
        "proposal approve <id> keep-current|suspend-current\r\n",
    );
    write_str(console, "proposal reject <id>\r\n");
    write_str(console, "reboot\r\n");
}

pub fn request_reboot(system_control: PackedCapability) {
    // SAFETY: matches syscall5's contract - SYSCALL_SYSTEM_REBOOT takes only
    // the system-control capability; no pointers involved.
    unsafe {
        syscall5(SYSCALL_SYSTEM_REBOOT, system_control.raw(), 0, 0, 0, 0);
    }
}

/// Set `request.authority` per ADR 0052's operation table, using cached
/// per-object capabilities where required and re-querying the workspace
/// first if the shell has none cached for `request.object_id`. If no object
/// capability is available even after refreshing, the request is still sent
/// with a null authority handle so the caller sees the real
/// `STATUS_DENIED` response - the workspace capability is never substituted
/// for a per-object operation.
pub fn dispatch_object_request(
    console: PackedCapability,
    object_caps: &mut CapabilityMap,
    request: &mut ObjectShellRequest,
    text: &[u8],
) {
    let mut transport = KernelObjectRequestTransport;
    let mut query_results = [empty_object_list_entry(); MAX_QUERY_RESULTS];
    let response = dispatch_object_request_with_transport(
        object_caps,
        request,
        text,
        &mut transport,
        &mut query_results,
    );
    present_response(console, request.operation, &response, &query_results);
}

/// Dispatch a typed task request using the shell's user task-control
/// capability. The caller supplies only parsed command data; authority is
/// rebound here from the read-only bootstrap capability block.
pub fn dispatch_task_request(
    console: PackedCapability,
    object_caps: &CapabilityMap,
    request: &mut TaskRequest,
    input: &[u8],
) {
    let mut transport = KernelTaskRequestTransport;
    let mut proposal_results = [empty_task_proposal_entry(); MAX_TASK_PROPOSAL_RESULTS];
    let response = dispatch_task_request_with_transport(
        object_caps,
        request,
        input,
        &mut transport,
        &mut proposal_results,
    );
    present_task_response(console, request.operation, &response, &proposal_results);
}

trait ObjectRequestTransport {
    fn send_object_request(
        &mut self,
        request: &mut ObjectShellRequest,
        response: &mut ObjectShellResponse,
    ) -> u64;
}

trait TaskRequestTransport {
    fn send_task_request(&mut self, request: &mut TaskRequest, response: &mut TaskResponse) -> u64;
}

struct KernelObjectRequestTransport;

impl ObjectRequestTransport for KernelObjectRequestTransport {
    fn send_object_request(
        &mut self,
        request: &mut ObjectShellRequest,
        response: &mut ObjectShellResponse,
    ) -> u64 {
        // SAFETY: matches syscall5's contract - SYSCALL_OBJECT_REQUEST takes a
        // pointer to a live, initialized `ObjectShellRequest` and a pointer to
        // a live, writable `ObjectShellResponse` buffer, both owned by this
        // function's caller for the duration of the call, plus their exact
        // byte sizes so PythCore's copy-in/copy-out policy can bound access.
        unsafe {
            syscall5(
                SYSCALL_OBJECT_REQUEST,
                request as *mut ObjectShellRequest as u64,
                size_of::<ObjectShellRequest>() as u64,
                response as *mut ObjectShellResponse as u64,
                size_of::<ObjectShellResponse>() as u64,
                0,
            )
        }
    }
}

struct KernelTaskRequestTransport;

impl TaskRequestTransport for KernelTaskRequestTransport {
    fn send_task_request(&mut self, request: &mut TaskRequest, response: &mut TaskResponse) -> u64 {
        // SAFETY: matches syscall5's contract - SYSCALL_TASK_REQUEST takes a
        // pointer to a live initialized TaskRequest and a pointer to a live,
        // writable TaskResponse, plus their exact byte sizes. Optional input
        // and output buffers are named by fields already set on the request
        // and remain live for this synchronous syscall.
        unsafe {
            syscall5(
                SYSCALL_TASK_REQUEST,
                request as *mut TaskRequest as u64,
                size_of::<TaskRequest>() as u64,
                response as *mut TaskResponse as u64,
                size_of::<TaskResponse>() as u64,
                0,
            )
        }
    }
}

fn dispatch_object_request_with_transport<T: ObjectRequestTransport>(
    object_caps: &mut CapabilityMap,
    request: &mut ObjectShellRequest,
    text: &[u8],
    transport: &mut T,
    query_results: &mut [ObjectListEntry; MAX_QUERY_RESULTS],
) -> ObjectShellResponse {
    match request.operation {
        OP_CREATE_OBJECT | OP_QUERY_OBJECTS => {
            request.authority = object_caps.workspace();
        }
        _ => {
            if object_caps.object_capability(request.object_id).is_none() {
                let mut discard = [empty_object_list_entry(); MAX_QUERY_RESULTS];
                refresh_workspace_query(object_caps, transport, &mut discard);
            }
            request.authority = object_caps
                .object_capability(request.object_id)
                .unwrap_or(PackedCapability::from_raw(0));
        }
    }

    if !text.is_empty() {
        request.input_ptr = text.as_ptr() as u64;
        request.input_len = text.len() as u64;
    }
    let response =
        send_object_request_with_transport(object_caps, request, transport, query_results);

    if request.operation == OP_CREATE_OBJECT && response.status == STATUS_OK {
        object_caps.remember(response.object_id, response.capability);
    }
    response
}

fn dispatch_task_request_with_transport<T: TaskRequestTransport>(
    object_caps: &CapabilityMap,
    request: &mut TaskRequest,
    input: &[u8],
    transport: &mut T,
    proposal_results: &mut [TaskProposalListEntry; MAX_TASK_PROPOSAL_RESULTS],
) -> TaskResponse {
    request.authority = object_caps.task_control().raw();
    if !input.is_empty() {
        request.input_ptr = input.as_ptr() as u64;
        request.input_len = input.len() as u64;
    }
    if request.operation == OP_LIST_PROPOSALS {
        request.output_ptr = proposal_results.as_mut_ptr() as u64;
        request.output_len = size_of::<[TaskProposalListEntry; MAX_TASK_PROPOSAL_RESULTS]>() as u64;
    } else {
        request.output_ptr = 0;
        request.output_len = 0;
    }

    let mut response = empty_task_response();
    let syscall_result = transport.send_task_request(request, &mut response);
    if syscall_result == SYSCALL_OK {
        response
    } else {
        empty_task_response()
    }
}

fn send_object_request_with_transport<T: ObjectRequestTransport>(
    object_caps: &mut CapabilityMap,
    request: &mut ObjectShellRequest,
    transport: &mut T,
    query_results: &mut [ObjectListEntry; MAX_QUERY_RESULTS],
) -> ObjectShellResponse {
    let mut response = empty_response();
    if request.operation == OP_QUERY_OBJECTS {
        request.output_ptr = query_results.as_mut_ptr() as u64;
        request.output_len = size_of::<[ObjectListEntry; MAX_QUERY_RESULTS]>() as u64;
    } else {
        request.output_ptr = 0;
        request.output_len = 0;
    }

    let syscall_result = transport.send_object_request(request, &mut response);
    if syscall_result != SYSCALL_OK {
        return empty_response();
    }

    if request.operation == OP_QUERY_OBJECTS {
        remember_valid_query_results(object_caps, &mut response, query_results);
    }
    response
}

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

const fn empty_task_response() -> TaskResponse {
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

const fn empty_task_proposal_entry() -> TaskProposalListEntry {
    TaskProposalListEntry {
        status: 0,
        proposal_kind: 0,
        reserved0: 0,
        proposal_id: 0,
        target_task_id: 0,
        candidate_task_id: 0,
        score: 0,
    }
}

const fn empty_object_list_entry() -> ObjectListEntry {
    ObjectListEntry {
        object_id: 0,
        capability: PackedCapability::from_raw(0),
    }
}

fn refresh_workspace_query<T: ObjectRequestTransport>(
    object_caps: &mut CapabilityMap,
    transport: &mut T,
    query_results: &mut [ObjectListEntry; MAX_QUERY_RESULTS],
) {
    let mut request = ObjectShellRequest {
        abi_major: OBJECT_SHELL_ABI_MAJOR,
        abi_minor: OBJECT_SHELL_ABI_MINOR,
        operation: OP_QUERY_OBJECTS,
        object_kind: OBJECT_KIND_NOTE,
        field_id: 0,
        reserved0: 0,
        authority: object_caps.workspace(),
        object_id: 0,
        input_ptr: 0,
        input_len: 0,
        output_ptr: 0,
        output_len: 0,
        reserved1: 0,
        reserved2: 0,
    };
    let _ = send_object_request_with_transport(object_caps, &mut request, transport, query_results);
}

fn remember_valid_query_results(
    object_caps: &mut CapabilityMap,
    response: &mut ObjectShellResponse,
    query_results: &[ObjectListEntry; MAX_QUERY_RESULTS],
) {
    if response.status != STATUS_OK {
        return;
    }
    let entry_size = size_of::<ObjectListEntry>() as u64;
    let buffer_size = size_of::<[ObjectListEntry; MAX_QUERY_RESULTS]>() as u64;
    if response.bytes_written > buffer_size || !response.bytes_written.is_multiple_of(entry_size) {
        response.status = STATUS_BAD_REQUEST;
        return;
    }
    let count = (response.bytes_written / entry_size) as usize;
    object_caps.remember_query_results(&query_results[..count]);
}

/// Format a typed response into the exact human transcript ADR 0051 shell
/// sessions produce. PythCore returns only typed fields; all presentation
/// text lives here in user space.
fn present_response(
    console: PackedCapability,
    operation: u16,
    response: &ObjectShellResponse,
    query_results: &[ObjectListEntry; MAX_QUERY_RESULTS],
) {
    match response.status {
        STATUS_OK => present_ok_response(console, operation, response, query_results),
        STATUS_DENIED | STATUS_NOT_FOUND => {
            write_str(console, "DENIED missing-capability\r\n");
        }
        STATUS_BUFFER_TOO_SMALL => write_str(console, "ERROR buffer-too-small\r\n"),
        _ => write_str(console, "ERROR bad-request\r\n"),
    }
}

fn present_ok_response(
    console: PackedCapability,
    operation: u16,
    response: &ObjectShellResponse,
    query_results: &[ObjectListEntry; MAX_QUERY_RESULTS],
) {
    match operation {
        OP_CREATE_OBJECT => {
            write_str(console, "CREATED object:");
            write_decimal(console, response.object_id);
            write_str(console, " revision:");
            write_decimal(console, response.revision);
            write_str(console, "\r\n");
        }
        OP_QUERY_OBJECTS => {
            let entry_size = size_of::<ObjectListEntry>() as u64;
            let count = (response.bytes_written / entry_size) as usize;
            for entry in &query_results[..count.min(query_results.len())] {
                write_str(console, "object:");
                write_decimal(console, entry.object_id);
                write_str(console, " kind:note\r\n");
            }
        }
        OP_REVISE_FIELD => {
            write_str(console, "COMMITTED revision:");
            write_decimal(console, response.revision);
            write_str(console, "\r\n");
        }
        OP_INSPECT_OBJECT => {
            write_str(console, "text=\"");
            let len = (response.bytes_written as usize).min(response.field_bytes.len());
            write_text_bytes(console, &response.field_bytes[..len]);
            write_str(console, "\" revision:");
            write_decimal(console, response.revision);
            write_str(console, "\r\n");
        }
        OP_GET_HISTORY => {
            write_str(console, "history object:");
            write_decimal(console, response.object_id);
            write_str(console, " revisions:");
            write_decimal(console, response.revision_count);
            write_str(console, "\r\n");
        }
        _ => write_str(console, "OK\r\n"),
    }
}

fn present_task_response(
    console: PackedCapability,
    operation: u16,
    response: &TaskResponse,
    proposal_results: &[TaskProposalListEntry; MAX_TASK_PROPOSAL_RESULTS],
) {
    match response.status {
        STATUS_OK => present_task_ok_response(console, operation, response, proposal_results),
        STATUS_DENIED | STATUS_NOT_FOUND => write_str(console, "DENIED missing-capability\r\n"),
        STATUS_BUFFER_TOO_SMALL => write_str(console, "ERROR buffer-too-small\r\n"),
        _ => write_str(console, "ERROR bad-request\r\n"),
    }
}

fn present_task_ok_response(
    console: PackedCapability,
    operation: u16,
    response: &TaskResponse,
    proposal_results: &[TaskProposalListEntry; MAX_TASK_PROPOSAL_RESULTS],
) {
    match operation {
        OP_CREATE_TASK => {
            write_str(console, "TASK_CREATED task:");
            write_decimal(console, response.task_id);
            write_str(console, " active:");
            write_decimal(console, response.active_task_id);
            write_str(console, "\r\n");
        }
        OP_READ_ACTIVE_TASK => {
            if response.active_task_id == 0 {
                write_str(console, "TASK_ACTIVE none\r\n");
            } else {
                write_str(console, "TASK_ACTIVE task:");
                write_decimal(console, response.active_task_id);
                write_str(console, "\r\n");
            }
        }
        OP_APPEND_TASK_EVENT => {
            write_str(console, "TASK_EVENT task:");
            write_decimal(console, response.task_id);
            write_str(console, "\r\n");
        }
        OP_LIST_PROPOSALS => present_proposal_list(console, response, proposal_results),
        OP_APPROVE_PROPOSAL => {
            write_str(console, "PROPOSAL_APPROVED proposal:");
            write_decimal(console, response.proposal_id);
            write_str(console, " task:");
            write_decimal(console, response.task_id);
            write_str(console, " active:");
            write_decimal(console, response.active_task_id);
            write_str(console, "\r\n");
        }
        OP_REJECT_PROPOSAL => {
            write_str(console, "PROPOSAL_REJECTED proposal:");
            write_decimal(console, response.proposal_id);
            write_str(console, "\r\n");
        }
        OP_SUSPEND_TASK | OP_REVIVE_TASK | OP_COMPLETE_TASK | OP_ABANDON_TASK => {
            present_task_transition(console, operation, response.task_id);
        }
        _ => write_str(console, "OK\r\n"),
    }
}

fn present_proposal_list(
    console: PackedCapability,
    response: &TaskResponse,
    proposal_results: &[TaskProposalListEntry; MAX_TASK_PROPOSAL_RESULTS],
) {
    let entry_size = size_of::<TaskProposalListEntry>() as u64;
    let count = (response.bytes_written / entry_size) as usize;
    if count == 0 {
        write_str(console, "PROPOSAL none\r\n");
        return;
    }
    for entry in &proposal_results[..count.min(proposal_results.len())] {
        write_str(console, "proposal:");
        write_decimal(console, entry.proposal_id);
        write_str(console, " kind:");
        write_decimal(console, u64::from(entry.proposal_kind));
        write_str(console, " target:");
        write_decimal(console, entry.target_task_id);
        write_str(console, " candidate:");
        write_decimal(console, entry.candidate_task_id);
        write_str(console, " score:");
        write_decimal(console, entry.score);
        write_str(console, "\r\n");
    }
}

fn present_task_transition(console: PackedCapability, operation: u16, task_id: u64) {
    match operation {
        OP_SUSPEND_TASK => write_str(console, "TASK_SUSPENDED task:"),
        OP_REVIVE_TASK => write_str(console, "TASK_REVIVED task:"),
        OP_COMPLETE_TASK => write_str(console, "TASK_COMPLETED task:"),
        OP_ABANDON_TASK => write_str(console, "TASK_ABANDONED task:"),
        _ => write_str(console, "TASK_UPDATED task:"),
    }
    write_decimal(console, task_id);
    write_str(console, "\r\n");
}

fn write_decimal(console: PackedCapability, mut value: u64) {
    let mut digits = [0u8; 20];
    let mut len = 0usize;
    loop {
        digits[len] = b'0' + (value % 10) as u8;
        len += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    for &digit in digits[..len].iter().rev() {
        write_byte(console, digit);
    }
}

fn write_text_bytes(console: PackedCapability, bytes: &[u8]) {
    for &byte in bytes {
        write_byte(console, byte);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::object_shell_abi::{
        FIELD_TEXT, OBJECT_KIND_NOTE, OBJECT_SHELL_ABI_MINOR, OP_CREATE_OBJECT, OP_INSPECT_OBJECT,
        OP_QUERY_OBJECTS, SHELL_BOOTSTRAP_MAGIC, STATUS_BAD_REQUEST, SYSCALL_OK,
    };

    fn valid_bootstrap() -> BootstrapCapabilityBlock {
        BootstrapCapabilityBlock {
            magic: SHELL_BOOTSTRAP_MAGIC,
            abi_major: OBJECT_SHELL_ABI_MAJOR,
            abi_minor: OBJECT_SHELL_ABI_MINOR,
            object_count: 0,
            reserved0: 0,
            console: PackedCapability::from_parts(1, 1),
            workspace: PackedCapability::from_parts(2, 1),
            system_control: PackedCapability::from_parts(3, 1),
            task_control: PackedCapability::from_parts(4, 1),
            objects: [ObjectListEntry {
                object_id: 0,
                capability: PackedCapability::from_raw(0),
            }; MAX_SHELL_OBJECT_CAPS],
        }
    }

    #[test]
    fn bootstrap_validation_rejects_minor_reserved_and_overfull_blocks() {
        let mut invalid_minor = valid_bootstrap();
        invalid_minor.abi_minor = OBJECT_SHELL_ABI_MINOR + 1;
        assert!(unsafe { bootstrap_capabilities(&raw const invalid_minor) }.is_err());

        let mut nonzero_reserved = valid_bootstrap();
        nonzero_reserved.reserved0 = 1;
        assert!(unsafe { bootstrap_capabilities(&raw const nonzero_reserved) }.is_err());

        let mut overfull = valid_bootstrap();
        overfull.object_count = (MAX_SHELL_OBJECT_CAPS + 1) as u16;
        assert!(unsafe { bootstrap_capabilities(&raw const overfull) }.is_err());
    }

    #[derive(Default)]
    struct RecordingTaskTransport {
        request: Option<TaskRequest>,
    }

    impl TaskRequestTransport for RecordingTaskTransport {
        fn send_task_request(
            &mut self,
            request: &mut TaskRequest,
            response: &mut TaskResponse,
        ) -> u64 {
            self.request = Some(*request);
            response.status = STATUS_OK;
            response.operation = request.operation;
            SYSCALL_OK
        }
    }

    #[test]
    fn task_request_dispatch_binds_bootstrap_task_control_authority() {
        let bootstrap = valid_bootstrap();
        let map = CapabilityMap::from_bootstrap(&bootstrap);
        let mut request = TaskRequest {
            abi_major: pythos_shared::task_abi::TASK_ABI_MAJOR,
            abi_minor: pythos_shared::task_abi::TASK_ABI_MINOR,
            operation: OP_READ_ACTIVE_TASK,
            proposal_kind: 0,
            authority: 0,
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
        };
        let mut transport = RecordingTaskTransport::default();
        let mut proposal_results = [empty_task_proposal_entry(); MAX_TASK_PROPOSAL_RESULTS];

        let response = dispatch_task_request_with_transport(
            &map,
            &mut request,
            b"",
            &mut transport,
            &mut proposal_results,
        );

        assert_eq!(response.status, STATUS_OK);
        assert_eq!(
            transport.request.unwrap().authority,
            bootstrap.task_control.raw()
        );
    }

    #[derive(Default)]
    struct QueryThenInspectTransport {
        calls: [Option<ObjectShellRequest>; 2],
        call_count: usize,
    }

    impl ObjectRequestTransport for QueryThenInspectTransport {
        fn send_object_request(
            &mut self,
            request: &mut ObjectShellRequest,
            response: &mut ObjectShellResponse,
        ) -> u64 {
            self.calls[self.call_count] = Some(*request);
            self.call_count += 1;
            if request.operation == OP_QUERY_OBJECTS {
                assert_ne!(request.output_ptr, 0);
                assert_ne!(
                    request.output_ptr,
                    response as *mut ObjectShellResponse as u64
                );
                assert_eq!(
                    request.output_len,
                    core::mem::size_of::<[ObjectListEntry; MAX_QUERY_RESULTS]>() as u64
                );
                let output = unsafe {
                    core::slice::from_raw_parts_mut(
                        request.output_ptr as *mut ObjectListEntry,
                        MAX_QUERY_RESULTS,
                    )
                };
                output[0] = ObjectListEntry {
                    object_id: 1042,
                    capability: PackedCapability::from_parts(9, 1),
                };
                response.status = STATUS_OK;
                response.bytes_written = core::mem::size_of::<ObjectListEntry>() as u64;
                SYSCALL_OK
            } else {
                response.status = STATUS_OK;
                response.field_id = FIELD_TEXT;
                response.object_id = request.object_id;
                SYSCALL_OK
            }
        }
    }

    #[test]
    fn missing_object_capability_queries_workspace_and_uses_rebound_capability() {
        let bootstrap = valid_bootstrap();
        let mut map = CapabilityMap::from_bootstrap(&bootstrap);
        let mut request = ObjectShellRequest {
            abi_major: OBJECT_SHELL_ABI_MAJOR,
            abi_minor: OBJECT_SHELL_ABI_MINOR,
            operation: OP_INSPECT_OBJECT,
            object_kind: 0,
            field_id: 0,
            reserved0: 0,
            authority: PackedCapability::from_raw(0),
            object_id: 1042,
            input_ptr: 0,
            input_len: 0,
            output_ptr: 0,
            output_len: 0,
            reserved1: 0,
            reserved2: 0,
        };
        let mut transport = QueryThenInspectTransport::default();
        let mut query_results = [empty_object_list_entry(); MAX_QUERY_RESULTS];

        let response = dispatch_object_request_with_transport(
            &mut map,
            &mut request,
            b"",
            &mut transport,
            &mut query_results,
        );

        assert_eq!(response.status, STATUS_OK);
        assert_eq!(transport.call_count, 2);
        assert_eq!(
            transport.calls[0].unwrap().authority.raw(),
            bootstrap.workspace.raw()
        );
        assert_eq!(transport.calls[0].unwrap().operation, OP_QUERY_OBJECTS);
        assert_eq!(transport.calls[1].unwrap().operation, OP_INSPECT_OBJECT);
        assert_eq!(
            transport.calls[1].unwrap().authority.raw(),
            PackedCapability::from_parts(9, 1).raw()
        );
        assert_eq!(
            map.object_capability(1042),
            Some(PackedCapability::from_parts(9, 1))
        );
    }

    #[test]
    fn unhandled_object_syscall_keeps_response_non_success_and_does_not_cache_capability() {
        struct UnhandledTransport;
        impl ObjectRequestTransport for UnhandledTransport {
            fn send_object_request(
                &mut self,
                _request: &mut ObjectShellRequest,
                _response: &mut ObjectShellResponse,
            ) -> u64 {
                0xBAD0_0001
            }
        }

        let bootstrap = valid_bootstrap();
        let mut map = CapabilityMap::from_bootstrap(&bootstrap);
        let mut request = ObjectShellRequest {
            abi_major: OBJECT_SHELL_ABI_MAJOR,
            abi_minor: OBJECT_SHELL_ABI_MINOR,
            operation: OP_CREATE_OBJECT,
            object_kind: OBJECT_KIND_NOTE,
            field_id: 0,
            reserved0: 0,
            authority: PackedCapability::from_raw(0),
            object_id: 0,
            input_ptr: 0,
            input_len: 0,
            output_ptr: 0,
            output_len: 0,
            reserved1: 0,
            reserved2: 0,
        };

        let mut query_results = [empty_object_list_entry(); MAX_QUERY_RESULTS];
        let response = dispatch_object_request_with_transport(
            &mut map,
            &mut request,
            b"",
            &mut UnhandledTransport,
            &mut query_results,
        );

        assert_eq!(response.status, STATUS_BAD_REQUEST);
        assert_eq!(map.object_capability(response.object_id), None);
    }

    #[test]
    fn malformed_query_bytes_written_does_not_rebind_capability() {
        struct MalformedQueryTransport;
        impl ObjectRequestTransport for MalformedQueryTransport {
            fn send_object_request(
                &mut self,
                request: &mut ObjectShellRequest,
                response: &mut ObjectShellResponse,
            ) -> u64 {
                if request.operation == OP_QUERY_OBJECTS {
                    response.status = STATUS_OK;
                    response.bytes_written = 1;
                    SYSCALL_OK
                } else {
                    response.status = STATUS_BAD_REQUEST;
                    SYSCALL_OK
                }
            }
        }

        let bootstrap = valid_bootstrap();
        let mut map = CapabilityMap::from_bootstrap(&bootstrap);
        let mut request = ObjectShellRequest {
            abi_major: OBJECT_SHELL_ABI_MAJOR,
            abi_minor: OBJECT_SHELL_ABI_MINOR,
            operation: OP_INSPECT_OBJECT,
            object_kind: 0,
            field_id: 0,
            reserved0: 0,
            authority: PackedCapability::from_raw(0),
            object_id: 1042,
            input_ptr: 0,
            input_len: 0,
            output_ptr: 0,
            output_len: 0,
            reserved1: 0,
            reserved2: 0,
        };

        let mut query_results = [empty_object_list_entry(); MAX_QUERY_RESULTS];
        let response = dispatch_object_request_with_transport(
            &mut map,
            &mut request,
            b"",
            &mut MalformedQueryTransport,
            &mut query_results,
        );

        assert_eq!(response.status, STATUS_BAD_REQUEST);
        assert_eq!(request.authority.raw(), 0);
        assert_eq!(map.object_capability(1042), None);
    }
}
