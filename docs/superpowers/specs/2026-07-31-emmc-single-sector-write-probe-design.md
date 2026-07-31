# eMMC Single-Sector Write Probe Design

Date: 2026-07-31

## Scope

This slice extends the Phase 11 hardware-probe path from proven one-sector
eMMC read to one gated write/readback test on a disposable target. It proves
only that the selected SDHCI controller accepts a single PIO `CMD24` write to a
fixed sacrificial sector and can read the same bytes back with `CMD17`.

In scope:

- Preserve SDHCI register snapshot, initialization, identification, and the
  existing read-only LBA 0 probe.
- Add `hardware-probe-emmc-write` as a separate build feature.
- Write exactly one 512-byte deterministic test pattern to command address
  `2048`. On the physical OCR `0xC0FF8080` target this is high-capacity LBA
  `2048`; on QEMU OCR `0x80FFFF00` this is byte offset `2048`.
- Read command address `2048` back and compare exact pattern identity.
- Emit explicit serial markers showing that a disk write test was armed.
- Render a write/readback summary on no-serial hardware.
- Add QEMU acceptance against a disposable eMMC image and verify the image
  contents at LBA `2048` after boot.

Out of scope:

- Any disk write in the default `hardware-probe` build.
- LBA 0 writes.
- DMA or ADMA.
- Multi-block commands.
- Filesystem, partition-table, boot-sector, or object-store integration.
- Interrupt-driven I/O.
- NVMe, AHCI, MSI, APIC, bridge-walking, or universal-device work.

## Safety Boundary

The write path is compiled only when `hardware-probe-emmc-write` is enabled.
The normal hardware-probe image stays read-only and keeps the
`PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES` marker.

The write image must instead emit:

```text
PYTHOS:CORE:HARDWARE_PROBE:DISK_WRITE_TEST_ARMED
```

That marker means the boot image is intentionally destructive to fixed LBA
`2048`. It still does not authorize writes to any other sector or to the PythOS
object store.

## Command Sequence

The write sequence runs after ADR 0059 identification:

```text
CMD7  arg=00010000 R1b select RCA 1 for the existing LBA 0 read
CMD16 arg=00000200 R1  set block length to 512 for the write
CMD24 arg=00000800 R1  write single block command address 2048, data-present
CMD13 arg=00010000 R1  poll card ready-for-data after programming
CMD16 arg=00000200 R1  set block length to 512 for readback
CMD17 arg=00000800 R1  read single block command address 2048, data-present
```

After the LBA 0 read has selected the card, the write/readback sub-sequence
does not issue another `CMD7`. QEMU returns a command error for that redundant
select, so the selected-card path is the verified command sequence.

The write transfer setup is:

```text
TIMEOUT_CONTROL=0E
BLOCK_SIZE=0200
BLOCK_COUNT=0001
TRANSFER_MODE=0000
```

`TRANSFER_MODE=0000` selects host-to-device direction with DMA disabled, ADMA
disabled, auto-command disabled, and single-block transfer.

## Report

The write report contains:

- BAR0 base address.
- Fixed LBA, `2048`.
- Block length, fixed at `512`.
- First dword of the written pattern.
- Byte-wise wrapping checksum of the written pattern.
- Readback first dword.
- Readback checksum.
- Readback nonzero byte count.
- Exact readback match boolean.
- Final normal interrupt status.
- Final error interrupt status.

## Serial Markers

On success, the hardware-probe path emits:

```text
PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE:LBA=0x0000000000000800
PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE:BLOCK_LEN=0x0000000000000200
PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE:FIRST_DWORD=0x0000000048545950
PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE:CHECKSUM=0x<16 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_READBACK:FIRST_DWORD=0x0000000048545950
PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_READBACK:CHECKSUM=0x<16 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_READBACK:NONZERO_BYTES=0x<16 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_READBACK_MATCH_READY
```

On failure, the probe emits a typed
`PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:*` marker and renders
`emmc write err` on the framebuffer panel.

## QEMU Oracle

The write acceptance test creates a disposable QEMU eMMC image initialized with
zeros, boots the explicit write image, requires the write/readback success
markers, parses the reported OCR, and verifies the raw-image bytes at the host
offset implied by that OCR. QEMU currently reports OCR `0x80FFFF00`, so command
address `2048` persists at byte offset `2048`. The physical target reported
OCR `0xC0FF8080`, so the same command address targets LBA `2048`.

Real hardware is not expected to have serial. The no-serial oracle is the
framebuffer panel showing `emmc write`, `lba 00000800`, matching write/read
checksums, and `match 01`.

## Verification

Verification requires:

- Host tests for `CMD24` data-present command encoding.
- Host tests for the deterministic 512-byte test pattern.
- Host tests for the PIO write path, fixed LBA `2048`, and no DMA/ADMA writes.
- Host tests for buffer-write-ready and transfer-complete typed timeouts.
- Host tests that the selected-card write path does not issue a second `CMD7`
  after the LBA 0 read.
- Host tests for `CMD13` program-complete timeout.
- Host tests that the read-only probe still does not write to the data port.
- QEMU write acceptance with disposable eMMC image content verification at the
  OCR-derived backing offset.
- Real hardware boot only after explicit disposable-drive confirmation.
