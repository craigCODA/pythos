# ADR 0027: Phase 8 Separate Address-Space Proof

Status: Accepted

## Context

ADR 0026 proved that PythCore can enter CPL3 and recover through a
user-originated trap while still using the current kernel address space. The
next Phase 8 slice must prove that user execution can run under a distinct CR3
root before later slices define syscall entry, user process stacks,
service-local runtimes, guarded shared memory, or hostile-code containment.

This proof must not silently define a process model or syscall ABI.

## Decision

PythCore builds a second PML4 root before the first PythCore-owned CR3 switch,
while the loader identity mappings still allow new page-table frames to be
zeroed. The root contains supervisor mappings for the kernel execution path,
the active bootstrap stack, descriptor-table storage, and COM1 code paths. It
contains user mappings only for the fixed ADR 0026 proof code page and proof
stack page.

Before the kernel root becomes active, PythCore validates that the user root is
physically distinct from the kernel root, that the proof code and stack are
user-accessible through the user root, and that kernel text and data are not
user-accessible through that root.

After the existing ADR 0026 ring-3 proof completes under the kernel root,
PythCore switches CR3 to the user root, emits:

```text
PYTHOS:CORE:ADDRESS_SPACE:CREATED
PYTHOS:CORE:ADDRESS_SPACE:ISOLATED
PYTHOS:CORE:ADDRESS_SPACE:SWITCHED
```

It then reruns the fixed CPL3 breakpoint proof under the user root, restores
the kernel root, and emits:

```text
PYTHOS:CORE:ADDRESS_SPACE:RESTORED
PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY
```

## Consequences

This proves a distinct user CR3 root can run the current CPL3 proof and return
to the kernel root without losing the existing boot path.

It does not implement syscall entry, user process stacks, service-local
runtimes, guarded shared memory, process termination, quotas, crash
containment, or capability enforcement at a hostile-code boundary.
