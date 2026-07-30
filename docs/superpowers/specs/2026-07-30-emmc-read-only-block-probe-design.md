# eMMC Read-Only Block Probe Design

Date: 2026-07-30

## Scope

This slice extends the Phase 11 hardware-probe path from eMMC identification to
one read-only data transfer. It proves only that the selected SDHCI controller
can read LBA 0 from the attached eMMC through PIO.

In scope:

- Preserve SDHCI register snapshot, initialization, and eMMC identification.
- Select the identified eMMC card at RCA `1`.
- Read exactly one 512-byte sector from LBA 0 with `CMD17`.
- Use PIO buffer reads only.
- Emit serial markers for first dword, checksum, nonzero byte count, and
  completion.
- Render the read summary on the hardware-probe framebuffer panel.
- Extend QEMU acceptance with a deterministic disposable eMMC image pattern.

Out of scope:

- Disk writes.
- DMA or ADMA.
- Multi-block commands.
- Filesystem, partition-table, boot-sector parsing, or object-store access.
- Generic block-device integration.
- Interrupt-driven I/O.
- NVMe, AHCI, MSI, APIC, bridge-walking, or universal-device work.

## Safety Boundary

This probe remains non-destructive. It writes SDHCI control registers needed to
select the card, set a read block length, request one PIO read, and clear
interrupt-status bits. It never writes sector data to the device and never
issues media write commands.

The screen and serial output continue to include:

```text
PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES
```

That marker means no eMMC media writes occurred. It does not mean the controller
registers were read-only, because SDHCI command and transfer setup registers
must be programmed to perform the read command.

## Command Sequence

The read sequence runs after ADR 0059 identification:

```text
CMD7  arg=00010000 R1b select RCA 1
CMD16 arg=00000200 R1  set block length to 512
CMD17 arg=00000000 R1  read single block LBA 0, data-present
```

The transfer setup is:

```text
BLOCK_SIZE=0200
BLOCK_COUNT=0001
TRANSFER_MODE=0010
```

`TRANSFER_MODE=0010` selects device-to-host data direction with DMA disabled,
ADMA disabled, auto-command disabled, and single-block transfer.

## Report

The read report contains:

- BAR0 base address.
- LBA read, fixed at `0`.
- Block length, fixed at `512`.
- First dword from the sector, little-endian as received from the data port.
- Byte-wise wrapping checksum across the 512-byte sector.
- Nonzero byte count.
- Final normal interrupt status.
- Final error interrupt status.

## Serial Markers

On success, the hardware-probe path emits:

```text
PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:LBA=0x0000000000000000
PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:BLOCK_LEN=0x0000000000000200
PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:FIRST_DWORD=0x<16 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:CHECKSUM=0x<16 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:NONZERO_BYTES=0x<16 hex digits>
PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ONLY_BLOCK_READY
```

On failure, the probe emits a typed
`PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:*` line and still emits
`PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES`.

For no-serial physical targets, the framebuffer panel title changes to
`emmc read err` and includes `err <code>` after the BAR0/OCR lines. The stable
screen codes are:

```text
1 SDHCI probe failure
2 register I/O failure
3 command-path failure
4 data-inhibit timeout
5 buffer-read-ready timeout
6 transfer-complete timeout
7 data-transfer error status
```

## QEMU Oracle

The hardware-probe acceptance test creates a disposable QEMU eMMC image whose
first sector is the byte pattern `00..FF 00..FF`. The expected read summary is:

```text
FIRST_DWORD=0x0000000003020100
CHECKSUM=0x000000000000FF00
NONZERO_BYTES=0x00000000000001FE
```

Real hardware is not expected to match those values; it only needs a successful
read marker and the reported values photographed or captured from the screen.

## Verification

Verification requires:

- Host tests for data-present command encoding and R1b select-card encoding.
- Host tests for the PIO read path, checksum accounting, and forbidden
  DMA/ADMA/buffer-write accesses.
- Host tests for buffer-ready or transfer-complete timeout errors.
- QEMU hardware-probe acceptance with the deterministic disposable eMMC image.
- Real hardware boot of the updated USB with visible `emmc read` output.
