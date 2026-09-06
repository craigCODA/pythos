# Viewing Input Routing Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a device-neutral relative-motion contract, a root/session-scoped one-way cursor activation command, a session-owned Viewing state that routes motion to Traversal by default and Cursor/FocusMark while active, and an opt-in xHCI/QEMU proof that renders the four-corner focus mark.

**Architecture:** Preserve ADR 0088 as the decoded transport boundary. Normalize device motion as RelativeMotion, recognize the global keyboard sequence in a root/session control interpreter, dispatch its one-way activation command into one ViewingState, and let Viewing alone select Traversal or Cursor/FocusMark as the motion consumer. Prove the boundary through a new opt-in viewing-input-probe layered above the existing recurring xHCI probe; do not cut over default normal boot.

**Tech Stack:** Rust no_std PythCore, existing capability-gated input normalization, bounded PS/2 polling, ADR 0088 xHCI recurring interrupt transfers, GOP framebuffer presentation, COM1 evidence markers, Python 3 QMP/QEMU acceptance, Cargo host and cross-target tests.

**Spec:** docs/superpowers/specs/2026-09-05-viewing-input-routing-design.md

## Global Constraints

- Plan baseline is D:\PythOS-Workspace\repo\pythos\.worktrees\hw-white-screen-diagnostic, branch agent/hw-white-screen-diagnostic, commit e9bf9689d66bb98e7d5ae878d4d0452780c19a98.
- At execution time, use superpowers:using-git-worktrees and start agent/viewing-input-routing from a base that contains e9bf968. If e9bf968 is not on the chosen base, stop instead of silently omitting or reimplementing ADR 0088.
- Do not alter xHCI register, ring, DMA, event-consumer, transfer-count, or decode semantics. The new feature consumes UsbBootMouseReport above that boundary.
- Keep existing ADR 0088 feature-only behavior, marker order, exact 16-report contract, ring wrap, button-state evidence, auxiliary-byte evidence, and no-write result unchanged.
- Replace typed PointerDelta authority with RelativeMotion, but preserve the Phase 5 capability checks and serial markers.
- Space Space Backspace Backspace is a root/session-scoped activation sequence over normalized KeyDown events.
- Cursor activation lifetime is the overall ViewingState for the current session. It is not place/projection state and is not durable across reboot.
- Activation is one-way and idempotent. Do not add a deactivate method, toggle, Escape handling, timeout, click-away behavior, or another exit gesture.
- Traversal is the default relative-motion consumer. Cursor/FocusMark consumes relative motion only after activation.
- Viewing chooses the motion route. Root/session controls do not move the focus mark; presentation does not choose the route.
- The focus mark is four separated L-shaped corners around an empty center. Do not draw or reuse the legacy arrow cursor, a dot, or a crosshair.
- Button reports remain states. Do not add click, double-click, drag, chord, selection, or activation semantics.
- The fourth USB byte remains raw auxiliary evidence. Do not assign wheel, scroll, or zoom meaning.
- Do not add Project Hall, Task Hall, place graph, camera, yaw/pitch, depth, path-selection, or locomotion semantics.
- Do not modify normal_boot.rs, launcher_screen.rs, window_interaction.rs, workspace_objects.rs, the PythTIG Session Manager ABI, or persistent object schemas in this slice.
- Every hardware poll remains bounded where it waits for hardware completion. The key-sequence recognizer itself has no time input and never expires partial progress by time.
- Preserve COM1 as the automated oracle. Require QEMU_OUTCOME success; timeout is failure.
- Use py -3 on this Windows machine.
- No push, merge, USB deployment, or physical-support claim is part of plan execution without its separate authorization.

## File and Responsibility Map

- core/src/input_drivers.rs: shared set-1/set-2 physical-keyboard make-event decoder; existing raw input and USB boot-mouse decoder remain hardware-facing.
- core/src/physical_keyboard_console.rs: consume the shared physical-keyboard decoder and retain console-byte policy only.
- core/src/physical_input_diagnostic.rs: consume the shared physical-keyboard decoder and retain diagnostic sequence/text policy only.
- core/src/input_events.rs: define RelativeMotion and normalize RawInputEvent::MouseMoved without pointer meaning.
- core/src/session_controls.rs: recognize the global activation sequence and emit SessionControlCommand::ActivateCursorFeature.
- core/src/viewing/mod.rs: own session-wide ViewingState, command application, snapshots, and the sole relative-motion routing decision.
- core/src/viewing/traversal.rs: accept neutral relative-motion intents without camera or place semantics.
- core/src/viewing/focus_mark.rs: own cursor-feature activation and bounded screen-relative focus position without presentation pixels.
- core/src/viewing_input_probe.rs: compose decoded keyboard/mouse input, session controls, and Viewing for the opt-in proof.
- core/src/framebuffer.rs: render a supplied ViewingSnapshot as a four-corner focus mark and diagnostic status.
- core/src/main.rs: register the new pure modules and feature-gated probe module.
- core/src/ps2.rs: compile the existing bounded keyboard polling helpers for viewing-input-probe.
- core/src/usb_xhci_probe_boot.rs: feature-gated integration only; pass decoded reports upward, pause with no transfer in flight for activation, render snapshots, and enforce terminal evidence.
- core/src/usb_xhci_probe_screen.rs: describe a typed Viewing integration failure in the existing probe failure panel without changing standalone ADR 0088 output.
- core/Cargo.toml: add viewing-input-probe depending on usb-xhci-boot-mouse-recurring-probe.
- scripts/launcher_click.py: add reusable QMP helpers for the exact activation keys only; retain launcher helpers unchanged.
- scripts/test-viewing-input-probe.py: build, drive, and validate the new QEMU integration and PPM focus mark.
- docs/decisions/0089-viewing-input-routing-foundation.md: architecture, accepted semantics, evidence boundary, and status.
- docs/PythOS-TDD-001.md, docs/TECHNICAL-OVERVIEW.md, README.md: opt-in feature, marker, command, and non-goal contracts.
- D:\PythOS-Workspace\CURRENT-STATE.md: refresh only after all fresh verification; keep physical status pending until a separately approved Lenovo run.

---

### Task 1: Device-Neutral Input and Shared Keyboard Decode

