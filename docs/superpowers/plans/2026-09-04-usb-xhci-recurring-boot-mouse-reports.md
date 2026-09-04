# USB xHCI Recurring Boot-Mouse Reports Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the opt-in PythOS xHCI diagnostic from one decoded boot-mouse report to exactly sixteen ordered reports, prove one transfer-ring wrap and cycle-state event consumption, aggregate movement and button-state evidence, then freeze on a no-cursor result panel.

**Architecture:** Refactor the existing one-shot interrupt transfer into a synchronous session that keeps one static DMA report buffer in flight, an explicit interrupt transfer producer cursor, and a cycle-aware event consumer cursor. The boot probe loops exactly sixteen times, decodes each completed three- or four-byte report through the existing USB boot-mouse decoder, updates a bounded diagnostic summary, and renders only after the terminal condition. A marker-count-driven QMP action in the existing QEMU runner injects fourteen movements followed by an armed left press and release.

**Tech Stack:** Rust `no_std` PythCore, static page-aligned xHCI DMA rings, volatile MMIO/DMA access, COM1 acceptance markers, fixed-font framebuffer diagnostics, Python 3 QMP/QEMU harnesses, Cargo host and cross-target tests.

**Spec:** `docs/superpowers/specs/2026-09-04-usb-xhci-recurring-boot-mouse-reports-design.md`

## Global Constraints

- Work only in `D:\PythOS-Workspace\repo\pythos\.worktrees\hw-white-screen-diagnostic` on `agent/hw-white-screen-diagnostic`; plan baseline is commit `909ee75ad6f2a490c2d5b0e6521908cf053f0a91`.
- Keep `usb-xhci-boot-mouse-recurring-probe` opt-in and dependent on `usb-xhci-boot-mouse-decode-probe`; default boot and every earlier feature-only path remain unchanged.
- Accept exactly sixteen successfully decoded reports; data TRB indices are `0..14,0`, cycle states are `true` for reports 1-15 and `false` for report 16, and no report 17 is armed.
- Keep exactly one interrupt transfer and the existing static 4 KiB report buffer in flight; never submit stack or ordinary heap memory to xHCI.
- Consume command, control, and transfer events through one ordered event cursor whose expected cycle toggles at ring wrap.
- Keep all MMIO and DMA accesses volatile, all polls bounded, all failures typed, and all unsafe invariants complete.
- Preserve existing marker names and ordering. New repeated markers must have exact count/order assertions, and QEMU timeout is never success.
- Treat button values as states. `released_after_pressed` records only an adjacent accepted `1 -> 0` state transition; it is not a click or double-click action.
- Keep byte four raw auxiliary evidence; do not call it a wheel or assign zoom/scroll behavior.
- Do not add a cursor, camera control, normal input-service routing, IRQ-driven xHCI, hub support, mass storage, evidence-volume writes, or any disk write.
- Do not claim physical acceptance from QEMU. Re-identify the Lexar target and obtain a new explicit write approval before deployment.

## File and Responsibility Map

- `core/src/input_drivers.rs`: pure USB boot-mouse sequence aggregation and button-state transition evidence.
- `core/src/usb_xhci_driver.rs`: cycle-aware event consumption, single-in-flight transfer producer/session, per-report raw capture, progress snapshots, and typed failures.
- `core/src/usb_xhci_probe_boot.rs`: exact sixteen-report orchestration, per-report/final COM1 markers, feature precedence, and terminal success/failure selection.
- `core/src/usb_xhci_probe_screen.rs`: final recurring result panel, recurring failure panel, signed aggregate formatting, and fixed-glyph tests.
- `core/Cargo.toml`: new opt-in recurring feature and its strict dependency boundary.
- `tests/test_qemu_marker_actions.py`: host tests for deterministic QMP report steps and marker-count gating.
- `scripts/run-qemu.py`: one reusable, marker-count-driven sixteen-step USB mouse injection option.
- `scripts/test-usb-xhci-boot-mouse-recurring-probe.py`: new QEMU acceptance oracle for exact sequence, wrap, totals, button transition, no-write, and no-cursor behavior.
- `docs/decisions/0088-usb-xhci-recurring-boot-mouse-probe.md`: accepted architecture and verified evidence for this new boundary.
- `docs/PythOS-TDD-001.md`, `docs/TECHNICAL-OVERVIEW.md`, `README.md`: marker/test/status contract updates.
- `docs/decisions/0087-usb-xhci-boot-mouse-decode-probe.md` and `docs/evidence/2026-09-04-physical-usb-xhci-boot-mouse-decode-report.md`: correct the earlier no-photo wording using the now-available physical screen hashes without upgrading the proof beyond one report.
- `D:\PythOS-Workspace\CURRENT-STATE.md`: external workspace checkpoint updated only after fresh verification.

---

### Task 1: Pure Mouse Summary and Ring Cursor Contracts

**Files:**
- Modify: `core/src/input_drivers.rs`
- Modify: `core/src/usb_xhci_driver.rs`

**Interfaces:**
- Produces: `UsbBootMouseSequenceSummary::new()`, `observe(UsbBootMouseReport)`, and public fields `report_count: u8`, `dx_total: i32`, `dy_total: i32`, `last_report: Option<UsbBootMouseReport>`, `pressed_seen: u8`, `released_after_pressed: u8`, `auxiliary_seen: bool`, `latest_auxiliary: Option<u8>`.
- Produces internally: `XhciEventRingConsumer`, `XhciInterruptTransferProducer`, and `XhciInterruptTransferCursorSnapshot` for later driver tasks.

