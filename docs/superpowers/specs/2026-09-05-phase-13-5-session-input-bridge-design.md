# Phase 13.5 Session Input Bridge Design

Date: 2026-09-05

Status: Proposed written design for owner review. The owner approved the
architectural direction on 2026-09-05; this document does not authorize
implementation, default-boot cutover, deployment, or physical claims.

## Goal

Create the smallest honest production boundary between PythCore's decoded
physical-input mechanisms and the future root/session owner of Viewing.

ADR 0089 already establishes the semantic result above this boundary:

```text
root/session control authority
    -> global ActivateCursorFeature command
    -> session-owned ViewingState

ViewingState
    -> Traversal by default
    -> Cursor/FocusMark while active
```

What is missing is not more cursor behavior. It is a capability-gated,
device-neutral way for one active ring-3 session consumer to receive recurring
normalized input without reading PS/2 ports, xHCI structures, or framebuffer
state. This design establishes that mechanism first and keeps the accepted
Viewing policy downstream.

## Current Repository Reality

The live repository does not yet have a persistent production session input
owner:

- `core/src/ps2.rs` decodes IRQ1/IRQ12 input into a fixed raw-event queue. Its
  only normal-boot consumer is the pre-ring-3 compatibility launcher.
- `core/src/input_events.rs` owns device-neutral normalization and a
  capability-shaped synchronous proof, but it is not a retained queue or a
  user-boundary delivery service.
- `core/src/session_controls.rs` and `core/src/viewing/` implement ADR 0089's
  pure semantics. They are compiled for host tests and the opt-in Viewing
  probe, not owned by a normal session runtime.
- `programs/session-manager/main.pyth` consumes one bounded `PythCommand` and
  returns. The current supervisor records lifecycle decisions but does not
  schedule a continuously running Session Manager.
- default `pyth-tig-default` boot admits package/service manifests, then still
  runs the compatibility launcher and irreversibly enters the legacy Rust
  shell.
- the PythTIG version-1 instruction and host-op surfaces are frozen. They have
  no physical-input receive operation.
- ADR 0088 and ADR 0089 are bounded opt-in probes. They prove recurring xHCI
  decode and Viewing routing respectively; neither is a normal input bus.

Therefore a direct call from the launcher, PS/2 IRQ handler, or xHCI probe into
`ViewingState` would create a false session authority in PythCore. Making the
current one-shot Session Manager loop forever would also exceed its accepted
runtime, budget, and supervision contracts.

## Locked Ownership

```text
PythCore hardware mechanisms
├── PS/2 IRQ decode today
├── future production USB/HID decode outside this slice
└── publish decoded RawInputEvent
    └── device-neutral bounded input queue
        └── SessionInputStream mechanism
            ├── normalizes device-neutral input
            ├── exposes one capability-gated receive syscall
            └── reports discontinuities without interpreting them

Root / Session interaction authority [later consumer]
├── holds the sole active SessionInputStream capability
├── recognizes Space Space Backspace Backspace globally
└── dispatches ActivateCursorFeature

ViewingState [ADR 0089]
├── TraversalState [default]
└── CursorFeatureState [Viewing-session lifetime]

Presentation [later bridge]
└── renders supplied Viewing state only
```

PythCore owns queueing, isolation, copy-out safety, and hardware-neutral event
encoding because those are privileged mechanisms. It does not recognize the
activation sequence, select Traversal versus Cursor, move a focus mark, or
render.

Queue ownership moves out of `ps2.rs` into the device-neutral session-input
mechanism. A PS/2 handler publishes `RawInputEvent`; it does not expose a
PS/2-specific receive API to the session syscall. This gives a later
production USB input path the same publication seam without reopening the
session ABI.

The eventual root/session consumer owns interpretation. Its global scope does
not make the cursor feature root-owned; the command affects the
Viewing-session-owned `CursorFeatureState` exactly as ADR 0089 specifies.

## Approaches Considered

### Selected: contract-first staged convergence

Add a stable session-input ABI and capability-gated nonblocking receive seam.
Prove it with a dedicated opt-in ring-3 consumer before connecting it to a
long-lived Session Manager or default boot. Subsequent slices bind the real
session owner, ADR 0089 routing, presentation, and only then normal boot.

This separates a missing privileged mechanism from unresolved service-runtime
policy. It also gives later Python/Pyth session work a real input boundary
without changing the frozen PythTIG version-1 host-op set merely for a probe.

### Rejected: kernel-owned Viewing loop

