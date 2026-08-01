# ADR 0061: eMMC Single-Sector Write Probe

Date: 2026-07-31
Status: Accepted

## Context

ADR 0060 proved that the Phase 11 hardware-probe boot can read one 512-byte
sector from the O2 Micro `1217:8620` SDHCI/eMMC controller after programming
SDHCI `TIMEOUT_CONTROL` to `0x0E`.

The next hardware question is narrower than generic storage support: can the
same controller accept one bounded PIO media write and return the same bytes
through a readback?

Operator confirmation on 2026-08-01: the physical O2 Micro `1217:8620`
SDHCI/eMMC laptop used for this Phase 11 probe is a disposable storage target
and has been treated as disposable throughout bring-up. Future agents do not
need to re-ask whether that exact target is disposable before running this
ADR's already-authorized fixed single-sector write probe on it.

## Decision

Add a separate `hardware-probe-emmc-write` build feature. The existing
`hardware-probe` feature remains read-only and continues to emit
`PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES`.

When `hardware-probe-emmc-write` is enabled, the hardware-probe boot may issue
one PIO write to fixed sacrificial command address `2048` after SDHCI
initialization, eMMC identification, and the existing LBA 0 read succeed. On
the physical target OCR `0xC0FF8080` indicates high-capacity/block addressing,
so command address `2048` is LBA `2048`. QEMU's `emmc` model reports
OCR `0x80FFFF00`, so the same command address is byte-addressed in the
disposable QEMU image.

The selected-card boot sequence is:

1. Reuse the card already selected by the LBA 0 read; do not issue another
   `CMD7`, because QEMU rejects a redundant select after the card is already
   selected.
2. Set the block length to 512 bytes with `CMD16`.
3. Program the SDHCI block size and block count for exactly one 512-byte block.
4. Program transfer mode for a single-block write with DMA and ADMA disabled.
5. Program SDHCI `TIMEOUT_CONTROL` to `0x0E`.
6. Issue `CMD24` for command address `2048`.
7. Poll buffer-write-ready and transfer-complete status with fixed budgets.
8. Write exactly 512 deterministic test-pattern bytes through the SDHCI buffer
   data port.
9. Poll `CMD13` status until the card reports ready-for-data.
10. Read back command address `2048` with `CMD17` without reselecting the card
    and compare checksum, first dword, byte count, and exact pattern identity.

The test pattern starts with ASCII `PYTHOS_EMMC_WR00` and fills the remaining
bytes deterministically from the byte offset. The probe must not write LBA 0,
must not issue multi-block, erase, DMA, or ADMA operations, and must not attach
the result to the normal object store.

## Consequences

QEMU acceptance must use a disposable eMMC image and verify that the host
backing offset implied by the reported OCR contains the expected test pattern
after boot. A passing QEMU write test proves only the emulated CMD24 path. A
passing physical boot proves only that this disposable eMMC target accepted the
same one-sector write/readback sequence.

This ADR does not authorize generic eMMC block-device integration,
filesystem/partition parsing, object-store persistence on eMMC, interrupt
support, DMA/ADMA support, universal SDHCI support, or applying this disposable
target confirmation to any other machine or storage controller.
