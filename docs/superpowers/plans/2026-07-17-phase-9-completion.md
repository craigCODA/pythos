# Phase 9 Completion Plan

## Steps

1. Add failing tests for:
   - bundle ordinal access for duplicate `TYPE_USER_ELF` records;
   - dynamic crash containment accepted as a user fault;
   - boot marker contracts for `general-fault-isolation` and
     `process-model-adversarial-suite`.
2. Preserve the existing ordinal-zero user ELF API and add ordinal lookup for
   additional dynamic ELF payloads.
3. Generate four dynamic ELF records in both ESP and ISO image builders.
4. Map guarded user stacks into dynamic ELF address spaces and validate dynamic
   entries before execution.
5. Add a Phase 9 process-model proof module that composes existing crash
   containment, syscall-boundary capability, and copy-in/copy-out proofs.
6. Wire the two new slices into `pythcore_entry`, emit exact markers, and emit
   `PYTHOS:CORE:PHASE_9_COMPLETE`.
7. Update ADRs, roadmap, TDD, AGENTS halt text, and QEMU marker harness.
8. Verify targeted unit tests, marker contracts, full unit tests, QEMU slice
   boots, ESP/ISO milestone boots, graceful no-audio fallback, and diff check.
9. Commit, push, confirm CI green, and stop at the Phase 9 -> Phase 10
   boundary.
