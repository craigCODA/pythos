# Phase 13 Package Lifecycle And Schema Extensibility Design

Status: accepted design; implementation requires explicit Phase 13 owner invocation

Date: 2026-08-22

Scope: Phase 13 only

## Purpose

Phase 13 introduces package lifecycle support without reintroducing the
conventional application model rejected by ADR 0066, and without using package
names as authority in conflict with ADR 0069.

The historical roadmap phrase "Applications and Packaging" is compatibility
vocabulary. In the post-ADR-0066 model, a PythOS package is not a desktop
application, not a window owner, not a launcher entry, and not a filesystem
tree. It is a graph-native, object-store-native, capability-scoped install unit
that may provide PythTIG graph programs, executable tools, service candidates,
schema definitions, projection descriptors, static data, and package-defined
semantic object behavior.

The Phase 13 goal is to prove one local package lifecycle end to end:

```text
package format
-> local install
-> capability-scoped launch
-> uninstall / revocation
-> independently authored package
-> durable schema extensibility
```

Phase 13 does not authorize persistent Pyth sessions, presentation bridges,
input bridges, Kai transport, Kai V0, WakeContext, context compilation, agent
memory, AI-authored Pyth, First Waking, autonomy, remote registries, dependency
resolution, package signing, networking, OS update/recovery, SMP, or new
hardware work.

## Normative Identity Model

Phase 13 carries forward the same separation of syntax, identity, description,
verification, and authority established by the object locator model:

```text
Package names / locators locate.
Package ObjectId identifies.
Digests verify immutable package content / revisions.
Manifest relationships describe.
Capability grants authorize.
```

A package name, locator, export name, or schema name is never canonical package
identity. A digest is not package identity either. A digest verifies an
immutable package release/content revision, while the installed Package
ObjectId identifies the durable PythOS package installation across compatible
updates, disable/enable cycles, and explicitly targeted reactivation.

Installation does not imply launch. Launch does not imply authority. A manifest
declaring a capability requirement means that the export cannot function
without that authority; it does not grant the authority.

## Live Repository Constraints

The live repository constrains the design in several important ways:

- `TypedObjectRecord` is intentionally compact: stable object id, fixed
  ObjectKind, schema version, and four bounded fields.
- Public object creation currently exposes only `Note` through the object shell
  ABI; this is not a general schema system.
- The object service already owns dynamic object creation, revision history,
  provenance, storage quota accounting, relationship insertion, and checkpoint
  persistence.
- Object-service checkpoint persistence currently preserves only the bounded
  snapshot fields it explicitly encodes. A package registry cannot depend on
  non-persisted in-memory relationships.
- ADR 0069 object locators are internal capability-scoped locators, not global
  POSIX paths and not package identity.
- PythTIG package bytes and verifier identities are frozen by the existing
  PythTIG ADRs. Phase 13 may wrap PythTIG content in a PythOS package, but it
  must not silently change PythTIG record layouts, opcode semantics, checksum
  behavior, or verifier denial identities.
- Existing PythTIG normal-boot service admission proves package admission and
  readiness gating, but not an independent long-lived packaged Pyth session
  runtime. That handoff remains outside Phase 13.

## 1. Proposed Package Format And Identities

### Package Artifact

The proposed Phase 13 local package artifact is a canonical byte stream:

```text
PythOSPackageV0
  header
  manifest
  content table
  content bytes
```

This is a proposed package format for the Phase 13 ADR. It is not a frozen ABI
until accepted by a future implementation ADR and acceptance tests.

Proposed header:

```text
magic                    8 bytes   "PYTHPKG0"
format_major             u16       0
format_minor             u16       1
header_length            u32
manifest_offset          u64
manifest_length          u64
content_table_offset     u64
content_table_length     u64
content_bytes_offset     u64
content_bytes_length     u64
manifest_sha256          [u8; 32]
artifact_sha256          [u8; 32]
reserved                 fixed zero bytes
```

The artifact digest verifies the exact immutable package release bytes. The
digest domain is the entire artifact byte stream with the `artifact_sha256`
field zero-filled. The digest field is restored to the computed SHA-256 value
after hashing. No other field is excluded.

The manifest digest verifies the canonical manifest region independently so a
manifest can be referenced, logged, and checked without treating the artifact
digest as package identity.

### Manifest

The proposed manifest is a canonical bounded binary record set. Text formats
may be useful for authoring tools, but the installed artifact should carry the
canonical manifest bytes described here rather than depend on parser-specific
text normalization.

Proposed manifest encoding:

```text
ManifestV0
  magic                 8 bytes   "PYTHMAN0"
  record_count          u32
  records[]             sorted by (record_type, stable_name)

ManifestRecord
  record_type           u16
  flags                 u16
  stable_name_length    u16
  payload_length        u32
  stable_name           bounded ASCII bytes
  payload               record-type-specific canonical bytes
```

The manifest encoding requires:

- deterministic field order;
- bounded integer and string lengths;
- explicit version fields;
- no duplicate keys or duplicate names inside a namespace;
- no implicit defaults that affect authority;
- deterministic digest over the exact canonical bytes.

The manifest integrity check is the header `manifest_sha256`, computed over
the exact `ManifestV0` bytes. Phase 13 does not add a second manifest checksum.

Conceptual manifest fields:

```text
package_locator_hint
release_label
release_sequence
manifest_schema_version
package_principal_hint
source_provenance
schemas[]
exports[]
content[]
requirements[]
relationships[]
```

`package_locator_hint` is only a human/package-facing locator hint. It is not
identity and does not resurrect old package identity. `source_provenance`
records where the local package came from, but Phase 13 does not treat that
provenance as publisher authenticity.

