# Phase 13.5 Slice 2 Bounded Session Runtime Design

Date: 2026-09-07

Status: Proposed written design for owner review. The owner approved bounded
supervised reinvocation as the Slice 2 runtime direction on 2026-09-07. This
document does not yet authorize implementation, normal-boot cutover, USB
deployment, or physical-hardware claims.

## Goal

Establish one retained ring-3 root/session runtime that can consume the
ADR 0090 session-input stream and repeatedly execute the existing bounded
PythTIG Session Manager graph without changing the frozen PythTIG major-version
1 package or opcode ABI.

Slice 2 proves lifecycle and ownership only:

```text
one retained ring-3 session runtime
    -> one stable session identity and input capability
    -> one retained-session-lifetime, non-durable session state
    -> bounded Session Manager graph invocation
    -> clean graph return
    -> supervisor selects reinvocation
    -> bounded Session Manager graph invocation again
```

ADR 0089 remains the semantic authority for the later Viewing slice. Slice 2
does not recognize the cursor activation sequence, instantiate `ViewingState`,
route relative motion, or render a FocusMark.

## Current Repository Reality

The live repository has the necessary pieces, but not their retained owner:

- `programs/session-manager/main.pyth` consumes one typed `PythCommand` and
  returns. Its README assigns later commands to supervisor-driven graph
  reinvocation.
- `core/src/pyth_service_supervisor.rs` models clean reinvocation and
  recovery/halt decisions, but it is decision-state evidence rather than an
  executable service scheduler.
- `core/src/normal_boot.rs` verifies and admits the Session Manager package,
  simulates a successful exit for the supervisor proof, then enters the legacy
  Rust shell. It does not execute the admitted graph.
- `user/pyth-runtime` verifies its bootstrap, interprets one already-verified
  graph, reports one `GraphExitRecord`, and spins. `Interpreter::new` already
  clears all invocation-local values and host-result slots.
- `core/src/user_mode.rs` treats a Pyth runtime as one non-returning user
  process. Graph exit clears the active caller and enters permanent safe idle;
  there is no process-level relaunch loop.
- ADR 0090 provides an exclusive, capability-checked, nonblocking
  `SessionInputEventV1` receive syscall. Its queue holder and continuity state
  can survive multiple reads by the same stable `ServiceId`.
- `core/src/scheduler.rs` and `core/src/process.rs` remain bounded proof models.
  They do not provide a production blocked state, wait queue, IRQ wakeup path,
  process registry, or general user-process scheduler.
- `GraphSyscallHost::command_read` and `command_result_emit` are still denied
  in production. Their interpreter behavior is covered only by a test host.

Wrapping the existing kernel supervisor in a loop would therefore be false:
the first real graph launch cannot return to it. Making the graph itself loop
would conflict with its one-command contract and finite instruction budget.

## Selected Runtime Boundary

Slice 2 introduces a retained ring-3 **session runtime host**. The retained host
is the root/session execution container; the PythTIG Session Manager remains a
bounded semantic graph executed inside it.

```text
PythCore
├── validates the session-runtime ELF and Session Manager graph
├── creates one session process and stable ServiceId
├── grants one session-input capability
├── maps immutable graph/package/import data
├── maps one bounded session-runtime bootstrap
└── enters the retained ring-3 session runtime

ring-3 session runtime host
├── owns retained-session-lifetime SessionRuntimeState
├── receives normalized input through ADR 0090
├── owns invocation-local typed command/result slots
├── invokes the verified Session Manager graph with a fresh Interpreter
├── observes its GraphExitRecord
└── applies the bounded lifecycle decision
    ├── clean exit -> reset invocation-local state and reinvoke
    └── graph fault/budget exhaustion -> stop and request recovery
```

“Reinvocation” means constructing and executing a new bounded interpreter
invocation for the same immutable verified graph inside the retained session
process. It does not mean repeatedly destroying and reconstructing a user
address space for each event or command.

The existing generic `pyth-runtime.elf` remains a one-shot runtime with its
current `PythGraphBootstrapBlock` and graph-exit syscall behavior. Slice 2 does
not silently convert every Pyth runtime into a persistent service. The retained
session runtime is a separately named user program that reuses the interpreter
library and is admitted only by the opt-in Slice 2 profile.

This boundary preserves the existing one-command graph contract, keeps the
session identity and non-durable state stable, and avoids making PythCore the
owner of input interpretation or session semantics.

## Runtime Bootstrap Boundary

