# ADR 0036: Phase 8 Capability Boundary Enforcement

Status: Accepted

## Context

Phase 8 now has ring-3 execution, distinct user address spaces, a syscall gate,
guarded user stacks, service-local runtime roots, guarded shared memory,
process termination, quotas, and crash containment. The remaining Phase 8 risk
is authority crossing the syscall boundary: a hostile user-mode service must not
be able to turn knowledge of a resource, a copied handle value, or a direct
hardware-style request into privileged action.

ADR 0028 defined the syscall ABI but explicitly did not claim hostile-code
capability enforcement. This ADR closes that gap for the fixed Phase 8 proof
surface.

## Decision

PythCore adds a final Phase 8 boundary proof around the syscall gate.
Privileged IPC sends through the boundary validate the Phase 3 capability table
before enqueueing a message. The proof first sends through a valid holder,
handle, resource, and `SEND` right, then verifies the queued message round
trips.

The proof then attempts to reuse that handle value from a different service
identity. Validation fails with `WrongHolder` before the IPC channel can mutate.
It also attempts to repurpose the same handle for a fixed hardware-port
resource. Validation fails with `WrongResource` before any privileged action.

The final adversarial proof also runs a fixed CPL3 bad-pointer probe by reading
address zero under the user root. The existing user-fault recovery path contains
the page fault, returns to the kernel, and the process proof terminates only the
faulting service while preserving a peer.

The boot proof emits:

```text
PYTHOS:CORE:BOUNDARY:BAD_POINTER_CONTAINED
PYTHOS:CORE:BOUNDARY:CAPABILITY_ALLOWED
PYTHOS:CORE:BOUNDARY:FORGERY_DENIED
PYTHOS:CORE:BOUNDARY:HARDWARE_DENIED
PYTHOS:CORE:CAPABILITY_BOUNDARY_READY
```

These markers occur after `PYTHOS:CORE:CRASH_CONTAINMENT_READY` and before
`PYTHOS:CORE:FRAMEBUFFER_READY`.

## Consequences

Phase 8 now proves that the current hostile-code boundary is hardware-backed for
the fixed proof surface: a user fault is contained, copied capability handles do
not authorize the wrong service, and hardware-style resource access cannot be
obtained by repurposing a different handle.

This is still not a general-purpose userspace ABI. The proof intentionally does
not implement dynamic user process creation, user pointer copy-in/copy-out,
networking, package management, SMP, or broad hardware expansion.