`package_principal_hint` is descriptive/requested identity metadata only. A
package cannot claim an existing service principal by placing its name or
numeric value in the manifest. PythOS allocates or validates runtime principals
through its own service/process identity rules.

Manifest-declared provenance and installer-observed provenance are separate:

```text
manifest-declared provenance
  -> untrusted package-supplied metadata

installer-observed provenance
  -> PackageSourceHandle identity
  -> source artifact digest
  -> installing service identity
  -> install time / object revision
```

The second category is PythOS-observed history. It records what the installer
actually saw and did, but it still does not prove cryptographic publisher
authenticity in Phase 13. It is not a manifest field; it is recorded in the
published Package revision and package-registry provenance state.

### Package Format Bounds

The Phase 13 package parser must reject inputs outside these bounds before
allocation, offset arithmetic with untrusted values, or semantic validation:

```text
MAX_PACKAGE_ARTIFACT_BYTES        4 MiB
MAX_MANIFEST_BYTES                64 KiB
MAX_CONTENT_TABLE_BYTES           32 KiB
MAX_CONTENT_BYTES                 4 MiB
MAX_MANIFEST_RECORDS              256
MAX_CONTENT_ENTRIES               64
MAX_EXPORT_RECORDS                32
MAX_REQUIREMENT_RECORDS           64
MAX_SCHEMA_DECLARATIONS           32
MAX_MANIFEST_RELATIONSHIPS        128
MAX_STABLE_NAME_BYTES             48
MAX_MANIFEST_RECORD_PAYLOAD_BYTES 1024
MAX_CONTENT_EXTENTS_PER_RECORD    32
MAX_PACKAGE_SOURCES               8
MAX_PACKAGE_SOURCE_LABEL_BYTES    48
MAX_LOCATOR_SEGMENTS              4
MAX_LOCATOR_SEGMENT_BYTES         16
```

All offsets are checked as half-open ranges within the package artifact. Length
addition must be overflow-checked before any range comparison. The locator
limits align with ADR 0069 / ADR 0070 rather than defining a package-specific
path grammar.

These are Phase 13 local-package limits, not a permanent claim about future
large package support.

### Local Package Ingress

Phase 13 has no POSIX filesystem, remote registry, networking, or package
signing. The initial local ingress is therefore a bounded package-source record
inside `INIT.PAK`.

The boot image may include one or more package-source records:

```text
InitPakPackageSourceV0
  source_id
  package_artifact_offset
  package_artifact_length
  package_artifact_sha256
  source_label
```

PythCore exposes each record to the package installer as a kernel-owned
`PackageSourceHandle`. That handle locates bytes for installation only. It is
not package identity, not a package locator, not an authority grant, and not a
claim of publisher authenticity.

The authority boundary is explicit:

```text
PackageSourceHandle
  -> identifies bounded source bytes

PackageSourceRead capability
  -> authorizes reading those bytes

PackageInstall capability
  -> authorizes attempting installation
```

For Phase 13 acceptance, the fixed trusted package installer service is the
only recipient of `PackageSourceHandle` values and it receives explicit
`PackageSourceRead` and `PackageInstall` authority from PythCore test setup.
Possessing or guessing a handle value is not sufficient to read source bytes or
install a package.

The installer must copy and stage bytes from the package-source handle into the
package content transaction, verify the artifact digest, and then proceed
through the normal install path. Future local transports can produce the same
kind of bounded package-source handle, but Phase 13 acceptance uses `INIT.PAK`
only.

### Content Entries

Each content entry describes one immutable payload inside the artifact:

```text
content_index
role
offset
length
sha256
format
declared_runtime
declared_entrypoint
```

Roles are descriptive and bounded. Initial roles should include:

```text
pythtig_graph
schema_descriptor
projection_descriptor
static_data
tool_data
```

Adding a role describes how a package wants bytes interpreted. It does not
grant authority and does not create a new core ObjectKind.

### Export Records

`PackageExport` is a manifest record, not a proposed core object kind.

An export record names something the package can provide:

```text
export_name
export_kind
content_index
entrypoint
schema_refs[]
requirement_refs[]
relationship_refs[]
```

Initial export kinds:

```text
tool
service_candidate
projection_descriptor
schema
static_data
```

`service_candidate` is deliberately not a persistent service runtime. In Phase
13 it may be launched as a bounded Phase 9 process according to normal process
rules. The later persistent Pyth session convergence milestone is outside this
phase.

### Capability Requirement Records

`CapabilityRequirement` is a manifest record, not a proposed core object kind.

A requirement record describes an authority need:

```text
requirement_id
resource_kind
operation
target_policy
required_for_exports[]
human_label
```

The requirement is inspected at launch. It never self-grants. A caller or owner
must supply an explicit capability grant set, and the kernel validates that the
grants satisfy the requested launch operation.

### Identity Distinctions

Phase 13 must keep these identifiers distinct:

```text
Package ObjectId
  Durable installed package identity.

Immutable release / content digest
  Verifies exact package artifact, manifest, and payload bytes.

Installed package revision
  Object-service revision of the Package object that records which release is
  currently installed, disabled, tombstoned, or reactivated.

SchemaDefinition ObjectId
  Durable schema identity in PythOS.

Schema revision
  Exact revision of a schema descriptor used to interpret instances.

PackageDefinedObject ObjectId
  Durable identity of an individual object instance created under a package-
  defined schema.
```

Names and locators may resolve to these identities, but they are not these
identities.

## 2. Minimum Core Object Vocabulary

The accepted conceptual starting point is:

