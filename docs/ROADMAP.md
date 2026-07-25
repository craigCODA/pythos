# PythOS Roadmap (Full Detail)

This file is the authoritative phase-by-phase build order for PythOS. It is
written for an executing agent, not as a pitch document. Every phase has a
locked slice sequence, a required serial marker exit condition, an explicit
scope boundary, and required documentation artifacts. Do not implement a phase
out of order. Do not implement a slice that is not listed. Do not implement
anything under "Parking Lot" or "Vision (Non-Binding)" under any circumstances
unless the phase gate for it has explicitly reopened.

If a slice's exit condition cannot be reached without touching a forbidden
area, stop and raise an ADR proposal instead of expanding scope silently.

## How To Read This File

Each phase has:

* Purpose - why this phase exists, one paragraph.
* Preconditions - what must already be true, verified rather than assumed, before starting.
* Locked slice sequence - ordered list of vertical slices. Each slice is one PR-sized unit: implement, test, verify serial or behavioral output, merge.
* Exit condition - the exact marker sequence or verifiable output required to call the phase complete.
* Scope boundary - an explicit list of things that must not be added during this phase, even if convenient.
* Required artifacts - ADRs, tests, docs that must exist before the phase is marked complete.
* Architectural test (non-binding) - a question to ask against the long-range vision docs under `docs/vision/`. Answering it is informational only. It never justifies adding vision-doc code to this phase.

## Phase 0: Reproducible Environment - COMPLETE

Repository, pinned toolchain, clean build, QEMU script, OVMF discovery, EFI
image builder, serial capture, automated smoke test, contributor instructions.

Exit condition: a clean checkout builds and launches the EFI loader.

## Phase 1: Boot Core Handoff - COMPLETE

UEFI loader, GOP discovery, ELF loading, `PythBootInfo`, memory-map handoff,
`ExitBootServices()`, PythCore entry, physical page ownership, bitmap
allocator, GDT, TSS, IDT, panic path, post-firmware framebuffer, serial
acceptance test.

Exit condition:

```text
PYTHOS:LOADER:ENTER
PYTHOS:LOADER:GOP_READY
PYTHOS:LOADER:KERNEL_LOADED
PYTHOS:LOADER:MEMORY_MAP_READY
PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK
PYTHOS:CORE:ENTER
PYTHOS:CORE:BOOTINFO_VALID
PYTHOS:CORE:MEMORY_READY
PYTHOS:CORE:GDT_READY
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

## Phase 1.5: Kernel-Owned Execution Substrate - IN PROGRESS

### Purpose

Replace every piece of transitional loader-owned execution state with
PythCore-owned infrastructure before any concurrency, scheduling, or IPC work
begins. Debugging a scheduler on top of transitional mappings or a
non-diagnostic panic loop wastes time that this phase exists to avoid.

### Preconditions

Phase 1 exit condition reproducible from a clean checkout.

### Locked Slice Sequence

Ordering note: this sequence is corrected against the verified serial log
(`PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY` fires before
`PYTHOS:CORE:VM_READY` in the actual boot capture). An earlier draft of this
roadmap listed these three slices in the wrong order twice: once as
`vm-ready` -> `identity-map-removed` -> `exceptions-diagnostic`, and once
corrected to `vm-ready` -> `exceptions-diagnostic` ->
`identity-map-removed`, which is closer but still wrong. The ground-truth
order, and the order a future agent should follow, is below. It also matches
the functional dependency: diagnostics should be live before the risky second
`CR3` switch is attempted, in case the switch itself faults.

1. `exceptions-diagnostic` - COMPLETE. Fault reports become actionable
   without relying on heap allocation or locks, both of which may be
   unavailable or suspect at fault time. At minimum, on any trap: vector
   number, error code where applicable, faulting `CR2` for page faults, and
   faulting `RIP` are written to serial before any halt or panic path
   executes. Emits `PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY` once installed.
   This slice is what makes the `identity-map-removed` slice's negative proof
   possible, and what makes the following `vm-ready` slice's own `CR3` switch
   diagnosable if it faults.

2. `vm-ready` - COMPLETE. PythCore builds final kernel-owned page tables,
   switches `CR3` a second time, keeps the first 2 MiB unmapped as a null
   guard, preserves W^X kernel mappings, keeps framebuffer and COM1 reachable,
   keeps `PythBootInfo` and memory map reachable, and retains a guarded active
   kernel stack. Emits `PYTHOS:CORE:VM_READY` only after post-switch
   validation through `validate_active`.

3. `identity-map-removed` (negative proof) - COMPLETE. Confirms the old broad
   loader identity range is untranslated via `translate_active`, arms the
   exception handler with an expected-recovery RIP, performs one controlled
   read against `OLD_IDENTITY_PROBE`, and only reports success if that specific
   page fault occurs and recovers at the pre-registered RIP. A generic
   panic-loop catch does not satisfy this slice; the proof must distinguish
   "the intended fault happened" from "something happened." Emits
   `PYTHOS:CORE:EXPECTED_PAGE_FAULT` then
   `PYTHOS:CORE:IDENTITY_MAP_REMOVED`.

4. `bootinfo-complete` - COMPLETE.
   * Loader, while it still has UEFI Boot Services access: discover and
     validate the ACPI RSDP via the UEFI configuration table lookup using the
     ACPI GUID, since RSDP discovery requires UEFI protocol access PythCore no
     longer has after `ExitBootServices()`. Pass the validated RSDP physical
     address through `PythBootInfo`.
   * PythCore: revalidate the RSDP signature and checksum independently. Do
     not trust the loader's validation blindly across the trust boundary.
     Select XSDT versus RSDT based on ACPI revision and availability, prefer
     XSDT when revision indicates 2.0 or later, then validate the chosen root
     system-description table's checksum before use.
   * Populate and validate SMBIOS entry point discovery with the same
     loader-discovers and PythCore-revalidates split as ACPI.
   * Replace `LocateProtocol()`-based filesystem discovery in the loader with
     resolution via the loaded image's own device handle:
     `EFI_LOADED_IMAGE_PROTOCOL` -> `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL` on the
     same device. The loader must read from the actual boot device instead of
     the first filesystem the firmware happens to expose.
   * Parse and validate the full `INIT.PAK` integrity boundary, not just the
     magic string. This is locked now because it becomes a durable ABI per the
     cross-phase standing rules. Required checks: magic
     `PYTHOS_INIT_PAK_V0`, declared header length, declared total length,
     actual payload length matches declared length, integer-overflow checks on
     every length field before use, checksum over the payload, reserved fields
     are zero, and supported major version. Reject nonzero reserved fields;
     that is a forward-compatibility signal, not something to silently ignore.
     Validate only in this slice; do not begin executing or interpreting
     payload contents, which is Phase 4 scope.
   * Emit `PYTHOS:CORE:BOOTINFO_COMPLETE` once ACPI, SMBIOS, boot-device
     filesystem path, and full `INIT.PAK` integrity validation all succeed.
   * Failure path: any missing or invalid table is a diagnosed panic through
     the Phase 1.5 exception path, not a silent skip.

5. `qemu-exit` - COMPLETE.
   * Replace timeout-based QEMU test termination in `run-qemu.py` and `tests/`
     with deterministic exit codes.
   * Required outcomes, each with a distinct exit code the test harness can
     assert on: success for full marker sequence observed, panic for
     `PYTHOS:PANIC` observed, reset for unexpected reboot or triple fault,
     timeout for no terminal marker within budget, and marker-order violation
     when a marker appears out of the required sequence, for example
     `FRAMEBUFFER_READY` before `VM_READY`.
   * Use QEMU's `isa-debug-exit` device, or an equivalent, so PythCore itself
     can request deterministic termination on the success path rather than the
     harness guessing from a timeout.
   * CI note: this is also the slice that unblocks running the QEMU acceptance
     suite in GitHub Actions instead of only locally. Add the workflow here if
     not already present.

### Exit Condition

Full Phase 1.5 marker sequence:

```text
PYTHOS:LOADER:ENTER
PYTHOS:LOADER:GOP_READY
PYTHOS:LOADER:KERNEL_LOADED
PYTHOS:LOADER:MEMORY_MAP_READY
PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK
PYTHOS:CORE:ENTER
PYTHOS:CORE:BOOTINFO_VALID
PYTHOS:CORE:MEMORY_READY
PYTHOS:CORE:GDT_READY
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY
PYTHOS:CORE:VM_READY
PYTHOS:CORE:EXPECTED_PAGE_FAULT
PYTHOS:CORE:IDENTITY_MAP_REMOVED
PYTHOS:CORE:BOOTINFO_COMPLETE
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

QEMU test harness also exits with a distinct, asserted code for each of
success, panic, reset, timeout, and marker-order violation.

### Scope Boundary

Do not add timer interrupts, scheduler, task structures, IPC, capabilities,
embedded Python, heap allocator beyond what `bootinfo-complete` strictly needs
for table parsing, networking, audio, storage, GUI, SMP, or any `docs/vision/`
concept. Prefer stack or static parsing first.

### Required Artifacts

* ADR for `bootinfo-complete`. Device-handle-based filesystem resolution is an
  ABI-relevant decision; record it following the pattern of
  `docs/decisions/0002-kernel-owned-page-tables.md`.
* ADR for `qemu-exit`. The exit-code contract is a test-infrastructure ABI.
* `AGENTS.md` Active Milestone section updated after each slice merges.
* CI workflow added or updated in the `qemu-exit` slice. Satisfied by
  `.github/workflows/qemu-acceptance.yml`, with
  `tests/test_ci_workflow.py` guarding the workflow contract.

