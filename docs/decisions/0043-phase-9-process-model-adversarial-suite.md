# ADR 0043: Phase 9 Process Model Adversarial Suite

Date: 2026-07-17

## Status

Accepted

## Context

The first Phase 9 slices proved dynamic ELF loading, a stable syscall number
space, copy-in/copy-out pointer validation, zero-default dynamic capability
grants, launch argv/environment delivery, and dynamic fault containment. The
phase exit condition requires those pieces to work together against multiple
arbitrary user binaries, not a single fixed proof path.

This ADR does not add a package format, a filesystem loader, a general storage
allocator, networking, updates, hardware expansion, or SMP.

## Decision

The ADR 0037 inner `INIT.PAK` bundle may contain multiple `TYPE_USER_ELF`
records. Existing consumers that ask for a user ELF without an ordinal receive
ordinal zero. Phase 9 completion consumes four ordinals:

1. A runnable payload that enters CPL3 and returns through an expected
   breakpoint trap.
2. The ADR 0042 invalid-instruction fault payload.
3. A bad-pointer payload that dereferences address zero and must fault as a
   user-originated page fault.
4. A direct hardware-access payload that attempts I/O port access from CPL3 and
   must fault as a user-originated general-protection fault.

PythCore validates and maps all four payloads before claiming the variants are
loaded. The final adversarial proof combines those dynamic execution results
with the existing general mechanisms:

- forged capability use is denied by the syscall-boundary capability model;
- out-of-range user pointer access is denied by the copy-in/copy-out policy and
  contained when attempted by a dynamic payload;
- direct hardware access is denied by both the syscall capability model and the
  CPU privilege boundary when attempted by a dynamic payload.

Boot-time proof markers are:

```text
PYTHOS:CORE:PROCESS_MODEL:PROGRAM_RAN
PYTHOS:CORE:PROCESS_MODEL:ELF_VARIANTS_LOADED
PYTHOS:CORE:PROCESS_MODEL:FORGED_CAPABILITY_DENIED
PYTHOS:CORE:PROCESS_MODEL:BAD_SYSCALL_POINTER_DENIED
PYTHOS:CORE:PROCESS_MODEL:HARDWARE_ACCESS_DENIED
PYTHOS:CORE:PROCESS_MODEL_ADVERSARIAL_READY
PYTHOS:CORE:PHASE_9_COMPLETE
```

## Consequences

Phase 9 is complete only when the marker chain proves that multiple dynamic ELF
variants loaded and ran or faulted under the general mechanism. Future Phase 12
package launch may reuse this process model, but it must not treat the Phase 9
inner-bundle delivery path as a package format or filesystem-backed loader.

The next phase is Phase 10 general-purpose storage. Starting it requires
explicit re-invocation.
