# SDHCI Initialization Probe Design

Status: Approved for implementation.

## Context

The laptop framebuffer proved:

```text
sdhci regs
no disk writes
BAR0 00000000E3B01000
state 01FF00F0
cap0 25FCC8BF
cap1 00002077
maxcur 005800C8
slotver 06030000
```

That confirms physical SDHCI BAR0 register visibility, not eMMC media access.

## Scope

This slice extends only the `hardware-probe` boot path. It performs a bounded
controller initialization proof and then halts. It does not enter normal boot,
select a block backend, restore the object store, launch the shell, identify
the eMMC device, or issue media commands.

## Behavior

For a selected SDHCI/eMMC candidate with a valid BAR0 window:

1. Take the existing read-only snapshot.
2. Write `SOFTWARE_RESET_ALL` to the software reset register.
3. Poll the reset register with a fixed iteration budget until all reset bits
   clear.
4. Enable internal clock only and poll for the stable bit.
5. Select bus voltage using capability bits in this order: 3.3V, 3.0V, 1.8V.
6. Write the selected voltage plus bus-power bit to the power-control register.
7. Read back reset, clock, power, present-state, and interrupt-status values.
8. Emit `PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT_READY`.
9. Render an `sdhci init` framebuffer panel containing no-write status and the
   key post-init register values.

## Constraints

- No PCI config writes beyond the existing read-only probe behavior.
- No command register writes.
- No argument register writes.
- No transfer-mode, data, block-size/count, DMA, or ADMA register writes.
- No media reads or writes.
- Every polling loop has a finite timeout and typed failure.
- Every MMIO access uses volatile semantics and documented unsafe invariants.
- The framebuffer result must remain readable without serial capture.
- QEMU proves only the emulated SDHCI controller path.

## Test Strategy

Host tests cover voltage selection, timeout behavior, reset/clock/power register
ordering through a fake MMIO window, typed errors, and framebuffer formatting.
The hardware-probe QEMU acceptance test attaches `sdhci-pci`, requires
`PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT_READY`, and keeps the existing no-write
forbidden-marker checks.
