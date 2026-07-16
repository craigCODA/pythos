# ADR 0032: Phase 8 Process Termination

Status: Accepted

## Context

Phase 8 now has CPL3 execution, distinct user address-space roots, guarded
user stacks, service-local runtime roots, and guarded shared-memory checks. The
next isolation primitive is kernel-forced process termination: a user-mode
process must not have to cooperate before the kernel can stop scheduling it and
reclaim the address-space allocation associated with it.

This slice still does not implement memory quotas, CPU quotas, crash
containment for arbitrary user faults, or hostile-code capability enforcement
at every syscall boundary.

## Decision

PythCore tracks a fixed Phase 8 process record with a task id and a user
address-space root. The process-termination proof marks that process
terminated, proves the process table no longer returns it as runnable, and
reclaims the page-table frames recorded by the terminated user address space.

The physical allocator's free-page count must increase by exactly the reclaimed
table-frame count before the address-space-reclaimed marker is emitted.

The required serial markers are:

```text
PYTHOS:CORE:PROCESS:TERMINATED
PYTHOS:CORE:PROCESS:UNSCHEDULABLE
PYTHOS:CORE:PROCESS:ADDRESS_SPACE_RECLAIMED
PYTHOS:CORE:PROCESS_TERMINATION_READY
```

## Consequences

Phase 8 now has a deterministic kernel-owned proof that a user process can be
removed from scheduling and its address-space table allocation returned to the
allocator.

This does not yet enforce per-service resource quotas, translate arbitrary
faulting user exceptions into service failure, or prove every capability check
at the user/kernel syscall boundary.
