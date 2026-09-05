# ADR 0082: USB xHCI Address Device Probe

Date: 2026-09-01

Status: Accepted

## Context

ADR 0081 is now physically accepted on the current target: the scratchpad-aware
command-ring image completed No-op and Enable Slot on AMD `1022:7914`, returned
slot `01`, reported scratchpad count `08`, and preserved `no disk writes`.

The next USB mouse layer is Address Device. The xHCI specification describes
this as a command-ring operation that consumes an input context, writes an
output device context through the DCBAA slot entry, and moves the slot into an
addressed state before descriptor reads can begin. That requires more than the
ADR 0081 command ring: PythCore must prepare the input control context, slot
context, endpoint-0 context, and endpoint-0 transfer ring, then submit an
Address Device TRB with a finite completion wait.

This remains below USB HID. Addressing a device does not read descriptors,
configure non-control endpoints, parse HID reports, poll interrupt endpoints,
move a cursor, integrate with the shell, or support the built-in I2C trackpad.

Specification reference:

- Intel xHCI Requirements Specification, Revision 1.22:
  <https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf>

## Decision

Add `usb-xhci-address-probe` as a separate opt-in feature depending on
`usb-xhci-command-probe`. The existing register, port-status, swap-port, and
command-ring diagnostics keep their marker contracts unless the address feature
is explicitly enabled.

When enabled, PythCore:

1. runs the ADR 0081 command-ring setup through successful No-op and Enable
   Slot completion;
2. decodes the xHCI context size from `HCCPARAMS1.CSZ` and supports both
   32-byte and 64-byte context layouts;
3. zeroes and translates static page-aligned input and output context pages;
4. writes `DCBAA[slot]` to the output device context;
5. initializes the input control context A0/A1 flags for Slot Context and EP0;
6. initializes the Slot Context route/root-port/speed/context-entry fields;
7. initializes Endpoint 0 as a control endpoint, sets its dequeue pointer to a
   static transfer ring, sets DCS, and selects default-control max-packet size
   from the reset `PORTSC` speed;
8. submits one Address Device command TRB with the input-context physical
   address and returned slot id;
9. observes the command-completion code without collapsing non-success into a
   generic driver error;
10. reads back the output device address, slot state, and EP0 state; and
11. renders `xhci addr`, the command results, context size, max packet size,
    `PORTSC`, scratchpad count, and `no disk writes`.

The diagnostic uses polled MMIO and finite waits. It does not issue
GET_DESCRIPTOR, configure endpoints beyond the EP0 context needed for Address
Device, parse HID, move a cursor, enable xHCI interrupts, enter the shell, or
touch storage after loader handoff.

## Verification

TDD started with failing Rust tests for the new context helpers, DMA context
wiring, Address Device TRB encoding, and framebuffer address-result/error
screens. The red run failed because the address-device result type, helpers,
context buffers, TRB encoder, and renderer did not exist.

The green focused runs passed:

```text
cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture
cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_address_probe -- --nocapture
```

The accepted QEMU address-probe harness builds with
`--features usb-xhci-address-probe`, repeats the boot-USB detach plus mouse
hotplug sequence, then requires the command-ring markers plus address markers.

Observed QEMU markers from the accepted run:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SCRATCHPAD_COUNT=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_NOOP_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENABLE_SLOT_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SLOT_ID=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_CONTEXT_READY
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_CONTEXT_SIZE=0x0000000000000020
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_PORT_SPEED=0x0000000000000003
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_MPS=0x0000000000000040
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DEVICE_ADDRESS=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SLOT_STATE=0x0000000000000002
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_EP0_STATE=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_READY
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
QEMU_OUTCOME success
USB_XHCI_ADDRESS_PROBE_TEST_OK
```

The deployable address-probe image was copied to the verified `P:` USB ESP
target: Disk 2, Lexar D70E, USB bus, serial `1026R51254700477`, MBR active
FAT32 `PYTHOS_ESP`, not Windows boot/system. The deployment copied only the
current `image\esp` files, did not format the drive, and did not delete
preserved root files. Source-to-target readback reported
`USB_XHCI_ADDRESS_VERIFY_OK files:8 bytes:3977256`, and the deployed
`P:\PYTHOS\PYTHCORE.ELF` SHA-256 was
`E666859BFEE4FE6162690F3D8860E24992492441F859AEE6C8F4FC14DDBC3D53`.

The physical target then produced the matching `xhci addr` success panel. The
physical frame shows `no disk writes`, count `0000000000000002`, BDF
`00 10 00`, vendor/device `1022 7914`, port `05`, slot `01`, No-op completion
code `01`, Enable Slot completion code `01`, Address Device completion code
`01`, device address `01`, slot state `02`, EP0 state `01`, speed `02`,
context size `32`, max packet size `0008`, `PORTSC 00220A03`, and scratchpad
count `08`. The preserved frame is
`docs/evidence/2026-09-01-physical-usb-xhci-address-device-success.png`,
SHA-256
`8A4D2D6D8F74AEE88D2B535F4447CBDC338E590944F0D8130E7B6FD6476A6D5A`.

This ADR now makes a target-specific physical Address Device acceptance claim
for AMD `1022:7914`. It still makes no descriptor-read, HID, cursor,
interrupt-input, shell-input, or trackpad claim.

## Consequences

PythOS now has a QEMU-accepted and physical-target-accepted Address Device
diagnostic built on a photo-backed physical command-ring base. The next bounded
USB mouse slice can issue the first EP0 GET_DESCRIPTOR request and report the
returned descriptor bytes. Descriptor reads are still below HID report parsing
and cursor movement.

The feature stays opt-in because it mutates xHCI controller state and uses DMA.
It preserves the existing no-disk-writes evidence boundary and leaves the
default boot path unchanged.
