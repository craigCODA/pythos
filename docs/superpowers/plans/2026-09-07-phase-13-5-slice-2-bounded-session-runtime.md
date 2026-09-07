# Phase 13.5 Slice 2 Bounded Session Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove, in an opt-in two-boot QEMU profile, that one retained ring-3 session runtime with one stable `ServiceId` and one ADR 0090 input capability can consume two normalized input events while executing two fresh bounded invocations of the existing Session Manager graph, retaining only neutral session-lifetime state and writing no storage.

**Architecture:** Add a separately named `session-runtime.elf` that embeds the existing PythTIG interpreter as a library. PythCore authenticates and maps the runtime ELF, the unchanged `session-manager.tig`, a versioned read-only bootstrap, a read-only two-command fixture, and a writable result page. The retained ring-3 host owns neutral session state, polls the existing nonblocking input syscall under a finite acceptance budget, constructs a fresh `Interpreter` for each typed command, and returns once through the existing expected-breakpoint continuation. PythCore retains authority for capability grants, input binding, result validation, recovery selection, and terminal acceptance.

**Tech Stack:** Rust 1.93.1 (`no_std`, x86_64 ring 0/ring 3), PythTIG v1.1, Python 3 `unittest`, QEMU 11.1.1, OVMF 2024.02, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-09-07-phase-13-5-slice-2-bounded-session-runtime-design.md`

## Global Constraints

- ADR 0089 remains the semantic authority. Do not add `ViewingState`, Traversal, Cursor/FocusMark, activation gestures, presentation, framebuffer, Project Hall, or Task Hall behavior.
- ADR 0090 remains the lower input boundary. Do not change `SessionInputEventV1`, its syscall number, queue ownership, exclusivity, continuity, or capability checks.
- Do not modify the PythTIG v1 package/opcode ABI or `programs/session-manager/main.pyth`.
- Keep `pyth-runtime.elf` one-shot and unchanged. The retained behavior belongs only to the separately named `session-runtime.elf`.
- The two normalized input events and two typed commands are independent acceptance inputs. No code or marker may claim an input-to-command translation.
- Use a single stable session `ServiceId`; bind session input exactly once before `ps2::initialize()`; never fall back to the compatibility consumer.
- Session state is reset on every boot and never stored in an object, package context, checkpoint, journal, or block device.
- Polling is finite and acceptance-only. Do not add a blocking syscall, wait queue, production scheduler, normal-boot cutover, USB/xHCI route, or physical-hardware claim.
- Every new `unsafe` block or `unsafe impl` must carry the repository's eight-point invariant comment.
- The serial transcript is the QEMU oracle. A screenshot is not acceptance evidence.
- Each task begins red, reaches green with the smallest scoped implementation, and ends with the stated commit. Do not start Slice 3 after this plan is complete.

---

### Task 1: Freeze the Slice 2 ABI and lifecycle policy

**Files:**

- Modify: `shared/src/lib.rs`
- Modify: `shared/src/user_program_manifest.rs`
- Create: `shared/src/session_runtime_abi.rs`
- Create: `shared/src/session_runtime_lifecycle.rs`
- Test: `shared/src/session_runtime_abi.rs`
- Test: `shared/src/session_runtime_lifecycle.rs`

- [ ] Add `pub mod session_runtime_abi;` and `pub mod session_runtime_lifecycle;` to `shared/src/lib.rs` before creating the module files.

- [ ] Run `cargo test -p pythos-shared` and verify it fails with missing-module errors for both new modules.

- [ ] Create `shared/src/session_runtime_lifecycle.rs` with this exact pure policy:

```rust
use crate::pyth_runtime_abi::GRAPH_EXIT_OK;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionGraphLifecycleAction {
    Reinvoke,
    RequestRecovery,
}

