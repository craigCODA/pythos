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

## Phase 7: Persistent Object Storage - IN PROGRESS

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
4. `checksums-and-commit-markers` - every committed write is checksummed and
   marked. A torn write is detectable, not silently accepted.
5. `crash-recovery` - replay/rollback logic that reconstructs a consistent
   state from the journal after an unclean shutdown. This is the actual
   recovery work that was correctly deferred earlier in the project's history.
   It belongs here, not before persistent state exists to recover.
6. `typed-object-format` - on-disk representation for typed objects with a
   stable id, kind, and versioned fields. This is the concrete implementation
   of the typed-object concept referenced throughout the vision docs. Build it
   as real OS storage infrastructure, evaluated on its own merits for
   durability and versioning correctness, not as a Causal Lens feature.
7. `object-relationships` - typed, queryable relationships between objects,
   for example blocks, created-by, and depends-on, stored alongside the objects
   themselves.
8. `revision-history` - objects retain prior versions on write, not just
   current state, with enough metadata, timestamp and writer service identity
   from Phase 3, to reconstruct what changed and who changed it.
9. `workspace-objects` - first concrete typed-object kind representing a saved
   shell layout or session. This ties back to Phase 5 window objects finally
   getting persisted.
10. `object-browser` - a Phase 5 application exposing the object store for
    inspection: list, view relationships, and view revision history. This is
    the first user-facing consumer of this data, deliberately minimal.
11. `save-and-restore-across-reboot` - end-to-end proof: create objects,
    reboot, confirm identical state including relationships and revision
    history is recoverable.

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

### Architectural Test (Non-Binding)

When typed objects are built, ask whether they carry enough provenance for the
Causal Lens.

Answer by checking whether `revision-history`, `object-relationships`, and the
writer-identity metadata are sufficient to answer a "why does this exist and
what does it block" query in principle, without building the query API or the
Causal Lens UI itself.

## Phase 8: Real Hardware Isolation - NOT STARTED

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

1. `ring-3-execution` - user-mode execution capability for tasks.
2. `separate-address-spaces` - per-task or per-service page-table isolation, a
   real extension of Phase 1.5 single kernel address space work.
3. `syscall-entry` - a defined syscall gate, for example `syscall`/`sysret`,
   replacing direct kernel-mode function calls for the Phase 3 IPC/capability
   primitives and Phase 4 `system.*` surface.
4. `user-stacks` - guarded per-task user-mode stacks, separate from Phase 2
   kernel stacks.
5. `service-local-python-runtimes` - each Python service interpreter instance
   runs in its own address space, not sharing interpreter state with other
   services by default.
6. `guarded-shared-memory` - Phase 3 shared-memory capability reverified under
   ring-3 plus separate-address-space constraints.
7. `process-termination` - a user-mode task or process can be forcibly
   terminated by the kernel without cooperation, cleanly reclaiming its address
   space.
8. `memory-quotas`, `cpu-quotas` - per-service resource limits enforced by the
   kernel, not self-reported.
9. `crash-containment` - a user-mode fault, bad pointer, or illegal instruction
   terminates only the faulting service, diagnosed through the Phase 1.5
   exception path, never the kernel.
10. `capability-enforcement-at-boundary` - every Phase 3 capability check is
    now enforced at the syscall gate itself, not just in cooperating service
    code. This is the actual hostile-code boundary this phase exists to build.

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

### Architectural Test (Non-Binding)

When capabilities are built, reevaluate post-Phase-8: are they strong enough to
safely scope an agent, now that enforcement is hardware-backed rather than
cooperative?

This is a re-ask of the Phase 3 question, now answerable for real because Phase
3's answer was necessarily provisional due to kernel-mode-only enforcement.

## Later Phases

These are unordered among themselves and all after Phase 8. They are described
at the same level of detail as the original roadmap intentionally; sequencing
them precisely now would be false precision. Each needs its own full-detail
roadmap section written when it becomes the active phase, following this file's
format.

* Applications and package installation.
* Immutable A/B updates, rollback, and recovery mode. This is OS-image-level
  recovery, meaning whether the system can boot a previous known-good image,
  distinct from Phase 7 crash recovery, meaning whether the object store can
  recover from a torn write. Do not conflate the two when this phase is
  detailed.
* Networking stack: DNS, TCP, secure update transport. This is also the
  earliest legitimate point to revisit the parked datacenter and
  capability-brokering idea, and even then only as a research branch, not a
  roadmap phase, until there is a concrete reason to promote it.
* Semantic indexing, natural-language command system, optional isolated local
  AI. This is the first phase where AI as a system-level concept legitimately
  enters the kernel-adjacent roadmap. Patch is downstream of this, not a
  substitute for it.
* Physical hardware support: USB, NVMe, Ethernet, audio beyond Phase 6 QEMU
  target, and power management.
* SMP: multiple CPU cores. Deliberately last; every phase from 2 onward,
  scheduler, IPC, capabilities, and syscalls, was designed and tested
  single-core first. SMP is a correctness re-audit of all of them, not a
  feature bolt-on.

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
8. AI remains outside the trusted core through Phase 8. This is reevaluated, not
   assumed, at the Phase 8 architectural test and again whenever Semantic
   Indexing or Local AI becomes active in Later Phases.