### Architectural Test (Non-Binding)

None yet. Phase 1.5 predates any typed-object or capability concept in the
vision docs. The first applicable test is at Phase 3.

## Phase 2: Timer and Native Tasks - COMPLETE

### Purpose

PythCore cannot multitask. This phase proves preemptive concurrency between
trusted, statically known native tasks before any external or capability-scoped
code exists.

### Preconditions

Phase 1.5 exit condition reproducible, including `qemu-exit` deterministic
termination. This is needed for reliable scheduler test assertions because
flaky interleavings are exactly what nondeterministic QEMU termination would
mask.

### Locked Slice Sequence

0. `exception-entry-hardening` - COMPLETE. Prerequisite added ahead of
   `interrupt-controller`. The Phase 1.5 `exceptions-diagnostic` slice is
   correctly complete for its own purpose, diagnosing a controlled synchronous
   fault, but its entry path is narrower than what preemptive scheduling needs.
   Before any interrupt controller or timer work begins, harden exception and
   interrupt entry to guarantee full general-purpose register preservation,
   explicit stack alignment before calling into Rust, a normalized frame layout
   shared between exception and interrupt entry, restoration guarantees, and a
   clear structural distinction between exception entry and external-interrupt
   entry. SysV ABI requires 16-byte alignment at call boundaries, and an
   interrupt can land at any alignment. Exit condition: an exception can
   interrupt arbitrary Rust code, enter the handler with ABI-correct stack
   alignment, expose a complete saved register frame, and return without
   corrupting registers. Prove this with a test that deliberately interrupts a
   register-heavy computation and verifies every register survives intact.
   Emits `PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED` after the controlled `INT3`
   probe returns with all general-purpose registers intact. This does not
   reopen or regress the existing `exceptions-diagnostic` marker; it extends
   the entry path underneath it.

1. `interrupt-controller` - COMPLETE. Bring up APIC, or PIC if APIC is deferred,
   mask and unmask per vector, and route to the existing IDT. Emits
   `PYTHOS:CORE:INTERRUPTS_READY`.
2. `timer` - COMPLETE. Configure a hardware timer, APIC timer or PIT, to a fixed
   deterministic tick rate suitable for QEMU determinism. Emits
   `PYTHOS:CORE:TIMER_READY`.
3. `monotonic-clock` - COMPLETE. Expose a monotonic tick counter derived from
   the timer, immune to reprogramming by task code. Emits
   `PYTHOS:CORE:CLOCK_READY`.
4. `task-structures` - COMPLETE. Define the native task control block: saved
   register state, kernel stack pointer, task id, and state enum for ready,
   running, blocked, and terminated. No allocation beyond a fixed static pool
   at this slice. Emits `PYTHOS:CORE:TASKS_READY`.
5. `kernel-stacks` - COMPLETE. Guarded per-task kernel stacks using the
   guard-page pattern from Phase 1.5. Stack overflow must fault into the
   diagnostic exception path, not silently corrupt an adjacent task. Emits
   `PYTHOS:CORE:KERNEL_STACKS_READY` after proving the active bootstrap stack's
   guard page faults and recovers through the diagnostic path.
6. `context-switch` - COMPLETE. Save and restore register, stack, instruction
   pointer, and flags state across a cooperative switch. Prove correctness with
   two native contexts writing distinguishable alternating markers with no
   stack corruption. Emits `PYTHOS:CORE:CONTEXT_SWITCH_READY`.
7. `scheduler` - COMPLETE. Round-robin first. Priority scheduling is deferred
   by ADR 0007 and must not be built speculatively. Emits
   `PYTHOS:CORE:SCHEDULER_READY`.
8. `idle-task` - COMPLETE. A task runs only when no other task is ready. The
   proof observes the empty-ready-set path, switches through a fixed idle
   context without permanently halting the CPU, and emits
   `PYTHOS:CORE:IDLE_TASK_READY`.
9. `preemption` - COMPLETE. IRQ0 forces context switches between spin-only
   native contexts without voluntary yield points, producing alternating
   `PREEMPT:TASK_A` and `PREEMPT:TASK_B` markers before
   `PYTHOS:CORE:PREEMPT_READY`.
10. `task-termination` - COMPLETE. A fixed native task exits back to bootstrap,
    its static scheduler slot is marked terminated/reclaimable, and the
    round-robin selector verifies the terminated slot is no longer ready. Emits
    `PYTHOS:CORE:TASK_TERMINATION_READY`.
11. `scheduler-tests` - COMPLETE. Deterministic QEMU acceptance asserts
    interleaved, alternating serial markers from three native tasks across
    multiple timer-forced preemptions using the serial marker-order oracle.
    Emits `PYTHOS:CORE:SCHEDULER_TESTS_READY`.

### Exit Condition

Several native tasks print interleaved, deterministic serial markers under
preemptive scheduling with zero register or stack corruption, verified by
automated marker-order assertion, not manual log inspection.

### Scope Boundary

Do not add IPC, capabilities, any task communication beyond serial output,
Python, SMP, priority scheduling, or dynamic task spawning from untrusted
input. All tasks in this phase are statically defined at compile time.

### Required Artifacts

ADR for scheduler algorithm choice, round-robin, and ADR for task-control data
layout. The data layout becomes an ABI other phases depend on. Scheduler test
suite under `tests/`.

### Architectural Test (Non-Binding)

None yet.

## Phase 3: IPC and Capabilities - COMPLETE

### Purpose

PythOS is meant to be service-oriented. This is the phase that makes the
resident-agent and typed-object vision even theoretically possible later, but
nothing agent-related is built here. This phase only proves the mechanism:
controlled communication and controlled authority between native tasks.

### Preconditions

Phase 2 exit condition reproducible: preemptive multitasking with automated
scheduler tests passing in CI.

`service-identity` also requires TCB invariants to be formally specified before
implementation begins: slot reuse, `TaskId` stability, saved-frame offsets, and
kernel stack bounds. This is recorded in ADR 0008. The Phase 3 capability-token
and revocation decisions are recorded in ADR 0009 and ADR 0010.

### Locked Slice Sequence

1. `service-identity` - COMPLETE. Every task gains a stable, kernel-assigned
   identity distinct from its scheduler TCB slot. Slot reuse gets a fresh
   service identity, and stale identities no longer resolve. Emits
   `PYTHOS:CORE:SERVICE_IDENTITY_READY`.
2. `ipc-channels` - COMPLETE. Typed, bounded message channels between two
   known service identities. Fixed maximum message size and queue depth at
   this slice. No dynamic growth. Channel creation is a trusted
   kernel-internal bootstrap operation until the later `capability-handles` and
   `permission-validation` slices add authority checks. Emits
   `PYTHOS:CORE:IPC:SEND`, `PYTHOS:CORE:IPC:RECV`, and
   `PYTHOS:CORE:IPC_CHANNELS_READY` after an exact payload-integrity proof.
3. `bounded-queues` - COMPLETE. Backpressure behavior is defined and tested:
   this early in-kernel channel returns an explicit `QueueFull` error when the
   fixed queue is full and preserves already queued messages without silent
   drop. Emits `PYTHOS:CORE:IPC:QUEUE_FULL` and
   `PYTHOS:CORE:BOUNDED_QUEUES_READY`.
4. `request-reply` - COMPLETE. A synchronous request/reply pattern built on top
   of the fixed IPC channel: requester sends, responder replies, requester
   receives the exact matching reply, and missing replies return an explicit
   timeout result instead of hanging. Emits `PYTHOS:CORE:IPC:REQUEST`,
   `PYTHOS:CORE:IPC:REPLY`, `PYTHOS:CORE:IPC:REPLY_TIMEOUT`, and
   `PYTHOS:CORE:REQUEST_REPLY_READY`.
5. `capability-handles` - COMPLETE. The core primitive: an unforgeable handle
   naming a kernel-owned table slot and generation. Authority remains in the
   kernel table entry: holder, resource, rights, generation, and state.
   Capabilities are not ambient. A task with no handle cannot validate an
   operation by knowing the resource id. Emits `PYTHOS:CORE:CAPABILITY:GRANT`,
   `PYTHOS:CORE:CAPABILITY:USE`, and
   `PYTHOS:CORE:CAPABILITY_HANDLES_READY`.
6. `shared-memory-handles` - COMPLETE. A capability-gated shared memory region
   between services. A read-only grant allows reading the region, rejects
   writes with `MissingRights`, and leaves the region unchanged. Shared memory
   is never implicitly writable. Emits `PYTHOS:CORE:SHM:READ_ONLY`,
   `PYTHOS:CORE:SHM:WRITE_DENIED`, and
   `PYTHOS:CORE:SHARED_MEMORY_HANDLES_READY`.
7. `permission-validation` - COMPLETE. Privileged IPC send checks a capability
   handle before proceeding. A handle with `SEND` rights allows the send; a
   handle for the same holder/resource without `SEND` rights is denied before
   enqueue. No operation checks a task's identity as a substitute for holding
   the actual capability. Emits `PYTHOS:CORE:PERMISSION:IPC_ALLOWED`,
   `PYTHOS:CORE:PERMISSION:IPC_DENIED`, and
   `PYTHOS:CORE:PERMISSION_VALIDATION_READY`.
8. `revocation` - COMPLETE. A specific handle can be revoked without affecting
   the holder's other handles and without requiring holder cooperation.
   Revocation marks the entry revoked, bumps its generation, and stale handles
   fail validation. Emits `PYTHOS:CORE:CAPABILITY:REVOKE`,
   `PYTHOS:CORE:CAPABILITY:STALE_DENIED`, and
   `PYTHOS:CORE:REVOCATION_READY`.