pub const fn session_graph_lifecycle_action(status: u16) -> SessionGraphLifecycleAction {
    if status == GRAPH_EXIT_OK {
        SessionGraphLifecycleAction::Reinvoke
    } else {
        SessionGraphLifecycleAction::RequestRecovery
    }
}
```

- [ ] Add tests proving `GRAPH_EXIT_OK` maps to `Reinvoke`, while `GRAPH_EXIT_RUNTIME_ERROR`, `GRAPH_EXIT_BUDGET_EXHAUSTED`, and `u16::MAX` map to `RequestRecovery`.

- [ ] Create `shared/src/session_runtime_abi.rs` with these exact constants:

```rust
pub const SESSION_RUNTIME_BOOTSTRAP_MAGIC: u64 = 0x3154_5253_4553_5950; // "PYSESRT1"
pub const SESSION_RUNTIME_FIXTURE_MAGIC: u64 = 0x314D_4353_4553_5950;   // "PYSESCM1"
pub const SESSION_RUNTIME_RESULT_MAGIC: u64 = 0x3130_5253_4553_5950;    // "PYSESR01"
pub const SESSION_RUNTIME_ABI_MAJOR: u16 = 1;
pub const SESSION_RUNTIME_ABI_MINOR: u16 = 0;
pub const SESSION_RUNTIME_COMMAND_COUNT: usize = 2;
pub const SESSION_RUNTIME_MAX_COMMAND_PAYLOAD: usize = 32;
pub const SESSION_RUNTIME_EMPTY_POLL_LIMIT: u64 = 100_000_000;
pub const SESSION_COMMAND_RESOURCE_ID: u64 = 0x5059_5345_5343_4D44;
pub const SESSION_RUNTIME_RESULT_UNINITIALIZED: u16 = 0;
pub const SESSION_RUNTIME_RESULT_COMPLETE: u16 = 1;
pub const SESSION_RUNTIME_RESULT_REQUEST_RECOVERY: u16 = 2;
pub const SESSION_RUNTIME_LIFECYCLE_NONE: u16 = 0;
pub const SESSION_RUNTIME_LIFECYCLE_REINVOKE: u16 = 1;
pub const SESSION_RUNTIME_LIFECYCLE_REQUEST_RECOVERY: u16 = 2;
```

- [ ] Define the three ABI records with primitive integer discriminants so malformed user-written values can be validated without constructing an invalid Rust enum:

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionRuntimeBootstrapV1 {
    pub magic: u64,                         // offset 0
    pub abi_major: u16,                     // offset 8
    pub abi_minor: u16,                     // offset 10
    pub command_count: u16,                 // offset 12
    pub reserved0: u16,                     // offset 14
    pub session_service_id: u64,            // offset 16
    pub runtime_principal_id: u64,          // offset 24
    pub graph_principal_id: u64,            // offset 32
    pub graph_package_digest: u64,          // offset 40
    pub input_capability: PackedCapability, // offset 48
    pub console_capability: PackedCapability,// offset 56
    pub fixture_ptr: u64,                   // offset 64
    pub fixture_len: u64,                   // offset 72
    pub result_ptr: u64,                    // offset 80
    pub result_len: u64,                    // offset 88
    pub graph: PythGraphBootstrapBlock,     // offset 96
    pub reserved1: [u64; 4],                // offset 912
} // size 944, align 8

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionRuntimeFixtureV1 {
    pub magic: u64,                                      // offset 0
    pub abi_major: u16,                                  // offset 8
    pub abi_minor: u16,                                  // offset 10
    pub command_count: u16,                              // offset 12
    pub reserved0: u16,                                  // offset 14
    pub commands: [PythCommand; 2],                      // offset 16
    pub payloads: [[u8; SESSION_RUNTIME_MAX_COMMAND_PAYLOAD]; 2], // offset 144
    pub reserved1: [u64; 2],                             // offset 208
} // size 224, align 8

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionRuntimeResultV1 {
    pub magic: u64,                              // offset 0
    pub abi_major: u16,                          // offset 8
    pub abi_minor: u16,                          // offset 10
    pub terminal_status: u16,                    // offset 12
    pub last_lifecycle_action: u16,              // offset 14
    pub session_service_id: u64,                 // offset 16
    pub runtime_principal_id: u64,               // offset 24
    pub graph_principal_id: u64,                 // offset 32
    pub input_event_count: u64,                  // offset 40
    pub invocation_count: u64,                   // offset 48
    pub retained_state_before_second: u64,       // offset 56
    pub retained_state_final: u64,               // offset 64
    pub command_results: [PythCommandResult; 2], // offset 72
    pub graph_exits: [GraphExitRecord; 2],       // offset 168
    pub reserved1: [u64; 3],                     // offset 232
} // size 256, align 8
```

- [ ] Add compile-time-friendly constructors `SessionRuntimeBootstrapV1::empty()`, `SessionRuntimeFixtureV1::empty()`, and `SessionRuntimeResultV1::empty()` that zero every field and use existing `PythCommand::empty`, `PythCommandResult::empty`, and an explicitly zeroed `GraphExitRecord`.

- [ ] Add layout tests using `size_of`, `align_of`, and `offset_of!` for every offset and size listed above, plus assertions that each record fits inside one 4 KiB page.

- [ ] Add pure validation errors and validators that reject: wrong magic/version/count; nonzero reserved fields; zero identities/capabilities; identity collisions between runtime principal, graph principal, and service ID; a graph bootstrap with any import other than exactly one `RESOURCE_COMMAND` binding at its declared slot with PythTIG rights `RIGHTS_READ | RIGHTS_APPEND`; unknown command kinds; flags other than `COMMAND_FLAG_NONE`; nonzero command reserved fields; payload lengths over 32; payload pointers outside the exact fixture payload slot for their ordinal; result overlap; non-ASCII or invalid UTF-8 payload bytes; malformed terminal status/lifecycle action; and nonzero result reserved fields.

- [ ] Add fixtures proving the only accepted commands are two `COMMAND_KIND_CREATE_NOTE` records with payloads `b"slice2-one"` and `b"slice2-two"`, stored in their matching 32-byte payload slots with zero-filled tails.

- [ ] Add to `shared/src/user_program_manifest.rs`:

```rust
pub const SESSION_RUNTIME_PROGRAM_NAME: &[u8] = b"session-runtime.elf";
pub const SESSION_RUNTIME_PRINCIPAL_ID: u64 = 0x5059_5352_544D_0001;
```

- [ ] Run `cargo fmt --check` and `cargo test -p pythos-shared --features pyth-tig-test-support`; expect all shared tests green.

- [ ] Commit with `git add shared/src && git commit -m "feat: freeze session runtime contracts"`.

### Task 2: Reuse one capability and lifecycle decision boundary

**Files:**

- Modify: `core/src/capabilities.rs`
- Modify: `core/src/syscall.rs`
- Modify: `core/src/pyth_runtime_launch.rs`
- Modify: `core/src/pyth_service_supervisor.rs`
- Test: the `#[cfg(test)]` modules in those four files

- [ ] Add failing tests that assert kernel `RightsMask::APPEND == 1 << 5`, that it is distinct from `WRITE`, and that a `READ | APPEND` grant does not satisfy `WRITE`.

- [ ] Add failing syscall tests for a `grant_session_command_capability_with_table` helper: exact holder succeeds for `SESSION_COMMAND_RESOURCE_ID` plus kernel `READ | APPEND`; wrong holder, wrong resource, missing `READ`, missing `APPEND`, stale generation, and forged slot fail.

