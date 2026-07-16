# ADR 0018: Phase 5 Shell Object And Input Contracts

Date: 2026-07-16

## Status

Accepted

## Context

Phase 5 is the first graphical-shell phase. It introduces input drivers,
windowing primitives, widgets, and first-party applications. The roadmap also
requires that drawable shell objects do not collapse their identity into pixels:
meaning and actions must be represented separately from presentation details
such as position, size, color, focus, and z-order.

The phase also needs an input-event service decision before raw keyboard and
mouse data become a service-facing stream.

## Decision

PythOS keeps Phase 5 native and fixed-shape inside PythCore. The input-event
service is native for this phase because the runtime is still the exact-shape
custom minimal interpreter, not a general Python environment. Input remains
capability gated: drivers produce raw device events, and only the input-event
service may normalize those into typed events for subscribers.

Every shell drawable created in Phase 5 has:

* a stable `ObjectId`
* a typed `ObjectKind`
* a separate `PresentationBinding`

`ObjectId` and `ObjectKind` define what the object is. `PresentationBinding`
defines how it is currently rendered and interacted with: x/y position,
width/height, z-order, focus, color, and related presentation state. A window
move, focus change, or repaint mutates presentation binding only; it does not
change object identity.

This is not Open Surface, Patch, persistent object storage, or semantic search.
It is the minimal Phase 5 data model needed so later phases do not have to
reverse-engineer object meaning from framebuffer pixels.

## Consequences

The compositor, widgets, and first-party applications must carry typed object
identity from their first slice. Tests should reject duplicate object ids and
should prove moving/focusing a window preserves the underlying object id and
kind.

Capability separation remains logical kernel-mode enforcement until Phase 8.
Do not claim hostile-code isolation for Phase 5 services.

