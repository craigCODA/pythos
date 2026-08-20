# PythTIG / Convergent Architecture

Status: ADR 0064 accepted as architecture direction; ADR 0065 accepted and its
tested PythTIG version 1 ABI frozen on 2026-08-08. PythTIG Phase 1 through
Phase 7 are implemented on the Phase 7 cutover/cross-target branch.

This directory contains the architecture, frozen package/verifier contract,
ring-3 runtime evidence, retained object/capability evidence, host compiler
evidence, Task Steward authority-boundary evidence, native backend evidence,
normal-boot cutover evidence, cross-target comparison records, and
physical-target evidence procedures for the Pyth Native Typed Instruction
Graph program.

Phase 7 changes normal boot to launch the Pyth-native service composition by
default. The Rust object shell remains available through the `legacy-shell`
maintenance fallback and as the recovery shell after a contained default-service
fault. PythCore still supplies typed services and capabilities only; it does
not parse Pyth source, human command text, semantic prompts, or agent policy.

Authoritative PythTIG documents:

- `docs/superpowers/specs/2026-08-03-pyth-typed-instruction-graph-design.md`
- `docs/superpowers/plans/2026-08-03-pyth-typed-instruction-graph-master-plan.md`
- `docs/superpowers/plans/2026-08-03-pyth-tig-phase-0-foundation.md`
- `docs/decisions/0064-pyth-native-typed-instruction-graph.md`
- `docs/decisions/0065-pyth-graph-package-abi.md`
- `docs/pyth-tig/ARCHITECTURE.md`
- `docs/pyth-tig/ACCEPTANCE.md`
- `docs/pyth-tig/acceptance/marker-contract.md`
- `docs/pyth-tig/acceptance/test-matrix.md`
- `docs/pyth-tig/acceptance/definition-of-done.md`
- `docs/pyth-tig/CROSS-TARGET-MATRIX.md`
- `docs/pyth-tig/PHYSICAL-EVIDENCE-PROCEDURE.md`
- `docs/pyth-tig/PHASE-0-RECONCILIATION-REPORT.md`

An incompatible change to ADR 0065's frozen version 1 package bytes requires a
new accepted ADR and a new major package version. Hardware claims remain
target-specific. Phase 7 acceptance does not claim generic hardware support,
networking, package management, updates, SMP, CPython compatibility,
self-hosting, cryptographic signing, or AI inside the trusted core.

Halt at the Phase 7 boundary. Do not begin the next PythTIG phase without
explicit owner re-invocation.