The retained session runtime needs a separately versioned bootstrap contract.
It must not resize, reinterpret, or silently extend `PythGraphBootstrapBlock`.

Conceptually, the new bootstrap supplies:

- the immutable Session Manager package address, length, digest, and bounded
  instruction budget;
- the validated graph import table;
- the session-input capability granted to the stable session identity;
- the session identity expected by the runtime;
- bounded addresses for invocation-local result/evidence storage; and
- reserved zero fields for explicit versioned extension.

The bootstrap points to two additional versioned mappings used only by the
opt-in acceptance profile:

- a read-only `SessionRuntimeFixtureV1` mapping containing exactly two
  validated `PythCommand` records and their bounded payload bytes; and
- a writable `SessionRuntimeResultV1` mapping containing the final host status,
  stable identities, input/event count, invocation count, last graph-exit
  record, and zeroed reserved fields.

Every command payload pointer must resolve wholly inside the read-only fixture
payload region. The runtime rejects a wrong count, unknown command kind,
unsupported flags, nonzero reserved field, out-of-range pointer, oversized
payload, or overlapping writable result mapping before the first graph
invocation. The implementation plan will freeze the exact record offsets,
sizes, alignment, constants, and maximum payload length as part of this new
shared Slice 2 ABI.

The implementation plan must freeze exact layout, sizes, alignment, version,
reserved fields, and copy/mapping validation before code is written. The
bootstrap is a session-runtime launch ABI, not a PythTIG package-format or
opcode change. It contains no `ViewingState`, Project Hall identity, physical
device fields, framebuffer address, or persistence handle.

Three identities remain distinct:

- the named session-runtime ELF principal authenticates the retained host
  program;
- the Session Manager graph principal authenticates the immutable graph; and
- the kernel-assigned session `ServiceId` is the capability holder retained for
  the boot session.

The input queue binds the `ServiceId`, not either principal. The runtime must
validate all three expected identities at launch and must never present the
host-program principal as if it were the graph principal.

## State Ownership and Reset Rules

The retained host owns exactly one `SessionRuntimeState` for its retained
session lifetime. It is initialized from zero on every boot and is destroyed
when the opt-in host returns. Slice 2 state is deliberately neutral and
contains only lifecycle evidence such as:

- stable session identity;
- number of normalized input events consumed;
- graph invocation ordinal;
- prior clean graph-exit status; and
- whether recovery has been requested.

It is not stored in the object service, package context, journal, checkpoint,
or disk. A fresh boot starts it from zero. This is the future ownership site
for session-wide `ViewingState`, but Slice 2 must not add Viewing fields or
behavior.

Every graph invocation receives fresh value and host-result tables. The typed
command input, typed result output, and graph exit record are invocation-local
and are cleared or replaced before reinvocation. Immutable package bytes and
validated imports may be reused. A clean graph exit must not preserve graph
locals accidentally.

The session-input capability is bound once before producers start and remains
held by the same stable `ServiceId`. Reinvoking the graph must neither rebind
the input queue nor mint a replacement session identity.

## Typed Command Boundary

Raw `SessionInputEventV1` values are not `PythCommand` values. Slice 2 must not
invent a command kind for keyboard or mouse input and must not treat input
arrival as Project, task, hall, cursor, or Viewing policy.

The retained host supplies the existing Session Manager graph with a bounded
typed command source through the already-frozen PythTIG v1.1 `CommandRead` and
`CommandResultEmit` host operations. Capability validation remains explicit.
The host adapter may project only the fields already authorized by ADR 0068.
It may not parse human command text or fabricate new command semantics.

Slice 2 assigns one concrete session-command resource identity in the new
shared session-runtime ABI:

```text
SESSION_COMMAND_RESOURCE_ID = 0x5059_5345_5343_4D44  # "PYSESCMD"
```

PythCore grants one opaque handle for that resource to the retained session
`ServiceId` with distinct kernel `READ | APPEND` rights. Slice 2 adds `APPEND`
as a new kernel capability right rather than widening append-only command
results to the existing general `WRITE` right. Kernel capability-right bits and
PythTIG import-right bits remain separate numeric namespaces and are mapped
explicitly. The same handle is placed only in an
import whose verified resource kind is `RESOURCE_COMMAND` and whose declared
rights are exactly `READ | APPEND`.

