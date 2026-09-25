# Phase 15 PCI Memory-Space Enable Correction Record

**Date:** 2026-09-24  
**Status:** Corrected locally; physical retest pending  
**ADR:** [0108](../decisions/0108-phase-15-pci-memory-space-enable-experiment.md)

## Observed failure

The first ADR 0108 image was booted on the Lenovo `81VS` and remained on the
violet `network pci enable` entry color. No final screen or serial transcript
was captured, so this run is not acceptance evidence and does not prove that a
PCI write occurred.

The prior Lenovo configuration observation reported BDF `02:00:00`,
`10EC:C82F`, and command/status `0x00100000`; the PCI Memory Space Enable bit
was clear.

## Root cause

The enable boot path called ADR 0107's `discover_setup()`. That read-only policy
correctly returns `PciMemorySpaceDisabled` before preparing a register plan.
The enable path treated that result as a terminal skip and halted without
rendering a skip panel. Therefore the violet screen was the expected symptom
of the incorrect control-path reuse, before any command-register write, BAR
mapping, or MMIO read.

## Correction

The enable profile now uses `discover_enable_setup()`, which preserves the
existing controller identity and BAR/target validation while allowing MSE to be
clear for this profile's explicit, gated transition. ADR 0107 remains
read-only. Unsupported or malformed enable targets render a visible safe-skip
panel.

## Verification

- 907 Rust tests passed with the enable feature.
- Bare-metal build and `clippy -- -D warnings` passed.
- 19 focused Python tests passed with 168 subtests.
- ADR 0107 and BAR self-tests passed.
- QEMU `e1000` and `e1000e` both reached `NETWORK_HARDWARE_REGISTER_ENABLE_PROBE_TEST_OK`.
- Corrected local ISO: `target/pythos-phase15-pci-memory-space-enable-corrected-20260924.iso`
- Corrected ISO size: `20,772,864` bytes
- Corrected ISO SHA-256: `4DC6855E8EE14036EF40D1EC88442839EABA58D4F79C6F64A7A6DBC989A33332`

The current session cannot see an `F:` drive, so the corrected ISO has not been
copied to `F:\iso` by this session. Existing images were not touched.

## Next gate

Copy the corrected ISO under its unique name, boot the Lenovo once, and capture
the original command, post-enable command, fixed register value, restored
command, and final screen. Do not mark ADR 0108 accepted until restoration is
verified.
