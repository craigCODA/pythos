# Phase 9 Completion Design

## Goal

Complete Phase 9 by proving the general-purpose process model against dynamic
user ELF execution, dynamic fault containment, and adversarial boundary cases.

## Scope

Implement only:

- `general-fault-isolation`
- `process-model-adversarial-suite`

Do not start Phase 10 storage allocation, package management, networking,
updates, hardware expansion, AI, or SMP.

## Design

Extend ADR 0037's inner `INIT.PAK` bundle by using multiple existing
`TYPE_USER_ELF` records addressed by ordinal:

1. Runnable breakpoint-return payload.
2. Invalid-instruction fault payload.
3. Bad-pointer payload.
4. Direct hardware I/O payload.

The existing non-ordinal user-ELF API remains ordinal zero, preserving current
behavior and avoiding a new record type.

Dynamic ELF address spaces must include guarded user stack pages and must
validate that the dynamic entry is user-accessible while kernel text/data stay
supervisor-only.

## Required Proofs

- A dynamically loaded invalid-instruction payload faults through the existing
  user fault path and terminates only its own service process.
- Multiple dynamic ELF variants are validated and mapped before
  `PROCESS_MODEL:ELF_VARIANTS_LOADED`.
- A runnable dynamic payload returns through the normal user-mode trap path.
- Forged capability, bad pointer, and direct hardware access attempts are
  denied by the general mechanisms, not special-cased fixed payloads.

## Completion Boundary

Emit `PYTHOS:CORE:PHASE_9_COMPLETE`, then halt at the Phase 9 -> Phase 10
boundary. Phase 10 `block-allocator` requires explicit re-invocation.
