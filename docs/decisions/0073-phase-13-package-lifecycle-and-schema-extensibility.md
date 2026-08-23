# ADR 0073: Phase 13 Package Lifecycle And Schema Extensibility

Date: 2026-08-22
Status: Accepted for Phase 13 implementation

## Scope

This ADR freezes the Phase 13 implementation ABI and safety rails only.

Phase 13 implements:

```text
package-format
-> package-install
-> package-launch
-> package-uninstall
-> independently-authored-package
-> PYTHOS:CORE:PHASE_13_COMPLETE
```

Phase 13 does not implement Phase 13.5, persistent Pyth sessions,
presentation bridges, input bridges, Kai, WakeContext, networking, AI
integration, First Waking, remote registries, dependency resolution, package
signing, update/recovery, SMP, or new hardware work.

## Context

ADR 0066 rejects the conventional application, desktop, launcher, and window
authority model. ADR 0069 rejects POSIX-style namespace authority. Phase 13
therefore defines packages as graph-native, object-store-native,
capability-scoped install units.

The governing identity model is:

```text
Package names / locators locate.
Package ObjectId identifies.
Digests verify immutable package content / revisions.
Manifest relationships describe.
Capability grants authorize.
```

Package names, locators, export names, schema names, and content digests are
not package identity and do not grant authority.

## Decision

Phase 13 freezes the following ABI additions before implementation tasks may
consume them.

### Core Object Kinds

```rust
pub const OBJECT_KIND_PACKAGE: u16 = 30;
pub const OBJECT_KIND_SCHEMA_DEFINITION: u16 = 31;
pub const OBJECT_KIND_PACKAGE_DEFINED_OBJECT: u16 = 32;
```

These values occupy a distinct Phase 13 block after existing shell,
presentation, locator, and Task Steward object-kind codes. They do not change
any existing `ObjectKind` value.

### INIT.PAK Package Source Type

```rust
pub const TYPE_PACKAGE_SOURCE: u32 = 0x0000_0006;
```

The Phase 13 local ingress is an `INIT.PAK` package-source record. This record
identifies bounded package artifact bytes. It is not package identity and not
an authority grant.

### Package Source Handle

```rust
pub const PACKAGE_SOURCE_HANDLE_MAGIC: u32 = 0x5059_504B;
```

`PackageSourceHandle` is a `repr(C)` `u64` wrapper with this raw layout:

```text
bits 63..32 = PACKAGE_SOURCE_HANDLE_MAGIC
bits 31..16 = generation
bits 15..0  = source_id
```

The handle locates source bytes only. Reading requires a separate
`PackageSourceRead` capability. Installing requires a separate `PackageInstall`
capability.

### Package Capability Resources

Phase 13 uses the existing capability table, `ResourceId`, and `RightsMask`
bits. It does not add new `RightsMask` bits.

```rust
pub const PACKAGE_SOURCE_RESOURCE_ID: u64 = 0x5059_504B_4753_5243; // "PYPKGSRC"
pub const PACKAGE_INSTALL_RESOURCE_ID: u64 = 0x5059_504B_4749_4E53; // "PYPKGINS"
pub const PACKAGE_SOURCE_READ_RIGHTS: RightsMask = RightsMask::new(RightsMask::READ);
pub const PACKAGE_INSTALL_RIGHTS: RightsMask = RightsMask::new(RightsMask::WRITE);
```

### Package Context Syscall

Phase 13 does not resize `PythGraphBootstrapBlock`; it remains unchanged.
Package-launched Pyth runtimes obtain schema identity through a syscall tied
to the current process launch context:

```rust
pub const SYSCALL_PACKAGE_CONTEXT: u64 = 0x5059_0300;
pub const OP_PACKAGE_CONTEXT_SCHEMA: u16 = 1;
```

Call contract:

```text
syscall5(
    SYSCALL_PACKAGE_CONTEXT,
    OP_PACKAGE_CONTEXT_SCHEMA,
    schema_slot,
    output_ptr,
    output_len,
    0
)
```

`schema_slot = 0` is the default schema declared by the launched export.
`output_len` must equal `size_of::<PackageRuntimeSchemaBindingV0>()`.

