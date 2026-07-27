//! Ring-3 syscall wrappers for the object shell (ADR 0051/0052).
//!
//! These wrappers follow the standard x86-64 `syscall` calling convention
//! (number in `rax`; args in `rdi`, `rsi`, `rdx`, `r10`, `r8`; result in
//! `rax`). PythCore's current syscall trampoline (`core/src/syscall.rs`) only
//! forwards the syscall number to its dispatcher; multi-argument dispatch is
//! Task 7's job. This module is correct against the intended ABI and links
//! cleanly, but nothing here executes until Task 8's persistent launch.

use crate::capability_map::CapabilityMap;
use core::arch::asm;
use core::mem::size_of;
use pythos_shared::object_shell_abi::{
    BootstrapCapabilityBlock, MAX_SHELL_OBJECT_CAPS, OBJECT_SHELL_ABI_MAJOR, ObjectListEntry,
    ObjectShellRequest, ObjectShellResponse, PackedCapability, SHELL_BOOTSTRAP_MAGIC, STATUS_OK,
    SYSCALL_CONSOLE_READ_BYTE, SYSCALL_CONSOLE_WRITE_BYTE, SYSCALL_OBJECT_REQUEST,
    SYSCALL_SYSTEM_REBOOT,
};

/// Sentinel `syscall5` result for "no byte waiting" from `read_byte`. No valid
/// byte value exceeds `0xFF`, so this is unambiguous.
const NO_BYTE: u64 = u64::MAX;

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
) -> BootstrapCapabilityBlock {
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
    //    magic/version/count checks below reject a garbage read rather than
    //    trust it, and return an all-null block that carries no authority.
    let block = unsafe { ptr.read() };
    let valid = block.magic == SHELL_BOOTSTRAP_MAGIC
        && block.abi_major == OBJECT_SHELL_ABI_MAJOR
        && usize::from(block.object_count) <= MAX_SHELL_OBJECT_CAPS;
    if valid {
        block
    } else {
        empty_bootstrap_block()
    }
}

fn empty_bootstrap_block() -> BootstrapCapabilityBlock {
    BootstrapCapabilityBlock {
        magic: 0,
        abi_major: 0,
        abi_minor: 0,
        object_count: 0,
        reserved0: 0,
        console: PackedCapability::from_raw(0),
        workspace: PackedCapability::from_raw(0),
        system_control: PackedCapability::from_raw(0),
        objects: [ObjectListEntry {
            object_id: 0,
            capability: PackedCapability::from_raw(0),
        }; MAX_SHELL_OBJECT_CAPS],
    }
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
    //    overwrites `rcx` (old RIP) and `r11` (old RFLAGS) as part of its ISA
    //    behavior, which is why they are declared clobbered below rather than
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
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            in("r8") arg5,
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
    use pythos_shared::object_shell_abi::{OP_CREATE_OBJECT, OP_QUERY_OBJECTS};

    match request.operation {
        OP_CREATE_OBJECT | OP_QUERY_OBJECTS => {
            request.authority = object_caps.workspace();
        }
        _ => {
            if object_caps.object_capability(request.object_id).is_none() {
                refresh_workspace_query(object_caps);
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

    let mut response = ObjectShellResponse {
        status: 0,
        reserved0: 0,
        object_kind: 0,
        field_id: 0,
        object_id: 0,
        revision: 0,
        revision_count: 0,
        bytes_written: 0,
        capability: PackedCapability::from_raw(0),
        reserved1: 0,
    };
    request.output_ptr = &raw mut response as u64;
    request.output_len = size_of::<ObjectShellResponse>() as u64;

    // SAFETY: matches syscall5's contract - SYSCALL_OBJECT_REQUEST takes a
    // pointer to a live, initialized `ObjectShellRequest` and a pointer to a
    // live, writable `ObjectShellResponse` buffer, both owned by this
    // function's stack frame for the duration of the call, plus their exact
    // byte sizes so PythCore's copy-in/copy-out policy can bound its access.
    unsafe {
        syscall5(
            SYSCALL_OBJECT_REQUEST,
            &raw const *request as u64,
            size_of::<ObjectShellRequest>() as u64,
            &raw const response as u64,
            size_of::<ObjectShellResponse>() as u64,
            0,
        );
    }

    if request.operation == OP_CREATE_OBJECT && response.status == STATUS_OK {
        object_caps.remember(response.object_id, response.capability);
    }
    present_response(console, &response);
}

fn refresh_workspace_query(object_caps: &mut CapabilityMap) {
    // A real query round-trip lands with Task 7 (kernel dispatch) and Task 8
    // (persistent launch); until then this is a no-op placeholder for the
    // control flow the ADR 0051 authority model requires.
    let _ = object_caps;
}

fn present_response(console: PackedCapability, response: &ObjectShellResponse) {
    if response.status == STATUS_OK {
        write_str(console, "OK\r\n");
    } else {
        write_str(console, "DENIED missing-capability\r\n");
    }
}
