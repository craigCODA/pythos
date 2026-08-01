# ADR 0062: Polling SDHCI/eMMC PIO Block Backend

Date: 2026-08-01
Status: Accepted

## Context

ADR 0060 proved one bounded read-only 512-byte PIO block transfer from the
O2 Micro `1217:8620` SDHCI/eMMC controller after programming the SDHCI data
timeout. ADR 0061 proved one explicitly gated single-sector PIO write/readback
on the confirmed disposable target and on a disposable QEMU eMMC image.

The next storage question is whether the proven single-block command path can
serve PythOS' existing storage proofs through the normal `BlockDeviceInfo`
surface without enabling DMA, interrupts, multi-block commands, partitions, or
filesystem work.

## Decision

Add an opt-in `sdhci-emmc-backend` feature that promotes the existing
single-block SDHCI/eMMC PIO path into `BlockDeviceInfo`. The backend performs
recursive PCI discovery, maps one SDHCI BAR0 page uncacheable, initializes and
selects one eMMC card per boot, derives 512-byte logical capacity from EXT_CSD,
and supports synchronous single-sector CMD17/CMD24 requests with finite polling
budgets.

The backend emits:

- `PYTHOS:CORE:BLOCK:SDHCI_EMMC_CONTROLLER_FOUND`
- `PYTHOS:CORE:BLOCK:SDHCI_EMMC_CARD_READY`
- `PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC`

QEMU acceptance boots from ISO with `--no-virtio-blk --sdhci --emmc`; it must
reject virtio and AHCI selection, run the existing storage proof sequence twice
against the same disposable eMMC image, and verify persisted bytes from the
host.

This ADR does not add DMA/ADMA, interrupts, multi-block commands, partitions,
filesystems, hotplug, generic SD-card support, or a universal SDHCI claim.

## Consequences

The default `hardware-probe` build remains read-only. The separate
`hardware-probe-emmc-write` gate remains the only probe path that may issue the
fixed ADR 0061 write/readback command sequence.

Physical validation of the `sdhci-emmc-backend` feature is forbidden until QEMU
backend acceptance and QEMU object-shell persistence acceptance pass repeatedly.
Physical validation, when authorized by those QEMU gates, applies only to the
confirmed disposable O2 Micro `1217:8620` SDHCI/eMMC laptop unless a later ADR
records a different target and safety boundary.

## Implementation Status

As of 2026-08-01, `feature/sdhci-emmc-backend` implements this ADR through the
ADR 0062 backend commits and the `cf68c3b` final-regression harness-timeout
follow-up. QEMU storage acceptance and QEMU object-shell persistence acceptance
pass repeatedly with `sdhci-emmc-backend` enabled, including fallback-marker
rejection and host-image signature checks.

The verify-only physical acceptance panel is implemented and renders only after
the Phase 7-10 storage path reaches `PYTHOS:CORE:PHASE_10_COMPLETE`. Physical
backend acceptance on the disposable O2 Micro `1217:8620` target remains
pending until two cold boots without reimaging show the final panel. One
2026-08-01 physical boot video reached that panel and is recorded in
`docs/milestones/2026-08-01-physical-emmc-phase10.md`; the second cold boot is
still pending. This ADR still does not claim generic SDHCI/eMMC support,
partition parsing, filesystem support, DMA/ADMA, interrupts, hotplug, or safe
writes on any other target.
