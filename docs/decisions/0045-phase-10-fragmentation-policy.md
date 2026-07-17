# ADR 0045: Phase 10 Fragmentation And Compaction Policy

Status: Accepted

## Context

Phase 10 generalizes Phase 7's single checkpoint slot into a dynamic object
store. Once objects can be deleted, free space can fragment. The phase must
state whether fragmentation is handled now or silently deferred.

Phase 10 is still establishing the storage substrate. Moving a typed object
after creation is not just a block allocation operation: relationships,
revision history, object-browser views, and later package references would
all need a stable migration rule.

## Decision

Phase 10 handles fragmentation by reusing freed extents through the ADR 0044
first-fit bitmap allocator. This is enough to prove that delete/create cycles
do not leak every freed block.

Phase 10 does not compact live objects. Live object extents are stable after
creation. Compaction is deferred until a future migration/defragmentation
decision can define how object records, relationships, revision history, and
writer provenance are updated atomically.

## Consequences

- A deleted object's block can be reused by a later object.
- A live object is never moved behind its identity in Phase 10.
- The proof marker `PYTHOS:CORE:FRAGMENTATION:FREED_BLOCK_REUSED` verifies
  freed space reuse at the dynamic-object-store level, not only at the raw
  allocator level.
- No Phase 10 claim is made about reducing long-term external fragmentation
  through compaction.
