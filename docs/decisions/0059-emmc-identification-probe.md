# ADR 0059: eMMC Identification Probe

Date: 2026-07-30
Status: Accepted

## Context

ADR 0058 proved that the hardware-probe boot path can discover an SDHCI PCI
controller and perform a bounded controller reset, clock-stable poll, and bus
power enable without touching the block data path. The first real target
reported an O2 Micro SDHCI/eMMC controller at BDF `01:00.0` with controller
reset complete, internal clock stable, 3.3 V bus power enabled, and no interrupt
errors.

That proves the controller can be initialized. It does not prove that an eMMC
device is present, addressable, or able to answer card-identification commands.

## Decision

The hardware-probe boot may perform one additional bounded eMMC identification
sequence after SDHCI initialization succeeds:

1. Program a conservative identification clock from the controller capability
   base clock.
2. Issue `CMD0` to place the attached card into idle state.
3. Issue `CMD1` with the MMC OCR voltage window until the busy bit is set or a
   fixed attempt budget expires.
4. Issue `CMD2` and capture the raw 136-bit CID response registers.
5. Issue `CMD3` with RCA `1`.
6. Issue `CMD9` for RCA `1` and capture the raw 136-bit CSD response registers.

The probe may write only SDHCI command-path and status registers required for
identification: clock control, argument, command, normal interrupt status, error
interrupt status, normal interrupt status enable, and error interrupt status
enable. It must not write interrupt signal-enable registers, transfer mode,
block size, block count, DMA/ADMA addresses, or the buffer data port. It must
not issue any data-transfer command and must not read or write media sectors.

Every command poll has a fixed iteration limit. Command inhibit, command
complete, card-busy, and SDHCI error outcomes are represented as explicit typed
errors instead of infinite waits.

## Consequences

The probe can now distinguish "controller initialized" from "eMMC answered
identification commands" on real hardware while preserving the no-disk-write
safety boundary. A passing identification probe is still not a block I/O proof;
block reads and writes remain outside this slice.
