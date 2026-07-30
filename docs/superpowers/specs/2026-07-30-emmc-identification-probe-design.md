# eMMC Identification Probe Design

Date: 2026-07-30

## Scope

This slice extends the Phase 11 hardware-probe path from SDHCI controller
initialization to eMMC identification. It is limited to command-path discovery
and raw card identity capture.

In scope:

- Keep the SDHCI register snapshot and initialization reports.
- Add a bounded eMMC identification command sequence after initialization.
- Capture OCR, RCA, CID, and CSD values.
- Display the concise identification result on the GOP screen.
- Emit serial markers for automated QEMU acceptance.
- Add a QEMU `sdhci-pci` plus `emmc` acceptance mode.

Out of scope:

- Data commands.
- DMA or ADMA.
- Buffer data port access.
- Block reads or writes.
- Filesystem, partition, or object-store access on the real device.
- NVMe, AHCI, MSI, APIC, bridge-walking, or universal-device work.

## Safety Boundary

The probe remains non-destructive. It may touch only controller registers needed
to issue identification commands, arm command/error status bits for polling, and
clear command interrupt status. The probe must not program interrupt
signal-enable registers, transfer mode, block size, block count, DMA address,
ADMA address, or the data buffer register.

The screen continues to include `no disk writes`. That text means no media data
path was used; command-path register writes are now expected in this slice.

## Command Sequence

The command sequence is:

```text
CMD0 arg=00000000 no-response
CMD1 arg=40FF8000 R3 OCR, repeated until OCR bit31 is set
CMD2 arg=00000000 R2 CID
CMD3 arg=00010000 R1 assign RCA 1
CMD9 arg=00010000 R2 CSD
```

Each command waits for command inhibit to clear before issuing. Each command
clears stale normal and error interrupt status before writing the argument and
command registers. Each command waits only for command complete or SDHCI error
interrupt, using fixed poll budgets.

## Report

The identification report contains:

- BAR0 base address.
- OCR.
- Assigned RCA.
- Raw CID response registers.
- Raw CSD response registers.
- Final normal interrupt status.
- Final error interrupt status.

## Serial Markers

On success, the hardware-probe path emits:

```text
PYTHOS:CORE:HARDWARE_PROBE:EMMC:OCR=<8 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC:RCA=<4 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC:CID0=<8 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC:CID1=<8 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC:CID2=<8 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC:CID3=<8 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC:CSD0=<8 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC:CSD1=<8 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC:CSD2=<8 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC:CSD3=<8 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC_IDENTIFICATION_READY
```

On failure, the probe emits a typed `PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:*`
line and still emits `PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES`.

## Screen Output

The screen title changes to `emmc id` when identification succeeds. The visible
summary includes count, BDF, vendor/device ID, class/subclass/interface, BAR0,
OCR, RCA, and a compact subset of the raw CID/CSD words.

## Verification

Verification requires:

- Host tests for command-word encoding, identification-clock divisor selection,
  command polling, OCR busy timeout, and forbidden data-path writes.
- QEMU acceptance with `sdhci-pci` plus `emmc`.
- A bootable USB ESP deployment for the real eMMC laptop.
