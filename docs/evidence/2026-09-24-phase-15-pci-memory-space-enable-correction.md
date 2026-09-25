# Phase 15 PCI Memory-Space Enable Correction Record

**Date:** 2026-09-24  
**Status:** Corrected and physically accepted
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

The corrected ISO was copied to
`F:\iso\pythos-phase15-pci-memory-space-enable-corrected-20260924.iso` without
touching existing images. The destination hash is the same
`4DC6855E8EE14036EF40D1EC88442839EABA58D4F79C6F64A7A6DBC989A33332` and the
size is `20,772,864` bytes.

## Physical acceptance

The corrected ISO completed on Lenovo. The framebuffer recorded original
command `0x00100000`, post-enable command `0x00100002`, fixed register value
`0x300034DB`, restored command `0x00100000`, `mse restored`, and `no bus
master`. The photo is recorded at
[`2026-09-25-phase-15-pci-memory-space-enable-lenovo.md`](2026-09-25-phase-15-pci-memory-space-enable-lenovo.md).

ADR 0108 is accepted. No later networking scope is implied.
