# Phase 9 Copy-In/Copy-Out Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Phase 9 `copy-in-copy-out-policy` slice without starting `dynamic-capability-grants`.

**Architecture:** Add `core/src/user_copy.rs` as a pure policy module that validates raw user pointer/length ranges against explicit user mapping metadata. Wire its boot self-test after `GENERAL_SYSCALL_ABI_READY`, emit specific denial markers, and update marker contracts.

**Tech Stack:** Rust `#![no_std]` PythCore module, Python unittest marker contracts, QEMU serial acceptance.

## Global Constraints

- Read `docs/PythOS-SAS-001.md` and `docs/PythOS-TDD-001.md` before editing.
- Do not add syscall numbers or silently change ABI.
- Do not dereference raw user pointers in this slice.
- Do not begin Phase 9 `dynamic-capability-grants`.
- Serial output is the boot oracle; `QEMU_OUTCOME success` is required.

---

### Task 1: Policy Module

**Files:**
- Create: `core/src/user_copy.rs`
- Modify: `core/src/main.rs`

**Interfaces:**
- Produces: `user_copy::run_self_test() -> Result<UserCopyProof, UserCopyError>`
- Produces: `UserCopyProof { valid_range, out_of_range_denied, length_overflow_denied, cross_mapping_denied }`

- [ ] Write failing Rust tests for valid range, out-of-range, overflow, cross-mapping, permission denial, and self-test proof.
- [ ] Run `cargo test -p pythos-core user_copy --target x86_64-pc-windows-msvc` and confirm failure.
- [ ] Implement `UserCopyMap`, `UserMapping`, `UserCopyAccess`, `ValidatedUserRange`, `UserCopyError`, and `run_self_test`.
- [ ] Add `mod user_copy;` to `core/src/main.rs`.
- [ ] Run `cargo test -p pythos-core user_copy --target x86_64-pc-windows-msvc` and confirm pass.

### Task 2: Boot Markers

**Files:**
- Modify: `core/src/main.rs`
- Modify: `scripts/test-boot.py`
- Modify: `tests/test_boot_marker_contract.py`
- Modify: `tests/boot_core_handoff.py`

**Interfaces:**
- Consumes: `user_copy::run_self_test()`
- Produces slice name: `copy-in-copy-out-policy`

- [ ] Wire the self-test after `PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY`.
- [ ] Emit `PYTHOS:CORE:COPY:VALIDATED`, `PYTHOS:CORE:COPY:OUT_OF_RANGE_DENIED`, `PYTHOS:CORE:COPY:LENGTH_OVERFLOW_DENIED`, `PYTHOS:CORE:COPY:CROSS_MAPPING_DENIED`, and `PYTHOS:CORE:COPY_IN_COPY_OUT_READY`.
- [ ] Extend `scripts/test-boot.py` with `COPY_IN_COPY_OUT_MARKERS`, `SLICE_MARKERS["copy-in-copy-out-policy"]`, milestone insertion, and no-audio fallback insertion.
- [ ] Extend Python tests to assert ordering after `GENERAL_SYSCALL_ABI_READY` and before `FRAMEBUFFER_READY`.
- [ ] Run `python -m unittest tests.test_boot_marker_contract`.

### Task 3: Active-Milestone Docs

**Files:**
- Modify: `docs/ROADMAP.md`
- Modify: `docs/PythOS-TDD-001.md`
- Modify: `AGENTS.md`

- [ ] Mark `copy-in-copy-out-policy` complete in `docs/ROADMAP.md`, name ADR 0039, and set next slice to `dynamic-capability-grants`.
- [ ] Add the slice description and marker requirements to `docs/PythOS-TDD-001.md`.
- [ ] Add markers and halt boundary to `AGENTS.md`.

### Task 4: Verification and Commit

**Files:**
- All changed files

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo test -p pythos-core --target x86_64-pc-windows-msvc`.
- [ ] Run `python -m unittest tests.test_boot_marker_contract`.
- [ ] Run `python scripts\test-boot.py --slice copy-in-copy-out-policy --timeout 60`.
- [ ] Run `python scripts\test-boot.py --slice milestone-1 --timeout 60`.
- [ ] Run `python scripts\test-boot.py --slice milestone-1 --media iso --timeout 60`.
- [ ] Run `python scripts\test-boot.py --slice graceful-audio-fallback --no-audio-device --timeout 60`.
- [ ] Commit as `feat: add phase 9 copy in copy out policy`.
- [ ] Push branch and confirm GitHub Actions success.