9. `negative-authorization-tests` - COMPLETE. Required, not optional:
   automated tests prove a task is denied access when it has no capability,
   even when it knows the exact target resource and operation name. This is the
   load-bearing proof for the entire phase. Emits
   `PYTHOS:CORE:CAPABILITY:KNOWN_TARGET_DENIED` and
   `PYTHOS:CORE:NEGATIVE_AUTHORIZATION_READY`.
10. `audit-logging` - COMPLETE. Every capability check, grant, use, denial,
    and revocation exercised by the test suite is logged with service identity,
    resource, operation, and outcome. This is infrastructure-level audit
    logging, a kernel event stream, not the user-facing provenance or Causal
    Lens concept from the vision docs. Emits `PYTHOS:CORE:AUDIT:GRANT`,
    `PYTHOS:CORE:AUDIT:USE`, `PYTHOS:CORE:AUDIT:DENIAL`,
    `PYTHOS:CORE:AUDIT:REVOCATION`, `PYTHOS:CORE:AUDIT_LOGGING_READY`, and
    `PYTHOS:CORE:PHASE_3_COMPLETE`.

### Exit Condition

```text
A task without the required capability is denied even when it knows the
operation and target - proven by an automated negative test, not manual
inspection.
```

Audit log entries are emitted and inspectable in serial or debug output for
every grant, use, denial, and revocation exercised by the test suite.

### Scope Boundary

Do not add Python runtime, any external tool or MCP concept, any notion of
agent, workspace, proposal, or provenance graph from the vision docs. This
phase builds the mechanism only. Do not add networking; capabilities are
local, in-kernel, and single-machine at this phase.

### Required Artifacts

ADR for capability token representation. The unforgeability mechanism, for
example kernel-object-table index versus cryptographic token, is a foundational
security decision. ADR for revocation semantics. Negative-authorization test
suite is required, not optional; a phase without it is not complete regardless
of what else works.

### Architectural Test (Non-Binding)

When capabilities are built, ask whether they are strong enough to safely scope
an agent.

Ask this only after the phase's own exit condition, the negative-authorization
proof, passes. This is a design reflection, not a task. Do not add agent-shaped
code to answer it. Record the answer as a note in `docs/vision/patch.md` if
useful, not in `core/`.

## Phase 4: Python Runtime - COMPLETE

### Purpose

PythOS begins becoming Python-native. This is a narrow, capability-gated
embedding, not a general-purpose scripting free-for-all.

### Preconditions

Phase 3 exit condition reproducible, including the negative-authorization proof
and audit logging.

Required ADR before any Phase 4 code lands: record explicitly that PythOS
prototypes trusted services such as Python runtime, later GUI, and storage
services in kernel mode through Phase 7, then migrates them into
hardware-isolated address spaces during Phase 8. This is a deliberate
sequencing choice, not an oversight, and it has real cost that should be
recorded now rather than discovered as a surprise at Phase 8. Python runtime
calls will need to cross a syscall boundary, GUI and storage services will need
separate address spaces, IPC paths using trusted pointers will become copy-in
and copy-out or mapped shared memory, service failures that were kernel
failures become process failures, and every service-facing ABI defined in
Phases 4 through 7 will receive materially harsher validation requirements once
Phase 8 lands. Writing this down now means Phase 8's scope is a planned
migration, not a rediscovered one.

### Locked Slice Sequence

1. `runtime-selection` - COMPLETE. Evaluated RustPython, MicroPython, and a
   custom minimal interpreter with candidate evidence under
   `docs/research/runtime-selection/`. ADR 0012 records the kernel-mode
   prototype sequencing decision. ADR 0013 selects a custom minimal interpreter
   as the first embedded runtime because it gives the narrowest host-controlled
   boundary and lowest Phase 8 migration risk. This emits
   `PYTHOS:CORE:RUNTIME_SELECTED` as a decision gate only; no interpreter has
   booted yet.
2. `init-pak-loading` - COMPLETE. Loads the chosen custom-minimal runtime
   source payload from the now-validated `INIT.PAK`. Phase 1.5 validated the
   outer header; this slice validates the ADR 0014 inner payload magic,
   version, header length, exact source length, checksum, and UTF-8 boundary.
   Emits `PYTHOS:CORE:INIT_PAK_LOADED`. It does not parse, interpret, execute,
   import, or grant authority to the payload.
3. `interpreter-boot` - COMPLETE. Starts the custom minimal interpreter under
   PythCore by recognizing the exact `HelloService` source from `INIT.PAK`,
   synthesizing the fixed internal operation plan, assigning the runtime to a
   native task id and kernel service identity, and requiring an explicit boot
   capability before the runtime instance is created. Emits
   `PYTHOS:CORE:INTERPRETER_BOOTED`. It does not execute `system.log`,
   transition service readiness, define the `system.*` API, or implement
   coroutine/event semantics.
4. `system-api-surface` - COMPLETE. Exposes the ADR 0016 `system.log(message)`
   host call as the first and only current `system.*` function. The call checks
   a `LOG` capability for the runtime service identity before emitting
   `PYTHOS:CORE:SYSTEM:LOG`, rejects empty or oversized messages, and emits
   `PYTHOS:CORE:SYSTEM_API_READY` after the proof. `self.ready()` and service
   lifecycle transitions remain later slices.
5. `value-validation` - COMPLETE. ADR 0017 defines the current native/runtime
   value boundary. The runtime plan carries untrusted byte values, the native
   validator accepts only bounded nonempty UTF-8 strings for the current
   `system.log` call, rejects unsupported non-string, raw-pointer-shaped, and
   unchecked-native-struct-shaped inputs, models explicit host-call
   success/error results, and emits `PYTHOS:CORE:VALUE_VALIDATION_READY`.
6. `service-manager` - COMPLETE. The current exact runtime plan's
   `self.ready()` operation now transitions the runtime service from starting
   to ready under a fixed service manager, rejects unknown-service and
   duplicate-ready transitions, emits `PYTHOS:CORE:SERVICE:READY`, and
   completes with `PYTHOS:CORE:SERVICE_MANAGER_READY`.
7. `exception-containment` - COMPLETE. A managed service can be marked failed
   by an unhandled runtime exception without panicking PythCore or changing an
   unrelated ready service. Emits `PYTHOS:CORE:SERVICE:EXCEPTION` and
   `PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED`.
8. `service-restart` - COMPLETE. A failed noncritical managed service can
   restart into a fresh starting generation and return to ready without reboot.
   Emits `PYTHOS:CORE:SERVICE:RESTART` and
   `PYTHOS:CORE:SERVICE_RESTART_READY`.
9. `async-events` - COMPLETE. A fixed native event dispatches only to a ready
   managed service and is rejected for a failed service. Emits
   `PYTHOS:CORE:SERVICE:EVENT` and `PYTHOS:CORE:ASYNC_EVENTS_READY`.

### Exit Condition

```python
class HelloService(Service):
    async def start(self):
        system.log("PythOS [HISS] We Are Woken")
        self.ready()
```

The service runs under PythCore, inside a capability-scoped task, with its
`system.log` call passing through validated boundary crossing and capability
checks, and its lifecycle managed by the service manager.

### Scope Boundary

Do not add GUI, input drivers, networking, persistent storage beyond `INIT.PAK`
read access, any MCP or external-tool integration, or any agent concept. The
`system.*` surface in this phase is OS-service-shaped, not agent-shaped.

### Required Artifacts

ADR for runtime selection with benchmark data attached. ADR for the `system.*`
API surface and ADR for runtime value validation; these are PythOS's first real
ABI-shaped runtime contracts, so treat them with the same care as the boot ABI.
Exception-containment test suite.

### Architectural Test (Non-Binding)

None new at this phase. The typed-object question applies starting Phase 7.

## Phase 5: Real Graphical Shell - COMPLETE

### Purpose

The current screen is diagnostic output. This phase creates the first actual
interface, and is the phase where the semantic-versus-presentation separation
from the Patch/Open Surface vision becomes architecturally relevant, even
though nothing agent-related is built here.

### Preconditions

Phase 4 exit condition reproducible: Python services running, restartable, and
capability-scoped.

### Locked Slice Sequence

1. `keyboard-driver`, `mouse-driver` - COMPLETE. Native drivers decode fixed
   raw keyboard and mouse events only through explicit input capabilities.
   Emits `PYTHOS:CORE:INPUT:KEYBOARD`, `PYTHOS:CORE:INPUT:MOUSE`, and
   `PYTHOS:CORE:INPUT_DRIVERS_READY`.
2. `input-event-service` - COMPLETE. ADR 0018 records the native Phase 5
   service decision. The native service normalizes raw driver events into a
   typed event stream only for a capability-holding subscriber, emits
   `PYTHOS:CORE:INPUT:EVENT`, and completes with
   `PYTHOS:CORE:INPUT_EVENT_SERVICE_READY`.
3. `software-renderer` - COMPLETE. Framebuffer-target 2D drawing primitives
   fill clipped rectangles into a bounded native pixel buffer and emit
   `PYTHOS:CORE:RENDER:RECT` followed by
   `PYTHOS:CORE:SOFTWARE_RENDERER_READY`.
4. `font-system` - COMPLETE. Replaces the empty `FONT.PSF` placeholder with a
   deterministic PSF1 asset, passes it through explicit ADR 0019 boot-info
   fields, reserves and maps the font bytes, validates PSF metadata, and emits
   `PYTHOS:CORE:FONT:PSF_LOADED` followed by
   `PYTHOS:CORE:FONT_SYSTEM_READY`.
