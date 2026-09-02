# ADR 0083: USB xHCI Device Descriptor Probe

Date: 2026-09-01

Status: Accepted

## Context

ADR 0082 is now accepted in QEMU and on the physical AMD `1022:7914` target:
the diagnostic completes Enable Slot, prepares the xHCI input/output contexts
and endpoint-0 context, issues Address Device, and renders an addressed slot
with `no disk writes`.

The next USB mouse layer is the first endpoint-0 control transfer. The xHCI
specification models a USB control transfer as Setup Stage, optional Data
Stage, and Status Stage transfer TRBs, with completion reported by a Transfer
Event. For the first descriptor probe PythOS needs only one static 18-byte
device-descriptor buffer and one in-flight `GET_DESCRIPTOR(Device)` transfer on
endpoint 0.

This remains below HID. A successful device-descriptor read does not configure
the device, read the configuration descriptor, enable an interrupt endpoint,
parse HID reports, move a cursor, enter the shell, or support the built-in I2C
trackpad.

Specification reference:

- Intel xHCI Requirements Specification, Revision 1.22:
  <https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf>

## Decision

Add `usb-xhci-descriptor-probe` as a separate opt-in feature depending on
`usb-xhci-address-probe`. The existing register, port-status, swap-port,
command-ring, and Address Device diagnostics keep their marker contracts unless
the descriptor feature is explicitly enabled.

When enabled, PythCore:

1. runs the ADR 0082 flow through a successful Address Device command;
2. zeroes and translates a static page-aligned 18-byte descriptor DMA buffer;
3. queues one endpoint-0 control transfer on the existing EP0 transfer ring:
   Setup Stage `GET_DESCRIPTOR(Device)`, Data Stage IN for 18 bytes, and Status
   Stage OUT;
4. rings the selected slot's doorbell for endpoint 0;
5. polls the event ring with finite waits until the Status Stage transfer event
   is observed;
6. records the descriptor transfer completion code without collapsing
   non-success into a generic panic path;
7. reads the descriptor DMA buffer through volatile byte reads;
8. parses length, descriptor type, USB BCD, device class/subclass/protocol,
   EP0 max packet size, VID, PID, device BCD, string indexes, and configuration
   count; and
9. renders `xhci desc`, the address and descriptor completion codes, parsed
   descriptor fields, scratchpad count, and `no disk writes`.

The diagnostic does not read string descriptors, read the configuration
descriptor, configure a non-control endpoint, parse HID report descriptors,
poll the mouse interrupt endpoint, move a cursor, enable xHCI interrupts, or
touch storage after loader handoff.

## Verification

TDD started with failing Rust tests for descriptor TRB encoding and parsing.
The first red run failed because the descriptor TRB helpers and parser did not
exist. A later red/green screen pass added the `xhci desc` success and error
framebuffer panels. One intermediate test caught an incorrect Setup Stage
transfer-length field; it was corrected to the fixed 8-byte setup packet.

The focused green runs passed:

```text
cargo fmt --check
py -3 -m py_compile scripts\test-usb-xhci-descriptor-probe.py
cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture
cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_descriptor_probe -- --nocapture
cargo build -p pythos-core --target x86_64-unknown-none --features usb-xhci-descriptor-probe
```

The accepted QEMU descriptor harness builds with
`--features usb-xhci-descriptor-probe`, repeats the boot-USB detach plus mouse
hotplug sequence, requires the ADR 0082 address markers, then requires
descriptor markers through `XHCI_DESCRIPTOR_READY`.

Observed QEMU markers from the accepted run:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DEVICE_ADDRESS=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SLOT_STATE=0x0000000000000002
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_EP0_STATE=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_READY
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_TRANSFER_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_LENGTH=0x0000000000000012
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_TYPE=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_USB_BCD=0x0000000000000200
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_CLASS=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_SUBCLASS=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_PROTOCOL=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_MPS0=0x0000000000000040
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_VENDOR=0x0000000000000627
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_PRODUCT=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_DEVICE_BCD=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_CONFIG_COUNT=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_READY
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
QEMU_OUTCOME success
USB_XHCI_DESCRIPTOR_PROBE_TEST_OK
```

The descriptor-probe image was copied to the verified `P:` USB ESP target:
Disk 2, Lexar D70E, USB bus, serial `1026R51254700477`, MBR active FAT32
`PYTHOS_ESP`, not Windows boot/system. The deployment copied only the current
`image\esp` files, did not format the drive, did not delete preserved report
files, and read back every copied hash. Source-to-target readback reported
`USB_XHCI_DESCRIPTOR_VERIFY_OK files:8 bytes:3996680`, and the deployed
`P:\PYTHOS\PYTHCORE.ELF` SHA-256 was
`CFE2381F38DA91E11A31F021B315B4B12C030DB3F296C8B591BA6DF0A5289924`.

The Linux Mint field-kit run `run-20260901-033724` confirmed the target-side
mouse expected by the physical PythOS descriptor test. The archived report
`docs/evidence/2026-09-01-linux-mint-usb-mouse-map.tar.gz` has SHA-256
`528058762026647AFA2D5FD94E6086128C6B4EDD88CE60880C573294AFA3006B`.
Linux observed the Dell/PixArt mouse on xHCI path
`/sys/bus/usb/devices/2-1`, VID/PID `413c:301a`, low speed, device descriptor
length `18`, descriptor type `1`, USB BCD `0200`, max packet size `8`, device
BCD `0100`, manufacturer index `1`, product index `2`, serial index `0`, and
configuration count `1`. Its interface is HID boot mouse
class/subclass/protocol `03/01/02` with interrupt IN endpoint `0x81`, max
packet size `4`, interval `10`. The built-in trackpad remains separate I2C HID
`ELAN0666:00 04F3:304B` at ACPI path `\_SB_.I2CD.TPDD`.

After deployment, the preserved physical PythOS descriptor frame
`docs/evidence/2026-09-01-physical-usb-xhci-device-descriptor-success.png`
matches all expected fields for the Dell/PixArt mouse: `xhci desc`,
`no disk writes`, BDF `00 10 00`, vendor/device `1022 7914`, port `05`, slot
`01`, Address Device CC `01`, descriptor CC `01`, length `12`, type `01`, USB
BCD `0200`, device BCD `0100`, class/subclass/protocol `00 00 00`, MPS0
`008`, configuration count `01`, VID/PID `413C 301A`, and scratchpad count
`08`. The frame SHA-256 is
`4204994560727C63A8F631A05CCECFA68C3FC20189E12A2834E621327FDA61B6`.

This ADR makes a QEMU descriptor-read claim, records the deployed physical test
image, and records photo-backed physical PythOS descriptor-read acceptance.

## Consequences

PythOS now has an opt-in xHCI descriptor diagnostic with QEMU acceptance,
deployment readback, and photo-backed physical descriptor acceptance. The
next bounded USB slice can read the configuration descriptor and prepare the
interrupt IN endpoint. HID report parsing and cursor movement remain later
work.

The feature stays opt-in because it mutates xHCI controller state and uses DMA.
It preserves the no-disk-writes boundary and leaves the default boot path
unchanged.
