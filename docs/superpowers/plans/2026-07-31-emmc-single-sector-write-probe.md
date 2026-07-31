# eMMC Single-Sector Write Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an explicit, gated eMMC CMD24 single-sector write/readback probe for disposable Phase 11 hardware.

**Architecture:** Keep the existing `hardware-probe` path read-only. Add a separate Cargo feature that enables one PIO write to fixed command address `2048`, followed by CMD13 program-complete polling, CMD17 readback, and serial/framebuffer reporting. Verify the behavior with host fake-MMIO tests and a disposable QEMU eMMC image inspected at the OCR-derived host backing offset.

**Tech Stack:** Rust `no_std`, PythOS hardware-probe feature, SDHCI PIO commands, Python QEMU harness.

## Global Constraints

- Implement only the eMMC single-sector write-probe slice.
- Default `hardware-probe` must remain read-only and emit `NO_DISK_WRITES`.
- Physical write build must use fixed command address `2048`; on the target OCR `0xC0FF8080` this is LBA `2048`; never write LBA `0`.
- Do not add DMA, ADMA, interrupts, multi-block commands, filesystems, object-store integration, NVMe, AHCI, MSI, APIC, bridge-walking, or universal-device work.
- Every polling loop must have a finite timeout and typed error.
- Every unsafe block must keep a documented invariant.
- QEMU writes must use a disposable raw image and verify the image bytes after boot.

---

### Task 1: Write Contract Artifacts

**Files:**
- Create: `docs/decisions/0061-emmc-single-sector-write-probe.md`
- Create: `docs/superpowers/specs/2026-07-31-emmc-single-sector-write-probe-design.md`
- Create: `docs/superpowers/plans/2026-07-31-emmc-single-sector-write-probe.md`

**Interfaces:**
- Produces: fixed command address `2048`, `hardware-probe-emmc-write`, selected-card CMD24/CMD13/CMD17 command sequence, and marker names for implementation tasks.

- [x] **Step 1: Record ADR/spec/plan.**

- [x] **Step 2: Self-review for scope creep, placeholders, and ambiguous write permission.**

- [x] **Step 3: Commit documentation if implementation is split into separate commits.**

### Task 2: Add Failing SDHCI Host Tests

**Files:**
- Modify: `core/src/sdhci_probe.rs`

**Interfaces:**
- Consumes: existing `FakeSdhciIo`, `SdhciRegisterIo`, `EmmcReadBlockReport`.
- Produces: desired APIs `emmc_write_test_word`, `emmc_write_test_checksum`, `emmc_write_test_nonzero_byte_count`, `write_emmc_test_block_with_io`, and `write_emmc_test_block_controller`.

- [x] **Step 1: Add a test proving the deterministic write pattern first word is `0x48545950`, the checksum is `0x0000FBD8`, and the nonzero byte count is `0x000001FE`.**

- [x] **Step 2: Run `cargo test -p pythos-core emmc_write_test_pattern_is_fixed` and verify it fails because the pattern API is missing.**

- [x] **Step 3: Add a test proving the standalone write path issues `CMD7`, `CMD16`, `CMD24`, `CMD13`, then readback `CMD16`, `CMD17`, uses command address `2048`, writes 128 data-port words, and does not touch DMA/ADMA registers.**

- [x] **Step 4: Run the focused test and verify it fails because the write API is missing.**

- [x] **Step 5: Add tests for buffer-write-ready timeout, transfer-complete timeout, program-complete timeout, and the selected-card no-second-`CMD7` sequence after the LBA 0 read.**

### Task 3: Implement SDHCI CMD24 Write Path

**Files:**
- Modify: `core/src/sdhci_probe.rs`

**Interfaces:**
- Produces: `EmmcWriteBlockReport`, `EmmcWriteBlockError`, `write_emmc_test_block_controller`, `write_emmc_test_block_with_io`, and marker/screen helpers.
- Consumes: existing CMD17 read helper after it is generalized to accept a block address.

- [x] **Step 1: Add constants for buffer-write-ready bit, write-test LBA `2048`, and transfer-mode write value `0`.**

- [x] **Step 2: Refactor `read_emmc_lba0_with_io` into a shared `read_emmc_block_with_io(io, block_address)` helper while keeping the public LBA0 wrapper unchanged.**

- [x] **Step 3: Add deterministic test-pattern word/checksum helpers.**