5. `compositor`, `surfaces`, `clipping` - COMPLETE. Windowing primitives now
   create typed drawable objects with separate presentation bindings, compose
   independently drawn bounded surfaces into a framebuffer target, prove target
   edge clipping, and emit `PYTHOS:CORE:COMPOSITOR:SURFACE`,
   `PYTHOS:CORE:COMPOSITOR:CLIP`, and `PYTHOS:CORE:COMPOSITOR_READY`.
6. `pointer-cursor`, `window-focus`, `movable-windows` - COMPLETE. Standard
   interaction primitives now prove bounded cursor state, z-order focus
   selection, and moving the focused window while preserving typed object
   identity. Emits `PYTHOS:CORE:POINTER_CURSOR_READY`,
   `PYTHOS:CORE:WINDOW_FOCUS_READY`, and
   `PYTHOS:CORE:MOVABLE_WINDOWS_READY`.
7. `buttons-and-text-fields` - COMPLETE. The minimal native widget set now
   proves fixed button activation and bounded text-field editing over typed
   widget objects. Emits `PYTHOS:CORE:WIDGET:BUTTON`,
   `PYTHOS:CORE:WIDGET:TEXT_FIELD`, and `PYTHOS:CORE:WIDGETS_READY`.
8. `application-launcher`, `service-monitor`, `python-console`,
   `settings-panel` - COMPLETE. The first fixed first-party applications are
   registered as capability-scoped services with typed windows, a fixed shell
   screen is rendered through the compositor path, and
   `PYTHOS:CORE:PHASE_5_COMPLETE` closes the phase boundary.

### Design Requirement

This binding requirement is carried from the vision docs for this phase's data
model, even though the vision itself is not being built.

Every drawable object created by a service must be backed by a typed object
with an explicit identity, independent of how it is currently rendered.
Presentation, such as position, color, and animation state, must be a separate
replaceable binding from meaning: what the object is and what actions are valid
on it. This is not "build the Semantic Canvas now"; it is "do not build a shell
where meaning only exists as pixels," because retrofitting that separation
later is far more expensive than building it straight. Concretely: a window has
a stable object id and a typed kind, such as `service-monitor-window`,
independent of its current x, y, width, height, or z-order.

### Exit Condition

A native or Python-driven desktop shell renders, accepts keyboard and mouse
input, supports multiple movable and focusable windows through the compositor,
and runs the four listed first-party applications, each isolated as its own
capability-scoped service.

### Scope Boundary

Do not add Open Surface, user-authored HTML/CSS/Canvas environments, Patch, any
agent, any MCP or tool integration, networking, audio, or persistent storage of
window layout.

### Required Artifacts

ADR for the typed-object/presentation-binding split. This is the decision the
architectural test below evaluates; record it explicitly enough that the answer
is checkable later, not just implied by code.

### Architectural Test (Non-Binding)

When the GUI is built, ask whether it supports user-authored surfaces instead
of locking everything into normal windows.

Answer by inspecting whether the compositor's surface abstraction could host a
non-native-widget presentation, such as arbitrary drawn content, without kernel
or compositor changes. Do not build Open Surface itself.

## Phase 6: Cinematic Boot and Voice - COMPLETE

### Purpose

Replace the current diagnostic boot screen with the native identity sequence:
"PythOS [HISS] We Are Woken." It must be natively implemented, not the
existing prototype HTML boot animation.

> Reopened by ADR 0047 (2026-07-25): the native cinematic was enriched to match
> the authored "Black/Violet/Electric-Blue" reference — a glowing serpent that
> forms in, shimmers, ignites an energy orb at the awakening beat, and resolves
> into the "PythOS / We Are Woken" title. Still native (the reference `.mp4`/HTML
> is used only as a visual reference, never embedded), still bounded (compact
> ~3.5 s so the milestone-1 boot stays inside its 20 s budget; the full-length
> version is a real-hardware-only option). The wake phrase and all Phase 6
> markers are unchanged.

### Preconditions

Phase 5 exit condition reproducible: shell renders and compositor is
functional.

### Locked Slice Sequence

1. `audio-device-selection` - COMPLETE. ADR 0020 chooses QEMU AC97 and records
   the no-audio fallback posture.
2. `audio-driver` - COMPLETE. PythCore configures the selected AC97 mixer and
   bus-master interface, or enters a silent driver state when absent.
3. `audio-buffers` - COMPLETE. A page-contained PCM buffer and AC97 BDL are
   configured for deterministic playback.
4. `pcm-playback` - COMPLETE. A fixed deterministic PCM asset is submitted to
   AC97.
5. `audio-mixing` - COMPLETE. More than one simultaneous PCM source is mixed
   for the layered hiss, sub-bass, and tremolo design already prototyped in the
   HTML boot animation.
6. `boot-asset-storage` - COMPLETE. The three boot assets, visual sequence
   data, PCM audio shape, and timing/sync data, are embedded in PythCore
   because Phase 7 persistent storage does not exist yet.
7. `audio-visual-sync` - COMPLETE. PCM playback timing is synchronized with the
   compositor-driven visual formation sequence.
8. `graceful-audio-fallback` - COMPLETE. If no audio device is available or
   detected, the visual sequence still completes correctly. Boot never blocks
   or degrades waiting on audio.

### Exit Condition

The native boot sequence replaces the diagnostic screen, plays synchronized
audio matching the previously prototyped design intent, including
bandpass-swept hiss, sub-bass layering, and tremolo, and completes correctly
with audio disabled or unavailable.

### Scope Boundary

Do not add user-configurable boot themes, Open Surface, or persistent storage
of user boot customization.

### Required Artifacts

ADR for audio device choice. Fallback-path test proving boot completes with the
audio device absent in QEMU config.

Satisfied by ADR 0020 and:

```powershell
python scripts\test-boot.py --slice graceful-audio-fallback --no-audio-device
```

### Architectural Test (Non-Binding)

None. This phase has no vision-doc dependency.

## Phase 7: Persistent Object Storage - COMPLETE

### Purpose

PythOS has no persistent user data yet. This phase is also the first point
where the Causal Lens vision becomes evaluable, because provenance requires
something to have provenance: a persisted, typed object history.

### Preconditions

Phase 6 exit condition reproducible.

### Locked Slice Sequence

1. `block-device-driver` - COMPLETE. Virtual block device driver selection,
   with QEMU legacy `virtio-blk` as the first target. The QEMU harness now
   attaches an explicit boot ESP plus a non-boot raw storage image, PythCore
   selects vendor `0x1AF4` device `0x1001`, validates the legacy I/O BAR,
   enables I/O and bus-master command bits, reads bounded capacity and queue
   metadata, and emits `PYTHOS:CORE:BLOCK:DEVICE_SELECTED` followed by
   `PYTHOS:CORE:BLOCK_DEVICE_READY`. This slice does not implement the
   storage service, raw read/write operations, journaling, or object storage.
2. `storage-service` - COMPLETE. Capability-gated service facade mediating all
   block access. The selected block device is opaque outside `block_device`;
   storage requests are authorized through Phase 3 capability handles, and
   wrong-holder or missing-rights attempts are denied before block access.
   Emits `PYTHOS:CORE:STORAGE:ACCESS_GRANTED`,
   `PYTHOS:CORE:STORAGE:ACCESS_DENIED`, and
   `PYTHOS:CORE:STORAGE_SERVICE_READY`. This slice does not implement sector
   I/O, journaling, commit markers, recovery, or object storage.
3. `append-only-journal` - COMPLETE. Storage-service-authorized write intents
   append monotonic journal records before any write completion can be
   considered, and the slice emits `PYTHOS:CORE:STORAGE:JOURNAL_APPEND`
   followed by `PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY`. This slice does not
   implement checksums, commit markers, recovery, sector I/O, or object
   storage.
4. `checksums-and-commit-markers` - COMPLETE. Every committed journal record
   carries a stable checksum over its record fields plus an explicit commit
   marker. Missing commit markers and checksum mismatches are detected instead
   of silently accepted. Emits `PYTHOS:CORE:STORAGE:CHECKSUM_VALID`,
   `PYTHOS:CORE:STORAGE:COMMIT_MARKER`, and
   `PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_READY`. This slice does not implement
   crash recovery, sector I/O, or object storage.
5. `crash-recovery` - COMPLETE. Replay/rollback logic reconstructs a
   consistent state from the committed journal prefix after an unclean
   shutdown. Missing commit markers and checksum mismatches terminate replay
   and roll back the invalid tail, including a simulated interrupted-write
   record. Emits `PYTHOS:CORE:STORAGE:RECOVERY_REPLAY`,
   `PYTHOS:CORE:STORAGE:RECOVERY_ROLLBACK`, and
   `PYTHOS:CORE:CRASH_RECOVERY_READY`. This slice does not implement typed
   objects, sector I/O, or object browser work.
6. `typed-object-format` - COMPLETE. ADR 0022 defines the fixed on-disk typed
   object record with magic, format version, record length, stable `ObjectId`,
   `ObjectKind` code, object schema version, and bounded versioned field
   slots. PythCore validates stable identity and versioned field round-trips
   and rejects bad magic, unsupported versions, invalid kind codes, invalid
   field counts, nonzero reserved fields, and oversized field values. Emits
   `PYTHOS:CORE:OBJECT:STABLE_ID`,
   `PYTHOS:CORE:OBJECT:VERSIONED_FIELDS`, and
   `PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY`. This slice does not implement
   relationships, revision history, workspace objects, object browser work, or
   sector persistence.