- [ ] Add failing `pyth_runtime_launch` tests for `build_pyth_command_graph_bootstrap`: it accepts exactly one verified `RESOURCE_COMMAND` import with PythTIG `RIGHTS_READ | RIGHTS_APPEND`, preserves the verified import slot, and rejects every other resource/right combination or a zero capability.

- [ ] Add failing supervisor tests for a new `record_graph_exit_status(service, status: u16)` entry: Session Manager `GRAPH_EXIT_OK` selects `RelaunchSessionManager`, while runtime error, budget exhaustion, and unknown statuses select recovery/halt without relaunching a fault loop.

- [ ] Run the four red filters separately—`cargo test -p pythos-core capabilities`, `cargo test -p pythos-core session_command`, `cargo test -p pythos-core pyth_runtime_launch`, and `cargo test -p pythos-core pyth_service_supervisor`; verify they fail because APPEND, the command grant, the graph bootstrap builder, and shared lifecycle reuse are absent.

- [ ] Add `pub const APPEND: u32 = 1 << 5;` to `RightsMask` without renumbering existing rights.

- [ ] Implement `grant_session_command_capability` in `core/src/syscall.rs` using the global capability table and a testable helper using an injected table. It must grant only:

```rust
ResourceId::new(SESSION_COMMAND_RESOURCE_ID)
RightsMask::new(RightsMask::READ | RightsMask::APPEND)
```

- [ ] Add a private `PythGraphBootstrapBinding::Command(PackedCapability)` variant and public `build_pyth_command_graph_bootstrap(...)` in `core/src/pyth_runtime_launch.rs`. Extend only the private import-binding match to admit exact `RESOURCE_COMMAND` plus `RIGHTS_READ | RIGHTS_APPEND`; do not widen generic package launches or any existing import rule.

- [ ] Add `PythServiceSupervisor::record_graph_exit_status(service, status: u16)` and make its Session Manager arm call `session_graph_lifecycle_action`. Keep `record_exit` as the existing `GraphExitStatus` compatibility entry that translates `Ok` to `GRAPH_EXIT_OK` and `Fault` to `GRAPH_EXIT_RUNTIME_ERROR`, then delegates. Preserve Task Steward behavior and the existing `SupervisorAction` public vocabulary.

- [ ] Run `cargo fmt --check` and `cargo test -p pythos-core capabilities`; then run `cargo test -p pythos-core pyth_runtime_launch` and `cargo test -p pythos-core pyth_service_supervisor`; expect green.

- [ ] Commit with `git add core/src/capabilities.rs core/src/syscall.rs core/src/pyth_runtime_launch.rs core/src/pyth_service_supervisor.rs && git commit -m "feat: add session command authority"`.

### Task 3: Build the retained runtime's pure session and command host

**Files:**

- Modify: `Cargo.toml`
- Create: `user/session-runtime/Cargo.toml`
- Create: `user/session-runtime/src/lib.rs`
- Create: `user/session-runtime/src/session_command_host.rs`
- Test: `user/session-runtime/src/lib.rs`
- Test: `user/session-runtime/src/session_command_host.rs`

- [ ] Add `user/session-runtime` to the workspace and create its manifest with package/bin name `pythos-user-session-runtime`, a library named `pythos_user_session_runtime`, normal dependencies on `pythos-shared` with `pyth-tig` and `pythos-user-pyth-runtime`, and a dev dependency on `pythos-shared` with `pyth-tig-test-support`.

- [ ] Create tests before implementation for a neutral `SessionRuntimeState` initialized as:

```rust
SessionRuntimeState {
    session_service_id,
    input_event_count: 0,
    graph_invocation_count: 0,
    previous_graph_exit_status: None,
    recovery_requested: false,
}
```

- [ ] Add tests for an `InputSequenceValidator` that accepts exactly a zero-flag, zero-reserved `KEY_A` key-down followed by relative motion `(7, -7)` with contiguous sequence numbers, including wrapping continuity. It must reject gaps, `GAP_BEFORE`, wrong source/kind/value, duplicates, reversals, extra events, and any reserved data.

- [ ] Add tests for `SessionCommandHost` proving: only the exact nonzero imported command handle is accepted; one `command_read` and one `command_result_emit` are permitted per invocation; every unrelated `Host` operation returns `HostError::Denied`; the read projects kind/object/task/proposal/text into `HostCallResult`; the emit must match the current command's kind, `COMMAND_RESULT_STATUS_OK`, and exact payload; double read, emit-before-read, double emit, wrong handle, wrong status, or wrong text fails.

- [ ] Add a test using `pyth_tig::test_support::command_read_result_emit_with_import_rights` and the real `Interpreter` to run two fresh invocations. Seed the value and host-result arrays with non-`None` sentinels before invocation 2 and assert `Interpreter::new` clears them, command 2 cannot see command 1's result, both exits are `GRAPH_EXIT_OK`, and state advances `0 -> 1 -> 2` under one service ID.

- [ ] Add a failure test in which invocation 1 returns runtime error or budget exhaustion and assert `RequestRecovery`, `recovery_requested = true`, `graph_invocation_count == 1`, and no second host is created.

- [ ] Run `cargo test -p pythos-user-session-runtime`; verify the tests initially fail for missing types and then implement the minimum pure library to make them green.

- [ ] Keep `SessionCommandHost` allocator-free, single-threaded, and bounded. Store borrowed command/payload input and one optional `PythCommandResult`; do not add a command queue or human text parser.

- [ ] Run `cargo fmt --check`, `cargo test -p pythos-user-pyth-runtime`, and `cargo test -p pythos-user-session-runtime`; expect green.

- [ ] Commit with `git add Cargo.toml Cargo.lock user/session-runtime && git commit -m "feat: add retained session runtime host"`.

### Task 4: Add the ring-3 entry, finite input client, and ELF build

