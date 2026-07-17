# Phase 10 General-Purpose Storage Design

## Goal

Complete Phase 10 by generalizing Phase 7's fixed checkpoint sector into a
bounded dynamic typed-object store with allocator metadata, dynamic object
counts, documented fragmentation policy, per-service disk quotas, serialized
multi-service writes, and adversarial storage proofs.

## Scope

Implement only Phase 10:

1. `block-allocator`
2. `dynamic-object-count`
3. `fragmentation-and-compaction-policy`
4. `storage-quota-per-service`
5. `concurrent-write-safety`
6. `storage-adversarial-suite`

Halt after `PYTHOS:CORE:PHASE_10_COMPLETE`. Do not begin Phase 11 real-hardware
boot smoke testing or later phases.

## Architecture

Phase 10 remains bounded and fixed-size, matching the proof style of prior
phases. It does not introduce a general filesystem or unbounded allocation.

The first durable decision is ADR 0044: allocator metadata is itself journaled.
The bitmap does not sit outside the journal. A free or allocation state change
becomes authoritative only after a committed journal record with checksum and
commit marker validates.

The object store consumes allocator blocks for ADR 0022 typed object records.
It supports a growing bounded count, deletion, and reuse of freed blocks. The
fragmentation slice records compaction as deferred and proves first-fit reuse
of a freed hole. The quota slice gates block charges by service identity. The
concurrency slice serializes writes with an explicit writer token. The final
adversarial suite combines repeated create/delete/write cycles, out-of-quota
denial, and dynamic torn-write recovery.

## Markers

```text
PYTHOS:CORE:ALLOCATOR:BITMAP_READY
PYTHOS:CORE:ALLOCATOR:METADATA_JOURNALED
PYTHOS:CORE:ALLOCATOR:TORN_METADATA_ROLLED_BACK
PYTHOS:CORE:BLOCK_ALLOCATOR_READY
PYTHOS:CORE:DYNAMIC_OBJECT:CREATED
PYTHOS:CORE:DYNAMIC_OBJECT:DELETED
PYTHOS:CORE:DYNAMIC_OBJECT_COUNT_READY
PYTHOS:CORE:FRAGMENTATION:POLICY_RECORDED
PYTHOS:CORE:FRAGMENTATION:FREED_BLOCK_REUSED
PYTHOS:CORE:FRAGMENTATION_COMPACTION_POLICY_READY
PYTHOS:CORE:STORAGE_QUOTA:GRANTED
PYTHOS:CORE:STORAGE_QUOTA:DENIED
PYTHOS:CORE:STORAGE_QUOTA_PER_SERVICE_READY
PYTHOS:CORE:CONCURRENT_WRITE:SERIALIZED
PYTHOS:CORE:CONCURRENT_WRITE:CORRUPTION_DENIED
PYTHOS:CORE:CONCURRENT_WRITE_SAFETY_READY
PYTHOS:CORE:STORAGE_ADVERSARIAL:CREATE_DELETE_CYCLE
PYTHOS:CORE:STORAGE_ADVERSARIAL:OUT_OF_QUOTA_DENIED
PYTHOS:CORE:STORAGE_ADVERSARIAL:DYNAMIC_TORN_WRITE_RECOVERED
PYTHOS:CORE:STORAGE_ADVERSARIAL_SUITE_READY
PYTHOS:CORE:PHASE_10_COMPLETE
```

## Required Tests

Each slice gets Rust unit tests first and boot marker contract tests before
the corresponding production marker is wired into `pythcore_entry`.

The final phase also updates the persistent-storage QEMU harness so a dynamic
allocation torn-write path is killed mid-commit and then recovered on reboot.
