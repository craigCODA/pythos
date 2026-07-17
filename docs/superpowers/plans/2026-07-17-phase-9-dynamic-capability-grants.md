# Phase 9 Dynamic Capability Grants Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Phase 9 `dynamic-capability-grants` slice without
starting `process-argv-and-environment`.

**Architecture:** Add `core/src/dynamic_capabilities.rs` as a pure
allocation-free process capability inventory over the existing kernel-owned
`CapabilityTable`. Wire its boot self-test after `COPY_IN_COPY_OUT_READY`, emit
specific grant/denial markers, and update marker contracts.

**Tech Stack:** Rust `#![no_std]` PythCore module, Python unittest marker
contracts, QEMU serial acceptance.

## Global Constraints

- Read `docs/PythOS-SAS-001.md` and `docs/PythOS-TDD-001.md` before editing.
- Do not add syscall numbers or silently change ABI.
- Do not add argv/env or execute the loaded ELF.
- Do not begin Phase 9 `process-argv-and-environment`.
- Serial output is the boot oracle; `QEMU_OUTCOME success` is required.

---

### Task 1: Dynamic Process Capability Module

**Files:**
- Create: `core/src/dynamic_capabilities.rs`
- Modify: `core/src/main.rs`

**Interfaces:**
- Produces: `dynamic_capabilities::run_self_test() -> Result<DynamicCapabilityGrantProof, DynamicCapabilityError>`
- Produces: `DynamicCapabilityGrantProof { process_created, zero_default, no_grant_denied, grant_issued, granted_use }`

- [ ] Write failing Rust tests for zero default inventory, no-grant denial, explicit grant use, capacity handling, and self-test proof.
- [ ] Run `cargo test -p pythos-core dynamic_capabilities --target x86_64-pc-windows-msvc` and confirm failure.
- [ ] Implement `DynamicProcessTable`, `DynamicProcess`, `CreatorGrantPolicy`, `InitialGrant`, `ProcessCapability`, `DynamicCapabilityError`, and `run_self_test`.
- [ ] Add `mod dynamic_capabilities;` to `core/src/main.rs`.
- [ ] Run `cargo test -p pythos-core dynamic_capabilities --target x86_64-pc-windows-msvc` and confirm pass.

### Task 2: Boot Markers

**Files:**
- Modify: `core/src/main.rs`
- Modify: `scripts/test-boot.py`
- Modify: `tests/test_boot_marker_contract.py`
- Modify: `tests/boot_core_handoff.py`

**Interfaces:**
- Consumes: `dynamic_capabilities::run_self_test()`
- Produces slice name: `dynamic-capability-grants`

- [ ] Wire the self-test after `PYTHOS:CORE:COPY_IN_COPY_OUT_READY`.
- [ ] Emit `PYTHOS:CORE:DYNAMIC_CAPABILITY:PROCESS_CREATED`, `PYTHOS:CORE:DYNAMIC_CAPABILITY:ZERO_DEFAULT`, `PYTHOS:CORE:DYNAMIC_CAPABILITY:NO_GRANT_DENIED`, `PYTHOS:CORE:DYNAMIC_CAPABILITY:GRANT`, `PYTHOS:CORE:DYNAMIC_CAPABILITY:USE`, and `PYTHOS:CORE:DYNAMIC_CAPABILITY_GRANTS_READY`.
- [ ] Extend `scripts/test-boot.py` with `DYNAMIC_CAPABILITY_GRANT_MARKERS`, `SLICE_MARKERS["dynamic-capability-grants"]`, milestone insertion, and no-audio fallback insertion.
- [ ] Extend Python tests to assert ordering after `COPY_IN_COPY_OUT_READY` and before `FRAMEBUFFER_READY`.
- [ ] Run `python -m unittest tests.test_boot_marker_contract`.

### Task 3: Active-Milestone Docs

**Files:**
- Modify: `docs/ROADMAP.md`
- Modify: `docs/PythOS-TDD-001.md`
- Modify: `AGENTS.md`

- [ ] Mark `dynamic-capability-grants` complete in `docs/ROADMAP.md`, name ADR 0040, and set next slice to `process-argv-and-environment`.
- [ ] Add the slice description and marker requirements to `docs/PythOS-TDD-001.md`.
- [ ] Add markers and halt boundary to `AGENTS.md`.

### Task 4: Verification and Commit

**Files:**
- All changed files

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo test -p pythos-core --target x86_64-pc-windows-msvc`.
- [ ] Run `python -m unittest tests.test_boot_marker_contract`.
- [ ] Run `python scripts\test-boot.py --slice dynamic-capability-grants --timeout 60`.
- [ ] Run `python scripts\test-boot.py --slice milestone-1 --timeout 60`.
- [ ] Run `python scripts\test-boot.py --slice milestone-1 --media iso --timeout 60`.
- [ ] Run `python scripts\test-boot.py --slice graceful-audio-fallback --no-audio-device --timeout 60`.
- [ ] Commit as `feat: add phase 9 dynamic capability grants`.
- [ ] Push branch and confirm GitHub Actions success.
