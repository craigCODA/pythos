# ADR 0056: Probe Identity Framebuffer Screen

Status: Accepted

## Context

The first probe-only real-hardware boot can report detailed controller identity
over COM1, but the target laptop has no serial capture path. The existing green
framebuffer result proves that an SDHCI/eMMC candidate was found, but it does
not expose the bus/device/function, vendor ID, device ID, class/subclass,
programming interface, or BAR values needed to choose the next storage slice.

## Decision

The `hardware-probe` boot path renders a fixed framebuffer identity panel after
PCI storage discovery. The panel prefers an SDHCI/eMMC candidate when one is
present and displays the selected controller's identity using fixed boot glyphs
and fixed-size stack buffers.

The boot path remains probe-only. It does not read, write, reset, initialize,
mount, select, or persist against any internal storage controller.

## Consequences

The laptop can provide controller identity evidence through a phone photo of
the framebuffer when serial is unavailable. QEMU continues to use serial output
as the automated oracle and gains a marker proving the framebuffer identity
path completed.

This ADR does not authorize SDHCI register access, eMMC command submission,
DMA, block reads, block writes, object-store integration, or claims of physical
eMMC support.
