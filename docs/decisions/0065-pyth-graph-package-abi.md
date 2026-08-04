# ADR 0065: Pyth Graph Package ABI

Date: 2026-08-04
Status: Provisionally Accepted for Phase 1

## Context

The host compiler, PythCore loader, ring-3 interpreter, native code generator,
and verification tools require one canonical package contract before any
PythTIG behavior can be implemented. The owner has accepted ADR 0064's
architecture direction, while keeping this byte-level ABI provisional until the
first encoder, decoder, verifier, and negative corpus exercise the format.

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

The Phase 1 candidate package bounds are:

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

The Phase 1 candidate primitive type set is:

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

The Phase 1 candidate opcode families cover structural nodes, constants, pure
arithmetic/boolean operations, control terminators, and typed host operations
for system logging, object operations, task/proposal operations, graph
relationship queries, relevance assertions, capability requests, and later
command input/result operations.

The shared verifier must reject invalid packages before ring-3 entry. Its
candidate pass order is:

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
version behavior, canonicalization, and checksum behavior remain Phase 1
candidate ABI until the host-side encoder, decoder, verifier, and negative
corpus pass against real packages. Any Phase 1 change to those bytes must update
this ADR in the same branch before merge. They become permanent stable ABI only
after the Phase 1 evidence exists and the owner explicitly freezes the format.

The compiler may not invent semantics that the shared verifier does not
understand. The runtime and native backend may not execute graph behavior that
the package ABI and verifier do not describe.

This ADR authorizes no boot/runtime behavior changes by itself. Phase 1 may
begin only after explicit owner invocation, and Phase 1 remains verifier/format
work only unless a later accepted plan says otherwise.

## References

- `docs/superpowers/specs/2026-08-03-pyth-typed-instruction-graph-design.md`
- `docs/pyth-tig/acceptance/test-matrix.md`
- `docs/pyth-tig/acceptance/marker-contract.md`
