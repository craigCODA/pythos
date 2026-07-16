# ADR 0030: Phase 8 Service-Local Runtime Instances

Status: Accepted

## Context

ADR 0026 proved CPL3 execution, ADR 0027 proved a distinct user CR3 root, ADR
0028 defined the first syscall ABI, and ADR 0029 added guarded user stacks. The
next Phase 8 slice must stop treating the Python runtime as one shared
interpreter instance by default. Each service-local runtime needs its own
service identity, address-space root, and mutable runtime state.

This slice still does not implement guarded shared memory, user pointer
copy-in/copy-out, process termination, quotas, crash containment, or
hostile-code capability enforcement.

## Decision

PythCore can boot more than one runtime instance from the validated Phase 4
runtime source through a shared service-identity table. Each service-local
runtime instance receives a distinct kernel-assigned service identity, a
distinct service task id, a distinct user CR3 root, and a distinct local state
slot. The proof mutates one runtime's local ready state and verifies the other
runtime's state is unchanged; cross-service state mutation is rejected.

The required serial markers are:

```text
PYTHOS:CORE:RUNTIME:LOCAL_INSTANCE
PYTHOS:CORE:RUNTIME:ADDRESS_SPACE
PYTHOS:CORE:RUNTIME:STATE_ISOLATED
PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY
```

## Consequences

This proves the runtime substrate no longer assumes one shared interpreter
state for all services by default.

It does not yet prove shared-memory mediation between service address spaces,
service termination/reclamation, resource quotas, crash containment, or
hostile-code capability enforcement at the syscall boundary.