Inside the retained process, a `SessionCommandHost` accepts only that exact
imported handle. `CommandRead` projects the current invocation's already
validated fixture record into the existing closed `HostCallResult`; one
`CommandResultEmit` fills the current invocation-local result slot. A second
read or emit, a wrong handle, or a result that does not match the current
command fails the invocation. `Interpreter::new` clears graph value and host
result tables before the next invocation, and the command adapter replaces its
current command/result slot explicitly.

This is the same logical-isolation level as the current trusted Pyth runtime:
PythCore authenticates and maps the host program, graph, imports, and fixture,
while the retained single-threaded host enforces its graph-facing adapter. It
is not claimed as hostile-code-secure nested isolation inside one process.

For the opt-in Slice 2 acceptance profile, two deterministic
`COMMAND_KIND_CREATE_NOTE` records with different bounded ASCII payloads are
test inputs supplied by the launch fixture. Their expected results echo the
matching payloads under the existing Session Manager behavior. They prove that
the same verified Session Manager graph can execute, return, reset, and execute
again. The two
normalized physical-input events independently prove that the retained
root/session host owns the ADR 0090 input capability and preserves neutral
session state. The acceptance oracle must not claim or imply an input-to-command
translation.

Normal production command ingress remains outside Slice 2. The Slice 2 host
adapter establishes the bounded graph-host seam; it does not replace a later
typed command source decision with a test fixture.

## Empty Input and Scheduling

ADR 0090 intentionally defines a nonblocking receive operation, and the live
repository has no accepted production sleep/wakeup scheduler. Slice 2 therefore
uses bounded polling only inside its finite opt-in acceptance run.

The retained runtime may retry `EMPTY` for a fixed test budget while waiting for
the two injected events. Exhausting that budget is a failed acceptance run. It
must not spin forever after success, claim efficient idle behavior, invoke input
from IRQ context, or add an undocumented blocking syscall.

A production wait/wakeup primitive remains a later explicit runtime decision.
Normal-boot cutover cannot occur until that decision is accepted. Slice 2 does
not build a general scheduler merely to prove retained ownership.

## Lifecycle and Failure Handling

The session runtime applies one shared pure lifecycle mapping without
duplicating Viewing or command policy:

- `GRAPH_EXIT_OK` permits the next bounded Session Manager invocation.
- budget exhaustion, runtime error, malformed host result, denied required
  command authority, or result mismatch maps to `RequestRecovery`; it must not
  reinvoke a fault loop.
- a native fault in the retained session-runtime process remains a PythCore
  user-process fault and follows the existing containment/recovery path.
- the second invocation cannot begin until the first `GraphExitRecord` has been
  validated and invocation-local state has been reset.
- no failure path writes storage, rebinds input, falls back to the compatibility
  consumer, or silently enters the legacy shell.

Slice 2 extracts the Session Manager graph-exit decision into a no-privilege
shared function used by both the retained host and PythCore's existing
`PythServiceSupervisor`:

```text
GraphExitRecord.status == GRAPH_EXIT_OK -> Reinvoke
all other graph exit statuses           -> RequestRecovery
```

The ring-3 host may select `Reinvoke` locally because that action creates only
a fresh in-process interpreter invocation. It may not enter a recovery shell or
halt the machine. PythCore retains those privileged actions and validates the
host's final result before choosing them.

The final host-to-kernel protocol is an acceptance-only versioned result page
plus the existing expected-breakpoint return continuation. The host writes
`SessionRuntimeResultV1` and executes `int3` exactly once. PythCore regains its
saved kernel continuation, restores the kernel address space, clears the caller,
and validates the complete result before emitting readiness. A failure record
returns through the same bounded continuation but suppresses readiness; a
native host fault follows existing user-fault containment and cannot pass.

This does not redefine the generic Pyth graph-exit syscall and does not claim a
production session shutdown/recovery protocol. That protocol, like efficient
wait/wakeup, remains required before normal-boot cutover.

## Opt-In Acceptance Profile

Slice 2 remains opt-in and finite. It does not replace normal boot.

The live QEMU proof uses the real PS/2 IRQ path and ADR 0090 syscall to deliver
two neutral events, for example one `A` key-down and one relative motion
`(7, -7)`. It also supplies two fixed typed Session Manager commands through
the acceptance fixture.

COM1 proves privileged setup and containment:

1. one session authority is created with a stable identity;
2. the input stream is bound exactly once before PS/2 initialization;
3. the retained session runtime enters ring 3;
4. invocation 1 returns cleanly;
5. the supervisor selects Session Manager reinvocation;
6. invocation 2 returns cleanly under the same session identity;
7. the runtime writes one versioned terminal result and returns through the
   expected-breakpoint continuation;