7. `object-relationships` - COMPLETE. Typed, queryable relationships between
   known typed objects are recorded for `blocks`, `created-by`, and
   `depends-on`, and relationship lookup rejects unknown endpoints and duplicate
   edges. Emits `PYTHOS:CORE:OBJECT:RELATIONSHIP`,
   `PYTHOS:CORE:OBJECT:RELATIONSHIP_QUERY`, and
   `PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY`. This slice does not implement
   revision history, workspace objects, object browser work, or sector
   persistence.
8. `revision-history` - COMPLETE. Object updates retain prior versions instead
   of overwriting the only copy, and each retained revision carries a monotonic
   timestamp plus writer service identity from Phase 3. Emits
   `PYTHOS:CORE:OBJECT:REVISION_RETAINED`,
   `PYTHOS:CORE:OBJECT:REVISION_PROVENANCE`, and
   `PYTHOS:CORE:REVISION_HISTORY_READY`. This slice does not implement
   workspace objects, object browser work, or sector persistence.
9. `workspace-objects` - COMPLETE. ADR 0023 defines the first concrete
   persistent object kind, `WorkspaceSession`, and schema version 1 stores the
   Phase 5 shell window layout in bounded ADR 0022 fields. Emits
   `PYTHOS:CORE:WORKSPACE:SESSION_OBJECT`,
   `PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT`, and
   `PYTHOS:CORE:WORKSPACE_OBJECTS_READY`. This slice does not implement object
   browser work, reboot persistence, or sector persistence.
10. `object-browser` - COMPLETE. ADR 0024 defines a minimal Phase 5
    app-facing inspection surface over the object-store substrate. It creates
    a typed object-browser window, lists stored typed objects, inspects typed
    relationships, and inspects retained revision counts. Emits
    `PYTHOS:CORE:OBJECT_BROWSER:LIST`,
    `PYTHOS:CORE:OBJECT_BROWSER:DETAIL`, and
    `PYTHOS:CORE:OBJECT_BROWSER_READY`. This slice does not implement reboot
    persistence or sector persistence.
11. `save-and-restore-across-reboot` - COMPLETE. ADR 0025 defines the fixed
    checkpoint/recovery sectors used for the Phase 7 end-to-end proof. PythCore
    writes the typed workspace snapshot through virtio-blk sector I/O, reboots
    against the same raw storage image, restores the same object, relationship,
    and revision metadata, and proves a deliberately killed mid-commit boot
    recovers by ignoring and clearing the torn tail. Emits
    `PYTHOS:CORE:OBJECT_STORE:PERSISTED`,
    `PYTHOS:CORE:OBJECT_STORE:RESTORED`, and
    `PYTHOS:CORE:PHASE_7_COMPLETE`. This slice does not implement a
    filesystem, dynamic object database, Causal Lens UI, Patch, networking, or
    multi-user access control.

### Exit Condition

Objects created before a reboot, including their relationships and revision
history, are present and correct after reboot, verified by an automated test
that reboots QEMU and re-queries the object store. A simulated torn write, by
killing QEMU mid-commit, recovers to the last consistent committed state, not a
corrupted one.

### Scope Boundary

Do not add Causal Lens confidence or provenance-labeling UI, Patch, any
agent-facing query API beyond what the object browser needs, networking, or
multi-user access control. The single local user model still holds.

### Required Artifacts

ADR for on-disk typed-object format. This is a durable format, so treat format
changes after this ADR as migrations, not silent rewrites. Crash-recovery test
suite including at least one deliberately interrupted write scenario.

Satisfied by ADR 0022, ADR 0025, and:

```powershell
python scripts\test-persistent-storage.py
```

### Architectural Test (Non-Binding)

When typed objects are built, ask whether they carry enough provenance for the
Causal Lens.

Answer by checking whether `revision-history`, `object-relationships`, and the
writer-identity metadata are sufficient to answer a "why does this exist and
what does it block" query in principle, without building the query API or the
Causal Lens UI itself.

Answer: yes, in principle. Revision history preserves prior object versions
with timestamps and writer service identity, relationships preserve typed
causal edges such as `blocks` and `depends-on`, and the object browser can
inspect the stored object graph. That is enough provenance substrate to explain
why an object exists and what it blocks later, as described in
`docs/vision/patch.md`; this phase intentionally does not build the query API,
Causal Lens UI, or Patch.

## Phase 8: Real Hardware Isolation - COMPLETE

### Purpose

Until this phase, capability separation is architectural, not a complete
hostile-code boundary; everything still runs in kernel mode. This phase makes
the capability system from Phase 3 actually enforceable against code that does
not cooperate.

### Preconditions

Phase 7 exit condition reproducible. This phase is intentionally last among the
numbered phases before "Later Phases" because every earlier phase's capability
checks were trust-the-caller; this phase makes them hardware-enforced.

### Locked Slice Sequence

1. `ring-3-execution` - COMPLETE. ADR 0026 records the first Phase 8
   hardware-isolation step. PythCore installs ring-3 GDT code/data selectors,
   sets `TSS.RSP0`, maps a fixed CPL3 proof code page and user stack in the
   current address space, exposes the breakpoint gate at DPL3 for the proof,
   enters user mode with `iretq`, verifies a user-originated trap frame, and
   returns to a saved kernel stack. Emits `PYTHOS:CORE:USER_MODE:ENTER`,
   `PYTHOS:CORE:USER_MODE:RETURN`, and
   `PYTHOS:CORE:RING3_EXECUTION_READY`. This slice does not implement separate
   address spaces, syscall entry, user process stacks, service-local runtimes,
   or hostile-code containment.
2. `separate-address-spaces` - COMPLETE. ADR 0027 records the first distinct
   user CR3 root. PythCore builds the root before the first PythCore-owned CR3
   switch, validates it is distinct from the kernel root, validates only the
   fixed proof code/stack are user-accessible while kernel text/data remain
   supervisor-only, switches to that root, reruns the CPL3 breakpoint proof,
   restores the kernel root, and emits
   `PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY`. This slice does not implement
   syscall entry, user process stacks, service-local runtimes, guarded shared
   memory, process termination, quotas, crash containment, or hostile-code
   capability enforcement.
3. `syscall-entry` - COMPLETE. ADR 0028 defines the first syscall ABI.
   PythCore configures the x86-64 `syscall`/`sysret` MSRs, enters the syscall
   gate from the fixed CPL3 proof running under the distinct user CR3 root,
   switches from the user stack to a fixed kernel syscall stack, dispatches
   syscall number `0x5059_0001`, proves a capability-gated Phase 3 IPC send,
   invokes the Phase 4 `system.log` surface with `PythOS [HISS] We Are Woken`,
   returns through `sysretq`, and completes with
   `PYTHOS:CORE:SYSCALL_ENTRY_READY`. This slice does not implement user
   process stacks, user pointer copy-in/copy-out, service-local runtimes,
   guarded shared memory, process termination, quotas, crash containment, or
   hostile-code capability enforcement.
4. `user-stacks` - COMPLETE. ADR 0029 records the guarded user-stack layout.
   PythCore reserves fixed page-aligned user stack slots with supervisor-only
   guard pages below usable non-executable user stack pages, maps only the
   usable pages into the distinct user CR3 root, validates both the usable and
   guard-page permissions, migrates the CPL3 proof onto that stack pool, and
   emits `PYTHOS:CORE:USER_STACK:ALLOCATED`,
   `PYTHOS:CORE:USER_STACK:GUARD_PAGE`, and
   `PYTHOS:CORE:USER_STACKS_READY`. This slice does not implement dynamic user
   processes, user pointer copy-in/copy-out, service-local runtimes, guarded
   shared memory, process termination, quotas, crash containment, or
   hostile-code capability enforcement.
5. `service-local-python-runtimes` - COMPLETE. ADR 0030 records the
   service-local runtime-instance proof. PythCore boots two runtime instances
   from the validated Phase 4 source through a shared service-identity table,
   gives them distinct service identities, task ids, user CR3 roots, and local
   runtime state slots, rejects cross-service state mutation, and emits
   `PYTHOS:CORE:RUNTIME:LOCAL_INSTANCE`,
   `PYTHOS:CORE:RUNTIME:ADDRESS_SPACE`,
   `PYTHOS:CORE:RUNTIME:STATE_ISOLATED`, and
   `PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY`. This slice does not implement
   guarded shared memory, user pointer copy-in/copy-out, process termination,
   quotas, crash containment, or hostile-code capability enforcement.
6. `guarded-shared-memory` - COMPLETE. ADR 0031 records the Phase 8
   shared-memory revalidation. PythCore binds reader and writer service
   identities to distinct user CR3 roots, proves a read-only shared-memory
   handle can still read the fixed region under those constraints, denies a
   cross-space write attempt through the wrong holder, verifies the region
   bytes remain unchanged, and emits `PYTHOS:CORE:SHM:RING3_READ`,
   `PYTHOS:CORE:SHM:CROSS_SPACE_WRITE_DENIED`, and
   `PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY`. This slice does not implement
   user pointer copy-in/copy-out, process termination, quotas, crash
   containment, or hostile-code capability enforcement.
7. `process-termination` - COMPLETE. ADR 0032 records the Phase 8
   process-termination proof. PythCore tracks a fixed user process record,
   marks it terminated, proves it is no longer returned as runnable, reclaims
   the terminated user address-space page-table frames, verifies the physical
   allocator free-page count increases by the exact reclaimed count, and emits
   `PYTHOS:CORE:PROCESS:TERMINATED`,
   `PYTHOS:CORE:PROCESS:UNSCHEDULABLE`,
   `PYTHOS:CORE:PROCESS:ADDRESS_SPACE_RECLAIMED`, and
   `PYTHOS:CORE:PROCESS_TERMINATION_READY`. This slice does not implement
   memory quotas, CPU quotas, crash containment, or hostile-code capability
   enforcement.
