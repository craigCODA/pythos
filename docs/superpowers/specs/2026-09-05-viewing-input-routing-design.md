# Viewing Input Routing Foundation Design

Date: 2026-09-05

Status: Approved architecture. Cursor activation lifetime and first-slice
activation-only behavior were explicitly approved by the user on 2026-09-05.

## Goal

Establish the first semantic boundary above decoded physical input where
relative mouse movement belongs to PythOS viewing rather than to a desktop
pointer. Relative motion normally reaches Traversal. A root/session-scoped
`Space Space Backspace Backspace` control sequence issues a global activation
command to the active Viewing session. Once activated, the Viewing-owned cursor
feature consumes relative motion and presents a four-corner focus mark.

This slice proves routing, ownership, and presentation. It does not choose a
camera model, create Project Halls or Task Halls, persist state across reboot,
or replace the normal-boot launcher.

## Approved Ownership

```text
Root / Session interaction authority
├── recognizes Space Space Backspace Backspace globally
└── dispatches ActivateCursorFeature
    └── affects the active ViewingState

ViewingState [one per current Viewing session]
├── TraversalState [default motion consumer]
└── CursorFeatureState [optional Viewing feature]
    └── activation lifetime = Viewing session

Presentation
└── renders a supplied ViewingSnapshot

USB/xHCI and PS/2
└── decode physical reports only
```

Activation scope and ownership are intentionally separate. Root/session gives
the command global reach across future projects, tasks, halls, places, and
projections. Viewing owns the cursor feature, its activation state, its focus
position, and the decision to route relative motion away from Traversal while
the feature is active.

## Locked Semantics

### Activation lifetime

`CursorFeatureState` is a field of the overall `ViewingState` for the current
session. It is not a field of Traversal, a place, a projection, a framebuffer,
or the root/session command interpreter. A future change of visible place or
projection therefore cannot implicitly deactivate it.

Session-wide does not mean durable. This slice creates no object schema, disk
record, checkpoint field, or reboot restoration contract.

### Activation sequence

The exact normalized key-down sequence is:

```text
Space Space Backspace Backspace
```

It emits `ActivateCursorFeature`. The recognizer has no time dependency and no
timeout. Mouse motion and button-state events do not reset keyboard sequence
progress. A different key-down resets progress, retaining one leading `Space`
when that key can start a new attempt.

Key releases are not part of this first contract because the current typed
input event surface exposes `KeyDown` only. Physical set-1 and set-2 release
bytes remain decoder concerns and do not reach the sequence recognizer.

### Activation only

The command is one-way and idempotent. Applying it while inactive activates
the cursor feature. Applying it again while active leaves it active.

This slice defines no deactivation command, toggle, Escape behavior, timeout,
click-away behavior, or other exit gesture. A later deactivation design
requires a separate owner decision and architecture update.

## Current Repository Boundary

The current repository has four relevant but disconnected paths:

1. `input_drivers.rs` decodes `RawInputEvent`, PS/2 packets, and bounded USB
   boot-mouse reports.
2. `input_events.rs` capability-gates raw-to-typed normalization, but names
   relative mouse movement `PointerDelta`.
3. `ps2.rs` delivers real IRQ keyboard/mouse events directly to the legacy
   launcher queue, while physical keyboard diagnostics use a separate polling
   decoder.
4. ADR 0088 terminates decoded recurring xHCI reports in an opt-in diagnostic
   summary and frozen framebuffer panel.

There is no current production Viewing service, root/session control-command
dispatcher, or normal input bus. The ring-3 Session Manager path is a PythTIG
service-lifecycle proof and does not yet own physical input or display state.
The Phase 5 launcher/window/pointer modules are compatibility evidence under
ADR 0066 and cannot be extended into the new authority.

## Approaches Considered

### Selected: pure domains plus an opt-in xHCI integration probe

Create pure normalized-input, session-control, Viewing, Traversal, and
Cursor/FocusMark units with host tests. Add a new opt-in
`viewing-input-probe` feature above ADR 0088. The integration probe feeds each
decoded USB report through the neutral input seam, proves one pre-activation
motion routes to Traversal, accepts the global keyboard activation sequence,
then proves later motion routes to the cursor feature and renders the focus
mark.

