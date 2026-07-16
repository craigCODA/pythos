# ADR 0035: Phase 8 Crash Containment

Status: Accepted

## Context

Phase 8 now has ring-3 entry, separate user address spaces, a syscall gate,
guarded user stacks, service-local runtime roots, guarded shared memory,
process termination, and kernel-owned memory/CPU quotas. The next isolation
proof must show that a user-mode crash is diagnosed and contained without
becoming a kernel panic or taking down unrelated services.

This slice is still not the final hostile-code capability boundary. Capability
forgery and every Phase 3 check moving to the syscall gate remain the next
slice.

## Decision

PythCore adds a fixed CPL3 illegal-instruction probe using `ud2`. The exception
path recognizes the expected user-mode invalid-opcode fault by vector and
ring-3 `CS`/`SS`, records `PYTHOS:CORE:CRASH:USER_FAULT`, and returns to the
kernel recovery path instead of entering the kernel panic path.

After the fault returns to the kernel, PythCore runs a fixed process-table
proof: the faulting service process is terminated, a peer service process
remains runnable, and the kernel continues to the normal milestone completion
path.

The boot proof emits:

```text
PYTHOS:CORE:CRASH:USER_FAULT
PYTHOS:CORE:CRASH:SERVICE_TERMINATED
PYTHOS:CORE:CRASH:PEER_ALIVE
PYTHOS:CORE:CRASH_CONTAINMENT_READY
```

These markers occur after `PYTHOS:CORE:CPU_QUOTAS_READY` and before
`PYTHOS:CORE:FRAMEBUFFER_READY`.

## Consequences

PythCore now proves a user-mode fault can be contained as a service crash
rather than becoming a kernel crash. The proof is intentionally bounded to a
fixed illegal-instruction probe and fixed process-table state.

This does not yet prove capability-forgery resistance at the syscall boundary.
That is reserved for `capability-enforcement-at-boundary`.