```text
Package
SchemaDefinition
PackageDefinedObject
```

This design accepts the split as the Phase 13 direction, but does not assign
permanent numeric ObjectKind values. The future Phase 13 implementation ADR
must still prove that each core kind is necessary and that a smaller primitive
set would fail one of the invariants below.

### Candidate: Package

`Package` appears to belong in the core vocabulary because the package
lifecycle is trusted substrate, not user ontology. The kernel and object store
must be able to distinguish an installed package identity from ordinary
package-defined semantic instances in order to enforce install, launch,
disable, upgrade, tombstone, provenance, quota, and revocation semantics.

If Package were represented only as a PackageDefinedObject, the authority model
would depend on a schema supplied by the thing being installed. That would make
the install authority boundary circular.

### Candidate: SchemaDefinition

`SchemaDefinition` appears to belong in the core vocabulary because durable
instances must remain interpretable after package disable, package uninstall,
or package upgrade. Schema identity must be stable and referenceable
independent of a package's current locator binding.

If schemas were stored only as package manifest records with no object identity,
an existing instance could not point at an exact durable schema identity and
revision in a way that survives package lifecycle changes.

### Candidate: PackageDefinedObject

`PackageDefinedObject` appears to belong in the core vocabulary because PythOS
needs one generic, stable envelope for open-ended semantic instances. This is
the mechanism that prevents Kai, Seed, rooms, tools, memories, environments,
and future user concepts from requiring new PythCore ObjectKind codes.

If every semantic type became a core kind, the core ontology would grow without
bound and the typed object ABI would churn. If arbitrary package-defined
objects were represented as `Note` or another compatibility kind, the object
store would lose the durable schema reference needed for interpretation,
migration, and provenance.

### Concepts That Should Not Be Core Object Kinds In Phase 13

The following concepts should remain manifest records, registry records, typed
relationships, or content metadata unless future evidence proves they need
independent object identity:

```text
PackageContent
PackageExport
CapabilityRequirement
PackageRelease
PackageLocatorBinding
PackageSource
```

Content exists, but bytes do not need object identity merely because they are
large. Exports name provided surfaces inside a package release, but the Package
ObjectId plus exact release digest plus export name is sufficient in Phase 13.
Capability requirements describe authority needs, but grants remain capability
records and kernel validation decisions. Package sources are local ingress
handles, not installed objects.

## 3. Schema Definition And Instance Representation

### SchemaDefinition

A SchemaDefinition object identifies a durable schema family. Its retained
revisions identify exact schema descriptor versions.

Conceptual authoritative state:

```text
schema_object_id
defining_package_object_id
current_schema_revision
schema_name_locator_hint
schema_descriptor_digest
schema_descriptor_content_ref
compatibility_policy
status
provenance
```

`schema_name_locator_hint`, such as `kai.v0`, locates or labels a schema for
humans and package manifests. It is not identity.

Compatible evolution may revise the same SchemaDefinition identity. An
incompatible schema change requires either:

- a new SchemaDefinition ObjectId; or
- an explicit migration/supersession relationship recorded in the object graph.

### PackageDefinedObject

A PackageDefinedObject is the generic envelope for package-defined instances.
It has its own ObjectId and points to the exact schema revision under which it
was created or last deliberately migrated.

Required instance state:

```text
instance_object_id
schema_definition_object_id
schema_revision
instance_state_ref
instance_state_revision
instance_state_digest
instance_state_format
creator_service_id
created_by_package_object_id
revision
provenance
```

The object store must preserve this invariant:

```text
PackageDefinedObject
  -> exact SchemaDefinition ObjectId
  -> exact schema revision
```

If an object still references a schema revision, that schema revision remains
durable. Uninstall must never create semantic amnesia.

### Immutable Package Content Vs Mutable Instance State

Phase 13 must not use the immutable package content store as the storage model
for runtime object state.

The distinction is normative:

```text
Package content
  -> immutable
  -> release-scoped
  -> digest-addressed / verified
  -> owned by the package installation

PackageDefinedObject instance state
  -> mutable through object revisions
  -> object-scoped
  -> history / provenance-bearing
  -> owned by the PackageDefinedObject or its quota principal
```

Schema descriptors and PythTIG executables live in immutable package content
extents. A PackageDefinedObject payload lives in object-owned dynamic storage
or payload extents managed through the object service. Updating an instance
creates a new object revision that may point to a new instance-state reference
and digest while preserving the same PackageDefinedObject ObjectId and exact
schema reference, unless a deliberate migration changes the schema reference.

This keeps package releases from becoming runtime storage. A tool can be
uninstalled while the objects it created, their state revisions, their
provenance, and their exact schema interpretation contracts remain durable.

### TypedObjectRecord Pressure

The existing fixed four-field `TypedObjectRecord` can carry only compact
authoritative metadata. Phase 13 should not stretch it into a general payload
or schema descriptor format.

For a PackageDefinedObject, the compact record should carry identity-critical
references such as schema id/revision and instance-state reference. Schema
descriptors and package bytes live in immutable package content extents.
PackageDefinedObject state lives in object-owned dynamic payload extents and
is revisioned by the object service.

## 4. Package Registry Persistence Format And Boundary

Phase 13 needs a durable package registry because package lifecycle state spans
content extents, package objects, schema definitions, manifest metadata,
locator bindings, export records, quota accounting, provenance, and install
transactions.

The package registry must not become a second authoritative object graph.
Semantic authority remains the published package/object graph: Package
ObjectIds, SchemaDefinition ObjectIds, PackageDefinedObject ObjectIds, object
revisions, retained schema revisions, typed relationships, provenance, and
capability decisions. The package registry checkpoint is the specialized
crash-consistent persistence and index representation used to restore and
materialize that graph.

