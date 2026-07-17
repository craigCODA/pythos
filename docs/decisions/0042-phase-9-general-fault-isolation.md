# ADR 0042: Phase 9 General Fault Isolation

Date: 2026-07-17

## Status

Accepted

## Context

Phase 8 proved crash containment with a fixed CPL3 illegal-instruction payload
owned by PythCore. Phase 9 generalizes the process model: dynamic ELF payloads
now arrive through the ADR 0037 inner `INIT.PAK` bundle and are mapped into
distinct user address spaces. The remaining question for this slice is whether
the Phase 8 containment proof still holds when the fault originates from a
dynamically loaded user ELF rather than the fixed proof page.

This ADR does not add filesystem-backed program loading, package installation,
networking, storage allocation, updates, hardware expansion, or SMP.

## Decision

PythCore carries a second `TYPE_USER_ELF` record in the inner bundle for the
fault probe. The loader validates it with the same Phase 9 ELF policy as the
normal dynamic payload, maps it into its own user address-space root, maps the
existing guarded user stack pages, validates that the entry is user-accessible
and kernel text/data remain supervisor-only, and then enters the ELF at CPL3.

The payload intentionally executes an invalid instruction. The expected result
is a user-originated fault handled by the existing user-mode trap recovery path.
After recovery, PythCore runs the general process crash-containment model
against `DynamicIllegalInstruction`, proving that only the faulting dynamic
process is terminated and an unrelated peer remains alive.

Boot-time proof markers are:

```text
PYTHOS:CORE:CRASH:USER_FAULT
PYTHOS:CORE:DYNAMIC_FAULT:ELF_LOADED
PYTHOS:CORE:DYNAMIC_FAULT:USER_FAULT
PYTHOS:CORE:DYNAMIC_FAULT:SERVICE_TERMINATED
PYTHOS:CORE:DYNAMIC_FAULT:PEER_ALIVE
PYTHOS:CORE:GENERAL_FAULT_ISOLATION_READY
```

## Consequences

Fault containment can no longer depend on the fixed Phase 8 proof payload. A
future dynamically loaded process that faults must travel through the same
containment path: user-originated trap, faulting process termination, peer
preservation, and explicit marker proof.

This slice still does not define filesystem-backed program lookup, package
metadata, process inheritance, signal delivery, or restart policy.