8. `memory-quotas` - COMPLETE. ADR 0033 records the Phase 8 kernel-owned
   memory quota proof. PythCore registers a service identity, grants an
   in-quota page charge, denies an over-quota page charge, verifies the denied
   charge does not mutate recorded usage, and emits
   `PYTHOS:CORE:QUOTA:MEMORY_GRANTED`,
   `PYTHOS:CORE:QUOTA:MEMORY_DENIED`, and
   `PYTHOS:CORE:MEMORY_QUOTAS_READY`. This slice does not implement CPU
   quotas, crash containment, or hostile-code capability enforcement.
9. `cpu-quotas` - COMPLETE. ADR 0034 records the Phase 8 kernel-owned
   CPU quota proof. PythCore registers a service identity, records an in-budget
   tick charge, denies an over-quota tick charge, verifies the denied charge
   does not mutate recorded usage, and emits `PYTHOS:CORE:QUOTA:CPU_TICK`,
   `PYTHOS:CORE:QUOTA:CPU_THROTTLED`, and
   `PYTHOS:CORE:CPU_QUOTAS_READY`. This slice does not implement crash
   containment or hostile-code capability enforcement.
10. `crash-containment` - COMPLETE. ADR 0035 records the Phase 8 crash
   containment proof. PythCore runs a fixed CPL3 illegal-instruction probe,
   diagnoses it through the Phase 1.5 exception path as a user fault, terminates
   only the faulting service process, preserves a peer service process, and
   emits `PYTHOS:CORE:CRASH:USER_FAULT`,
   `PYTHOS:CORE:CRASH:SERVICE_TERMINATED`,
   `PYTHOS:CORE:CRASH:PEER_ALIVE`, and
   `PYTHOS:CORE:CRASH_CONTAINMENT_READY`. This slice does not implement full
   hostile-code capability enforcement.
11. `capability-enforcement-at-boundary` - COMPLETE. ADR 0036 records the
   final Phase 8 syscall-boundary proof. PythCore contains a fixed CPL3
   bad-pointer page fault, validates a legitimate capability before enqueueing
   privileged IPC, denies a copied handle value from the wrong service identity
   before mutation, denies a hardware-port resource request through the wrong
   capability resource, and emits
   `PYTHOS:CORE:BOUNDARY:BAD_POINTER_CONTAINED`,
   `PYTHOS:CORE:BOUNDARY:CAPABILITY_ALLOWED`,
   `PYTHOS:CORE:BOUNDARY:FORGERY_DENIED`,
   `PYTHOS:CORE:BOUNDARY:HARDWARE_DENIED`, and
   `PYTHOS:CORE:CAPABILITY_BOUNDARY_READY`. This slice does not implement a
   general-purpose userspace ABI, copy-in/copy-out, networking, package
   management, SMP, or broad hardware expansion.

### Exit Condition

A deliberately hostile test service, using bad pointer dereference, attempted
direct hardware access, and attempted capability forgery, is contained,
terminated, and diagnosed without affecting the kernel or other services. This
must be proven by an automated adversarial test suite, not just the Phase 3
negative-authorization tests rerun in ring 3.

### Scope Boundary

Do not add networking, package management, or updates. Those are Later Phases.

### Required Artifacts

ADR for the syscall ABI. This is the most consequential ABI in the project; once
user-mode code exists against it, breaking changes have real cost. Adversarial
test suite is a required deliverable with the same weight as Phase 3
negative-authorization suite.

ADR 0026 records the `ring-3-execution` proof. It is intentionally not the
syscall ABI ADR and does not claim hostile-code containment.

ADR 0027 records the `separate-address-spaces` proof. It creates a distinct
user CR3 root for the fixed proof path only and is intentionally not a process
model or syscall ABI.

ADR 0028 records the `syscall-entry` ABI. It defines the first syscall number
and register contract, but intentionally accepts no user pointers and does not
claim hostile-code containment.

ADR 0029 records the `user-stacks` guarded stack layout. It defines fixed
guarded stack slots for Phase 8 proofs, but intentionally does not define
dynamic process stacks or stack reclamation.

ADR 0030 records the `service-local-python-runtimes` proof. It gives
service-local runtime instances separate service identities, address-space
roots, and local state slots, but intentionally does not define guarded shared
memory or hostile-code containment.

ADR 0031 records the `guarded-shared-memory` proof. It revalidates Phase 3
shared-memory capability semantics under distinct Phase 8 user roots, but
intentionally does not define copy-in/copy-out, process termination, or
hostile-code containment.

ADR 0032 records the `process-termination` proof. It removes a fixed user
process from scheduling and reclaims its recorded address-space table frames,
but intentionally does not define quotas, crash containment, or full syscall
boundary enforcement.

ADR 0033 records the `memory-quotas` proof. It enforces a kernel-owned memory
page budget keyed by service identity and denies over-quota charges without
mutating usage, but intentionally does not define CPU budgets, crash
containment, or full syscall boundary enforcement.

ADR 0034 records the `cpu-quotas` proof. It enforces a kernel-owned CPU tick
budget keyed by service identity and denies over-quota tick charges without
mutating usage, but intentionally does not define crash containment or full
syscall boundary enforcement.

ADR 0035 records the `crash-containment` proof. It contains a fixed CPL3
illegal-instruction fault as a service crash, terminates only the faulting
service process, and keeps a peer service process runnable, but intentionally
does not define full syscall boundary enforcement.

ADR 0036 records the `capability-enforcement-at-boundary` proof. It contains a
fixed CPL3 bad-pointer page fault, denies copied-handle use by the wrong
service identity, denies hardware-resource repurposing through the syscall gate,
and completes Phase 8's hardware-backed authority boundary for the fixed proof
surface.

### Architectural Test (Non-Binding)

When capabilities are built, reevaluate post-Phase-8: are they strong enough to
safely scope an agent, now that enforcement is hardware-backed rather than
cooperative?

Answer: yes for the fixed local proof surface, but not yet for arbitrary
agent-facing workflows. Phase 8 proves the kernel, not cooperating service code,
owns boundary enforcement for user faults, copied handle values, direct
hardware-style resource requests, and syscall-gated privileged IPC. Agent scoping
can build on that mechanism later, but it still needs a later user/API surface,
copy-in/copy-out policy, storage/object authorization policy, and audit UX
before any AI can invoke privileged operations.

## Later Phases (Superseded)

The current Phase 11 onward roadmap is maintained in
[`docs/ROADMAP-LATER-PHASES.md`](ROADMAP-LATER-PHASES.md). That document is
canonical after verified Phase 9 and Phase 10 completion. The older detailed
planning sketch below is retained only as historical context and must not be
used to start new work.

This extends `docs/ROADMAP.md`'s deliberately coarse "Later Phases" section
now that Phase 8 is complete. It uses the same format as Phases 1.5-8:
purpose, preconditions, locked slice sequence, exit condition, scope
boundary, required artifacts.

## Corrected Sequencing, Stated Up Front

The original "Later Phases" list read as: applications, updates, networking,
semantic indexing, hardware, SMP. That ordering assumes apps/packaging can
build directly on what Phase 7/8 already proved. It can't. Both of those
phases proved *bounded, fixed* surfaces:

- Phase 8 proves a fixed CPL3/syscall/capability surface against fixed,
  compile-time-known test scenarios. It does not prove PythOS can load and
  run an arbitrary, dynamically-provided user program.
- Phase 7 proves a fixed checkpoint object survives reboot and torn writes.
  It does not prove general, dynamically-sized, arbitrarily-many-object
  storage allocation.

Two phases were missing from the list entirely. Corrected order:

```text
Phase 9   general-purpose-process-model   (generalizes Phase 8)
Phase 10  general-purpose-storage          (generalizes Phase 7)
Phase 11  physical-hardware-boot-smoke-test (cheap, do this early)
Phase 12  applications-and-packaging       (needs 9 + 10)
Phase 13  networking
Phase 14  hardware-driver-expansion
Phase 15  updates-and-recovery-mode
Phase 16  smp                              (deliberately last)
```

