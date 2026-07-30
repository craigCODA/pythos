# eMMC Read-Only Block Probe Implementation Plan

Date: 2026-07-30

## Goal

Extend the hardware probe so the real eMMC laptop can prove one non-destructive
PIO sector read after SDHCI initialization and eMMC identification.

## Scope Guardrails

- Read only LBA 0.
- Read exactly 512 bytes.
- Use PIO, not DMA or ADMA.
- Do not write to the SDHCI buffer data port.
- Do not issue media write, erase, trim, multi-block, filesystem, partition, or
  object-store commands.
- Do not change the normal boot storage backend.

## Steps

1. Add read-path tests.
   - Extend command encoding tests for R1b and data-present commands.
   - Add a fake SDHCI register model that returns a deterministic 512-byte
     sector from the buffer data port.
   - Assert the read path computes first dword, checksum, and nonzero count.
   - Assert it never writes DMA/ADMA registers or the buffer data port.
   - Assert timeout paths return typed errors.

2. Implement bounded PIO read helpers.
   - Add `CMD7`, `CMD16`, and `CMD17` helpers after identification.
   - Program block size/count and single-block read transfer mode.
   - Poll command-inhibit, command-complete, buffer-read-ready, and
     transfer-complete with fixed limits.
   - Return a compact read report.

3. Wire probe reporting.
   - Call the read probe only after eMMC identification succeeds.
   - Emit `EMMC_READ:*` serial markers and a completion marker.
   - Emit typed `EMMC_READ_ERROR:*` markers on failure.
   - Render `emmc read` plus LBA/checksum summary on the framebuffer panel.

4. Extend QEMU acceptance.
   - Create a deterministic disposable eMMC image for the hardware-probe test.
   - Require the read markers and expected QEMU pattern values.

5. Verify and deploy.
   - Run focused Rust unit tests.
   - Run the full hardware-probe QEMU acceptance test.
   - Run storage safety scripts.
   - Commit, push, and build the USB ESP for the laptop test.
