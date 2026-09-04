# USB xHCI Recurring Boot-Mouse Reports Design

Date: 2026-09-04

Status: Approved in conversation; awaiting review of this written specification.

## Goal

Extend the opt-in physical xHCI boot-mouse diagnostic from one decoded input
report to an exact sequence of sixteen decoded reports. The sequence must prove
that PythOS can re-arm the discovered interrupt-IN endpoint, preserve ordered
button and movement states, cross one transfer-ring Link TRB, and stop on a
stable framebuffer result.

This is the transport boundary immediately before cursor or camera behavior.
It does not add a cursor, route reports into the normal input service, interpret
the auxiliary byte as a wheel, recognize clicks or multi-click gestures, add
hub support, or write evidence to the boot USB.

## Current Evidence and Problem

The current `usb-xhci-boot-mouse-decode-probe` configures the physical AMD
`1022:7914` xHCI controller, queues one Normal TRB, captures one three- or
four-byte boot-mouse report, decodes it, renders it, and stops. Physical tests
have separately observed:

- motion: buttons `00`, `dx -007`, `dy -007`, auxiliary `00`;
- a left-button state: buttons `01`, `l1 r0 m0`, zero movement;
- a neutral state after releasing a button that was held before attachment:
  buttons `00`, `l0 r0 m0`.

The last observation does not prove that PythOS saw a release transition. A
button already held while the device is attached can be absent from the first
report PythOS accepts; seeing only the later neutral report supplies no earlier
pressed state to compare against. This design therefore requires press and
release states after PythOS has armed recurring transfers.

The existing transfer implementation cannot safely be placed in a loop:

- it always publishes at transfer-ring index zero with cycle state one;
- it does not maintain the interrupt endpoint's producer index or cycle state;
- event-ring polling accepts only cycle state one and does not toggle the
  expected cycle when the event consumer wraps;
- sixteen additional Transfer Events begin after the command and control events
  used during enumeration, so event-ring wrap is unavoidable in this test.

## Approaches Considered

### Sequential single-in-flight transfers

Publish one Normal TRB, ring the endpoint doorbell, wait for its matching
Transfer Event, capture and decode the completed buffer, then publish the next
TRB. Maintain explicit transfer-producer and event-consumer cursors.

This is the selected approach. It preserves the existing static DMA model,
keeps the controller and CPU ownership boundary simple, and gives each report a
deterministic serial evidence point.

### Prequeue sixteen transfers and buffers

Allocate sixteen report buffers, publish all data TRBs, and drain their events.
This would better resemble a higher-throughput driver, but it introduces
multiple simultaneous DMA ownership regions and more recovery states before
the single-in-flight path is proven on the physical target.

### Unbounded live polling loop

Continuously recycle the ring and update the display. This could feel more like
a real mouse driver, but it has no deterministic terminal condition, obscures
which ring transition failed, and is unsuitable as the next evidence boundary.

## Feature Boundary

Add a new opt-in feature named `usb-xhci-boot-mouse-recurring-probe`. It depends
on `usb-xhci-boot-mouse-decode-probe` so descriptor discovery, endpoint
configuration, bounded polling, raw capture, and boot-mouse decoding remain the
established lower layers.

When both feature flags are present, the recurring probe is the terminal
diagnostic path and the one-report framebuffer path must not run first. Builds
without the new feature remain behaviorally unchanged. This design makes no
boot ABI change.

The sequence target is a compile-time constant of sixteen successfully decoded
reports. It is deliberately tied to the present sixteen-entry transfer ring:
entries zero through fourteen are data entries and entry fifteen is the Link
TRB with Toggle Cycle set. Reports one through fifteen use data indices zero
through fourteen with cycle state one. Report sixteen reuses data index zero
with cycle state zero after the controller crosses the Link TRB. No seventeenth
report is armed.

## Ring State and DMA Ownership

### Transfer producer

The interrupt endpoint owns an explicit producer cursor containing:

- `data_index`, bounded to `0..=14`;
- `cycle`, initially `true`;
- `wrap_count`, initially zero.

For every report, PythOS:

1. verifies that no interrupt transfer is in flight;
2. zeroes the existing static 4 KiB report buffer;
3. writes a Normal TRB at `data_index` with the current cycle state, exact
   requested packet length, and Interrupt On Completion;
