# SDHCI/eMMC Register Snapshot Implementation Plan

**Goal:** Render a no-write SDHCI/eMMC BAR0 register snapshot on the laptop
framebuffer.

**Architecture:** Add a small `sdhci_probe.rs` module for fixed register-window
validation and volatile read-only snapshot collection. Keep PCI discovery in
`storage_probe.rs`, screen formatting in `hardware_probe_screen.rs`, and QEMU
device wiring in `scripts/run-qemu.py`.

## Tasks

- [x] Add focused tests for SDHCI BAR0 validation, overflow rejection, non-SDHCI
  rejection, and fixed snapshot extraction.
- [x] Add a framebuffer formatter test for the SDHCI register panel.
- [x] Add `sdhci_probe.rs` with a fixed `0x100` byte register window and
  volatile reads only at `0x24`, `0x40`, `0x44`, `0x48`, and `0xFC`.
- [x] Extend hardware-probe boot integration to emit register values and
  `PYTHOS:CORE:HARDWARE_PROBE:SDHCI_REGISTERS_READY` after a successful
  snapshot.
- [x] Add a `--sdhci` QEMU option and require the SDHCI marker in
  `scripts/test-hardware-probe.py`.
- [x] Run focused Rust tests, QEMU acceptance, and verify-feature compile.

## Verification Commands

```powershell
cargo test -p pythos-core sdhci_probe
cargo test -p pythos-core hardware_probe_screen
cargo test -p pythos-core font
python -m unittest tests.test_qemu_exit
python scripts/test-hardware-probe.py
cargo build -p pythos-core --target x86_64-unknown-none --features verify
```