Polling `ps2::poll_event()` and mutating `ViewingState` in `normal_boot.rs`
would either block before ring 3 or duplicate root/session authority inside
the kernel. It would also tempt presentation to consume hardware directly.

### Rejected: extend the legacy Rust shell

The shell and launcher are compatibility paths under ADR 0066. Giving them
session-wide Viewing authority would make the migration target depend on the
component it is intended to supersede.

### Rejected for the first slice: add a PythTIG input host op

That would change the frozen instruction/ABI surface and require coordinated
compiler, verifier, shared-format, runtime, and fixture work plus a separate
accepted ADR. A bounded syscall seam can be proven without making that larger
decision. The eventual Session Manager integration must still decide whether
to introduce a new PythTIG version or use bounded supervised invocations over
another service contract.

### Rejected: extend xHCI or PS/2 with Viewing policy

Hardware drivers may emit decoded physical state only. They must not know the
activation sequence, Project Halls, Traversal, focus position, or rendering.

## Phase 13.5 Slice Boundaries

Phase 13.5 is staged so each authority change can be reviewed and accepted
independently:

1. **Session input contract and ring-3 delivery proof.** Add the ABI, explicit
   queue-discontinuity evidence, exclusive capability, syscall, and opt-in
   ring-3 probe described in detail below. Do not route Viewing yet.
2. **Persistent root/session consumer.** Decide and implement how the Pyth
   Session Manager is repeatedly invoked or retained so one session authority
   owns input interpretation and non-durable session state. This requires its
   own accepted runtime decision.
3. **ADR 0089 production routing.** Bind one `SessionControlInterpreter` and
   one `ViewingState` to that session lifetime. Preserve one-way activation,
   default Traversal, and exclusive motion routing.
4. **Presentation bridge.** Deliver read-only Viewing snapshots to
   presentation outside interrupt context. Presentation draws the FocusMark
   but never reads devices or interprets controls.
5. **Normal-boot cutover.** Replace the compatibility launcher/shell path only
   after the session, input, Viewing, presentation, failure, and recovery
   contracts have independent acceptance evidence.

Only Slice 1 is sufficiently specified for the next implementation plan. The
later slices are architectural sequence, not implementation authorization.

## Slice 1: Session Input Contract

### Shared ABI

A new shared, versioned session-input ABI owns the wire representation. It is
not part of the object-shell ABI and does not use PythTIG opcodes.

The contract uses:

```text
SESSION_INPUT_ABI_MAJOR = 1
SESSION_INPUT_ABI_MINOR = 0
SYSCALL_SESSION_INPUT_TRY_READ = 0x5059_0150
```

Adding the syscall is a backward-compatible general syscall-registry extension,
so the registry reports version 1.1 and records this entry as introduced in
1.1. Existing syscall numbers and meanings remain unchanged.

The existing input-stream resource identity `0x1A50_0100` and `INPUT` right
remain authoritative. A new parallel resource identity must not be invented.
The capability handle is opaque and generation-checked under the existing
`PackedCapability` discipline. Because that wire type currently lives under
`object_shell_abi`, Slice 1 extracts it to a shared capability-ABI module and
re-exports it from `object_shell_abi` for source compatibility. Session input
must not acquire a dependency on the legacy shell merely to reuse the handle
layout.

`SessionInputEventV1` is a fixed 40-byte, little-endian, `repr(C)` value:

```text
offset  size  field
0       8     sequence: u64
8       2     kind: u16
10      2     source: u16
12      4     flags: u32
16      4     value0: i32
20      4     value1: i32
24      8     reserved0: u64
32      8     reserved1: u64
```

The initial tags are:

```text
source Keyboard = 1
source Mouse = 2

kind KeyDown = 1
kind RelativeMotion = 2
kind MouseButtonState = 3

flag GAP_BEFORE = 0x00000001
```

All unknown flags and both reserved fields are zero on output. Logical key
codes become explicit shared numeric tags rather than Rust's implicit enum
layout: `A..Z` are `0x0001..0x001A`, `Digit0..Digit9` are
`0x0020..0x0029`, and Enter, Escape, Space, and Backspace are
`0x0030..0x0033`. These are PythOS logical-key values, not PS/2 scancodes or
USB usages.

Event payloads are:

```text
KeyDown          value0 = logical key tag; value1 = 0
RelativeMotion   value0 = signed dx; value1 = signed dy
MouseButtonState value0 = left state 0 or 1; value1 = 0
```

