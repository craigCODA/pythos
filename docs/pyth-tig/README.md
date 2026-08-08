# PythTIG / Convergent Architecture

Status: ADR 0064 accepted as architecture direction; ADR 0065 accepted and its
tested PythTIG version 1 ABI frozen on 2026-08-08.

This directory contains the architecture, frozen Phase 1 package/verifier
contract, and bounded Phase 2 ring-3 runtime evidence for the Pyth Native Typed
Instruction Graph program. Phase 2 is an explicit `pythtig-phase2-test` plus
`--with-pythtig` proof path; it is not the default production boot path and does
not authorize compiler, Task Steward, native backend, or production cutover
work.

Authoritative PythTIG documents:

- `docs/superpowers/specs/2026-08-03-pyth-typed-instruction-graph-design.md`
- `docs/superpowers/plans/2026-08-03-pyth-typed-instruction-graph-master-plan.md`
- `docs/superpowers/plans/2026-08-03-pyth-tig-phase-0-foundation.md`
- `docs/decisions/0064-pyth-native-typed-instruction-graph.md`
- `docs/decisions/0065-pyth-graph-package-abi.md`
- `docs/pyth-tig/acceptance/marker-contract.md`
- `docs/pyth-tig/acceptance/test-matrix.md`
- `docs/pyth-tig/acceptance/definition-of-done.md`
- `docs/pyth-tig/PHASE-0-RECONCILIATION-REPORT.md`

The default image and ISO remain on the existing object-shell path. An
incompatible change to ADR 0065's frozen version 1 package bytes requires a new
accepted ADR and a new major package version. Phase 3 and later behavior remain
pending explicit owner invocation.
