# ADR 0087: USB xHCI One-Shot Boot-Mouse Decode Probe

Date: 2026-09-04

Status: Accepted in QEMU and user-observed on Lenovo 81VS; no physical media capture

## Context

ADR 0086 established one validated four-byte interrupt-IN report in QEMU and
on the Lenovo `81VS`. The physical Dell/PixArt report was `00 FE 00 00`, but
that boundary deliberately stopped before assigning HID meaning to the bytes.

PythOS already has typed mouse movement events, but its existing three-byte
`MouseDriver` decoder is PS/2-specific and requires the PS/2 byte-zero
synchronization bit. A USB boot-mouse report does not carry that requirement,
so routing USB bytes through the PS/2 decoder would reject the physically
captured report and would conflate two protocols.

## Decision

Add `usb-xhci-boot-mouse-decode-probe` as a separate opt-in feature depending
on `usb-xhci-interrupt-transfer-probe`. After the one-shot transfer completes,
PythCore:

1. accepts only a three- or four-byte report for this bounded decoder;
2. decodes standard button bits 0 through 2 as left, right, and middle state;
3. interprets bytes 1 and 2 as signed eight-bit X and Y movement;
4. maps that movement to the existing `RawInputEvent::MouseMoved` type;
5. retains a fourth byte, when present, as raw `aux` evidence without calling
   it a wheel;
6. emits the decoded button, signed-axis-byte, auxiliary, event-ready, and
   decode-ready serial markers;
7. renders the decoded button states, signed movement, and raw auxiliary byte
   on the final framebuffer panel; and
8. halts without queuing another transfer.

The USB decoder is distinct from the PS/2 packet decoder. This boundary does
not synthesize a mouse-button transition because one isolated state snapshot
has no prior USB state against which to prove a transition.

This feature does not interpret the auxiliary byte, queue a second report,
move a cursor, connect xHCI to the normal launcher loop, enable xHCI
interrupts, support the built-in I2C trackpad, or write storage.

## Verification

TDD began with failing host tests for the physically captured report, signed
axis extremes, standard button bits, the optional auxiliary byte, invalid
lengths, and signed framebuffer formatting.

The QEMU acceptance harness is:

```text
py -3 scripts\test-usb-xhci-boot-mouse-decode-probe.py
```

It reuses the ADR 0086 detach, delayed mouse hotplug, stable-connection gate,
and one-motion injection. The accepted run captured `00 08 FC 00` and emitted:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_READY
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_BUTTONS=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DX_I8=0x0000000000000008
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DY_I8=0x00000000000000FC
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_AUX_PRESENT=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_AUX=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_EVENT_READY
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DECODE_READY
PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
QEMU_OUTCOME success
USB_XHCI_BOOT_MOUSE_DECODE_PROBE_TEST_OK
```

The accepted verification set also includes all 683 PythCore host tests, the
unchanged ADR 0086 QEMU harness, and the normal `scripts/test-boot.py` path with
`BOOT_TEST_OK` and `QEMU_OUTCOME success`.

The final QEMU-accepted ADR 0087 artifacts are:

```text
BOOTX64.EFI   085A02AA250050CB55B065B7842B09CDE5C087291ABD19D83FA05F6197918578
PYTHCORE.ELF  FBE7D2C7FE8B21EB0A5A68B491B4F239A650F75293A0A73685A7309C919EC31B
serial log    33E8E44709E8BD09098AFC5F067174117D262A4F80FED621D002E77375976A63
```

The prior physical report `00 FE 00 00` is covered by a host test that decodes
buttons `00`, X `-2`, Y `0`, auxiliary `00`, and constructs
`RawInputEvent::MouseMoved { dx: -2, dy: 0 }`.

Physical acceptance followed on 2026-09-04 on the same Lenovo `81VS`, AMD xHCI
`1022:7914`, and Dell/PixArt mouse path used for ADR 0086. The deployed
`PYTHCORE.ELF` matched the QEMU-accepted SHA-256 above. The user directly
transcribed these final decoded-panel lines:

```text
btn 00 l0 r0 m0
dx -007 dy -007
aux 00
```

The user's phone lost power before a photograph or video could be recorded, so
this physical acceptance is explicitly user-observed and transcription-backed,
not media-backed. The lowercase `l0` is the literal left-button field rendered
by the source; it can resemble `10` in the fixed boot font. No firmware-setting
change was reported. The probe retained its one-report, no-cursor, and no-disk-
write boundary; the separately authorized USB deployment replaced eight boot
image files and preserved 111 unrelated files byte-for-byte.

## Consequences

PythOS now has a QEMU-accepted and user-observed Lenovo-accepted semantic bridge
from one validated xHCI report to its existing typed mouse-movement
representation while preserving the one-report safety boundary. Recurring
transfers, button-transition tracking, and visible cursor movement remain
later, separately approved boundaries.
