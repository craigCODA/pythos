# `pythc` Compiler Agent Contract

**Destination:** `tools/pythc/AGENTS.md`

**Status:** Guardrail for accepted PythTIG architecture. ADR 0065 remains
provisional until Phase 1 evidence supports an owner format freeze. This file
does not authorize implementation before a phase-specific re-invocation.

## Purpose

This tree owns the host-side Pyth source lexer, parser, semantic checker, graph builder, canonical package encoder, diagnostics, and later x86-64 lowering tools.

## Rules

1. `pythc` is a host development tool. No compiler or parser logic is added to PythCore.
2. The compiler accepts only the grammar and types recorded in the current design/ADR.
3. Source typing, graph typing, and shared verifier signatures must agree exactly.
4. Compile output must pass the shared verifier before it is written as a successful package.
5. Preserve deterministic output: same source, compiler version, and flags produce byte-identical package bytes.
6. Never assign new type/opcode numbers locally. ABI additions require the shared-tree change and ADR first.
7. Lower effectful expressions into one explicit effect-token chain.
8. Lower loops only with explicit source budgets and runtime node-budget enforcement.
9. No implicit capability acquisition. Imports must be declared and emitted as capability-import records.
10. Diagnostics identify source span, error category, and stable reason without leaking host paths into canonical package content.
11. Native x86-64 lowering consumes only verified graph structures and must match interpreter semantics.
12. Generated ELF segments may not be writable and executable simultaneously.
13. Golden package tests compare canonical bytes; negative tests prove type, effect, capability, control-flow, and budget errors.

## Mandatory local checks

```powershell
cargo test -p pythc
python scripts\test-pythc.py
python scripts\test-pyth-native-codegen.py
```