- [ ] **Step 1: Write failing aggregation tests using hand-derived report literals**

Add tests that name the bugs they catch:

```rust
#[test]
fn usb_boot_mouse_sequence_accumulates_signed_motion_and_latest_report() {
    let mut summary = UsbBootMouseSequenceSummary::new();
    summary.observe(UsbBootMouseReport {
        buttons: 0,
        dx: 8,
        dy: -4,
        auxiliary: Some(0),
    });
    summary.observe(UsbBootMouseReport {
        buttons: 0,
        dx: -7,
        dy: -7,
        auxiliary: None,
    });

    assert_eq!(summary.report_count, 2);
    assert_eq!(summary.dx_total, 1);
    assert_eq!(summary.dy_total, -11);
    assert_eq!(summary.last_report.unwrap().dx, -7);
    assert!(summary.auxiliary_seen);
    assert_eq!(summary.latest_auxiliary, Some(0));
}

#[test]
fn usb_boot_mouse_sequence_records_only_observed_press_then_release() {
    let mut summary = UsbBootMouseSequenceSummary::new();
    summary.observe(UsbBootMouseReport { buttons: 0, dx: 0, dy: 0, auxiliary: None });
    assert_eq!(summary.released_after_pressed, 0);
    summary.observe(UsbBootMouseReport { buttons: 1, dx: 0, dy: 0, auxiliary: None });
    summary.observe(UsbBootMouseReport { buttons: 0, dx: 0, dy: 0, auxiliary: None });
    assert_eq!(summary.pressed_seen, 1);
    assert_eq!(summary.released_after_pressed, 1);
}
```

Production changes caught: clearing the previous button state too early, treating an initial neutral report as a release, unsigned axis accumulation, and replacing rather than retaining the latest observed auxiliary byte.

- [ ] **Step 2: Run the focused summary tests and verify RED**

Run:

```powershell
cargo test -p pythos-core --bin pythcore usb_boot_mouse_sequence -- --nocapture
```

Expected: compile failure because `UsbBootMouseSequenceSummary` does not exist. Fix only test syntax if needed; retain a feature-missing failure.

- [ ] **Step 3: Implement the minimal summary**

Add beside `UsbBootMouseReport`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsbBootMouseSequenceSummary {
    pub report_count: u8,
    pub dx_total: i32,
    pub dy_total: i32,
    pub last_report: Option<UsbBootMouseReport>,
    pub pressed_seen: u8,
    pub released_after_pressed: u8,
    pub auxiliary_seen: bool,
    pub latest_auxiliary: Option<u8>,
}

impl UsbBootMouseSequenceSummary {
    pub const fn new() -> Self {
        Self {
            report_count: 0,
            dx_total: 0,
            dy_total: 0,
            last_report: None,
            pressed_seen: 0,
            released_after_pressed: 0,
            auxiliary_seen: false,
            latest_auxiliary: None,
        }
    }

    pub fn observe(&mut self, report: UsbBootMouseReport) {
        let prior_buttons = self.last_report.map_or(0, |prior| prior.buttons);
        self.released_after_pressed |= prior_buttons & !report.buttons & 0x07;
        self.pressed_seen |= report.buttons & 0x07;
        self.dx_total += i32::from(report.dx);
        self.dy_total += i32::from(report.dy);
        if let Some(auxiliary) = report.auxiliary {
            self.auxiliary_seen = true;
            self.latest_auxiliary = Some(auxiliary);
        }
        self.last_report = Some(report);
        self.report_count += 1;
    }
}
```

- [ ] **Step 4: Write failing cursor tests before cursor production code**

Add literal tests in `usb_xhci_driver::tests`:

```rust
#[test]
fn recurring_transfer_cursor_uses_fifteen_data_trbs_then_toggles_cycle() {
    let mut producer = XhciInterruptTransferProducer::new();
    for expected_index in 0..15 {
        let armed = producer.arm().unwrap();
        assert_eq!(armed.index, expected_index);
        assert!(armed.cycle);
        let wrapped = producer.complete().unwrap();
        assert_eq!(wrapped, expected_index == 14);
    }
    let sixteenth = producer.arm().unwrap();
    assert_eq!(sixteenth.index, 0);
    assert!(!sixteenth.cycle);
    assert_eq!(producer.wrap_count(), 1);
}

#[test]
fn recurring_transfer_cursor_rejects_second_arm_while_dma_is_owned() {
    let mut producer = XhciInterruptTransferProducer::new();
    producer.arm().unwrap();
    assert_eq!(producer.arm(), Err(XhciDriverError::InterruptTransferAlreadyArmed));
}

#[test]
fn event_consumer_toggles_expected_cycle_only_at_ring_wrap() {
    let mut consumer = XhciEventRingConsumer::new();
    for _ in 0..15 { consumer.advance(); }
    assert_eq!(consumer.index(), 15);
    assert!(consumer.expected_cycle());
    consumer.advance();
    assert_eq!(consumer.index(), 0);
    assert!(!consumer.expected_cycle());
    assert_eq!(consumer.wrap_count(), 1);
}

