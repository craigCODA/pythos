# ADR 0058: SDHCI Initialization Probe

Status: Accepted

## Context

ADR 0057 proved that the target laptop's SDHCI/eMMC BAR0 is readable from the
probe-only hardware image. The next storage-driver workflow layer is controller
reset and initialization, but not device identification or block I/O.

The laptop still has no serial capture. The initialization result must therefore
remain visible on the framebuffer, and the QEMU harness must remain the
automated serial oracle.

## Decision

The `hardware-probe` boot path may perform a bounded SDHCI controller
initialization proof after the read-only register snapshot succeeds:

1. Write `SOFTWARE_RESET_ALL` to the SDHCI software reset register.
2. Poll the reset register until the reset bit clears or a fixed iteration
   timeout expires.
3. Enable the SDHCI internal clock.
4. Poll until the internal-clock-stable bit is set or a fixed iteration timeout
   expires.
5. Select a supported bus voltage from the capability register and set the
   SD bus power bit.
6. Read back reset, clock, power, interrupt-status, and present-state registers.

The proof emits typed failure markers for timeout and unsupported-voltage
conditions, and emits `PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT_READY` only after
reset, clock, and power all complete.

This path still emits `PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES` and halts
before normal boot, block-device selection, object-store restore, shell launch,
or any storage-service path.

## Consequences

The physical laptop can prove whether the SDHCI host controller accepts the
first reset/clock/power sequence. Passing this ADR is still not eMMC media
support. It does not identify the card, read card registers, read blocks, write
blocks, parse partitions, integrate object storage, enable interrupts, or use
DMA.

The initialization sequence writes SDHCI controller registers. It deliberately
does not write command, argument, transfer-mode, data, block-size/count, DMA, or
ADMA registers.

## Alternatives Considered

- **Skip to card identification.** Rejected. CMD0/CMD1/CMD2/CMD3/CMD9/CMD13
  introduce command sequencing and response parsing, which require a separate
  proof layer.
- **Enable full SD clock immediately.** Deferred. This slice proves internal
  controller clock stability and bus power first; card clock policy belongs
  with command sequencing.
- **Use interrupts for completion.** Rejected for this slice. Polling with fixed
  timeouts is simpler and aligns with the current probe-only boot path.
