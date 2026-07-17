# Phase 10 General-Purpose Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Phase 10 with a bounded dynamic storage allocator and object
store, then halt before Phase 11.

**Architecture:** Add focused Phase 10 modules under `core/src/` and wire each
slice into the existing serial-marker boot path before `FRAMEBUFFER_READY`.
Allocator metadata is journaled per ADR 0044, not maintained as unchecked side
state.

**Tech Stack:** Rust `no_std` PythCore modules, Python QEMU marker harnesses,
ADR markdown docs, GitHub Actions QEMU acceptance.

## Global Constraints

Do not start Phase 11. Do not add filesystem paths, package management,
networking, updates, hardware expansion, AI, or SMP. Every slice must have
unit tests, marker contract coverage, QEMU acceptance coverage, and a commit.
The persistent torn-write proof must kill QEMU mid-commit and recover on reboot.

---

### Task 1: `block-allocator`

**Files:**
- Create: `core/src/storage_allocator.rs`
- Modify: `core/src/main.rs`
- Modify: `scripts/test-boot.py`
- Modify: `tests/test_boot_marker_contract.py`
- Modify: `tests/boot_core_handoff.py`
- Create: `docs/decisions/0044-phase-10-block-allocator-format.md`

**Interfaces:**
- Produces: `BlockAllocator`, `AllocatorJournal`, `run_self_test()`.
- Consumes: `BlockDeviceInfo` for bounded capacity checks.

- [ ] Write failing tests for first-fit allocation, freeing/reuse, committed
      bitmap journal records, and rollback of torn allocator metadata.
- [ ] Implement fixed bitmap allocator and journaled metadata commit.
- [ ] Emit allocator markers after `PYTHOS:CORE:PHASE_9_COMPLETE`.
- [ ] Run `cargo test -p pythos-core storage_allocator --target x86_64-pc-windows-msvc`.
- [ ] Run `python -m unittest tests.test_boot_marker_contract`.
- [ ] Run `python scripts/test-boot.py --slice block-allocator --timeout 60`.
- [ ] Commit as `feat: add phase 10 block allocator`.

### Task 2: `dynamic-object-count`

**Files:**
- Create: `core/src/dynamic_object_store.rs`
- Modify: `core/src/main.rs`
- Modify: `scripts/test-boot.py`
- Modify: `tests/test_boot_marker_contract.py`
- Modify: `tests/boot_core_handoff.py`

**Interfaces:**
- Consumes: `BlockAllocator`.
- Produces: `DynamicObjectStore`, `run_self_test()`.

- [ ] Write failing tests for creating multiple typed objects, deleting one,
      and preserving count/block ownership.
- [ ] Implement bounded dynamic object slots backed by allocator blocks.
- [ ] Emit dynamic object markers.
- [ ] Verify with Rust, Python marker contract, and QEMU slice tests.
- [ ] Commit as `feat: add phase 10 dynamic object count`.

### Task 3: `fragmentation-and-compaction-policy`

**Files:**
- Create: `docs/decisions/0045-phase-10-fragmentation-policy.md`
- Modify: `core/src/dynamic_object_store.rs`
- Modify marker docs/tests/scripts.

**Interfaces:**
- Consumes: `DynamicObjectStore::delete_object`.
- Produces: first-fit freed-block reuse proof.

- [ ] Write failing test proving a freed middle block is reused.
- [ ] Record ADR 0045: compaction deferred, first-fit reuse required.
- [ ] Implement/verify reuse proof and markers.
- [ ] Commit as `feat: add phase 10 fragmentation policy`.

### Task 4: `storage-quota-per-service`

**Files:**
- Create: `core/src/storage_quotas.rs`
- Modify marker docs/tests/scripts.

**Interfaces:**
- Consumes: `ServiceId`.
- Produces: `DiskQuotaTable`, `run_self_test()`.

- [ ] Write failing tests for in-quota block charge, out-of-quota denial, and
      non-mutating denial.
- [ ] Implement per-service disk quota records.
- [ ] Emit quota markers.
- [ ] Commit as `feat: add phase 10 storage quotas`.

### Task 5: `concurrent-write-safety`

**Files:**
- Create: `core/src/storage_concurrency.rs`
- Modify marker docs/tests/scripts.

**Interfaces:**
- Consumes: `DynamicObjectStore`.
- Produces: `StorageWriteGate`, serialized write proof.

- [ ] Write failing tests for single-writer token ownership, second-writer
      denial while locked, and successful second write after commit.
- [ ] Implement bounded serialized write gate.
- [ ] Emit concurrent write markers.
- [ ] Commit as `feat: add phase 10 concurrent write safety`.

### Task 6: `storage-adversarial-suite`

**Files:**
- Create: `core/src/storage_adversarial.rs`
- Modify: `scripts/test-persistent-storage.py`
- Modify: `.github/workflows/qemu-acceptance.yml`
- Modify marker docs/tests/scripts and hard-stop docs.

**Interfaces:**
- Consumes: allocator, dynamic object store, quotas, concurrency.
- Produces: `run_self_test()` and Phase 10 completion marker.

- [ ] Write failing tests for repeated create/delete/write cycles,
      out-of-quota denial, and dynamic torn-write recovery.
- [ ] Implement adversarial suite.
- [ ] Extend persistent-storage harness with a Phase 10 killed mid-commit path.
- [ ] Emit `PYTHOS:CORE:PHASE_10_COMPLETE`.
- [ ] Update `AGENTS.md`, `docs/ROADMAP.md`, and `docs/PythOS-TDD-001.md`
      to halt at Phase 10 -> Phase 11.
- [ ] Run full local verification and CI.
- [ ] Commit as `feat: complete phase 10 general storage`.