`MouseButtonState` preserves the current compatibility state transition. It
does not mean click, double-click, selection, activation, drag, or focus.
Right/middle buttons and the USB auxiliary byte remain outside this ABI until
a separate device-neutral input decision exists.

### Receive syscall

The nonblocking syscall shape is:

```text
try_read(input_capability, output_pointer, output_length, 0, 0)
```

It returns one of the session-input ABI's stable results:

```text
EVENT = 0x5059_004F        one event was copied out
EMPTY = 0x5059_0150_0000   no event was available; output remains unchanged
```

Existing general syscall error encoding continues to represent unsupported,
denied, malformed, or unsafe calls. The implementation must validate, in this
order, the active caller, exact capability resource/right/holder, exact output
length, reserved arguments, natural alignment, and writable user mapping
before it removes an event from the queue. A rejected call cannot consume an
event or modify the output buffer.

The call is deliberately nonblocking because the repository has no accepted
sleep/wakeup scheduler contract for a waiting session service. The opt-in
probe may poll. A production session wait primitive is a later runtime slice,
not an IRQ spin loop or a silent addition to this syscall.

### Exclusive subscriber

Exactly one active session-input capability may consume the stream. Slice 1
grants it only to the dedicated probe in the opt-in boot profile. Default
shell, package graphs, unrelated services, and forged or stale holders receive
no input authority.

The mechanism is exclusive rather than broadcast because ADR 0089 defines one
active root/session interpreter and one active Viewing session. Multi-seat,
observers, event duplication, and subscription revocation are separate future
decisions.

The probe receives the stream capability through a probe-only bootstrap
wrapper. Its evidence-console capability remains explicitly probe-only and is
not a field of the durable session-input ABI.

## Queue and Discontinuity Contract

The current PS/2 queue has 16 storage slots, 15 usable entries under its
head/tail sentinel, and drops the newest event when full. Slice 1 preserves
that bounded, allocation-free, non-overwriting policy but makes loss visible.

The extracted queue has exactly one consumer mode. Default boot retains a
named compatibility dequeue used by the existing launcher. The opt-in Slice 1
profile binds the exclusive session stream instead and does not run the
launcher. Once a session stream is bound, the compatibility dequeue is denied;
the same events can never race between two consumers. Slice 1 defines no live
handoff between those modes.

Every decoded candidate event receives a wrapping `u64` sequence number before
enqueue. Dropped events consume sequence numbers. The single consumer compares
each delivered number with the wrapping successor of the prior delivered
number. A mismatch sets `GAP_BEFORE` on that event.

When the exclusive stream is bound, queued pre-binding input is discarded and
the consumer baseline is set to the producer's next sequence. Binding must be
performed without an interrupt-visible half-state. This prevents stale boot
keystrokes from becoming root/session commands.

The producer remains interrupt-safe: no allocation, blocking, normalization,
syscall dispatch, capability lookup, rendering, or logging occurs in IRQ
context. Sequence assignment and the fixed-ring push are the only additions to
the top half.

The later session interpreter must reset any partial activation-sequence
progress before processing an event marked `GAP_BEFORE`. It must not claim an
exact `Space Space Backspace Backspace` match across unknown lost input. Slice
1 proves the flag but does not yet run that interpreter.

Sequence wrap uses wrapping addition; equality against the wrapping successor
remains the continuity test. Queue-full behavior stays drop-newest so unread
events are never overwritten.

## Ring-3 Integration Probe

An opt-in `session-input-bridge-probe` boot profile launches a dedicated
minimal ring-3 evidence consumer. It is not the Session Manager, shell,
Viewing owner, or a new permanent native policy service.

The probe path is:

```text
QEMU input injection
    -> PS/2 IRQ1/IRQ12 decode
    -> device-neutral sequence-stamped bounded raw queue
    -> PythCore session-input try-read syscall
    -> existing device-neutral normalization
    -> SessionInputEventV1 copy-out
    -> capability-holding ring-3 probe
    -> probe-only COM2 acknowledgement
```

The deterministic live test injects the four key-downs Space, Space,
Backspace, Backspace and then one non-zero relative motion. The ring-3 probe
must acknowledge the five ordered normalized events and their continuous
sequence numbers. It deliberately does not recognize the sequence, issue
`ActivateCursorFeature`, instantiate `ViewingState`, move a FocusMark, or
render.

The ring-3 probe first submits a forged handle and demonstrates that the call
cannot consume or mutate the next event, then uses its real stream capability.
Wrong-holder and stale-generation cases are deterministic host/core tests;
they do not require a nonexistent multi-process scheduler. Queue-full and
sequence-gap behavior are also host/core tests. The QEMU acceptance path
requires no gap for its five-event delivery sequence.

