# ADR 0075: Physical Input Event Diagnostic

Date: 2026-08-27
Status: Accepted

## Context

ADR 0074 proved a narrower question: the opt-in verify image can reach the
native Phase 6 wake screen and, on one physical boot machine, receive enough
keyboard bytes to accept exact `wake` plus Enter. That does not prove that the
raw bytes can be normalized into the existing typed input-event shape, and it
does not prove shell keyboard control.

The next bounded question is whether the same post-firmware polling path can
observe a small event sequence that includes `Space`, `Backspace`, letters, and
`Enter`, then report both raw bytes and normalized key events. This stays below
the production input service and does not extend hardware claims to USB HID,
trackpads, IRQ-driven input, or generic PC keyboards.

## Decision

Add an opt-in `physical-input-event-diagnostic` Cargo feature that requires
`verify`. When enabled, PythCore pauses immediately after
`PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY`, initializes only PS/2 controller port 1
for polling, overlays a framebuffer diagnostic panel, and waits for this exact
sequence:

```text
space space backspace backspace wake enter
```

The diagnostic decodes only the fixed key set needed for the sequence:
`Space`, `Backspace`, `W`, `A`, `K`, `E`, and `Enter`. It accepts translated
scancode-set-1 bytes and the equivalent small scancode-set-2 byte set. It logs
recent raw bytes, a compact key-event history, the editable text buffer, and a
status line on the framebuffer.

New serial markers are diagnostic-only:

```text
PYTHOS:CORE:PHYSICAL_INPUT:ENTER
PYTHOS:CORE:PHYSICAL_INPUT:READY
PYTHOS:CORE:PHYSICAL_INPUT:RAW:<hex>
PYTHOS:CORE:PHYSICAL_INPUT:KEY:SPACE
PYTHOS:CORE:PHYSICAL_INPUT:KEY:BACKSPACE
PYTHOS:CORE:PHYSICAL_INPUT:KEY:W
PYTHOS:CORE:PHYSICAL_INPUT:KEY:A
PYTHOS:CORE:PHYSICAL_INPUT:KEY:K
PYTHOS:CORE:PHYSICAL_INPUT:KEY:E
PYTHOS:CORE:PHYSICAL_INPUT:KEY:ENTER
PYTHOS:CORE:PHYSICAL_INPUT:REJECTED
PYTHOS:CORE:PHYSICAL_INPUT:ACCEPTED
PYTHOS:CORE:PHYSICAL_INPUT:PS2_INIT_FAILED
```

`scripts/test-physical-input-event-diagnostic.py` is the QEMU acceptance
harness. It builds the opt-in feature image, waits for `READY`, injects the
sequence through QMP, requires ordered normalized key markers, and treats
`ACCEPTED` as the success marker.

## Consequences

Default builds are unchanged. ADR 0074's wake-only diagnostic remains
available and unchanged.

This diagnostic proves the QEMU path from raw keyboard bytes to typed key-event
markers for the fixed sequence. It does not yet prove that the current physical
boot machine accepts this wider event sequence; that requires a separate USB
image copy and operator boot report. Even after a physical acceptance report,
the claim remains scoped to this polling diagnostic on that machine, not USB
HID, trackpad input, IRQ-driven input, shell keyboard control, or generic
hardware support.
