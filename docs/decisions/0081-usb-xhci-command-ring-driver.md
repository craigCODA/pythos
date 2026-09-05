# ADR 0081: USB xHCI Command Ring Driver Diagnostic

Date: 2026-09-01

Status: Accepted

## Context

ADR 0078 proved xHCI register reachability, ADR 0079 proved read-only
port-status snapshots, and ADR 0080 proved a swap-friendly way to keep polling
after boot-USB removal until a later mouse-connect transition appears. The next
USB mouse step is no longer just a probe. It has to begin driver behavior:
reset the xHCI controller, allocate DMA-visible command/event structures, ring
doorbell 0, and consume command-completion events.

This step is still below USB HID. A successful Enable Slot command proves only
that the controller can execute the first xHCI command path and return a slot
identifier. It does not address the device, read descriptors, configure
endpoints, parse HID reports, or move a cursor.

## Decision

Add `usb-xhci-command-probe` as a separate opt-in feature depending on
`usb-xhci-swap-probe`. The inherited register, port-status, and swap-port
features remain unchanged unless this new feature is explicitly enabled.

When enabled, PythCore:

1. uses the ADR 0080 swap-port flow to find the disconnected-to-connected root
   port after the boot USB is removed and the mouse is inserted;
2. maps a bounded xHCI MMIO window large enough for the operational, runtime,
   and doorbell registers used by the diagnostic;
3. stops and resets the controller with finite polling loops;
4. rejects unsupported page-size, too-many-scratchpad-buffer, invalid-slot, and
   insufficient-MMIO-window cases with typed serial markers and framebuffer
   `xhci cmd err` codes;
5. configures static page-aligned DMA structures for the Device Context Base
   Address Array, scratchpad pointer array, scratchpad pages, command ring,
   event ring, and Event Ring Segment Table;
6. writes `DCBAAP`, `CRCR`, interrupter 0 `ERSTSZ`, `ERSTBA`, `ERDP`,
   `CONFIG`, and `USBCMD`;
7. resets the selected connected root port;
8. submits a No-op Command TRB, rings host doorbell 0, and waits for a command
   completion event with completion code `1`;
9. submits an Enable Slot Command TRB, rings host doorbell 0, waits for command
   completion code `1`, and records the returned slot id;
10. renders a final framebuffer panel headed `xhci cmd` with port number,
    slot id, completion codes, `USBSTS`, `PORTSC`, scratchpad count, and
    `no disk writes`.

The diagnostic uses polled MMIO and finite waits. It does not enable xHCI
interrupts, address a USB device, create input/output contexts, evaluate
contexts, configure endpoints, issue control transfers, read USB descriptors,
parse HID descriptors or reports, integrate pointer events, move a cursor,
enter the shell, or touch storage after loader handoff.

## Verification

Host-side TDD added pure tests for TRB type/control encoding, xHCI scratchpad
count decoding, bounded scratchpad support, DCBAA[0] scratchpad pointer-array
wiring, command-completion event decoding, command MMIO-window sizing,
root-port reset write preservation, and framebuffer success/error panels.

The accepted QEMU command-probe harness builds with
`--features usb-xhci-command-probe`, boots with `qemu-xhci`, attaches simulated
USB storage for the boot-USB-removal case, waits for `SWAP_READY`, removes the
storage device, waits for `XHCI_SWAP_POLL_IGNORED_CHANGE`, hotplugs a QEMU USB
mouse, then requires the command-ring driver markers through Enable Slot
completion.

Observed QEMU markers from the accepted run:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_CHANGE
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_CHANGED
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_START
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SCRATCHPAD_COUNT=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONTROLLER_RESET_READY
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_COMMAND_RING_READY
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_EVENT_RING_READY
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_USBSTS=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_PORT_RESET_READY
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_PORT=0x0000000000000005
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_PORTSC=0x0000000000220E03
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_NOOP_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_NOOP_COMMAND_COMPLETE
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENABLE_SLOT_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SLOT_ID=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENABLE_SLOT_READY
PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
PYTHOS:CORE:USB_XHCI_PROBE_READY
QEMU_OUTCOME success
USB_XHCI_COMMAND_PROBE_TEST_OK
```

The QEMU run used controller `1B36:000D`, BAR0 `0x000000C000000000`, DBOFF
`0x2000`, RTSOFF `0x1000`, and mouse connect on port 5.

On 2026-09-01, the first physical command-ring boot reached the command-driver
error panel after the same swap-port mouse connect: `chg p5`,
`was sc 000002A0`, `now sc 000202E1`, `err 00000006`, and `no disk writes`.
The preserved frame is
`docs/evidence/2026-09-01-physical-usb-xhci-command-scratchpad-error.png`,
SHA-256
`A6D6271A065EA3B6547A28F69CCFDD37484F10B4266105A980255D2B1B24CB2E`.
Driver error code `6` maps to `UnsupportedScratchpadBuffers`, so the next
bounded change was to support scratchpads rather than continue to command
execution with an incomplete DCBAA.

The refreshed diagnostic now supports up to 32 static page-aligned scratchpad
buffers. It decodes `HCSPARAMS2`, zeroes and translates a static scratchpad
pointer array and scratchpad pages, writes scratchpad page physical addresses
into the pointer array, and writes that pointer-array physical address into
`DCBAA[0]` when the controller reports nonzero scratchpad count.

The scratchpad-enabled image was deployed to the verified `P:` USB ESP target:
Disk 2, Lexar D70E, USB bus, serial `1026R51254700477`, MBR active FAT32
`PYTHOS_ESP`, not Windows boot/system. The deployment copied the `image\esp`
files without formatting the drive or deleting preserved root files.
Source-to-target readback reported
`USB_XHCI_SCRATCHPAD_VERIFY_OK files:8 bytes:3949296`, and the deployed
`P:\PYTHOS\PYTHCORE.ELF` SHA-256 was
`5E65C5A697A443369CB9AAC11E4AADAB7A26888B920EC89F43BEC5F33CF8CC44`.

The scratchpad-enabled target boot then produced the matching `xhci cmd`
success panel. The physical frame shows `no disk writes`, count
`0000000000000002`, BDF `00 10 00`, vendor/device `1022 7914`, port `06`,
slot `01`, No-op completion code `01`, Enable Slot completion code `01`,
`USBSTS 00000000`, `PORTSC 00220603`, and scratchpad count `08`. The preserved
frame is
`docs/evidence/2026-09-01-physical-usb-xhci-command-ring-success.png`,
SHA-256
`534B40C205D3BC4FE43F8BF0CBF6D0EFA0687E7F19E247BE08C94F818664AC52`.

This ADR now makes a target-specific physical command-ring acceptance claim for
AMD `1022:7914`. It still makes no USB addressing, descriptor-read, HID,
cursor, interrupt-input, shell-input, or trackpad claim.

## Consequences

PythOS now has the first opt-in xHCI write/DMA driver diagnostic after the
read-only probe stack, plus the scratchpad DMA setup required by the physical
AMD controller. The physical hardware path has reached No-op and Enable Slot
completion through the event ring and returned a slot id. The next bounded USB
mouse slice can build on that result by creating input/output device contexts,
issuing Address Device, reading descriptors through endpoint 0, and only then
moving toward HID report parsing and cursor movement.

The risk profile has changed from read-only observation to controller mutation
and DMA. The feature therefore stays opt-in, preserves no-disk-writes evidence,
uses bounded waits, reports typed failure codes on the framebuffer, and remains
outside the default boot path.
