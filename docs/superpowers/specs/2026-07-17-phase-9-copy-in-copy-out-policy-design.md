# Phase 9 Copy-In/Copy-Out Policy Design

## Goal

Add the Phase 9 `copy-in-copy-out-policy` slice: a tested, reusable policy for
validating user-supplied pointer and length pairs before syscall boundary code
may read from or write to user memory.

## Scope

In scope:

- Define ADR 0039 for the validation policy.
- Add an allocation-free `core::user_copy` policy module.
- Prove valid range acceptance and specific denial classes: out-of-range
  pointer, length overflow, cross-mapping range, and permission mismatch.
- Emit serial proof markers after `GENERAL_SYSCALL_ABI_READY` and before
  `FRAMEBUFFER_READY`.
- Update QEMU marker contracts and active-milestone docs.

Out of scope:

- No new syscall numbers.
- No actual unsafe copy routines.
- No dynamic capability grants.
- No argv/env.
- No dynamically loaded ELF execution.
- No filesystem-backed program loading.

## Policy

The caller supplies `(ptr, len, access)`.

Validation computes the half-open range `[ptr, ptr + len)` using checked
arithmetic. It succeeds only when the complete range fits inside exactly one
mapped user region and that region grants the requested access.

The first policy rejects ranges that cross from one mapped region into another.
That makes the process/object boundary explicit and prevents a syscall from
accidentally accepting a buffer that starts in one valid object and ends in a
different one.

## Proof Markers

```text
PYTHOS:CORE:COPY:VALIDATED
PYTHOS:CORE:COPY:OUT_OF_RANGE_DENIED
PYTHOS:CORE:COPY:LENGTH_OVERFLOW_DENIED
PYTHOS:CORE:COPY:CROSS_MAPPING_DENIED
PYTHOS:CORE:COPY_IN_COPY_OUT_READY
```

## Test Plan

- Rust unit tests for `core::user_copy` cover valid ranges, out-of-range,
  overflow, cross-mapping, read/write permission checks, and the self-test
  proof struct.
- Python marker-contract tests prove the new slice extends
  `general-syscall-abi` before `FRAMEBUFFER_READY`.
- QEMU acceptance runs the new slice, milestone ESP boot, milestone ISO boot,
  and no-audio fallback.