#[test]
fn event_consumer_rejects_stale_cycle_after_wrap() {
    let mut consumer = XhciEventRingConsumer::new();
    for _ in 0..16 { consumer.advance(); }
    assert!(!consumer.accepts(XhciTrb::new(0, 0, 0, XHCI_TRB_CYCLE)));
    assert!(consumer.accepts(XhciTrb::empty()));
}
```

Production changes caught: using all sixteen entries as data, failing to toggle cycle, allowing the CPU to overwrite an in-flight buffer, and accepting a stale event after wrap.

- [ ] **Step 5: Run the focused cursor tests and verify RED**

Run:

```powershell
cargo test -p pythos-core --bin pythcore usb_xhci_driver::tests::recurring_transfer_cursor -- --nocapture
cargo test -p pythos-core --bin pythcore usb_xhci_driver::tests::event_consumer -- --nocapture
```

Expected: compile failures for the missing cursor types and new typed error.

- [ ] **Step 6: Implement the minimal pure cursors and error markers**

Use private cursor types with no MMIO side effects. `XhciInterruptTransferProducer::arm()` sets `in_flight`, returns the current index/cycle snapshot, and rejects a second arm. `complete()` requires `in_flight`, clears it, increments the data index, and on index 14 sets index 0, toggles cycle, increments wrap count, and returns `true`. Add typed errors and stable markers for invalid producer state, already armed, and not armed.

`XhciEventRingConsumer::accepts(event)` must be exactly:

```rust
event.cycle() == self.expected_cycle
```

`advance()` increments modulo `XHCI_EVENT_RING_TRBS`; only an index-15-to-0 transition toggles `expected_cycle` and increments `wrap_count`.

- [ ] **Step 7: Run both focused groups and all core tests to verify GREEN**

Run:

```powershell
cargo test -p pythos-core --bin pythcore usb_boot_mouse_sequence -- --nocapture
cargo test -p pythos-core --bin pythcore usb_xhci_driver::tests::recurring_transfer_cursor -- --nocapture
cargo test -p pythos-core --bin pythcore usb_xhci_driver::tests::event_consumer -- --nocapture
cargo test -p pythos-core
```

Expected: all new tests pass and the baseline count increases from 683 with zero failures.

- [ ] **Step 8: Commit the pure contracts**

```powershell
git add core/src/input_drivers.rs core/src/usb_xhci_driver.rs
git commit -m "feat: add recurring mouse ring state contracts"
```

---

### Task 2: Cycle-Aware Event Consumption

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`

**Interfaces:**
- Consumes: `XhciEventRingConsumer::{new, accepts, advance}` from Task 1.
- Produces: one `event_consumer: XhciEventRingConsumer` in `XhciCommandProbeState`, passed through every command/control/interrupt completion poll.

- [ ] **Step 1: Add a failing event-consumption behavior test**

Extract a pure helper used by every poll:

```rust
fn accept_event_at_consumer(
    event: XhciTrb,
    consumer: &mut XhciEventRingConsumer,
) -> Option<XhciTrb>
```

Test it with a literal stale-cycle event followed by a matching-cycle event at index 0 after sixteen advances. Assert that the stale event returns `None` without advancing and the matching event returns `Some(event)` and advances to index 1 with cycle false.

Production change caught: retaining the current hard-coded `if event.cycle()` behavior after event-ring wrap.

- [ ] **Step 2: Run the focused test and verify RED**

```powershell
cargo test -p pythos-core --bin pythcore usb_xhci_driver::tests::accept_event_at_consumer -- --nocapture
```

Expected: compile failure because the shared consumer helper does not exist.

- [ ] **Step 3: Refactor every event poll to the shared consumer**

Replace `event_index: usize` in `XhciCommandProbeState` with `event_consumer: XhciEventRingConsumer`. Change all submit/poll signatures that currently accept `&mut usize` to accept `&mut XhciEventRingConsumer`.

Each poll loop must follow this order:

```rust
let event = read_trb(event_ring_ptr(), event_consumer.index());
if let Some(event) = accept_event_at_consumer(event, event_consumer) {
    ack_event(registers, dma, event_consumer.index())?;
    // Preserve the existing event type, pointer, completion, slot, endpoint,
    // residual-length, and timeout checks here.
}
```

Initialize the consumer before the No-op command and move the same value into `XhciCommandProbeState` after Enable Slot. Do not reset it during Address Device, descriptor reads, Configure Endpoint, `SET_CONFIGURATION`, or interrupt reports.

- [ ] **Step 4: Run focused and complete xHCI tests to verify GREEN**

```powershell
cargo test -p pythos-core --bin pythcore usb_xhci_driver::tests -- --nocapture
cargo test -p pythos-core
```

Expected: new wrap/stale-cycle tests pass and all existing command, control, and one-shot transfer tests remain green.

- [ ] **Step 5: Build and QEMU-test the existing one-shot feature unchanged**

```powershell
cargo build -p pythos-core --target x86_64-unknown-none --features usb-xhci-boot-mouse-decode-probe
py -3 scripts/test-usb-xhci-boot-mouse-decode-probe.py
```

Expected: `USB_XHCI_BOOT_MOUSE_DECODE_PROBE_TEST_OK` and `QEMU_OUTCOME success`; existing one-shot marker counts remain one.

- [ ] **Step 6: Commit the event-consumer refactor**

```powershell
git add core/src/usb_xhci_driver.rs
git commit -m "fix: consume xhci events across cycle wrap"
```

---

