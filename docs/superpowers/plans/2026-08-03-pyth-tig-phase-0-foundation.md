# PythTIG Phase 0 Adoption and Reconciliation Plan

**Status:** Proposed architecture pending owner adoption
**Date reconciled:** 2026-08-04
**Branch:** `docs/pythtig-phase0-adoption`

## Goal

Reconcile the proposed PythTIG / Convergent Architecture program against the
live private repository before any runtime behavior changes. Phase 0 is a
documentation and guardrail pass only.

This plan supersedes the unreconciled Phase 0 instructions in the original
2026-08-03 handoff bundle. The bundle was a proposal package. The live
repository already uses ADR 0053 and ADR 0054 for unrelated accepted work, so
PythTIG adopts the next available live ADR numbers instead.

## Hard Boundary

Do not implement:

- graph verifier;
- graph runtime;
- `pythc`;
- Task Steward runtime behavior;
- native x86-64 backend;
- session-manager cutover;
- marker-contract scripts;
- CI behavior;
- production boot, syscall, ABI, package, or persistence changes.

Phase 0 may modify only documentation and `AGENTS.md` instruction files.

## Phase 0A: Live Repository Inventory

1. Verify the work happens in an isolated worktree and record:
   - branch;
   - HEAD;
   - short status;
   - recent log.
2. Identify the next available ADR numbers from `docs/decisions/`.
3. Search the live tree for the object-shell baseline:
   - `ObjectShellRequest`;
   - `BootstrapCapabilityBlock`;
   - `TYPE_NAMED_USER_ELF`;
   - `load_named_user_program`;
   - `enter_persistent_user_process`;
   - `with_object_service`;
   - `SYSCALL_OBJECT_REQUEST`.
4. Determine the current authoritative baseline for:
   - object shell;
   - evidence terminal;
   - retained object-service persistence path;
   - normal boot path.
5. Account for the unmerged `agent/physical-evidence-terminal` branch and its
   `docs/HANDOVER.md` merge conflict without resolving or discarding either
   side.

## Phase 0B: Docs-Only Adoption Branch

1. Import the reconciled proposal design under `docs/superpowers/specs/`.
2. Import the reconciled master plan and future phase plans under
   `docs/superpowers/plans/`.
3. Import the acceptance contract under `docs/pyth-tig/acceptance/`.
4. Create proposed ADR 0064 for the Pyth native typed instruction graph.
5. Create proposed ADR 0065 for the Pyth graph package ABI.
6. Merge the PythTIG guardrails into the root `AGENTS.md` without weakening
   existing PythOS rules.
7. Add scoped PythTIG `AGENTS.md` files only as future guardrails. These files
   do not authorize implementation before owner adoption.
8. Update `docs/ROADMAP.md` to list PythTIG as a proposed program, not the
   active implementation sequence.
9. Update `docs/HANDOVER.md` with the proposed adoption branch and unresolved
   baseline conflicts.
10. Produce `docs/pyth-tig/PHASE-0-RECONCILIATION-REPORT.md`.

## Preserved Acceptance Suite

Before the docs-only commit, run the current preserved acceptance commands from
the isolated worktree:

```powershell
python scripts\test-boot.py
python scripts\test-persistent-storage.py
python scripts\test-normal-fast-boot.py
python scripts\test-object-shell.py
```

The live object-shell harness currently reports task-specific terminal markers,
not a single `OBJECT_SHELL_TEST_OK` line. Phase 0 records the exact observed
terminal evidence in the reconciliation report.

## Exit Criteria

Phase 0 exits when:

- the proposal docs are imported as proposed architecture only;
- ADR 0064 and ADR 0065 exist with `Status: Proposed`;
- `docs/ROADMAP.md`, `docs/HANDOVER.md`, and `AGENTS.md` preserve the current
  accepted roadmap and behavior;
- the preserved acceptance suite has fresh command results recorded;
- a docs-only commit exists on the isolated branch;
- Phase 1 is still blocked on owner adoption.

Phase 1 may not begin until the owner explicitly accepts or revises the
architecture decisions after review.
