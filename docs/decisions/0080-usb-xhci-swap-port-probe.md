# ADR 0080: USB xHCI Swap Port Probe

Date: 2026-08-31

Status: Accepted

## Context

The current physical target may need the same external USB port for both the
boot USB drive and the USB mouse. ADR 0079 proves a QEMU-accepted one-shot xHCI
port-status snapshot with a mouse already present, but that is awkward for a
single-port physical test because the boot media occupies the port at snapshot
time.

The boot loader loads PythCore, `INIT.PAK`, and `FONT.PSF` into memory before
`ExitBootServices`. The USB/xHCI diagnostic path does not perform later block
device reads or writes, so the boot USB can be removed after PythCore has
rendered a diagnostic screen.

## Decision

Add `usb-xhci-swap-probe` as a separate opt-in feature depending on
`usb-xhci-port-probe`. The feature preserves ADR 0078 and ADR 0079 behavior
unless it is explicitly enabled.

When enabled, PythCore:

1. discovers and maps the selected xHCI controller exactly as ADR 0078 does;
2. takes the initial read-only ADR 0079 port-status snapshot;
3. renders a framebuffer prompt headed `swap mouse now`;
4. emits `PYTHOS:CORE:USB_XHCI_PROBE:SWAP_READY`;
5. polls the same read-only port-status registers for a bounded number of
   attempts;
6. compares each later snapshot to the current baseline and selects only a
   disconnected-to-connected `PORTSC` transition as the terminal mouse-insert
   event;
7. emits `XHCI_SWAP_POLL_IGNORED_CHANGE` for a boot-USB detach or other
   non-connect status change, rebases the comparison to that snapshot, and
   keeps polling;
8. renders a final framebuffer panel headed `xhci swap` with the connected
   port and before/after `PORTSC` values when a connect transition appears;
9. emits `PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES`.

The feature still does not reset xHCI, claim ownership from firmware, allocate
command/event rings, ring doorbells, enable USB interrupts, enumerate devices,
configure endpoints, parse HID reports, move a cursor, enter the shell, or
touch storage after the loader handoff.

## Verification

Host-side TDD added tests for the pure snapshot comparator and the framebuffer
swap prompt/change panels. The initial RED run failed on the missing
`XhciPortChange`, `first_changed_port`, and `build_swap_screen` symbols. A
second RED run for `scripts/test-usb-xhci-swap-probe.py` failed because
`scripts/run-qemu.py` did not yet support the QMP hotplug argument.

The first physical swap attempt exposed an event-selection bug: unplugging the
boot USB was the first observed port-status change, so the diagnostic rendered
the final `xhci swap` screen before the mouse was inserted and could not observe
the later mouse connection.

The follow-up RED test
`usb_xhci_probe::tests::reports_connected_port_after_ignored_usb_boot_disconnect`
failed on the missing connect-only comparator. The corrected implementation
selects only a `PORTSC` current-connect-status transition from 0 to 1, while
non-connect changes update the baseline and continue polling.

The updated QEMU harness builds with `--features usb-xhci-swap-probe`, boots
with `qemu-xhci`, attaches a simulated USB storage device, waits for
`SWAP_READY`, QMP-removes that storage device, waits for the kernel's
`XHCI_SWAP_POLL_IGNORED_CHANGE` marker, then QMP-hotplugs a `usb-mouse` onto
the emulated xHCI bus.

Observed QEMU markers from the accepted run:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_PORT_STATUS_READY
PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_SWAP_READY
PYTHOS:CORE:USB_XHCI_PROBE:SWAP_READY
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_START
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_CHANGE
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_NUMBER=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_BEFORE_PORTSC=0x0000000000001203
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_AFTER_PORTSC=0x00000000000202A0
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_ATTEMPT=0x000000000000013D
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:FOUND
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:NUMBER=0x0000000000000005
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:BEFORE_PORTSC=0x00000000000002A0
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:AFTER_PORTSC=0x0000000000020EE1
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_CHANGED
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
PYTHOS:CORE:USB_XHCI_PROBE_READY
QEMU_OUTCOME success
USB_XHCI_SWAP_PROBE_TEST_OK
```

After QEMU acceptance, the candidate was copied to the verified `P:` USB ESP
target without formatting or a delete pass. The target was re-identified as
Disk 2, Partition 1, Lexar D70E USB, serial `1026R51254700477`, MBR, active
FAT32 `PYTHOS_ESP`, not Windows boot/system. Read-only `chkdsk P:` reported no
filesystem problems. Source-to-target readback reported
`USB_XHCI_SWAP_CONNECT_VERIFY_OK files:8 bytes:3857656`; the deployed
`P:\PYTHOS\PYTHCORE.ELF` SHA-256 was
`A11B480D37A0C0299B4D6D96080C506C533BC0D1E3492CE1876C3F4F1A269BFE`.
Existing root files such as `LINUX-USB-MOUSE-MAP.SH` and the Linux mouse-map
archive were preserved.

The corrected physical image was then booted on the target. A retained still
from the operator-provided video shows the final framebuffer panel headed
`xhci swap`, with `chg p5`, `was sc 000002A0`, and `now sc 000202E1` after the
boot USB was removed and the external USB mouse was inserted. The still is
stored as `docs/evidence/2026-08-31-physical-usb-xhci-swap-port.jpg`; SHA-256
is `20B2CCA74EB8FD23080943FB368147F3D56A884A09F753479A9E4D5FF9A038E8`.

## Consequences

The physical port-connect question for the corrected swap image is answered for
the current AMD `1022:7914` target. PythOS can keep polling after the boot USB
is removed and can observe the later external mouse connection at the xHCI
root-port register layer.

This is still only a port-status observation layer. The remaining USB mouse work
is controlled xHCI ownership/initialization, DMA-safe command and event rings,
device enumeration, endpoint setup, HID descriptor/report parsing,
pointer-event integration, and physical cursor movement proof. ADR 0081 starts
that next command-ring diagnostic layer in QEMU only.
