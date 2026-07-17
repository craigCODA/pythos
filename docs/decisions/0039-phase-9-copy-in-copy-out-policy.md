# ADR 0039: Phase 9 Copy-In/Copy-Out Policy

Date: 2026-07-17

## Status

Accepted

## Context

ADR 0038 freezes the first PythOS syscall number space, but it intentionally
does not define user pointer arguments. Phase 8 proved that a bad pointer from
ring 3 is contained as a fault. Phase 9 needs the stronger general mechanism:
before any syscall dereferences user memory, PythCore must validate the
caller-provided pointer and length against that caller's mapped user address
space.

This ADR does not add new syscall numbers, user execution of the dynamically
loaded ELF, filesystem-backed program loading, argv/env, dynamic capability
grants, networking, packages, or SMP.

## Decision

PythCore validates user buffers as half-open ranges:

```text
[ptr, ptr + len)
```

The addition must use checked arithmetic. If `ptr + len` overflows, validation
fails before any mapping lookup or memory access.

A user buffer is valid only when:

1. `ptr` is nonzero.
2. `len` is nonzero.
3. `ptr + len` does not overflow.
4. The complete range is contained inside one mapped user region owned by the
   calling process.
5. The region grants the requested access direction: read for copy-in, write
   for copy-out.

Phase 9 deliberately rejects one syscall buffer spanning more than one mapped
region, even when both regions are user mapped. This keeps the first policy
simple and makes cross-object or cross-process boundary mistakes explicit
instead of depending on mapping order.

Validation returns distinct errors for the load-bearing denial classes:

```text
OutOfRange
LengthOverflow
CrossMapping
PermissionDenied
```

Boot-time proof markers are:

```text
PYTHOS:CORE:COPY:VALIDATED
PYTHOS:CORE:COPY:OUT_OF_RANGE_DENIED
PYTHOS:CORE:COPY:LENGTH_OVERFLOW_DENIED
PYTHOS:CORE:COPY:CROSS_MAPPING_DENIED
PYTHOS:CORE:COPY_IN_COPY_OUT_READY
```

The first implementation is a policy and proof surface. It validates ranges
and emits denial markers, but it does not perform unsafe memory copies yet.
Actual copying is allowed only after the policy is wired to concrete syscall
arguments and still must validate before dereferencing.

## Consequences

Future syscalls can accept pointer/length pairs only through this policy or a
successor ADR. A raw user pointer is never a Rust reference and is never
trusted as mapped, aligned, owned, or readable/writable by itself.

Zero-length buffers are rejected in this phase. A later syscall may define a
specific zero-length no-op contract, but it must do so explicitly rather than
inheriting one accidentally.

This completes the missing Phase 8 writeup caveat around copy-in/copy-out, but
it does not claim arbitrary third-party application support. That remains the
Phase 9 exit condition across later slices.
