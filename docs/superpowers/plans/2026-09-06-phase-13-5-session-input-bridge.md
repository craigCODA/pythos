# Phase 13.5 Session Input Bridge Slice 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a versioned, capability-gated, nonblocking syscall through which one authorized ring-3 session consumer can receive recurring normalized keyboard and relative-mouse events from the current PS/2 IRQ path, with explicit sequence-gap evidence and no Viewing behavior.

**Architecture:** Move the existing fixed raw-event ring out of the PS/2 driver into a device-neutral `session_input` mechanism. PS/2 IRQ handlers only publish decoded `RawInputEvent` values; the compatibility launcher remains the default exclusive consumer, while an opt-in probe boot binds one ring-3 consumer and denies compatibility dequeue. The syscall validates caller, capability, ABI shape, and writable user mapping before dequeue, then copies one fixed shared-ABI event. A finite ring-3 probe receives the capability and COM2 evidence capability as launch-register arguments, proves forged-handle denial and exact ordered delivery, and returns through the existing breakpoint recovery path. ADR 0089 remains the semantic authority above this seam and is not reimplemented.

**Tech Stack:** Rust 1.93.1 `no_std` PythCore and ring-3 ELF, shared `repr(C)` ABI types, existing generation-checked capability table and `UserCopyMap`, PS/2 IRQ1/IRQ12 input, x86-64 `syscall`/`sysretq`, QEMU/QMP, COM1/COM2 evidence, Python 3 acceptance harnesses, Cargo host tests, strict cross-target Clippy.

**Spec:** docs/superpowers/specs/2026-09-05-phase-13-5-session-input-bridge-design.md

## Global Constraints

- Plan reconnaissance was performed in `D:\PythOS-Workspace\repo\pythos\.worktrees\phase13-5-session-input-bridge-design`, branch `agent/phase13-5-session-input-bridge-design`, at design commit `5f4d7e2e3b60acf818d361f097ffb57c5cb15208`.
- At execution time, create an isolated implementation worktree from a ref containing both the approved design and this committed plan. Stop on a missing design/plan ancestor, unexpected dirty tree, or a changed ADR 0089 contract instead of reconstructing intent from memory.
- ADR 0089 is the sole semantic authority for root/session activation reach, `ViewingState`, Traversal, Cursor/FocusMark, one-way activation, and presentation ownership.
- Do not modify `core/src/session_controls.rs`, `core/src/viewing/`, `core/src/viewing_input_probe.rs`, or focus-mark/framebuffer rendering in Slice 1.
- Do not recognize `Space Space Backspace Backspace`, activate a cursor feature, instantiate `ViewingState`, route Traversal, move a FocusMark, or render any Viewing state in this slice. The four keys are only deterministic event values for the transport proof.
- Do not change PythTIG v1, `programs/session-manager/main.pyth`, Pyth graph host operations, package/service supervision, normal-session lifetime, or default-boot ownership.
- Do not add click, double-click, selection, drag, chord, right/middle-button, wheel, scroll, zoom, trackpad, hub, USB HID, or xHCI production semantics.
- Preserve ADR 0088 and the existing xHCI probe stack unchanged. The new QEMU proof uses the real emulated PS/2 IRQ path and makes no xHCI or physical-hardware claim.
- Preserve existing syscall numbers and meanings. The general syscall registry changes compatibly from 1.0 to 1.1 only because `SYSCALL_SESSION_INPUT_TRY_READ` is added as introduced in 1.1.
- Reuse resource raw id `0x1A50_0100` and `RightsMask::INPUT`; do not invent a second input-stream resource or right.
- Keep the input producer allocation-free, nonblocking, and policy-free. Sequence assignment plus fixed-ring push are the only new operations reached from the IRQ publication seam. Existing first-fire IRQ evidence markers remain unchanged.
- The queue has 16 storage slots and 15 usable entries, drops the newest candidate when full, consumes a sequence number for every candidate including drops, and marks the first later delivered discontinuous event with `GAP_BEFORE`.
- There is exactly one consumer mode per boot. Default boot uses compatibility dequeue. The opt-in probe binds one session stream before `ps2::initialize()`, flushes stale events, and denies compatibility dequeue. No live handoff or unbind API is added.
- The receive syscall is nonblocking. `EMPTY` is normal flow and leaves the output buffer byte-for-byte unchanged.
- Validate active caller, exact capability resource/right/holder, exact output length, reserved arguments, natural alignment, and writable user mapping before dequeue. Every rejection leaves both queue and output untouched.
- Pass the probe-only input and COM2 capabilities in launch registers (`RDI` and `RSI`) through a finite proof helper. This is a probe launch convention, not a durable Session ABI or a new bootstrap memory format.
- The probe spins after emitting a COM2 error marker and reaches `int3` only after all evidence succeeds. Therefore PythCore cannot emit terminal COM1 readiness for a failed ring-3 proof.
- COM1 and COM2 are independent required evidence channels. `QEMU_OUTCOME success`, `NO_DISK_WRITES`, exact marker order/count, and absence of panic/timeout/error markers are mandatory.
- Use the existing `scripts/run-qemu.py`, its QMP endpoint, existing COM2 socket patterns, and `scripts/launcher_click.py` helpers. Do not create a second QEMU runner.
- Use `py -3` on Windows. Timeout is never success.
- No push, merge, USB/media write, deployment, Lenovo claim, or physical-input claim is part of implementing this plan without separate authorization.

## File and Responsibility Map

