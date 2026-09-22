# ADR 0105: Phase 15 Network-Hardware Identity Probe

Status: Accepted
Date: 2026-09-22

## Context

Phase 11 records that PythOS boots on real UEFI hardware, but that the
development laptop's Wi-Fi target is difficult to bring up. QEMU-emulated or
wired Ethernet is the tractable hardware path. Phase 14 completed its bounded
networking proof on the existing legacy QEMU `virtio-net-pci` transport
adapter. That proof remains unchanged while Phase 15 begins hardware
investigation.

The next useful question is narrower than implementing another network path:
can PythOS read and report a PCI network-controller identity? A deterministic
QEMU oracle is required before any physical-target claim. QEMU's Intel
`e1000` and `e1000e` devices provide that oracle without a PythOS network data
path. This slice does not add a network datapath or a generalized hardware abstraction.

## Decision

Add a dedicated opt-in `network-hardware-probe` boot profile. It is separate
from the existing storage `hardware-probe`, because the storage probe may
continue into SDHCI/eMMC register and media operations. The dedicated profile
does only a bounded read-only PCI configuration-space scan, emits a fixed
identity report, renders the same evidence to the framebuffer, and halts.

The report contains only:

- BDF;
- vendor and device identifiers;
- subsystem vendor and device identifiers;
- class, subclass, and programming-interface bytes; and
- a bounded result count and overflow state.

Class `0x02`, subclass `0x00` is classified as Ethernet. Other class-`0x02`
controllers are classified as `OtherNetwork`; that classification does not
infer Wi-Fi. Raw class/subclass and subsystem identity are the evidence for a
later target-specific investigation. BARs are deliberately not read in this
slice; BAR reachability belongs to a later register-reachability slice.

The scan reads PCI configuration fields through the existing x86 CF8/CFC
mechanism, but it does not write PCI configuration, enable memory space or bus
mastering, map or access BARs, reset a controller, allocate DMA buffers,
configure queues, send or receive frames, load firmware, enable interrupts,
or touch a physical network link.

`VirtioTransport` and the phrase "transport adapter" remain the PythOS names
for the privileged Phase 14 Virtio component. This ADR does not add a PythOS
hardware abstraction, change `NetworkPort`, change the PythTIG v1 ABI, or
alter the Phase 14 legacy Virtio lifecycle.

## QEMU oracle

The QEMU runner gains `--network-device {e1000,e1000e}`. Selecting it first
uses `-nic none` to suppress QEMU's implicit default NIC, then attaches exactly
one explicit PCI identity device. No netdev peer is configured and no packet
behavior is asserted.

The model identity oracle is:

| QEMU device | vendor | device | class/subclass/prog-if |
| --- | ---: | ---: | --- |
| `e1000` | `0x8086` | `0x100E` | `0x02/0x00/0x00` |
| `e1000e` | `0x8086` | `0x10D3` | `0x02/0x00/0x00` |

These values identify the QEMU models only. They are not a claim about the
Lenovo hardware.

## Evidence contract

Each run emits the following ordered terminal marker sequence, with the
identity fields between the controller marker and readiness:

```text
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:ENTER
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PCI_SCAN_READY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_COUNT=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_CONTROLLER_FOUND
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_KIND:ETHERNET
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:BUS=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:DEVICE=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:FUNCTION=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:VENDOR=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:DEVICE_ID=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBSYSTEM_VENDOR=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBSYSTEM_DEVICE=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:CLASS=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBCLASS=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PROG_IF=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_IDENTITY_READY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PCI_CONFIG_READ_ONLY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE_READY
```

The framebuffer panel shows the same bounded identity fields for a
serial-less physical boot. A physical observation remains target-specific
evidence; it is not physical hardware support and does not establish general
hardware support.
No physical claims are made by the QEMU oracle.

## Scope exclusions

This ADR does not authorize Lenovo Wi-Fi frames, association, firmware,
regulatory behavior, or a Wi-Fi implementation; Ethernet packet transmission
or reception; a second network transport; a new `NetworkPort` consumer; socket
or protocol work; modern Virtio; multiqueue; offloads; MSI/MSI-X; interrupts;
DMA; bus mastering; MMIO control; reset; power management; a generalized PCI,
physical-device, or hardware-memory abstraction; persistent network state;
routing; or capability-policy changes.

The existing storage `hardware-probe`, Phase 14 `VirtioTransport`, transport
adapter, `NetworkPort`, ABI, and acceptance claims remain unchanged.
Wi-Fi behavior remains deferred.

## Acceptance

`scripts/test-network-hardware-probe.py` builds the core with
`--no-default-features --features network-hardware-probe` and boots once with
each QEMU model. Both runs must report exactly one controller, match the
model's vendor/device/class identity, show subsystem fields, reach the
framebuffer and read-only markers, and finish with one successful QEMU outcome.
The test rejects storage, runtime-network, packet, MMIO, DMA, interrupt,
reset, bus-master, BAR-access, and PCI-configuration-write evidence.

Focused Rust tests cover Ethernet and `OtherNetwork` classification, the
recorded Lenovo RTL8822CE identity fixture without calling it Wi-Fi, invalid
functions, and bounded overflow. Full workspace and Python verification
remain required before changing this ADR to Accepted.
