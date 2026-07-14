# ADR 0003: Add Allocation-Free CPU Exception Diagnostics

Date: 2026-07-14

## Status

Accepted

## Context

Milestone 1 initially routed every IDT entry to a minimal panic loop. That was
enough to prove descriptor-table installation, but it is too opaque for
kernel-owned virtual memory validation and later scheduler work.

PythCore still has no heap, scheduler, or lock discipline, so exception
diagnostics must avoid allocation and blocking synchronization.

## Decision

PythCore installs per-vector stubs for CPU exception vectors 0 through 31. The
stubs normalize exceptions with and without hardware error codes into one stack
layout and call a Rust diagnostic handler that prints over COM1:

```text
PYTHOS:EXCEPTION
vector=<hex>
error_code=<hex>
rip=<hex>
cs=<hex>
rflags=<hex>
rsp=<hex>
ss=<hex>
cr2=<hex for page faults>
cr3=<hex>
PYTHOS:PANIC
```

After reporting, the handler enters the existing panic loop. The normal boot
path emits `PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY` after the IDT is installed
and before the kernel-owned page-table switch.

## Consequences

Faults should now leave actionable serial evidence instead of only a bare panic
marker. This is still not recovery, and it is not a controlled expected-fault
test harness. The old-identity-map negative VM proof should build on this by
distinguishing the intended page fault from unrelated panics or hangs.
