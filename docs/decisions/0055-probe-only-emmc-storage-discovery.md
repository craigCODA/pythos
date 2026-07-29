# ADR 0055: Probe-Only eMMC Storage Discovery

Status: Accepted

## Context

The next physical boot target has an internal eMMC device, not a disposable
SATA disk. The current development PC has an AHCI-visible drive with live data,
so physical AHCI testing on that machine is explicitly off limits. ADR 0054's
polling AHCI backend remains QEMU-verified and useful, but it must not be
treated as permission to touch a real disk on this host.

The first useful real-hardware storage step is therefore inventory, not a block
driver: identify whether firmware exposes an SDHCI/eMMC-class PCI function, or
instead exposes NVMe, AHCI, Intel VMD, RAID, legacy IDE, virtio, or another
mass-storage controller. That inventory must be safe on the target disk.

## Decision

1. Add a `hardware-probe` PythCore feature that is mutually exclusive with
   `verify`. This boot path branches immediately after
   `PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY`, before VM replacement, normal
   boot, block backend selection, object-store restore, shell launch, or any
   storage service work.
2. Discover storage controllers only through x86 PCI configuration mechanism 1
   reads (`0xCF8`/`0xCFC`). The probe scans bus 0, follows PCI-to-PCI bridge
   secondary buses, classifies a bounded set of controller records, and decodes
   memory BAR base addresses as data. It performs no PCI config writes, no BAR
   probing writes, no MMIO access, no controller reset, no DMA setup, and no
   media reads or writes.
3. Classify SD/eMMC host controllers as an `SDHCI_EMMC_CANDIDATE` when PCI
   class/subclass are `0x08/0x05`. This is intentionally a candidate, not a
   claim that the controller is initialized or that the attached media is safe
   to access.
4. Emit the probe result through COM1 serial markers and a coarse framebuffer
   color:
   - violet: hardware-probe path entered
   - green: SDHCI/eMMC candidate found
   - blue: storage found, but not SDHCI/eMMC
   - red: no storage controller found
5. Add `scripts/test-hardware-probe.py` as the automated QEMU acceptance test.
   QEMU uses its normal disposable `target/` storage image for discoverability,
   but the guest probe path must stop at `PYTHOS:CORE:HARDWARE_PROBE_READY` and
   must not emit normal block-device, object-store, or shell-entry markers.

## Consequences

- The eMMC target can be booted with a probe-only image that does not touch its
  storage controller beyond PCI config reads.
- The result is a hardware inventory signal, not a supported storage backend.
  Actual SDHCI/eMMC initialization, command sequencing, DMA/PIO policy,
  partition policy, and persistence are later work requiring their own ADR and
  QEMU or hardware acceptance strategy.
- On machines without visible COM1, the screen color can distinguish
  "eMMC-class candidate" from "other storage" or "no storage," but detailed
  bus/vendor/device IDs still require serial capture or a later text diagnostic
  slice.
- Physical AHCI validation is deliberately not attempted on the development PC
  with a live data drive.

## Alternatives Considered

- **Attempt AHCI on the current PC.** Rejected because a live data drive is
  present and there is no sacrificial physical AHCI disk.
- **Implement SDHCI/eMMC commands immediately.** Deferred. Even read-only media
  commands require controller initialization, MMIO semantics, interrupt/polling
  choices, and careful timeout/error handling. That is too wide for the first
  safe hardware-identification slice.
- **Implement framebuffer text diagnostics now.** Deferred. It would make
  hardware IDs visible without serial, but it is a separate early-rendering
  surface. The current slice keeps display output to already established solid
  framebuffer colors.
