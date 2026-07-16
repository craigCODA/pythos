# Phase 7 Persistent Object Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Phase 7 so Phase 8 real hardware isolation is the next roadmap step.

**Architecture:** Add storage in substrate-first order. Extend the QEMU harness to attach a QEMU `virtio-blk` disk image, add one focused Rust module per storage concern, extend marker tests before each implementation, and commit after each verified slice.

**Tech Stack:** Rust `no_std` PythCore modules, Rust host unit tests, Python QEMU marker and persistence harnesses, QEMU q35 with OVMF and `virtio-blk`.

## Global Constraints

Serial output is the oracle. A compile is not a boot proof.
Phase 7 markers must appear after `PYTHOS:CORE:PHASE_6_COMPLETE` and before `PYTHOS:CORE:FRAMEBUFFER_READY`.
Durability primitives must land before typed-object features.
ADR 0018 is the in-memory typed-object contract that Phase 7 extends to disk.
Every ABI or durable format change gets an ADR before or with the slice that introduces it.
No Causal Lens UI, Patch, networking, multi-user access control, AI, ring-3, SMP, package management, or broad hardware support.

---

### Task 1: Block Device Driver

**Files:** `core/src/block_device.rs`, `core/src/main.rs`, `scripts/run-qemu.py`, `scripts/test-boot.py`, `tests/boot_core_handoff.py`, docs.

**Interfaces:**
- Produces: `BlockDeviceInfo`, `BlockDeviceError`, `select_qemu_virtio_blk`, `PYTHOS:CORE:BLOCK:DEVICE_SELECTED`, `PYTHOS:CORE:BLOCK_DEVICE_READY`

- [ ] Add the `block-device-driver` marker expectation and verify QEMU fails with the missing marker.
- [ ] Attach a bounded raw storage image to QEMU as `virtio-blk`.
- [ ] Scan the primary PCI bus for QEMU virtio-blk and expose bounded device metadata without data I/O.
- [ ] Run Rust unit tests and `python scripts\test-boot.py --slice block-device-driver`.
- [ ] Commit.

### Task 2: Storage Service

**Files:** `core/src/storage_service.rs`, `core/src/main.rs`, marker tests, docs.

**Interfaces:**
- Consumes: `BlockDeviceInfo`
- Produces: `StorageService`, capability-gated block read/write requests, `PYTHOS:CORE:STORAGE:ACCESS_GRANTED`, `PYTHOS:CORE:STORAGE:ACCESS_DENIED`, `PYTHOS:CORE:STORAGE_SERVICE_READY`

- [ ] Add the `storage-service` marker expectation and verify it fails.
- [ ] Implement fixed service identity and capability checks for storage operations.
- [ ] Prove a service without storage capability cannot touch the block device path.
- [ ] Verify the focused QEMU slice and host tests.
- [ ] Commit.

### Task 3: Append-Only Journal

**Files:** `core/src/storage_journal.rs`, `core/src/storage_service.rs`, marker tests, docs.

**Interfaces:**
- Consumes: capability-gated storage service writes
- Produces: fixed journal record layout, journal append operation, `PYTHOS:CORE:STORAGE:JOURNAL_APPEND`, `PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY`

- [ ] Add the `append-only-journal` marker expectation and verify it fails.
- [ ] Define a bounded append-only journal record and writer.
- [ ] Prove writes go through the journal path before any object-store state is exposed.
- [ ] Verify and commit.

### Task 4: Checksums And Commit Markers

**Files:** `core/src/storage_journal.rs`, marker tests, docs.

**Interfaces:**
- Consumes: journal records
- Produces: checksum validation, commit-marker validation, `PYTHOS:CORE:STORAGE:COMMIT_MARKER`, `PYTHOS:CORE:STORAGE:TORN_WRITE_DETECTED`, `PYTHOS:CORE:CHECKSUMS_AND_COMMITS_READY`

- [ ] Add the `checksums-and-commit-markers` marker expectation and verify it fails.
- [ ] Add deterministic checksums over journal payloads and explicit commit markers.
- [ ] Prove a torn write is rejected rather than accepted as committed state.
- [ ] Verify and commit.

### Task 5: Crash Recovery

**Files:** `core/src/storage_recovery.rs`, `core/src/storage_journal.rs`, `scripts/test-storage-recovery.py`, marker tests, docs.