**Files:**

- Create: `user/session-runtime/src/main.rs`
- Create: `user/session-runtime/src/syscalls.rs`
- Create: `user/session-runtime/linker.ld`
- Create: `scripts/build-session-runtime.py`
- Modify: `tests/test_build_orchestration.py`
- Test: `user/session-runtime/src/lib.rs`
- Test: `tests/test_build_orchestration.py`

- [ ] Add failing bootstrap tests for null/misaligned pointers, wrong bootstrap and fixture versions, wrong fixed addresses/lengths, wrong identities, malformed graph import metadata, bad fixture pointers, overlapping result memory, and nonzero reserved fields. Assert no input polling or graph invocation occurs after validation failure.

- [ ] Add failing orchestration tests that require `scripts/build-session-runtime.py` to use only `user/session-runtime/linker.ld`, target `x86_64-unknown-none`, accept an optional isolated `--target-dir`, and build package `pythos-user-session-runtime`.

- [ ] Run `cargo test -p pythos-user-session-runtime` and `py -3 -m unittest tests.test_build_orchestration`; verify the new tests fail.

- [ ] Implement `user/session-runtime/src/syscalls.rs` with only the existing console byte read/write and `SYSCALL_SESSION_INPUT_TRY_READ` wrappers. The syscall assembly must remain synchronous, use the shared numeric ABI constants, and carry the eight-point safety invariant.

- [ ] Use one aligned static `SessionInputEventV1` output slot. Never pass bootstrap, fixture, graph package, or terminal result memory as the input syscall output buffer.

- [ ] Implement `_start(bootstrap_ptr: *const SessionRuntimeBootstrapV1) -> !` in `main.rs` with this fixed order:

  1. Copy and validate the read-only bootstrap into runtime-owned storage.
  2. Copy and validate the two-command fixture.
  3. Copy and validate the embedded unchanged `PythGraphBootstrapBlock` and import table.
  4. Decode the read-only package and use `VerifiedGraph::assume_kernel_verified_package` only after the authenticated kernel boundary has been validated.
  5. Emit `BOOT_STATE_0` and `READY_FOR_EVENT_1` on COM2.
  6. Poll no more than `SESSION_RUNTIME_EMPTY_POLL_LIMIT` empty results for event 1; validate key A and continuity.
  7. Execute command/invocation 1 with fresh value and host-result tables; validate its result and clean exit; update neutral state to one.
  8. Explicitly replace the command host and seed invocation-local arrays before constructing invocation 2, allowing `Interpreter::new` to prove reset.
  9. Emit `INVOCATION_LOCAL_RESET` and `READY_FOR_EVENT_2`.
  10. Poll under a fresh finite budget for event 2; validate motion `(7, -7)` and continuity.
  11. Execute command/invocation 2 with a fresh host; validate its result and clean exit; update neutral state to two.
  12. Write one complete `SessionRuntimeResultV1` to the validated writable result pointer.
  13. Execute exactly one `int3` and never issue `SYSCALL_PYTH_GRAPH_EXIT`.

- [ ] On polling, host, result, graph, or fixture failure after the outer bootstrap has safely validated its console/result pointers, write a result with `SESSION_RUNTIME_RESULT_REQUEST_RECOVERY` and `SESSION_RUNTIME_LIFECYCLE_REQUEST_RECOVERY`, emit one bounded COM2 error marker, execute one `int3`, and never begin another graph invocation. If the outer bootstrap pointer or its console/result ranges cannot be trusted, perform no pointer-based write; execute the expected `int3` with the result page still uninitialized so PythCore deterministically rejects readiness.

- [ ] Emit the exact COM2 contract once and in order on success:

```text
PYTHOS:SESSION_RUNTIME:BOOT_STATE_0
PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_1
PYTHOS:SESSION_RUNTIME:EVENT_1_KEY_A_SEQUENCE_0
PYTHOS:SESSION_RUNTIME:COMMAND_1_SLICE2_ONE
PYTHOS:SESSION_RUNTIME:RESULT_1_SLICE2_ONE
PYTHOS:SESSION_RUNTIME:INVOCATION_1_EXIT_OK
PYTHOS:SESSION_RUNTIME:STATE_INPUTS_1_INVOCATIONS_1
PYTHOS:SESSION_RUNTIME:INVOCATION_LOCAL_RESET
PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_2
PYTHOS:SESSION_RUNTIME:EVENT_2_RELATIVE_MOTION_DX_7_DY_NEG_7_SEQUENCE_1
PYTHOS:SESSION_RUNTIME:COMMAND_2_SLICE2_TWO
PYTHOS:SESSION_RUNTIME:RESULT_2_SLICE2_TWO
PYTHOS:SESSION_RUNTIME:INVOCATION_2_EXIT_OK
PYTHOS:SESSION_RUNTIME:STATE_INPUTS_2_INVOCATIONS_2
PYTHOS:SESSION_RUNTIME:INPUT_CONTIGUOUS
PYTHOS:SESSION_RUNTIME:SESSION_ID_STABLE
PYTHOS:SESSION_RUNTIME:READY
```

- [ ] Use sequence suffixes `0` and `1` as event ordinals, not raw queue sequence values; validate the actual raw values for continuity internally so a prior queue history does not make acceptance marker text nondeterministic.

- [ ] Place the ELF at virtual base `0x0060_0000` in `user/session-runtime/linker.ld`, distinct from the shell and generic Pyth runtime bases, with page-aligned RX text and RW data/BSS segments and no RWX load segment.

- [ ] Implement `scripts/build-session-runtime.py` by the isolated builder pattern; do not edit `RUSTFLAGS` globally.

- [ ] Run:

