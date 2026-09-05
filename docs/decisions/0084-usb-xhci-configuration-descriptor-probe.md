# ADR 0084: USB xHCI Configuration Descriptor Probe

Date: 2026-09-02

Status: Accepted in QEMU; first physical attempt timed out; staged retry pending

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

The first physical ADR 0084 deployment reached the intended Lenovo handoff but
ended at `xhci cfg err` / `0x0F`. Both command-completion polling and
transfer-completion polling returned the same generic `CommandTimeout`, so the
frame could not establish whether the wait was No-op, Enable Slot, Address
Device, Device Descriptor, configuration header, or full configuration.

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

The physical follow-up keeps the generic `CommandTimeout` identity for the
polling primitives but remaps it at the six existing call sites. This is an
additive diagnostic contract; no polling limit, command ordering, transfer
layout, or error behavior changes:

| Screen code | Framebuffer stage | Serial identity |
|---|---|---|
| `0x2C` | `stage noop command` | `NOOP_COMMAND_TIMEOUT` |
| `0x2D` | `stage enable slot` | `ENABLE_SLOT_TIMEOUT` |
| `0x2E` | `stage address device` | `ADDRESS_DEVICE_TIMEOUT` |
| `0x2F` | `stage device descriptor` | `DEVICE_DESCRIPTOR_TIMEOUT` |
| `0x30` | `stage config header` | `CONFIGURATION_HEADER_TIMEOUT` |
| `0x31` | `stage config full` | `CONFIGURATION_TRANSFER_TIMEOUT` |

## Verification

TDD began with failing tests for configuration Setup/Data TRBs, bounded header
and descriptor parsing, malformed inputs, and sequential control-ring
progression. The focused driver suite passed 29 tests. The framebuffer RED /
GREEN pass added success and typed-error panels. Both the previous descriptor
feature and the new configuration feature build for `x86_64-unknown-none`.

The staged physical follow-up began with a failing host contract for the six
missing error identities and a failing framebuffer contract for the missing
stage line. After the minimal mapping change, all 656 PythCore host tests pass,
including the exact codes, labels, serial identities, and preservation of
non-timeout errors.

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
PYTHCORE.ELF  325A4F142282BDA353178110A74D97FEF20ADF78A0117EA1D3BAA44366990A11
```

QEMU's mouse reports interval `7`; the physical Dell/PixArt mouse mapped by
Linux reports interval `10`. These are device-specific descriptor values, not
a parser discrepancy.

Fresh regression gates passed `cargo fmt --all -- --check`, `git diff --check`,
the full 656-test PythCore host suite, the prior ADR 0083 descriptor harness,
the refreshed ADR 0084 configuration harness, `scripts/test-boot.py` with
`BOOT_TEST_OK`, and `scripts/test-persistent-storage.py` with
`PERSISTENT_STORAGE_TEST_OK`. The refreshed ADR 0084 harness again reached
`XHCI_CONFIGURATION_READY`, `NO_DISK_WRITES`, `QEMU_OUTCOME success`, and
`USB_XHCI_CONFIGURATION_PROBE_TEST_OK` without emitting a driver error.

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

The QEMU-accepted configuration image was deployed to the re-identified Lexar
D70E USB ESP and booted on the Lenovo `81VS` / IdeaPad Slim 1 14AST-05. Two
chronological frames are preserved:

```text
docs/evidence/2026-09-02-physical-usb-xhci-configuration-swap-ready.jpg
SHA-256 60C9B9D5626CC30866D99A501C1221EAC75A741B1C8A409A8B57EF4A648E323F

docs/evidence/2026-09-02-physical-usb-xhci-configuration-timeout.jpg
SHA-256 D36E95696074815A034C219042D94D53BA9F083890B014A050D7E8E7AE60608D
```

The first shows `swap mouse now`, `no disk writes`, AMD `1022:7914`, and the
frozen eight-port baseline while the boot USB is still inserted. The second,
after boot-USB removal and mouse insertion, shows port `06` changing from
`000002A0` to `000202E1`, `xhci cfg err`, `no disk writes`, and error
`0000000F`. This proves the physical swap/connect path and a bounded timeout;
it does not prove any specific command or configuration transfer completed.

The staged diagnostic was then deployed to the re-identified Disk 2 Lexar D70E
USB ESP, serial `1026R51254700477`, active MBR/FAT32 `PYTHOS_ESP`, with
non-boot/non-system flags and a clean volume. The predeployment 119-file volume
was copied to
`D:\PythOS-Workspace\checkpoints\2026-09-02-adr0084-timeout-stage-usb-predeploy-backup`
and verified file by file. Deployment overwrote only the 8 accepted image files
(4,042,520 bytes), preserved all 111 unrelated files byte-identically, and
read back the deployed core as:

```text
325A4F142282BDA353178110A74D97FEF20ADF78A0117EA1D3BAA44366990A11
```

A visible boot-source identifier was added after the operator confirmed that a
later Lenovo screen had repeated the old panel character for character even
though the refreshed kernel was present on the Lexar. The new build renders
`diag cfg stage1` before the handoff. Its deployed core SHA-256 is:

```text
689276371BD5A69CB567D2E204022C8F86747F25FD276336BE0FABFD6907DDAD
```

The 2026-09-03 retry produced two new chronological frames:

```text
docs/evidence/2026-09-03-physical-usb-xhci-configuration-stage1-swap-ready.png
SHA-256 BC96F7B4609831D6FB16721A2B25525FFCC1C7274BD30C4FCDD8CDDEC6B11B3C

docs/evidence/2026-09-03-physical-usb-xhci-configuration-success.png
SHA-256 54E9FBFA04EBF0F225AD90707AFC55C72703615CA8BCF0B01F96D0B8EF419BEA
```

The first frame proves the intended staged image reached the Lenovo by showing
`diag cfg stage1` above the frozen AMD `1022:7914` port snapshot. After the
wide boot USB was removed and the external mouse inserted, the second frame
shows physical success on port `05`, slot `01`. Address Device, Device
Descriptor, the nine-byte configuration header, and the bounded full
configuration transfer all report completion code `01`. The parsed Dell/PixArt
mouse reports total length `34`, configuration value `1`, one configuration,
one interface, HID boot mouse class/subclass/protocol `03/01/02`, interrupt-IN
endpoint `0x81`, attributes `0x03`, maximum packet size `4`, and interval `10`.
Together with the exact QEMU-accepted image hash and prior file-by-file USB
readback, these frames establish physical configuration-descriptor acceptance
for the Lenovo `81VS` and this mouse. They do not establish HID input or cursor
support.

## Consequences

PythOS now has a bounded, opt-in configuration discovery layer accepted in
QEMU and on the Lenovo `81VS` with the Dell/PixArt mouse. It can identify the
standard HID boot-mouse interface and interrupt-IN endpoint metadata without
activating that configuration or endpoint.

The next phase boundary is `SET_CONFIGURATION` plus endpoint-context setup as a
separate bounded slice. HID report discovery, interrupt-IN polling, cursor
behavior, and shell input remain later slices requiring explicit owner
invocation.
