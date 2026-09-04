# ADR 0086: USB xHCI One-Shot Interrupt Transfer Probe

Date: 2026-09-03

Status: Accepted in QEMU; physical acceptance pending

## Context

ADR 0085 physically established a configured Dell/PixArt boot-mouse endpoint
on the Lenovo `81VS`: endpoint `0x81`, DCI 3, maximum packet size `4`, Running
Endpoint state, and Configured Slot state. It deliberately left the interrupt
transfer ring empty.

The next safe boundary is one raw interrupt-IN transfer. This must establish
that PythCore can receive device-owned bytes without prematurely claiming HID
decoding, recurring input, cursor behavior, shell input, or generic mouse
support.

Physical follow-up on 2026-09-04 exposed a false arrival before the mouse was
inserted. Immediately after the boot USB was removed, the Lenovo screen showed
`xhci input err`, code `0x11` (`UnexpectedCommandPointer`), for port 5 changing
from `PORTSC=0x000002A0` to `PORTSC=0x000202E1`. Because the mouse was not yet
connected, this is evidence that a single sampled disconnected-to-connected
edge is not sufficient device-arrival evidence. The downstream command-pointer
failure was therefore not treated as proof of a mouse transfer failure.

## Decision

Add `usb-xhci-interrupt-transfer-probe` as a separate opt-in feature depending
on `usb-xhci-endpoint-configuration-probe`. After the ADR 0085 transition,
PythCore:

1. treats a disconnected-to-connected port edge as a candidate only;
2. requires that same port to remain connected for 1,024 consecutive samples,
   resetting the candidate if it disconnects;
3. renders `move mouse once` before waiting;
4. zeros a dedicated page-aligned DMA report buffer;
5. writes exactly one cycle-1 Normal TRB at interrupt-ring index 0 with IOC;
6. requests the endpoint descriptor's payload size, bounded to the page;
7. rings the configured slot's DCI doorbell once;
8. emits `XHCI_INTERRUPT_TRANSFER_ARMED` and performs a long but finite polled
   wait intended for one human movement;
9. accepts Success (`1`) or Short Packet (`13`) only;
10. validates Transfer Event type, TRB pointer, slot ID, endpoint ID, and
   residual length before reading the DMA buffer;
11. captures at most the first eight received bytes, reports the exact actual
   and captured lengths, renders the raw bytes, and halts; and
12. retains the existing `NO_DISK_WRITES` terminal marker.

The typed failure identities added by this boundary are:

| Screen code | Failure |
|---|---|
| `0x3A` | one-shot interrupt transfer timeout |
| `0x3B` | non-success/non-short transfer completion |
| `0x3C` | unexpected transfer slot |
| `0x3D` | unexpected transfer endpoint |
| `0x3E` | invalid request or residual length |

This feature does not read the HID report descriptor, issue HID class
requests, interpret buttons or axes, queue a second report, move a cursor,
enter the object shell, enable xHCI interrupts, support the built-in I2C
trackpad, or write storage.

## Verification

TDD began with failing host tests for the Normal TRB, short-packet residual
accounting, event identity, dedicated report-buffer ownership, bounded capture,
timeout identity, and the pre-wait framebuffer prompt. The integration harness
was first observed failing because the feature did not yet exist.

The QEMU harness is:

```text
py -3 scripts\test-usb-xhci-interrupt-transfer-probe.py
```

It reuses the accepted boot-storage detach and mouse hotplug sequence, waits
two seconds after the guest reports the boot-storage port change before
hotplugging the mouse, waits for the guest's armed marker, injects one QMP
relative movement (`x=8`, `y=-4`), and requires one matching completion. The
delay is a regression for the physical failure: the guest must continue polling
through an empty human swap interval rather than consume the removal transient
as the mouse. The accepted run qualified the later connection at poll attempt
`0x139F` and reported:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_REQUESTED=0x0000000000000004
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_ARMED
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_ACTUAL=0x0000000000000004
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_CAPTURED=0x0000000000000004
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_RAW=0x0000000000FC0800
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_READY
PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
QEMU_OUTCOME success
USB_XHCI_INTERRUPT_TRANSFER_PROBE_TEST_OK
```

The packed value represents raw bytes `00 08 FC 00` in report order. The
harness requires a nonzero report, exact length consistency, marker ordering,
exactly one armed/ready pair, and absence of HID-report and cursor markers.

Fresh regression verification passed `cargo fmt --all -- --check`,
`git diff --check`, all 679 PythCore host tests, the stable-connection host
tests, the QEMU marker-delay unit test, the unchanged ADR 0085 QEMU harness,
the ADR 0086 QEMU harness, `scripts/test-boot.py` with `BOOT_TEST_OK` and the
default virtio block selection, `scripts/test-persistent-storage.py` with
`PERSISTENT_STORAGE_TEST_OK`, and `scripts/test-normal-boot-diagnostic.py` with
`NORMAL_BOOT_DIAGNOSTIC_TEST_OK`.

The final QEMU-accepted ADR 0086 artifacts are:

```text
BOOTX64.EFI   085A02AA250050CB55B065B7842B09CDE5C087291ABD19D83FA05F6197918578
PYTHCORE.ELF  5E5C53FB66903ECE0970B919B4BAF8AC69FE706CB8EEF52ADDB01A3A5BE601DE
serial log    754298A9CADC61949F2DEFBEADFA5B34C655FF563C2DC356D5D79124E0087F3B
```

This is QEMU acceptance only. It does not prove that the physical Lenovo and
Dell/PixArt mouse can complete this transfer. A separately authorized media
deployment, source-to-target hash readback, and physical result frame are
required before changing that status.

## Consequences

PythOS now has a bounded QEMU-accepted path from an empty configured endpoint
ring to one validated raw input report. The endpoint is not a persistent input
service: the diagnostic owns one TRB, one DMA buffer, one doorbell, one event,
and then stops.

The next possible separately approved boundary is raw boot-mouse report
decoding for buttons and signed movement. Recurring reports and a visible
cursor remain later work.
