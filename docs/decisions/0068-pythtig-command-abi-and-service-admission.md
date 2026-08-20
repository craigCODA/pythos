# ADR 0068: PythTIG Command ABI And Service Package Admission

Status: Accepted

Date: 2026-08-20

## Context

ADR 0065 froze the tested PythTIG major-version-1 package ABI on 2026-08-08.
Phase 7 needs a typed command host-operation surface for the Pyth-native
session manager and a normal-boot proof that default service graph packages are
admitted from `INIT.PAK` before readiness markers are emitted.

Those additions must not be introduced as an undocumented mutation of the ADR
0065 v1.0 freeze baseline.

## Decision

PythTIG adopts a compatible minor-version-1 extension under major version 1.
No v1.0 record size, field offset, numeric type ID, existing opcode ID, error
identity, checksum rule, canonicalization rule, or package-limit rule changes.

A runtime/verifier that supports major 1 minor 1 accepts minor 0 and minor 1
packages. It rejects minor 2 or higher unless a later accepted ADR assigns that
behavior. A runtime/verifier that supports only major 1 minor 0 must reject
minor 1 packages.

Minor 1 assigns these command host-operation opcodes:

```text
0x1500 CommandRead
0x1501 CommandResultEmit
```

`CommandRead` consumes `[Effect, Capability]`, requires command read authority,
and exposes only closed `HostResult` fields for command kind, object id, task
id, proposal id, and bounded UTF-8 text. `CommandResultEmit` consumes
`[Effect, Capability, ErrorCode, Utf8]` and requires command append authority.

The shared verifier must enforce known opcodes, opcode signatures, capability
imports, required rights, closed host-result schemas, single effect-chain
ownership, and canonical zeroing before a package can enter ring 3.

The concrete command ABI records live in `shared/src/pyth_command_abi.rs` and
are `repr(C)` fixed-layout records. PythCore may marshal typed command records
and validate caller capabilities. PythCore must not parse Pyth source, human
command text, semantic prompts, or agent policy.

Normal boot must also admit the Phase 7 default service graph packages before
readiness:

```text
session-manager.tig principal 0x5059_5448_534D_0001
task-steward.tig    principal 0x5059_5448_5354_0001
```

Admission requires the named graph manifest to be present in `INIT.PAK`, the
manifest digest to match the package bytes, the shared PythTIG verifier to
accept the package, the manifest principal to match the expected service
principal, and the verified package to have a non-empty node/block shape.

The marker for this proof is:

```text
PYTHOS:PYTHTIG:SERVICE_PACKAGE_ADMITTED service:<stable-name> package:<hex> principal:<hex> nodes:<decimal> blocks:<decimal>
```

Invalid, missing, duplicate, wrong-principal, or verifier-rejected service
packages fail before a default-service readiness marker. This marker proves
package admission and service-readiness gating. It does not claim an
independent long-lived graph-service daemon scheduler.

## Consequences

Phase 7 can compile and verify the session-manager graph under the typed
command ABI without changing the ADR 0065 v1.0 baseline in place.

Default normal boot has serial evidence that readiness depends on real graph
package admission rather than marker-only supervisor state.

The Rust object shell remains the persistent compatibility and recovery process
until a later owner-invoked phase explicitly replaces that execution model.
