# ADR 0033: Phase 8 Memory Quotas

Status: Accepted

## Context

Phase 8 has user-mode execution, address-space isolation, service-local
runtimes, guarded shared memory, and kernel-forced process termination. The
next isolation primitive is kernel-owned resource accounting. Memory use must
be charged by the kernel to a service identity and denied when it exceeds a
configured limit.

This slice does not implement CPU quotas, crash containment, or hostile-code
capability enforcement at every syscall boundary.

## Decision

PythCore introduces a fixed memory-quota table keyed by kernel-assigned service
identity. The proof registers a service, grants an in-quota page charge, denies
an over-quota charge, and verifies the denied charge does not mutate recorded
usage.

The required serial markers are:

```text
PYTHOS:CORE:QUOTA:MEMORY_GRANTED
PYTHOS:CORE:QUOTA:MEMORY_DENIED
PYTHOS:CORE:MEMORY_QUOTAS_READY
```

## Consequences

Memory accounting is now kernel-owned and tied to service identity rather than
self-reported by a runtime.

This does not yet schedule CPU budgets, convert arbitrary user faults into
service failure, or prove every capability check at the user/kernel syscall
boundary.