**Files:**
- Modify: core/src/input_drivers.rs
- Modify: core/src/input_events.rs
- Modify: core/src/physical_keyboard_console.rs
- Modify: core/src/physical_input_diagnostic.rs

**Interfaces:**
- Produces: PhysicalKeyboardDecoder::new() and feed_raw_byte(byte: u8) -> Option<RawInputEvent>.
- Produces: RelativeMotion { dx: i8, dy: i8 } and InputEventKind::RelativeMotion(RelativeMotion).
- Preserves: RawInputEvent, InputEventService capability checks, InputEventKind::PointerButton compatibility behavior, and all existing serial markers.

- [ ] **Step 1: Write failing tests for the shared keyboard decoder**

Add tests beside input_drivers.rs proving set 1, set 2, release suppression, and exact raw-event output:

~~~rust
#[test]
fn physical_keyboard_decoder_emits_set1_key_down_events() {
    let mut decoder = PhysicalKeyboardDecoder::new();
    assert_eq!(
        decoder.feed_raw_byte(0x39),
        Some(RawInputEvent::KeyPressed {
            scancode: 0x39,
            key: KeyCode::Space,
        })
    );
    assert_eq!(
        decoder.feed_raw_byte(0x0E),
        Some(RawInputEvent::KeyPressed {
            scancode: 0x0E,
            key: KeyCode::Backspace,
        })
    );
}

#[test]
fn physical_keyboard_decoder_emits_set2_key_down_and_ignores_release() {
    let mut decoder = PhysicalKeyboardDecoder::new();
    assert_eq!(
        decoder.feed_raw_byte(0x29),
        Some(RawInputEvent::KeyPressed {
            scancode: 0x29,
            key: KeyCode::Space,
        })
    );
    assert_eq!(decoder.feed_raw_byte(0xF0), None);
    assert_eq!(decoder.feed_raw_byte(0x29), None);
    assert_eq!(
        decoder.feed_raw_byte(0x66),
        Some(RawInputEvent::KeyPressed {
            scancode: 0x66,
            key: KeyCode::Backspace,
        })
    );
}
~~~

- [ ] **Step 2: Write the failing RelativeMotion normalization test**

Replace pointer-delta expectations in input_events.rs with:

~~~rust
#[test]
fn raw_mouse_motion_normalizes_without_pointer_semantics() {
    assert_eq!(
        normalize(RawInputEvent::MouseMoved { dx: 5, dy: -3 }),
        Ok(InputEvent {
            source: InputSource::Mouse,
            kind: InputEventKind::RelativeMotion(RelativeMotion {
                dx: 5,
                dy: -3,
            }),
        })
    );
}
~~~

- [ ] **Step 3: Run focused tests and verify RED**

Run:

~~~powershell
cargo test -p pythos-core physical_keyboard_decoder -- --nocapture
cargo test -p pythos-core raw_mouse_motion_normalizes_without_pointer_semantics -- --nocapture
~~~

Expected: compile failures because PhysicalKeyboardDecoder, RelativeMotion, and the RelativeMotion event variant do not exist.

- [ ] **Step 4: Implement the shared physical-keyboard decoder**

Move the set-selection and release-prefix mechanics currently duplicated in physical_keyboard_console.rs and physical_input_diagnostic.rs into input_drivers.rs:

~~~rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PhysicalScanSet {
    Unknown,
    Set1,
    Set2,
}

pub(crate) struct PhysicalKeyboardDecoder {
    mode: PhysicalScanSet,
    release_prefix: bool,
    extended_prefix: bool,
}

impl PhysicalKeyboardDecoder {
    pub(crate) const fn new() -> Self {
        Self {
            mode: PhysicalScanSet::Unknown,
            release_prefix: false,
            extended_prefix: false,
        }
    }

    pub(crate) fn feed_raw_byte(&mut self, byte: u8) -> Option<RawInputEvent> {
        if self.consume_non_make_byte(byte) {
            return None;
        }
        let key = self.decode_make_byte(byte)?;
        Some(RawInputEvent::KeyPressed {
            scancode: byte,
            key,
        })
    }
}
~~~

Use the existing scancode_to_keycode function for set 1. Move the complete set-2 KeyCode table from physical_keyboard_console.rs into the shared decoder. Preserve the existing rule that E0-prefixed input and make-code releases are not emitted as KeyDown events.

- [ ] **Step 5: Implement the typed RelativeMotion contract**

In input_events.rs, define:

~~~rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelativeMotion {
    pub dx: i8,
    pub dy: i8,
}

impl RelativeMotion {
    pub const fn is_zero(self) -> bool {
        self.dx == 0 && self.dy == 0
    }
}
~~~

Replace InputEventKind::PointerDelta with:

~~~rust
RelativeMotion(RelativeMotion),
~~~

Update normalize, run_self_test, and local tests. Do not rename INPUT_EVENT_STREAM, change resource IDs, add a new right, or change the Phase 5 serial marker.

- [ ] **Step 6: Replace duplicate diagnostic/console decoding**

In physical_keyboard_console.rs:

- replace its ScanSetMode and decode_set2_key implementation with a PhysicalKeyboardDecoder field;
- call feed_raw_byte and match only RawInputEvent::KeyPressed;
- retain keycode_to_console_byte as console policy.

In physical_input_diagnostic.rs:

- replace its ScanSetMode, consume_non_make_byte, decode_set1_key, and decode_set2_key code with a PhysicalKeyboardDecoder field;
- normalize the returned RawInputEvent through input_events::normalize;
- retain its text buffer, WAKE suffix, framebuffer, and diagnostic acceptance rules unchanged.

- [ ] **Step 7: Run all affected tests and verify GREEN**

Run:

~~~powershell
cargo test -p pythos-core input_drivers -- --nocapture
cargo test -p pythos-core input_events -- --nocapture
cargo test -p pythos-core physical_input -- --nocapture
cargo test -p pythos-core physical_keyboard_console -- --nocapture
~~~

Expected: all pass. Existing physical-input and console fixtures must produce the same bytes, KeyCodes, and acceptance results as before.

- [ ] **Step 8: Commit the neutral input boundary**

~~~powershell
git add core/src/input_drivers.rs core/src/input_events.rs core/src/physical_keyboard_console.rs core/src/physical_input_diagnostic.rs
git commit -m "refactor: normalize device input as relative motion"
~~~