### PackageRuntimeSchemaBindingV0

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageRuntimeSchemaBindingV0 {
    pub abi_major: u16,
    pub abi_minor: u16,
    pub schema_slot: u16,
    pub reserved0: u16,
    pub package_object_id: u64,
    pub package_revision: u64,
    pub schema_object_id: u64,
    pub schema_revision: u64,
    pub schema_descriptor_sha256: [u8; 32],
    pub reserved1: [u8; 16],
}
```

Layout is frozen:

```text
size = 88
align = 8
abi_major offset = 0
abi_minor offset = 2
schema_slot offset = 4
reserved0 offset = 6
package_object_id offset = 8
package_revision offset = 16
schema_object_id offset = 24
schema_revision offset = 32
schema_descriptor_sha256 offset = 40
reserved1 offset = 72
```

### PackageDefinedObject Create Buffer

`ObjectShellRequest` remains 80 bytes. `ObjectShellResponse` remains 64 bytes.
For package-defined creation, `ObjectShellRequest.input_ptr/input_len` points
to `PackageDefinedObjectCreateV0` only when:

```text
operation == OP_CREATE_OBJECT
object_kind == OBJECT_KIND_PACKAGE_DEFINED_OBJECT
```

```rust
pub const PACKAGE_DEFINED_OBJECT_CREATE_ABI_MAJOR: u16 = 0;
pub const PACKAGE_DEFINED_OBJECT_CREATE_ABI_MINOR: u16 = 1;
pub const PACKAGE_DEFINED_STATE_FORMAT_EMPTY: u16 = 0;
pub const PACKAGE_DEFINED_STATE_FORMAT_INLINE_BYTES_V0: u16 = 1;
pub const PACKAGE_DEFINED_MAX_INITIAL_STATE_BYTES: u64 = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageDefinedObjectCreateV0 {
    pub abi_major: u16,
    pub abi_minor: u16,
    pub state_format: u16,
    pub flags: u16,
    pub schema_object_id: u64,
    pub schema_revision: u64,
    pub initial_state_ptr: u64,
    pub initial_state_len: u64,
    pub reserved0: u64,
    pub reserved1: u64,
    pub reserved2: u64,
}
```

Layout is frozen:

```text
size = 64
align = 8
abi_major offset = 0
abi_minor offset = 2
state_format offset = 4
flags offset = 6
schema_object_id offset = 8
schema_revision offset = 16
initial_state_ptr offset = 24
initial_state_len offset = 32
reserved0 offset = 40
reserved1 offset = 48
reserved2 offset = 56
```

`flags`, `reserved0`, `reserved1`, and `reserved2` must be zero. Empty state
requires a null pointer and zero length. Inline state requires
`1 <= initial_state_len <= 16` and is copied through the existing user-copy
validation boundary.

### PackageDefinedObject Typed Fields

```rust
pub const FIELD_PACKAGE_SCHEMA_REF_V0: u16 = 0x1301;
pub const FIELD_PACKAGE_INLINE_STATE_V0: u16 = 0x1302;
```

`FIELD_PACKAGE_SCHEMA_REF_V0` stores:

```text
schema_object_id little-endian u64
schema_revision little-endian u64
```

`FIELD_PACKAGE_INLINE_STATE_V0` stores object-owned, revisioned mutable
inline state. It is never stored in the immutable package content store.

### Package Status Values

```rust
#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageStatus {
    Ok = 0,
    Denied = 1,
    NotFound = 2,
    BadRequest = 3,
    BufferTooSmall = 4,
    InvalidMagic = 100,
    UnsupportedMajor = 101,
    UnsupportedRequiredMinor = 102,
    InvalidOffset = 103,
    LengthOverflow = 104,
    BoundsExceeded = 105,
    DuplicateStableName = 106,
    DigestMismatch = 107,
    InvalidLocator = 108,
    SourceMissing = 200,
    SourceHandleInvalid = 201,
    SourceReadDenied = 202,
    InstallDenied = 203,
    InvalidManifest = 300,
    InvalidSchema = 301,
    PythTigVerificationFailed = 302,
    QuotaDenied = 303,
    TransactionAnchorMismatch = 304,
    RegistryWriteDenied = 305,
    PackageDisabled = 400,
    PackageTombstoned = 401,
    ExportMissing = 402,
    ContentCorrupt = 403,
    RequiredGrantMissing = 404,
    FinalCapabilityDenied = 405,
    LiveProcessExists = 500,
    SchemaRetained = 501,
    ContentRetained = 502,
    RegistryRecoveryDenied = 600,
}
```

Package-layer PythTIG failures use `PackageStatus::PythTigVerificationFailed`
and preserve the nested existing PythTIG verifier identity. They do not erase
the frozen PythTIG evidence contract.

### Package Format Bounds

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

### Package Content Allocator

The existing Phase 10 allocator remains unchanged. Phase 13 adds a
package-content-specific allocator:

```rust
pub const PACKAGE_CONTENT_BASE_SECTOR: u64 = 256;
pub const PACKAGE_CONTENT_MAX_BLOCKS: u16 = 8192;
pub const PACKAGE_CONTENT_BITMAP_WORDS: usize = 128;
pub const PACKAGE_CONTENT_MAX_STAGED_RECORDS: usize = 64;
```

The allocator tracks immutable content extents for candidate package worlds,
but allocation metadata must not become independently authoritative. Selected
anchored registry reachability determines which package-content extents are
live; candidate-only bytes are unreachable/reclaimable. Package content bytes
are immutable release-scoped bytes. Runtime instance state belongs to the
object service and remains object-owned and revisioned.

### Package Registry Snapshot

`PackageRegistrySnapshotV0` is the durable registry checkpoint representation.
It is subordinate to the object graph, not a second semantic authority:

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

`snapshot_crc32c` is CRC-32C Castagnoli over the exact snapshot bytes with the
`snapshot_crc32c` field zero-filled.

`PackageRegistrySnapshotV0` has a bounded encoded size of 32768 bytes. The
snapshot may include content records in Phase 13; those records are the durable
reachability description for immutable package-content bytes. A selected
published registry root makes referenced content extents live. A candidate
registry root that is not selected by a valid publication anchor does not make
its referenced content extents live.

`PackageRegistryContentRecordV0` is encoded after package and schema records:

```text
content_index                  u16
role                           u16
format                         u16
extent_count                   u16
package_object_id              u64
byte_len                       u64
release_digest                 [u8; 32]
sha256                         [u8; 32]
retention_count                u16
flags                          u16
reserved0                      u32
extent_list[32]                repeated { start_block u16, block_count u16 }
reserved1                      [u8; 32] zero-filled
```

The content record length is 256 bytes. `extent_count` must be no greater than
`MAX_CONTENT_EXTENTS_PER_RECORD`, every referenced extent must be inside
`PACKAGE_CONTENT_BASE_SECTOR..PACKAGE_CONTENT_BASE_SECTOR +
PACKAGE_CONTENT_MAX_BLOCKS`, and all reserved bytes must be zero. `content_id`
remains the tuple `(package_object_id, release_digest, content_index)`.

Phase 13 package-owned durable storage uses these regions:

```rust
pub const PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES: usize = 32 * 1024;
pub const PACKAGE_CANDIDATE_REGISTRY_SLOT_SECTORS: usize = 64;
pub const PACKAGE_CANDIDATE_REGISTRY_SLOT_A_SECTOR: u64 = 8500;
pub const PACKAGE_CANDIDATE_REGISTRY_SLOT_B_SECTOR: u64 = 8564;
pub const PACKAGE_PUBLICATION_ANCHOR_SLOT_A_SECTOR: u64 = 8628;
pub const PACKAGE_PUBLICATION_ANCHOR_SLOT_B_SECTOR: u64 = 8629;
```

The package-content byte region remains sectors `256..=8447`.
Object-service candidate checkpoints remain sectors `8448..=8499`. Registry
candidate slots and publication-anchor slots begin only after that object
candidate region and do not overlap content bytes or ordinary object
checkpoints.

The durable package-domain API must provide these storage surfaces before the
publication-boundary QEMU task consumes them:

```rust
pub fn write_candidate_registry_generation(
    device: BlockDeviceInfo,
    registry: &PackageRegistry,
) -> Result<PackageRegistryGeneration, PackageStatus>;