### Task 3: Single-In-Flight Interrupt Transfer Session

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`

**Interfaces:**
- Consumes: Task 1 transfer producer and Task 2 event consumer.
- Produces: `XhciInterruptTransferProbeSession::begin(registers, port_number)`, `capture_next(&mut self)`, `endpoint_configuration(&self)`, and `progress(&self)`.
- Produces: public `XhciInterruptTransferSample` and `XhciInterruptTransferProgress` values used by the boot and screen tasks.
- Preserves: `run_interrupt_transfer_probe(registers: crate::usb_xhci_probe::XhciRegisterSnapshot, port_number: u8) -> Result<XhciInterruptTransferProbeResult, XhciDriverError>` by implementing it as one session capture.

- [ ] **Step 1: Write failing indexed-TRB preparation tests**

Test a pure/host-visible `prepare_interrupt_transfer_at(dma, requested_length, cursor)` path against the real static ring:

```rust
#[test]
fn recurring_interrupt_preparation_publishes_wrapped_cycle_without_overwriting_link() {
    let dma = test_dma_state();
    prepare_interrupt_in_endpoint_context(dma, 32, 3, 0x81, 4, 7).unwrap();
    let link_before = read_trb(interrupt_ring_ptr(), 15);

    let first = XhciInterruptTransferCursorSnapshot { index: 0, cycle: true };
    let fifteenth = XhciInterruptTransferCursorSnapshot { index: 14, cycle: true };
    let sixteenth = XhciInterruptTransferCursorSnapshot { index: 0, cycle: false };
    assert_eq!(prepare_interrupt_transfer_at(dma, 4, first).unwrap(), dma.interrupt_ring_phys);
    assert_eq!(prepare_interrupt_transfer_at(dma, 4, fifteenth).unwrap(), dma.interrupt_ring_phys + 14 * 16);
    assert_eq!(prepare_interrupt_transfer_at(dma, 4, sixteenth).unwrap(), dma.interrupt_ring_phys);
    assert!(!read_trb(interrupt_ring_ptr(), 0).cycle());
    assert_eq!(read_trb(interrupt_ring_ptr(), 15), link_before);
}
```

Use the existing test DMA helper or introduce a private test helper that returns the same complete `XhciDmaState` shape used by current interrupt-transfer tests. Expected physical offsets are literal 16-byte TRB strides.

Production changes caught: always writing index zero/cycle one, using the Link entry as data, and corrupting the Link TRB.

- [ ] **Step 2: Run the indexed-preparation test and verify RED**

```powershell
cargo test -p pythos-core --bin pythcore usb_xhci_driver::tests::recurring_interrupt_preparation -- --nocapture
```

Expected: compile failure for missing cursor-aware preparation.

- [ ] **Step 3: Implement cursor-aware preparation and progress types**

Define:

```rust
pub const XHCI_BOOT_MOUSE_RECURRING_REPORTS: u8 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciInterruptTransferProgress {
    pub completed_reports: u8,
    pub next_trb_index: u8,
    pub next_cycle: bool,
    pub transfer_wrap_count: u8,
    pub event_index: u8,
    pub event_cycle: bool,
    pub event_wrap_count: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciInterruptTransferSample {
    pub ordinal: u8,
    pub trb_index: u8,
    pub trb_cycle: bool,
    pub wrapped_after_completion: bool,
    pub transfer_completion_code: u8,
    pub requested_length: u16,
    pub actual_length: u16,
    pub captured_length: u8,
    pub raw_report: [u8; XHCI_RAW_REPORT_CAPTURE_BYTES],
}
```

`prepare_interrupt_transfer_at` validates `requested_length`, rejects an index at or beyond 15, zeroes the report page only after `producer.arm()` establishes no prior request is in flight, writes the Normal TRB at the supplied index/cycle, fences, and returns the checked physical TRB address.

- [ ] **Step 4: Implement the synchronous session with no new behavior exposed yet**

`begin()` performs the current command/address/descriptor/configuration/endpoint initialization once and stores the endpoint result, registers, DMA state, event consumer, fresh transfer producer, and requested length.

`capture_next()` must:

```rust
if self.completed_reports >= XHCI_BOOT_MOUSE_RECURRING_REPORTS {
    return Err(XhciDriverError::InterruptTransferSequenceComplete);
}
let cursor = self.transfer_producer.arm()?;
let transfer_trb_phys = prepare_interrupt_transfer_at(self.dma, self.requested_length, cursor)?;
emit requested/index/cycle data as required by the caller's established marker order;
ring the endpoint doorbell;
emit XHCI_INTERRUPT_TRANSFER_ARMED;
let completion = poll_interrupt_transfer_completion(
    self.registers,
    self.dma,
    transfer_trb_phys,
    self.endpoint_configuration.configuration.descriptor.address.command.slot_id,
    self.endpoint_configuration.endpoint_id,
    self.requested_length,
    &mut self.event_consumer,
)?;
let (raw_report, captured_length) = capture_interrupt_report_prefix(completion.actual_length);
let wrapped = self.transfer_producer.complete()?;
self.completed_reports += 1;
return Ok(XhciInterruptTransferSample {
    ordinal: self.completed_reports,
    trb_index: cursor.index as u8,
    trb_cycle: cursor.cycle,
    wrapped_after_completion: wrapped,
    transfer_completion_code: completion.completion_code,
    requested_length: self.requested_length,
    actual_length: completion.actual_length,
    captured_length,
    raw_report,
});
```

If preparation or polling fails after `arm()`, do not clear or reuse the buffer; the bounded diagnostic terminates. `progress()` reports the current producer and shared event-consumer state without mutating either.

- [ ] **Step 5: Rewrite the old one-shot entry point as a compatibility wrapper**

Start a session, capture exactly once, and reconstruct the unchanged `XhciInterruptTransferProbeResult` from the session's copied endpoint result and sample. The old feature must still emit exactly one requested/armed/completion/raw/ready group and must not emit recurring sequence markers.

- [ ] **Step 6: Run host, cross-target, and one-shot QEMU verification**

```powershell
cargo test -p pythos-core --bin pythcore usb_xhci_driver::tests -- --nocapture
cargo test -p pythos-core
cargo build -p pythos-core --target x86_64-unknown-none --features usb-xhci-interrupt-transfer-probe
cargo build -p pythos-core --target x86_64-unknown-none --features usb-xhci-boot-mouse-decode-probe
py -3 scripts/test-usb-xhci-interrupt-transfer-probe.py
py -3 scripts/test-usb-xhci-boot-mouse-decode-probe.py
```

Expected: both legacy script success lines and `QEMU_OUTCOME success`; no recurring marker exists in either log.

- [ ] **Step 7: Commit the transfer session**

```powershell
git add core/src/usb_xhci_driver.rs
git commit -m "feat: add bounded xhci interrupt transfer session"
```

---

### Task 4: Deterministic Marker-Driven QMP Mouse Sequence

**Files:**
- Modify: `tests/test_qemu_marker_actions.py`
- Modify: `scripts/run-qemu.py`

**Interfaces:**
- Produces: `usb_mouse_sequence_events(step: int) -> list[dict]` for steps 0-15.
- Produces: `next_marker_sequence_step(observed: int, sent: int, limit: int) -> int | None` to prevent duplicate, skipped, and over-limit actions.
- Produces CLI: `--sequence-usb-mouse-after-marker MARKER`, valid only with `--xhci` and mutually exclusive with `--move-usb-mouse-after-marker`.

- [ ] **Step 1: Write failing real-helper tests with literal QMP payloads**

Extend `QemuMarkerActionTest`:

```python
def test_usb_mouse_sequence_is_fourteen_moves_then_left_press_release(self) -> None:
    run_qemu = load_run_qemu_module()
    move = [
        {"type": "rel", "data": {"axis": "x", "value": 8}},
        {"type": "rel", "data": {"axis": "y", "value": -4}},
    ]
    self.assertEqual(run_qemu.usb_mouse_sequence_events(0), move)
    self.assertEqual(run_qemu.usb_mouse_sequence_events(13), move)
    self.assertEqual(
        run_qemu.usb_mouse_sequence_events(14),
        [{"type": "btn", "data": {"button": "left", "down": True}}],
    )
    self.assertEqual(
        run_qemu.usb_mouse_sequence_events(15),
        [{"type": "btn", "data": {"button": "left", "down": False}}],
    )
    with self.assertRaises(ValueError):
        run_qemu.usb_mouse_sequence_events(16)

def test_marker_sequence_sends_each_new_occurrence_once(self) -> None:
    run_qemu = load_run_qemu_module()
    self.assertIsNone(run_qemu.next_marker_sequence_step(0, 0, 16))
    self.assertEqual(run_qemu.next_marker_sequence_step(1, 0, 16), 0)
    self.assertIsNone(run_qemu.next_marker_sequence_step(1, 1, 16))
    with self.assertRaises(ValueError):
        run_qemu.next_marker_sequence_step(3, 1, 16)
    with self.assertRaises(ValueError):
        run_qemu.next_marker_sequence_step(17, 16, 16)
```

Production changes caught: sending a button state before report 15, omitting release, injecting twice for one marker, silently skipping an armed report, or acting after the 16-report limit.

- [ ] **Step 2: Run the Python test and verify RED**

```powershell
py -3 -m pytest tests/test_qemu_marker_actions.py -q
```

Expected: failures for the two missing helpers.

- [ ] **Step 3: Implement pure helpers and one QMP sender**

Build an `input-send-event` command from the helper's real event list:

```python
USB_MOUSE_SEQUENCE_LENGTH = 16

def request_usb_mouse_sequence_step(step: int) -> None:
    events = usb_mouse_sequence_events(step)
    run_qmp_commands(({
        "execute": "input-send-event",
        "arguments": {"events": events},
    },))
```

The helper uses movement for `0 <= step < 14`, left down at 14, left up at 15, and raises `ValueError` otherwise. Confirm the installed QEMU QMP schema accepts the literal `InputButton` value before relying on it in acceptance; if the local QEMU names it differently, update both the independently derived literal test and implementation together and record the local schema value in ADR 0088.

- [ ] **Step 4: Add CLI validation and marker-count action state**

Add `--sequence-usb-mouse-after-marker`. Require `--xhci`; reject simultaneous one-shot movement and sequence options. In the monitor loop, compare `serial.count(marker)` with `mouse_sequence_steps_sent`; send exactly the next step returned by `next_marker_sequence_step`, increment only after a successful QMP command, and classify QMP/value/gap errors as runner failure. Never treat incomplete sequence injection as success if the recurring success marker somehow appears.

- [ ] **Step 5: Run the Python test and existing runner tests to verify GREEN**

```powershell
py -3 -m pytest tests/test_qemu_marker_actions.py tests/test_qemu_exit.py -q
```

Expected: all tests pass.

- [ ] **Step 6: Commit the QMP sequence mechanism**

```powershell
git add tests/test_qemu_marker_actions.py scripts/run-qemu.py
git commit -m "test: drive recurring usb mouse reports over qmp"
```

---

### Task 5: Write the Recurring Acceptance and Screen Tests First

**Files:**
- Create: `scripts/test-usb-xhci-boot-mouse-recurring-probe.py`
- Modify tests in: `core/src/usb_xhci_probe_screen.rs`

**Interfaces:**
- Consumes: Task 3 progress/session types and Task 1 sequence summary.
- Defines expected screen interfaces for Task 6:

```rust
pub enum UsbBootMouseRecurringFailure {
    Driver(crate::usb_xhci_driver::XhciDriverError),
    Decode(crate::input_drivers::InputDriverError),
    TerminalInvariant,
}

pub fn build_boot_mouse_recurring_probe_screen(
    report: &crate::usb_xhci_probe::UsbProbeReport,
    endpoint: crate::usb_xhci_driver::XhciEndpointConfigurationProbeResult,
    progress: crate::usb_xhci_driver::XhciInterruptTransferProgress,
    summary: crate::input_drivers::UsbBootMouseSequenceSummary,
) -> ProbeScreen;

pub fn build_boot_mouse_recurring_error_screen(
    report: &crate::usb_xhci_probe::UsbProbeReport,
    port_status: crate::usb_xhci_probe::XhciPortStatusSnapshot,
    change: crate::usb_xhci_probe::XhciPortChange,
    progress: crate::usb_xhci_driver::XhciInterruptTransferProgress,
    summary: crate::input_drivers::UsbBootMouseSequenceSummary,
    failure: UsbBootMouseRecurringFailure,
) -> ProbeScreen;
```
- Defines stable acceptance markers for Task 6.

- [ ] **Step 1: Add failing final and error screen tests**

Construct real existing `UsbProbeReport` and endpoint-result fixtures, plus literal progress/summary values. Assert exact lines:

```text
PythOS
xhci mouse sequence
no disk writes
reports 16 wrap 1
last 00 l0 r0 m0
seen 01 rel 01
sumx +0112 sumy -0056
aux 00 present 1
frozen no cursor
```

The error fixture uses seven accepted reports, next TRB index seven/cycle one, and `InterruptTransferTimeout`; assert `xhci input error`, `done 07`, `next 07 cycle 1`, the typed timeout label, and no success/frozen claim. Check every output byte against the fixed boot font.

Production changes caught: rendering controller count as report count, losing signs on totals, claiming release without the transition mask, or showing a success freeze on failure.

- [ ] **Step 2: Run the screen tests and verify RED**

```powershell
cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_recurring -- --nocapture
```

Expected: compile failure because the recurring screen builders do not exist.

- [ ] **Step 3: Create the QEMU acceptance script before the feature exists**

The script must:

- build `pythos-boot` for `x86_64-unknown-uefi`;
- build `pythos-core` for `x86_64-unknown-none` with `usb-xhci-boot-mouse-recurring-probe`;
- build the verified user shell and image through the same prerequisite helpers as ADR 0087;
- boot with qemu-xhci and simulated boot storage, detach the simulated boot USB after `SWAP_READY`, delayed-hotplug the QEMU USB mouse, then invoke `--sequence-usb-mouse-after-marker` on `XHCI_INTERRUPT_TRANSFER_ARMED`;
- require `QEMU_OUTCOME success` and print exactly `USB_XHCI_BOOT_MOUSE_RECURRING_PROBE_TEST_OK` on acceptance.

Parse report groups by the one-based ordinal markers. Assert exact literal sequences:

```python
assert ordinals == list(range(1, 17))
assert trb_indices == list(range(15)) + [0]
assert trb_cycles == [1] * 15 + [0]
assert raw_group_count == 16
assert decode_group_count == 16
assert transfer_wrap_count == 1
assert report_count == 16
assert dx_total_u32 == 0x00000070
assert dy_total_u32 == 0xFFFFFFC8
assert buttons_last == 0
assert pressed_seen == 1
assert released_after_pressed == 1
```

Also require event wrap count at least one; exactly one recurring-ready, framebuffer-ready, no-write, and overall-ready marker; and no driver-error, decode-invalid, pointer-cursor, input-service-ready, storage-write, panic, timeout-success, or duplicate terminal marker.

- [ ] **Step 4: Run the acceptance script and verify RED**

```powershell
py -3 scripts/test-usb-xhci-boot-mouse-recurring-probe.py
```

Expected: nonzero exit while Cargo reports that `usb-xhci-boot-mouse-recurring-probe` does not exist. The script must not print its success line.

- [ ] **Step 5: Commit the proven-red acceptance tests**

```powershell
git add core/src/usb_xhci_probe_screen.rs scripts/test-usb-xhci-boot-mouse-recurring-probe.py
git commit -m "test: specify recurring xhci mouse acceptance"
```

---

### Task 6: Recurring Boot Orchestration, Markers, and Framebuffer

**Files:**
- Modify: `core/Cargo.toml`
- Modify: `core/src/usb_xhci_probe_boot.rs`
- Modify: `core/src/usb_xhci_probe_screen.rs`

**Interfaces:**
- Consumes: all Task 1-5 interfaces and marker contracts.
- Produces: feature `usb-xhci-boot-mouse-recurring-probe`, exactly sixteen decoded report groups, terminal aggregate markers, final/error renderers, and automatic freeze.

- [ ] **Step 1: Add the feature with strict dependency and precedence**

Add:

```toml
# Sixteen single-in-flight boot-mouse reports prove one transfer-ring wrap and
# aggregate state only. No cursor, wheel semantics, IRQ input, or storage.
usb-xhci-boot-mouse-recurring-probe = ["usb-xhci-boot-mouse-decode-probe"]
```

Where the one-shot result/decode variables, execution branch, and final render branch use `usb-xhci-boot-mouse-decode-probe`, add `not(feature = "usb-xhci-boot-mouse-recurring-probe")`. Add separate recurring state so both enabled features cannot run two controller initializations.

- [ ] **Step 2: Implement exact recurring orchestration**

Factor a bounded helper in `usb_xhci_probe_boot.rs` that starts one session and loops over `1..=XHCI_BOOT_MOUSE_RECURRING_REPORTS`. Before each capture emit:

```text
XHCI_BOOT_MOUSE_REPORT_ORDINAL=<one-based hex>
XHCI_INTERRUPT_TRANSFER_TRB_INDEX=<next index>
XHCI_INTERRUPT_TRANSFER_CYCLE=<0 or 1>
```

The session then emits the existing requested/armed/completion/actual/captured/raw/ready transport markers. Immediately decode the captured prefix, call `summary.observe(decoded)`, emit the existing decoded value/event/decode markers, and emit `XHCI_BOOT_MOUSE_REPORT_READY=<ordinal>`.

When `sample.wrapped_after_completion` is true, emit `XHCI_INTERRUPT_TRANSFER_RING_WRAP=<wrap_count>`. On any driver or decoder failure, save the summary/progress, emit one typed error or `XHCI_BOOT_MOUSE_DECODE_INVALID`, stop, and do not arm again.

After ordinal 16, require summary count 16 and transfer wrap count 1, then emit these exact final fields using `u64::from(total as u32)` for signed totals:

```text
XHCI_BOOT_MOUSE_REPORT_COUNT=0x10
XHCI_BOOT_MOUSE_DX_TOTAL_I32=0x00000070
XHCI_BOOT_MOUSE_DY_TOTAL_I32=0xFFFFFFC8
XHCI_BOOT_MOUSE_BUTTONS_LAST=0x00
XHCI_BOOT_MOUSE_PRESSED_SEEN=0x01
XHCI_BOOT_MOUSE_RELEASED_AFTER_PRESSED=0x01
XHCI_BOOT_MOUSE_SEQUENCE_AUX_PRESENT=0x01
XHCI_BOOT_MOUSE_SEQUENCE_AUX_LAST=0x00
XHCI_INTERRUPT_TRANSFER_WRAP_COUNT=0x01
XHCI_EVENT_RING_WRAP_COUNT=<observed count>
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_RECURRING_READY
```

The numeric values shown are the QEMU fixture's expected results; production emits values from the actual summary. Render first, then emit framebuffer-ready, no-write, and overall-ready. Never emit recurring-ready from the error branch.

- [ ] **Step 3: Implement the minimal screen builders to satisfy the red tests**

Add a signed `i32` decimal formatter that handles zero and negative values without overflow, using a widened magnitude for `i32::MIN`. Render the exact final/error lines from Task 5 plus existing controller/endpoint identity. Keep line lengths within `ProbeLine` capacity and validate fixed-font glyph coverage.

The success renderer receives the endpoint result, final `XhciInterruptTransferProgress`, and `UsbBootMouseSequenceSummary`. The error renderer receives port/change identity, last progress/summary, and the typed driver error or explicit decode-failure state.

- [ ] **Step 4: Run screen, core, and cross-target tests to verify GREEN**

```powershell
cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_recurring -- --nocapture
cargo test -p pythos-core
cargo build -p pythos-core --target x86_64-unknown-none --features usb-xhci-boot-mouse-decode-probe
cargo build -p pythos-core --target x86_64-unknown-none --features usb-xhci-boot-mouse-recurring-probe
```

Expected: all tests/builds pass with earlier feature behavior preserved.

- [ ] **Step 5: Run recurring QEMU acceptance and fix only failures reproduced by tests**

```powershell
py -3 scripts/test-usb-xhci-boot-mouse-recurring-probe.py
```

Expected: sixteen ordered report groups, one transfer wrap, cycle-zero report 16, event-cycle wrap, totals `+112/-56`, left press/release evidence, `NO_DISK_WRITES`, `QEMU_OUTCOME success`, and `USB_XHCI_BOOT_MOUSE_RECURRING_PROBE_TEST_OK`.

If QEMU hangs, times out, emits an unexpected completion, or produces different report grouping, stop normal implementation and invoke `superpowers:systematic-debugging`: preserve the COM1 log, reproduce, add a failing regression test, then make the minimal fix.

- [ ] **Step 6: Re-run the one-shot and endpoint prerequisites**

```powershell
py -3 scripts/test-usb-xhci-endpoint-configuration-probe.py
py -3 scripts/test-usb-xhci-interrupt-transfer-probe.py
py -3 scripts/test-usb-xhci-boot-mouse-decode-probe.py
```

Expected: all three old stable success lines and `QEMU_OUTCOME success`; one-shot marker counts remain one.

- [ ] **Step 7: Commit the recurring production boundary**

```powershell
git add core/Cargo.toml core/src/usb_xhci_probe_boot.rs core/src/usb_xhci_probe_screen.rs
git commit -m "feat: capture recurring xhci mouse reports"
```

---

### Task 7: ADR, Evidence Correction, Full Regression, and Checkpoint

**Files:**
- Create: `docs/decisions/0088-usb-xhci-recurring-boot-mouse-probe.md`
- Modify: `docs/PythOS-TDD-001.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Modify: `README.md`
- Modify: `docs/decisions/0087-usb-xhci-boot-mouse-decode-probe.md`
- Modify: `docs/evidence/2026-09-04-physical-usb-xhci-boot-mouse-decode-report.md`
- Modify: `D:\PythOS-Workspace\CURRENT-STATE.md`

**Interfaces:**
- Consumes: fresh host/QEMU outputs and artifact hashes only.
- Produces: auditable QEMU-only ADR 0088 status, corrected ADR 0087 physical media record, and an external workspace checkpoint.

- [ ] **Step 1: Record ADR 0088 from fresh evidence**

Document the selected single-in-flight approach, 15-data-plus-Link ring, transfer/event cycle state, exact 16-report terminal condition, typed failures, marker sequence, QEMU command/output, artifact/log SHA-256 values, and strict no-cursor/no-wheel/no-write boundary. Status must be `Accepted in QEMU; physical validation pending` until a later physical run.

- [ ] **Step 2: Correct the ADR 0087 evidence wording without inflating it**

Replace the obsolete `no physical media capture` statement with the two available physical observations and hashes:

```text
C:\Users\NeverAMoment\Desktop\Screenshot 2026-09-04 112914.png
SHA-256 5E31E4BF9E5E6571E2A11BB5B86023B3475EC8135747CB339246B262C1A8B4AE

C:\Users\NeverAMoment\Desktop\Screenshot 2026-09-04 142212.png
SHA-256 6990E21E42BB2CA3B347DA9B8E81A60C3572E21408737A20AEF328E211149D31
```

Record motion `btn 00 / dx -007 / dy -007 / aux 00`, dock-topology left state `btn 01 l1 r0 m0`, and the later neutral state after releasing a pre-held button. State explicitly that the last observation does not prove a PythOS-observed release transition because no preceding pressed report was accepted in that boot.

- [ ] **Step 3: Update the test and project status documents**

Add the recurring marker order and harness command to `docs/PythOS-TDD-001.md`. Add ADR 0088 status/non-goals and links to the technical overview and README. Update `CURRENT-STATE.md` only after verification, including branch/commit, test counts, QEMU-only status, physical test instructions, and the next write-approval boundary.

- [ ] **Step 4: Run formatting and static safety checks**

```powershell
cargo fmt --all --check
git diff --check
py -3 C:\Users\NeverAMoment\.codex\skills\pythos-kernel-engineer\scripts\scan-unsafe-rust.py .
py -3 C:\Users\NeverAMoment\.codex\skills\pythos-kernel-engineer\scripts\verify-driver-timeouts.py .
```

Expected: formatting/diff clean; no new undocumented unsafe block or unbounded hardware poll. Existing unrelated warnings must be named, not hidden.

- [ ] **Step 5: Run the mandatory host and normal-boot regression gates**

```powershell
cargo test -p pythos-core
py -3 -m pytest tests
py -3 scripts/test-boot.py
py -3 scripts/test-persistent-storage.py
```

Expected: all tests pass, `BOOT_TEST_OK`, persistent-storage stable success line, and every required `QEMU_OUTCOME success`. A timeout or missing success marker fails the task.

- [ ] **Step 6: Run the complete scoped xHCI QEMU matrix**

```powershell
py -3 scripts/test-usb-xhci-endpoint-configuration-probe.py
py -3 scripts/test-usb-xhci-interrupt-transfer-probe.py
py -3 scripts/test-usb-xhci-boot-mouse-decode-probe.py
py -3 scripts/test-usb-xhci-boot-mouse-recurring-probe.py
```

Expected: all four script success lines plus `QEMU_OUTCOME success`; only the recurring log contains recurring markers.

- [ ] **Step 7: Hash the verified artifacts and logs**

After the final recurring harness build, record SHA-256 for `target/esp/EFI/BOOT/BOOTX64.EFI`, `target/esp/PYTHOS/PYTHCORE.ELF`, and `target/usb-xhci-boot-mouse-recurring-probe-com1.log`. Re-run the recurring harness if any source or build input changes after hashing.

- [ ] **Step 8: Commit the documentation and external checkpoint state**

Commit repository documentation normally:

```powershell
git add docs/decisions/0088-usb-xhci-recurring-boot-mouse-probe.md docs/PythOS-TDD-001.md docs/TECHNICAL-OVERVIEW.md README.md docs/decisions/0087-usb-xhci-boot-mouse-decode-probe.md docs/evidence/2026-09-04-physical-usb-xhci-boot-mouse-decode-report.md
git commit -m "docs: record recurring xhci mouse acceptance"
```

`D:\PythOS-Workspace\CURRENT-STATE.md` is outside the repository; update it with `apply_patch` and report it separately rather than implying it is contained in the Git commit.

- [ ] **Step 9: Invoke verification-before-completion and inspect the final branch**

Read and follow `superpowers:verification-before-completion`, then verify:

```powershell
git status --short
git log --oneline --decorate -8
git diff 20116b381d490ee899cd62feb76bbcc1c5b34fe6..HEAD --stat
git rev-parse HEAD
git rev-parse --verify '@{u}'
```

The worktree must be clean except for the separately reported external `CURRENT-STATE.md`. Do not push, merge, deploy, or write the USB without the corresponding explicit authorization/finishing choice.

## Verification Matrix

- Host behavior: aggregation, press/release ordering, producer ownership, data-ring wrap, event-cycle wrap, stale-event rejection, screen formatting, QMP step/count actions.
- Old QEMU paths: endpoint configuration, one raw report, and one decoded report remain accepted with unchanged one-shot counts.
- New QEMU path: sixteen decoded groups, indices `0..14,0`, cycles `1..1,0`, transfer/event wrap proof, movement totals, left press/release, frozen framebuffer, no cursor, no writes.
- Normal regressions: complete PythCore tests, Python test suite, standard boot, and persistent storage.
- Physical hardware: pending a separately approved Lexar deployment and Lenovo boot; no claim in this plan.

## Non-Goals

- Cursor, view rotation, translation, scroll, or zoom behavior.
- Click, double-click, chord, or keyboard-sequence modes.
- Normal input-event service or compositor integration.
- IRQ/MSI/MSI-X USB input.
- USB hub or generic dock enumeration.
- USB mass storage, FAT, evidence volumes, or disk writes.
- Hot-unplug recovery, a second transfer-ring wrap, or an unbounded production ring.
