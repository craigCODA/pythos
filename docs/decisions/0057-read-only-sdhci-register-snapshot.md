# ADR 0057: Read-Only SDHCI Register Snapshot

Status: Accepted

## Context

The probe identity framebuffer screen confirmed a real SDHCI/eMMC candidate on
the target laptop without serial capture:

- BDF `01:00.0`
- vendor/device `1217:8620`
- class/subclass/programming-interface `08/05/01`
- BAR0 `0x00000000E3B01000`

That proves PCI configuration-space discovery reaches the controller, but it
does not say whether the BAR0 register window is readable or what SDHCI host
version/capability bits the machine exposes. The next useful physical evidence
is therefore a bounded register snapshot, still not a storage driver.

## Decision

The `hardware-probe` boot path may read only these SDHCI BAR0 registers after
PCI discovery identifies an `SDHCI_EMMC_CANDIDATE`:

- `0x24` `PRESENT_STATE`
- `0x40` `CAPABILITIES_LOW`
- `0x44` `CAPABILITIES_HIGH`
- `0x48` `MAX_CURRENT_CAPABILITIES`
- `0xFC` `SLOT_INTERRUPT_STATUS`
- `0xFE` `HOST_CONTROLLER_VERSION`, read as the high half of the aligned
  32-bit word at `0xFC`

Before MMIO access, PythCore validates that BAR0 exists and that the fixed
`0x100` byte register window lies inside the loader's temporary identity map.
This path runs before replacement kernel page tables, normal block-device
selection, object-store restore, or shell launch.

The snapshot is emitted over COM1 and rendered on the framebuffer when present.
If the snapshot cannot be taken, the probe falls back to the existing identity
screen and emits a typed failure marker.

## Consequences

The laptop can provide host-controller state and capability evidence through a
phone photo when serial is unavailable. The automated QEMU acceptance test adds
an emulated `sdhci-pci` controller and requires
`PYTHOS:CORE:HARDWARE_PROBE:SDHCI_REGISTERS_READY`.

This ADR does not authorize SDHCI initialization, power or clock programming,
software reset, interrupt enable, command submission, data transfer, PIO, DMA,
ADMA, block reads, block writes, partition parsing, or object-store integration.

## Alternatives Considered

- **Start eMMC command sequencing now.** Deferred. Even read-only media
  commands require controller initialization policy and timeout/error handling
  that are wider than the current evidence-gathering slice.
- **Read all SDHCI registers.** Rejected. The current slice needs only the
  stable identification/capability registers and should avoid accidental
  dependency on wider controller state.
- **Map a kernel MMIO window first.** Deferred for this probe-only path because
  it intentionally runs before VM replacement while the loader identity map is
  still active. A later SDHCI driver must use an explicit kernel MMIO mapping.
