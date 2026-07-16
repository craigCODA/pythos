# ADR 0031: Phase 8 Guarded Shared Memory

Status: Accepted

## Context

Phase 3 introduced capability-gated shared memory while all services still ran
as cooperating kernel-mode code. Phase 8 has now added CPL3 execution, separate
user address-space roots, guarded user stacks, and service-local runtime roots.
The shared-memory contract must be rechecked under those hardware-isolation
constraints before later slices add termination, quotas, crash containment, or
the final hostile-code capability boundary.

This slice still does not implement user pointer copy-in/copy-out, process
termination, resource quotas, crash containment, or hostile-code capability
enforcement.

## Decision

PythCore binds two service identities to distinct Phase 8 user address-space
roots and reuses the Phase 3 shared-memory capability table. A read-only
capability held by the reader service can read the fixed shared region while
the writer service, running under a different root and identity, cannot use
that handle to write the region. The denied cross-space write must leave the
region bytes unchanged.

The required serial markers are:

```text
PYTHOS:CORE:SHM:RING3_READ
PYTHOS:CORE:SHM:CROSS_SPACE_WRITE_DENIED
PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY
```

## Consequences

This proves the existing shared-memory handle semantics are still meaningful
when services are associated with distinct Phase 8 user roots.

It does not yet prove user-buffer copy-in/copy-out, process termination,
address-space reclamation, quotas, crash containment, or hostile-code
capability enforcement at the syscall boundary.