```powershell
cargo test -p pythos-user-session-runtime
py -3 -m unittest tests.test_build_orchestration
py -3 scripts/build-session-runtime.py --target-dir target/session-runtime-probe
py -3 scripts/verify-user-elf.py --elf target/session-runtime-probe/x86_64-unknown-none/debug/pythos-user-session-runtime
```

  Expect every command green and the verifier to report a valid non-RWX user ELF.

- [ ] Commit with `git add user/session-runtime scripts/build-session-runtime.py tests/test_build_orchestration.py && git commit -m "feat: add bounded session runtime entry"`.

### Task 5: Compose the opt-in kernel launch and verified return

**Files:**

- Modify: `core/Cargo.toml`
- Modify: `core/src/main.rs`
- Modify: `core/src/input_drivers.rs`
- Modify: `core/src/ps2.rs`
- Modify: `core/src/serial.rs`
- Modify: `core/src/runtime_loader.rs`
- Modify: `core/src/process_context.rs`
- Modify: `core/src/user_mode.rs`
- Create: `core/src/session_runtime_probe.rs`
- Test: the `#[cfg(test)]` modules in `core/src/process_context.rs`, `core/src/user_mode.rs`, and `core/src/session_runtime_probe.rs`

- [ ] Add failing `SessionRuntimeCopyMapSpec` tests that require readable/non-writable bootstrap, package, and fixture ranges; readable/writable result and ELF data/BSS ranges; executable/readable ELF text; a readable/writable guarded stack; and rejection of overlap, range overflow, wrong permissions, or missing payloads.

- [ ] Add failing launch/result tests for the exact virtual map:

```text
bootstrap  0x0000_0000_7200_0000  one page, user read-only
package    0x0000_0000_7200_1000  one page, user read-only
fixture    0x0000_0000_7200_2000  one page, user read-only
result     0x0000_0000_7200_3000  one page, user read-write
```

- [ ] Add failing tests for the stable kernel session ID `0x5059_5345_5353_0001`, distinct runtime/graph/service identities, exact two-command fixture construction, graph digest/identity mismatch, one command grant, one input bind, malformed result suppression, caller clearing, and kernel-root restoration.

- [ ] Add failing `runtime_loader` tests proving `SESSION_RUNTIME_PRINCIPAL_ID` is accepted only for `session-runtime.elf`, the reserved name rejects every other principal, and a different program cannot claim the reserved principal.

- [ ] Add `session-runtime-probe = ["verify"]` to `core/Cargo.toml`, plus compile errors making it mutually exclusive with `phase13-package-test`, `session-input-bridge-probe`, `evidence-terminal`, every hardware/USB diagnostic, and normal-boot-only feature combinations.

- [ ] Run the targeted Rust tests and one conflicting-feature `cargo check`; verify red failures before implementation:

```powershell
cargo test -p pythos-core session_runtime
cargo test -p pythos-core process_context
cargo check -p pythos-core --target x86_64-unknown-none --no-default-features --features "session-runtime-probe evidence-terminal"
```

- [ ] Add `SessionRuntimeCopyMapSpec`, `ActiveUserProcess::from_session_runtime_launch`, and `copy_map_from_session_runtime_launch` to `process_context.rs`. Build from validated ELF segments, the selected guarded stack, and the four exact payload ranges; do not reuse the generic Pyth runtime map that omits ELF segments.

- [ ] Extend `runtime_loader::enforce_kernel_identity_policy` and its program-inventory duplicate-principal check with the exact session-runtime name/principal pair. Preserve the existing shell rule.

- [ ] Add a generic `run_returnable_user_process` helper in `user_mode.rs` that binds the supplied process, arms one expected breakpoint, enters with `arg0` in RDI and `arg1` in RSI, validates return, and clears the caller. Keep `run_dynamic_process_breakpoint_test` as a compatibility wrapper so Slice 1 is unchanged.

- [ ] Implement `session_runtime_probe::prepare` by following the established retained-root pattern. Before kernel CR3 activation it must load and authenticate `session-runtime.elf`, load and verify only `session-manager.tig`, allocate four fresh pages, write the immutable package and fixture, zero the bootstrap/result pages, build the validated copy map, build the retained user root, and store one prepared launch record.

- [ ] Use `SESSION_RUNTIME_PRINCIPAL_ID = 0x5059_5352_544D_0001`, graph principal `0x5059_5448_534D_0001`, and `ServiceId::from_raw(0x5059_5345_5353_0001)`. Reject equality or mismatch among them.

- [ ] Implement `session_runtime_probe::run` only after syscall and guarded-stack initialization. Grant console and session-command capabilities, bind ADR 0090 input once, construct and write the final bootstrap page through the kernel scratch mapping, emit setup markers, call `ps2::initialize()`, activate the retained user root, and enter ring 3 with bootstrap pointer in RDI and zero in RSI.

- [ ] Extend the full physical keyboard decoder and ACK-filter cfg gates in `input_drivers.rs` and `ps2.rs` to `session-runtime-probe`. Do not change decoded event semantics. Extend `serial.rs` only enough to keep COM2 available in the new finite verify profile.

- [ ] Restore the already validated kernel root after the expected breakpoint. Confirm `process_context::current_caller()` is empty, copy one `SessionRuntimeResultV1` from the retained physical result frame, validate every field and both graph exits/results, and emit readiness only after that validation.

- [ ] Emit the exact COM1 contract once and in order per boot:

