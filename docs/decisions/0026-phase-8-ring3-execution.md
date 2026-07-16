# ADR 0026: Phase 8 Ring-3 Execution Proof

Status: Accepted

## Context

Phase 8 begins the migration from cooperative, kernel-mode service isolation to
hardware-enforced isolation. The first slice must prove that PythCore can enter
user mode and return to the kernel through CPU privilege mechanics before later
slices add separate address spaces, a syscall ABI, user stacks per process, or
hostile-code containment.

The proof must not silently define the syscall ABI. That ABI is explicitly
reserved for the later `syscall-entry` slice.

## Decision

PythCore adds DPL3 user code and data descriptors to the GDT after the TSS
descriptor:

```text
USER_DATA_SELECTOR = 0x2B
USER_CODE_SELECTOR = 0x33
```

The TSS now exposes `RSP0` so a CPL3 trap can switch onto a kernel-owned stack.
The IDT breakpoint gate is callable from DPL3 for the proof path. PythCore maps
one fixed user-executable proof page and one fixed user-writable stack page in
the current address space, including the required user bit on intermediate page
table entries. The proof page contains a single `INT3` followed by `HLT` as a
failure stop.

The self-test enters CPL3 with `iretq`, executes the user `INT3`, verifies the
trap frame came from the expected user code and stack selectors, emits
`PYTHOS:CORE:USER_MODE:RETURN`, restores the saved kernel stack, and resumes
the boot path. Successful completion emits:

```text
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:RING3_EXECUTION_READY
```

## Consequences

This proves hardware privilege transition and kernel recovery from a user-origin
trap in the existing shared address space.

It does not implement separate address spaces, a stable syscall ABI, user-mode
service runtimes, guarded user stacks, quotas, process termination,
crash-containment, or full hostile-code isolation. Those remain locked to their
later Phase 8 slices.