4. publishes the TRB with the existing compiler ordering fence;
5. rings the discovered endpoint doorbell;
6. waits for and validates the matching Transfer Event;
7. copies the received prefix before the DMA buffer is reused;
8. advances the cursor only after successful capture and decode.

Advancing from data index fourteen crosses the already initialized Link TRB,
sets `data_index` to zero, toggles the producer cycle to false, increments the
wrap count, and emits the one wrap marker. Because this boundary stops at report
sixteen, it does not republish the Link TRB for a second traversal. Supporting a
second transfer-ring wrap is a separate boundary.

Exactly one report buffer and one data TRB are controller-owned at a time. The
CPU must not zero or read the report buffer until the matching completion has
transferred ownership back. This preserves the existing single-core,
single-in-flight safety invariant.

### Event consumer

The event ring owns a consumer cursor containing:

- `event_index`, initially zero;
- `expected_cycle`, initially true.

Every command, control-transfer, and interrupt-transfer poll compares the event
cycle bit with `expected_cycle`, not with a hard-coded true value. After an
accepted event at entry fifteen, the consumer advances to entry zero and
toggles `expected_cycle`. The Event Ring Dequeue Pointer acknowledgement uses
the post-advance index exactly as it does now.

The cursor lives in the xHCI command-probe state from controller initialization
onward so all events share one ordered consumer history. This avoids guessing
the cycle state when the recurring phase starts partway through the event ring.

## Report Aggregation

Each successful raw capture must decode through the existing bounded standard
USB boot-mouse decoder. A capture outside three or four bytes is an immediate
failure rather than a skipped sample.

The recurring result contains only diagnostic evidence:

- successful report count;
- signed `i32` sums of all `dx` and `dy` values;
- the most recent decoded report;
- a three-bit `pressed_seen` mask, ORed from button states in all reports;
- a three-bit `released_after_pressed` mask, set only when adjacent accepted
  reports show a button changing from one to zero;
- whether any auxiliary byte was present and the latest raw auxiliary byte;
- transfer-ring wrap count.

`released_after_pressed` proves an observed state transition in this bounded
stream. It is not a click event, debounce policy, double-click detector, or user
interface action. Unknown button bits stay masked out by the existing decoder.
The auxiliary byte remains raw evidence and has no scroll or zoom meaning.

## Serial Evidence Contract

The normal xHCI discovery, enumeration, endpoint-configuration, and no-write
markers remain ordered and unchanged. The recurring path adds a stable marker
family under `PYTHOS:CORE:USB_XHCI_PROBE:`.

Before input:

```text
XHCI_BOOT_MOUSE_SEQUENCE_TARGET=0x10
```

For every report, serial output identifies the one-based ordinal, transfer TRB
index, transfer cycle, armed state, completion, raw bytes, and decoded values.
The ordinal marker must let the QEMU runner inject the next input only after the
corresponding transfer is armed. Existing raw-transfer and decoded-value marker
names may be repeated inside this sequence, but the acceptance test must check
their exact count and associate them with each ordinal.

The transfer-ring crossing emits exactly once:

```text
XHCI_INTERRUPT_TRANSFER_RING_WRAP=0x01
```

