# ADR 0044: Phase 10 Block Allocator Format

## Status

Accepted

## Context

Phase 7 proved a fixed checkpoint sector, not general allocation. Phase 10
turns that fixed slot into a bounded general-purpose allocator for typed object
storage. The allocator now becomes part of the durable object-store state: if a
free-space update tears or is replayed out of order, later dynamic object work
can corrupt live objects by reusing blocks that are still authoritative.

## Decision

Phase 10 uses a bitmap allocator over a bounded object-store data region. Each
bit represents one allocatable block. Blocks outside that region remain reserved
for boot, Phase 7 checkpoint sectors, control sectors, metadata sectors, and
future layout expansion.

Allocator metadata is journaled. A bitmap change is not authoritative until a
journal record covering the allocator generation, previous bitmap, next bitmap,
checksum, and explicit commit marker validates. Recovery replays only the
committed prefix and keeps the last committed bitmap. Torn or checksum-invalid
allocator metadata is rolled back and never silently accepted.

This deliberately reuses Phase 7's journal-first posture at the allocator
metadata layer. Free-space tracking does not sit outside the journal.

## Consequences

The `block-allocator` slice must prove:

```text
PYTHOS:CORE:ALLOCATOR:BITMAP_READY
PYTHOS:CORE:ALLOCATOR:METADATA_JOURNALED
PYTHOS:CORE:ALLOCATOR:TORN_METADATA_ROLLED_BACK
PYTHOS:CORE:BLOCK_ALLOCATOR_READY
```

Later Phase 10 slices may allocate more than one object, delete objects, reuse
freed blocks, enforce quotas, serialize concurrent writers, and test
adversarial storage cases. They must consume the committed allocator state
rather than inventing another free-space source.

This ADR does not add filesystem paths, POSIX directories, package
installation, networking, updates, hardware expansion, or SMP.
