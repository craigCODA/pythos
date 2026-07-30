# SDHCI/eMMC Register Snapshot Design

Status: Approved for implementation.

## Context

The target laptop rendered the SDHCI/eMMC identity panel and exposed BAR0 at
`0x00000000E3B01000`. There is no serial capture path on that machine, so the
next diagnostic needs to put the controller's read-only host register evidence
on the framebuffer.

## Scope

This slice extends only the `hardware-probe` boot path. It remains a
no-storage-access diagnostic and still halts after rendering the probe result.

## Behavior

After PCI storage discovery selects an SDHCI/eMMC candidate, PythCore validates
the BAR0 register window and reads a fixed SDHCI snapshot:

- present state
- low capabilities
- high capabilities
- max current capabilities
- slot interrupt status
- host controller version

The framebuffer panel shows `sdhci regs` plus the snapshot values when the read
succeeds. If it fails validation, the existing identity panel remains the
fallback. Serial output gains:

```text
PYTHOS:CORE:HARDWARE_PROBE:SDHCI_REGISTERS_READY
```

## Constraints

- No PCI config writes.
- No SDHCI register writes.
- No reset, power programming, clock programming, interrupt enable, command
  issue, data path, DMA, ADMA, media read, or media write.
- Validate BAR0 before any MMIO read.
- Use volatile reads only for the fixed register offsets.
- Keep all framebuffer text inside fixed stack buffers and fixed glyphs.
- QEMU proves the emulated path only; physical eMMC support remains unclaimed.

## Test Strategy

Host tests cover BAR0 validation, overflow rejection, non-SDHCI rejection,
volatile snapshot extraction from a host-owned backing window, framebuffer line
formatting, and glyph coverage. QEMU acceptance attaches `sdhci-pci`, requires
the SDHCI candidate and register-ready markers, and keeps the existing
no-write forbidden-marker checks.
