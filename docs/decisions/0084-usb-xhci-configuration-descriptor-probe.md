# ADR 0084: USB xHCI Configuration Descriptor Probe

Date: 2026-09-02

Status: Accepted in QEMU; physical validation pending

## Context

ADR 0083 proves one endpoint-0 `GET_DESCRIPTOR(Device)` transfer in QEMU and
on the physical AMD `1022:7914` target. The device descriptor reports one
configuration, but it does not describe the interfaces or endpoints needed by
the external mouse.

The next bounded discovery step is to read standard configuration metadata.
USB exposes the total configuration length in the first nine bytes, so a safe
probe must read and validate that header before choosing the size of a second
transfer. This slice must remain below device activation and HID input.

Specification reference:

- Intel xHCI Requirements Specification, Revision 1.22:
  <https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf>

## Decision

Add `usb-xhci-configuration-probe` as a separate opt-in feature depending on
`usb-xhci-descriptor-probe`. When enabled, PythCore:

1. completes the ADR 0083 flow through a successful Device Descriptor read;
2. queues an endpoint-0 `GET_DESCRIPTOR(Configuration)` control transfer for
   the fixed nine-byte configuration header;
3. requires a successful transfer completion and validates descriptor type,
   minimum length, and `wTotalLength`;
4. rejects `wTotalLength` above a fixed 256-byte diagnostic cap;
5. queues a second configuration request for exactly the validated total
   length;
6. walks the returned descriptors with checked `bLength` progression;
7. records the selected standard interface fields and its interrupt-IN
   endpoint address, attributes, maximum packet size, and interval; and
8. renders an `xhci cfg` framebuffer panel plus typed error codes and stable
   serial markers.

The existing 16-entry endpoint-0 transfer ring reserves its final Link TRB.
Three sequential control TDs use slots `0..=2`, `3..=5`, and `6..=8`. A
caller-owned bounded index rejects any TD that cannot fit before the Link TRB;
this slice does not wrap the ring, so the cycle state remains `true`.

The configuration parser accepts only bounded standard descriptor walking. It
requires a valid configuration header, an interface, and an interrupt-IN
endpoint. Malformed lengths, overruns, oversized totals, missing interfaces,
missing interrupt-IN endpoints, non-success transfer completions, and control
ring exhaustion return distinct `XhciDriverError` values.

This diagnostic does not send `SET_CONFIGURATION`, issue xHCI Configure
Endpoint, create a non-control endpoint context, read a HID report descriptor,
poll an interrupt endpoint, parse HID reports, move a cursor, enable xHCI
interrupts, support the built-in I2C trackpad, or write storage.

## Verification

TDD began with failing tests for configuration Setup/Data TRBs, bounded header
and descriptor parsing, malformed inputs, and sequential control-ring
progression. The focused driver suite passed 28 tests. The framebuffer RED /
GREEN pass added success and typed-error panels. Both the previous descriptor
feature and the new configuration feature build for `x86_64-unknown-none`.

The QEMU harness is:

```text
py -3 scripts\test-usb-xhci-configuration-probe.py
```

It boots with disposable simulated USB storage, detaches that boot device,
hotplugs QEMU's USB mouse, requires all ADR 0083 markers, then requires the
two-stage configuration transfer and parsed result. The accepted run reported:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_HEADER_TRANSFER_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_HEADER_READY
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_TOTAL_LENGTH=0x0000000000000022
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_VALUE=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_COUNT=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_TRANSFER_CC=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_NUMBER=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_ALTERNATE_SETTING=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_ENDPOINT_COUNT=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_CLASS=0x0000000000000003
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_SUBCLASS=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_PROTOCOL=0x0000000000000002
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_ENDPOINT=0x0000000000000081
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_ATTRIBUTES=0x0000000000000003
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_MPS=0x0000000000000004
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_INTERVAL=0x0000000000000007
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_READY
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
QEMU_OUTCOME success
USB_XHCI_CONFIGURATION_PROBE_TEST_OK
```

The QEMU-accepted ESP artifacts produced by that final harness run have these
SHA-256 hashes:

```text
BOOTX64.EFI   085A02AA250050CB55B065B7842B09CDE5C087291ABD19D83FA05F6197918578
PYTHCORE.ELF  5A15A9CAD0D44D5C5CA3DD2495F5BD3C27331D2F182263AF877C8E4151162E47
```

QEMU's mouse reports interval `7`; the physical Dell/PixArt mouse mapped by
Linux reports interval `10`. These are device-specific descriptor values, not
a parser discrepancy.

Fresh regression gates also passed `cargo fmt --all -- --check`,
`git diff --check`, Python compilation of the new harness, the prior ADR 0083
descriptor harness, `scripts/test-boot.py` with `BOOT_TEST_OK`, and
`scripts/test-persistent-storage.py` with `PERSISTENT_STORAGE_TEST_OK`.

The repository-wide `python -m pytest tests` command is not runnable in this
Windows environment because `python` resolves to the Microsoft Store alias and
the real `py -3` interpreter does not have `pytest` installed. The available
fallback, `py -3 -m unittest discover tests`, ran 108 tests and retained the
same three Phase 13 production-wiring failures verified on the clean base:

- `test_install_paths_materialize_manifest_exports_without_seed_helper`
- `test_non_verify_package_context_provider_uses_retained_service`
- `test_package_runtime_bootstrap_uses_launch_granted_import_capabilities`

They are baseline failures, not configuration-probe regressions, and are not
silently represented as green.

No physical configuration-probe image has been deployed in this ADR. The USB
ESP must be re-identified before any later deployment, and physical PythOS
acceptance requires a fresh target boot result. QEMU evidence does not prove
the AMD controller path, Dell/PixArt configuration read, HID input, or cursor
support.

## Consequences

PythOS now has a bounded, opt-in configuration discovery layer in QEMU. It can
identify the standard HID boot-mouse interface and interrupt-IN endpoint
metadata without activating that configuration or endpoint.

The next phase boundary is physical validation of this exact slice. Endpoint
configuration, HID report discovery, polling, and cursor behavior remain
separate future slices requiring explicit owner invocation.