### Task 2: Root/Session Cursor Activation Command

**Files:**
- Create: core/src/session_controls.rs
- Modify: core/src/main.rs

**Interfaces:**
- Consumes: InputEvent and InputEventKind::KeyDown(KeyCode).
- Produces: SessionControlCommand::ActivateCursorFeature.
- Produces: SessionControlInterpreter::new() and observe(event: InputEvent) -> Option<SessionControlCommand>.
- Does not consume: clocks, framebuffer state, Viewing coordinates, USB reports, or PS/2 bytes.

- [ ] **Step 1: Write failing command-recognition tests**

Create session_controls.rs with tests first:

~~~rust
#[test]
fn exact_key_down_sequence_emits_global_cursor_activation() {
    let mut controls = SessionControlInterpreter::new();
    let keys = [
        KeyCode::Space,
        KeyCode::Space,
        KeyCode::Backspace,
        KeyCode::Backspace,
    ];
    for key in &keys[..3] {
        assert_eq!(controls.observe(key_event(*key)), None);
    }
    assert_eq!(
        controls.observe(key_event(keys[3])),
        Some(SessionControlCommand::ActivateCursorFeature)
    );
}

#[test]
fn relative_motion_does_not_reset_activation_progress() {
    let mut controls = SessionControlInterpreter::new();
    assert_eq!(controls.observe(key_event(KeyCode::Space)), None);
    assert_eq!(
        controls.observe(InputEvent {
            source: InputSource::Mouse,
            kind: InputEventKind::RelativeMotion(RelativeMotion { dx: 1, dy: -1 }),
        }),
        None
    );
    assert_eq!(controls.observe(key_event(KeyCode::Space)), None);
    assert_eq!(controls.observe(key_event(KeyCode::Backspace)), None);
    assert_eq!(
        controls.observe(key_event(KeyCode::Backspace)),
        Some(SessionControlCommand::ActivateCursorFeature)
    );
}

#[test]
fn unrelated_key_down_resets_but_leading_space_can_restart() {
    let mut controls = SessionControlInterpreter::new();
    controls.observe(key_event(KeyCode::Space));
    controls.observe(key_event(KeyCode::A));
    controls.observe(key_event(KeyCode::Space));
    controls.observe(key_event(KeyCode::Space));
    controls.observe(key_event(KeyCode::Backspace));
    assert_eq!(
        controls.observe(key_event(KeyCode::Backspace)),
        Some(SessionControlCommand::ActivateCursorFeature)
    );
}
~~~

Add a fourth test proving two completed sequences emit two activation commands; this tests recognizer reset without assigning toggle behavior.

- [ ] **Step 2: Run the focused tests and verify RED**

~~~powershell
cargo test -p pythos-core session_controls -- --nocapture
~~~

Expected: compile failure because the module and types do not exist.

- [ ] **Step 3: Implement the allocation-free interpreter**

Implement:

