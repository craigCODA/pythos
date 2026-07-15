# ADR 0008: Phase 3 Task-Control Invariants

Date: 2026-07-15

## Status

Accepted

## Context

Phase 3 `service-identity` depends on Phase 2 task records without allowing a
new service to inherit authority from a previously terminated task that reused
the same scheduler slot. ADR 0007 records the scheduler and fixed task layout,
but Phase 3 needs the invariants stated explicitly before service identities and
capability grants are layered on top.

## Decision

PythCore treats scheduler slots, task ids, and service identities as separate
concepts:

```text
scheduler slot  reusable storage position
TaskId          kernel-assigned task lifetime identifier
ServiceId       Phase 3 service identity, never derived from slot alone
```

The following invariants are binding for Phase 3:

* A scheduler slot may be reused only after the prior task reaches
  `TaskState::Terminated` and its saved frame and stack pointer are no longer
  considered runnable.
* A terminated slot must not be selected by the round-robin scheduler.
* A new task in a reused slot must receive a fresh identity value; it must never
  inherit capabilities, IPC endpoints, or audit identity from the previous
  occupant.
* `TaskId` stability is per task lifetime. It is not proof of current authority
  after termination.
* Saved-frame offsets remain checked by tests before any interrupt, scheduler,
  or IPC code may depend on them.
* Kernel stack bounds remain explicit: bottom, top, current pointer, and guard
  page ownership must stay known for each runnable task.

`service-identity` may reference a task as its execution host, but it must not
use a scheduler slot index as a security identity.

## Consequences

Phase 3 can safely introduce service identities without conflating slot reuse
with authority reuse. This is still kernel-mode logical isolation, not a
hostile-code security boundary; Phase 8 is still required for hardware-backed
enforcement.