- `shared/src/capability_abi.rs`: neutral home of `PackedCapability`.
- `shared/src/object_shell_abi.rs`: compatibility re-export of `PackedCapability`; object-shell ABI remains otherwise unchanged.
- `shared/src/session_input_abi.rs`: version, syscall/result/resource constants, logical-key tags, source/kind/flag tags, and the exact 40-byte `SessionInputEventV1` wire type.
- `shared/src/lib.rs`: export the two shared ABI modules.
- `shared/src/user_program_manifest.rs`: stable name/principal constants for the opt-in ring-3 probe.
- `core/src/input_events.rs`: retain transport-neutral normalization and consume the shared input-resource identity; no queue, syscall, or Viewing policy.
- `core/src/session_input.rs`: fixed sequence-stamped queue, consumer-mode binding, compatibility dequeue, session dequeue, normalization-to-wire conversion, and discontinuity accounting.
- `core/src/ps2.rs`: retain controller setup and IRQ decode; publish raw candidates to `session_input` instead of owning a queue.
- `core/src/launcher_screen.rs`: read the named compatibility dequeue without changing launcher behavior.
- `core/src/syscall.rs`: general ABI 1.1 registry entry, exclusive input-capability grant, validation-before-dequeue, nonblocking receive, and copy-out.
- `core/src/process_context.rs`: construct a copy map for a validated user ELF plus guarded stack without requiring a shell bootstrap block.
- `core/src/user_mode.rs`: finite ring-3 entry accepting two opaque launch arguments while preserving all existing proof and persistent-entry ABIs.
- `core/src/session_input_probe.rs`: opt-in kernel orchestration only: load/validate the named probe, allocate its isolated address space and caller identity, initialize COM2, bind/grant before PS/2 initialization, enter ring 3, restore the kernel root, and gate terminal evidence.
- `core/src/main.rs`: declare the mechanism and feature-gated probe module; invoke the probe only in the opt-in verification profile.
- `core/Cargo.toml`: add `session-input-bridge-probe = ["verify"]` without depending on `viewing-input-probe` or any xHCI feature.
- `user/probes/session-input/Cargo.toml`: minimal host-testable/ring-3 probe crate.
- `user/probes/session-input/linker.ld`: explicit static user ELF layout matching the accepted ELF validator.
- `user/probes/session-input/src/lib.rs`: expected-event state machine and host tests; no hardware, control, Viewing, or rendering code.
- `user/probes/session-input/src/main.rs`: probe launch, COM2 handshake, forged call, five authorized reads, evidence, and success-only `int3`.
- `user/probes/session-input/src/syscalls.rs`: raw five-argument syscall wrapper plus input-read and console helpers.
- `Cargo.toml`: add the probe crate to the workspace.
- `scripts/build-session-input-probe.py`: build the probe with its linker and optional isolated target directory.
- `scripts/verify-user-elf.py`: accept an optional ELF path while preserving the current no-argument shell verification contract.
- `scripts/build-image.py`: opt-in packaging of `session-input-probe.elf` as a named user ELF; normal images remain byte-for-byte contract-compatible.
- `scripts/launcher_click.py`: reusable QMP relative-motion helper and exact Slice 1 injection helper.
- `scripts/test-session-input-bridge-probe.py`: build, COM2 handshake, QMP injection, dual-channel oracle, process cleanup, and oracle self-tests.
- `tests/test_build_orchestration.py`: prove probe build/verification precedes packaging and uses isolated artifacts.
- `tests/test_qemu_marker_actions.py`: prove exact keyboard and relative-motion QMP commands.
- `tests/test_session_input_bridge_boundary.py`: source-level architecture guard forbidding Viewing, PythTIG, xHCI, and framebuffer dependencies in the new slice.
- `.github/workflows/qemu-acceptance.yml`: compile-check the new scripts, run oracle/unit checks, strict-Clippy the feature, and execute the opt-in QEMU proof.
- `docs/decisions/0090-session-input-bridge.md`: mechanism decision subordinate to ADR 0089, with evidence scope and status.
- `docs/PythOS-TDD-001.md`, `docs/TECHNICAL-OVERVIEW.md`, `README.md`: ABI, opt-in command, evidence, and explicit non-goals.
- `D:\PythOS-Workspace\CURRENT-STATE.md`: final external checkpoint only after fresh verification.

---

### Task 1: Neutral Capability ABI and Versioned Session-Input Wire Contract

**Files:**
- Create: `shared/src/capability_abi.rs`
- Create: `shared/src/session_input_abi.rs`
- Modify: `shared/src/object_shell_abi.rs`
- Modify: `shared/src/lib.rs`
- Modify: `shared/src/user_program_manifest.rs`
- Create: `docs/decisions/0090-session-input-bridge.md`

**Interfaces:**
- Produces: `capability_abi::PackedCapability` with the existing raw/slot/generation layout.
- Preserves: `object_shell_abi::PackedCapability` as a public re-export.
- Produces: `SessionInputEventV1`, ABI 1.0 constants, syscall/result tags, logical-key tags, and resource id `0x1A50_0100`.
- Produces: `SESSION_INPUT_PROBE_PROGRAM_NAME` and `SESSION_INPUT_PROBE_PRINCIPAL_ID`.

- [ ] **Step 1: Write failing shared-ABI tests**

Add tests in `shared/src/session_input_abi.rs` before defining the symbols:

~~~rust
#[test]
fn event_v1_layout_is_exact_and_reserved_fields_start_zero() {
    assert_eq!(SESSION_INPUT_ABI_MAJOR, 1);
    assert_eq!(SESSION_INPUT_ABI_MINOR, 0);
    assert_eq!(SYSCALL_SESSION_INPUT_TRY_READ, 0x5059_0150);
    assert_eq!(SESSION_INPUT_RESULT_EVENT, 0x5059_004F);
    assert_eq!(SESSION_INPUT_RESULT_EMPTY, 0x5059_0150_0000);
    assert_eq!(SESSION_INPUT_RESOURCE_ID, 0x1A50_0100);
    assert_eq!(core::mem::size_of::<SessionInputEventV1>(), 40);
    assert_eq!(core::mem::align_of::<SessionInputEventV1>(), 8);
    assert_eq!(core::mem::offset_of!(SessionInputEventV1, sequence), 0);
    assert_eq!(core::mem::offset_of!(SessionInputEventV1, kind), 8);
    assert_eq!(core::mem::offset_of!(SessionInputEventV1, source), 10);
    assert_eq!(core::mem::offset_of!(SessionInputEventV1, flags), 12);
    assert_eq!(core::mem::offset_of!(SessionInputEventV1, value0), 16);
    assert_eq!(core::mem::offset_of!(SessionInputEventV1, value1), 20);
    assert_eq!(core::mem::offset_of!(SessionInputEventV1, reserved0), 24);
    assert_eq!(core::mem::offset_of!(SessionInputEventV1, reserved1), 32);
    let event = SessionInputEventV1::empty();
    assert_eq!(event.reserved0, 0);
    assert_eq!(event.reserved1, 0);
}

#[test]
fn logical_key_tags_are_stable_not_rust_discriminants() {
    assert_eq!(KEY_A, 0x0001);
    assert_eq!(KEY_Z, 0x001A);
    assert_eq!(KEY_DIGIT0, 0x0020);
    assert_eq!(KEY_DIGIT9, 0x0029);
    assert_eq!(KEY_ENTER, 0x0030);
    assert_eq!(KEY_ESCAPE, 0x0031);
    assert_eq!(KEY_SPACE, 0x0032);
    assert_eq!(KEY_BACKSPACE, 0x0033);
}
~~~

In `shared/src/object_shell_abi.rs`, add a compatibility test importing both paths and asserting equal layout and values.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

~~~powershell
cargo test -p pythos-shared session_input_abi -- --nocapture
cargo test -p pythos-shared packed_capability_reexport -- --nocapture
~~~

