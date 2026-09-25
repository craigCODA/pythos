# Phase 15 PCI Memory-Space Enable Experiment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an isolated diagnostic profile that temporarily sets only PCI Memory Space Enable, reads the existing fixed network status register, and restores the original PCI command word.

**Architecture:** Keep ADR 0107 unchanged. Add a new feature-gated boot path and one narrow 16-bit PCI configuration write helper used only by this profile. Reuse the existing target/BAR/mapping/read policy, and require command readback before and after the one MMIO load.

**Tech Stack:** Rust `no_std` PythCore, legacy PCI configuration mechanism #1, existing x86-64 page-table builder, Python marker oracle, QEMU `e1000`/`e1000e`, physical Lenovo evidence.

**Spec:** `docs/decisions/0108-phase-15-pci-memory-space-enable-experiment.md`

## Global Constraints

- Only `network-hardware-register-enable-probe` may perform this PCI configuration write.
- Write exactly the low 16-bit PCI Command register at offset `0x04`; never write the adjacent Status upper word.
- Set only Memory Space Enable (`1 << 1`); never set Bus Master Enable (`1 << 2`).
- Verify enable readback, perform one fixed 32-bit volatile register read, restore the original low command word, and verify restoration.
- Do not write BARs, MMIO registers, queues, interrupts, firmware, or network data.
- Preserve `VirtioTransport`, transport adapter, and `NetworkPort` terminology and all ADR 0107 behavior.
- Preserve the existing Phase 15 ISOs; the new ISO receives a unique name under `F:\iso`.

## Review Focus

- Low-word configuration access must not write the adjacent Status register; test the exact port-width and offset contract.
- Command derivation must not add Bus Master Enable or alter unrelated original command bits; test original values with and without MSE.
- Enable readback failure must stop before MMIO; test the terminal failure transcript.
- Restore must occur after the register read and must be verified; test restoration mismatch as failure.
- Existing profiles must not compile the new write helper or dispatch this boot path; test feature isolation and static forbidden markers.

### Task 1: Record the decision and pure command-transition policy

**Files:**
- Create: `docs/decisions/0108-phase-15-pci-memory-space-enable-experiment.md`
- Create: `core/src/network_hardware_register_enable_probe.rs`
- Create: `tests/test_network_hardware_register_enable_probe.py`

**Interfaces:**
- Consume: `NetworkController`, ADR 0107 `RegisterProbePlan`, and the original command/status dword.
- Produce: pure `derive_enabled_command`, `restore_command`, and readback-validation functions plus unit/static tests.

- [x] Write tests for setting only bit 1, preserving unrelated command bits, refusing Bus Master Enable, and restoring the original command.
- [x] Run the focused tests and observe the expected missing-function failure.
- [x] Implement the pure policy without hardware access.
- [x] Rerun focused Rust/Python tests and commit the policy.

### Task 2: Add the isolated 16-bit PCI command write and boot path

**Files:**
- Modify: `core/Cargo.toml`
- Modify: `core/src/main.rs`
- Modify: `core/src/network_hardware_probe.rs`
- Create: `core/src/network_hardware_register_enable_probe_boot.rs`
- Create: `core/src/network_hardware_register_enable_probe_screen.rs`
- Modify: `core/src/memory/virtual.rs` only if the existing dedicated mapping option cannot be reused unchanged.

**Interfaces:**
- Consume: Task 1 command policy and ADR 0107 target/mapping policy.
- Produce: feature-gated boot flow with `write_controller_command_word(controller, value: u16)` and verified enable/read/restore markers.

- [x] Add feature isolation and a 16-bit config-data write at offset `0x04`; keep all existing profiles unchanged.
- [x] Add the failing boot/contract assertions for missing write, readback, restore, and final-marker ordering.
- [x] Implement the minimal enable, verify, one-read, restore, verify flow; halt on any failed gate.
- [x] Render original/after/restored command state and the fixed register result on the framebuffer.
- [x] Run focused Rust tests, bare-metal build, and clippy.

### Task 3: Add synthetic and QEMU safety oracles

**Files:**
- Create: `scripts/test-network-hardware-register-enable-probe.py`
- Modify: `tests/test_network_hardware_register_enable_probe.py`

**Interfaces:**
- Consume: Task 2 markers and the existing QEMU runner.
- Produce: static forbidden-operation checks, synthetic disabled/write/restore transcripts, and live QEMU already-enabled acceptance.

- [x] Require exact ordered command/readback/restore markers and reject missing restoration.
- [x] Reject dword PCI writes, BAR/MMIO writes, Bus Master Enable, DMA, interrupts, reset, queues, packets, sockets, NetworkPort, Wi-Fi, and later Phase 15 markers.
- [x] Run QEMU `e1000` and `e1000e` with `-nic none` and verify the already-enabled path.
- [x] Run the self-test and focused Python tests.

### Task 4: Build the physical gated image and observe Lenovo

**Files:**
- Create: `docs/evidence/2026-09-24-phase-15-pci-memory-space-enable.md`
- Modify: `README.md`, `docs/HANDOVER.md`, `docs/ROADMAP.md`, `docs/PythOS-TDD-001.md`, and `AGENTS.md` only for current status.

**Interfaces:**
- Consume: QEMU acceptance, the unique ISO, and owner-provided Lenovo photo/serial evidence.
- Produce: command-before/after/restored evidence or an explicitly failed restoration result.

- [x] Build and copy the corrected unique ISO without overwriting any existing image under `F:\iso`; the first physical attempt exposed a discovery-policy defect.
- [ ] Boot the corrected ISO on Lenovo and record command-before, command-after, register result, command-restored, and the photo hash.
- [ ] Treat missing or mismatched restoration as failure; do not claim register reachability in that case.

### Task 5: Final verification and handoff

**Files:**
- Modify: this plan with completed checks and verification notes.
- Modify: ADR 0108 status only after all acceptance gates pass.

- [x] Run focused/full Rust tests, Python tests, clippy, formatting, static safety scans, and prior ADR 0107/BAR oracles for the corrected path.
- [x] Confirm default and normal-session profiles remain unchanged.
- [x] Perform the correction scope scan and complete a whole-branch self-review.
- [ ] Mark ADR 0108 Accepted only after QEMU acceptance and a verified physical restore.

## Self-Review Notes

This plan adds one reversible PCI command transition only because ADR 0107
proved the Lenovo gate is currently disabled. It does not authorize any device
operation. The physical run remains a distinct acceptance gate because the
configuration write is a real hardware side effect even though it is restored.
