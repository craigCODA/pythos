# ADR 0024: Object Browser Inspection App

## Status

Accepted

## Context

Phase 7 `object-browser` is the first user-facing consumer of the persistent
object substrate. It needs to inspect typed objects, relationships, and revision
history without adding reboot persistence, sector writes, networking, or
general workspace policy.

The shell already models application windows as typed objects. The object
browser should follow that split instead of exposing object-store state as
untyped debug text.

## Decision

Reserve `ObjectKind` code `9` for `ObjectBrowserWindow`.

The Phase 7 object browser is a fixed native inspection app. It can:

* list known typed object records
* inspect a selected object's typed relationship target for a requested
  relationship kind
* inspect the number of retained prior revisions for a selected object

The browser consumes the existing ADR 0022 typed-object records,
`object-relationships` store, and `revision-history` store. It does not own the
storage device and does not bypass the storage service.

## Consequences

The object-browser slice must prove a typed browser window exists, that object
listing is deterministic, and that relationship/revision details are surfaced
from the existing store substrate.

This ADR does not implement arbitrary queries, persistence across reboot,
sector writes, Patch, Causal Lens UI, networking, or multi-user behavior.
