# ADR 0034: Phase 8 CPU Quotas

Status: Accepted

## Context

Phase 8 moves service isolation from cooperative kernel-mode proofs toward
hardware-enforced user-mode boundaries. After ADR 0033 added kernel-owned
memory quota accounting, services also need CPU budget accounting that is owned
by PythCore rather than self-reported by runtime code.

This slice is still a bounded proof path. It does not implement a dynamic
scheduler policy, wall-clock fairness, crash containment, or full hostile-code
capability enforcement.

## Decision

PythCore records CPU budget usage in a fixed kernel-owned quota table keyed by
`ServiceId`. A service must be registered before ticks can be charged to it.
Charging an in-budget tick mutates only that service's used-tick count.
Charging beyond the configured tick budget returns an explicit
`CpuQuotaExceeded` error and leaves the recorded usage unchanged.

The boot proof emits:

```text
PYTHOS:CORE:QUOTA:CPU_TICK
PYTHOS:CORE:QUOTA:CPU_THROTTLED
PYTHOS:CORE:CPU_QUOTAS_READY
```

These markers occur after `PYTHOS:CORE:MEMORY_QUOTAS_READY` and before
`PYTHOS:CORE:FRAMEBUFFER_READY`.

## Consequences

CPU budget enforcement is now a kernel-owned Phase 8 proof keyed by service
identity. Later scheduler integration can build on this accounting, but changes
to the accounting semantics should be recorded explicitly.

This does not claim complete denial-of-service resistance. It only proves that
PythCore, not the service runtime, owns the tick counter and rejects an
over-budget charge without silently corrupting the quota state.
