# Shared PythTIG ABI and Verifier Agent Contract

**Destination:** `shared/src/pyth_tig/AGENTS.md`

**Status:** Guardrail for accepted PythTIG architecture. ADR 0065 remains
provisional until Phase 1 evidence supports an owner format freeze. This file
does not authorize implementation before a phase-specific re-invocation.

## Purpose

This tree owns architecture-independent PythTIG package definitions, canonical encoding, stable type/opcode identities, verifier error identities, and verification logic shared by host tools and PythCore.

## Rules

1. No `std` dependency. The module must remain usable from `no_std` PythCore and user crates.
2. Public record sizes, field order, numeric type IDs, opcode IDs, and verifier error codes are ABI.
3. All integers are little-endian on disk. Host architecture layout is never used as an implicit encoder.
4. Reject unknown major versions. Minor-version compatibility must be explicit and tested in both directions.
5. Reserved fields must be zero unless an accepted ADR assigns them.
6. Validate bounds and section arithmetic with checked operations before slicing bytes.
7. Validate section non-overlap, record counts, block ownership, terminators, dominance, typing, effects, capabilities, budgets, canonical ordering, and checksum.
8. Capability values may originate only from declared imports or capability-returning host operations.
9. A verifier error must be deterministic for the same invalid package.
10. The encoder must produce one canonical byte representation for one graph.
11. Never add an opcode only to a compiler or runtime. Update the shared signature table, verifier, encoder/decoder, fixtures, ADR, design spec, and negative tests together.
12. No hardware type, controller ID, MMIO address, port number, page-table concept, ELF relocation, or machine register belongs in this tree.
13. Every bug fix begins with a package fixture or mutation that proves the previous verifier accepted or misclassified invalid data.

## Mandatory local checks

```powershell
cargo test -p pythos-shared pyth_tig
cargo fmt --all -- --check
cargo clippy -p pythos-shared --all-targets -- -D warnings
```
