# Ring-3 Pyth Runtime Agent Contract

**Destination:** `user/pyth-runtime/AGENTS.md`

**Status:** Proposed guardrail pending owner adoption of ADR 0064 and ADR 0065.
This file does not authorize implementation before a phase-specific
re-invocation.

## Purpose

This tree owns the reference PythTIG interpreter and its user-side syscall wrappers. The interpreter defines version-1 execution semantics used by the native differential suite.

## Rules

1. The runtime executes only a package already validated by the shared verifier.
2. Treat package and bootstrap mappings as immutable.
3. Use fixed-size, bounded tables for values, blocks, and execution state.
4. Check the node budget before every node dispatch.
5. Never cast integer graph values to pointers or capability handles.
6. Capability imports are copied only from the validated bootstrap block.
7. Effectful operations execute in verified effect-token order.
8. Host operations go through typed syscall wrappers; no port I/O, MMIO, privileged instruction, or direct object-store access is allowed.
9. Unknown opcodes, impossible types, malformed block transitions, and bootstrap mismatch terminate with a typed runtime exit or contained user fault. They never continue speculatively.
10. Keep interpreter semantics simple and explicit. Optimizations that obscure reference behavior belong in later backends, not here.
11. Every unsafe syscall wrapper carries the complete invariant required by the root contract.
12. Interpreter output is typed `GraphExitRecord` state plus bounded evidence markers, not formatted kernel prose.

## Mandatory local checks

```powershell
cargo test -p pythos-user-pyth-runtime
python scripts\build-user-pyth-runtime.py
python scripts\verify-user-elf.py --elf target\x86_64-unknown-none\debug\pythos-user-pyth-runtime
python scripts\test-pyth-graph-runtime.py
```
