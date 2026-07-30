# SDHCI Initialization Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove bounded SDHCI reset, internal-clock, and bus-power initialization in the probe-only boot image without media commands.

**Architecture:** Extend `sdhci_probe.rs` with a fake-MMIO-testable initialization state machine that uses volatile MMIO accessors in hardware builds. Feed the resulting `SdhciInitializationReport` into the existing hardware-probe boot and framebuffer panel. Keep QEMU acceptance on `sdhci-pci` and preserve the existing no-write oracle.

**Tech Stack:** Rust `no_std`, PythOS hardware-probe feature, QEMU `sdhci-pci`, Python QEMU harness.

## Global Constraints

- Implement only the SDHCI initialization probe slice.
- Do not issue SDHCI commands or media I/O.
- Do not write argument, command, transfer-mode, data, block-size/count, DMA, or ADMA registers.
- Every polling loop must have a finite timeout and typed error.
- Every MMIO access must be volatile and have a documented unsafe invariant.
- Preserve `PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES`.
- Do not claim physical eMMC support from QEMU.

---

### Task 1: Initialization State Machine

**Files:**
- Modify: `core/src/sdhci_probe.rs`

**Interfaces:**
- Consumes: `SdhciRegisterSnapshot`, `SdhciRegisterWindow`
- Produces: `SdhciInitializationReport`, `SdhciInitializationError`, `initialize_controller`

- [x] **Step 1: Write failing tests for voltage selection, timeout, and write order.**
- [x] **Step 2: Run `cargo test -p pythos-core sdhci_probe` and verify the tests fail for missing init APIs.**
- [x] **Step 3: Implement the smallest fake-MMIO-testable init state machine.**
- [x] **Step 4: Run `cargo test -p pythos-core sdhci_probe` and verify it passes.**

### Task 2: Probe Boot and Framebuffer Integration

**Files:**
- Modify: `core/src/hardware_probe_boot.rs`
- Modify: `core/src/hardware_probe_screen.rs`
- Modify: `core/src/font.rs`

**Interfaces:**
- Consumes: `SdhciInitializationReport`
- Produces: `sdhci init` framebuffer panel and `PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT_READY`

- [x] **Step 1: Write failing formatter tests for an `sdhci init` panel.**
- [x] **Step 2: Add the init report to the probe boot path after the register snapshot succeeds.**
- [x] **Step 3: Emit typed init success/failure serial markers.**
- [x] **Step 4: Run `cargo test -p pythos-core hardware_probe_screen font`.**

### Task 3: QEMU Acceptance and Deployment

**Files:**
- Modify: `scripts/test-hardware-probe.py`

**Interfaces:**
- Consumes: `scripts/run-qemu.py --sdhci`
- Produces: QEMU serial acceptance requiring `SDHCI_INIT_READY`

- [x] **Step 1: Add `PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT_READY` to required markers.**
- [x] **Step 2: Run `python scripts/test-hardware-probe.py`.**
- [x] **Step 3: Run `cargo build -p pythos-core --target x86_64-unknown-none --features verify`.**
- [x] **Step 4: Commit, push to `origin/object-shell`, and deploy the rebuilt ESP to Disk 2 Partition 2.**

## Verification Results

```text
cargo test -p pythos-core sdhci_probe
7 passed

cargo test -p pythos-core hardware_probe_screen
4 passed

cargo test -p pythos-core font
4 passed

python -m unittest tests.test_qemu_exit
7 passed

python scripts/test-hardware-probe.py
PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT_READY
PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES
QEMU_OUTCOME success
HARDWARE_PROBE_TEST_OK

cargo build -p pythos-core --target x86_64-unknown-none --features verify
finished

git push origin HEAD:object-shell
124911f pushed to origin/object-shell

target/deploy-usb-esp.ps1
USB_ESP_DEPLOY_OK

real laptop no-serial framebuffer boot
sdhci init
reset 00
clock 0003
power 0F
state 01FF0000
ints 00000000
```