This is selected because it proves the full ownership chain on the already
accepted physical xHCI transport without changing xHCI driver semantics or
cutting over normal boot before a production input/session service exists.

### Rejected for this slice: replace the normal launcher loop

Changing `launcher_screen::run_until_click` would immediately affect default
boot, retain the legacy PS/2 queue, and mix the new authority with the
superseded click-to-launch flow. It would also require deciding how Viewing
survives the irreversible ring-3 entry. That is a later cutover boundary.

### Rejected: teach the xHCI driver about Viewing

Having `usb_xhci_driver.rs` emit traversal or cursor operations would make a
transport driver own policy. The driver and ADR 0088 report decoder must remain
unchanged in meaning. Only the opt-in integration orchestrator may pass a
decoded report upward.

## Normalized Input Contract

`RawInputEvent::MouseMoved` remains the transport-neutral raw envelope shared
by PS/2 and USB. The typed event above it becomes
`InputEventKind::RelativeMotion(RelativeMotion)` rather than `PointerDelta`.
`RelativeMotion` preserves the signed `i8` axes proven by the current devices
without assigning screen, camera, path, or place meaning.

The existing Phase 5 input-service marker and capability checks remain
unchanged. `PointerButton { left }` remains as a compatibility event for the
legacy launcher but is not consumed by Viewing and gains no click semantics.
The USB auxiliary byte remains raw evidence and gains no scroll or zoom
meaning.

The repeated set-1/set-2 make/release parsing currently duplicated by the
physical input diagnostic and physical keyboard console is extracted into one
`PhysicalKeyboardDecoder`. It emits `RawInputEvent::KeyPressed`, after which
the existing typed normalization produces `KeyDown`.

## Root / Session Control Contract

`SessionControlInterpreter` consumes normalized input events and emits only
root/session-scoped semantic commands. Its first command is:

```rust
SessionControlCommand::ActivateCursorFeature
```

The interpreter does not mutate Viewing state, process mouse coordinates,
render, or know about USB/PS2 transports. The active session owner dispatches
the command to `ViewingState::apply_session_command`.

The first diagnostic hosts one interpreter for its entire run. This proves
command semantics without falsely claiming the current PythTIG Session Manager
already implements a normal input service.

## Viewing Domain

The new parent module is `core/src/viewing/` because the approved domain has
multiple cohesive responsibilities and the existing flat input and rendering
modules do not own them.

`ViewingState` owns:

- one `TraversalState`;
- one `CursorFeatureState` whose lifetime is the Viewing session;
- the neutral extent used to bound screen-relative focus position.

`ViewingState::route_relative_motion` is the only mode-routing authority:

```text
cursor inactive -> TraversalState consumes RelativeMotion
cursor active   -> CursorFeatureState consumes RelativeMotion
```

The method returns a typed route outcome so diagnostics and future consumers
can observe which branch accepted an event without inspecting internal state.

`TraversalState` records only the count and most recent neutral relative-motion
intent in this foundation. Those fields prove routing; they are not a camera,
orientation, position, path, or place model.

`CursorFeatureState` owns activation and the screen-relative focus position.
Its inactive initial position is the center of the supplied Viewing extent.
Motion uses saturating signed arithmetic and clamps the center to the extent.
It exposes no deactivation operation.

## Presentation Contract

Viewing exposes a read-only `ViewingSnapshot`. The snapshot contains an
optional focus-mark position only while the cursor feature is active.
Presentation never reads physical devices or the activation sequence.

The framebuffer diagnostic renderer draws four separated L-shaped corners
around an empty center. It does not reuse the legacy arrow sprite and does not
draw a dot or crosshair. Shape dimensions and color belong to presentation;
Viewing owns only the focus position and visibility state.

The renderer clips safely at framebuffer edges. Host framebuffer tests verify
the four corners, empty center, separated arms, inactive no-op, and bounded
edge behavior.