There is no meaningful state where the Package object says `Installed` while
the registry says `Tombstoned`, or the reverse. A package lifecycle publication
selects the object/revision state and the package registry generation as one
semantic unit. If recovery observes object-service state and package-registry
state that cannot be reconciled for the selected publication anchor, that world
is invalid. Recovery must select the previous valid published world or deny
package operations rather than treating the disagreement as live policy.

The registry should follow existing object-service checkpoint and Phase 10
journal/recovery discipline. It should not introduce a parallel filesystem or
unrelated transaction model.

Proposed registry checkpoint encoding:

```text
PackageRegistrySnapshotV0
  magic                         8 bytes   "PYTHPKR0"
  major                         u16       0
  minor                         u16       1
  generation                    u64
  active_transaction_id         u64       0 when no active transaction
  committed_root_digest         [u8; 32]
  package_record_count          u32
  schema_record_count           u32
  content_record_count          u32
  export_record_count           u32
  requirement_record_count      u32
  locator_binding_record_count  u32
  tombstone_record_count        u32
  records                       canonical record blocks in the order above
  snapshot_crc32c               u32
```

Records are sorted by their stable identity fields inside each block. Unknown
major versions deny package operations. Unknown minor versions may be accepted
only if all unknown record flags are explicitly ignorable.

`snapshot_crc32c` uses CRC-32C Castagnoli over the exact snapshot bytes with
the `snapshot_crc32c` field zero-filled. SHA-256 remains the integrity identity
for immutable package artifacts and content; the registry CRC is a checkpoint
corruption detector aligned with the existing storage recovery style, not a
content identity.

### Cross-Checkpoint Publication Anchor

Package registry recovery must know which object-service checkpoint belongs to
which package-registry generation. Phase 13 therefore needs a durable
publication anchor that selects the candidate object/package pair as the next
authoritative world.

Normative transaction invariant:

```text
Uncommitted state was never reality.
```

Phase 13 must distinguish three concepts that are not equivalent:

```text
Physical durability
  bytes have reached storage

Candidate validity
  bytes decode, checksum, and form a complete candidate world

Semantic publication
  a valid PackageTransactionCommitV0 selects that candidate world
```

Durability does not imply publication. Validity does not imply publication.
Only a valid publication anchor selects reality.

Proposed commit anchor:

```text
PackageTransactionCommitV0
  transaction_id
  operation
  package_registry_generation
  package_registry_root_digest
  object_checkpoint_generation
  object_checkpoint_root_digest
  package_object_id
  package_installed_revision
  commit_crc32c
```

`commit_crc32c` is CRC-32C Castagnoli over the exact canonical anchor bytes
with the `commit_crc32c` field zero-filled.

Recovery accepts a Phase 13 world only when the referenced object-service
candidate checkpoint and package-registry generation match the same valid
`PackageTransactionCommitV0` publication anchor:

```text
candidate object checkpoint B
+ candidate package registry generation B
+ publication anchor B
  -> published Phase 13 world B
```

If the newest candidate registry generation and candidate object checkpoint do
not match a valid publication anchor, recovery selects the previous valid
published world. If no valid package publication anchor exists, package
operations are denied while older object-store behavior continues according to
the existing ordinary object-service recovery rules.

This anchor is the mechanism behind the "one semantic unit" rule. The registry
does not decide package truth independently; it restores the object/package
graph state selected by the matching publication anchor.

Ordinary object-service checkpoint semantics are unchanged. A normal object
checkpoint still becomes recovery-eligible through its existing commit-sector
protocol, and ordinary recovery still selects the newest valid committed object
checkpoint. Phase 13 package installation requires a separate candidate path
capable of producing object-service state that is durable, verifiable,
root-addressable, package-anchor-referenceable, and excluded from ordinary
object-service recovery selection until a valid package publication anchor
selects it.

### Package Record

```text
package_object_id
state
installed_revision
current_release_digest
current_manifest_digest
package_locator_hint
principal_hint
source_provenance_digest
quota_charge
created_revision
last_updated_revision
```

### Schema Record

```text
schema_object_id
defining_package_object_id
state
current_schema_revision
retained_revisions[]
schema_descriptor_digest
schema_descriptor_content_ref
schema_locator_hint
supersedes_schema_object_id
```

### Content Record

```text
content_id
state
role
package_object_id
release_digest
sha256
size
extent_list
ref_count
quota_owner
```

`content_id` is a registry-local persistent handle scoped to one published
package ObjectId and release digest. It is not canonical content identity and
does not replace SHA-256 verification. In Phase 13, identical content bytes do
not deduplicate across packages or across package releases. A new release gets
new content records even if a digest matches earlier bytes; old content records
remain only while referenced by retained package, export, schema revision, or
explicit immutable asset reference.

### Export Record

```text
package_object_id
release_digest
export_name
export_kind
content_id
entrypoint
schema_refs[]
requirement_refs[]
```

### Requirement Record

```text
package_object_id
release_digest
requirement_id
resource_kind
operation
target_policy
required_for_exports[]
```

### Locator Binding Record

Published locator visibility is derived directly from the selected published
registry generation. A binding record may be materialized into ADR 0069
name-binding relationships, but those materialized bindings are rebuildable
mirrors. They cannot become authoritative independently of the selected
registry generation.

```text
locator_root_object_id
segment
target_kind
target_object_id
package_object_id
release_digest
state
```

