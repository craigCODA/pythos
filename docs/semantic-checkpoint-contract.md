# PythOS Semantic Checkpoint Contract

Status: Accepted companion contract for ADR 0069.

This document defines the comparison vocabulary for parallel PythOS evidence
lanes. It is an acceptance and build-evidence contract, not a new runtime
service, package format, filesystem format, or kernel ABI.

## Purpose

PythOS phases already use serial markers, host tests, QEMU outcomes, package
digests, object-store checkpoints, and physical evidence records. Those oracles
are correct but currently phase-specific. Later work will increasingly run in
parallel: object locator resolution, PythTIG differential execution, storage
backend matrix work, packaging, updates, and eventually SMP.

The semantic checkpoint contract gives those lanes one comparison language.
Backends and build paths may differ. Meaning must not drift.

## Contract Version

The current contract name is:

```text
pythos-semantic-checkpoint-v0
```

Version `v0` records the evidence fields future tooling should emit. Once a
script or CI job consumes a field as a hard gate, incompatible field changes
require a follow-up ADR or an explicitly versioned `v1` contract.

## Required Top-Level Shape

A producer emits UTF-8 JSON with this top-level shape:

```json
{
  "schema": "pythos-semantic-checkpoint-v0",
  "producer": {
    "name": "phase12-path-resolution",
    "phase": "Phase 12",
    "slice": "path-resolution"
  },
  "source": {
    "git_commit": "hex commit",
    "tree_dirty": false,
    "build_profile": "qemu-verify"
  },
  "target": {
    "kind": "host|qemu|physical",
    "machine": "qemu-q35",
    "backend": "virtio-blk",
    "target_id": "stable evidence id"
  },
  "artifacts": [],
  "abi": [],
  "markers": {
    "raw_log_sha256": null,
    "normalized": [],
    "normalization_rules": []
  },
  "state": {},
  "authority": {},
  "denials": [],
  "comparisons": []
}
```

Fields may be `null`, empty arrays, or empty objects only when the producer
does not yet own that evidence class. A producer must not emit a placeholder
hash or fabricated marker.

## Artifact Digests

The `artifacts` array records every byte artifact whose identity is part of the
claim:

```json
{
  "name": "session-manager.tig",
  "kind": "pythtig-package",
  "sha256": "hex sha256",
  "semantic_role": "default-service-package"
}
```

Examples include:

- `pythos-core` ELF;
- `BOOTX64.EFI`;
- `INIT.PAK`;
- PythTIG graph packages;
- generated native user ELFs;
- package archives or package objects;
- prepared storage images;
- physical-import manifests.

## ABI Records

The `abi` array records versioned contracts the producer relies on:

```json
{
  "name": "pythtig",
  "version": "1.1",
  "digest": "optional semantic digest"
}
```

Common names include:

- `bootinfo`;
- `syscall`;
- `object-store`;
- `object-shell`;
- `pythtig`;
- `semantic-checkpoint`;
- `object-locator`;
- `package-format`.

If a branch changes an ABI record that is frozen by an accepted ADR, the
checkpoint is not enough. The branch also needs the required migration or new
ADR.

## Marker Normalization

The `markers.normalized` array records semantic markers after removing allowed
target noise.

Allowed normalization:

- PCI bus/device/function numbers when the semantic selected-backend claim is
  preserved separately;
- controller discovery ordering when the selected controller identity is
  preserved separately;
- timing, tick counts, dwell counts, and retry counts unless the test is about
  timing;
- framebuffer resolution and pixel format unless the test is about rendering;
- QEMU-only exit plumbing.

Forbidden normalization:

- missing required markers;
- changed marker order;
- panic markers;
- package rejection markers;
- changed object ids;
- changed capability grant/use/denial outcomes;
- changed denial identity;
- changed committed storage state;
- changed PythTIG package checksum or runtime digest.

## State Hashes

The `state` object uses named hashes for semantic state:

```json
{
  "object_graph_hash": "hex sha256 or null",
  "relationship_hash": "hex sha256 or null",
  "revision_history_hash": "hex sha256 or null",
  "storage_checkpoint_hash": "hex sha256 or null",
  "package_registry_hash": "hex sha256 or null",
  "locator_namespace_hash": "hex sha256 or null"
}
```

Hash inputs must be canonicalized by the producing harness. Canonicalization
must sort unordered maps, encode integers in one endianness, preserve stable
object identities, preserve revision/provenance fields, and exclude
backend-only log noise.

## Authority Transcript

The `authority` object records capability-sensitive outcomes:

```json
{
  "capability_transcript_hash": "hex sha256 or null",
  "grant_count": 0,
  "use_count": 0,
  "revocation_count": 0,
  "denial_count": 0
}
```

The transcript must distinguish at least:

- granted use;
- missing capability;
- wrong holder;
- stale generation;
- forged handle;
- missing rights;
- hardware denied;
- known target denied.

The exact transcript encoding belongs to the implementation harness that first
emits it, but once emitted and consumed by CI it becomes part of this evidence
contract.

## Denial Records

The `denials` array records named negative proofs:

```json
{
  "case": "locator-dot-dot-denied",
  "expected": "NamespaceEscapeDenied",
  "observed": "NamespaceEscapeDenied"
}
```

A denial passes only when the expected and observed denial identities match.
Generic failure, timeout, panic, or missing marker is not a denial proof.

## Comparison Records

The `comparisons` array records what this checkpoint was compared against:

```json
{
  "against": "target/semantic-checkpoints/qemu-ahci.json",
  "mode": "cross-backend",
  "result": "equal",
  "differences": []
}
```

Known comparison modes:

- `cross-backend`;
- `interpreter-vs-native`;
- `compiler-vs-reference`;
- `direct-launch-vs-package-launch`;
- `pre-update-vs-post-update`;
- `rollback`;
- `single-core-vs-smp`;
- `physical-import`.

## Lane Requirements

### Phase 12 Object Locator

Compare:

- same namespace root authority;
- same locator input;
- same resolved object identity;
- same revision selection;
- same relationship path;
- same denial identity for missing rights, missing segment, stale binding, and
  namespace escape attempts.

### PythTIG Differential Execution

Compare:

- same package SHA-256;
- same runtime digest where emitted;
- same normalized PythTIG semantic markers;
- same host-operation capability behavior;
- same runtime entry and exit outcome.

### Storage Backend Matrix

Compare:

- same object-store checkpoint hash;
- same journal and recovery outcome;
- same quota outcome;
- same capability transcript;
- same storage denial transcript.

Backend identity markers remain target-specific evidence and must not be
generalized from QEMU to physical hardware.

### Phase 13 Packaging

Compare:

- same package artifact digest;
- same installed typed object graph;
- same package registry hash;
- same grant manifest;
- same launch authority;
- same uninstall reclamation and capability revocation.

### Updates And Recovery

Compare:

- pre-update boot manifest;
- candidate update manifest;
- activated manifest;
- failed-update rollback manifest;
- package registry and object-store hashes before and after rollback.

### SMP

Compare:

- all earlier adversarial and negative suites;
- object-store committed state;
- capability denials;
- crash-containment outcomes;
- marker order constraints.

Scheduling interleavings may differ only where the original proof permits
interleaving differences.

## Merge Rule

A semantic checkpoint is evidence, not permission to ignore the roadmap. A
parallel lane may merge only when:

1. its active phase or slice has been explicitly invoked;
2. it does not silently alter frozen ABI or marker contracts;
3. its checkpoint matches the required reference for the lane;
4. its docs state the target-specific evidence boundary;
5. existing acceptance gates still pass.
