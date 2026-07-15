# ADR 0011: IPC Channel Bootstrap

Date: 2026-07-15

## Status

Accepted

## Context

Phase 3 `ipc-channels` introduces the first communication primitive between
known service identities. The slice comes before `bounded-queues`,
`capability-handles`, and `permission-validation`, so it must not silently
prebuild those later mechanisms.

## Decision

The first IPC channel is a trusted kernel-internal bootstrap primitive:

```text
owner ServiceId
peer  ServiceId
fixed queue depth
fixed maximum message length
typed message header
payload copied into kernel-owned channel storage
```

Channel creation is not capability-gated in this slice. It is only used by
PythCore self-tests with known service identities. The later
`capability-handles` and `permission-validation` slices must add the authority
checks before IPC creation or send/receive becomes a service-facing operation.

Messages are copied inside kernel memory. Phase 8 will revisit copy-in/copy-out
when services move behind ring-3 address spaces. This slice deliberately does
not add syscall, user-pointer, shared-memory, heap-backed queue, or dynamic
growth machinery.

Queue depth and maximum message size are fixed constants. If the fixed storage
is full, the primitive reports an internal `QueueFull` error, but the explicit
blocking-vs-error backpressure contract remains the next `bounded-queues` slice.

## Consequences

The proof for this slice can focus on identity-addressed delivery and payload
integrity: a message sent from one known service identity is received by the
other identity with the same type, length, bytes, and checksum. Authority
enforcement is intentionally not claimed until later Phase 3 slices.
