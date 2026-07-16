# ADR 0029: Phase 8 Guarded User Stacks

Status: Accepted

## Context

ADR 0026 proved fixed CPL3 execution, ADR 0027 moved that proof under a
distinct user CR3 root, and ADR 0028 defined the first syscall ABI. The next
Phase 8 slice needs guarded user-mode stack storage that is separate from the
Phase 2 kernel stacks and from the single ADR 0026 proof page.

This slice still does not implement dynamic user processes, user pointer
copy-in/copy-out, service-local Python runtimes, guarded shared memory, process
termination, quotas, crash containment, or hostile-code capability enforcement.

## Decision

PythCore reserves a fixed page-aligned user-stack pool. Each stack slot has one
unmapped user guard page immediately below one usable user stack page. The
kernel maps the usable pages into the ADR 0027 user CR3 root with user,
present, writable, and non-executable permissions. The guard pages remain
supervisor-only and are validated through page-table inspection before the
slice is accepted.

The existing CPL3 proof is migrated to use the first reserved stack slot. The
proof still enters through the ADR 0028 syscall gate and returns through the
already verified user breakpoint path. Required serial markers are:

```text
PYTHOS:CORE:USER_STACK:ALLOCATED
PYTHOS:CORE:USER_STACK:GUARD_PAGE
PYTHOS:CORE:USER_STACKS_READY
```

## Consequences

This establishes a reusable guarded-stack layout for later Phase 8 runtime and
service-isolation work without defining a dynamic process model.

Future slices may allocate stacks to service-local runtimes or terminate and
reclaim them, but changes to the guard-page layout or stack permission contract
require an ADR update or a new ADR before implementation.
