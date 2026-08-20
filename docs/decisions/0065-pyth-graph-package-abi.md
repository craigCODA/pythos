# ADR 0065: Pyth Graph Package ABI

Date: 2026-08-04
Status: Accepted; PythTIG version 1 ABI frozen 2026-08-08

## Context

The host compiler, PythCore loader, ring-3 interpreter, native code generator,
and verification tools require one canonical package contract before any
PythTIG behavior can be implemented. The owner has accepted ADR 0064's
architecture direction. The first encoder, decoder, verifier, and negative
corpus have now exercised the byte-level ABI and passed the Phase 1 acceptance
matrix.

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

The version 1 `PythGraphHeader` public layout struct is 96 bytes using
`#[repr(C, packed(4))]`. This 4-byte packing is part of the frozen layout
contract because the `checksum` field is intentionally at byte offset 84. A
plain `#[repr(C)]` layout on x86-64 would align that `u64` field to byte 88 and
produce a 104-byte structure.

Header field offsets are byte offsets from the start of the package:

```text
offset size field
0      8    magic
8      2    major
10     2    minor
12     4    flags
16     8    package_id
24     8    principal_id
32     4    entry_block
36     4    type_count
40     4    block_count
44     4    node_count
48     4    import_count
52     4    constant_pool_len
56     4    string_table_len
60     4    types_offset
64     4    blocks_offset
68     4    nodes_offset
72     4    imports_offset
76     4    constant_pool_offset
80     4    string_table_offset
84     8    checksum
92     4    reserved
96     0    end
```

All integer fields remain little-endian on disk. Encoders and decoders must use
explicit little-endian reads and writes for each field; they must not transmute
or otherwise treat host struct layout as the on-disk codec. Packed public fields
also must not be borrowed as if they were naturally aligned.

The version 1 package bounds are:

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

The version 1 primitive type set is:

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

The version 1 opcode families cover structural nodes, constants, pure
arithmetic/boolean operations, control terminators, and typed host operations
for system logging, object operations, task/proposal operations, graph
relationship queries, relevance assertions, capability requests, and later
command input/result operations.

The frozen task host-operation family includes `TaskContextRead` at opcode
`0x1205`. It consumes `[Effect, Capability]` and requires the task
read-context right. Its closed `HostResult` fields are active task id,
candidate task id, confidence score, proposal kind, and reason text. The
operation exposes only a bounded typed context summary; it does not grant task
state authority.

Capability imports become graph values only through the version 1 import
materialization convention: an entry-block `BlockParam` node with
`result_type = Capability` and `auxiliary0 = import_slot` names a declared
`CapabilityImportRecord`. The verifier validates that the import slot exists,
that the import's expected type is `Capability`, and that host operations consume
that capability as an ordinary SSA input. Host operations must not gain
authority from a hidden per-op import slot. `HostResult` typed fields remain
closed unless a per-op result schema is documented and verified; Phase 1 defines
no capability-returning host-result field.

The shared decoder and verifier must reject invalid packages before ring-3
entry. Their version 1 admission order is:

1. The decoder validates header magic/version/flags/reserved fields, declared
   count and byte limits, checked section ranges, record-section alignment,
   section non-overlap, exact package length, reserved record bytes, and the
   checksum.
2. The verifier validates known types and opcodes.
3. It validates block ownership, contiguous node coverage, reachability, and
   exactly one final terminator per block.
4. It validates control targets and block-argument count/type.
5. It validates dominance and value availability.
6. It validates opcode-specific types, the single effect chain, capability
   origin/import/rights, and closed host-result fields.
7. It validates every referenced constant/string half-open range with checked
   addition; import names and `ConstUtf8` payloads must also be valid UTF-8.
8. It validates canonical record encodings.

Canonical version 1 records require zero unassigned type/block/node flags and
zero unused auxiliary/immediate fields. Blocks must be encoded in block-table
order with contiguous `first_node` coverage and no more than four parameters.
Import slots must be dense and equal their record-table index; expected types,
resource kinds, and rights bits must be known. `ConstBool` immediates are exactly
zero or one. Opcode-assigned auxiliary fields remain meaningful only where the
frozen opcode schema assigns them.

Referenced-range or record-canonicalization failures use the frozen
`NonCanonicalEncoding` verifier identity. Section-range failures and checksum
failures retain their decoder error identities under `VerifyError::Decode`.

Unknown major versions are rejected. A higher minor version is rejected unless
all newly set flags and records are explicitly understood. Reserved fields must
be zero unless a later accepted ADR assigns them.

## Consequences

Record sizes, offsets, numeric type IDs, opcode IDs, limits, verifier error
identities, version behavior, canonicalization, and checksum behavior are the
stable PythTIG version 1 ABI. The owner froze this ABI on 2026-08-08 after the
host-side encoder, decoder, verifier, canonical-format tests, and deterministic
negative mutation corpus passed against real packages. An incompatible change
requires a new accepted ADR and a new major package version; it must not be
silently introduced under major version 1.

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