```text
PYTHOS:CORE:SESSION_RUNTIME:COM2_READY
PYTHOS:CORE:SESSION_RUNTIME:AUTHORITY_CREATED
PYTHOS:CORE:SESSION_RUNTIME:IDENTITIES_VALID
PYTHOS:CORE:SESSION_RUNTIME:STREAM_BOUND
PYTHOS:CORE:SESSION_RUNTIME:PS2_READY
PYTHOS:CORE:SESSION_RUNTIME:RING3_ENTER
PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED
PYTHOS:CORE:PS2:MOUSE_IRQ_FIRED
PYTHOS:CORE:SESSION_RUNTIME:RING3_RETURN
PYTHOS:CORE:SESSION_RUNTIME:INVOCATION_1_VALID
PYTHOS:CORE:SESSION_RUNTIME:REINVOKE_VALID
PYTHOS:CORE:SESSION_RUNTIME:INVOCATION_2_VALID
PYTHOS:CORE:SESSION_RUNTIME:STATE_RETENTION_VALID
PYTHOS:CORE:SESSION_RUNTIME:NO_DISK_WRITES
PYTHOS:CORE:SESSION_RUNTIME:READY
```

- [ ] Wire `main.rs` using the same minimal-kernel-root prepare/run phase as `session-input-bridge-probe`, but in a mutually exclusive branch. Do not place Slice 2 inside `normal_boot.rs`.

- [ ] Run:

```powershell
cargo fmt --check
cargo test -p pythos-core session_runtime
cargo test -p pythos-core process_context
cargo test -p pythos-core user_mode
cargo clippy -p pythos-core --target x86_64-unknown-none --features session-runtime-probe -- -D warnings
```

  Expect green with no new warnings.

- [ ] Commit with `git add core && git commit -m "feat: compose retained session runtime probe"`.

### Task 6: Package exactly the retained runtime and Session Manager graph

**Files:**

- Modify: `scripts/build-image.py`
- Modify: `tests/test_build_orchestration.py`
- Test: `tests/test_build_orchestration.py`

- [ ] Add failing tests for `--session-runtime-elf` that require absolute-path resolution and ELF verification before any ESP mutation, an exact named-program identity/digest, inclusion of exactly `session-manager.tig`, exclusion of `pyth-runtime.elf` and `task-steward.tig`, rejection with every other PythTIG acceptance set or `--session-input-probe-elf`, and byte-identical default `build_default_init_pak()` output.

- [ ] Run `py -3 -m unittest tests.test_build_orchestration`; verify the new packaging tests fail.

- [ ] Add `SESSION_RUNTIME_ELF`, `SESSION_RUNTIME_PROGRAM_NAME`, and the matching principal constant to `build-image.py`. Add a dedicated `session_runtime_records()` helper returning one named user ELF record and one named Session Manager graph record.

- [ ] Add `session_runtime_elf: Path | None` to `build_default_init_pak`, count it as a mutually exclusive PythTIG set, and append only its two records when selected.

- [ ] Add `--session-runtime-elf` to `main()`, resolve it strictly, run `verify-user-elf.py --elf <absolute-path>`, and abort before creating directories or copying files if verification fails.

- [ ] Leave `scripts/build-iso.py` unchanged because this slice's live proof uses the ESP image path only.

- [ ] Run:

```powershell
py -3 -m unittest tests.test_build_orchestration
cargo run -p pythc -- build programs/session-manager/main.pyth -o target/pyth-tig/session-manager.tig
cargo run -p pyth-tig-tool -- verify target/pyth-tig/session-manager.tig
py -3 scripts/build-image.py --kernel target/session-runtime-probe/x86_64-unknown-none/debug/pythcore --session-runtime-elf target/session-runtime-probe/x86_64-unknown-none/debug/pythos-user-session-runtime
```

  Expect all commands green and `INIT.PAK` to contain `session-runtime.elf` plus `session-manager.tig` only for the opt-in runtime profile.

- [ ] Commit with `git add scripts/build-image.py tests/test_build_orchestration.py && git commit -m "build: package session runtime probe"`.

### Task 7: Extract the characterized QEMU probe support

**Files:**

- Create: `scripts/qemu_probe_support.py`
- Modify: `scripts/test-session-input-bridge-probe.py`
- Test: `scripts/test-session-input-bridge-probe.py`

- [ ] Move, without semantic changes, these characterized helpers from the Slice 1 harness into `qemu_probe_support.py`: `WindowsJob`, `RunnerHandle`, `spawn_runner_process`, `AcceptanceTimeline`, `SerialTail`, `Com1Observer`, `RunnerCapture`, `Com2Collector`, `connect_com2(port, timeout)`, `cleanup_posix_process_group`, and `cleanup_runner_process`.

- [ ] Change `connect_com2` to accept its TCP port explicitly, and update Slice 1 to pass `SHELL_PORT`.

- [ ] Preserve Slice 1's marker tuples, oracle assertions, build logic, input sequence, and all self-test cases in `test-session-input-bridge-probe.py`; only their helper imports/call sites may change.

- [ ] Run `py -3 scripts/test-session-input-bridge-probe.py --self-test`; verify all existing self-tests pass after extraction.

- [ ] Run `py -3 scripts/test-session-input-bridge-probe.py`; expect `SESSION_INPUT_BRIDGE_PROBE_OK` and one exact `QEMU_OUTCOME success`.

- [ ] Run `git diff -- scripts/test-session-input-bridge-probe.py` and verify no Slice 1 marker or acceptance policy changed.

- [ ] Commit with `git add scripts/qemu_probe_support.py scripts/test-session-input-bridge-probe.py && git commit -m "refactor: share qemu probe lifecycle support"`.

### Task 8: Prove two retained invocations across two fresh boots

**Files:**

- Create: `scripts/test-session-runtime-probe.py`
- Create: `tests/test_session_runtime_boundary.py`
- Test: `scripts/test-session-runtime-probe.py`
- Test: `tests/test_session_runtime_boundary.py`