After report sixteen, serial output records the report count, signed delta
totals, last button state, pressed-seen mask, released-after-pressed mask,
auxiliary presence/latest value, and wrap count, then emits:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_RECURRING_READY
```

The standard framebuffer/no-write markers and
`PYTHOS:CORE:USB_XHCI_PROBE_READY` follow only after the final panel is rendered.
QEMU acceptance still requires `QEMU_OUTCOME success`. Timeout, panic, decode
failure, missing ordinal, duplicate terminal marker, marker-order violation, or
an incorrect marker count is failure.

## Framebuffer Behavior

The initial physical prompt continues to identify the xHCI controller and asks
the operator to attach the mouse. After sixteen valid reports, a final static
panel replaces the prompt and includes:

- `xhci mouse sequence` and `no disk writes`;
- controller BDF and vendor/device identity;
- selected root port, slot, endpoint address, and DCI;
- `reports 16 wrap 1`;
- latest left/right/middle state;
- pressed-seen and released-after-pressed masks;
- signed accumulated X and Y totals;
- latest raw auxiliary value or an explicit absent indicator;
- `frozen no cursor`.

The final panel does not redraw in response to later movement. In QEMU the
debug-exit success path terminates acceptance after the panel marker. On real
hardware the ignored debug-exit write leaves the existing non-returning
halt/spin path with the final panel visible.

## Failure Behavior

Any failure stops the sequence immediately. The error framebuffer identifies
the recurring-input stage, the number of reports completed, the next transfer
index/cycle, and the existing xHCI driver error. It must not claim the recurring
ready marker or `USB_XHCI_PROBE_READY`.

Specific hard failures include:

- a transfer or event cursor outside its bounded ring;
- a Transfer Event with the wrong TRB pointer, slot, or endpoint;
- a non-success/non-short-packet completion;
- residual length larger than the requested length;
- a report length the boot-mouse decoder rejects;
- timeout while waiting for any of the sixteen reports;
- a wrap count other than one at the terminal condition.

Disconnect recovery, endpoint reset, Stop Endpoint, Set TR Dequeue Pointer, and
device re-enumeration are not attempted in this boundary.

## Verification

Implementation follows test-driven development. Required host tests cover:

- transfer cursor indices and cycle states for reports one, fifteen, and
  sixteen;
- exactly one wrap across sixteen advances and no seventeenth arm;
- event cursor cycle matching and toggle on entry fifteen to zero;
- rejection of stale-cycle events after event-ring wrap;
- one-in-flight ownership and rejection of buffer reuse before completion;
- ordered aggregation of signed movement, latest state, pressed-seen, and
  released-after-pressed masks;
- report length/decode failures preserving the completed count;
- final screen text, fixed-font coverage, and the no-cursor/no-write labels.

A new QEMU acceptance script builds only the recurring feature path and drives
sixteen deterministic QMP mouse reports. Input is handshake-driven: it waits
for each report's armed ordinal before injecting that report, including an
explicit left-button press followed by release after arming. It asserts:

- all prerequisite xHCI markers in order;
- exactly sixteen armed, completed, captured, and decoded report groups;
- transfer indices `0..14,0` and cycle states `1..1,0`;
- exactly one transfer-ring wrap marker;
- at least one event-ring consumer-cycle wrap with no stale-cycle acceptance;
- the deterministic signed X/Y totals;
- left press and release-after-press evidence;
- exactly one recurring-ready, framebuffer-ready, no-write, and terminal probe
  marker;
- absence of cursor, normal input routing, disk-write, and panic markers;
- `QEMU_OUTCOME success` and a stable script success line.

Before any physical-media deployment, the normal regression gates required by
the repository remain mandatory, including the core unit suite and existing
boot/persistence acceptance scripts. A passing QEMU run proves the emulated
QEMU xHCI path only; it does not prove the Lenovo hardware result.

## Physical Acceptance

Deployment remains separately authorization-gated. Immediately before a write,
the operator and Codex must re-identify the exact removable target by disk
number, Lexar D70E model and serial, USB bus, partition geometry, filesystem,
label, and non-boot/non-system flags, then obtain explicit write approval.

The intended physical topology keeps the Lexar boot drive attached through the
BENFEI dock and attaches the mouse directly to the Lenovo laptop. After the
recurring armed prompt appears, the operator moves the mouse enough to produce
reports, presses and releases the left button while transfers are armed, and
continues until the panel automatically freezes at sixteen.

Physical acceptance requires the final panel to show `reports 16 wrap 1`, a
left pressed-seen bit, a left released-after-pressed bit, nonzero movement
evidence, and `frozen no cursor`. A photo or operator transcript is
target-specific evidence and must not be generalized to other controllers,
ports, docks, mice, or USB hubs.

## Out of Scope

- A visible cursor or pointer sprite.
- Camera/point-of-view rotation, WASD/arrow translation, or scroll-wheel zoom.
- Click, double-click, chord, keyboard-sequence, or mode-switch semantics.
- Routing physical reports to the normal input-event service or compositor.
- Interrupt-driven xHCI operation; this remains bounded polling.
- USB hubs, hub class requests, or generic dock enumeration.
- USB mass storage, SCSI, FAT, evidence volumes, or any disk write.
- Hot-unplug recovery or re-enumeration during the sixteen-report sequence.
- A second transfer-ring wrap or an unbounded reusable production ring.
