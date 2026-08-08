# PythCore PythTIG Loader and Host Bridge Agent Contract

**Destination:** `core/src/pyth_tig/AGENTS.md`

**Status:** Guardrail for accepted PythTIG architecture and the frozen ADR 0065
version 1 ABI. An incompatible format change requires a new accepted ADR and a
new major package version. This file does not authorize implementation before a
phase-specific re-invocation.

## Purpose

This tree owns trusted package admission, read-only package mapping, runtime bootstrap construction, typed host operation dispatch, evidence emission, and containment at the PythCore boundary.

## Rules

1. Run the shared verifier before creating an address space or entering ring 3.
2. A rejected package emits one bounded rejection marker and receives no execution authority.
3. Package bytes and bootstrap data are mapped read-only to the runtime.
4. Derive caller identity from the active process context. Never infer authority from package-declared principal data alone.
5. Bind capability imports through kernel policy and validated program identity. Never deserialize or trust runtime capability handles from persistent storage or package constants.
6. Every host operation validates caller, capability holder, resource, rights, operation, user buffers, and lengths before mutation.
7. PythCore never interprets source syntax, semantic relevance, task intent, or agent prose.
8. PythCore never exposes raw hardware access to graph instructions. Hardware is represented by bounded typed service resources.
9. Runtime fault, bad pointer, invalid syscall, budget exhaustion, wrong-holder capability, and unknown operation paths must leave PythCore and permitted peers alive.
10. Existing verification boot and normal boot behavior remain distinct and preserved.
11. New markers use `PYTHOS:PYTHTIG:` and are emitted only at the proven state transition.
12. One static retained service owner and one documented access boundary remain authoritative until a later ADR changes concurrency.
13. Universal-boot backend edits may alter discovery and service binding, never graph semantics or package format.

## Mandatory local checks

```powershell
cargo test -p pythos-core pyth_tig
python scripts\test-boot.py
python scripts\test-normal-fast-boot.py
python scripts\test-object-shell.py
```
