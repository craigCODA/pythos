# ADR 0065: Pyth Graph Package ABI

Date: 2026-08-04
Status: Proposed

## Context

The host compiler, PythCore loader, ring-3 interpreter, native code generator,
and verification tools require one stable canonical package contract before any
PythTIG behavior can be implemented.

The original PythTIG handoff proposed ADR 0054 for this decision. The live
repository already uses ADR 0054 for the polling AHCI block backend, so this
proposal is renumbered to ADR 0065.

## Decision

PythTIG version 1 packages are canonical, little-endian byte streams with:

- magic `PYTHTIG1`;
- major version `1`;
- explicit minor version;
- package and principal identifiers;
- entry block;
- type, block, node, capability-import, constant-pool, and string-table
  sections;
- a deterministic 64-bit integrity checksum with the checksum field treated as
  zero.

The proposed package bounds are:

```text
maximum package bytes       131072
maximum graph nodes         1024
maximum blocks              128
maximum capability imports  32
maximum constant pool bytes 65536
maximum string table bytes  16384
maximum runtime values      1024
maximum executed nodes      65536 per invocation
```

The proposed stable primitive type set is:

```text
0x0000 Unit
0x0001 Bool
0x0002 U64
0x0003 I64
0x0004 Bytes
0x0005 Utf8
0x0006 ObjectId
0x0007 RevisionId
0x0008 TaskId
0x0009 ProposalId
0x000A Capability
0x000B Effect
0x000C ErrorCode
```

The proposed opcode families cover structural nodes, constants, pure
arithmetic/boolean operations, control terminators, and typed host operations
for system logging, object operations, task/proposal operations, graph
relationship queries, relevance assertions, capability requests, and later
command input/result operations.

The shared verifier must reject invalid packages before ring-3 entry. Its
proposed pass order is:

1. Header, version, reserved fields, offsets, alignment, and total-length
   validation.
2. Section non-overlap and count/size validation.
3. Known type and opcode validation.
4. Block ownership and exactly-one-terminator validation.
5. Control target and block-argument count validation.
6. Dominance and value-availability validation.
7. Opcode-specific type validation.
8. Effect-token single-chain validation.
9. Capability origin, resource kind, and rights validation.
10. Constant and string pool range validation.
11. Resource-budget validation.
12. Canonical-encoding validation and checksum verification.

Unknown major versions are rejected. A higher minor version is rejected unless
all newly set flags and records are explicitly understood. Reserved fields must
be zero unless a later accepted ADR assigns them.

## Consequences

Record sizes, offsets, numeric type IDs, opcode IDs, verifier error identities,
version behavior, canonicalization, and checksum behavior become ABI once this
proposal is accepted.

The compiler may not invent semantics that the shared verifier does not
understand. The runtime and native backend may not execute graph behavior that
the package ABI and verifier do not describe.

This ADR is proposed architecture pending owner adoption. It does not authorize
Phase 1 implementation or any boot/runtime behavior changes.

## References

- `docs/superpowers/specs/2026-08-03-pyth-typed-instruction-graph-design.md`
- `docs/pyth-tig/acceptance/test-matrix.md`
- `docs/pyth-tig/acceptance/marker-contract.md`
