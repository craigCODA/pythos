# ADR 0067: PythTIG Phase 3 Guarded Stack Headroom

Status: Accepted

## Context

ADR 0029 reserved fixed guarded user-stack slots with one unmapped guard page
below one usable stack page. That was sufficient for the early CPL3 proof,
object shell entry, and PythTIG Phase 2 graph runtime acceptance fixtures.

PythTIG Phase 3 adds bounded object-service request construction and host-result
handling inside the existing ring-3 Pyth runtime. The Phase 3 object-flow
fixture exercises the shared ADR 0065 package decoder and interpreter in a
debug QEMU acceptance build. With one usable page, the runtime reaches the guard
page during package decode before it can issue the first object-service syscall.
The resulting fault is contained, but it prevents the authorized Phase 3 object
capability proof from running.

## Decision

Keep the ADR 0029 guarded-stack authority and permission contract:

* stack slots remain fixed and page-aligned;
* each slot keeps one unmapped user guard page immediately below the usable
  stack extent;
* only usable stack pages are mapped with user, present, writable, and
  non-executable permissions;
* guard pages remain supervisor-only and are validated by page-table
  inspection.

PythTIG Phase 3 increased each static usable user-stack extent from one page to
four pages. Four pages remain the baseline for default boot, normal-session,
the secure denied profile, and all other profiles that do not opt into the
secure transport proof.

The later accepted Phase 14 finite TLS proof selects a feature-specific static
usable extent of 16 pages (64 KiB) only when `secure-transport-probe` is active.
The tamper profile inherits that granted feature; the denied profile does not.
Both sizes retain the same Phase 8 guard-page contract. The larger extent is
bounded proof headroom, not a new baseline, dynamic stack allocator, or process
model.

## Consequences

PythTIG Phase 3 object graphs retain the four-page baseline, while the finite
Phase 14 TLS granted/tamper proof can use its isolated 16-page headroom without
tripping the guard during certificate processing.

The guard-page behavior remains intact: stack overflow still faults into the
existing user-fault containment path. No marker strings, syscall numbers,
object-service ABI fields, PythTIG v1 package bytes, capability values, or
persistent object formats change.

Future dynamic stack allocation, stack reclamation, per-process stack sizing, or
scheduler-managed stack ownership remains outside this ADR and requires a later
authorized phase.
