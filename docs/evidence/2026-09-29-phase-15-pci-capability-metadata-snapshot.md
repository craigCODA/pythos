# Phase 15 PCI Capability Metadata Snapshot — QEMU Evidence

**ADR:** [0109 — Bounded PCI Capability Metadata Snapshot](../decisions/0109-phase-15-pci-capability-metadata-snapshot.md)
**ADR status:** Proposed for owner review
**Evidence scope:** QEMU only

## Bounded implemented scope

The opt-in `network-hardware-capability-probe` records a bounded, read-only
conventional PCI capability metadata snapshot from the selected QEMU network
controller. It reports list presence, bounded entry metadata, recognized
PM/MSI/PCIe/MSI-X capability IDs and offsets, and PCI interrupt line/pin
metadata. It does not interpret capability control fields or claim controller
or device readiness.

The probe preserves the existing `VirtioTransport`, transport adapter, and
`NetworkPort` boundaries. It includes no PCI writes, BAR mapping, MMIO or
device-register access, MSI/MSI-X enablement, power control, bus mastering,
DMA, interrupts, reset, queues, packets, sockets, Wi-Fi, or physical
networking.

## Recorded QEMU acceptance

Task 4 recorded the following isolated QEMU oracle outcomes using one explicit
network model, `-nic none`, no backend peer, no non-boot block device, and one
`QEMU_OUTCOME success` per model:

- `e1000` (`8086:100E`) produced a successful absent-list result:
  `PCI_CAPABILITY_LIST_ABSENT` and
  `PCI_CAPABILITY_SUMMARY=PM=NONE;PCIE=NONE;MSI=NONE;MSIX=NONE`.
- `e1000e` (`8086:10D3`) produced a successful four-entry traversal in pointer
  order: PM at `0xC8`, MSI at `0xD0`, PCIe at `0xE0`, and MSI-X at `0xA0`.

Both runs emitted `PCI_CONFIG_READ_ONLY`, rendered the framebuffer capability
result, reached the terminal ready marker, and ended with `QEMU_OUTCOME success`.
The oracle terminal result was `NETWORK_HARDWARE_CAPABILITY_PROBE_TEST_OK`.

## Lenovo 81VS remains pending

No Lenovo `81VS` evidence is recorded for ADR 0109. The QEMU Intel-model
observations do not establish the meaning of the Lenovo Realtek `10EC:C82F`
capabilities. Any Lenovo observation is a separately gated physical action
under `F:\iso`; existing Ventoy and ISO contents must be preserved. This record
does not claim that a physical ISO was built, copied, booted, or observed.