**Interfaces:**
- Consumes: checksummed journal records and commit markers
- Produces: replay/rollback to last consistent commit, interrupted-write test, `PYTHOS:CORE:STORAGE:RECOVERY_REPLAY`, `PYTHOS:CORE:STORAGE:RECOVERY_ROLLBACK`, `PYTHOS:CORE:CRASH_RECOVERY_READY`

- [ ] Add the `crash-recovery` marker expectation and verify it fails.
- [ ] Implement journal replay that accepts only complete committed records.
- [ ] Add an automated interrupted-write test that kills QEMU mid-commit and verifies rollback.
- [ ] Verify and commit.

### Task 6: Typed Object Format

**Files:** `docs/decisions/0022-phase-7-on-disk-typed-object-format.md`, `core/src/object_store.rs`, marker tests, docs.

**Interfaces:**
- Consumes: recovered journal substrate, ADR 0018 `ObjectId` / `ObjectKind` / `PresentationBinding`
- Produces: versioned on-disk typed object record, `PYTHOS:CORE:OBJECT:FORMAT`, `PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY`

- [ ] Write and accept the on-disk typed-object-format ADR before production format code.
- [ ] Add the `typed-object-format` marker expectation and verify it fails.
- [ ] Encode/decode stable id, kind, schema version, and bounded fields.
- [ ] Verify and commit.

### Task 7: Object Relationships

**Files:** `core/src/object_relationships.rs`, `core/src/object_store.rs`, marker tests, docs.

**Interfaces:**
- Consumes: typed object records
- Produces: typed queryable relationships, `PYTHOS:CORE:OBJECT:RELATIONSHIP`, `PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY`

- [ ] Add the `object-relationships` marker expectation and verify it fails.
- [ ] Store and query fixed relationship kinds: `blocks`, `created-by`, and `depends-on`.
- [ ] Verify and commit.

### Task 8: Revision History

**Files:** `core/src/object_revisions.rs`, `core/src/object_store.rs`, marker tests, docs.

**Interfaces:**
- Consumes: typed objects and service identities
- Produces: retained prior revisions with timestamp and writer identity, `PYTHOS:CORE:OBJECT:REVISION`, `PYTHOS:CORE:REVISION_HISTORY_READY`

- [ ] Add the `revision-history` marker expectation and verify it fails.
- [ ] Retain prior object versions on write with monotonic tick timestamp and writer service identity.
- [ ] Verify and commit.

### Task 9: Workspace Objects

**Files:** `core/src/workspace_objects.rs`, `core/src/object_store.rs`, marker tests, docs.

**Interfaces:**
- Consumes: Phase 5 shell object ids and presentation bindings
- Produces: saved shell layout/session object kind, `PYTHOS:CORE:WORKSPACE:OBJECT_SAVED`, `PYTHOS:CORE:WORKSPACE_OBJECTS_READY`

- [ ] Add the `workspace-objects` marker expectation and verify it fails.
- [ ] Persist a fixed shell layout/session object derived from Phase 5 window objects.
- [ ] Verify and commit.

### Task 10: Object Browser

**Files:** `core/src/object_browser.rs`, `core/src/shell_apps.rs`, marker tests, docs.

**Interfaces:**
- Consumes: object store list/detail/relationship/revision APIs
- Produces: minimal inspection app, `PYTHOS:CORE:APP:OBJECT_BROWSER`, `PYTHOS:CORE:OBJECT_BROWSER_READY`

- [ ] Add the `object-browser` marker expectation and verify it fails.
- [ ] Add a fixed Phase 5-style shell app that lists stored objects and exposes relationships and revisions for inspection.
- [ ] Verify and commit.

### Task 11: Save And Restore Across Reboot

**Files:** `scripts/test-object-persistence.py`, `scripts/test-boot.py`, storage modules, marker tests, docs.

**Interfaces:**
- Consumes: object browser and full object store
- Produces: reboot persistence proof, `PYTHOS:CORE:OBJECT:RESTORED`, `PYTHOS:CORE:PHASE_7_COMPLETE`

- [ ] Add the `save-and-restore-across-reboot` marker expectation and verify it fails.
- [ ] Create objects, reboot QEMU using the same storage image, and verify identical object state after re-query.
- [ ] Re-run the deliberately interrupted-write recovery test.
- [ ] Update `docs/vision/patch.md` with the non-binding provenance answer only.
- [ ] Run full ESP/ISO verification, update handover docs, commit, push, and stop at the Phase 7 -> Phase 8 boundary.