Semantic indexing / local AI is intentionally not on this list — that's
vision-layer territory (Patch's actual dependency), out of scope for "extend
the OS itself," and stays parked per the existing rule in `docs/ROADMAP.md`.

---

## Phase 9: General-Purpose Process Model

### Purpose

Turn Phase 8's fixed adversarial proof into an actual capability: PythOS can
load an arbitrary, dynamically-provided user program, not just run its own
fixed test scenarios in ring 3.

### Preconditions

Phase 8 exit condition reproducible (ring-3 entry, syscall gate, capability
enforcement at the boundary, all hardware-backed).

### Locked slice sequence

1. **`dynamic-elf-loading`** — COMPLETE. ADR 0037 defines the versioned inner
   `INIT.PAK` bundle. PythCore loads an arbitrary user-mode ELF binary at
   runtime from that bundle, not from a fixed compiled-in test payload,
   validates its headers, maps its segments with correct permissions (RX for
   text, RW for data, no segment ever both W and X), proves malformed buffer
   range, W+X, and kernel-range segments are denied, and emits
   `PYTHOS:CORE:DYNAMIC_ELF_LOADING_READY`.
2. **`general-syscall-abi`** — COMPLETE. ADR 0038 defines ABI version `1.0`,
   reserves `0x5059_0000` for side-effect-free ABI metadata, preserves
   `0x5059_0001` as the permanent Phase 8 system-log proof syscall, routes
   dispatch through a fixed syscall registry, denies unsupported syscall
   numbers without running privileged bridges, and emits
   `PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY`.
3. **`copy-in-copy-out-policy`** — COMPLETE. ADR 0039 defines the
   copy-in/copy-out validation policy for every user-supplied pointer and
   length crossing the syscall boundary: checked range arithmetic, single
   mapped-region containment, read/write access direction, no raw pointer
   dereference before validation, and distinct denial proofs for
   out-of-range, length-overflow, and cross-mapping buffers. Emits
   `PYTHOS:CORE:COPY_IN_COPY_OUT_READY`.
4. **`dynamic-capability-grants`** — COMPLETE. ADR 0040 defines the dynamic
   process grant model: a newly created process starts with zero capabilities
   by default; its initial grant set is determined by an explicit
   creator-supplied policy, not hardcoded. PythCore proves process creation,
   zero-default inventory, no-grant denial, explicit grant issuance, and
   granted use, then emits `PYTHOS:CORE:DYNAMIC_CAPABILITY_GRANTS_READY`.
5. **`process-argv-and-environment`** — COMPLETE. ADR 0041 defines the
   bounded launch-data policy: argv is delivered as an immutable launch vector,
   environment entries are keyed to explicit resource capabilities, a granted
   process can read its environment value, and an ungranted process is denied
   even when it asks for the same key. Emits
   `PYTHOS:CORE:PROCESS_ARGV_ENV_READY`.
6. **`general-fault-isolation`** — COMPLETE. ADR 0042 re-runs Phase 8's
   crash-containment proof against a dynamically loaded invalid-instruction ELF
   payload, not the fixed Phase 8 proof page. PythCore maps the payload into a
   dynamic user address space, enters it at CPL3, recovers through the
   user-originated fault path, proves only the faulting service terminates, and
   emits `PYTHOS:CORE:GENERAL_FAULT_ISOLATION_READY`.
7. **`process-model-adversarial-suite`** — COMPLETE. ADR 0043 loads multiple
   dynamic user ELF variants from the inner `INIT.PAK` bundle, proves a
   runnable dynamic payload returns through the general user-mode path, proves
   a forged capability is denied by the syscall-boundary capability model,
   proves a bad user pointer is denied by the copy-in/copy-out policy and
   contained when attempted by a dynamic payload, proves direct hardware access
   is denied by the CPU privilege boundary and capability model, and emits
   `PYTHOS:CORE:PROCESS_MODEL_ADVERSARIAL_READY` followed by
   `PYTHOS:CORE:PHASE_9_COMPLETE`.

### Exit condition

An arbitrary, previously-unseen-by-the-kernel user ELF binary loads, runs,
and is capability/pointer/fault-contained by the general mechanism — proven
against a binary the test suite didn't special-case, not the fixed Phase 8
proof payload.

### Scope boundary

No filesystem-backed program loading yet (that's Phase 10 dependency for
Phase 12) — programs can be loaded from a fixed in-memory bundle or the
existing `INIT.PAK`-style mechanism at this phase. No networking. No
multi-process scheduling changes beyond what Phase 2 already provides.

### Required artifacts

ADR 0037 for the inner `INIT.PAK` bundle format is complete. ADR 0038 for the
syscall number space and versioning policy is complete. ADR 0039 for the
copy-in/copy-out pointer policy is complete. ADR 0040 for dynamic capability
grants is complete. ADR 0041 for process argv/environment launch data is
complete. ADR 0042 for dynamic general fault isolation is complete. ADR 0043
for the process-model adversarial suite is complete. Phase 9 is complete;
Phase 10 now follows below. The active hard stop is the Phase 10 -> Phase 11
boundary in the Phase 10 section.

---

## Phase 10: General-Purpose Storage

### Purpose

Generalize Phase 7's bounded checkpoint into a real allocator: arbitrary
numbers of arbitrarily-sized typed objects, not one fixed checkpoint sector.

### Preconditions

Phase 7 exit condition reproducible (journal, checksums, crash recovery,
typed-object format, reboot round-trip, torn-write recovery all proven for
the bounded case).

### Locked slice sequence

1. **`block-allocator`** — complete. ADR 0044 records a journaled first-fit
   bitmap allocator; torn allocator metadata rolls back to the last committed
   bitmap.
2. **`dynamic-object-count`** — complete. The object store creates and deletes
   multiple typed objects over allocator-owned extents.
3. **`fragmentation-and-compaction-policy`** — complete. ADR 0045 records
   freed-block reuse now and explicitly defers live-object compaction.
4. **`storage-quota-per-service`** — complete. Storage block quotas are scoped
   by service identity and over-quota writes are denied without mutating
   usage.
5. **`concurrent-write-safety`** — complete. Capability-gated write tokens
   serialize writers and stale tokens cannot release another writer's gate.
6. **`storage-adversarial-suite`** — complete. Repeated create/delete cycles,
   out-of-quota denial, dynamic torn-write rollback, and a killed-mid-commit
   QEMU recovery path are verified.

### Exit condition

An arbitrary, growing set of typed objects can be created, deleted, and
survive reboot and a torn write under dynamic allocation — the same rigor
as Phase 7's proof, generalized past the single fixed checkpoint.

Status: complete through `PYTHOS:CORE:PHASE_10_COMPLETE`. Halt at the Phase
10 -> Phase 11 boundary; do not begin Phase 11
`real-hardware-target-selection` without explicit re-invocation.

### Scope boundary

No general POSIX-style filesystem hierarchy/path semantics required at this
phase — that's a Phase 12 decision (does PythOS want paths, or does it stay
object-graph-native per the Phase 5 `ADR 0018` design direction). Don't
prematurely commit to paths here.

### Required artifacts

ADR 0044 records the on-disk allocator format and journaled metadata
consistency rule. ADR 0045 records the fragmentation/compaction policy. The
crash-recovery harness in `scripts/test-persistent-storage.py` includes a
Phase 10 killed-mid-commit dynamic-allocation recovery scenario.

---

## Phase 11: Physical Hardware Boot Smoke Test

### Purpose

Cheap, high-value, and worth doing *before* the bigger phases, not after:
confirm the entire Phase 1-8 boot chain — UEFI handoff through
`MILESTONE_1_COMPLETE` — actually works on real silicon, not just QEMU and
VMware's own UEFI implementation. This is the same instinct as the VMware
check from earlier, extended to real hardware, and it's cheap because it
requires zero new drivers — just booting the existing ISO on a real machine
via USB.

### Preconditions

None beyond Phase 1's bootable ISO — this phase deliberately doesn't depend
on Phases 9/10, which is why it's sequenced early despite being numbered
after them. Can run in parallel with 9/10 if convenient.

### Locked slice sequence

1. **`real-hardware-target-selection`** — pick one specific real machine
   (ideally one you already own) as the first target. Record its exact UEFI
   firmware vendor/version in an ADR — this matters, because unlike OVMF and
   VMware's EFI, real firmware varies significantly vendor to vendor.
2. **`bootable-usb-creation`** — write `target/pythos.iso` to a USB drive as
   a real bootable UEFI device.
3. **`serial-capture-on-real-hardware`** — this is the hard part: real
   machines don't expose COM1 the way QEMU/VMware do by default. Determine
   whether the target has a physical serial port, a USB-to-serial adapter
   path, or whether this phase has to fall back to framebuffer-only visual
   verification for the first pass.
4. **`real-hardware-boot-attempt`** — boot it. Record what actually happens:
   full marker sequence, partial, hang, or reset.
5. **`divergence-catalog`** — document every place real hardware behaves
   differently from OVMF/VMware: ACPI table shape, memory map layout,
   framebuffer pixel format/resolution, SMBIOS content. This is genuinely
   useful data regardless of outcome.

### Exit condition

Either: the full marker sequence through `MILESTONE_1_COMPLETE` is observed
on real hardware (best case, validates the whole foundation is portable),
or: a documented, specific divergence point is identified (still valuable —
tells you exactly what Phase 14's hardware expansion actually needs to
handle, instead of guessing).

### Scope boundary

Do not start writing real drivers for USB/NVMe/Ethernet here — that's Phase
14. This phase only proves or disproves that the existing kernel boots on
real firmware at all.

### Required artifacts

A findings document (`docs/research/real-hardware-boot-findings.md`) even if
— especially if — it fails. A documented failure here is more valuable than
skipping this phase and finding out during Phase 14 with drivers involved.

---

## Phase 12: Applications and Packaging

### Purpose

The first phase where PythOS runs software it didn't compile itself.

### Preconditions

Phase 9 (general process model) and Phase 10 (general storage) both
reproducible. This is the phase that was previously listed as immediately
next — it isn't, until both of those exist.

### Locked slice sequence

1. **`package-format`** — define what a PythOS package actually is (ADR):
   a typed object in Phase 10's store, presumably, given the project's
   object-graph-native direction from `ADR 0018`. Decide now whether this
   is filesystem-path-based or purely object-graph-based — this is the
   moment that decision can no longer be deferred.