8. no second binding, compatibility fallback, panic, or disk write occurs.

COM2 independently proves ring-3 ownership:

1. session state starts at zero;
2. the first exact normalized input event is consumed;
3. invocation 1 receives the first exact typed command and emits its exact
   result;
4. neutral session state advances from zero to one;
5. invocation-local tables are reset;
6. invocation 2 observes retained session state one;
7. the second exact normalized input event is consumed;
8. invocation 2 receives the second exact typed command and emits its exact
   result;
9. neutral session state advances from one to two;
10. the same session identity is reported throughout.

A second fresh boot must again report initial state zero. Disposable attached
storage is hashed before and after both boots, and any change fails acceptance.

The oracle must reject missing, duplicate, malformed, or reordered lifecycle
markers; a changed session identity; a second successful input binding; state
reset between invocations; stale invocation-local results; a graph fault that
relaunches; `GAP_BEFORE` while claiming continuous delivery; kernel-only proof
without matching COM2 evidence; Viewing/cursor/focus/presentation markers;
panic; timeout; disk writes; or an extra/malformed `QEMU_OUTCOME`.

## Existing Infrastructure to Reuse

- ADR 0090's session-input ABI, exclusive binding, capability validation,
  queue continuity, and copy-out checks;
- `pyth_runtime_launch` package verification, immutable mappings, capability
  import grant rules, guarded stack construction, and launch validation;
- `pythos_user_pyth_runtime::interpreter` and its existing `Host` trait;
- `PythServiceSupervisor` clean-exit/fault action meanings;
- `scripts/test-session-input-bridge-probe.py` for QMP input injection, dual
  COM1/COM2 collection, strict timeline checks, and process-tree cleanup;
- `scripts/test-persistent-storage.py` only for its two-boot orchestration and
  before/after storage hash pattern.

The test harness must extract or reuse characterized helpers instead of adding
a third QEMU cleanup implementation.

## Approaches Rejected or Deferred

### Rejected: an endless Session Manager graph

This violates the graph's one-command contract, conflicts with its finite
instruction budget, and would require a new input/wait host operation.

### Rejected: kernel polls input and launches a process per event

This makes PythCore the session lifecycle driver for semantic work, requires
address-space reconstruction or reclamation for every event, and complicates
stable identity and non-durable state ownership.

### Rejected: persist session state as objects

ADR 0089 defines current Viewing/session state as non-durable. Object or
checkpoint persistence would create an unapproved restoration contract.

### Deferred: PythTIG v2 event and wait operations

A new package major version could eventually expose event-driven service
semantics, but that requires a separate ADR and coordinated format, verifier,
compiler, interpreter, native backend, and compatibility work. Slice 2 does
not need it.

### Deferred: production blocking wait/wakeup

The repository has no reusable production primitive. This remains mandatory
before normal-boot cutover but is not silently invented in Slice 2.

## Non-Goals

- `Space Space Backspace Backspace` recognition or any other control sequence.
- `ViewingState`, Traversal, Cursor/FocusMark, presentation, framebuffer, or
  Project/Task Hall behavior.
- Cursor deactivation, toggle, Escape, click-away, timeout, or persistence.
- Click, double-click, chord, drag, selection, wheel, scroll, or zoom semantics.
- Normal-boot replacement of the launcher or Rust shell.
- A PythTIG v1 package/opcode mutation or PythTIG v2 design.
- A general user-process scheduler, multi-process service daemon framework,
  blocking input syscall, SMP, multi-seat, unbind, or live consumer handoff.
- Production USB/xHCI/HID delivery, hubs, trackpads, hot-unplug recovery, or
  physical Lenovo acceptance.
- Durable session restoration or any disk write.

## Completion Boundary

Slice 2 is complete only when an opt-in QEMU profile proves that one retained
ring-3 session runtime, under one stable identity and one exclusive input
capability, consumes two recurring normalized input events, retains neutral
non-durable session state across two clean bounded executions of the admitted
Session Manager graph, resets all invocation-local state, contains faults, and
leaves attached storage unchanged across two fresh boots.

That evidence authorizes Slice 3 to bind ADR 0089's
`SessionControlInterpreter` and `ViewingState` to the retained session lifetime.
It does not itself prove Viewing, presentation, normal-boot cutover, production
wait/wakeup, USB input, or physical hardware.
