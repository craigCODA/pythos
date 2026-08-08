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

Increase each static usable user-stack extent from one page to four pages. This
is bounded headroom for the existing ring-3 debug acceptance profile, not a
dynamic stack allocator and not a new process model.

## Consequences

PythTIG Phase 3 object graphs can run through the existing retained object
service without tripping the guard during normal decoder/interpreter setup.

The guard-page behavior remains intact: stack overflow still faults into the
existing user-fault containment path. No marker strings, syscall numbers,
object-service ABI fields, PythTIG v1 package bytes, capability values, or
persistent object formats change.

Future dynamic stack allocation, stack reclamation, per-process stack sizing, or
scheduler-managed stack ownership remains outside this ADR and requires a later
authorized phase.