The probe emits terminal readiness only after PythCore observes the authorized
syscall path and the ring-3 consumer acknowledges all expected values. It also
retains `NO_DISK_WRITES`. COM1 and COM2 are separate evidence channels; the
oracle must require both and must reject a kernel-only transcript that lacks
the ring-3 acknowledgement.

## Evidence and Verification

Slice 1 requires red/green tests for:

- exact ABI size, alignment, version, constants, zeroed reserved fields, and
  all logical-key mappings;
- normalizing every current `InputEventKind` into its wire event without
  transport or Viewing semantics;
- exclusive correct-holder delivery;
- forged, stale, wrong-holder, and missing capability denial;
- denial before dequeue and before output mutation;
- exact output length, alignment, reserved-argument, and user-copy-map checks;
- nonblocking `EMPTY` with unchanged output;
- continuous event sequencing;
- fixed-queue drop-newest behavior and `GAP_BEFORE` on the first later
  delivered event;
- clean wrapping-sequence continuity;
- binding flushes stale pre-session input;
- no queue access, normalization, rendering, or control interpretation from an
  IRQ handler;
- the opt-in ring-3 QEMU sequence described above;
- unchanged default normal boot, milestone boot, persistent storage, ADR 0089
  Viewing probe, and strict Clippy profiles.

The implementation plan must use the repository's existing QEMU/QMP and COM2
helpers rather than create a second runner. Static oracle self-tests must prove
that missing, duplicated, out-of-order, kernel-only, wrong-value, gap-bearing,
panic, timeout, or disk-write transcripts cannot pass.

QEMU acceptance proves emulated PS/2 IRQ-to-ring-3 delivery only. It does not
prove physical Lenovo input, production USB input, xHCI interrupt delivery,
or default session ownership.

## Failure Handling

- Queue full drops the newest event, preserves unread events, and creates a
  later observable sequence gap.
- `EMPTY` is normal flow and emits no failure marker.
- Bad capability, ABI shape, reserved argument, alignment, or user mapping is
  denied without consuming input.
- Normalization failure suppresses probe readiness and does not fabricate an
  event.
- Probe mismatch, discontinuity, unexpected event kind/value, panic, or timeout
  suppresses terminal readiness.
- No failure path writes storage or falls back to direct hardware access.

## Non-Goals

- Cursor/FocusMark activation, motion, deactivation, or rendering in Slice 1.
- A toggle, Escape, timeout, click-away, or any other deactivation behavior.
- Traversal, camera, yaw/pitch, path, place, Project Hall, or Task Hall
  semantics.
- Click, double-click, chord, drag, selection, wheel, scroll, or zoom
  semantics.
- Normal-boot launcher/shell replacement.
- A generic desktop, compositor authority, windowing system, or pointer model.
- A PythTIG version change or new host opcode in Slice 1.
- Production USB HID delivery, xHCI IRQ mode, hubs, trackpads, hot unplug, or
  multi-device arbitration.
- Multiple subscribers, multi-seat input, stream revocation, blocking waits,
  SMP, or durable input/session state.
- USB deployment, disk writes, or physical acceptance.

## Preserved Decisions and Later Decisions

The following remain locked from ADR 0089:

- activation lifetime is the overall current `ViewingState` session;
- session-wide does not mean durable across reboot;
- the exact sequence is activation-only, one-way, and idempotent;
- Traversal is the default relative-motion consumer;
- Cursor/FocusMark is a subordinate Viewing feature;
- Presentation renders supplied Viewing state only.

The following remain deliberately outside Slice 1 and require explicit later
decisions:

- whether the Session Manager becomes persistent through a new PythTIG version,
  bounded supervised reinvocation, or another accepted service-runtime
  contract;
- the production session wait/wakeup primitive;
- the presentation-state transport and normal-boot cutover;
- any cursor deactivation behavior;
- physical-device acceptance for the resulting production path.

## Slice 1 Acceptance Boundary

Slice 1 is complete only when an exclusively authorized ring-3 probe receives
the exact normalized keyboard and relative-motion values produced by the real
QEMU PS/2 IRQ path through the versioned syscall ABI, while all isolation,
copy-out, nonblocking, queue-loss, regression, and no-write oracles pass.

That result establishes a trustworthy session-input mechanism. It does not yet
establish a production Session Manager, activate the cursor feature, route
Traversal, render a FocusMark, cut over normal boot, or prove physical input.