A crash between publishing a generation and rebuilding materialized
name-binding mirrors must recover as a published package with rebuildable
mirrors, not as a half-installed package. A crash before publication must
recover by selecting the previous published world.

The semantic checkpoint contract should include package registry state,
materialized locator bindings, package artifact digests, export records,
schema-definition revision references, and denial identities.

## 5. Large Content Extent Model

Large immutable package content lives outside compact typed object records in a
package content store backed by Phase 10 allocated extents.

Content-store invariants:

- committed content is immutable;
- every committed content record has digest, size, role, extent metadata,
  package ObjectId, and release digest;
- candidate content is not reachable through package locators or launch until a
  valid publication anchor selects the candidate package world;
- content digest is verified before candidate validation;
- launch revalidates the content digest before verifying PythTIG content;
- unpublished candidate content is unreachable and reclaimable after recovery;
- content quota accounting is derived from the selected anchored package world;
- retained schema revisions hold live references to their schema descriptor
  content records and extents.

Package-content allocation metadata must not become an independently
authoritative allocation map. Selected anchored package registry state
references content extents, and referenced extents are live. Candidate registry
state may reference physically written candidate bytes, but those bytes are not
live in the authoritative world until the candidate registry is selected by a
valid publication anchor. No selected anchored root reference means the extent
is reclaimable/free.

`PackageContent` should not be introduced as a core object kind merely because
content bytes exist. If future phases need content objects with independent
authority, that should be justified separately.

## 6. Atomic Install, Upgrade, Uninstall, And Recovery

Installation, upgrade, and uninstall must be crash-consistent semantic
publication transitions over the package registry and content store.

The key invariant:

```text
Uncommitted state was never reality.
```

After recovery, the previous published world or the newly published world is
authoritative. No half-world is authoritative.

No recovered state may expose:

- a locator binding without published package content;
- a Package object without required SchemaDefinition records;
- a package release without verified content digests;
- quota-consumed content with no selected published package world;
- a launchable export whose manifest or PythTIG content was not published.

### Install Publication Boundary

Proposed install sequence:

```text
1. Allocate transaction id.
2. Verify artifact, manifest, and per-content digests.
3. Validate manifest structure and bounded names.
4. Verify PythTIG exports without launching them, preserving any existing
   PythTIG verifier denial identity as the nested cause of package rejection.
5. Validate schema declarations.
6. Validate locator bindings through ADR 0069 grammar rules.
7. Reserve candidate quota in memory or candidate metadata without exposing it
   as authoritative usage.
8. Allocate Package and SchemaDefinition object identities in a candidate
   object graph.
9. Prepare package registry candidate generation, including package records,
    schema records, content records, export records, requirement records, and
    locator binding records.
10. Write candidate content bytes.
11. Write candidate package registry state.
12. Write candidate object-service checkpoint state through a candidate path
    that is not eligible for ordinary object-service recovery selection.
13. Re-read and validate the candidate content, registry root, and object root.
14. Emit candidate-ready evidence only after the complete candidate world is
    durable and valid.
15. Publish the candidate world by writing `PackageTransactionCommitV0` as the
    semantic anchor binding the object checkpoint root and package registry
    root.
16. Rebuild or refresh materialized ADR 0069 name-binding mirrors from the
    selected published generation.
```

The package is not installed until step 15 publishes. Locator exposure is
derived from the selected published generation, not an earlier side effect and
not a separate authority source.

### Install Recovery

Recovery rules:

```text
valid publication anchor
  -> load the exact referenced object candidate/checkpoint and package
     registry generation
  -> validate both roots
  -> select that published world

candidate state without valid publication anchor
  -> ignore as non-authoritative
  -> treat candidate-only material as unreachable/reclaimable

torn or corrupt transaction tail
  -> select previous valid published world

candidate content with no selected anchored registry reference
  -> mark reclaimable/free

valid publication anchor but missing materialized locator mirrors
  -> rebuild mirrors from the selected published registry generation

registry generation without matching object checkpoint publication anchor
  -> reject that generation and select the previous valid published world
```

Recovery does not reconstruct or undo an unfinished semantic world. If
publication never occurred, the candidate world was never authoritative.

### Upgrade Semantics

Upgrade semantics are future-compatible lifecycle semantics, not required Phase
13 operations. The Phase 13 identity model must leave room for upgrade, but the
five Phase 13 slices do not need to implement or accept an upgrade operation.

When a future phase implements upgrade, it should preserve Package ObjectId and
create a new installed package revision that points at a new release digest and
manifest digest.

Upgrade transaction invariants:

- old release remains authoritative until new generation publishes;
- new schema revisions are retained before any instance can reference them;
- incompatible schema changes require a new SchemaDefinition identity or
  explicit migration/supersession relationship;
- failed upgrade recovery selects the old published package world.

### Uninstall Semantics

Uninstall is a state transition, not erasure of history.

For Phase 13, uninstall is denied while any launched process from the package
remains live. This avoids mixing durable uninstall with irreversible live
process teardown in the same slice.

Uninstall publication ordering:

```text
1. Deny new launches for the package while the uninstall publication is active.
2. Verify no package-launched Phase 9 process remains live.
3. Prepare candidate registry generation with Package state Tombstoned, active
   locator bindings removed, launchability removed, and reclaimable content
   identified.
4. Publish the tombstone generation and matching object/revision state as one
   semantic unit through `PackageTransactionCommitV0`.
5. Revoke or invalidate package launch-derived capability state as part of the
   published transition.
6. On recovery after publication, replay revocation/invalidation until complete.
7. On recovery before publication, select the previous installed world and do
   not claim uninstall success.
```

Uninstall published effects:

