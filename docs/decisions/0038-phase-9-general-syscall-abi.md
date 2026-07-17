# ADR 0038: Phase 9 General Syscall ABI

Date: 2026-07-17

## Status

Accepted

## Context

ADR 0028 defined the first PythOS x86-64 `syscall`/`sysret` gate and assigned
one syscall number, `0x5059_0001`, to the fixed `system.log` proof. Phase 8
needed that fixed proof to validate the hardware boundary. Phase 9 needs a
general-purpose process model, which means user programs need a stable syscall
number space rather than an ad hoc one-off constant.

This ADR is the ABI freeze point for syscall numbers. It does not add pointer
arguments, user buffer copying, argv/env, dynamic capability grants, filesystem
program loading, package launching, networking, updates, SMP, or arbitrary
third-party application support.

## Decision

PythCore syscall ABI versioning starts at:

```text
major = 1
minor = 0
```

Syscall numbers are 64-bit unsigned integers. The first permanent assignments
are:

```text
0x5059_0000 = SYSCALL_ABI_INFO
0x5059_0001 = SYSCALL_SYSTEM_LOG_PROOF
```

`SYSCALL_ABI_INFO` accepts no pointers and has no side effects. It returns ABI
version metadata in `RAX` because Phase 9 `copy-in-copy-out-policy` has not yet
defined safe user-buffer writes.

`SYSCALL_SYSTEM_LOG_PROOF` keeps the exact Phase 8 behavior from ADR 0028: it
runs the capability-gated IPC proof bridge and invokes the Phase 4
`system.log` surface with:

```text
PythOS [HISS] We Are Woken
```

PythCore owns a fixed syscall registry. Dispatch consults that registry before
running a handler. Unsupported syscall numbers return an explicit unsupported
result and do not fall through to any privileged bridge.

Existing syscall numbers are never reused. New syscall numbers require an ADR
update or successor ADR before code lands. Removing a syscall from an active ABI
version is not allowed; deprecation must be represented as a documented handler
result, not silent renumbering.

## Consequences

Future Phase 9 and later user binaries can depend on stable syscall numbers.
The existing Phase 8 proof remains valid because its number and behavior are
unchanged.

The ABI is still intentionally narrow. General user-pointer validation,
copy-in/copy-out, structured user buffers, and richer syscall arguments remain
the next Phase 9 slice, not this ADR.
