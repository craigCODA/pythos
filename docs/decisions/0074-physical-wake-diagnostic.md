# ADR 0074: Physical Wake Diagnostic Gate

Date: 2026-08-26
Status: Accepted

## Context

The current verify image has now been prepared on a USB ESP and observed by the
operator reaching the native Phase 6 wake screen on physical hardware:
`PythOS [HISS] We Are Woken`. That proves the physical UEFI loader path,
PythCore handoff, GOP framebuffer rendering, and enough of the verify sequence
to reach the cinematic frame on this machine. It does not prove physical
keyboard, USB HID, trackpad, shell input, or generic hardware support.

ADR 0047 explicitly deferred an interactive wake/login gate. ADR 0053 later
added a QEMU-scoped legacy PS/2 driver for the normal object-shell launcher,
but its scope remains QEMU's emulated i8042/PS/2 path. This diagnostic reuses
only the minimal i8042 polling mechanics needed to discover whether the current
physical machine exposes a usable keyboard byte stream after `ExitBootServices`.

## Decision

Add an opt-in `physical-wake-diagnostic` Cargo feature that requires `verify`.
When enabled, PythCore pauses immediately after
`PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY`, overlays a framebuffer diagnostic panel
on the settled wake screen, and waits for the exact `wake` plus Enter keyboard
sequence.

The diagnostic initializes only PS/2 controller port 1, disables PS/2 IRQ
delivery in the controller config, does not unmask IRQ1, does not enable mouse
streaming, and polls the controller output buffer from normal context. The
panel shows the typed wake buffer and the most recent 16 raw bytes in hex. The
recognizer accepts the exact set-1 sequence for `wake` plus Enter and the exact
set-2 sequence after detecting the initial `w`.

New serial markers are diagnostic-only:

```text
PYTHOS:CORE:PHYSICAL_WAKE:ENTER
PYTHOS:CORE:PHYSICAL_WAKE:READY
PYTHOS:CORE:PHYSICAL_WAKE:REJECTED
PYTHOS:CORE:PHYSICAL_WAKE:ACCEPTED
PYTHOS:CORE:PHYSICAL_WAKE:PS2_INIT_FAILED
```

`scripts/test-physical-wake-diagnostic.py` is the QEMU acceptance harness. It
builds the opt-in feature image, waits for `READY`, types `wake` plus Enter via
QMP key events, and treats `ACCEPTED` as the success marker. Ordinary
`scripts/test-boot.py --slice milestone-1` remains the default verify oracle
without this feature.

## Consequences

Default builds are unchanged. The diagnostic can produce physical evidence for
one machine's post-firmware keyboard byte path, but it does not add a USB HID
stack, trackpad support, interrupt-driven physical input, a login/auth surface,
shell control from the physical keyboard, or generic PC input support.

If the panel shows `ps2 init failed`, the current machine likely lacks a usable
legacy PS/2 controller path after firmware exit. If the raw byte line changes
but `wake` is not accepted, the next step is to update the decoder from those
visible bytes. If no raw bytes change, the next hardware-input path is USB HID
or another platform-specific keyboard controller, not more PS/2 assumptions.
