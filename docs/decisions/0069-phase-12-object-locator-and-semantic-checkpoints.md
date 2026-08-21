# ADR 0069: Phase 12 Object Locator Namespace And Semantic Checkpoints

Date: 2026-08-20
Status: Accepted

## Context

Phase 10 gives PythOS dynamic, journaled, quota-enforced typed-object storage.
It does not decide how later services, packages, diagnostics, or operator-facing
tools name stored objects without already holding an `ObjectId`.

Phase 12 slice 1 exists to decide between three directions:

1. pure object-graph addressing with no path-like locator surface;
2. a thin path layer over the object graph;
3. a POSIX-adjacent hierarchy with directories, permissions, links, and mount
   behavior.

ADR 0018 established stable typed object identity separate from presentation
state. ADR 0022 made typed object identity durable. ADR 0066 supersedes the
desktop-shell and conventional file-navigation authority model. ADR 0064 and
ADR 0065 also require PythCore to preserve typed graph and capability semantics
without parsing human command text or granting authority from semantic
relevance.

At the same time, the PythTIG cross-target work showed a useful evidence model:
compare package bytes, runtime digest, and normalized semantic markers across
backends, while refusing to treat backend-specific noise as semantic equality.
Phase 12 should generalize that discipline before package management,
hardware-backend expansion, update/recovery, or SMP work begins to produce
parallel evidence.

## Decision

PythOS adopts a **capability-scoped object locator namespace** for Phase 12.

This is not a POSIX filesystem, and the locator text is not authoritative
identity. Canonical identity remains the typed object identity already accepted
by earlier phases:

```text
ObjectId
ObjectKind
schema version
RevisionId or revision policy
relationship records
capability policy
writer/service provenance
```

A namespace is itself a typed object or typed object relationship set. A
locator lookup starts from an explicit namespace root object for which the
caller holds a capability. The resolver walks bounded name-binding
relationships inside that namespace and returns a typed object result only if
the caller also has authority for the resolved object operation.

Locator strings may look path-like for package manifests, tests, diagnostics,
and operator handoff, but slash-separated spelling is only a projection over
typed relationships. It never creates ambient authority, a global root, a
current working directory, file descriptors, Unix permission bits, hard links,
symlinks, mount points, or byte-stream-first file identity.

The Phase 12 slice names remain compatibility labels:

```text
path-vs-graph-decision  -> this ADR
path-resolution         -> object locator namespace resolution
path-adversarial-suite  -> namespace confusion and authority-denial proofs
```

## Required Resolver Semantics

The later `path-resolution` slice must define the exact ABI and implementation,
but it must preserve these semantics:

- Resolution starts from a caller-supplied namespace root, not from a global
  root or ambient current directory.
- Each locator segment resolves through a typed name-binding relationship.
- Empty segments, `.`, `..`, drive prefixes, URI schemes, wildcard expansion,
  shell expansion, and host filesystem absolute paths are not valid authority.
- Holding the locator text is never enough to access the object.
- Visibility or semantic relevance is never enough to access the object.
- The caller must hold namespace traversal authority for every namespace
  boundary and operation authority for the resolved object.
- A resolver may return a stable object identity, relationship path, revision
  selection, and denial identity; it must not return raw disk blocks, raw
  pointers, inodes, or file descriptors.
- Name collisions, stale bindings, missing segments, missing traversal rights,
  missing final-object rights, and namespace-confusion attempts must produce
  explicit denials.
- Rebinding a name is an object-store mutation with normal revision,
  provenance, journal, quota, and capability checks.

## Semantic Checkpoint Contract

Phase 12 also accepts a build-evidence contract:
`docs/semantic-checkpoint-contract.md`.

The contract generalizes the PythTIG cross-target comparison model into a
shared checkpoint language for later parallel lanes. Producers record artifact
digests, ABI versions, normalized serial markers, object graph state,
relationship state, revision/provenance state, capability transcripts, denial
transcripts, storage checkpoints, package registry state when it exists, and
locator namespace state when it exists.

Checkpoint normalization may remove target noise, such as PCI bus ordering,
timing, framebuffer dimensions, or backend-selection detail. It must not hide
semantic differences such as changed package bytes, changed PythTIG runtime
digest, changed object identity, changed authority outcome, changed denial
identity, changed committed object-store state, or changed marker order.

Parallel work may run only when it can either:

1. avoid frozen ABI and semantic contracts entirely; or
2. emit checkpoints proving equivalence at its merge gate.

Merge authority remains serial. The checkpoint contract allows parallel
evidence gathering and independent branch work; it does not allow independent
branches to silently mutate object formats, syscall numbers, capability handle
semantics, PythTIG package layouts, marker contracts, journal/checkpoint
formats, package identity rules, or resolver authority rules.

## Consequences

Phase 12 does not build a general filesystem. It builds a capability-gated
object locator service over the existing object graph.

Phase 13 package work can name install roots and package contents through
object locator namespaces, but package identity and launch authority remain
typed-object and capability decisions.

Hardware backend work can run in parallel only below typed service contracts.
Backends may differ in discovery and transport; object-store semantics,
capability outcomes, and committed checkpoints must compare equal.

PythTIG compiler, interpreter, and native backend work must continue to compare
the same verified package semantics rather than trusting successful execution
alone.

Update/recovery and SMP work can design against this checkpoint model before
their implementation phases, but they remain outside Phase 12 implementation
scope.

Semantic indexing and local AI remain outside the trusted core and outside the
numbered roadmap. They do not become authority sources through locator names,
checkpoint comparison, task relevance, or proposal confidence.

## Non-Goals

This ADR does not authorize:

- POSIX directories, hard links, symlinks, chmod bits, uid/gid ownership,
  mount tables, file descriptors, or a VFS;
- package installation or launch;
- networking;
- update/recovery implementation;
- SMP implementation;
- new PythTIG package ABI changes;
- AI, semantic search, or task-steward authority;
- hardware expansion beyond existing target-specific evidence work.

## Verification

This ADR is the Phase 12 `path-vs-graph-decision` artifact. It changes
documentation only and does not alter boot behavior, marker order, kernel code,
object-store bytes, syscall ABI, or PythTIG package bytes.

The later `path-resolution` implementation slice must add automated acceptance
for successful locator resolution and explicit denial cases. The later
`path-adversarial-suite` slice must prove namespace-confusion attacks fail
specifically, not generically.

Until those implementation slices land, Phase 12 is complete only through this
decision slice.