pub fn read_candidate_registry_generation(
    device: BlockDeviceInfo,
    expected: PackageRegistryGeneration,
) -> Result<PackageRegistry, PackageStatus>;

pub fn write_publication_anchor(
    device: BlockDeviceInfo,
    anchor: PackageTransactionCommitV0,
) -> Result<(), PackageStatus>;

pub fn read_publication_anchor_slot(
    device: BlockDeviceInfo,
    slot: PackagePublicationAnchorSlot,
) -> Result<Option<PackageTransactionCommitV0>, PackageStatus>;
```

`read_publication_anchor_slot` validates anchor layout and CRC only. Later
publication recovery must still load and validate the referenced object
candidate checkpoint and package-registry generation before selecting that
world.

### Cross-Checkpoint Publication Anchor

Normative Phase 13 transaction invariant:

```text
Uncommitted state was never reality.
```

Package installation is a world publication transition:

```text
world A remains authoritative
-> construct candidate world B durably
-> validate B
-> publish B with PackageTransactionCommitV0
-> B becomes authoritative
```

Durability does not imply publication. Validity does not imply publication.
Only a valid publication anchor selects reality.

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

Recovery accepts a package world only when the referenced object-service
candidate checkpoint and referenced package-registry generation match the same
valid `PackageTransactionCommitV0` publication anchor. Otherwise, recovery
selects the previous valid published world or denies package operations while
preserving older object-store behavior.

Ordinary object-service checkpoint semantics are unchanged. Normal object
checkpoints become recovery-eligible through the existing commit-sector
protocol, and ordinary recovery selects the newest valid committed object
checkpoint. Phase 13 adds a package-transaction path for durable,
root-addressable, verifiable ObjectService candidate state that is excluded
from ordinary recovery selection until selected by a valid
`PackageTransactionCommitV0`.

### Marker Order

The high-level Phase 13 marker tail is frozen as:

```text
PYTHOS:CORE:PHASE_12_COMPLETE
PYTHOS:CORE:PACKAGE_FORMAT_READY
PYTHOS:CORE:PACKAGE_INSTALL_READY
PYTHOS:CORE:PACKAGE_LAUNCH_READY
PYTHOS:CORE:PACKAGE_UNINSTALL_READY
PYTHOS:CORE:INDEPENDENT_PACKAGE_READY
PYTHOS:CORE:PACKAGE_SCHEMA_EXTENSIBILITY_READY
PYTHOS:CORE:PHASE_13_COMPLETE
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

