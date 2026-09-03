# ADR 0085: USB xHCI Endpoint Configuration Probe

Date: 2026-09-03

Status: Accepted in QEMU; physical deployment and acceptance pending

## Context

ADR 0084 physically identifies the Dell/PixArt mouse's single HID boot-mouse
interface and interrupt-IN endpoint `0x81` on the Lenovo `81VS`. The descriptor
reports maximum packet size `4` and interval `10`. That discovery deliberately
stops before configuring a non-control endpoint or selecting the device
configuration.

The next safe boundary is endpoint configuration without input polling. xHCI
requires software to configure the endpoint context successfully before
issuing the USB `SET_CONFIGURATION` request. Reversing that order leaves host
controller behavior undefined. This slice therefore proves only the controller
and device configuration transition and does not enqueue an interrupt transfer.

Specification references:

- Intel xHCI Requirements Specification, Revision 1.2b:
  <https://cdrdv2-public.intel.com/625472/625472_xHCI_Rev1_2b.pdf>
- USB 2.0 Specification, Chapter 9:
  <https://www.usb.org/document-library/usb-20-specification>

## Decision

Add `usb-xhci-endpoint-configuration-probe` as a separate opt-in feature that
depends on `usb-xhci-configuration-probe`. When enabled, PythCore:

1. completes the ADR 0084 flow and obtains validated interrupt-IN endpoint
   metadata;
2. maps endpoint `0x81` to xHCI Device Context Index 3;
3. translates the descriptor interval for the observed port speed and validates
   the maximum packet size;
4. allocates a separate static, page-aligned 16-TRB interrupt transfer ring,
   zeros it, and installs only its Link TRB;
5. builds a fresh Input Context with Add Context flags A0 and A3, Context
   Entries 3, and the DCI 3 interrupt-IN Endpoint Context;
6. submits Configure Endpoint at command-ring index 3 and requires completion
   code `1`;
7. only after that success, submits the no-data USB
   `SET_CONFIGURATION(bConfigurationValue)` control TD at endpoint-0 ring
   indices 9 and 10 and requires completion code `1`;
8. reads the configured output Slot and DCI 3 Endpoint states; and
9. renders an `xhci ep cfg` framebuffer panel and halts without ringing the
   interrupt endpoint doorbell.

The endpoint ring is owned separately from the endpoint-0 ring. The transfer
ring dequeue pointer is 16-byte aligned and begins with DCS 1. No Normal TRB is
written to it in this slice. Endpoint 0 remains the only transfer-ring doorbell
used, for `SET_CONFIGURATION`.

The implementation supports the standard full-speed, low-speed, and high-speed
interrupt endpoint encodings required by the discovered device metadata. It
rejects endpoint zero, OUT endpoints, invalid descriptor intervals, unsupported
speeds, and invalid maximum packet sizes before submitting Configure Endpoint.
SuperSpeed endpoint configuration remains outside this bounded parser because
the companion descriptor is not yet consumed.

The new typed diagnostic identities are:

| Screen code | Failure |
|---|---|
| `0x32` | Configure Endpoint command timeout |
| `0x33` | `SET_CONFIGURATION` transfer timeout |
| `0x34` | Configure Endpoint non-success completion |
| `0x35` | `SET_CONFIGURATION` non-success completion |
| `0x36` | invalid interrupt-IN endpoint address |
| `0x37` | invalid interrupt interval |
| `0x38` | unsupported interrupt endpoint speed |
| `0x39` | invalid interrupt maximum packet size |

This diagnostic does not read a HID report descriptor, send HID class
requests, enqueue an interrupt-IN Normal TRB, ring the interrupt endpoint
doorbell, poll or parse reports, move a cursor, enter the object shell, enable
xHCI interrupts, support the built-in I2C trackpad, or write storage.

## Verification

TDD began with failing literal tests for endpoint-address-to-DCI mapping,
interval translation, endpoint-context words, Configure Endpoint and
`SET_CONFIGURATION` TRBs, 32-byte and 64-byte context layouts, separate ring
ownership, bounded endpoint-0 ring indices, and success/error framebuffer
panels. The focused endpoint-configuration driver suite passes 8 tests and the
focused framebuffer suite passes 2 tests.

The QEMU harness is:

```text
py -3 scripts\test-usb-xhci-endpoint-configuration-probe.py
```

It reuses the accepted detach/hotplug sequence, requires every ADR 0084 marker,
then requires the endpoint-context, Configure Endpoint, device-configuration,
and configured-state markers in order. It rejects driver errors and any HID
report, interrupt-transfer, or cursor-ready marker. The accepted run reported:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_PORT_SPEED=0x0000000000000003
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_ENDPOINT=0x0000000000000081
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_MPS=0x0000000000000004
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_INTERVAL=0x0000000000000007
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENDPOINT_ID=0x0000000000000003
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENDPOINT_CONTEXT_INTERVAL=0x0000000000000006
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURE_ENDPOINT_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SET_CONFIGURATION_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURED_SLOT_STATE=0x0000000000000003
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURED_ENDPOINT_STATE=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENDPOINT_CONFIGURATION_READY
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
QEMU_OUTCOME success
USB_XHCI_ENDPOINT_CONFIGURATION_PROBE_TEST_OK
```

QEMU enumerates its mouse at high speed, so descriptor interval `7` encodes as
Endpoint Context interval `6`. The physical Dell/PixArt mouse previously
enumerated at low speed with descriptor interval `10`; its physical Endpoint
Context encoding must be established by the later hardware run rather than
inferred from QEMU.

The unchanged ADR 0084 harness also passes with
`USB_XHCI_CONFIGURATION_PROBE_TEST_OK` and `QEMU_OUTCOME success`.

Fresh regression verification passed `cargo fmt --all -- --check`,
`git diff --check`, all 666 PythCore host tests, both the ADR 0084 and ADR 0085
cross-target feature builds, the unchanged ADR 0084 QEMU harness, the ADR 0085
QEMU harness, and `scripts/test-boot.py` with `BOOT_TEST_OK` and
`QEMU_OUTCOME success`.

The QEMU-accepted ADR 0085 ESP artifacts have these SHA-256 hashes:

```text
BOOTX64.EFI   085A02AA250050CB55B065B7842B09CDE5C087291ABD19D83FA05F6197918578
PYTHCORE.ELF  7BCEAD13881D8ED7455543B127AC072128FCA270102C181B0D3C75ECCAE653C7
```

An additional strict Clippy sample is not represented as green. With the
current Rust toolchain, `cargo clippy ... -- -D warnings` reports 17 style
lints in unchanged Phase 13 files, including `needless_return`,
`wrong_self_convention`, `let_and_return`, and `too_many_arguments`. None are
in this slice's changed USB/xHCI files, so they remain outside this ADR rather
than being silently rewritten.

## Consequences

PythOS now has a bounded, opt-in QEMU-accepted transition from parsed endpoint
metadata to a Running interrupt-IN Endpoint Context and a Configured USB device.
The interrupt ring exists but contains no input request, so this is not mouse
input support.

The next action is not HID polling. First, re-identify the removable USB target,
deploy the exact QEMU-accepted image without formatting or deleting unrelated
files, verify hashes by readback, and boot it on the Lenovo with the Dell/PixArt
mouse. Only a successful physical `xhci ep cfg` result can close this boundary.
Interrupt-IN polling and cursor behavior remain a later separately authorized
slice.
