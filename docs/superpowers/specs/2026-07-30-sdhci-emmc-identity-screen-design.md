# SDHCI/eMMC Identity Screen Design

Status: Approved for implementation.

## Context

The probe-only hardware image reached the green framebuffer result on the target
laptop, which means PythOS detected an SDHCI/eMMC candidate through PCI
configuration-space discovery. The laptop has no serial capture, so the next
slice must make the same controller identity visible on the framebuffer.

## Scope

This slice extends the existing `hardware-probe` boot path only. It still stops
after PCI discovery. It must not read, write, reset, initialize, mount, select,
or persist against the internal storage device.

## Behavior

After PCI storage discovery, PythCore renders a fixed text panel to the GOP
framebuffer. If an SDHCI/eMMC candidate exists, it is preferred over other
storage controllers. The panel shows:

- `PythOS`
- `sdhci emmc`, `other storage`, or `no storage`
- `no disk writes`
- controller count
- bus/device/function
- vendor ID and device ID
- class/subclass/programming-interface
- BAR0 and BAR5 decoded base values

Serial reporting remains unchanged as the QEMU oracle and gains one additional
marker after the framebuffer identity panel is rendered:

```text
PYTHOS:CORE:HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY
```

## Constraints

- No disk reads, writes, resets, command submission, DMA, object-store
  selection, or storage-service integration.
- Preserve the existing final color contract: green for SDHCI/eMMC, blue for
  other storage, red for no storage, violet for probe entered but unfinished.
- Use fixed-size stack buffers only; no allocation.
- Use existing framebuffer metadata validation and volatile writes.
- Render only characters with fixed boot glyphs.
- QEMU evidence proves the emulated no-write probe path only; real-hardware
  support remains unclaimed until later slices.

## Test Strategy

Host tests cover identity-line formatting, SDHCI preference over other storage
controllers, glyph coverage for every rendered character, and framebuffer text
panel rendering against a host-backed pixel buffer. The hardware-probe QEMU
acceptance test verifies the new serial marker and the existing no-write
markers remain ordered.