- [ ] Write oracle self-tests first. A valid transcript must contain exactly the COM1 and COM2 contracts from Tasks 4 and 5, one exact `QEMU_OUTCOME success`, and the required cross-channel ordering. Confirm the valid synthetic transcript passes before adding mutations.

- [ ] Add one mutation test for every missing, duplicate, malformed, and reordered COM1/COM2 marker; changed session identity; boot 2 starting above zero; state reset between invocations; stale command/result reuse; event 2 before `INVOCATION_LOCAL_RESET`; wrong event kind/value/continuity; `GAP_BEFORE` with `INPUT_CONTIGUOUS`; graph failure followed by reinvocation; second stream binding; one-channel-only evidence; any Viewing/cursor/focus/presentation marker; panic; timeout; malformed or duplicate QEMU outcome; image mutation/truncation/replacement after either boot; and surviving runner/child processes.

- [ ] Add `tests/test_session_runtime_boundary.py`, reusing the comment/string-aware Rust scanner from `tests/test_session_input_bridge_boundary.py`. Enforce the feature's verify-only direction, allowed crate dependencies, absence of forbidden Viewing/framebuffer/USB/xHCI declarations/imports, unchanged PythTIG opcode/package files, unchanged Session Manager source, unchanged generic one-shot runtime, and no normal-boot integration. Pin the immutable lower-layer files to these current SHA-256 values so "unchanged" is an exact byte contract:

```text
shared/src/pyth_runtime_abi.rs       6008712A29E1AB1D3936EA002A681151EBBE0687C8B600302B902165FBB056D5
shared/src/pyth_command_abi.rs       0DF61C05B5C458F6BC9EC2B66F6A22764E31EB616A9EFC90D8E3083888EF4E36
shared/src/pyth_tig/opcode.rs        0BFB6E965F565B1ADCE87379651F7D4BD1037FCB1593BA4BF401F68C2C477EE7
shared/src/pyth_tig/format.rs        A898F035E9149897C0194D1A544EC6D76075C3BB2D1BE82AC140E6BB90472F92
programs/session-manager/main.pyth   2E231E6F2CBC1E6528907EE4876AFD2AEA047A5CAE6C82096AA0AFA6B3D65313
user/pyth-runtime/src/main.rs        531254EAD8904843A2F75A8E7FD02D86953544341DB9CD804912C1B52117BD71
```

- [ ] Run:

```powershell
py -3 scripts/test-session-runtime-probe.py --self-test
py -3 -m unittest tests.test_session_runtime_boundary
```

  Verify the self-tests initially fail until every rejection rule exists, then make them green without weakening the valid transcript.

- [ ] Implement `build_boot_image()` with this exact order:

```text
cargo build -p pythos-boot --target x86_64-unknown-uefi
cargo build -p pythos-core --target x86_64-unknown-none --target-dir target/session-runtime-probe --features session-runtime-probe
cargo run -p pythc -- build programs/session-manager/main.pyth -o target/pyth-tig/session-manager.tig
cargo run -p pyth-tig-tool -- verify target/pyth-tig/session-manager.tig
py -3 scripts/build-user-shell.py
py -3 scripts/verify-user-elf.py
py -3 scripts/build-session-runtime.py --target-dir target/session-runtime-probe
py -3 scripts/verify-user-elf.py --elf <absolute session-runtime ELF>
py -3 scripts/build-image.py --kernel <absolute probe pythcore> --session-runtime-elf <same absolute verified ELF>
```

- [ ] Create `target/session-runtime-probe/session-runtime-store.img` as a fresh zeroed 16 MiB file. Record its size and chunked SHA-256 before boot 1, after boot 1, and after boot 2. Adapt the chunked hashing pattern from `scripts/prepare-pyth-physical-image.py`; do not claim `test-persistent-storage.py` already hashes files.

- [ ] For each boot, use separate COM1 log paths and the shared QEMU process support. Attach the same disposable image through `--storage-image`, connect COM2, wait for `READY_FOR_EVENT_1`, call `launcher_click.press_qcode_keys(["a"])`, wait for `INVOCATION_LOCAL_RESET` and `READY_FOR_EVENT_2`, call `launcher_click.send_relative_mouse_motion(7, -7)`, collect terminal evidence, fully reap QEMU, and assert the oracle.

- [ ] Require this cross-channel order on both boots:

```text
COM1 RING3_ENTER
< COM2 BOOT_STATE_0
< COM2 READY_FOR_EVENT_1
< HARNESS QMP_A_SENT
< COM1 KEYBOARD_IRQ_FIRED
< COM2 EVENT_1_KEY_A_SEQUENCE_0
< COM2 INVOCATION_LOCAL_RESET
< COM2 READY_FOR_EVENT_2
< HARNESS QMP_MOUSE_SENT
< COM1 MOUSE_IRQ_FIRED
< COM2 EVENT_2_RELATIVE_MOTION_DX_7_DY_NEG_7_SEQUENCE_1
< COM2 READY
< COM1 RING3_RETURN
< COM1 READY
< RUNNER QEMU_OUTCOME success
```

- [ ] Require the initial, post-boot-1, and post-boot-2 image sizes and SHA-256 values to be identical. Require both COM2 transcripts to begin with `BOOT_STATE_0`.

- [ ] Run `py -3 scripts/test-session-runtime-probe.py`; expect two successful QEMU boots, unchanged hashes, `SESSION_RUNTIME_PROBE_OK`, and exactly one success outcome per boot.

- [ ] Commit with `git add scripts/test-session-runtime-probe.py tests/test_session_runtime_boundary.py && git commit -m "test: prove retained session runtime lifecycle"`.

### Task 9: Integrate CI, record the accepted boundary, and run final gates

**Files:**

