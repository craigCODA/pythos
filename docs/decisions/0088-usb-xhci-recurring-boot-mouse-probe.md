# ADR 0088: USB xHCI Recurring Boot-Mouse Probe

Date: 2026-09-04

Status: Accepted in QEMU; physical validation pending

## Context

ADR 0087 stops after one decoded USB boot-mouse report. That establishes one
bounded semantic sample, but it does not prove safe DMA-buffer reuse, ordered
press/release observation, transfer-ring wrap, or event-ring cycle-state wrap.
The next diagnostic boundary must prove those mechanics without becoming a
normal input driver or changing the boot ABI.

The interrupt transfer ring has sixteen entries. Entries 0 through 14 are
Normal TRBs and entry 15 is a Link TRB with Toggle Cycle set. Sixteen accepted
reports are therefore the smallest deterministic sequence that crosses the
Link TRB and reuses data entry 0 with the opposite cycle state.

## Decision

Add `usb-xhci-boot-mouse-recurring-probe` as an opt-in feature depending on the
ADR 0087 decode feature. It uses one `XhciInterruptTransferProbeSession` and a
strictly sequential single-in-flight loop:

1. verify that no transfer is already in flight;
2. prepare and publish one Normal TRB and one static report buffer;
3. ring the discovered interrupt-IN endpoint;
4. accept the matching Transfer Event through the shared event consumer;
5. copy and decode the received three- or four-byte prefix;
6. return DMA ownership to the CPU and only then advance the producer; and
7. repeat until exactly sixteen reports have been decoded.

The transfer producer owns `data_index`, `cycle`, and `wrap_count`. Reports 1
through 15 use data indices `0..14` with cycle state 1. Crossing the Link TRB
toggles the producer, and report 16 reuses data index 0 with cycle state 0.
The event consumer independently owns `event_index` and `expected_cycle`,
toggles its expected cycle when entry 15 advances to entry 0, and rejects stale
cycle-state events. No seventeenth report is armed.

The terminal invariant requires all of the following: report count 16,
completed-transfer count 16, and transfer wrap count 1. The result aggregates
signed X/Y totals, latest button state, buttons observed pressed, adjacent
release-after-press transitions, auxiliary-byte presence/latest raw value,
and transfer/event wrap counts.

Failures remain typed as an xHCI driver error, boot-mouse decode error, or
terminal-invariant error. Any failure stops immediately, preserves the last
progress/summary for the framebuffer error panel, and does not emit recurring
or overall readiness.

This boundary does not move or draw a cursor, interpret the auxiliary byte as
a wheel, recognize clicks, route input into the normal input-event service or
compositor, use IRQ/MSI/MSI-X USB input, support hubs, recover hot unplug,
traverse the transfer ring a second time, or write storage.

## Serial and Framebuffer Contract

The sequence starts with:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_SEQUENCE_TARGET=0x0000000000000010
```

Each report then emits, in order, its ordinal, transfer TRB index/cycle,
request/armed markers, completion/actual/captured/raw evidence, transfer-ready,
decoded fields, decode-ready, and report-ready ordinal. Crossing the Link TRB
emits exactly one:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_RING_WRAP=0x0000000000000001
```

After report 16 the accepted terminal order is:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_REPORT_COUNT=0x0000000000000010
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DX_TOTAL_I32=0x0000000000000070
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DY_TOTAL_I32=0x00000000FFFFFFC8
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_BUTTONS_LAST=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_PRESSED_SEEN=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_RELEASED_AFTER_PRESSED=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_SEQUENCE_AUX_PRESENT=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_SEQUENCE_AUX_LAST=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_WRAP_COUNT=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_EVENT_RING_WRAP_COUNT=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_RECURRING_READY
PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
PYTHOS:CORE:USB_XHCI_PROBE_READY
```

The final framebuffer is static and includes `reports 16 wrap 1`, the last
button state, press/release masks, signed motion totals, raw auxiliary evidence,
and `frozen no cursor`.

## Verification

Fresh QEMU acceptance on the final implementation tree used:

```text
py -3 scripts\test-usb-xhci-boot-mouse-recurring-probe.py
```

The harness built only the recurring feature path, detached the simulated boot
USB, delayed mouse hotplug, and injected one deterministic QMP input action
after each new armed marker. It observed ordinals `1..16`, indices
`0..14,0`, cycles `1..1,0`, sixteen requested/armed/raw/transfer-ready/
decode-ready/report-ready groups, one transfer wrap, one event wrap, the
terminal values above, and exactly one readiness/no-write terminal. It ended
with:

```text
QEMU_OUTCOME success
USB_XHCI_BOOT_MOUSE_RECURRING_PROBE_TEST_OK
```

The same final tree retained QEMU acceptance for endpoint configuration, the
one-shot raw interrupt report, and the one-shot decoded report; normal boot
ended with `BOOT_TEST_OK` / `QEMU_OUTCOME success`; persistent storage ended
with `PERSISTENT_STORAGE_TEST_OK` and its required success outcomes; and all
697 PythCore host tests passed.

The final recurring harness rebuilt `image/esp`. The repository does not
produce a `target/esp` directory; the freshly verified build outputs and COM1
log have these SHA-256 values:

```text
image/esp/EFI/BOOT/BOOTX64.EFI                         085A02AA250050CB55B065B7842B09CDE5C087291ABD19D83FA05F6197918578
image/esp/PYTHOS/PYTHCORE.ELF                          974490964C5AB48AE711694D8ECCD92121C28A9A0A33CE5630EEF919B0FDD096
target/usb-xhci-boot-mouse-recurring-probe-com1.log    C83D79B4E17CE155A57102FE048597AD1E8E4354681D2E32550A0C9082FE94E6
```

This is QEMU xHCI evidence only. No recurring image has been deployed to USB
and no physical recurring report sequence has been accepted.

## Consequences

PythOS now has a deterministic emulator proof for bounded recurring boot-mouse
transport, ordered decode/aggregation, one transfer-ring cycle transition, and
event-ring cycle-state handling. Physical validation remains separately gated:
the exact removable target must be re-identified, a write must be explicitly
approved, and the Lenovo/Dell-PixArt run must reach the frozen sixteen-report
panel before this status can be expanded.