## Opt-In Integration Probe

Add `viewing-input-probe` as an opt-in feature depending on
`usb-xhci-boot-mouse-recurring-probe`. Standalone ADR 0088 builds remain
byte-for-byte equivalent in behavior except for source refactoring that is
covered by their existing acceptance oracle.

The integration sequence is:

1. Configure the existing recurring xHCI boot-mouse session.
2. Accept decoded reports exactly as ADR 0088 does.
3. Route the first non-zero relative-motion report to inactive Viewing and
   require the Traversal outcome.
4. With no transfer in flight, initialize keyboard-only PS/2 polling and show
   the global activation prompt.
5. Feed decoded key-downs to `SessionControlInterpreter` until the exact
   activation command is emitted. Sequence progress has no timeout.
6. Dispatch the command to the same session-wide `ViewingState`.
7. Resume recurring xHCI capture. Route later non-zero motion to the active
   cursor feature and redraw the focus mark from a Viewing snapshot.
8. Preserve ADR 0088's exact sixteen-report, ring-wrap, button-state, raw
   auxiliary, and no-write evidence.
9. Require at least one Traversal-routed motion and at least one cursor-routed
   motion before terminal success.

Zero-motion button reports continue contributing to ADR 0088 state evidence
but do not create relative-motion intents. Button transitions remain raw
states, not clicks.

## Evidence Contract

New COM1 markers live above the xHCI marker namespace:

```text
PYTHOS:CORE:VIEWING:TRAVERSAL_RELATIVE_MOTION
PYTHOS:CORE:SESSION_CONTROL:CURSOR_ACTIVATION_READY
PYTHOS:CORE:SESSION_CONTROL:CURSOR_ACTIVATED
PYTHOS:CORE:VIEWING:CURSOR_RELATIVE_MOTION
PYTHOS:CORE:VIEWING:FOCUS_MARK_X=<decimal>
PYTHOS:CORE:VIEWING:FOCUS_MARK_Y=<decimal>
PYTHOS:CORE:VIEWING:FOCUS_MARK_READY
PYTHOS:CORE:VIEWING_INPUT_PROBE_READY
```

The decimal focus coordinates are emitted from the final supplied Viewing
snapshot immediately before `FOCUS_MARK_READY`. The QEMU oracle must
additionally require the existing ADR 0088 terminal summary,
`NO_DISK_WRITES`, `USB_XHCI_PROBE_READY`, `QEMU_OUTCOME success`, and an exact
PPM focus-mark shape at those coordinates. It must reject focus motion before
activation, Traversal motion after activation, arrow/crosshair center pixels,
panic, timeout, storage writes, or missing lower-layer evidence.

QEMU acceptance proves the emulated integration only. Physical acceptance
requires a separately approved USB deployment, explicit Lenovo instructions,
and a photo/video or serial record. No physical claim is made from the QEMU
run.

## Failure Handling

The integration layer reports typed failures for keyboard initialization,
normalization, wrong routing, missing Traversal evidence, missing post-
activation cursor motion, and invalid Viewing extent. xHCI failures remain the
existing typed ADR 0088 failures.

A failure suppresses `VIEWING_INPUT_PROBE_READY` and overall probe readiness,
emits a stable COM1 failure marker, renders a failure status where framebuffer
metadata remains valid, and performs no storage write.

## Non-Goals

- Project Hall, Task Hall, place graph, or project/task navigation semantics.
- Yaw, pitch, camera transforms, depth, zoom, scroll, or continuous locomotion.
- Default normal-boot cutover or replacement of the compatibility launcher.
- A conventional arrow cursor, dot, crosshair, desktop, compositor policy, or
  windowing system.
- Click, double-click, chord, drag, selection, or button-action semantics.
- Cursor deactivation, toggle, Escape, timeout, click-away, or another exit
  gesture.
- Durable cursor activation across reboot.
- IRQ-driven xHCI input, USB hub support, trackpad support, hot-unplug recovery,
  or a generic HID stack.
- USB mass-storage or evidence-volume writes.