2. **`package-install`** — install from a local source (USB, local file —
   not networked yet, that's Phase 13) into Phase 10's storage, capability-
   scoped so installation itself is a mediated, auditable operation.
3. **`package-launch`** — launch an installed package as a Phase 9 process
   with a capability grant set derived from the package's declared needs,
   not ambient full access.
4. **`package-uninstall`** — clean removal, including reclaiming Phase 10
   storage and revoking any outstanding capabilities the package held.
5. **`first-third-party-app`** — the actual proof: something you didn't
   write as part of the kernel test suite, packaged and run through this
   pipeline end to end.

### Exit condition

A package built independently of the kernel test suite installs, launches
as a properly capability-scoped process, and uninstalls cleanly, verified
by an automated test — not a manual demo.

### Scope boundary

No package registry, no remote fetching, no dependency resolution between
packages yet. Single local package, installed and run, is the whole bar.

### Required artifacts

ADR for the package format (item 1) — this is a durable, user-facing format
the moment real packages exist against it.

---

## Phase 13: Networking

### Purpose

The parking-lot condition from `docs/ROADMAP.md` — "revisit only after local
IPC and kernel-enforced capabilities are implemented, tested, and boring" —
is now genuinely satisfied. Phase 3 and Phase 8 made that true. This is also
the earliest legitimate point the parked datacenter/capability-brokering
idea from early in this project could be reopened, though only as a much
later research branch, not as this phase's scope.

### Preconditions

Phase 9 (general process model, since network-facing code should run as a
capability-scoped process, not kernel-privileged by default).

### Locked slice sequence

Standard layered build: `nic-driver` (virtio-net in QEMU first) →
`link-layer` → `arp` → `ip` → `icmp` (ping as the first working proof) →
`udp` → `tcp` → `dns` → `capability-gated-socket-api` (a process needs an
explicit network capability, scoped to which ports/destinations, before it
can open any socket — this is the slice that matters most given the whole
project's capability discipline) → `secure-transport` (TLS, needed before
any real update mechanism in Phase 15 can be trusted).

### Exit condition

Two PythOS processes, or PythOS and an external host, exchange data over
TCP, with the socket capability grant proven to gate access the same way
every other Phase 3/8 capability does — a process without the network
capability is denied even knowing the destination, same proof shape as
every prior phase.

### Scope boundary

No DNS-based service discovery beyond basic resolution, no routing beyond a
single default gateway, no firewall/NAT. This phase proves the mechanism
and the capability gate, not a production network stack.

### Required artifacts

ADR for the socket capability model — this is where PythOS's capability
discipline either extends cleanly to network resources or reveals a gap,
same role Phase 7 played for the Phase 5 typed-object split.

---

## Phase 14: Hardware Driver Expansion

### Purpose

The deep, unglamorous, genuinely open-ended grind: real USB, NVMe, Ethernet,
audio beyond Phase 6's QEMU AC97 target, and power management. This is
explicitly the least sliceable phase in the whole roadmap — driver work
against real, varied, often undocumented hardware does not decompose into
clean vertical slices the way kernel-theory work does.

### Preconditions

Phase 11's findings document exists and is read first — it tells you
exactly where real hardware already diverges from what's been tested.

### Locked slice sequence

Sequence by what Phase 11 found, not a fixed list. If Phase 11 succeeded
cleanly, start with USB (most immediately useful — keyboard/mouse beyond
QEMU's virtual ones, mass storage). Each device class gets its own
purpose/preconditions/exit-condition treatment written *when it becomes
active*, same discipline as this document did for Phase 9/10/11 — don't
pre-write driver slices for hardware you haven't tested against yet.

### Exit condition

Defined per device class when that sub-phase starts. No single exit
condition for "hardware expansion" as a whole — that would be false
precision, same reasoning the original roadmap used to leave this coarse.

### Scope boundary

Resist scope pressure to support "common" hardware broadly. One real,
working USB mass storage path is worth more than five half-working ones.

---

## Phase 15: Immutable A/B Updates and Recovery Mode

### Purpose

OS-image-level recovery — can the system boot a previous known-good image —
distinct from Phase 7/10's crash recovery, which is object-store-level.
Don't conflate the two.

### Preconditions

Phase 13 (networking, for update transport) and Phase 14 (enough hardware
support that "the machine" means something beyond QEMU) both reasonably
mature. Phase 12 (packaging) informs what "an update" actually updates.

### Locked slice sequence

`dual-partition-layout` → `image-signing-and-verification` (do not skip —
an update mechanism without signature verification is a remote-code-
execution vector, and this is the first phase where PythOS has real network
exposure to make that matter) → `atomic-switch` → `automatic-rollback-on-
boot-failure` → `recovery-mode-boot-path` (a minimal, always-bootable
fallback independent of the A/B slots themselves).

### Exit condition

A deliberately corrupted/failing update triggers automatic rollback to the
last known-good image without manual intervention, verified by an automated
test that corrupts an update and confirms recovery.

### Scope boundary

No delta/incremental updates — full image swap only, at this phase.

### Required artifacts

ADR for the signing/trust model — this is a security-critical decision that
needs the same rigor as Phase 8's syscall ABI.

---

## Phase 16: SMP

### Purpose

Multiple CPU cores. Last, deliberately, because every phase from 2 through
15 was designed and tested single-core first. This phase is a correctness
re-audit of all of them, not a feature bolt-on — the same category of work
as Phase 8's isolation migration, but broader.

### Preconditions

Every prior phase in this document reproducible. This is not a phase to
start with anything else in flight.

### Locked slice sequence

1. **`ap-startup`** — bring up additional CPU cores (INIT-SIPI-SIPI
   sequence), each running PythCore's existing single-core init path first,
   proving the boot path itself is safe to run per-core before anything
   shares state.
2. **`per-cpu-data`** — everything that was implicitly single-instance
   (scheduler ready queue, capability table locks, IPC queues) gets audited
   for per-CPU vs. shared state, explicitly, one subsystem at a time.
3. **`spinlocks-and-atomics`** — replace any single-core assumption of
   "interrupts disabled = exclusive access" (which Phase 2's scheduler
   likely relies on) with real cross-core synchronization primitives.
4. **`multi-core-scheduler`** — extend Phase 2's round-robin to distribute
   across cores without breaking the existing preemption/termination
   proofs.
5. **`per-subsystem-smp-audit`** — go through Phase 3 (IPC/capabilities),
   Phase 7/10 (storage), Phase 8 (isolation), and Phase 13 (networking) one
   at a time, re-running each phase's original adversarial/negative test
   suite under multi-core execution, not just re-running it single-core on
   an SMP-capable kernel.

### Exit condition

Every phase from 2 through 15's original negative/adversarial test suite
passes when re-run under real multi-core execution — this phase's exit
condition is explicitly "nothing regressed," not a new capability marker.

### Scope boundary

No NUMA-awareness, no core-affinity policy beyond basic distribution, no
per-core power management (that's Phase 14 territory once cores exist).

### Required artifacts

A per-subsystem SMP audit document, one section per phase re-verified, with
explicit sign-off that phase's original proofs still hold — this is the
closest thing in the whole roadmap to a formal audit trail, appropriate
given this phase touches everything.

## Vision (Non-Binding, Explicitly Out of Phase 1-8 Critical Path)

Long-range product direction, captured while fresh, deliberately kept out of
the numbered roadmap:

```text
docs/vision/patch.md
docs/vision/causal-lens.md
docs/vision/open-surface.md
```

Rules for these files:

* They may be read, discussed, and refined at any time.
* They may never justify adding code to `core/`, `boot/`, or `shared/` ahead of
  the phase that would naturally produce it.
* Each Architectural Test (Non-Binding) callout above is the only sanctioned
  link between active roadmap work and these documents. Anything beyond
  answering that single question, at that phase, is scope creep.
* If a vision document's requirements turn out to demand a change to an
  already completed phase's ABI, for example Phase 3 capability token format
  turning out too weak once Phase 8 reevaluates it for agent scoping, that
  becomes a new ADR and a new phase, not a retroactive edit.

## Parking Lot

Datacenter capability brokering, remote workload orchestration, and cluster or
tenant policy integration are long-term research directions only. They must not
alter Milestone 1.5, Phase 2, or Phase 3 scope. Revisit only after local IPC
and kernel-enforced capabilities are implemented, tested, and boring. In
practice, that means no earlier than the Networking later-phase above, and
likely later than that.

## Deferred Indefinitely (Not Scheduled)

Broad laptop compatibility, Wi-Fi, Bluetooth, accelerated 3D graphics, Windows
or Linux binary compatibility, POSIX completeness, web browser, cloud account
system, package marketplace, unrestricted AI control, voice recognition,
hibernation, production secure boot, and full formal verification.

## Cross-Phase Standing Rules

These apply to every phase above and are restated from `AGENTS.md` for a single
point of reference in this file:

1. Implement only the active milestone or slice. Do not prebuild a later
   phase's infrastructure because it is convenient.
2. At a phase boundary, halt and report after the final slice passes. Do not
   begin the next phase's first slice without explicit re-invocation.
3. Do not invent or silently change an ABI. Every ABI-relevant decision, such
   as boot-info layout, capability token format, syscall numbers, or on-disk
   object format, gets an ADR before or with the slice that introduces it.
4. Every `unsafe` block documents: invariant, who established it, permitted
   lifetime, pointer ownership, expected alignment, expected mapped length,
   concurrency assumptions, and consequences of violation.
5. Every milestone or phase requires an automated acceptance test. Serial
   output, or from Phase 8 onward the adversarial test harness, is the test
   oracle. A successful compile is not a successful boot. A screenshot is not
   sufficient evidence.
6. Do not land a slice that regresses any already verified marker.
7. Do not claim full security where only logical isolation exists. This is
   explicitly why Phase 8 exists as a separate, later phase from Phase 3.
8. AI remains outside the trusted core through Phase 8 and through the numbered
   Phase 9-16 roadmap unless a separate vision-layer phase is explicitly
   invoked and documented.
