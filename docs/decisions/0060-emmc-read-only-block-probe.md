# ADR 0060: eMMC Read-Only Block Probe

Date: 2026-07-30
Status: Accepted

## Context

ADR 0059 proved that the Phase 11 hardware-probe boot can initialize the SDHCI
controller and receive eMMC identification responses. The first real target
reported OCR `C0FF8080`, CID words beginning `00D35C77 34471800`, and CSD word
`EF8A4040`.

That proves the command path reaches an attached eMMC device. It does not prove
that the data path can transfer a sector.

## Decision

The hardware-probe boot may perform one bounded, read-only PIO data command
after SDHCI initialization and eMMC identification succeed:

1. Select RCA `1` with `CMD7`.
2. Set the block length to 512 bytes with `CMD16`.
3. Program the SDHCI block size and block count for exactly one 512-byte block.
4. Program transfer mode for a single-block read with DMA and ADMA disabled.
5. Issue `CMD17` for logical block address `0`.
6. Poll for buffer-read-ready and transfer-complete status with fixed budgets.
7. Read exactly 512 bytes from the SDHCI buffer data port and report only a
   compact checksum, first dword, and nonzero byte count.

The probe must not write to the buffer data port, DMA address, ADMA address, or
any storage-service/object-store path. It must not issue write commands, erase
commands, multi-block commands, or filesystem/partition reads beyond LBA 0.

All hardware waits remain finite and all failure paths return typed errors.

## Consequences

The probe can now distinguish "eMMC answered identification" from "the SDHCI
PIO read data path can transfer one sector" on QEMU and real hardware. A
passing read-only block probe is still not generic block-device integration,
interrupt support, DMA support, write support, partition discovery, or object
store persistence on eMMC.