- revoke package-created launch capabilities according to existing capability
  revocation machinery;
- remove active locator bindings for launch and normal lookup;
- mark the Package object tombstoned;
- reclaim unreferenced immutable content extents;
- retain schema revisions still referenced by existing instances;
- retain package provenance and tombstone records;
- preserve enough registry state to interpret existing PackageDefinedObjects.

Failed uninstall recovery selects either the old installed world or the
fully published tombstone world. It must not leave partially revoked or
partially exposed package state.

## 7. Launch And Capability Grant Semantics

Launch flow:

```text
1. Resolve package / export locator from an explicit authorized namespace root.
2. Identify Package ObjectId.
3. Identify exact installed revision and release/content digest.
4. Confirm package state permits launch.
5. Resolve PackageExport manifest record.
6. Verify export content digest.
7. Verify PythTIG package content through the existing PythTIG verifier,
   preserving any verifier denial identity as a nested result.
8. Inspect declared capability requirements.
9. Receive explicit caller/owner supplied capability grant set.
10. Validate supplied grants against requirements and caller authority.
11. Create a Phase 9 process with only the explicit granted capabilities.
12. Record launch provenance and denial/audit events.
```

Launch denial must preserve this distinction:

```text
valid package / export, missing authority
  != invalid locator
  != missing package
  != corrupt content
  != failed PythTIG verification
```

Manifest requirements do not grant capabilities. Package identity does not
grant capabilities. Locators do not grant capabilities. A package export is
launched only with a grant set explicitly supplied and validated for that
launch.

If PythTIG verification fails during install or launch, the package layer may
record that the failure occurred during package install/launch verification,
but it must preserve the specific frozen PythTIG verifier denial identity as
the nested cause. A generic package denial must not erase the existing PythTIG
evidence contract.

## 8. Denial And Error Taxonomy

Denial identities are part of the contract, not incidental formatting. Phase
13 should add only the identities needed for stable acceptance tests, and the
implementation ADR must freeze any ABI-visible values before tests depend on
them.

Proposed denial taxonomy:

```text
PackageFormatInvalid
PackageFormatUnsupported
PackageBoundsExceeded
PackageOffsetOverflow
PackageSourceMissing
PackageSourceBoundsExceeded
PackageSourceDigestMismatch
PackageSourceReadAuthorityDenied
PackageDigestMismatch
PackageManifestDigestMismatch
PackageManifestInvalid
PackageLocatorInvalid
PackageNameCollision
PackageInstallAuthorityDenied
PackageQuotaDenied
PackageTransactionInProgress
PackageContentMissing
PackageContentDigestMismatch
PackagePythTigVerificationFailed
PackageSchemaDeclarationInvalid
PackageSchemaIdentityConflict
PackageSchemaRevisionMissing
PackageNotInstalled
PackageDisabled
PackageTombstoned
PackageExportMissing
PackageExportKindUnsupported
PackageLaunchAuthorityDenied
PackageCapabilityRequirementUnsatisfied
PackageUninstallAuthorityDenied
PackageUninstallBlockedByLiveProcess
PackageUninstallWouldForgetSchema
PackageRegistryGenerationInvalid
PackageTransactionAnchorInvalid
PackageRecoveryRolledBack
PackageRecoveryCommittedGenerationSelected
```

`PackageReactivationRequiresIdentity` is future-compatible terminology for a
later reactivation operation. It is not required for Phase 13 acceptance unless
the owner explicitly expands Phase 13 scope.

Recommended grouping:

- syntax/format denials before graph resolution;
- install authority and quota denials before publishing state;
- content integrity denials before launch;
- PythTIG verifier denials preserve the existing verifier identity as a nested
  cause;
- package lifecycle denials for disabled/tombstoned/not-installed states;
- launch authority denials distinct from missing export or corrupt content;
- recovery identities for published-vs-ignored-candidate evidence.

Do not claim package security from these identities alone. They prove logical
package lifecycle isolation under the existing object/capability model.

## 9. Package And Schema Tombstone Rules

Package lifecycle states:

```text
Installed
Disabled
Tombstoned
```

### Disable

Disable preserves Package ObjectId and installed revision history. For Phase
13, disable:

- prevents new launches;
- preserves Package ObjectId;
- does not terminate already-running package-launched processes;
- does not revoke capabilities from an already-running process;
- lets those processes continue under the capabilities granted at launch until
  exit;
- does not remove schema definitions, content required for interpretation, or
  provenance.

A future re-enable operation may preserve the same Package ObjectId because
the operation targets that identity, but Phase 13 does not need to implement
or accept re-enable.

### Upgrade

Upgrade semantics are future-compatible lifecycle semantics, not required Phase
13 operations. A future upgrade operation should preserve Package ObjectId,
create a new installed revision, and point to a new immutable release digest.
Compatible schema evolution may revise existing SchemaDefinition identities.
Incompatible schema evolution requires new schema identity or explicit
migration/supersession.

### Tombstone / Uninstall

Uninstall tombstones the Package ObjectId. It removes active locator bindings
and launchability, revokes package launch capabilities where applicable, and
reclaims unreferenced content. It retains provenance, package history,
schema-definition revisions still referenced by instances, and tombstone state.

### Future Reactivation

Reactivation semantics are future-compatible lifecycle semantics, not required
Phase 13 operations. The Phase 13 identity model must leave room for explicit
reactivation, but the five Phase 13 slices do not need to implement or accept a
reactivation operation.

When a future phase implements reactivation, it may preserve Package ObjectId
only when the operation targets the tombstoned package identity directly and
validates the intended release/content. A locator/name alone cannot resurrect
identity.