Expected RED: the neutral capability module, session-input module, constants, and event type do not exist.

- [ ] **Step 3: Extract `PackedCapability` without breaking imports**

Move the existing type and methods unchanged into `shared/src/capability_abi.rs`:

~~~rust
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackedCapability {
    raw: u64,
}

impl PackedCapability {
    pub const fn from_raw(raw: u64) -> Self { Self { raw } }
    pub const fn from_parts(slot: u32, generation: u32) -> Self {
        Self { raw: (slot as u64) | ((generation as u64) << 32) }
    }
    pub const fn raw(self) -> u64 { self.raw }
    pub const fn slot(self) -> u32 { self.raw as u32 }
    pub const fn generation(self) -> u32 { (self.raw >> 32) as u32 }
}
~~~

Replace the old definition in `object_shell_abi.rs` with:

~~~rust
pub use crate::capability_abi::PackedCapability;
~~~

Do not mass-rewrite existing object-shell or Pyth runtime imports; the re-export is the compatibility contract.

- [ ] **Step 4: Define the exact session-input ABI**

In `session_input_abi.rs`, define the locked numeric constants and:

~~~rust
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionInputEventV1 {
    pub sequence: u64,
    pub kind: u16,
    pub source: u16,
    pub flags: u32,
    pub value0: i32,
    pub value1: i32,
    pub reserved0: u64,
    pub reserved1: u64,
}

impl SessionInputEventV1 {
    pub const fn empty() -> Self {
        Self {
            sequence: 0,
            kind: 0,
            source: 0,
            flags: 0,
            value0: 0,
            value1: 0,
            reserved0: 0,
            reserved1: 0,
        }
    }
}
~~~

Define every `A..Z` and `Digit0..Digit9` tag explicitly rather than deriving values from `KeyCode` order. Define `SESSION_INPUT_SOURCE_KEYBOARD = 1`, `SESSION_INPUT_SOURCE_MOUSE = 2`, kinds 1/2/3, and `SESSION_INPUT_FLAG_GAP_BEFORE = 1`.

- [ ] **Step 5: Record the subordinate mechanism ADR**

Create ADR 0090 with status `Accepted design; implementation pending`. It must state:

~~~text
ADR 0089 remains authoritative for all Viewing and activation semantics.
ADR 0090 decides only the privileged delivery mechanism beneath that boundary.
QEMU acceptance will prove emulated PS/2 IRQ-to-ring-3 delivery, not physical or USB input.
~~~

Include the fixed ABI, exclusive consumer, queue-loss contract, nonblocking syscall, probe-only launch convention, and explicit non-goals from the approved spec.

- [ ] **Step 6: Run shared tests and compatibility checks**

Run:

~~~powershell
cargo test -p pythos-shared -- --quiet
cargo test -p pythos-shared --features pyth-tig-test-support -- --quiet
cargo test -p pythos-user-shell -- --quiet
cargo test -p pythos-user-pyth-runtime -- --quiet
~~~

Expected GREEN: ABI tests pass and existing imports compile through the re-export.

- [ ] **Step 7: Commit the ABI checkpoint**

~~~powershell
git add shared/src/capability_abi.rs shared/src/session_input_abi.rs shared/src/object_shell_abi.rs shared/src/lib.rs shared/src/user_program_manifest.rs docs/decisions/0090-session-input-bridge.md
git commit -m "feat: define session input abi"
~~~

---

### Task 2: Extract the Sequence-Stamped Device-Neutral Input Queue

**Files:**
- Create: `core/src/session_input.rs`
- Modify: `core/src/input_events.rs`
- Modify: `core/src/ps2.rs`
- Modify: `core/src/launcher_screen.rs`
- Modify: `core/src/main.rs`

**Interfaces:**
- Produces: `session_input::publish(raw: RawInputEvent)` for IRQ-safe producers.
- Produces: `bind_session_consumer_quiescent(holder: ServiceId)` for one pre-init binding.
- Produces: `try_read_session(holder: ServiceId) -> Result<Option<SessionInputEventV1>, SessionInputError>`.
- Produces: `try_read_compatibility() -> Result<Option<RawInputEvent>, SessionInputError>`.
- Removes: PS/2-owned queue and `ps2::poll_event()`.

- [ ] **Step 1: Write failing queue and conversion tests**

In the new module, specify these behaviors with an instance-local queue so tests do not mutate the production static:

~~~rust
#[test]
fn fifteen_unread_entries_are_preserved_and_newest_is_dropped() {
    let queue = SessionInputQueue::new();
    for ordinal in 0..15 {
        assert_eq!(queue.publish(mouse(ordinal)), PublishOutcome::Enqueued);
    }
    assert_eq!(queue.publish(mouse(99)), PublishOutcome::DroppedNewest);
    for ordinal in 0..15 {
        assert_eq!(queue.try_read_compatibility().unwrap(), Some(mouse(ordinal)));
    }
}

#[test]
fn dropped_candidate_consumes_sequence_and_sets_gap_before() {
    let queue = SessionInputQueue::new();
    let holder = ServiceId::from_raw(7);
    queue.bind_session_consumer_quiescent(holder).unwrap();
    for ordinal in 0..15 { queue.publish(mouse(ordinal)); }
    queue.publish(mouse(99));
    for _ in 0..15 { queue.try_read_session(holder).unwrap().unwrap(); }
    queue.publish(mouse(100));
    let after_gap = queue.try_read_session(holder).unwrap().unwrap();
    assert_eq!(after_gap.flags, SESSION_INPUT_FLAG_GAP_BEFORE);
}

#[test]
fn bind_flushes_stale_events_and_denies_compatibility_consumer() {
    let queue = SessionInputQueue::new();
    queue.publish(key(KeyCode::A));
    queue.bind_session_consumer_quiescent(ServiceId::from_raw(9)).unwrap();
    assert_eq!(queue.try_read_session(ServiceId::from_raw(9)), Ok(None));
    assert_eq!(queue.try_read_compatibility(), Err(SessionInputError::SessionBound));
}
~~~

Also test wrong holder, second bind denial, wrapping continuity across `u64::MAX`, and every `KeyCode`/current `InputEventKind` wire mapping including zero reserved fields.

- [ ] **Step 2: Run focused tests and verify RED**

~~~powershell
cargo test -p pythos-core session_input -- --nocapture
~~~

Expected RED: `session_input` and its queue contracts do not exist.

- [ ] **Step 3: Implement the fixed queue and exclusive modes**

Use a 16-slot SPSC ring and explicit consumer state:

~~~rust
const QUEUE_CAPACITY: usize = 16;

