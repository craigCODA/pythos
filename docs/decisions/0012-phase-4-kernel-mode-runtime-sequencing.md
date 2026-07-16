# ADR 0012: Phase 4 Kernel-Mode Runtime Sequencing

Date: 2026-07-16

## Status

Accepted

## Context

Phase 4 introduces the first Python runtime and Python service machinery. Phase
8 is the point where services move behind hardware-enforced ring-3 isolation,
separate address spaces, and syscall-mediated capability checks. Those pieces do
not exist yet.

PythOS still needs the Python runtime earlier than Phase 8 so the service model,
`system.*` API surface, value validation, service manager, exception
containment, restart behavior, and async event delivery can be built and tested
against the Phase 2 scheduler and Phase 3 capability mechanisms.

This creates a deliberate sequencing tension: Phase 4 through Phase 7 trusted
services run in kernel mode, but their APIs must be shaped so they can later
cross the Phase 8 syscall and address-space boundary without being redesigned
from scratch.

## Decision

Phase 4 will prototype the Python runtime and Python services as trusted
kernel-mode services through Phase 7. This is a temporary implementation
location, not a permission model.

Every native/Python interaction introduced in Phase 4 must route through a
narrow host-provided boundary:

```text
Python service
-> validated system.* call
-> capability check
-> native service/kernel operation
```

Python code receives no ambient authority. Runtime bootstrap receives only the
capabilities explicitly required to start the first service. The runtime may not
reach raw kernel pointers, unchecked native structures, arbitrary physical
memory, direct device I/O, scheduler internals, capability-table internals, or
IPC/channel internals except through explicit host functions that validate
types, bounds, ownership, service identity, resource identity, and requested
rights.

The Phase 4 runtime-selection slice must prefer runtimes that already tolerate a
host-controlled embedding surface over runtimes that require broad ambient OS
assumptions. This criterion is both a Phase 4 requirement and a Phase 8
migration requirement.

Phase 8 must move these services behind hardware-enforced isolation:

```text
kernel-mode trusted call
-> syscall or mapped shared-memory boundary
-> per-service address space
-> service-local runtime instance
```

The Phase 4 implementation must document any place where it temporarily relies
on trusted kernel-mode execution so Phase 8 can replace that call path with
copy-in/copy-out, shared-memory mapping, or syscall validation.

## Consequences

This keeps Phase 4 implementable without waiting for ring-3 execution, while
avoiding a runtime design that assumes unchecked access to kernel internals.

The cost is explicit and deferred to Phase 8: Python runtime calls will need to
cross a syscall boundary, GUI and storage services will need separate address
spaces, IPC paths that use trusted pointers will become copy-in/copy-out or
capability-gated shared memory, service-local runtimes will replace any shared
kernel-mode runtime state, and service crashes will become process failures
instead of kernel failures.

Until Phase 8, capability enforcement for Python services is logical and
kernel-mediated, not a hostile-code security boundary. Documentation and tests
must not claim otherwise.