### Fresh Install After Uninstall

A fresh install discovered only by the same package locator/name must allocate
a new Package ObjectId. Same name does not imply same identity.

Until cryptographic publisher/package lineage exists, source provenance records
where content came from but does not prove publisher authenticity. Phase 13
must not pull package signing or remote trust into scope.

### Schema Durability

Schema lifecycle states should allow:

```text
Active
Superseded
RetainedForInstances
TombstonedButReferenced
```

A schema revision cannot be physically forgotten while any published
PackageDefinedObject references it. Schema uninstall cannot create semantic
amnesia.

The same rule applies to schema descriptor bytes. A retained schema revision
keeps a live reference to the content record and extents containing its
descriptor. Uninstall may remove launchability and reclaim unrelated package
content, but it cannot reclaim descriptor content required to interpret a
referenced schema revision.

## 10. Backward Compatibility Effects

Phase 13 must preserve existing storage and ABI evidence:

- Existing typed object records remain readable.
- Existing ObjectKind numeric values remain unchanged.
- Existing object shell note create/query/inspect/revise/history behavior
  remains unchanged.
- Existing PythTIG package format, checksum behavior, opcode set, and verifier
  denial identities remain unchanged.
- Existing Phase 12 locator behavior remains unchanged.
- Existing boot markers remain in order unless a future accepted ADR documents
  an explicit marker extension.
- If no package registry checkpoint exists, recovery treats the system as
  having no installed packages rather than corrupt storage.

Adding Package, SchemaDefinition, and PackageDefinedObject requires a future
implementation ADR before numeric ObjectKind values or serialized layouts are
assigned. That ADR must define migration/compatibility behavior for older
snapshots.

The package registry should be versioned independently:

```text
PackageRegistrySnapshotV0 absent
  -> no packages installed

PackageRegistrySnapshotV0 present and valid
  -> package lifecycle enabled

unknown registry major version
  -> deny package operations without weakening existing object store recovery
```

## 11. QEMU Acceptance Plan

A successful compile is not Phase 13 acceptance. Every slice needs automated
QEMU serial evidence, plus focused host/unit tests where appropriate.

The marker names below are proposed design markers, not implemented markers.

### Phase 13 Slice 1: package-format

Purpose:

```text
Define and validate canonical local package artifact format.
```

Acceptance:

- canonical package artifact encodes and decodes deterministically;
- artifact digest uses the zero-filled `artifact_sha256` digest domain;
- artifact digest and manifest digest are verified separately;
- hard package bounds reject oversized manifests, records, names, payloads,
  content tables, content bytes, and extent lists before allocation or offset
  arithmetic;
- ingress bounds reject excessive `INIT.PAK` package sources and source labels;
- invalid magic/version/length/digest/order/duplicate-name cases are denied
  before install state is mutated;
- manifest requirements are parsed as descriptions, not grants;
- package names/locators are validated as locators, not identity.

QEMU evidence:

```text
PYTHOS:CORE:PACKAGE_FORMAT_READY
```

Suggested tests:

```text
host: package format corpus encode/decode/digest tests
host: parser bounds and offset-overflow denial corpus
qemu: boot package-format fixture and denial corpus
```

### Phase 13 Slice 2: package-install

Purpose:

```text
Install one local package into Phase 10 storage as a crash-consistent semantic
publication transition.
```

Acceptance:

- installer obtains local package bytes through a bounded `INIT.PAK`
  package-source handle, not a POSIX path, remote registry, or network source;
- package source reads require `PackageSourceRead` authority and installation
  requires `PackageInstall` authority;
- candidate content is hidden before publication;
- all digests are verified before candidate validation and publication;
- PythTIG exports are verified before install acceptance and preserve nested
  verifier denial identities on failure;
- SchemaDefinition and Package object identities are created in the candidate
  package graph before publication and become authoritative only when selected
  by a valid `PackageTransactionCommitV0` anchor;
- registry generation and matching object/revision state publish atomically
  through a valid `PackageTransactionCommitV0` anchor;
- package locator visibility is derived from the selected published generation;
- reboot restores published package registry state;
- interrupted install before publication leaves the previous world selected and
  leaves candidate-only material unreachable/reclaimable;
- a crash after publication but before materialized locator mirror rebuild
  recovers by rebuilding mirrors from the published generation.

QEMU evidence:

```text
PYTHOS:CORE:PACKAGE_SOURCE_READY
PYTHOS:CORE:PACKAGE_SOURCE_AUTHORITY_READY
PYTHOS:CORE:PACKAGE_CANDIDATE_READY
PYTHOS:CORE:PACKAGE_CANDIDATE_VALIDATED
PYTHOS:CORE:PACKAGE_ANCHOR_PUBLISHED
PYTHOS:CORE:PACKAGE_WORLD_SELECTED
PYTHOS:CORE:PACKAGE_MIRRORS_REBUILT
PYTHOS:CORE:PACKAGE_INSTALL_READY
```

Suggested tests:

```text
qemu: successful INIT.PAK package-source install and reboot restore
qemu: deny package source read without PackageSourceRead capability
qemu/script: killed-before-anchor selects previous world with no exposed locator and candidate-only content reclaimable
qemu/script: killed-after-anchor-before-mirror rebuilds locator visibility from the published generation
qemu/script: mismatched object checkpoint / registry generation selects previous valid publication anchor
```

### Phase 13 Slice 3: package-launch

Purpose:

```text
Launch an installed package export as a Phase 9 process with explicit
capability grants.
```

Acceptance:

