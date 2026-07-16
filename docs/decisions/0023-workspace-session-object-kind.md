# ADR 0023: Workspace Session Object Kind

## Status

Accepted

## Context

Phase 7 `workspace-objects` is the first concrete persistent object kind built
on ADR 0022. ADR 0022 locks the typed-object record layout, but later slices
are expected to define object kinds and versioned fields within that layout.

This slice needs to persist Phase 5 window identities and presentation bindings
without changing the `PYOB` record format and without collapsing object identity
into framebuffer pixels.

## Decision

Reserve `ObjectKind` code `8` for `WorkspaceSession`.

The schema version 1 workspace-session object stores one fixed field per saved
window layout. Each field value is a bounded 16-byte little-endian layout
payload:

* 64-bit window `ObjectId`
* 16-bit x coordinate
* 16-bit y coordinate
* 8-bit width
* 8-bit height
* 8-bit z-order
* 8-bit reserved zero

This is a schema addition under ADR 0022, not a record-format change. The
record magic, format version, header layout, and field slot layout remain
unchanged.

## Consequences

The workspace-object slice must prove that a workspace session is a typed
object with kind `WorkspaceSession`, captures Phase 5 window object ids, and
round-trips layout fields through the current typed-object and revision-history
substrate.

This ADR does not implement object browsing, reboot persistence, sector writes,
or generalized workspace policy.