Detailed slice scripts may require additional markers between these high-level
markers. Forbidden-marker checks remain part of each QEMU scenario.

## Consequences

The core ontology grows by three Phase 13 primitives while retaining the open
ontology above them. Kai, Seed, rooms, tools, memories, environments, and other
future concepts remain package-defined schemas/instances and do not require
new PythCore object-kind codes.

Package installation, launch, and uninstall are capability-scoped. A manifest
stating a requirement does not grant it. A package source handle does not grant
source reading. Locator publication is a rebuildable mirror, not semantic
truth.

Package content liveness is derived from the selected anchored package
registry. Physically written candidate package bytes are not live merely
because they exist; unpublished candidate content is unreachable/reclaimable.

Disable prevents new launches, preserves the Package `ObjectId`, does not
terminate already-running processes, and does not revoke capabilities from
already-running processes. Phase 13 uninstall is denied while package-launched
processes remain live.

## Verification

Phase 13 is accepted only when:

- compatibility tests freeze the new ABI values and preserve existing frozen
  ABI sizes;
- marker-contract tests prove Phase 13 marker order before framebuffer tail
  markers;
- package format, install, launch, uninstall, and independent package QEMU
  scripts verify required markers, forbidden markers, order, and
  `QEMU_OUTCOME success`;
- final boot evidence reaches `PYTHOS:CORE:PHASE_13_COMPLETE`;
- no Phase 13.5, Waking, Kai, networking, AI, SMP, or update/recovery markers
  are emitted.