#[derive(Clone, Copy)]
struct SequencedRawInputEvent {
    sequence: u64,
    raw: RawInputEvent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConsumerMode {
    Compatibility,
    Session { holder: ServiceId, expected: u64 },
}
~~~

`publish` obtains `sequence = next_sequence.fetch_add(1, Ordering::Relaxed)` before the full check. Full means `next_tail == head`; return `DroppedNewest` without overwriting. `bind_session_consumer_quiescent` sets `head = tail`, samples the producer's next sequence as `expected`, then switches mode. Document and test that production calls it before `ps2::initialize()`; do not add a live transition or production reset.

On session dequeue, compare `event.sequence` with `expected` using wrapping equality, set `GAP_BEFORE` on mismatch, and set the next expected value to `event.sequence.wrapping_add(1)`.

- [ ] **Step 4: Convert normalized events to the wire ABI outside IRQ context**

Keep `input_events::normalize(raw)` authoritative, then exhaustively map `KeyCode` to shared numeric tags. Construct:

~~~rust
SessionInputEventV1 {
    sequence,
    kind,
    source,
    flags,
    value0,
    value1,
    reserved0: 0,
    reserved1: 0,
}
~~~

For `PointerButton { left }`, emit kind `MouseButtonState`, source Mouse, `value0` 0/1, and no click semantics. Change `input_events.rs` to construct its `ResourceId` from the shared `SESSION_INPUT_RESOURCE_ID`; preserve `InputEventService` and its Phase 5 proof.

- [ ] **Step 5: Rewire PS/2 publication and compatibility consumption**

Replace every `QUEUE.push(...)` in the keyboard handler and mouse assembler with `session_input::publish(...)`. Delete the queue implementation and `ps2::poll_event`. Update `launcher_screen::run_until_click` to call `session_input::try_read_compatibility()` while preserving the same event matching, busy-poll behavior, marker order, and failure behavior.

Do not call normalization, capability lookup, syscall dispatch, control interpretation, or rendering from either IRQ handler.

- [ ] **Step 6: Run queue, PS/2, and launcher regressions**

~~~powershell
cargo test -p pythos-core session_input -- --quiet
cargo test -p pythos-core ps2 -- --quiet
cargo test -p pythos-core launcher_screen -- --quiet
cargo build -p pythos-core --target x86_64-unknown-none
~~~

Expected GREEN: queue contracts pass, the cross-target build succeeds, and default launcher semantics remain intact.

- [ ] **Step 7: Commit the mechanism extraction**

~~~powershell
git add core/src/session_input.rs core/src/input_events.rs core/src/ps2.rs core/src/launcher_screen.rs core/src/main.rs
git commit -m "refactor: extract device neutral input queue"
~~~

---

### Task 3: Add the Capability-Gated Nonblocking Receive Syscall

**Files:**
- Modify: `core/src/syscall.rs`

**Interfaces:**
- Changes: `SYSCALL_ABI_MINOR` from 0 to 1.
- Adds: registry entry `SYSCALL_SESSION_INPUT_TRY_READ`, introduced in 1.1.
- Produces: `bind_session_input_capability(process: ActiveUserProcess) -> Result<PackedCapability, SyscallError>` for the pre-PS/2 probe setup.
- Produces: validation-before-dequeue `SessionInputEventV1` copy-out.

- [ ] **Step 1: Write failing registry and authorization tests**

Add tests proving:

~~~rust
#[test]
fn session_input_syscall_is_registry_version_1_1() {
    assert_eq!(SYSCALL_ABI_MAJOR, 1);
    assert_eq!(SYSCALL_ABI_MINOR, 1);
    let entry = lookup_syscall(SYSCALL_SESSION_INPUT_TRY_READ).unwrap();
    assert_eq!((entry.introduced_major, entry.introduced_minor), (1, 1));
    assert!(!entry.proof_only);
}
~~~

Add table-driven tests for missing caller, forged slot, stale generation, wrong holder, wrong resource, and missing INPUT right. Each test must preload one event and assert the same event remains after denial.

- [ ] **Step 2: Write failing ABI-shape and copy-out tests**

Cover the required order with one sentinel output record:

~~~rust
let sentinel = SessionInputEventV1 {
    sequence: 0xAAAA_AAAA_AAAA_AAAA,
    kind: 0xBBBB,
    source: 0xCCCC,
    flags: 0xDDDD_DDDD,
    value0: -11,
    value1: -22,
    reserved0: u64::MAX,
    reserved1: u64::MAX,
};
~~~

For wrong length, nonzero `arg3`, nonzero `arg4`, misalignment, read-only mapping, and out-of-map pointer, assert dispatch returns an error, the sentinel bytes are unchanged, and the queued event remains. For a valid empty call, assert `SESSION_INPUT_RESULT_EMPTY` and unchanged sentinel. For a valid queued call, assert `SESSION_INPUT_RESULT_EVENT` and the exact copied record.

- [ ] **Step 3: Run focused tests and verify RED**

~~~powershell
cargo test -p pythos-core session_input_syscall -- --nocapture
cargo test -p pythos-core session_input_denial -- --nocapture
~~~

Expected RED: no registry entry, grant, or dispatch exists and the ABI is still 1.0.

- [ ] **Step 4: Add the registry entry and transactional exclusive grant**

Add `SessionInputTryRead` to `SyscallDispatchKind` and this sorted table entry:

~~~rust
SyscallEntry {
    number: SYSCALL_SESSION_INPUT_TRY_READ,
    name: "SYSCALL_SESSION_INPUT_TRY_READ",
    introduced_major: 1,
    introduced_minor: 1,
    proof_only: false,
    dispatch_kind: SyscallDispatchKind::SessionInputTryRead,
},
~~~

`bind_session_input_capability` must grant resource `SESSION_INPUT_RESOURCE_ID` with `RightsMask::INPUT`, then bind the session queue to the process's `ServiceId`. If binding fails, revoke the just-granted handle before returning the error. This function is only called while the producer is quiescent. Add `SessionInput(SessionInputError)` to `SyscallError` rather than collapsing mechanism errors into unrelated capability errors.

- [ ] **Step 5: Implement validation before dequeue**

The dispatch implementation order is literal:

~~~rust
let caller = process_context::current_caller()?;
validate_syscall_capability(caller, PackedCapability::from_raw(args.arg0), input_resource, input_right)?;
if args.arg2 != size_of::<SessionInputEventV1>() as u64 { return Err(SyscallError::BadResult); }
if args.arg3 != 0 || args.arg4 != 0 { return Err(SyscallError::BadResult); }
validate_user_buffer(&caller.copy_map(), args.arg1, args.arg2, align_of::<SessionInputEventV1>(), UserCopyAccess::Write)?;
let Some(event) = session_input::try_read_session(caller.service_id())? else {
    return Ok(SESSION_INPUT_RESULT_EMPTY);
};
unsafe { (args.arg1 as *mut SessionInputEventV1).write(event); }
Ok(SESSION_INPUT_RESULT_EVENT)
~~~

Use the repository's full eight-point unsafe-comment format on the one copy-out. Do not write a zero/default record on `EMPTY` or error.

- [ ] **Step 6: Run syscall, capability, and user-copy tests**

~~~powershell
cargo test -p pythos-core syscall -- --quiet
cargo test -p pythos-core capabilities -- --quiet
cargo test -p pythos-core user_copy -- --quiet
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify -- -D warnings
~~~

Expected GREEN: all authorization/copy tests pass, existing syscall entries remain stable, and strict cross-target Clippy reports no warnings.

- [ ] **Step 7: Commit the syscall boundary**

~~~powershell
git add core/src/syscall.rs
git commit -m "feat: deliver session input through capability syscall"
~~~

---

### Task 4: Add a Finite Isolated Ring-3 Launch with Two Probe Arguments

**Files:**
- Modify: `core/src/process_context.rs`
- Modify: `core/src/user_mode.rs`

**Interfaces:**
- Produces: `ActiveUserProcess::from_user_elf_launch(...)` for ELF-plus-stack copy maps.
- Produces: `run_dynamic_process_breakpoint_test(process, entry, user_stack_top, arg0, arg1)`.
- Preserves: `ring3_enter_abi`, `ring3_enter_forever_abi`, persistent process kinds, and all existing proof entry behavior.

- [ ] **Step 1: Write failing process copy-map tests**

Add a test that builds a minimal ELF and guarded stack, calls the new constructor, and proves executable ELF read, writable data, writable stack, rejected code write, and rejected unrelated addresses. Assert no bootstrap page is admitted.

- [ ] **Step 2: Run focused tests and verify RED**

~~~powershell
cargo test -p pythos-core user_elf_launch_copy_map -- --nocapture
~~~

Expected RED: the constructor and copy-map helper do not exist.

- [ ] **Step 3: Implement the ELF-plus-stack copy map**

Factor the already accepted ELF-segment loop into a private helper reused by `copy_map_from_validated_launch` and the new `copy_map_from_user_elf_launch`. The new path maps only validated ELF segments and the selected guarded stack; it does not fabricate a bootstrap mapping.

- [ ] **Step 4: Add a separate finite entry assembly symbol**

Do not change either existing symbol. Add `ring3_enter_with_args_abi(entry, stack, arg0, arg1)` that preserves kernel callee-saved registers, saves `arg0`/`arg1` across `prepare_ring3_return_abi`, then places them in user `RDI`/`RSI` immediately before `iretq`. Its recovery label must restore kernel segments/registers and return 1 exactly like `ring3_enter_abi`.

The Rust wrapper must:

1. bind `ActiveUserProcess`;
2. arm only the expected breakpoint;
3. set the ring-0 trap stack;
4. enter through the new assembly symbol;
5. clear the current process on every returned result;
6. return success only when breakpoint recovery set `USER_RETURNED` and assembly returned 1.

This is not a new persistent process kind and must not alter fault containment for the shell or Pyth runtimes.

- [ ] **Step 5: Run ring-3 regressions and cross-target checks**

~~~powershell
cargo test -p pythos-core process_context -- --quiet
cargo test -p pythos-core user_mode -- --quiet
cargo build -p pythos-core --target x86_64-unknown-none --features verify
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify -- -D warnings
~~~

Expected GREEN: host tests and existing verify entry paths pass; no persistent ABI or caller binding regresses.

- [ ] **Step 6: Commit the finite launch primitive**

~~~powershell
git add core/src/process_context.rs core/src/user_mode.rs
git commit -m "feat: add finite authorized ring3 probe launch"
~~~

---

### Task 5: Build the Minimal Ring-3 Evidence Consumer and Opt-In Image Record

**Files:**
- Create: `user/probes/session-input/Cargo.toml`
- Create: `user/probes/session-input/linker.ld`
- Create: `user/probes/session-input/src/lib.rs`
- Create: `user/probes/session-input/src/main.rs`
- Create: `user/probes/session-input/src/syscalls.rs`
- Modify: `Cargo.toml`
- Create: `scripts/build-session-input-probe.py`
- Modify: `scripts/verify-user-elf.py`
- Modify: `scripts/build-image.py`
- Modify: `tests/test_verify_user_elf.py`
- Modify: `tests/test_build_orchestration.py`

**Interfaces:**
- Probe entry: `_start(input_raw: u64, console_raw: u64) -> !`, immediately converted to `PackedCapability` values inside the probe.
- Host-testable state: exact five-event validator with wrapping contiguous sequence checks and no activation interpretation.
- Packaging option: `--session-input-probe-elf <path>`.

- [ ] **Step 1: Write failing probe-state tests**

In `src/lib.rs`, test the exact expected values:

~~~rust
const EXPECTED: [ExpectedEvent; 5] = [
    ExpectedEvent::key(KEY_SPACE),
    ExpectedEvent::key(KEY_SPACE),
    ExpectedEvent::key(KEY_BACKSPACE),
    ExpectedEvent::key(KEY_BACKSPACE),
    ExpectedEvent::motion(7, -7),
];
~~~

Tests must reject swapped keys, a button event, zero motion, wrong motion values, nonzero reserved fields, unknown flags, `GAP_BEFORE`, duplicate/skip sequence numbers, and a sixth event. Test wrapping continuity separately. The validator may name events by ordinal but must not contain `ActivateCursorFeature`, `ViewingState`, Traversal, focus, or sequence-recognizer state.

- [ ] **Step 2: Write failing build/packaging tests**

Extend Python tests to require:

- arbitrary `--elf` verification preserves the default shell behavior;
- the probe build uses its own linker and isolated `--target-dir` when supplied;
- probe ELF verification occurs before `build-image.py`;
- the opt-in named record has the exact probe name/principal/digest;
- omitting `--session-input-probe-elf` leaves the default record set unchanged.

Run and confirm RED:

~~~powershell
cargo test -p pythos-user-session-input-probe -- --nocapture
py -3 -m unittest tests.test_verify_user_elf tests.test_build_orchestration
~~~

- [ ] **Step 3: Implement the host-testable validator**

Use only `pythos_shared::session_input_abi` types/constants. The state stores `next_ordinal` and `next_sequence: Option<u64>`. It accepts exactly the five records above, requires `flags == 0`, requires both reserved fields zero, and advances expected sequence with `wrapping_add(1)`.

- [ ] **Step 4: Implement the ring-3 syscall and evidence loop**

Reuse the shell's accepted raw syscall register convention in the probe-local `syscalls.rs`. `try_read` passes:

~~~text
RAX = SYSCALL_SESSION_INPUT_TRY_READ
RDI = input capability raw value
RSI = output pointer
RDX = 40
R10 = 0
R8  = 0
~~~

The non-test `_start` must:

1. emit `PYTHOS:SESSION_INPUT_PROBE:READY_FOR_INPUT` on COM2;
2. poll console read until the harness sends byte `G` after QMP injection;
3. fill an aligned output record with sentinel bytes;
4. call `try_read` with a forged generation and require a dispatch error plus unchanged sentinel;
5. emit `PYTHOS:SESSION_INPUT_PROBE:FORGED_DENIED_OUTPUT_UNCHANGED`;
6. poll the real input call, treating `EMPTY` as retry;
7. validate and acknowledge the five events with fixed markers `EVENT_1_SPACE` through `EVENT_5_RELATIVE_MOTION`;
8. emit `CONTIGUOUS` and `READY` only after the validator is complete;
9. execute `int3` only on success.

Any unexpected status/event emits one `PYTHOS:SESSION_INPUT_PROBE:ERROR` marker and spins forever. The probe does not issue an activation command and imports no Viewing code.

- [ ] **Step 5: Add build, verification, and opt-in packaging**

The build script mirrors the accepted static-user-ELF flags but points at the probe linker. Generalize `verify-user-elf.py` with `--elf`, defaulting to the existing shell ELF, and keep `USER_ELF_VERIFY_OK` stable.

Add the probe named record only when the explicit option is supplied:

~~~python
if session_input_probe_elf is not None:
    records.append(
        (
            INIT_BUNDLE_NAMED_USER_ELF_TYPE,
            build_named_user_program(
                b"session-input-probe.elf",
                SESSION_INPUT_PROBE_PRINCIPAL_ID,
                require_file(session_input_probe_elf, "session input probe ELF"),
            ),
        )
    )
~~~

Do not add a probe to normal images implicitly and do not change INIT.PAK formats.

- [ ] **Step 6: Build and verify the probe ELF**

~~~powershell
py -3 scripts/build-session-input-probe.py --target-dir target/session-input-bridge-probe
py -3 scripts/verify-user-elf.py --elf target/session-input-bridge-probe/x86_64-unknown-none/debug/pythos-user-session-input-probe
cargo test -p pythos-user-session-input-probe -- --quiet
py -3 -m unittest tests.test_verify_user_elf tests.test_build_orchestration
~~~

Expected GREEN: the static ELF passes the real-shape verifier, validator tests pass, and opt-in packaging tests preserve normal images.

- [ ] **Step 7: Commit the user probe and packaging**

~~~powershell
git add Cargo.toml user/probes/session-input scripts/build-session-input-probe.py scripts/verify-user-elf.py scripts/build-image.py tests/test_verify_user_elf.py tests/test_build_orchestration.py
git commit -m "test: add ring3 session input consumer"
~~~

---

### Task 6: Add the Dual-Channel Oracle Before Kernel Probe Orchestration

**Files:**
- Modify: `scripts/launcher_click.py`
- Modify: `tests/test_qemu_marker_actions.py`
- Create: `scripts/test-session-input-bridge-probe.py`
- Create: `tests/test_session_input_bridge_boundary.py`

**Interfaces:**
- Produces: `send_relative_mouse_motion(dx, dy)` and `type_session_input_bridge_sequence()`.
- Produces: strict COM1/COM2 assertion functions and `--self-test`.
- Produces: architecture guard for Slice 1 exclusions.

- [ ] **Step 1: Test the exact QMP injection helpers**

Patch the socket helper in `tests/test_qemu_marker_actions.py` and assert the Slice 1 helper sends four press/release qcodes in order:

~~~text
spc, spc, backspace, backspace
~~~

Then assert one `input-send-event` contains exactly relative x `+7` and relative y `-7`, with no button event and no USB/xHCI hotplug.

- [ ] **Step 2: Write oracle self-tests first**

`SessionInputBridgeOracleSelfTest` must build valid synthetic COM1/COM2 transcripts, then prove failure for each mutation:

- missing, duplicated, or out-of-order marker;
- kernel-only transcript with no COM2 evidence;
- COM2-only transcript with no kernel path;
- wrong key or motion value marker;
- missing forged-denial or sentinel-preservation marker;
- missing continuity marker;
- any `GAP`, `ERROR`, `PYTHOS:PANIC`, or disk-write marker;
- timeout or missing/excess `QEMU_OUTCOME success`;
- terminal COM1 readiness before ring-3 return;
- terminal COM2 readiness before event 5.

Run:

~~~powershell
py -3 scripts/test-session-input-bridge-probe.py --self-test
py -3 -m unittest tests.test_qemu_marker_actions tests.test_session_input_bridge_boundary
~~~

Expected RED: the new harness/helpers do not exist.

- [ ] **Step 3: Implement the strict dual-channel oracle**

Require COM1, in order and exactly once:

~~~text
PYTHOS:CORE:SESSION_INPUT_BRIDGE:COM2_READY
PYTHOS:CORE:SESSION_INPUT_BRIDGE:STREAM_BOUND
PYTHOS:CORE:SESSION_INPUT_BRIDGE:PS2_READY
PYTHOS:CORE:SESSION_INPUT_BRIDGE:RING3_ENTER
PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED
PYTHOS:CORE:PS2:MOUSE_IRQ_FIRED
PYTHOS:CORE:SESSION_INPUT_BRIDGE:RING3_RETURN
PYTHOS:CORE:SESSION_INPUT_BRIDGE:NO_DISK_WRITES
PYTHOS:CORE:SESSION_INPUT_BRIDGE:READY
~~~

Require COM2, in order and exactly once:

~~~text
PYTHOS:SESSION_INPUT_PROBE:READY_FOR_INPUT
PYTHOS:SESSION_INPUT_PROBE:FORGED_DENIED_OUTPUT_UNCHANGED
PYTHOS:SESSION_INPUT_PROBE:EVENT_1_SPACE
PYTHOS:SESSION_INPUT_PROBE:EVENT_2_SPACE
PYTHOS:SESSION_INPUT_PROBE:EVENT_3_BACKSPACE
PYTHOS:SESSION_INPUT_PROBE:EVENT_4_BACKSPACE
PYTHOS:SESSION_INPUT_PROBE:EVENT_5_RELATIVE_MOTION_DX_7_DY_NEG_7
PYTHOS:SESSION_INPUT_PROBE:CONTIGUOUS
PYTHOS:SESSION_INPUT_PROBE:READY
~~~

The live harness starts `run-qemu.py` with `--shell-port`, connects COM2 before waiting for kernel readiness, reads `READY_FOR_INPUT`, injects QMP input, waits a short bounded drain interval after QMP acknowledgements, sends `G`, collects COM2 through its terminal marker, and then requires the runner to exit with exactly one `QEMU_OUTCOME success`. Reuse the existing platform-specific process-tree cleanup pattern.

Its build phase must use this exact artifact order and an isolated probe target directory:

~~~powershell
cargo build -p pythos-boot --target x86_64-unknown-uefi
cargo build -p pythos-core --target x86_64-unknown-none --target-dir target/session-input-bridge-probe --features session-input-bridge-probe
py -3 scripts/build-user-shell.py
py -3 scripts/verify-user-elf.py
py -3 scripts/build-session-input-probe.py --target-dir target/session-input-bridge-probe
py -3 scripts/verify-user-elf.py --elf target/session-input-bridge-probe/x86_64-unknown-none/debug/pythos-user-session-input-probe
py -3 scripts/build-image.py --kernel target/session-input-bridge-probe/x86_64-unknown-none/debug/pythcore --session-input-probe-elf target/session-input-bridge-probe/x86_64-unknown-none/debug/pythos-user-session-input-probe
~~~

- [ ] **Step 4: Add explicit source-boundary guards**

The new boundary test must assert:

- `session-input-bridge-probe` depends only on `verify`;
- `session_input.rs`, `session_input_probe.rs`, and the probe crate do not mention/import `viewing`, `session_controls`, `FocusMark`, `framebuffer`, Project Hall, or Task Hall;
- no xHCI/USB feature or module is referenced by the probe path;
- the user probe contains no activation command or sequence-recognizer symbol.

Use source guards only for forbidden dependency direction; behavior remains proven by Rust and QEMU tests.

- [ ] **Step 5: Run self-tests and confirm oracle GREEN but live proof RED**

~~~powershell
py -3 scripts/test-session-input-bridge-probe.py --self-test
py -3 -m unittest tests.test_qemu_marker_actions tests.test_session_input_bridge_boundary
py -3 scripts/test-session-input-bridge-probe.py
~~~

Expected: oracle/unit tests GREEN. Live QEMU RED because PythCore has not yet composed or launched the probe and cannot emit terminal markers. This is the integration red gate.

- [ ] **Step 6: Commit the red integration oracle**

~~~powershell
git add scripts/launcher_click.py scripts/test-session-input-bridge-probe.py tests/test_qemu_marker_actions.py tests/test_session_input_bridge_boundary.py
git commit -m "test: specify ring3 session input acceptance"
~~~

---

### Task 7: Compose the Opt-In Kernel Probe Without Viewing Behavior

**Files:**
- Create: `core/src/session_input_probe.rs`
- Modify: `core/src/main.rs`
- Modify: `core/src/syscall.rs`
- Modify: `core/src/serial.rs`
- Modify: `core/Cargo.toml`

**Interfaces:**
- Feature: `session-input-bridge-probe = ["verify"]`.
- Kernel entry: `session_input_probe::run(boot_info, physical_memory, kernel_address_space) -> Result<(), SessionInputProbeError>`.
- Terminal readiness: emitted only after successful ring-3 breakpoint return and kernel-root restoration.

- [ ] **Step 1: Add feature and module gates only**

Register:

~~~toml
# Opt-in Phase 13.5 Slice 1 proof of capability-gated recurring PS/2 input
# delivery to a finite ring-3 consumer. No Viewing or default-boot cutover.
session-input-bridge-probe = ["verify"]
~~~

Gate `session_input_probe` under `#[cfg(all(not(test), feature = "session-input-bridge-probe"))]`. Extend the existing COM2 and console-capability cfgs only enough for this feature; do not alter their normal-boot behavior.

- [ ] **Step 2: Implement ordered setup with a quiescent producer**

`run` must perform these operations in order:

1. initialize COM2 and emit `COM2_READY` on COM1;
2. load `session-input-probe.elf` via `runtime_loader::load_named_user_program`;
3. require its manifest principal equals `SESSION_INPUT_PROBE_PRINCIPAL_ID`;
4. validate the ELF and build a retained isolated user address space with `build_with_user_elf`;
5. construct `ActiveUserProcess::from_user_elf_launch` with the first guarded proof stack;
6. grant the probe-only COM2 capability;
7. transactionally bind/grant the exclusive session-input capability and emit `STREAM_BOUND`;
8. call `ps2::initialize()` and emit `PS2_READY`;
9. activate the probe user root and emit `RING3_ENTER` immediately before finite entry;
10. pass input capability raw value as user `RDI` and console capability raw value as user `RSI`;
11. after success-only breakpoint return, restore the kernel address space before emitting `RING3_RETURN`;
12. verify no current caller remains bound;
13. emit `NO_DISK_WRITES` and `READY`.

Every error returns a typed stage error to `main`, which emits `PYTHOS:PANIC` and exits failure. Do not fall back to compatibility dequeue, direct PS/2 reads, or kernel-side event inspection.

- [ ] **Step 3: Invoke only the opt-in branch**

In the existing `verify` block, before ordinary milestone terminal success, add a mutually exclusive feature block that calls the probe and then `qemu_exit::success()`. The normal `verify` path and default non-verify `normal_boot::run` remain unchanged when the feature is absent.

Do not compile or call `session_controls`, `viewing`, `viewing_input_probe`, or framebuffer focus rendering under the new feature.

- [ ] **Step 4: Run focused host and strict-build gates**

~~~powershell
cargo test -p pythos-core session_input -- --quiet
cargo test -p pythos-core syscall -- --quiet
cargo build -p pythos-core --target x86_64-unknown-none --features session-input-bridge-probe
cargo clippy -p pythos-core --target x86_64-unknown-none --features session-input-bridge-probe -- -D warnings
py -3 -m unittest tests.test_session_input_bridge_boundary
~~~

Expected GREEN: the feature builds lint-clean and the boundary guard proves it is independent of ADR 0089 implementation modules.

- [ ] **Step 5: Run the live QEMU proof and verify GREEN**

~~~powershell
py -3 scripts/test-session-input-bridge-probe.py
~~~

Expected GREEN: exact COM1 and COM2 contracts pass, the five events are continuous and gap-free, forged access does not consume/mutate, `NO_DISK_WRITES` is present, and the runner prints exactly one `QEMU_OUTCOME success` plus `SESSION_INPUT_BRIDGE_PROBE_TEST_OK`.

- [ ] **Step 6: Commit the opt-in composition**

~~~powershell
git add core/src/session_input_probe.rs core/src/main.rs core/src/syscall.rs core/src/serial.rs core/Cargo.toml
git commit -m "feat: prove capability gated ring3 input delivery"
~~~

---

### Task 8: CI, Documentation, Full Regression, and Evidence Promotion

**Files:**
- Modify: `.github/workflows/qemu-acceptance.yml`
- Modify: `docs/decisions/0090-session-input-bridge.md`
- Modify: `docs/PythOS-TDD-001.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Modify: `README.md`
- Modify: `D:\PythOS-Workspace\CURRENT-STATE.md`

**Interfaces:**
- CI runs the new script self-tests, strict feature lint, and live QEMU proof.
- ADR 0090 becomes `Accepted in QEMU; physical and production-session integration pending` only after fresh evidence.
- Documentation says Slice 1 is delivery capability, not Viewing behavior or default ownership.

- [ ] **Step 1: Add CI assertions before workflow edits**

Extend `tests/test_ci_workflow.py` to require:

- `scripts/build-session-input-probe.py` and `scripts/test-session-input-bridge-probe.py` in Python compilation checks;
- `cargo test -p pythos-user-session-input-probe` in Rust unit tests;
- strict Clippy for `--features session-input-bridge-probe`;
- `python scripts/test-session-input-bridge-probe.py --self-test` before live QEMU;
- `python scripts/test-session-input-bridge-probe.py` in milestone acceptance.

Run and confirm RED:

~~~powershell
py -3 -m unittest tests.test_ci_workflow
~~~

- [ ] **Step 2: Wire CI and document the accepted boundary**

Update the workflow without removing existing gates. In all docs, use this exact distinction:

~~~text
Phase 13.5 Slice 1 proves one capability-authorized ring-3 consumer can receive recurring normalized input from the emulated PS/2 IRQ path. ADR 0089 still defines Viewing semantics; Slice 1 does not activate, route, or render Viewing and does not cut over normal boot.
~~~

Document the syscall ABI, queue capacity/loss flag, opt-in command, dual-channel evidence, and non-claims. Promote ADR 0090 status only after the live proof passes.

- [ ] **Step 3: Run formatting, all host tests, and strict lint**

~~~powershell
cargo fmt --check
cargo test --workspace -- --quiet
py -3 -m unittest discover -s tests -p "test_*.py"
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify,sdhci-emmc-backend -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none --features viewing-input-probe -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none --features session-input-bridge-probe -- -D warnings
cargo clippy -p pythos-boot --target x86_64-unknown-uefi -- -D warnings
~~~

Expected GREEN: no warnings, no skipped workspace crate, and all Python oracle/boundary tests pass.

- [ ] **Step 4: Run the new acceptance twice from fresh artifacts**

~~~powershell
py -3 scripts/test-session-input-bridge-probe.py --self-test
py -3 scripts/test-session-input-bridge-probe.py
py -3 scripts/test-session-input-bridge-probe.py
~~~

Expected GREEN twice: both runs show exact dual-channel order/count, continuous five-event delivery, success-only terminal readiness, `NO_DISK_WRITES`, and `QEMU_OUTCOME success`.

- [ ] **Step 5: Run unchanged regression QEMU gates**

~~~powershell
py -3 scripts/test-boot.py --slice milestone-1 --timeout 60
py -3 scripts/test-normal-fast-boot.py
py -3 scripts/test-persistent-storage.py
py -3 scripts/test-viewing-input-probe.py --self-test
py -3 scripts/test-viewing-input-probe.py
~~~

Expected GREEN: milestone, normal boot, persistent storage, and ADR 0089 Viewing proof all retain their existing markers and outcomes. The Viewing probe passing is regression evidence only; it is not part of the new implementation.

- [ ] **Step 6: Review the final diff against the authority boundary**

Run:

~~~powershell
git diff --check
git diff --name-only 5f4d7e2e3b60acf818d361f097ffb57c5cb15208...HEAD
git status --short
~~~

Required review results:

- no changes under `core/src/viewing/`;
- no changes to `core/src/session_controls.rs` or `core/src/viewing_input_probe.rs`;
- no PythTIG v1, Session Manager, xHCI, or focus-renderer change;
- normal boot has no new session input binding;
- no placeholder/TODO/`unimplemented!()` remains;
- only the intended external `CURRENT-STATE.md` update is outside the repository.

- [ ] **Step 7: Commit documentation and CI**

~~~powershell
git add .github/workflows/qemu-acceptance.yml docs/decisions/0090-session-input-bridge.md docs/PythOS-TDD-001.md docs/TECHNICAL-OVERVIEW.md README.md tests/test_ci_workflow.py
git commit -m "docs: accept phase 13.5 session input bridge"
~~~

- [ ] **Step 8: Refresh the external checkpoint**

Update `D:\PythOS-Workspace\CURRENT-STATE.md` with branch, exact HEAD, commits, commands/results, COM1/COM2 log paths and hashes, ADR 0090 status, and explicit pending boundaries:

~~~text
not merged or pushed unless separately authorized
no USB/media write or deployment
no physical Lenovo evidence
no production USB/xHCI input path
no persistent Session Manager consumer
no ADR 0089 production routing or presentation bridge
no normal-boot cutover
~~~

- [ ] **Step 9: Request independent review before publication**

Use `superpowers:requesting-code-review` on the complete implementation diff. Treat findings as evidence to investigate; use `superpowers:receiving-code-review` before edits. Re-run every affected gate after corrections. Do not push, merge, or deploy as part of this task.

---

## Completion Criteria

- One versioned 40-byte session-input ABI exists with stable logical-key tags and general syscall ABI 1.1 registration.
- `PackedCapability` has a neutral shared home and all legacy imports remain compatible.
- PS/2 IRQ handlers publish only sequence-stamped raw events to a device-neutral bounded queue.
- Default launcher compatibility behavior remains unchanged when no session stream is bound.
- Exactly one authorized ring-3 holder can consume; forged, stale, wrong-holder, malformed, and unsafe calls cannot dequeue or mutate output.
- Queue loss is visible through `GAP_BEFORE`, including wrapping sequence continuity.
- A finite opt-in ring-3 consumer receives exact Space, Space, Backspace, Backspace, and relative motion `(7, -7)` through the syscall, with COM1+COM2 acceptance and no gap.
- The proof emits terminal readiness only after success-only ring-3 return and kernel address-space restoration.
- ADR 0089 implementation paths, Viewing behavior, presentation, PythTIG v1, xHCI, and default session ownership remain untouched.
- All host, lint, QEMU, normal-boot, persistent-storage, and ADR 0089 regression gates are fresh and green.
- Publication, USB deployment, and physical validation remain separately authorized actions.
