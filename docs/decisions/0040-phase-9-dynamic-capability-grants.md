# ADR 0040: Phase 9 Dynamic Capability Grants

Date: 2026-07-17

## Status

Accepted

## Context

Phase 3 introduced kernel-owned capability handles for fixed service identities.
Phase 8 proved that the syscall boundary enforces those handles against fixed
adversarial cases. Phase 9 now needs the same model to apply to a process that
is created dynamically, rather than to only the fixed tasks and services that
earlier proofs constructed internally.

ADR 0038 freezes the syscall number space and ADR 0039 defines user pointer
validation. This ADR does not add new syscall numbers, execute the dynamically
loaded ELF, add argv/env, load programs from a filesystem, install packages,
add networking, or add SMP.

## Decision

A newly created process has an empty process-local capability inventory by
default. Knowing a resource identifier, operation, or copied handle value does
not authorize use unless the process inventory contains the handle and the
kernel capability table validates that handle for the process service identity.

Initial capabilities are granted only from an explicit creator-supplied grant
policy. For the first implementation, the creator is represented by the kernel
test harness/service-manager role, and the policy is a fixed bounded list of
resource/right pairs passed at process creation time. The process model does
not contain ambient default grants and does not special-case a task id, ELF
payload, or syscall path.

The kernel capability table remains the authority for holder, resource, rights,
revocation, and generation checks. The process-local inventory is only the
dynamic process's owned handle list. A process must pass both checks:

1. Its inventory must contain a handle for the requested resource and rights.
2. The capability table must validate that handle for the process service id.

Boot-time proof markers are:

```text
PYTHOS:CORE:DYNAMIC_CAPABILITY:PROCESS_CREATED
PYTHOS:CORE:DYNAMIC_CAPABILITY:ZERO_DEFAULT
PYTHOS:CORE:DYNAMIC_CAPABILITY:NO_GRANT_DENIED
PYTHOS:CORE:DYNAMIC_CAPABILITY:GRANT
PYTHOS:CORE:DYNAMIC_CAPABILITY:USE
PYTHOS:CORE:DYNAMIC_CAPABILITY_GRANTS_READY
```

## Consequences

Future process creation paths must choose initial capabilities explicitly. A
process that starts from an empty grant set cannot reach a privileged resource
by guessing identifiers, and a process with a grant still depends on the
kernel-owned table for holder/resource/right validation.

This slice generalizes the Phase 3 capability model to dynamically created
process records. It does not yet define argv/env delivery, process launch from
installed packages, filesystem-backed executable loading, or arbitrary
third-party application support.
