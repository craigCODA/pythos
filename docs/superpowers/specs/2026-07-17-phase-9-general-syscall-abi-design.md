# Phase 9 General Syscall ABI Design

## Goal

Generalize the Phase 8 fixed syscall proof into a stable, versioned syscall
number space that future user binaries can depend on without breaking existing
Phase 8 behavior.

## Architecture

ADR 0028 defined the first hardware syscall gate and one fixed proof syscall:
`0x5059_0001 = SYSCALL_SYSTEM_LOG_PROOF`. This slice keeps that number and
behavior permanent, but stops treating it as a lone ad hoc constant. PythCore
adds a small syscall ABI registry that records ABI version metadata, permanent
number assignments, dispatch kind, and rejection behavior for unknown numbers.

The x86-64 register ABI remains unchanged for this slice:

```text
RAX on entry  = syscall number
RAX on return = result code
```

No user pointers, user buffers, argv/env data, dynamically granted process
capabilities, loaded-ELF execution, filesystem program loading, networking,
packaging, updates, or SMP are added here. Copy-in/copy-out is the next Phase 9
slice and remains out of scope.

## Required ADR

ADR 0038 records the syscall number space and versioning policy. It amends ADR
0028 without renumbering it:

* ABI major version starts at `1`.
* ABI minor version starts at `0`.
* Existing number `0x5059_0001` remains `SYSCALL_SYSTEM_LOG_PROOF` permanently.
* New number `0x5059_0000` is reserved as `SYSCALL_ABI_INFO`.
* Existing syscall numbers are never reused.
* New syscall numbers require an ADR update or successor ADR before code lands.
* Unsupported numbers return an explicit unsupported-number result and do not
  run any privileged bridge.

## ABI Registry

The registry is a fixed, allocation-free table in PythCore. Each entry records:

```text
number
name
introduced_major
introduced_minor
dispatch_kind
```

The initial registry contains exactly:

```text
0x5059_0000  SYSCALL_ABI_INFO           returns ABI version metadata
0x5059_0001  SYSCALL_SYSTEM_LOG_PROOF   runs the existing Phase 8 proof bridge
```

Registry validation checks that the table is nonempty, sorted by syscall number,
and has no duplicate numbers. The dispatch path consults this table before
running a handler. An unknown number is a real denial case, not a fall-through.

`SYSCALL_ABI_INFO` is side-effect-free and accepts no pointers. Because the
Phase 9 copy-in/copy-out policy does not exist yet, it returns a packed integer
through `RAX` rather than writing to a user-supplied structure.

## Slice Proof

The positive path proves:

1. ABI major/minor metadata exists and is self-consistent.
2. The registry validates as sorted and duplicate-free.
3. `SYSCALL_ABI_INFO` dispatches through the registry and returns version
   metadata.
4. `SYSCALL_SYSTEM_LOG_PROOF` still dispatches through the registry and preserves
   the existing Phase 8 capability-gated IPC plus `system.log` proof.

The negative path proves:

1. An unsupported syscall number is rejected by the general registry path.
2. The rejection does not execute the privileged system-log proof bridge.

Required markers:

```text
PYTHOS:CORE:SYSCALL_ABI:VERSIONED
PYTHOS:CORE:SYSCALL_ABI:KNOWN_DISPATCH
PYTHOS:CORE:SYSCALL_ABI:UNKNOWN_DENIED
PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY
```

These markers must appear after `PYTHOS:CORE:DYNAMIC_ELF_LOADING_READY` and
before `PYTHOS:CORE:FRAMEBUFFER_READY`.

## Test Strategy

Host tests cover the pure ABI pieces first:

* ABI version constants are `1.0`.
* ABI-info packing is stable.
* The syscall table contains no duplicate numbers.
* `0x5059_0001` remains the system-log proof number.
* Known dispatch of `SYSCALL_ABI_INFO` succeeds without side effects.
* Unknown dispatch returns `SyscallError::UnsupportedNumber`.
* The existing system-log proof dispatch still succeeds when expected.

QEMU tests add a `general-syscall-abi` slice and extend `milestone-1` marker
ordering. The dynamic ELF slice remains the predecessor, and the copy-in/copy-
out slice is not started.

## Scope Boundary

Do not accept user pointers. Do not copy user buffers. Do not execute the
dynamically loaded ELF. Do not introduce argv/env. Do not grant dynamic
capabilities. Do not add package loading, networking, updates, hardware
expansion, SMP, semantic indexing, local AI, or vision-layer behavior.

## Phase Boundary

This slice is complete only when ADR 0038 is accepted, host tests prove known
and unknown syscall dispatch through the registry, QEMU emits the new markers in
order, and the repository halts at the Phase 9 `general-syscall-abi` ->
`copy-in-copy-out-policy` boundary.