- launch resolves package/export through an authorized namespace root;
- launch identifies Package ObjectId and exact installed release digest;
- content digest is reverified before launch;
- PythTIG verifier runs before ring-3 entry and preserves nested verifier
  denial identities on failure;
- missing capability grant is denied distinctly from missing export;
- supplied valid grants create a Phase 9 process with only those grants;
- package launch records provenance/audit evidence.

QEMU evidence:

```text
PYTHOS:CORE:PACKAGE_LAUNCH_READY
PYTHOS:CORE:PACKAGE_LAUNCH_CAPABILITY_DENIED_READY
```

Suggested tests:

```text
qemu: launch installed tool export with explicit LOG/Object capability grant
qemu: deny same export without required grant
qemu: deny corrupt content before PythTIG runtime entry
```

### Phase 13 Slice 4: package-uninstall

Purpose:

```text
Disable, uninstall/tombstone, revoke launchability, and reclaim unreferenced
package content without semantic amnesia.
```

Acceptance:

- disable preserves Package ObjectId and blocks launch;
- disable does not terminate already-running package-launched processes or
  revoke capabilities already granted to those processes;
- uninstall is denied while package-launched Phase 9 processes remain live;
- uninstall tombstones Package ObjectId and removes active locator bindings;
- package-created launch capabilities are revoked through existing capability
  machinery;
- unreferenced content extents are reclaimed;
- schema revisions referenced by existing PackageDefinedObjects are retained;
- schema descriptor content/extents referenced by retained schema revisions are
  retained;
- fresh install by same locator after tombstone receives a new Package
  ObjectId;
- reboot restores tombstone/disable state.

QEMU evidence:

```text
PYTHOS:CORE:PACKAGE_DISABLE_READY
PYTHOS:CORE:PACKAGE_UNINSTALL_READY
PYTHOS:CORE:PACKAGE_REINSTALL_IDENTITY_READY
```

Suggested tests:

```text
qemu: disable blocks launch and preserves identity
qemu: disabled package does not terminate an already-running process or revoke its granted capabilities
qemu: live package process blocks uninstall
qemu: uninstall tombstones and reclaims unreferenced content
qemu: same-name fresh reinstall does not inherit old ObjectId
qemu: referenced schema revision and descriptor bytes remain readable after uninstall
```

### Phase 13 Slice 5: independently-authored-package

Purpose:

```text
Prove a package not compiled into the kernel test path can be installed,
launched, and removed through the Phase 13 lifecycle.
```

This replaces the stale `first-third-party-app` wording. The acceptance target
is an independently authored PythOS package/tool/service candidate, not a
desktop application.

Acceptance:

- package artifact is built separately from the kernel image;
- package artifact is delivered to PythOS through an `INIT.PAK` package-source
  record for Phase 13 acceptance;
- package declares at least one schema and one launchable tool export;
- install creates Package and SchemaDefinition evidence;
- launch runs the tool as a capability-scoped Phase 9 process;
- the launched tool explicitly creates a PackageDefinedObject using the
  installed SchemaDefinition;
- uninstall removes the package from active lifecycle while the created
  PackageDefinedObject remains durable;
- uninstall preserves the exact referenced SchemaDefinition revision,
  descriptor bytes, instance state, and provenance for existing instances;
- the final Phase 13 marker is emitted only after package lifecycle and schema
  extensibility pass.

QEMU evidence:

```text
PYTHOS:CORE:INDEPENDENT_PACKAGE_READY
PYTHOS:CORE:PACKAGE_SCHEMA_EXTENSIBILITY_READY
PYTHOS:CORE:PHASE_13_COMPLETE
```

Suggested tests:

```text
host: build independent fixture package artifact
qemu: install package, launch tool, create schema-defined object, uninstall, reboot, inspect retained instance and schema revision
```

## 12. Phase 13 To Phase 13.5 Stop Boundary

Phase 13 stops after:

```text
package format
-> local install
-> capability-scoped launch
-> uninstall / revocation
-> independently authored package
-> durable package-defined schema extensibility
-> PYTHOS:CORE:PHASE_13_COMPLETE
```

At that boundary:

- Rust object shell remains the persistent compatibility/recovery process.
- PythTIG service admission remains existing boot evidence, not a persistent
  packaged Pyth session runtime.
- No Waking graphics, presentation bridge, input bridge, Kai transport, Kai
  identity, WakeContext, AI authoring loop, remote registry, dependency
  resolver, package signing, native networking, OS update/recovery, SMP, or
  hardware expansion has begun.

The separate convergence milestone after Phase 13 is:

```text
packaged persistent Pyth session
-> long-lived supervised ring-3 execution
-> retained typed-service access
-> Rust object shell demoted to recovery / maintenance fallback
```

That later milestone must be invoked explicitly by the owner. Phase 13 design
must leave it possible, but Phase 13 acceptance must not depend on it.

## Design Review Questions Before Implementation

The Phase 13 implementation ADR should answer these before assigning ABI
numbers or writing code:

1. Are Package, SchemaDefinition, and PackageDefinedObject all necessary core
   kinds, or can one be represented by a smaller trusted primitive without
   weakening identity/lifecycle invariants?
2. What exact binary encoding is chosen for the package artifact and registry
   checkpoint?
3. Which denial identities are ABI-visible and therefore frozen?
4. How are Package and SchemaDefinition ObjectKind numeric values allocated
   without disturbing existing object storage evidence?
5. Which package registry relationships are persisted as materialized object
   relationships, and which are registry-only records?
6. What exact recovery script proves candidate-before-publication is ignored as
   non-authoritative and reclaimable?
7. What exact fixture proves independently authored package lifecycle without
   reviving the conventional application model?
