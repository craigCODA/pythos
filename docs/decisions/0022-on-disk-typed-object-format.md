# ADR 0022: On-Disk Typed Object Format

## Status

Accepted

## Context

Phase 7 makes the Phase 5 typed-object split durable. ADR 0018 established that
`ObjectId` and `ObjectKind` define object identity and meaning, while
`PresentationBinding` remains a separate rendering concern. The persistent
object store needs a stable binary representation before relationships,
revision history, workspace objects, or object-browser views build on top of it.

This is the format lock-in point for Phase 7. After this ADR, incompatible
format changes require explicit migrations rather than silent rewrites.

## Decision

Typed objects are persisted as fixed-layout little-endian records with:

* magic `PYOB`
* format version `1`
* total record length
* stable 64-bit `ObjectId`
* 16-bit `ObjectKind` code
* object schema version
* bounded field count
* fixed slots for versioned fields

Each field slot carries:

* 16-bit field id
* 16-bit field version
* 16-bit value length
* reserved 16-bit zero field
* a fixed bounded value byte array

The format stores object identity and versioned data fields only. Presentation
state remains outside this record unless a later typed object kind explicitly
defines a field for a presentation concept. Phase 7's later workspace-object
slice may persist layout state as typed fields, but the object format itself
must not collapse identity into framebuffer coordinates.

## Consequences

The Phase 7 typed-object-format slice must reject bad magic, unsupported format
versions, invalid kind codes, impossible field counts, nonzero reserved fields,
and field lengths beyond the fixed slot capacity.

Object relationships, revision history, workspace layout objects, object
browser UI, and sector-level persistence are later Phase 7 slices. They must
build on this record format instead of inventing a second object identity
encoding.