- Modify: `.github/workflows/qemu-acceptance.yml`
- Modify: `tests/test_ci_workflow.py`
- Create: `docs/decisions/0091-bounded-session-runtime-lifecycle.md`
- Modify: `docs/ROADMAP.md`
- Modify: `docs/HANDOVER.md`
- Modify outside the worktree after the repository commit: `D:/PythOS-Workspace/CURRENT-STATE.md`
- Create: `.superpowers/sdd/2026-09-07-phase-13-5-session-runtime/final-report.md`

- [ ] Add failing CI contract tests requiring: the new crate unit test; core session runtime/supervisor tests; new user ELF build and verification; core and user-runtime clippy; Python compilation of both new scripts; boundary tests; oracle self-tests before the live oracle; the live Slice 2 oracle in `milestone_acceptance`; no duplication in `handoff_acceptance`; and the unchanged one-aggregate-gate job structure.

- [ ] Run `py -3 -m unittest tests.test_ci_workflow`; verify red before editing the workflow.

- [ ] Update the workflow with the required commands using `python` on Ubuntu. Keep pinned QEMU 11.1.1, pinned OVMF, cache key, parallel milestone/handoff jobs, and aggregate job unchanged.

- [ ] Run the fast static and unit gate:

```powershell
cargo fmt --check
cargo test --workspace
cargo clippy -p pythos-user-session-runtime --target x86_64-unknown-none -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none --features session-runtime-probe -- -D warnings
py -3 -m py_compile scripts/qemu_probe_support.py scripts/build-session-runtime.py scripts/test-session-runtime-probe.py
py -3 -m unittest tests.test_build_orchestration tests.test_ci_workflow tests.test_session_input_bridge_boundary tests.test_session_runtime_boundary
py -3 scripts/test-session-input-bridge-probe.py --self-test
py -3 scripts/test-session-runtime-probe.py --self-test
```

- [ ] Run the live Slice 2 gate: `py -3 scripts/test-session-runtime-probe.py`. Save both COM1 logs, the captured COM2 transcripts, both exact `QEMU_OUTCOME success` lines, and all three storage hashes in the final report.

- [ ] Run the regression gate without rebuilding between already-compatible checkpoints:

```powershell
py -3 scripts/test-session-input-bridge-probe.py
py -3 scripts/test-normal-fast-boot.py
py -3 scripts/test-persistent-storage.py
```

- [ ] If any regression fails, use `superpowers:systematic-debugging`, fix the root cause inside this slice, and repeat only the failed stage plus its downstream stages. Do not restart already-green independent stages.

- [ ] Write ADR 0091 only after the live evidence exists. Record: separately named retained host; stable session identity; exact command capability; in-process bounded reinvocation; shared lifecycle policy; acceptance-only polling and breakpoint return; QEMU/two-boot/no-write evidence; and explicit deferral of normal boot, production wait/wakeup, Viewing, USB, and physical acceptance.

- [ ] Update `docs/ROADMAP.md` and `docs/HANDOVER.md` with the exact verified commit, commands, markers, hashes, evidence limits, and the next separately invoked boundary: Slice 3 binds ADR 0089 `SessionControlInterpreter` and session-lifetime `ViewingState` to this retained owner.

- [ ] Write `.superpowers/sdd/2026-09-07-phase-13-5-session-runtime/final-report.md` with the exact HEAD, dirty/clean status, command outputs, artifact paths, SHA-256 values, and claims/non-claims. No physical Lenovo or USB claim may appear as passed.

- [ ] Run `git diff --check`, scan the complete diff for `TODO`, `TBD`, placeholder language, accidental PythTIG ABI changes, Viewing behavior, normal-boot changes, and disk writes, then run `git status --short --branch`.

- [ ] Commit repository evidence with:

```powershell
git add .github/workflows/qemu-acceptance.yml tests/test_ci_workflow.py docs/decisions/0091-bounded-session-runtime-lifecycle.md docs/ROADMAP.md docs/HANDOVER.md .superpowers/sdd/2026-09-07-phase-13-5-session-runtime/final-report.md
git commit -m "docs: accept bounded session runtime lifecycle"
```

- [ ] Update `D:/PythOS-Workspace/CURRENT-STATE.md` only after the repository commit so it records that exact branch tip and evidence. Keep that external checkpoint out of the Git commit.

- [ ] Re-run `git status --short --branch` and the plan's final evidence commands if the documentation commit changed any CI-tested file. The branch is ready for review only when the worktree is clean and every required gate is green.

- [ ] Stop at the Phase 13.5 Slice 2 boundary. Do not create Slice 3 files, do not implement Viewing, and do not deploy to USB or claim physical Lenovo acceptance.

## Final Acceptance Checklist

- [ ] One retained ring-3 `session-runtime.elf` entered once per boot under one stable `ServiceId`.
- [ ] ADR 0090 input capability granted/bound once and two exact normalized events consumed contiguously.
- [ ] Two independent typed CREATE_NOTE commands consumed from the read-only fixture; neither is derived from physical input.
- [ ] Two fresh `Interpreter` invocations of unchanged `session-manager.tig` return cleanly.
- [ ] Invocation-local value, host-result, command, result, and exit state reset between invocations.
- [ ] Neutral session state advances `0 -> 1 -> 2` and resets to zero on boot 2.
- [ ] Any graph/runtime/host/fixture failure requests recovery and never enters a reinvocation fault loop.
- [ ] PythCore validates the terminal result after restoring its root and clearing the caller.
- [ ] Disposable storage size and SHA-256 remain unchanged before, between, and after the two boots.
- [ ] Slice 1, normal fast boot, persistent storage, workspace tests, formatting, clippy, and both oracle self-test suites remain green.
- [ ] ADR 0091 and checkpoints state only the QEMU-proven Slice 2 boundary and its explicit non-claims.
