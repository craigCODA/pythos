# Phase 9 Process Argv and Environment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Phase 9 `process-argv-and-environment` slice without
starting `general-fault-isolation`.

**Architecture:** Add `core/src/process_launch.rs` as a pure allocation-free
launch-context module over `dynamic_capabilities::DynamicProcess`. It stores
bounded argv strings directly, stores environment entries with resource ids,
and gates environment reads through the existing process capability inventory.

**Tech Stack:** Rust `#![no_std]` PythCore module, Python unittest marker
contracts, QEMU serial acceptance.

## Global Constraints

- Read `docs/PythOS-SAS-001.md` and `docs/PythOS-TDD-001.md` before editing.
- Do not add syscall numbers or silently change ABI.
- Do not execute the loaded ELF.
- Do not begin Phase 9 `general-fault-isolation`.
- Serial output is the boot oracle; `QEMU_OUTCOME success` is required.

---

### Task 1: Launch Context Module

**Files:**
- Create: `core/src/process_launch.rs`
- Modify: `core/src/main.rs`

**Interfaces:**
- Produces: `process_launch::run_self_test() -> Result<ProcessLaunchProof, ProcessLaunchError>`
- Produces: `ProcessLaunchProof { argv_delivered, env_capability_allowed, ungranted_env_denied }`

- [ ] Write failing Rust tests for bounded launch strings, argv delivery, granted environment read, ungranted environment denial, unknown key denial, and self-test proof.
- [ ] Run `cargo test -p pythos-core process_launch --target x86_64-pc-windows-msvc` and confirm failure.
- [ ] Implement `LaunchString`, `LaunchArguments`, `EnvironmentEntry`, `ProcessEnvironment`, `ProcessLaunchContext`, `ProcessLaunchError`, `ProcessLaunchProof`, and `run_self_test`.
- [ ] Add `mod process_launch;` to `core/src/main.rs`.
- [ ] Run `cargo test -p pythos-core process_launch --target x86_64-pc-windows-msvc` and confirm pass.

### Task 2: Boot Markers

**Files:**
- Modify: `core/src/main.rs`
- Modify: `scripts/test-boot.py`
- Modify: `tests/test_boot_marker_contract.py`
- Modify: `tests/boot_core_handoff.py`

**Interfaces:**
- Consumes: `process_launch::run_self_test()`
- Produces slice name: `process-argv-and-environment`

- [ ] Wire the self-test after `PYTHOS:CORE:DYNAMIC_CAPABILITY_GRANTS_READY`.
- [ ] Emit `PYTHOS:CORE:PROCESS_ARGV:DELIVERED`, `PYTHOS:CORE:PROCESS_ENV:CAPABILITY_ALLOWED`, `PYTHOS:CORE:PROCESS_ENV:UNGRANTED_DENIED`, and `PYTHOS:CORE:PROCESS_ARGV_ENV_READY`.
- [ ] Extend `scripts/test-boot.py` with `PROCESS_ARGV_ENV_MARKERS`, `SLICE_MARKERS["process-argv-and-environment"]`, milestone insertion, and no-audio fallback insertion.
- [ ] Extend Python tests to assert ordering after `DYNAMIC_CAPABILITY_GRANTS_READY` and before `FRAMEBUFFER_READY`.
- [ ] Run `python -m unittest tests.test_boot_marker_contract`.

### Task 3: Active-Milestone Docs

**Files:**
- Modify: `docs/ROADMAP.md`
- Modify: `docs/PythOS-TDD-001.md`
- Modify: `AGENTS.md`

- [ ] Mark `process-argv-and-environment` complete in `docs/ROADMAP.md`, name ADR 0041, and set next slice to `general-fault-isolation`.
- [ ] Add the slice description and marker requirements to `docs/PythOS-TDD-001.md`.
- [ ] Add markers and halt boundary to `AGENTS.md`.

### Task 4: Verification and Commit

**Files:**
- All changed files

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo test -p pythos-core --target x86_64-pc-windows-msvc`.
- [ ] Run `python -m unittest tests.test_boot_marker_contract`.
- [ ] Run `python scripts\test-boot.py --slice process-argv-and-environment --timeout 60`.
- [ ] Run `python scripts\test-boot.py --slice milestone-1 --timeout 60`.
- [ ] Run `python scripts\test-boot.py --slice milestone-1 --media iso --timeout 60`.
- [ ] Run `python scripts\test-boot.py --slice graceful-audio-fallback --no-audio-device --timeout 60`.
- [ ] Commit as `feat: add phase 9 process argv environment`.
- [ ] Push branch and confirm GitHub Actions success.