~~~rust
const CURSOR_ACTIVATION_KEYS: [KeyCode; 4] = [
    KeyCode::Space,
    KeyCode::Space,
    KeyCode::Backspace,
    KeyCode::Backspace,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionControlCommand {
    ActivateCursorFeature,
}

pub struct SessionControlInterpreter {
    activation_index: u8,
}
~~~

observe must:

1. ignore every non-KeyDown event without changing activation_index;
2. advance on the expected key;
3. emit ActivateCursorFeature and reset to zero on the fourth key;
4. on a wrong KeyDown, set progress to one only when the wrong key is Space, otherwise zero;
5. never consult a timer or emit a deactivation/toggle command.

Register mod session_controls in main.rs under cfg(any(test, feature = "viewing-input-probe")) so the new policy is absent from default production code until a later cutover.

- [ ] **Step 4: Run tests and verify GREEN**

~~~powershell
cargo test -p pythos-core session_controls -- --nocapture
~~~

Expected: all session-control tests pass, including repeated one-way activation command emission.

- [ ] **Step 5: Commit the root/session command contract**

~~~powershell
git add core/src/session_controls.rs core/src/main.rs
git commit -m "feat: define global cursor activation command"
~~~

### Task 3: Session-Owned Viewing Domain and Motion Routing

**Files:**
- Create: core/src/viewing/mod.rs
- Create: core/src/viewing/traversal.rs
- Create: core/src/viewing/focus_mark.rs
- Modify: core/src/main.rs

**Interfaces:**
- Consumes: RelativeMotion and SessionControlCommand.
- Produces: ViewingExtent::new(width: u32, height: u32) -> Result<ViewingExtent, ViewingError>.
- Produces: ViewingState::new(extent: ViewingExtent), apply_session_command(command), route_relative_motion(motion) -> MotionRoute, and snapshot() -> ViewingSnapshot.
- Produces: MotionRoute::Traversal(TraversalIntent) and MotionRoute::CursorFocus(FocusMarkPosition).
- Guarantees: CursorFeatureState is owned by ViewingState for the Viewing session and has no deactivation operation.

- [ ] **Step 1: Write failing default-routing and activation tests**

In viewing/mod.rs:

~~~rust
#[test]
fn traversal_is_the_default_relative_motion_consumer() {
    let extent = ViewingExtent::new(100, 80).unwrap();
    let mut viewing = ViewingState::new(extent);
    let motion = RelativeMotion { dx: 7, dy: -3 };

    assert_eq!(
        viewing.route_relative_motion(motion),
        MotionRoute::Traversal(TraversalIntent { motion })
    );
    assert_eq!(viewing.traversal().motion_count(), 1);
    assert_eq!(viewing.snapshot().focus_mark, None);
}

#[test]
fn activation_routes_later_motion_to_viewing_owned_cursor_feature() {
    let extent = ViewingExtent::new(100, 80).unwrap();
    let mut viewing = ViewingState::new(extent);
    viewing.apply_session_command(SessionControlCommand::ActivateCursorFeature);

    assert_eq!(
        viewing.route_relative_motion(RelativeMotion { dx: 7, dy: -3 }),
        MotionRoute::CursorFocus(FocusMarkPosition { x: 57, y: 37 })
    );
    assert_eq!(viewing.traversal().motion_count(), 0);
    assert!(viewing.cursor_feature().is_active());
}

#[test]
fn repeated_activation_is_idempotent_and_never_deactivates() {
    let extent = ViewingExtent::new(100, 80).unwrap();
    let mut viewing = ViewingState::new(extent);
    viewing.apply_session_command(SessionControlCommand::ActivateCursorFeature);
    viewing.route_relative_motion(RelativeMotion { dx: 4, dy: 2 });
    let active_position = viewing.snapshot().focus_mark;

    viewing.apply_session_command(SessionControlCommand::ActivateCursorFeature);

    assert!(viewing.cursor_feature().is_active());
    assert_eq!(viewing.snapshot().focus_mark, active_position);
}
~~~

- [ ] **Step 2: Write failing bound and invalid-extent tests**

~~~rust
#[test]
fn focus_position_clamps_to_viewing_extent() {
    let extent = ViewingExtent::new(4, 3).unwrap();
    let mut viewing = ViewingState::new(extent);
    viewing.apply_session_command(SessionControlCommand::ActivateCursorFeature);

    viewing.route_relative_motion(RelativeMotion { dx: 127, dy: 127 });
    assert_eq!(
        viewing.snapshot().focus_mark,
        Some(FocusMarkPosition { x: 3, y: 2 })
    );
    viewing.route_relative_motion(RelativeMotion { dx: -128, dy: -128 });
    assert_eq!(
        viewing.snapshot().focus_mark,
        Some(FocusMarkPosition { x: 0, y: 0 })
    );
}

#[test]
fn zero_sized_viewing_extent_is_rejected() {
    assert_eq!(ViewingExtent::new(0, 10), Err(ViewingError::EmptyExtent));
    assert_eq!(ViewingExtent::new(10, 0), Err(ViewingError::EmptyExtent));
}
~~~

- [ ] **Step 3: Run Viewing tests and verify RED**

~~~powershell
cargo test -p pythos-core viewing:: -- --nocapture
~~~

Expected: compile failure because the Viewing parent domain does not exist.

- [ ] **Step 4: Implement TraversalState without place semantics**

In viewing/traversal.rs:

~~~rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraversalIntent {
    pub motion: RelativeMotion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraversalState {
    motion_count: u32,
    last_motion: Option<RelativeMotion>,
}
~~~

accept_relative_motion increments motion_count with saturating_add, records last_motion, and returns TraversalIntent. Do not add orientation, coordinates, path identifiers, place identifiers, velocity, or camera values.

- [ ] **Step 5: Implement CursorFeatureState without rendering**

In viewing/focus_mark.rs:

~~~rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocusMarkPosition {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CursorFeatureState {
    active: bool,
    position: FocusMarkPosition,
}
~~~

Initialize position to integer center width / 2, height / 2. Implement activate as one-way assignment to true. Implement consume_relative_motion using signed i64 intermediate arithmetic and clamping to 0..width-1 and 0..height-1. Expose is_active and position. Do not implement deactivate, toggle, visible pixel shape, click state, or button state.

- [ ] **Step 6: Implement ViewingState as the sole router**

In viewing/mod.rs, own TraversalState and CursorFeatureState directly:

~~~rust
pub struct ViewingState {
    extent: ViewingExtent,
    traversal: TraversalState,
    cursor_feature: CursorFeatureState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionRoute {
    Traversal(TraversalIntent),
    CursorFocus(FocusMarkPosition),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewingSnapshot {
    pub extent: ViewingExtent,
    pub focus_mark: Option<FocusMarkPosition>,
}
~~~

apply_session_command handles the only current command by activating the cursor feature. route_relative_motion checks CursorFeatureState::is_active and delegates to exactly one child. snapshot returns focus_mark None while inactive and Some(position) while active.

Register mod viewing under cfg(any(test, feature = "viewing-input-probe")). Do not add a current-place field merely to reserve it.

- [ ] **Step 7: Run Viewing and input tests and verify GREEN**

~~~powershell
cargo test -p pythos-core viewing:: -- --nocapture
cargo test -p pythos-core input_events -- --nocapture
cargo test -p pythos-core session_controls -- --nocapture
~~~

Expected: all pass. The Traversal count must remain unchanged for focus-routed motion.

- [ ] **Step 8: Commit the Viewing boundary**

~~~powershell
git add core/src/viewing core/src/main.rs
git commit -m "feat: add session-owned viewing input routing"
~~~

### Task 4: Pure End-to-End Viewing Input Probe Controller

**Files:**
- Create: core/src/viewing_input_probe.rs
- Modify: core/src/main.rs

**Interfaces:**
- Consumes: raw keyboard bytes, UsbBootMouseReport, input_events::normalize, SessionControlInterpreter, and ViewingState.
- Produces: ViewingInputProbe::new(width, height), observe_keyboard_byte(byte), observe_mouse_report(report), finish(), and snapshot().
- Produces: ViewingInputProbeKeyboardStep::{Waiting, Activated}.
- Produces: ViewingInputProbeError::{EmptyExtent, InputNormalization, WrongRoute, MissingTraversalMotion, MissingCursorMotion}.
- Produces: ViewingInputPresentationStatus::{WaitingForTraversal, WaitingForActivation, Active, Complete, Failed}.
- Produces: ViewingInputIntegrationFailure::{Probe(ViewingInputProbeError), KeyboardUnavailable, Presentation} for the feature-gated hardware/presentation boundary.

- [ ] **Step 1: Write a failing decoded-USB-to-Viewing test**

~~~rust
#[test]
fn decoded_usb_motion_routes_to_traversal_then_focus_after_global_activation() {
    let mut probe = ViewingInputProbe::new(100, 80).unwrap();
    let first = decode_usb_boot_mouse_report(&[0, 8, 0xFC, 0]).unwrap();

    assert_eq!(
        probe.observe_mouse_report(first).unwrap(),
        Some(MotionRoute::Traversal(TraversalIntent {
            motion: RelativeMotion { dx: 8, dy: -4 },
        }))
    );
    assert_eq!(probe.snapshot().focus_mark, None);

    for byte in [0x39, 0x39, 0x0E] {
        assert_eq!(
            probe.observe_keyboard_byte(byte).unwrap(),
            ViewingInputProbeKeyboardStep::Waiting
        );
    }
    assert_eq!(
        probe.observe_keyboard_byte(0x0E).unwrap(),
        ViewingInputProbeKeyboardStep::Activated
    );

    assert!(matches!(
        probe.observe_mouse_report(first).unwrap(),
        Some(MotionRoute::CursorFocus(_))
    ));
    assert!(probe.finish().is_ok());
}
~~~

- [ ] **Step 2: Write failing no-toggle, zero-motion, and terminal tests**

Add tests proving:

- repeating the four-key sequence leaves the feature active;
- a zero-motion left-button report returns Ok(None) and does not change Traversal or focus position;
- finish fails until both one Traversal motion and one post-activation cursor motion were observed;
- set-2 Space Space Backspace Backspace also activates through PhysicalKeyboardDecoder;
- a wrong key resets activation progress without changing Viewing state.

- [ ] **Step 3: Run focused tests and verify RED**

~~~powershell
cargo test -p pythos-core viewing_input_probe -- --nocapture
~~~

Expected: compile failure because ViewingInputProbe and its result types do not exist.

- [ ] **Step 4: Implement the pure controller**

The controller owns exactly one PhysicalKeyboardDecoder, SessionControlInterpreter, and ViewingState:

~~~rust
pub struct ViewingInputProbe {
    keyboard: PhysicalKeyboardDecoder,
    controls: SessionControlInterpreter,
    viewing: ViewingState,
    traversal_motion_seen: bool,
    cursor_motion_seen: bool,
}
~~~

observe_keyboard_byte must:

1. decode a physical make event;
2. normalize it to InputEvent;
3. pass it to SessionControlInterpreter;
4. dispatch ActivateCursorFeature into ViewingState;
5. return Activated only when a command was emitted.

observe_mouse_report must return Ok(None) for dx == 0 and dy == 0. Otherwise it must call report.movement_event(), normalize the RawInputEvent, extract RelativeMotion, route it through ViewingState, and update exactly the matching evidence Boolean.

presentation_status returns WaitingForTraversal initially, WaitingForActivation after the first Traversal motion, Active after activation, and Complete after the first cursor-routed motion. Failed is supplied only by feature-gated integration failure handling; it is never inferred from Viewing state.

finish returns the final ViewingSnapshot only when traversal_motion_seen, cursor_feature active, and cursor_motion_seen are all true. ViewingInputIntegrationFailure wraps pure probe failures and names keyboard/presentation failures without classifying either as xHCI transport failure. The controller adds no deactivation path and does not inspect button or auxiliary fields.

- [ ] **Step 5: Run pure integration tests and verify GREEN**

~~~powershell
cargo test -p pythos-core viewing_input_probe -- --nocapture
~~~

Expected: every decoded-input routing, activation, zero-motion, and terminal invariant test passes.

- [ ] **Step 6: Commit the pure integration controller**

~~~powershell
git add core/src/viewing_input_probe.rs core/src/main.rs
git commit -m "test: prove decoded input reaches viewing policy"
~~~

### Task 5: FocusMark Presentation

**Files:**
- Modify: core/src/framebuffer.rs

**Interfaces:**
- Consumes: ViewingSnapshot and a feature-gated ViewingInputPresentationStatus.
- Produces: render_viewing_input_probe(framebuffer, snapshot, status) -> Result<(), ()>.
- Produces internally: Surface::draw_focus_mark(position).
- Does not consume: RawInputEvent, InputEvent, SessionControlCommand, UsbBootMouseReport, or PS/2 state.

- [ ] **Step 1: Write failing pixel-shape tests**

Use the existing host-backed test_framebuffer helper. Define presentation constants with half-span 12, arm length 6, and thickness 2, then test:

~~~rust
#[test]
fn focus_mark_draws_four_separated_corners_with_empty_center() {
    let (buffer, info) = test_framebuffer(64, 64);
    let snapshot = active_viewing_snapshot(64, 64, 32, 32);
    render_viewing_input_probe(
        &info,
        snapshot,
        ViewingInputPresentationStatus::Active,
    )
    .unwrap();

    assert!(pixel_set(&buffer, 64, 20, 20));
    assert!(pixel_set(&buffer, 64, 44, 20));
    assert!(pixel_set(&buffer, 64, 20, 44));
    assert!(pixel_set(&buffer, 64, 44, 44));
    assert!(!pixel_set(&buffer, 64, 32, 32));
    assert!(!pixel_set(&buffer, 64, 32, 20));
    assert!(!pixel_set(&buffer, 64, 20, 32));
}

#[test]
fn inactive_viewing_snapshot_draws_no_focus_mark_pixels() {
    let (buffer, info) = test_framebuffer(64, 64);
    render_viewing_input_probe(
        &info,
        inactive_viewing_snapshot(64, 64),
        ViewingInputPresentationStatus::WaitingForTraversal,
    )
    .unwrap();
    assert!(!pixel_set(&buffer, 64, 32, 32));
    assert_eq!(focus_color_pixel_count(&buffer), 0);
}
~~~

Add edge tests with centers at (0, 0) and (width - 1, height - 1), proving clipping and no out-of-bounds write. Add an exact focus-color count test for the centered mark so accidental dots, crosshair spans, or arrow pixels fail.

- [ ] **Step 2: Run focused framebuffer tests and verify RED**

~~~powershell
cargo test -p pythos-core focus_mark -- --nocapture
~~~

Expected: compile failure because the Viewing renderer and focus-mark drawing primitive do not exist.

- [ ] **Step 3: Implement the presentation-only focus shape**

Add a dedicated FOCUS_MARK_COLOR and draw eight bounded rectangles: one horizontal and one vertical arm for each corner. Use Surface::fill_rect so existing framebuffer validation, pixel-format encoding, and clipping remain authoritative.

The renderer must:

- clear the diagnostic background before each frame so motion leaves no trail;
- draw short textual status supplied by ViewingInputPresentationStatus;
- draw nothing focus-shaped when snapshot.focus_mark is None;
- draw the four-corner mark when snapshot.focus_mark is Some;
- never call draw_cursor_sprite or inspect feature activation directly.

- [ ] **Step 4: Run framebuffer tests and verify GREEN**

~~~powershell
cargo test -p pythos-core focus_mark -- --nocapture
cargo test -p pythos-core framebuffer::tests -- --nocapture
~~~

Expected: exact corner pixels pass, the center and gaps remain background, inactive snapshots contain no focus-color pixels, and legacy launcher sprite tests remain unchanged.

- [ ] **Step 5: Commit the presentation boundary**

~~~powershell
git add core/src/framebuffer.rs
git commit -m "feat: render viewing focus mark projection"
~~~

### Task 6: Opt-In xHCI Viewing Integration

**Files:**
- Modify: core/Cargo.toml
- Modify: core/src/main.rs
- Modify: core/src/ps2.rs
- Modify: core/src/usb_xhci_probe_boot.rs
- Modify: core/src/usb_xhci_probe_screen.rs
- Modify: core/src/viewing_input_probe.rs

**Interfaces:**
- Produces feature: viewing-input-probe = ["usb-xhci-boot-mouse-recurring-probe"].
- Consumes: the existing XhciInterruptTransferProbeSession and decoded UsbBootMouseReport.
- Produces markers: VIEWING:TRAVERSAL_RELATIVE_MOTION, SESSION_CONTROL:CURSOR_ACTIVATION_READY, SESSION_CONTROL:CURSOR_ACTIVATED, VIEWING:CURSOR_RELATIVE_MOTION, VIEWING:FOCUS_MARK_X=<decimal>, VIEWING:FOCUS_MARK_Y=<decimal>, VIEWING:FOCUS_MARK_READY, VIEWING_INPUT_PROBE_READY.
- Preserves: every standalone ADR 0088 marker and terminal rule.

- [ ] **Step 1: Write failing feature and terminal-state tests**

In viewing_input_probe.rs add a terminal predicate:

~~~rust
#[test]
fn terminal_readiness_requires_both_routes_activation_and_render() {
    assert!(viewing_input_terminal_ready(true, true, true, true));
    assert!(!viewing_input_terminal_ready(false, true, true, true));
    assert!(!viewing_input_terminal_ready(true, false, true, true));
    assert!(!viewing_input_terminal_ready(true, true, false, true));
    assert!(!viewing_input_terminal_ready(true, true, true, false));
}
~~~

Add a source-level feature test in tests/test_build_orchestration.py that parses core/Cargo.toml and requires viewing-input-probe to depend on usb-xhci-boot-mouse-recurring-probe, not the reverse.

- [ ] **Step 2: Run tests and verify RED**

~~~powershell
cargo test -p pythos-core viewing_input_terminal_ready -- --nocapture
py -3 -m unittest tests.test_build_orchestration -v
~~~

Expected: failures because the predicate and feature do not exist.

- [ ] **Step 3: Add the feature and compilation boundaries**

In core/Cargo.toml:

~~~toml
# Semantic Viewing integration above ADR 0088. It proves default Traversal,
# one-way session cursor activation, FocusMark routing, and no storage writes.
viewing-input-probe = ["usb-xhci-boot-mouse-recurring-probe"]
~~~

Compile session_controls, viewing, and viewing_input_probe under cfg(any(test, feature = "viewing-input-probe")). Extend only the cfg lists on ps2::initialize_keyboard_polling and ps2::poll_raw_output_byte so the new opt-in probe can use the existing keyboard-only polling path.

- [ ] **Step 4: Add the activation pause above the xHCI driver**

When viewing-input-probe is enabled, extend run_boot_mouse_recurring_probe with a `framebuffer: &PythFramebufferInfo` parameter and create one ViewingInputProbe from `framebuffer.width` and `framebuffer.height`. The standalone ADR 0088 call passes the same metadata but does not consume it unless the higher feature is compiled. Do not pass framebuffer information into usb_xhci_driver.rs.

After each decoded report:

1. preserve emit_boot_mouse_decode and summary.observe in their existing order;
2. pass the decoded report to ViewingInputProbe;
3. emit exactly one route marker for each non-zero movement;
4. after the first Traversal route, do not arm the next xHCI transfer;
5. initialize keyboard polling and emit CURSOR_ACTIVATION_READY;
6. non-blockingly poll bytes and feed them to the probe until Activated;
7. dispatch occurs inside the pure probe controller, then emit CURSOR_ACTIVATED;
8. resume the existing report loop and render supplied snapshots after cursor-routed motion.

The wait has no sequence timeout. Existing xHCI transfer completion waits remain bounded. Because the activation pause occurs after capture_next returned and before the next call, no xHCI transfer or report buffer is controller-owned during keyboard entry.

- [ ] **Step 5: Add typed integration failures**

Use ViewingInputIntegrationFailure to wrap a pure ViewingInputProbeError or identify KeyboardUnavailable and Presentation. Add a feature-gated Viewing variant to the probe screen's existing failure enum so the framebuffer can identify the higher-layer failure without reclassifying it as an xHCI driver failure.

Each failure must emit one stable marker, suppress VIEWING_INPUT_PROBE_READY and USB_XHCI_PROBE_READY, preserve NO_DISK_WRITES, and render a failure status if framebuffer metadata is valid.

- [ ] **Step 6: Enforce final success ordering**

The successful terminal order must be:

~~~text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_RECURRING_READY
PYTHOS:CORE:VIEWING:FOCUS_MARK_X=<decimal>
PYTHOS:CORE:VIEWING:FOCUS_MARK_Y=<decimal>
PYTHOS:CORE:VIEWING:FOCUS_MARK_READY
PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
PYTHOS:CORE:VIEWING_INPUT_PROBE_READY
PYTHOS:CORE:USB_XHCI_PROBE_READY
~~~

VIEWING_INPUT_PROBE_READY requires the pure probe finish result, successful focus-mark render, and existing recurring terminal readiness. Standalone usb-xhci-boot-mouse-recurring-probe builds must retain their current order without the Viewing markers.

- [ ] **Step 7: Run feature-focused host and cross-target checks**

~~~powershell
cargo test -p pythos-core viewing_input -- --nocapture
cargo test -p pythos-core usb_boot_mouse -- --nocapture
cargo build -p pythos-core --target x86_64-unknown-none --features viewing-input-probe
cargo clippy -p pythos-core --target x86_64-unknown-none --features viewing-input-probe -- -D warnings
~~~

Expected: all host tests pass; the feature build and clippy pass without changing the existing USB driver interfaces or adding unsafe code.

- [ ] **Step 8: Re-run standalone ADR 0088 before committing**

~~~powershell
py -3 scripts/test-usb-xhci-boot-mouse-recurring-probe.py --self-test
py -3 scripts/test-usb-xhci-boot-mouse-recurring-probe.py
~~~

Expected: USB_XHCI_BOOT_MOUSE_RECURRING_PROBE_TEST_OK and QEMU_OUTCOME success with the original counts, totals, no-cursor diagnostic panel, and no Viewing markers.

If the standalone oracle changes, stop and invoke superpowers:systematic-debugging. Do not weaken ADR 0088 assertions to accommodate the new feature.

- [ ] **Step 9: Commit the opt-in integration**

~~~powershell
git add core/Cargo.toml core/src/main.rs core/src/ps2.rs core/src/usb_xhci_probe_boot.rs core/src/usb_xhci_probe_screen.rs core/src/viewing_input_probe.rs
git commit -m "feat: route recurring mouse input through viewing"
~~~

### Task 7: QEMU Keyboard/Mouse and FocusMark Acceptance

**Files:**
- Modify: scripts/launcher_click.py
- Create: scripts/test-viewing-input-probe.py

**Interfaces:**
- Produces: launcher_click.type_cursor_activation_sequence().
- Reuses: run-qemu.py --sequence-usb-mouse-after-marker and the existing 14-move/press/release ADR 0088 QMP sequence.
- Produces: VIEWING_INPUT_PROBE_TEST_OK and target/viewing-input-probe.ppm.

- [ ] **Step 1: Write oracle self-tests before the live harness**

Create test-viewing-input-probe.py with --self-test fixtures proving:

- missing or out-of-order activation markers fail;
- a cursor-route marker before CURSOR_ACTIVATED fails;
- a Traversal-route marker after CURSOR_ACTIVATED fails;
- repeated activation does not produce a deactivation marker;
- missing ADR 0088 recurring/no-write/overall-ready markers fail;
- a PPM with an arrow, filled center, dot, crosshair, joined corners, or wrong focus position fails;
- exactly four separated L-shaped corners at the reported position pass.

Keep the parser functions pure so self-test does not build or launch QEMU.

- [ ] **Step 2: Run oracle self-tests and verify RED**

~~~powershell
py -3 scripts/test-viewing-input-probe.py --self-test
~~~

Expected: failure until the serial and PPM validators are complete.

- [ ] **Step 3: Add the exact QMP activation helper**

In launcher_click.py add:

~~~python
def type_cursor_activation_sequence(
    qmp_port: int = QMP_PORT, timeout: float = 5.0
) -> None:
    press_qcode_keys(
        ["spc", "spc", "backspace", "backspace"],
        qmp_port=qmp_port,
        timeout=timeout,
    )
~~~

Do not add Escape, a second sequence, mouse clicks, or timing policy.

- [ ] **Step 4: Implement the live QEMU harness**

The harness must:

1. build pythos-boot for x86_64-unknown-uefi;
2. build pythos-core for x86_64-unknown-none with viewing-input-probe;
3. reuse the existing verified user-shell/Pyth graph/image prerequisites from the ADR 0088 harness;
4. start run-qemu.py with xHCI, boot-USB removal, delayed USB-mouse hotplug, --sequence-usb-mouse-after-marker on XHCI_INTERRUPT_TRANSFER_ARMED, --screendump target/viewing-input-probe.ppm, success marker USB_XHCI_PROBE_READY, and --expect-outcome success;
5. wait for SESSION_CONTROL:CURSOR_ACTIVATION_READY;
6. call type_cursor_activation_sequence exactly once;
7. wait for process completion and validate serial plus PPM.

The inherited QMP sequence sends 14 movement reports followed by left press and release. Report 1 must route to Traversal. Reports 2 through 14 must route to Cursor/FocusMark. Reports 15 and 16 remain button-state evidence and must not create motion routes.

- [ ] **Step 5: Assert the exact semantic evidence**

Require:

~~~text
one VIEWING:TRAVERSAL_RELATIVE_MOTION before activation
one SESSION_CONTROL:CURSOR_ACTIVATION_READY
one SESSION_CONTROL:CURSOR_ACTIVATED
thirteen VIEWING:CURSOR_RELATIVE_MOTION markers after activation
one VIEWING:FOCUS_MARK_X=<decimal>
one VIEWING:FOCUS_MARK_Y=<decimal>
one VIEWING:FOCUS_MARK_READY
one VIEWING_INPUT_PROBE_READY
one USB_XHCI_PROBE:NO_DISK_WRITES
one USB_XHCI_PROBE_READY
QEMU_OUTCOME success
~~~

Also require the complete ADR 0088 report groups, transfer wrap, event wrap, totals, pressed/released evidence, and exact repeated-marker counts by importing or reusing its validator rather than copying a weaker subset.

Read the final focus x/y markers from serial. Parse the P6 PPM and require the exact focus color only in the eight clipped corner-arm rectangles around that center, with background at the center and both axis gaps.

- [ ] **Step 6: Run self-test and live acceptance**

~~~powershell
py -3 scripts/test-viewing-input-probe.py --self-test
py -3 scripts/test-viewing-input-probe.py
~~~

Expected:

~~~text
VIEWING_INPUT_PROBE_SELF_TEST_OK
VIEWING_INPUT_PROBE_TEST_OK
QEMU_OUTCOME success
~~~

- [ ] **Step 7: Commit the acceptance harness**

~~~powershell
git add scripts/launcher_click.py scripts/test-viewing-input-probe.py
git commit -m "test: accept viewing input routing in qemu"
~~~

### Task 8: ADR, Documentation, and Full Regression

**Files:**
- Create: docs/decisions/0089-viewing-input-routing-foundation.md
- Modify: docs/PythOS-TDD-001.md
- Modify: docs/TECHNICAL-OVERVIEW.md
- Modify: README.md
- Modify after verification: D:\PythOS-Workspace\CURRENT-STATE.md

**Interfaces:**
- Produces: auditable QEMU-only status and exact commands/hashes.
- Preserves: ADR 0066 interface authority and ADR 0088 lower-layer evidence boundary.

- [ ] **Step 1: Record ADR 0089**

Record:

- root/session activation scope versus Viewing ownership;
- session-wide, non-durable CursorFeatureState lifetime;
- one-way idempotent activation-only behavior;
- exact no-timeout key sequence semantics;
- RelativeMotion vocabulary;
- Viewing-owned routing;
- Traversal's intentionally neutral first state;
- presentation-only four-corner mark;
- opt-in integration above ADR 0088;
- default normal-boot cutover and deactivation as separate future decisions;
- QEMU status as pending until Task 7 passes.

- [ ] **Step 2: Update technical contracts without rewriting history**

In PythOS-TDD-001.md append a new ADR 0089 acceptance section after ADR 0088. Do not rename historical Pointer/Window markers.

In TECHNICAL-OVERVIEW.md describe Viewing as a new semantic parent domain and explicitly identify launcher_screen/window_interaction as compatibility paths.

In README.md add the one command:

~~~powershell
py -3 scripts/test-viewing-input-probe.py
~~~

Label it opt-in QEMU evidence, not default input or generic physical support.

- [ ] **Step 3: Run formatting, host, and Python tests**

~~~powershell
cargo fmt --check
cargo test -p pythos-core
py -3 -m unittest discover -s tests -p "test_*.py" -v
~~~

Expected: all pass. Any failure is investigated before documentation status changes.

- [ ] **Step 4: Run focused feature and predecessor QEMU matrix**

~~~powershell
py -3 scripts/test-usb-xhci-endpoint-configuration-probe.py
py -3 scripts/test-usb-xhci-interrupt-transfer-probe.py
py -3 scripts/test-usb-xhci-boot-mouse-decode-probe.py
py -3 scripts/test-usb-xhci-boot-mouse-recurring-probe.py
py -3 scripts/test-viewing-input-probe.py
~~~

Expected: each prints its own TEST_OK line and QEMU_OUTCOME success. Only the new feature log may contain VIEWING or SESSION_CONTROL cursor-activation markers.

- [ ] **Step 5: Run normal and persistence regressions**

~~~powershell
py -3 scripts/test-boot.py --slice milestone-1
py -3 scripts/test-normal-fast-boot.py
py -3 scripts/test-persistent-storage.py
~~~

Expected: all pass with QEMU_OUTCOME success. Normal boot retains the existing launcher behavior because no cutover is authorized in this slice.

- [ ] **Step 6: Run final cross-target lint**

~~~powershell
cargo clippy -p pythos-core --target x86_64-unknown-none --features viewing-input-probe -- -D warnings
~~~

Expected: pass with no warnings.

- [ ] **Step 7: Hash final evidence**

After the final successful new harness build, run:

~~~powershell
Get-FileHash -Algorithm SHA256 target\esp\EFI\BOOT\BOOTX64.EFI
Get-FileHash -Algorithm SHA256 target\esp\PYTHOS\PYTHCORE.ELF
Get-FileHash -Algorithm SHA256 target\viewing-input-probe-com1.log
Get-FileHash -Algorithm SHA256 target\viewing-input-probe.ppm
git rev-parse HEAD
git status --short
~~~

Record hashes in ADR 0089 and the external checkpoint. If source or a build input changes afterward, rerun the new QEMU oracle and regenerate hashes.

- [ ] **Step 8: Mark QEMU acceptance and refresh CURRENT-STATE**

Only after every fresh command above passes:

- set ADR 0089 status to Accepted in QEMU; physical validation pending;
- record exact passing commands and artifact hashes;
- update D:\PythOS-Workspace\CURRENT-STATE.md with the new semantic boundary;
- explicitly retain: no default normal-boot cutover, no deactivation behavior, no durable cursor state, and no physical acceptance claim.

- [ ] **Step 9: Commit documentation and checkpoint**

Commit repository files:

~~~powershell
git add docs/decisions/0089-viewing-input-routing-foundation.md docs/PythOS-TDD-001.md docs/TECHNICAL-OVERVIEW.md README.md
git commit -m "docs: record viewing input routing boundary"
~~~

Report CURRENT-STATE.md separately because it is outside the Git worktree. Do not push, merge, deploy, or write a USB as part of this step.

## Physical Validation Gate

Physical Lenovo validation is intentionally outside the executable implementation tasks because the removable target identity and a fresh write authorization cannot be known from this plan.

After the QEMU plan passes:

1. stop and report the branch commit, clean status, four artifact hashes, exact expected screen sequence, and QEMU evidence;
2. perform read-only Get-Disk, Get-Partition, and Get-Volume discovery;
3. have the owner authorize one exact disk number/model/partition;
4. only then create a separate deployment/physical-validation checklist with literal resolved targets;
5. preserve no-disk-write behavior during the PythOS probe;
6. require the user to move the mouse once before activation, type Space Space Backspace Backspace, then move again;
7. accept physical behavior only from a matching screen/photo/video or serial record showing Traversal first, activation, cursor-routed motion, FocusMark, and no writes.

No plan step treats a screenshot alone as generic device support or upgrades ADR 0088 beyond its target-specific evidence.

## Verification Matrix

- Pure input: set-1/set-2 make events, releases ignored, RelativeMotion normalization, existing capability checks unchanged.
- Root/session controls: exact sequence, no timer, unrelated-key reset, interleaved motion ignored, repeated command emission without toggle.
- Viewing: Traversal default, session-owned cursor activation, exclusive motion routing, idempotent activation, bounded focus position, no deactivation API.
- Presentation: four separated corners, empty center, no arrow/dot/crosshair, inactive no-op, edge clipping.
- Integration: decoded USB report -> RawInputEvent -> InputEvent -> Viewing route; physical keyboard bytes -> normalized KeyDown -> global command -> same ViewingState.
- QEMU: first movement routes Traversal, exact key sequence activates, later movements route Cursor/FocusMark, final PPM matches, ADR 0088 evidence remains complete, no writes.
- Regressions: full Rust/Python tests, normal boot, persistent storage, endpoint/raw/decode/recurring xHCI probes.
- Physical hardware: pending a separate exact-target deployment authorization and Lenovo evidence run.

## Non-Goals

- Default normal-boot input cutover.
- Cursor deactivation or any exit gesture.
- Durable cursor activation across reboot.
- Project/task/hall/place behavior.
- Camera, yaw/pitch, zoom, scroll, or depth.
- Click, double-click, drag, chord, or button actions.
- Arrow pointer, dot, crosshair, desktop, windows, or generic compositor policy.
- IRQ-driven USB input, hubs, trackpads, generic HID, hot unplug, or unbounded input rings.
- Storage writes, evidence volumes, or filesystem work.