- [x] **Step 4: Add `EmmcWriteBlockError` with marker and no-serial screen-code helpers.**

- [x] **Step 5: Implement CMD24 setup, buffer-write-ready polling, 128 data-port writes, transfer-complete polling, CMD13 ready-for-data polling, and readback comparison.**

- [x] **Step 6: Extend `FakeSdhciIo` to simulate CMD24 by accepting data-port writes into its backing block and then serving that block to CMD17.**

- [x] **Step 7: Run focused host tests until they pass.**

### Task 4: Gate Boot Integration and Screen Output

**Files:**
- Modify: `core/Cargo.toml`
- Modify: `core/src/hardware_probe_boot.rs`
- Modify: `core/src/hardware_probe_screen.rs`
- Modify if needed: `core/src/font.rs`

**Interfaces:**
- Consumes: `EmmcWriteBlockReport` and `EmmcWriteBlockError`.
- Produces: `hardware-probe-emmc-write` feature-gated boot path and `emmc write` panel.

- [x] **Step 1: Add `hardware-probe-emmc-write = ["hardware-probe"]`.**

- [x] **Step 2: In read-only builds, keep current read flow and final `NO_DISK_WRITES` marker.**

- [x] **Step 3: In write builds, emit `DISK_WRITE_TEST_ARMED`, run `write_emmc_test_block_controller`, emit write/readback markers, and omit `NO_DISK_WRITES`.**

- [x] **Step 4: Add screen state for `emmc write` success and `emmc write err` failure with LBA/checksum/match lines.**

- [x] **Step 5: Add any missing fixed boot glyphs required by the new screen text.**

### Task 5: Add QEMU Write Acceptance

**Files:**
- Create: `scripts/test-emmc-write-probe.py`

**Interfaces:**
- Consumes: `hardware-probe-emmc-write` feature and `scripts/run-qemu.py --sdhci --emmc`.
- Produces: `EMMC_WRITE_PROBE_TEST_OK`.

- [x] **Step 1: Build loader/core with `hardware-probe hardware-probe-emmc-write`.**

- [x] **Step 2: Create `target/hardware-probe-emmc-write.img` as a 32 MiB zeroed disposable image.**

- [x] **Step 3: Boot QEMU with `--sdhci --emmc --emmc-image target/hardware-probe-emmc-write.img`.**

- [x] **Step 4: Require the write/readback markers and forbid `NO_DISK_WRITES`, object-store, shell, AHCI, and write-error markers.**

- [x] **Step 5: Parse the QEMU OCR marker and verify raw image bytes at the OCR-derived command-address offset exactly match the deterministic pattern.**

### Task 6: Verify and Commit

**Files:**
- Modify only files touched by Tasks 1-5.

**Interfaces:**
- Produces: committed and pushed write-probe branch ready for a separate physical USB deployment approval.

- [x] **Step 1: Run focused Rust tests for `sdhci_probe` and `hardware_probe_screen`.**

- [x] **Step 2: Run `python scripts/test-hardware-probe.py` to prove the read-only probe still passes and still emits `NO_DISK_WRITES`.**

- [x] **Step 3: Run `python scripts/test-emmc-write-probe.py` to prove disposable QEMU write/readback and image verification.**

- [x] **Step 4: Run storage safety scripts: `check-storage-constants.py`, `verify-driver-timeouts.py`, and `scan-unsafe-rust.py`.**

- [x] **Step 5: Commit and push with a message that names the destructive feature gate.**

## Verification Notes

- `cargo test -p pythos-core`: 333 passed, 0 failed.
- `python -m py_compile scripts\test-emmc-write-probe.py`: passed.
- `python scripts\test-hardware-probe.py`: `HARDWARE_PROBE_TEST_OK`; the read-only build still emits `PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES`.
- `python scripts\test-emmc-write-probe.py`: `EMMC_WRITE_PROBE_TEST_OK`; QEMU write/readback markers passed and the disposable image bytes matched the deterministic pattern at the OCR-derived offset.
- Storage guard scripts were run. They still fail on pre-existing baseline findings outside this slice: 60 storage-constant findings and 2 timeout findings in `core/src/block_device.rs`, plus 126 unsafe-comment findings in older files. No new findings name `core/src/sdhci_probe.rs`, `core/src/hardware_probe_boot.rs`, `core/src/hardware_probe_screen.rs`, or `scripts/test-emmc-write-probe.py`.
