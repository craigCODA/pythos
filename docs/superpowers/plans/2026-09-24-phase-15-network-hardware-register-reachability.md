# Phase 15 Network Hardware Register Reachability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in, read-only PythCore probe that proves one fixed network-controller status register is reachable through an already-enabled PCI memory BAR.

**Architecture:** Reuse the existing PCI identity and BAR observations. Add a pure target/window policy, a dedicated cache-disabled page-table mapping option, and a boot path that performs one volatile 32-bit load after activation. A separate QEMU/synthetic oracle and physical evidence record close the slice without adding networking behavior.

**Tech Stack:** Rust `no_std` PythCore, existing x86-64 page-table builder, Python stdlib marker oracle, QEMU `e1000`/`e1000e`, physical Lenovo evidence.

**Spec:** `docs/decisions/0107-phase-15-network-hardware-register-reachability.md`

## Global Constraints

- Only the opt-in `network-hardware-register-probe` feature may enable this slice.
- PCI configuration reads are allowed; PCI configuration writes are forbidden.
- MMIO mapping is limited to one validated 4 KiB memory-BAR window.
- The probe performs exactly one fixed volatile 32-bit register load and no MMIO write.
- Do not enable the device, set PCI Memory Space Enable, set bus mastering, reset, use firmware, enable interrupts, allocate DMA, configure queues, move frames, or use NetworkPort.
- The fixed physical status candidate is `0x00F4`; there is no DMA, no interrupts, and no packet movement in this slice.
- Preserve `VirtioTransport`, transport adapter, and `NetworkPort` terminology.
- Preserve the existing Phase 15 ISO in `F:\iso`; any new image receives a unique name.

## Review Focus

- PCI Memory Space Enable is clear: skip before mapping or dereference; test in Task 1.
- Target BAR is I/O, zero, malformed, unaligned, or overflows the window: reject; test in Task 1.
- Register offset is outside the mapped 4 KiB window: reject; test in Task 1.
- Marker transcript claims a write/control operation or reorders mapping/read evidence: reject; test in Task 3.
- Existing profiles accidentally compile or dispatch the new probe: test in Task 2.

### Task 1: Add pure register-target and window policy

**Files:**
- Modify: `core/src/network_hardware_probe.rs`
- Modify: `core/src/network_hardware_bar_probe.rs`
- Create: `core/src/network_hardware_register_probe.rs`

**Interfaces:**
- Consume `NetworkController`, `read_controller_bar_dwords`, and `decode_bar_layout`.
- Produce fixed target selection, PCI command-bit validation, BAR validation, and a mapping tuple for the boot path.

- [x] Add constants for PCI command offset `0x04`, Memory Space Enable bit `1 << 1`, a 4 KiB register window, the dedicated virtual address, and the fixed target offsets `0x08` and `0xF4`.
- [x] Add tests for all supported IDs, disabled memory space, I/O BAR rejection, malformed/unaligned/overflowing windows, and successful QEMU/Realtek policy selection.
- [x] Implement the smallest pure policy functions needed by the boot path; keep hardware `inl`/volatile access outside the pure test surface.
- [x] Run the focused Rust tests and commit the policy slice (`65f08bf`).

### Task 2: Wire the opt-in mapped boot path

**Files:**
- Modify: `core/src/main.rs`
- Modify: `core/src/memory/virtual.rs`
- Create: `core/src/network_hardware_register_probe_boot.rs`
- Modify: `core/Cargo.toml`

**Interfaces:**
- Consume the Task 1 mapping decision.
- Produce an isolated feature-gated address-space build and boot dispatch.

- [x] Add the feature and mutual-exclusion guards without changing normal, identity, BAR, or USB probe dispatch.
- [x] Add one dedicated `network_hardware_register_mmio` mapping option using `PTE_NO_EXECUTE | PTE_CACHE_DISABLE`; retain the existing mapping options unchanged.
- [x] Discover the mapping from PCI configuration before building the dedicated root, activate and validate that root, verify the translated address, then run the boot probe.
- [x] Emit only the ADR 0107 markers and halt with the QEMU success outcome; use explicit safe skip markers for every precondition failure.
- [x] Perform one fixed volatile 32-bit read only after all gates pass; do not add a write helper.
- [x] Run `cargo fmt --check` and the focused Rust build/test commands.

### Task 3: Add synthetic and QEMU acceptance oracles

**Files:**
- Create: `scripts/test-network-hardware-register-probe.py`
- Create: `tests/test_network_hardware_register_probe.py`
- Modify: `scripts/run-qemu.py` only if the existing isolated network-device arguments need a narrow reuse hook.

**Interfaces:**
- Consume the Task 2 markers and existing QEMU runner.
- Produce self-tests plus live `e1000` and `e1000e` acceptance.

- [x] Freeze ordered read and safe-skip marker contracts, including the framebuffer result marker.
- [x] Reject write/control, DMA, interrupt, reset, queue, packet, socket, NetworkPort, and Phase 15-later markers.
- [x] Build the new feature, run both QEMU models with `-nic none`, require exactly one controller, and require one successful outcome.
- [x] Run the Python self-test and live oracle; record exact serial logs.

### Task 4: Build and document physical Lenovo observation

**Files:**
- Create: `docs/evidence/2026-09-24-phase-15-network-hardware-register-reachability.md`
- Modify: `docs/evidence/README.md`
- Modify: `README.md`, `HANDOVER.md`, `ROADMAP.md`, `docs/TECHNICAL-DESIGN.md`, `docs/PythOS-TDD-001.md`, and `AGENTS.md` only where the current Phase 15 status is recorded.

**Interfaces:**
- Consume the accepted QEMU logs and new physical serial/photo evidence.
- Produce a reproducible evidence record that distinguishes a real register read from a safe skip.

- [x] Build a uniquely named ISO without overwriting any existing `F:\iso` file. Local image prepared; `F:` is not mounted in this environment.
- [ ] Owner boots the Lenovo and supplies the serial/photo result; record BDF, command word, BAR selection, register offset/value or skip reason, image hash, and photo hash.
- [ ] Update current status documents only after the evidence is captured; leave Phase 15 later scope explicitly pending.

### Task 5: Final verification and handoff

**Files:**
- Modify: this plan with completed checkboxes and verification notes.
- Modify: ADR 0107 status after all acceptance gates pass.

- [ ] Run focused Rust tests, Python tests, Python self-tests, QEMU `e1000`/`e1000e`, formatting, clippy, compile checks, and the no-diff safety scan.
- [ ] Verify the default feature build and the prior BAR oracle remain green.
- [ ] Perform a final scope scan for writes, DMA, interrupts, resets, packets, sockets, NetworkPort, and Phase 15-later work.
- [ ] Mark the ADR Accepted only if the QEMU oracle and physical result satisfy the bounded contract; otherwise retain the explicit safe-skip result and leave the next decision pending.

## Self-Review Notes

The plan covers the PCI command gate, BAR/window policy, page-table mapping, fixed register load, feature isolation, QEMU oracle, physical evidence, and final regression checks. It does not authorize a generalized MMIO abstraction or any network datapath work.
