# Phase 13 Package Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Phase 13 only: graph-native package format, local install, capability-scoped launch, uninstall/revocation, and package-defined schema extensibility, ending at `PYTHOS:CORE:PHASE_13_COMPLETE`.

**Architecture:** Package names and locators locate. Package `ObjectId` identifies. Digests verify immutable package content and revisions. Manifest relationships describe. Capability grants authorize. Phase 13 packages are object-store-native install units, not desktop applications or global filesystem entries.

**Tech Stack:** Rust `no_std` PythCore/shared crates, existing PythTIG verifier and runtime, existing Phase 9 process model, existing Phase 10 storage/journal machinery, `INIT.PAK` local ingress, Python QEMU acceptance scripts.

**Spec:** `docs/superpowers/specs/2026-08-22-phase-13-package-schema-design.md`

## Global Constraints

- Do not implement Phase 13.5, persistent Pyth sessions, presentation/input bridges, Kai, WakeContext, networking, AI, autonomy, remote registries, dependency resolution, package signing, update/recovery, SMP, or hardware expansion.
- Do not revive desktops, launchers, windows, widgets, settings panels, conventional applications, POSIX directories, global filesystem authority, or ambient current-working-directory behavior.
- Preserve existing frozen ABI values and marker order unless ADR 0073 explicitly extends them.
- Treat `Package`, `SchemaDefinition`, and `PackageDefinedObject` as candidate core object kinds until ADR 0073 proves and assigns them.
- Preserve PythTIG v1 package bytes, opcode set, verifier error identities, canonicalization, limits, and checksum behavior.
- Use existing PythTIG verification before ring-3 launch and preserve nested verifier results inside package-layer denials.
- Serial/QEMU evidence is the acceptance oracle. A compile-only result is not acceptance.
- Every QEMU script added by this phase must verify exact required markers, forbidden markers, required ordering, and `QEMU_OUTCOME success`.
- Phase 13 markers are emitted after the current Phase 12 completion path and before the framebuffer tail markers.

## Locked Phase 13 Slice Sequence

```text
package-format
-> package-install
-> package-launch
-> package-uninstall
-> independently-authored-package
```

## Live Repository Findings That Constrain Execution

- `ObjectShellRequest` in `shared/src/object_shell_abi.rs` is frozen at 80 bytes. `input_ptr` is at offset 32 and `input_len` is at offset 40, so Phase 13 may place a typed payload behind those fields without resizing the request.
- `ObjectShellResponse` is frozen at 64 bytes and must not be resized for package responses.
- `ObjectKind` currently maps `1..11` to legacy shell/presentation/object-locator kinds and `20..25` to Task Steward kinds. Unknown kind codes are rejected in `core/src/typed_object_format.rs`.
- `core/src/syscall.rs::request_object_kind(kind: u16) -> Result<ObjectKind, ObjectServiceError>` currently admits only `OBJECT_KIND_NOTE`.
- `user/pyth-runtime/src/syscalls.rs::object_kind_from_graph(kind: &[u8]) -> Result<u16, HostError>` currently admits only `b"note"`.
- `tools/pythc/src/lower.rs::lower_object_kind(&mut self, expr: &Expression) -> Result<u32, Diagnostic>` currently maps source literal `1` to the UTF-8 graph token `b"note"`.
- `PythGraphBootstrapBlock` is frozen at 816 bytes and runtime validation requires exact ABI major/minor. Phase 13 does not change that bootstrap layout.
- The existing PythTIG `ObjectCreate` opcode has signature `[Effect, Capability, Utf8]`. Schema identity for `PackageDefinedObject` creation must not be added to PythTIG package bytes or opcodes.
- `shared/src/init_bundle.rs` supports record types `0x0000_0001` through `0x0000_0005`; the package-source record must be the next explicit record type.
- `core/src/object_relationships.rs` already contains `NameBinding` and `BindingTarget`; package locator publication must reuse these as rebuildable mirrors.
- `core/src/storage_allocator.rs::BlockAllocator` has `MAX_ALLOCATOR_BLOCKS = 64`, a `u64` bitmap, and `AllocatorJournal` capacity 8. At 512-byte sectors this represents only 32 KiB, not the accepted 4 MiB package-content limit.
- `core/src/storage_quotas.rs::StorageQuotaTable` has charge/release accounting but no durable reservation object. Phase 13 install can use an in-memory reservation before commit and a committed registry charge after anchor commit.
- `core/src/object_service_checkpoint.rs` provides two object checkpoint slots and generations, but no root digest or package transaction anchor.

## Storage Substrate Decision For ADR 0073

The current Phase 10 allocator cannot directly represent the accepted Phase 13 content model:

```text
4 MiB package content at 512 bytes/sector = 8192 sectors
current allocator max = 64 sectors
```

It also does not carry content-record extent lists, staged-but-uncommitted owner records, orphan-staging recovery records, or durable quota reservation records.

ADR 0073 must therefore define a package-content allocator inside `core/src/package_content_store.rs` with this exact Phase 13 interface:

```rust
pub const PACKAGE_CONTENT_BASE_SECTOR: u64 = 256;
pub const PACKAGE_CONTENT_MAX_BLOCKS: u16 = 8192;
pub const PACKAGE_CONTENT_BITMAP_WORDS: usize = 128;
pub const PACKAGE_CONTENT_MAX_STAGED_RECORDS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageExtent {
    pub start_block: u16,
    pub block_count: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageExtentList {
    pub extents: [Option<PackageExtent>; 32],
}

pub struct PackageExtentAllocator {
    base_sector: u64,
    block_count: u16,
    bitmap: [u64; PACKAGE_CONTENT_BITMAP_WORDS],
    committed_bitmap: [u64; PACKAGE_CONTENT_BITMAP_WORDS],
}

impl PackageExtentAllocator {
    pub fn new(base_sector: u64, block_count: u16) -> Result<Self, PackageError>;
    pub fn restore(base_sector: u64, block_count: u16, bitmap: [u64; PACKAGE_CONTENT_BITMAP_WORDS]) -> Result<Self, PackageError>;
    pub fn allocate_staged(&mut self, requested_blocks: u16) -> Result<PackageExtent, PackageError>;
    pub fn commit(&mut self) -> [u64; PACKAGE_CONTENT_BITMAP_WORDS];
    pub fn rollback_staged(&mut self);
    pub fn free_committed(&mut self, extent: PackageExtent) -> Result<(), PackageError>;
}
```

This is not a replacement for the Phase 10 allocator. It is a package-content-specific extension recorded by ADR 0073 and covered by its own TDD task.

## Phase 13 ABI To Freeze In Slice 0

ADR 0073 and `shared/src/package_abi.rs` freeze these exact names, layouts, and values before Slice 1 begins.

### Object Kind Values

```rust
pub const OBJECT_KIND_PACKAGE: u16 = 30;
pub const OBJECT_KIND_SCHEMA_DEFINITION: u16 = 31;
pub const OBJECT_KIND_PACKAGE_DEFINED_OBJECT: u16 = 32;
```

Justification: existing values occupy `1..11` and `20..25`; `30..32` creates a distinct Phase 13 block without changing old meanings.

### INIT.PAK Package Source Type

```rust
pub const TYPE_PACKAGE_SOURCE: u32 = 0x0000_0006;
```

### Package Source Handle

```rust
pub const PACKAGE_SOURCE_HANDLE_MAGIC: u32 = 0x5059_504B;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageSourceHandle {
    raw: u64,
}

impl PackageSourceHandle {
    pub const fn from_parts(source_id: u16, generation: u16) -> Self;
    pub const fn raw(self) -> u64;
    pub const fn source_id(self) -> u16;
    pub const fn generation(self) -> u16;
    pub const fn has_magic(self) -> bool;
}
```

Raw layout:

```text
bits 63..32 = PACKAGE_SOURCE_HANDLE_MAGIC
bits 31..16 = generation
bits 15..0  = source_id
```

### Package Capability Resources

Use the existing `core/src/capabilities.rs::ResourceId` and `RightsMask` bits. Do not add new `RightsMask` bits in Phase 13.

```rust
pub const PACKAGE_SOURCE_RESOURCE_ID: u64 = 0x5059_504B_4753_5243; // "PYPKGSRC"
pub const PACKAGE_INSTALL_RESOURCE_ID: u64 = 0x5059_504B_4749_4E53; // "PYPKGINS"

pub const PACKAGE_SOURCE_READ_RIGHTS: RightsMask = RightsMask::new(RightsMask::READ);
pub const PACKAGE_INSTALL_RIGHTS: RightsMask = RightsMask::new(RightsMask::WRITE);
```

### Package Runtime Context Syscall

Do not resize `PythGraphBootstrapBlock`. Package-launched Pyth runtimes obtain schema identity through a package-context syscall tied to the current process launch context.

```rust
pub const SYSCALL_PACKAGE_CONTEXT: u64 = 0x5059_0300;
pub const OP_PACKAGE_CONTEXT_SCHEMA: u16 = 1;

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

Layout requirements:

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

`schema_slot = 0` is the default schema declared by the launched export. The output buffer length must equal `size_of::<PackageRuntimeSchemaBindingV0>()`.

### PackageDefinedObject Create Buffer

`ObjectShellRequest.input_ptr/input_len` points to this structure only when `operation == OP_CREATE_OBJECT` and `object_kind == OBJECT_KIND_PACKAGE_DEFINED_OBJECT`.

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

Layout requirements:

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

Semantics:

- `flags`, `reserved0`, `reserved1`, and `reserved2` must be zero.
- `state_format == PACKAGE_DEFINED_STATE_FORMAT_EMPTY` requires `initial_state_ptr == 0` and `initial_state_len == 0`.
- `state_format == PACKAGE_DEFINED_STATE_FORMAT_INLINE_BYTES_V0` requires `1 <= initial_state_len <= 16`; PythCore copies bytes from `initial_state_ptr` through existing user copy validation and stores them in the object-owned typed record field described below.
- PythCore validates that `schema_object_id` names a live or retained `SchemaDefinition` and that `schema_revision` exactly matches a retained schema revision.
- The created object is a `PackageDefinedObject` whose identity is its `ObjectId`; the schema name is not identity.

### PackageDefinedObject Typed Fields

The compact typed-object record uses these Phase 13 field IDs:

```rust
pub const FIELD_PACKAGE_SCHEMA_REF_V0: u16 = 0x1301;
pub const FIELD_PACKAGE_INLINE_STATE_V0: u16 = 0x1302;
```

`FIELD_PACKAGE_SCHEMA_REF_V0` value is exactly:

```text
schema_object_id little-endian u64
schema_revision little-endian u64
```

`FIELD_PACKAGE_INLINE_STATE_V0` value is zero-padded initial state bytes with `value_len == initial_state_len`. This is object-owned, revisioned mutable state. It is never stored in the immutable package content store.

### Package Graph Object Interface

`core/src/object_service.rs` must expose these internal helpers:

```rust
pub struct PackageDefinedCreateInput<'a> {
    pub schema_object_id: ObjectId,
    pub schema_revision: u64,
    pub state_format: u16,
    pub initial_state: &'a [u8],
}

impl ObjectService {
    pub fn create_package_object(
        &mut self,
        caller: ActiveUserProcess,
        object_id: ObjectId,
        package_name_hint: &[u8],
    ) -> Result<ObjectCreateResult, ObjectServiceError>;

    pub fn create_schema_definition_object(
        &mut self,
        caller: ActiveUserProcess,
        object_id: ObjectId,
        defining_package: ObjectId,
        schema_revision: u64,
        descriptor_sha256: [u8; 32],
    ) -> Result<ObjectCreateResult, ObjectServiceError>;

    pub fn create_package_defined_object(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        input: PackageDefinedCreateInput<'_>,
    ) -> Result<ObjectCreateResult, ObjectServiceError>;
}
```

Public note creation remains:

```rust
pub fn create_object(
    &mut self,
    caller: ActiveUserProcess,
    workspace_capability: PackedCapability,
    object_kind: ObjectKind,
) -> Result<ObjectCreateResult, ObjectServiceError>;
```

Only `ObjectKind::Note` remains accepted by that legacy public helper.

### Package Status Values

`shared/src/package_abi.rs` freezes:

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

PythTIG failures record `PackageStatus::PythTigVerificationFailed` plus the nested existing PythTIG verifier identity.

## Module Interfaces

These are the interfaces later tasks consume. Keep names and signatures stable unless ADR 0073 is amended before Slice 1 starts.

```rust
// shared/src/package_format.rs
pub struct PackageArtifactV0<'a> { /* private fields */ }
pub struct ManifestV0<'a> { /* private fields */ }
pub struct ManifestRecordV0<'a> { /* private fields */ }
pub struct ContentEntryV0 { /* copyable metadata */ }

impl<'a> PackageArtifactV0<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, PackageFormatError>;
    pub fn artifact_sha256(&self) -> [u8; 32];
    pub fn manifest_sha256(&self) -> [u8; 32];
    pub fn manifest(&self) -> ManifestV0<'a>;
    pub fn content_entry(&self, index: u16) -> Option<ContentEntryV0>;
    pub fn content_bytes(&self, entry: ContentEntryV0) -> Result<&'a [u8], PackageFormatError>;
}
```

```rust
// core/src/package_source.rs
pub struct PackageSourceService<'a> { /* INIT.PAK-backed table */ }

impl<'a> PackageSourceService<'a> {
    pub fn from_init_bundle(bundle: &'a pythos_shared::init_bundle::InitBundle<'a>) -> Result<Self, PackageError>;
    pub fn handle_at(&self, ordinal: usize) -> Option<PackageSourceHandle>;
    pub fn read(
        &self,
        caller: ServiceId,
        capabilities: &CapabilityTable,
        handle: PackageSourceHandle,
        read_capability: CapabilityHandle,
        out: &mut [u8],
    ) -> Result<usize, PackageError>;
}
```

```rust
// core/src/package_content_store.rs
pub struct PackageContentStore { /* registry-restored immutable content */ }
pub struct PackageContentTransaction { /* staged content */ }
pub struct PackageContentCommit { pub content_count: u16, pub bitmap: [u64; PACKAGE_CONTENT_BITMAP_WORDS] }
pub type ContentId = u64;

impl PackageContentStore {
    pub fn new(base_sector: u64, block_count: u16) -> Result<Self, PackageError>;
    pub fn begin_install(&mut self, package_object_id: ObjectId, release_sha256: [u8; 32]) -> Result<PackageContentTransaction, PackageError>;
    pub fn stage(&mut self, tx: &mut PackageContentTransaction, role: PackageContentRole, digest: [u8; 32], bytes: &[u8]) -> Result<ContentId, PackageError>;
    pub fn commit(&mut self, tx: PackageContentTransaction) -> Result<PackageContentCommit, PackageError>;
    pub fn rollback(&mut self, tx: PackageContentTransaction);
    pub fn read_committed(&self, content_id: ContentId, out: &mut [u8]) -> Result<usize, PackageError>;
    pub fn retain(&mut self, content_id: ContentId) -> Result<(), PackageError>;
    pub fn release(&mut self, content_id: ContentId) -> Result<(), PackageError>;
}
```

```rust
// core/src/package_registry.rs
pub struct PackageRegistry { /* current committed generation */ }
pub struct PackageInstallDraft { /* package/schema/content/locator records before anchor */ }
pub struct PackageRegistryGeneration { pub generation: u64, pub root_digest: [u8; 32] }
pub struct PackageTransactionCommitV0 { /* fields from accepted design */ }

impl PackageRegistry {
    pub fn empty() -> Self;
    pub fn decode_snapshot(bytes: &[u8]) -> Result<Self, PackageError>;
    pub fn encode_snapshot(&self, out: &mut [u8]) -> Result<PackageRegistryGeneration, PackageError>;
    pub fn stage_install(&self, artifact: &PackageArtifactV0<'_>, content: &PackageContentCommit) -> Result<PackageInstallDraft, PackageError>;
    pub fn commit_install(&mut self, draft: PackageInstallDraft, anchor: PackageTransactionCommitV0) -> Result<PackageRegistryGeneration, PackageError>;
    pub fn disable(&mut self, package_object_id: ObjectId) -> Result<PackageRegistryGeneration, PackageError>;
    pub fn uninstall(&mut self, package_object_id: ObjectId) -> Result<PackageRegistryGeneration, PackageError>;
    pub fn export_for_locator(&self, locator: &crate::object_locator::ObjectLocator) -> Result<PackageExportRecord, PackageError>;
}
```

```rust
// core/src/package_service.rs
pub struct PackageService { /* source/content/registry/object-service orchestration */ }

impl PackageService {
    pub fn install(&mut self, request: PackageInstallRequest<'_>) -> Result<PackageInstallResult, PackageError>;
    pub fn launch(&mut self, request: PackageLaunchRequest<'_>) -> Result<PackageLaunchResult, PackageError>;
    pub fn disable(&mut self, package_object_id: ObjectId) -> Result<(), PackageError>;
    pub fn uninstall(&mut self, package_object_id: ObjectId) -> Result<(), PackageError>;
    pub fn recover(&mut self) -> Result<PackageRecoveryReport, PackageError>;
    pub fn runtime_schema_binding(&self, process: ActiveUserProcess, schema_slot: u16) -> Result<PackageRuntimeSchemaBindingV0, PackageError>;
}
```

## Files And Responsibilities

### Documentation

- `docs/decisions/0073-phase-13-package-lifecycle-and-schema-extensibility.md`: implementation ADR, numeric ABI assignments, package format, registry checkpoint, package create-buffer ABI, denial taxonomy, marker contract.
- `docs/ROADMAP.md`: Phase 13 slice status after acceptance evidence.
- `docs/HANDOVER.md`: final evidence and Phase 13 -> Phase 13.5 boundary after the final slice passes.
- `docs/technical-overview.md`: narrow Phase 13 update after code and evidence pass.

### Shared Crate

- `shared/src/lib.rs`: exports package modules.
- `shared/src/sha256.rs`: no_std SHA-256 primitive.
- `shared/src/package_format.rs`: canonical package artifact parser/encoder.
- `shared/src/package_abi.rs`: status codes, handles, source records, runtime schema binding, create buffer ABI.
- `shared/src/init_bundle.rs`: package-source record type.
- `shared/src/object_shell_abi.rs`: object kind constants only; no struct resizing.

### Core

- `core/src/shell_objects.rs`: new object kind variants after ADR 0073.
- `core/src/typed_object_format.rs`: new kind encode/decode mappings after ADR 0073.
- `core/src/object_service.rs`: internal package/schema/object creation helpers.
- `core/src/object_service_checkpoint.rs`: checkpoint root digest support.
- `core/src/object_relationships.rs`: reuse `NameBinding`/`BindingTarget` mirrors.
- `core/src/object_locator.rs`: consume existing resolver semantics.
- `core/src/package_source.rs`: package source table and authority.
- `core/src/package_content_store.rs`: package content extent allocator and immutable content store.
- `core/src/package_registry.rs`: registry snapshot, generations, anchors, lifecycle records.
- `core/src/package_service.rs`: install, launch, disable, uninstall, recovery orchestration.
- `core/src/package_acceptance.rs`: feature-gated QEMU acceptance flow.
- `core/src/pyth_graph_loader.rs`: package-contained PythTIG verification helper.
- `core/src/pyth_runtime_launch.rs`: package launch helper using existing runtime path.
- `core/src/syscall.rs`: package-context syscall and PackageDefinedObject object-create dispatch.
- `core/src/main.rs`: Phase 13 acceptance entrypoint ordering.
- `core/Cargo.toml`: `phase13-package-test` feature.

### User Runtime And Compiler

- `tools/pythc/src/lower.rs`: source literal `2` maps to `b"package-defined"`.
- `tools/pythc/tests/lower.rs`: compiler token test.
- `user/pyth-runtime/src/syscalls.rs`: package context syscall and `PackageDefinedObjectCreateV0` request.
- `user/pyth-runtime/src/interpreter.rs`: no opcode changes; host call continues through `Host::object_create`.
- `programs/phase13/create-seed.pyth`: independent package fixture.
- `programs/phase13/schemas/seed.v0.schema`: schema descriptor fixture.

### Scripts And Tests

- `scripts/build-image.py`: `INIT.PAK` package-source records.
- `scripts/build-phase13-package-fixture.py`: deterministic package fixture builder.
- `scripts/test-phase13-package-format.py`
- `scripts/test-phase13-package-install.py`
- `scripts/test-phase13-package-launch.py`
- `scripts/test-phase13-package-uninstall.py`
- `scripts/test-phase13-independent-package.py`
- `scripts/test-boot.py`: Phase 13 marker integration.
- `tests/test_boot_marker_contract.py`
- `tests/test_interface_compatibility_freeze.py`
- `tests/fixtures/interface_compatibility_freeze.json`
- `tests/test_iso_image.py`

---

## Slice 0: Implementation ADR And Frozen Interfaces

### Task 0.1: ADR 0073 Freezes ABI Values And Layouts

**Files:**
- Create: `docs/decisions/0073-phase-13-package-lifecycle-and-schema-extensibility.md`
- Modify: none
- Test: human review of ADR text before code-bearing tasks

**Interfaces Consumed:** accepted design document.

**Interfaces Produced:** exact ABI contract listed in this plan: object kind values `30..32`, `TYPE_PACKAGE_SOURCE = 0x0000_0006`, `SYSCALL_PACKAGE_CONTEXT = 0x5059_0300`, `PackageSourceHandle`, `PackageRuntimeSchemaBindingV0`, `PackageDefinedObjectCreateV0`, package status values, package marker order, package content allocator constants.

- [ ] **Step 1: Write the ADR**

Include these sections verbatim by contract:

```text
Status: Accepted for Phase 13 implementation
Scope: Phase 13 only
Object kinds: Package=30, SchemaDefinition=31, PackageDefinedObject=32
INIT.PAK package source type: 0x00000006
Package context syscall: 0x50590300
PackageDefinedObject create buffer: PackageDefinedObjectCreateV0 size 64
Package content allocator: 8192 blocks, 128 bitmap words
PythTIG v1 package/opcode ABI: unchanged
PythGraphBootstrapBlock: unchanged
```

- [ ] **Step 2: Review failure mode**

Run:

```powershell
rg -n "PackageDefinedObjectCreateV0|OBJECT_KIND_PACKAGE_DEFINED_OBJECT|SYSCALL_PACKAGE_CONTEXT|TYPE_PACKAGE_SOURCE" docs/decisions/0073-phase-13-package-lifecycle-and-schema-extensibility.md
```

Expected before writing: command fails with no matches. Expected after writing: all four names match.

- [ ] **Step 3: Commit**

```powershell
git add docs/decisions/0073-phase-13-package-lifecycle-and-schema-extensibility.md
git commit -m "docs: accept Phase 13 implementation ABI"
```

### Task 0.2: Compatibility Freeze Tests For New ABI

**Files:**
- Modify: `tests/fixtures/interface_compatibility_freeze.json`
- Modify: `tests/test_interface_compatibility_freeze.py`
- Test: `tests/test_interface_compatibility_freeze.py`

**Interfaces Consumed:** ADR 0073 constants and layouts.

**Interfaces Produced:** failing compatibility expectations for Phase 13 additions while preserving old values.

- [ ] **Step 1: Write the failing test**

Add fixture entries:

```json
"phase13_package_abi": {
  "object_kind_package": 30,
  "object_kind_schema_definition": 31,
  "object_kind_package_defined_object": 32,
  "init_bundle_type_package_source": 6,
  "syscall_package_context": 1348010752,
  "package_defined_object_create_v0_size": 64,
  "package_runtime_schema_binding_v0_size": 88
}
```

Add Python assertions that read the relevant Rust source constants and assert those values while reasserting `ObjectShellRequest` remains 80 bytes and `PythGraphBootstrapBlock` remains 816 bytes.

- [ ] **Step 2: Run test to verify it fails**

```powershell
python -m unittest tests.test_interface_compatibility_freeze
```

Expected: FAIL because package constants and structs do not exist yet.

- [ ] **Step 3: Minimum implementation**

Create only enough shared/core constants and empty layout structs to satisfy the freeze test. Do not add behavior.

- [ ] **Step 4: Run test to verify it passes**

```powershell
python -m unittest tests.test_interface_compatibility_freeze
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add tests/fixtures/interface_compatibility_freeze.json tests/test_interface_compatibility_freeze.py shared/src/package_abi.rs shared/src/lib.rs shared/src/object_shell_abi.rs
git commit -m "test: freeze Phase 13 package ABI surface"
```

### Task 0.3: Marker Contract Tests

**Files:**
- Modify: `tests/test_boot_marker_contract.py`
- Modify: `scripts/test-boot.py`

**Interfaces Consumed:** ADR 0073 marker list.

**Interfaces Produced:** marker ordering contract for Phase 13.

- [ ] **Step 1: Write the failing test**

Add `PHASE13_MARKERS`:

```python
PHASE13_MARKERS = [
    "PYTHOS:CORE:PACKAGE_FORMAT_READY",
    "PYTHOS:CORE:PACKAGE_INSTALL_READY",
    "PYTHOS:CORE:PACKAGE_LAUNCH_READY",
    "PYTHOS:CORE:PACKAGE_UNINSTALL_READY",
    "PYTHOS:CORE:INDEPENDENT_PACKAGE_READY",
    "PYTHOS:CORE:PACKAGE_SCHEMA_EXTENSIBILITY_READY",
    "PYTHOS:CORE:PHASE_13_COMPLETE",
]
```

Assert `PYTHOS:CORE:PHASE_12_COMPLETE` precedes the first Phase 13 marker and `PYTHOS:CORE:FRAMEBUFFER_READY` follows `PYTHOS:CORE:PHASE_13_COMPLETE`.

- [ ] **Step 2: Run test to verify it fails**

```powershell
python -m unittest tests.test_boot_marker_contract
```

Expected: FAIL because Phase 13 markers are not present in `scripts/test-boot.py`.

- [ ] **Step 3: Minimum implementation**

Add marker names to the script contract only. Do not emit them from PythCore yet.

- [ ] **Step 4: Run test to verify it passes**

```powershell
python -m unittest tests.test_boot_marker_contract
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add tests/test_boot_marker_contract.py scripts/test-boot.py
git commit -m "test: define Phase 13 marker contract"
```

---

## Slice 1: package-format

### Task 1.1: SHA-256 Primitive

**Files:**
- Create: `shared/src/sha256.rs`
- Modify: `shared/src/lib.rs`
- Test: `shared/src/sha256.rs`

**Interfaces Consumed:** none.

**Interfaces Produced:**

```rust
pub fn sha256(bytes: &[u8]) -> [u8; 32];

pub struct Sha256 {
    state: [u32; 8],
    len_bytes: u64,
    buffer: [u8; 64],
    buffer_len: usize,
}

impl Sha256 {
    pub const fn new() -> Self;
    pub fn update(&mut self, bytes: &[u8]);
    pub fn finalize(self) -> [u8; 32];
}
```

- [ ] **Step 1: Write the failing tests**

Add tests for empty message, `abc`, a multi-block NIST vector, 55/56/63/64/65-byte padding boundaries, chunked equals one-shot, and a fixture where 32 bytes are zero-filled before hashing.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-shared sha256
```

Expected: FAIL because `shared::sha256` does not exist.

- [ ] **Step 3: Minimum implementation**

Implement SHA-256 compression, padding, one-shot, and incremental APIs in `no_std` Rust.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-shared sha256
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add shared/src/lib.rs shared/src/sha256.rs
git commit -m "feat(shared): add no_std sha256 primitive"
```

### Task 1.2: Package Artifact Header Parser

**Files:**
- Create: `shared/src/package_format.rs`
- Modify: `shared/src/lib.rs`
- Test: `shared/src/package_format.rs`

**Interfaces Consumed:** `shared::sha256::sha256`.

**Interfaces Produced:** `PackageArtifactV0::parse`, `PackageFormatError`, package format bounds.

- [ ] **Step 1: Write the failing test**

Add `package_header_validates_zero_filled_artifact_digest_domain` using a hand-built `PYTHPKG0` byte array whose `artifact_sha256` field is zero during hashing and restored after hashing.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-shared package_header_validates_zero_filled_artifact_digest_domain
```

Expected: FAIL because `PackageArtifactV0` is undefined.

- [ ] **Step 3: Minimum implementation**

Parse the fixed header, enforce magic/version/header length, check half-open ranges with overflow checks, verify `manifest_sha256`, verify zero-filled `artifact_sha256`, and return borrowed manifest/content regions.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-shared package_header_validates_zero_filled_artifact_digest_domain
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add shared/src/lib.rs shared/src/package_format.rs
git commit -m "feat(shared): parse Phase 13 package artifact header"
```

### Task 1.3: Manifest And Content Table Parser

**Files:**
- Modify: `shared/src/package_format.rs`
- Test: `shared/src/package_format.rs`

**Interfaces Consumed:** `PackageArtifactV0::parse`.

**Interfaces Produced:** `ManifestV0`, `ManifestRecordV0`, `ContentEntryV0`, `PackageArtifactV0::content_entry`, `PackageArtifactV0::content_bytes`.

- [ ] **Step 1: Write the failing tests**

Add tests that reject duplicate stable names, unsorted records, oversized stable names, oversized payloads, excessive manifest records, excessive content entries, excessive extent count, and content ranges outside `content_bytes`.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-shared package_format
```

Expected: FAIL because manifest/content parsing is missing.

- [ ] **Step 3: Minimum implementation**

Implement canonical manifest record iteration, stable-name ordering, content-entry parsing, per-content digest metadata, and all bounds from the accepted design.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-shared package_format
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add shared/src/package_format.rs
git commit -m "feat(shared): parse package manifest and content records"
```

### Task 1.4: INIT.PAK Package Source Records

**Files:**
- Modify: `shared/src/init_bundle.rs`
- Modify: `tests/test_iso_image.py`
- Test: `shared/src/init_bundle.rs`, `tests/test_iso_image.py`

**Interfaces Consumed:** `TYPE_PACKAGE_SOURCE = 0x0000_0006`.

**Interfaces Produced:** `RecordType::PackageSource`.

- [ ] **Step 1: Write the failing tests**

Add Rust tests that parse a package-source record and reject a seventeenth total `INIT.PAK` record. Add Python ISO test coverage that expects package source records to appear only when requested by `build-image.py`.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-shared init_bundle
python -m unittest tests.test_iso_image
```

Expected: FAIL because `TYPE_PACKAGE_SOURCE` and build support do not exist.

- [ ] **Step 3: Minimum implementation**

Add `TYPE_PACKAGE_SOURCE`, `RecordType::PackageSource`, and parser admission. Do not change existing record numbers.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-shared init_bundle
python -m unittest tests.test_iso_image
```

Expected: PASS after the next task adds script support; if only Rust passes here, keep the Python test failing and complete Task 1.5 in the same review batch.

- [ ] **Step 5: Commit**

```powershell
git add shared/src/init_bundle.rs tests/test_iso_image.py
git commit -m "feat(shared): add INIT.PAK package source record type"
```

### Task 1.5: Package Fixture Builder And Image Ingress

**Files:**
- Create: `scripts/build-phase13-package-fixture.py`
- Modify: `scripts/build-image.py`
- Modify: `tests/test_iso_image.py`
- Test: fixture builder self-test and ISO image tests

**Interfaces Consumed:** `PackageArtifactV0` byte layout, `TYPE_PACKAGE_SOURCE`.

**Interfaces Produced:** `--phase13-package-source <path[:label]>`, `--with-phase13-package-format-fixture`.

- [ ] **Step 1: Write the failing test**

Add `tests.test_iso_image` assertions that `build-image.py --with-phase13-package-format-fixture` emits exactly one package-source record with label length <= 48 and that nine requested package sources are rejected.

- [ ] **Step 2: Run test to verify it fails**

```powershell
python -m unittest tests.test_iso_image
```

Expected: FAIL because script flags do not exist.

- [ ] **Step 3: Minimum implementation**

Implement deterministic package fixture building and `INIT.PAK` package-source record insertion. Enforce `MAX_PACKAGE_SOURCES = 8` and `MAX_PACKAGE_SOURCE_LABEL_BYTES = 48`.

- [ ] **Step 4: Run test to verify it passes**

```powershell
python scripts/build-phase13-package-fixture.py --self-test
python -m unittest tests.test_iso_image
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add scripts/build-phase13-package-fixture.py scripts/build-image.py tests/test_iso_image.py
git commit -m "feat(scripts): add Phase 13 package fixture ingress"
```

### Task 1.6: QEMU package-format Acceptance

**Files:**
- Create: `scripts/test-phase13-package-format.py`
- Create: `core/src/package_acceptance.rs`
- Modify: `core/src/main.rs`
- Modify: `core/Cargo.toml`
- Test: `scripts/test-phase13-package-format.py`

**Interfaces Consumed:** package parser, package-source `INIT.PAK` records.

**Interfaces Produced:** `phase13-package-test` feature and package-format serial markers.

- [ ] **Step 1: Write the failing QEMU script**

Script requires:

```text
PYTHOS:CORE:PACKAGE_SOURCE_READY
PYTHOS:CORE:PACKAGE_FORMAT:VALID
PYTHOS:CORE:PACKAGE_FORMAT:INVALID_DENIED
PYTHOS:CORE:PACKAGE_FORMAT_READY
QEMU_OUTCOME success
```

Forbidden:

```text
PYTHOS:PANIC
PYTHOS:CORE:PACKAGE_INSTALL:STAGED
PYTHOS:CORE:PACKAGE_INSTALL:COMMITTED
PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED
```

- [ ] **Step 2: Run script to verify it fails**

```powershell
python scripts/test-phase13-package-format.py
```

Expected: FAIL because no PythCore package-format acceptance path emits the markers.

- [ ] **Step 3: Minimum implementation**

Add a feature-gated acceptance path that validates package-source fixtures and exits QEMU successfully after `PACKAGE_FORMAT_READY`. It must not mutate package registry, content store, object service, or locator state.

- [ ] **Step 4: Run script to verify it passes**

```powershell
python scripts/test-phase13-package-format.py
```

Expected: PASS with `QEMU_OUTCOME success`.

- [ ] **Step 5: Commit**

```powershell
git add scripts/test-phase13-package-format.py core/src/package_acceptance.rs core/src/main.rs core/Cargo.toml
git commit -m "test(qemu): prove Phase 13 package format acceptance"
```

---

## Slice 2: package-install

### Task 2.1: PackageSourceHandle ABI

**Files:**
- Modify: `shared/src/package_abi.rs`
- Test: `shared/src/package_abi.rs`

**Interfaces Consumed:** ADR 0073 `PackageSourceHandle` raw layout.

**Interfaces Produced:** `PackageSourceHandle`.

- [ ] **Step 1: Write the failing test**

Test that `PackageSourceHandle::from_parts(7, 3).raw()` has high 32 bits `0x5059504B`, `source_id() == 7`, `generation() == 3`, and `has_magic() == true`.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-shared package_source_handle
```

Expected: FAIL because methods are missing.

- [ ] **Step 3: Minimum implementation**

Implement the raw layout exactly.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-shared package_source_handle
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add shared/src/package_abi.rs
git commit -m "feat(shared): define package source handles"
```

### Task 2.2: Package Source Service Authority

**Files:**
- Create: `core/src/package_source.rs`
- Modify: `core/src/main.rs`
- Test: `core/src/package_source.rs`

**Interfaces Consumed:** `PackageSourceHandle`, `InitBundle`.

**Interfaces Produced:** `PackageSourceService::from_init_bundle`, `handle_at`, `read`.

- [ ] **Step 1: Write the failing tests**

Add tests:

```rust
#[test]
fn source_handle_without_read_capability_is_denied() { /* expects PackageStatus::SourceReadDenied */ }

#[test]
fn source_read_copies_exact_bounded_bytes() { /* expects fixture bytes and exact length */ }
```

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_source
```

Expected: FAIL because `package_source` module is missing.

- [ ] **Step 3: Minimum implementation**

Build a source table from `INIT.PAK`, verify handle magic/source/generation, validate `PackageSourceRead` using `CapabilityTable::validate`, and copy bytes into caller-provided output.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_source
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_source.rs core/src/main.rs
git commit -m "feat(core): gate package source reads by capability"
```

### Task 2.3: Package Content Extent Allocator

**Files:**
- Create: `core/src/package_content_store.rs`
- Modify: `core/src/main.rs`
- Test: `core/src/package_content_store.rs`

**Interfaces Consumed:** `PACKAGE_CONTENT_MAX_BLOCKS`, `PackageExtent`.

**Interfaces Produced:** `PackageExtentAllocator`.

- [ ] **Step 1: Write the failing tests**

Add tests that allocate 8192 one-block extents, reject the 8193rd block, roll back staged blocks without mutating committed bitmap, commit staged blocks, restore from bitmap, and reject extents outside 8192 blocks.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_extent_allocator
```

Expected: FAIL because allocator type is missing.

- [ ] **Step 3: Minimum implementation**

Implement `[u64; 128]` bitmap allocation, rollback, commit, restore, and free.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_extent_allocator
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_content_store.rs core/src/main.rs
git commit -m "feat(core): add package content extent allocator"
```

### Task 2.4: Content Staging And Digest Commit

**Files:**
- Modify: `core/src/package_content_store.rs`
- Test: `core/src/package_content_store.rs`

**Interfaces Consumed:** `PackageExtentAllocator`, `sha256`.

**Interfaces Produced:** `PackageContentStore`, `PackageContentTransaction`, `PackageContentCommit`, `ContentId`.

- [ ] **Step 1: Write the failing tests**

Add tests that stage bytes, reject digest mismatch, hide staged content from `read_committed`, expose content after `commit`, reclaim staged extents on `rollback`, and keep `content_id` scoped to one package/release.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_content_store
```

Expected: FAIL because staging methods are missing.

- [ ] **Step 3: Minimum implementation**

Implement content transaction records with package object id, release digest, role, digest, extent list, byte length, retention count, and committed flag.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_content_store
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_content_store.rs
git commit -m "feat(core): stage immutable package content"
```

### Task 2.5: Package Registry V0 Encoding

**Files:**
- Create: `core/src/package_registry.rs`
- Modify: `core/src/main.rs`
- Test: `core/src/package_registry.rs`

**Interfaces Consumed:** `PackageContentCommit`, `PackageArtifactV0`.

**Interfaces Produced:** `PackageRegistry::encode_snapshot`, `decode_snapshot`, `PackageRegistryGeneration`.

- [ ] **Step 1: Write the failing tests**

Add tests for canonical record sorting, CRC-32C zero-filled `snapshot_crc32c`, unknown major denial, unsupported required minor flag denial, and round-trip of one package plus one schema record.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_registry
```

Expected: FAIL because registry module is missing.

- [ ] **Step 3: Minimum implementation**

Implement snapshot V0 encode/decode and root digest calculation over canonical snapshot bytes.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_registry
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_registry.rs core/src/main.rs
git commit -m "feat(core): encode package registry snapshots"
```

### Task 2.6: Registry Dual-Generation Recovery

**Files:**
- Modify: `core/src/package_registry.rs`
- Test: `core/src/package_registry.rs`

**Interfaces Consumed:** registry snapshot V0.

**Interfaces Produced:** `PackageRegistry::select_generation`.

- [ ] **Step 1: Write the failing tests**

Add tests that select the highest valid generation, ignore corrupt CRC generation, ignore unsupported major generation, and return `RegistryRecoveryDenied` when no valid package generation exists while object store recovery remains separate.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_registry_recovery
```

Expected: FAIL because generation selection is missing.

- [ ] **Step 3: Minimum implementation**

Implement two-slot selection over decoded snapshot candidates.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_registry_recovery
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_registry.rs
git commit -m "feat(core): recover package registry generations"
```

### Task 2.7: Object Checkpoint Root Identity

**Files:**
- Modify: `core/src/object_service_checkpoint.rs`
- Test: `core/src/object_service_checkpoint.rs`

**Interfaces Consumed:** existing object checkpoint slot image.

**Interfaces Produced:**

```rust
pub struct ObjectCheckpointIdentity {
    pub generation: u64,
    pub root_digest: [u8; 32],
}

pub fn object_checkpoint_identity(snapshot: &ObjectServiceSnapshot) -> ObjectCheckpointIdentity;
```

- [ ] **Step 1: Write the failing tests**

Add tests that identical snapshots produce identical root digests, changed object record changes digest, and generation is preserved.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core object_checkpoint_identity
```

Expected: FAIL because identity helper is missing.

- [ ] **Step 3: Minimum implementation**

Compute SHA-256 over canonical encoded object checkpoint contents without changing existing slot sectors or commit behavior.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core object_checkpoint_identity
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/object_service_checkpoint.rs
git commit -m "feat(core): identify object checkpoint roots"
```

### Task 2.8: PackageTransactionCommitV0 Anchor

**Files:**
- Modify: `core/src/package_registry.rs`
- Test: `core/src/package_registry.rs`

**Interfaces Consumed:** `ObjectCheckpointIdentity`, `PackageRegistryGeneration`.

**Interfaces Produced:** `PackageTransactionCommitV0::new`, `encode`, `decode`.

- [ ] **Step 1: Write the failing tests**

Add tests that CRC-32C uses zero-filled `commit_crc32c`, anchor decode rejects wrong object digest, rejects wrong registry digest, and accepts the exact paired object generation plus registry generation.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_transaction_anchor
```

Expected: FAIL because anchor type is missing.

- [ ] **Step 3: Minimum implementation**

Implement the anchor with fields from the accepted design and pair-check helper.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_transaction_anchor
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_registry.rs
git commit -m "feat(core): bind package registry to object checkpoints"
```

### Task 2.9: Package And SchemaDefinition Object Creation

**Files:**
- Modify: `core/src/shell_objects.rs`
- Modify: `core/src/typed_object_format.rs`
- Modify: `core/src/object_service.rs`
- Test: `core/src/typed_object_format.rs`, `core/src/object_service.rs`

**Interfaces Consumed:** object kind constants `30` and `31`.

**Interfaces Produced:** `ObjectKind::Package`, `ObjectKind::SchemaDefinition`, `ObjectService::create_package_object`, `ObjectService::create_schema_definition_object`.

- [ ] **Step 1: Write the failing tests**

Add tests that package/schema kinds round-trip with codes `30`/`31`, unknown code still denies, public `create_object(..., ObjectKind::Package)` returns `UnsupportedKind`, and internal helpers create package/schema objects with revisions.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_schema_object_creation
```

Expected: FAIL because object kinds/helpers are missing.

- [ ] **Step 3: Minimum implementation**

Add enum variants, kind mappings, and internal helpers. Keep public arbitrary create denied.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_schema_object_creation
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/shell_objects.rs core/src/typed_object_format.rs core/src/object_service.rs
git commit -m "feat(core): create package and schema definition objects"
```

### Task 2.10: Install Transaction Orchestration

**Files:**
- Create: `core/src/package_service.rs`
- Modify: `core/src/main.rs`
- Modify: `core/src/package_registry.rs`
- Modify: `core/src/package_content_store.rs`
- Test: `core/src/package_service.rs`

**Interfaces Consumed:** source service, artifact parser, content store, registry, object helpers, anchor.

**Interfaces Produced:** `PackageService::install`.

- [ ] **Step 1: Write the failing test**

Add `install_commits_package_schema_content_registry_and_anchor_as_one_unit`. It must assert one Package object, one SchemaDefinition object, committed content, committed registry generation, valid anchor, and no locator mirror before publish step.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_install_transaction
```

Expected: FAIL because `PackageService` is missing.

- [ ] **Step 3: Minimum implementation**

Implement install sequence exactly: source read, artifact parse, digest verify, PythTIG export verify, schema validate, quota reservation, content stage, Package/Schema object creation, registry snapshot, object checkpoint identity, transaction anchor, content commit.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_install_transaction
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_service.rs core/src/main.rs core/src/package_registry.rs core/src/package_content_store.rs
git commit -m "feat(core): install package as anchored transaction"
```

### Task 2.11: Locator Mirror Rebuild

**Files:**
- Modify: `core/src/package_service.rs`
- Modify: `core/src/object_relationships.rs`
- Test: `core/src/package_service.rs`

**Interfaces Consumed:** `PackageRegistryGeneration`, existing `NameBinding` and `BindingTarget`.

**Interfaces Produced:** `PackageService::rebuild_locator_mirrors`.

- [ ] **Step 1: Write the failing test**

Add `locator_mirrors_are_rebuilt_from_committed_registry_generation` and assert mirrors disappear when registry state is absent.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_locator_mirror
```

Expected: FAIL because rebuild function is missing.

- [ ] **Step 3: Minimum implementation**

Create `NameBinding` and `BindingTarget` relationships from active registry records only. Do not persist mirror authority independently.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_locator_mirror
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_service.rs core/src/object_relationships.rs
git commit -m "feat(core): rebuild package locator mirrors"
```

### Task 2.12: QEMU Install Success And Source Denial

**Files:**
- Create: `scripts/test-phase13-package-install.py`
- Modify: `core/src/package_acceptance.rs`
- Test: `scripts/test-phase13-package-install.py`

**Interfaces Consumed:** `PackageService::install`.

**Interfaces Produced:** install QEMU scenarios `success` and `source-denied`.

- [ ] **Step 1: Write the failing script**

`success` required markers:

```text
PYTHOS:CORE:PACKAGE_SOURCE_AUTHORITY_READY
PYTHOS:CORE:PACKAGE_INSTALL:STAGED
PYTHOS:CORE:PACKAGE_INSTALL:COMMITTED
PYTHOS:CORE:PACKAGE_TRANSACTION_ANCHOR_READY
PYTHOS:CORE:PACKAGE_INSTALL_READY
QEMU_OUTCOME success
```

`success` forbidden markers:

```text
PYTHOS:PANIC
PYTHOS:CORE:PACKAGE_SOURCE:DENIED
PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY:ABSENT
```

`source-denied` required markers:

```text
PYTHOS:CORE:PACKAGE_SOURCE:DENIED
QEMU_OUTCOME success
```

`source-denied` forbidden markers:

```text
PYTHOS:PANIC
PYTHOS:CORE:PACKAGE_INSTALL:STAGED
PYTHOS:CORE:PACKAGE_INSTALL:COMMITTED
PYTHOS:CORE:PACKAGE_TRANSACTION_ANCHOR_READY
```

- [ ] **Step 2: Run script to verify it fails**

```powershell
python scripts/test-phase13-package-install.py --scenario success
python scripts/test-phase13-package-install.py --scenario source-denied
```

Expected: FAIL because acceptance markers are not emitted.

- [ ] **Step 3: Minimum implementation**

Wire package install scenarios into feature-gated acceptance mode.

- [ ] **Step 4: Run script to verify it passes**

```powershell
python scripts/test-phase13-package-install.py --scenario success
python scripts/test-phase13-package-install.py --scenario source-denied
```

Expected: PASS with `QEMU_OUTCOME success`.

- [ ] **Step 5: Commit**

```powershell
git add scripts/test-phase13-package-install.py core/src/package_acceptance.rs
git commit -m "test(qemu): prove package install and source denial"
```

### Task 2.12.x: Package Service Recovery Surface

**Purpose:** expose the already-designed package-registry/object-checkpoint
recovery machinery through `PackageService` so later QEMU recovery scenarios
invoke one stable package-domain recovery operation rather than reconstructing
recovery policy inside the acceptance harness.

**Files:**
- Modify: `core/src/package_service.rs`
- Test: `core/src/package_service.rs`

Do not modify `scripts/test-phase13-package-install.py` or
`core/src/package_acceptance.rs` in this inserted task. Those remain Task 2.13.

**Interfaces Consumed:** newest-valid anchored object-checkpoint /
package-registry pair selection, mismatched-anchor rejection, unanchored
staged-state rollback/reclaim result, locator-mirror rebuild requirement/state,
and package-registry recovery denial state.

**Interfaces Produced:**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageRecoveryReport {
    pub committed_anchored_generation_selected: bool,
    pub previous_valid_anchored_generation_selected: bool,
    pub staged_uncommitted_install_rolled_back: bool,
    pub orphan_content_reclaimed: bool,
    pub locator_mirrors_require_rebuild: bool,
}

impl<'a> PackageService<'a> {
    pub fn recover(&mut self) -> Result<PackageRecoveryReport, PackageStatus>;
}
```

`PackageRecoveryReport` describes recovery facts only. It is not an authority
source, does not publish locator mirrors by itself, and does not replace object
checkpoint or package-registry validation.

- [ ] **Step 1: Write the failing tests**

Add focused `package_service_recovery` tests for at least:

- clean committed recovery;
- unanchored staged install -> rollback report;
- mismatched newest anchor -> previous valid pair report;
- committed generation with missing mirrors -> rebuild report;
- no valid package anchor -> `PackageStatus::RegistryRecoveryDenied` without
  weakening existing object-store recovery.

- [ ] **Step 2: Run tests to verify they fail**

```powershell
cargo test -p pythos-core package_service_recovery
```

Expected: FAIL because `PackageService::recover` and `PackageRecoveryReport`
do not exist.

- [ ] **Step 3: Minimum implementation**

Implement only the minimal service orchestration/reporting needed to delegate
to the already-completed recovery primitives. Do not invent a second recovery
algorithm inside `PackageService`.

- [ ] **Step 4: Run tests to verify they pass**

```powershell
cargo test -p pythos-core package_service_recovery
cargo test -p pythos-core package_registry
cargo test -p pythos-core package_content_store
cargo test -p pythos-core object_service_checkpoint
py -3 -m unittest tests.test_interface_compatibility_freeze
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_service.rs
git commit -m "feat(core): expose package recovery service"
```

### Task 2.13: killed-before-anchor Recovery

**Files:**
- Modify: `scripts/test-phase13-package-install.py`
- Modify: `core/src/package_acceptance.rs`
- Test: `scripts/test-phase13-package-install.py`

**Interfaces Consumed:** `PackageService::recover`.

**Interfaces Produced:** `kill-before-anchor` scenario.

- [ ] **Step 1: Write the failing scenario**

Required markers after reboot:

```text
PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY:ABSENT
PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY:ORPHAN_CONTENT_RECLAIMED
PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY_READY
QEMU_OUTCOME success
```

Forbidden:

```text
PYTHOS:PANIC
PYTHOS:CORE:PACKAGE_INSTALL:COMMITTED
PYTHOS:CORE:PACKAGE_TRANSACTION_ANCHOR_READY
PYTHOS:CORE:PACKAGE_LOCATOR:VISIBLE
```

- [ ] **Step 2: Run scenario to verify it fails**

```powershell
python scripts/test-phase13-package-install.py --scenario kill-before-anchor
```

Expected: FAIL because killed-mid-install recovery is missing.

- [ ] **Step 3: Minimum implementation**

Use `scripts/run-qemu.py --kill-after-marker PYTHOS:CORE:PACKAGE_INSTALL:STAGED`, then reboot the same storage image and recover absent package state.

- [ ] **Step 4: Run scenario to verify it passes**

```powershell
python scripts/test-phase13-package-install.py --scenario kill-before-anchor
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add scripts/test-phase13-package-install.py core/src/package_acceptance.rs
git commit -m "test(qemu): recover killed package install before anchor"
```

### Task 2.14: killed-after-anchor And mismatched-anchor Recovery

**Files:**
- Modify: `scripts/test-phase13-package-install.py`
- Modify: `core/src/package_acceptance.rs`
- Modify: `core/src/package_registry.rs`
- Test: `scripts/test-phase13-package-install.py`

**Interfaces Consumed:** transaction anchor pair selection.

**Interfaces Produced:** `kill-after-anchor-before-mirror`, `mismatched-anchor`.

- [ ] **Step 1: Write the failing scenarios**

`kill-after-anchor-before-mirror` required:

```text
PYTHOS:CORE:PACKAGE_TRANSACTION_ANCHOR_READY
PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY:COMMITTED
PYTHOS:CORE:PACKAGE_LOCATOR:REBUILT
PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY_READY
QEMU_OUTCOME success
```

Forbidden:

```text
PYTHOS:PANIC
PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY:ABSENT
```

`mismatched-anchor` required:

```text
PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY:PREVIOUS_ANCHOR_SELECTED
PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY_READY
QEMU_OUTCOME success
```

Forbidden:

```text
PYTHOS:PANIC
PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY:COMMITTED_MISMATCH
PYTHOS:CORE:PACKAGE_LOCATOR:VISIBLE_FROM_BAD_ANCHOR
```

- [ ] **Step 2: Run scenarios to verify they fail**

```powershell
python scripts/test-phase13-package-install.py --scenario kill-after-anchor-before-mirror
python scripts/test-phase13-package-install.py --scenario mismatched-anchor
```

Expected: FAIL because anchor recovery markers are missing.

- [ ] **Step 3: Minimum implementation**

Recover anchored committed generation before mirror rebuild and select previous valid pair when registry/object roots do not match.

- [ ] **Step 4: Run scenarios to verify they pass**

```powershell
python scripts/test-phase13-package-install.py --scenario kill-after-anchor-before-mirror
python scripts/test-phase13-package-install.py --scenario mismatched-anchor
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add scripts/test-phase13-package-install.py core/src/package_acceptance.rs core/src/package_registry.rs
git commit -m "test(qemu): recover anchored package install states"
```

---

## Slice 3: package-launch

### Task 3.1: Export Resolution Through Package Registry

**Files:**
- Modify: `core/src/package_registry.rs`
- Modify: `core/src/package_service.rs`
- Test: `core/src/package_service.rs`

**Interfaces Consumed:** ADR 0069 locator resolver, registry export records.

**Interfaces Produced:** `PackageRegistry::export_for_locator`, `PackageService::resolve_export`.

- [ ] **Step 1: Write the failing tests**

Add tests that resolve an export from an explicit namespace root, reject missing export as `ExportMissing`, reject invalid locator syntax before registry lookup, and keep package locator text separate from Package `ObjectId`.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_export_resolution
```

Expected: FAIL because export resolution is missing.

- [ ] **Step 3: Minimum implementation**

Use existing object locator validation and registry export records. Do not introduce global roots or POSIX path rules.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_export_resolution
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_registry.rs core/src/package_service.rs
git commit -m "feat(core): resolve installed package exports"
```

### Task 3.2: Package Content PythTIG Verification

**Files:**
- Modify: `core/src/pyth_graph_loader.rs`
- Modify: `core/src/package_service.rs`
- Test: `core/src/pyth_graph_loader.rs`

**Interfaces Consumed:** `PackageContentStore::read_committed`, existing PythTIG verifier.

**Interfaces Produced:** `validate_package_export_graph`.

- [ ] **Step 1: Write the failing tests**

Add tests that valid package content verifies and corrupt content returns `PackageStatus::ContentCorrupt`; invalid PythTIG returns `PackageStatus::PythTigVerificationFailed` with the nested existing verifier identity.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_pythtig_verification
```

Expected: FAIL because helper is missing.

- [ ] **Step 3: Minimum implementation**

Read committed content, verify content digest, then call existing PythTIG verifier without changing PythTIG package bytes.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_pythtig_verification
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/pyth_graph_loader.rs core/src/package_service.rs
git commit -m "feat(core): verify package-contained PythTIG exports"
```

### Task 3.3: Capability Grant Check Before Launch

**Files:**
- Modify: `core/src/package_service.rs`
- Test: `core/src/package_service.rs`

**Interfaces Consumed:** manifest requirements, `CapabilityTable`.

**Interfaces Produced:** `PackageService::launch` denial before process creation.

- [ ] **Step 1: Write the failing tests**

Add tests that missing required grant returns `RequiredGrantMissing`, final authority denial returns `FinalCapabilityDenied`, and valid explicit grants produce a launch request with only supplied grants.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_launch_capability
```

Expected: FAIL because grant checks are missing.

- [ ] **Step 3: Minimum implementation**

Compare manifest-declared requirements to caller-supplied grant set. Requirements describe; they do not grant.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_launch_capability
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_service.rs
git commit -m "feat(core): require explicit package launch grants"
```

### Task 3.4: Launch Through Existing Pyth Runtime Path

**Files:**
- Modify: `core/src/pyth_runtime_launch.rs`
- Modify: `core/src/package_service.rs`
- Test: `core/src/pyth_runtime_launch.rs`

**Interfaces Consumed:** verified package graph, explicit grant set.

**Interfaces Produced:** package launch helper.

- [ ] **Step 1: Write the failing test**

Add `package_launch_uses_existing_graph_runtime_bootstrap_without_resizing_bootstrap`. Assert `size_of::<PythGraphBootstrapBlock>() == 816` and package launch prepares normal graph runtime bootstrap with expected import capability.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_launch_runtime
```

Expected: FAIL because package launch helper is missing.

- [ ] **Step 3: Minimum implementation**

Refactor existing private launch preparation as needed so package-verified content enters the same Phase 9 runtime path.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_launch_runtime
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/pyth_runtime_launch.rs core/src/package_service.rs
git commit -m "feat(core): launch package exports through Pyth runtime"
```

### Task 3.5: Package Runtime Context Syscall

**Files:**
- Modify: `shared/src/package_abi.rs`
- Modify: `core/src/syscall.rs`
- Modify: `core/src/package_service.rs`
- Test: `core/src/syscall.rs`

**Interfaces Consumed:** `PackageRuntimeSchemaBindingV0`.

**Interfaces Produced:** `SYSCALL_PACKAGE_CONTEXT` dispatch and `PackageService::runtime_schema_binding`.

- [ ] **Step 1: Write the failing tests**

Add tests that non-package processes receive `STATUS_DENIED`, package-launched process with schema slot 0 receives exact package id/revision/schema id/schema revision/digest, wrong output length receives package buffer denial, and syscall does not mutate package state.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_context_syscall
```

Expected: FAIL because syscall number is not dispatched.

- [ ] **Step 3: Minimum implementation**

Register `SYSCALL_PACKAGE_CONTEXT` and copy out `PackageRuntimeSchemaBindingV0` through existing copy-out validation.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_context_syscall
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add shared/src/package_abi.rs core/src/syscall.rs core/src/package_service.rs
git commit -m "feat(core): expose package launch schema context"
```

### Task 3.6: QEMU Launch Scenarios

**Files:**
- Create: `scripts/test-phase13-package-launch.py`
- Modify: `core/src/package_acceptance.rs`
- Test: `scripts/test-phase13-package-launch.py`

**Interfaces Consumed:** `PackageService::launch`.

**Interfaces Produced:** launch QEMU scenarios `success`, `grant-denied`, `corrupt-content`, `pythtig-denied`.

- [ ] **Step 1: Write the failing script**

`success` required:

```text
PYTHOS:CORE:PACKAGE_LAUNCH:EXPORT_RESOLVED
PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_VERIFIED
PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_VERIFIED
PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED
PYTHOS:CORE:PACKAGE_LAUNCH_READY
QEMU_OUTCOME success
```

`success` forbidden:

```text
PYTHOS:PANIC
PYTHOS:CORE:PACKAGE_LAUNCH:CAPABILITY_DENIED
PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_CORRUPT_DENIED
```

`grant-denied` required:

```text
PYTHOS:CORE:PACKAGE_LAUNCH:EXPORT_RESOLVED
PYTHOS:CORE:PACKAGE_LAUNCH:CAPABILITY_DENIED
PYTHOS:CORE:PACKAGE_LAUNCH_CAPABILITY_DENIED_READY
QEMU_OUTCOME success
```

`grant-denied` forbidden:

```text
PYTHOS:PANIC
PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED
```

`corrupt-content` forbidden includes `PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_VERIFIED` and `PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED`.

`pythtig-denied` forbidden includes `PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED`.

- [ ] **Step 2: Run script to verify it fails**

```powershell
python scripts/test-phase13-package-launch.py --scenario success
python scripts/test-phase13-package-launch.py --scenario grant-denied
python scripts/test-phase13-package-launch.py --scenario corrupt-content
python scripts/test-phase13-package-launch.py --scenario pythtig-denied
```

Expected: FAIL because QEMU launch scenarios are not wired.

- [ ] **Step 3: Minimum implementation**

Wire feature-gated package launch acceptance cases and exact forbidden-marker checks.

- [ ] **Step 4: Run script to verify it passes**

```powershell
python scripts/test-phase13-package-launch.py --scenario success
python scripts/test-phase13-package-launch.py --scenario grant-denied
python scripts/test-phase13-package-launch.py --scenario corrupt-content
python scripts/test-phase13-package-launch.py --scenario pythtig-denied
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add scripts/test-phase13-package-launch.py core/src/package_acceptance.rs
git commit -m "test(qemu): prove package launch authority boundaries"
```

---

## Slice 4: package-uninstall

### Task 4.1: Disable Semantics

**Files:**
- Modify: `core/src/package_registry.rs`
- Modify: `core/src/package_service.rs`
- Test: `core/src/package_service.rs`

**Interfaces Consumed:** installed package registry state.

**Interfaces Produced:** `PackageService::disable`.

- [ ] **Step 1: Write the failing tests**

Add tests that disable preserves Package `ObjectId`, blocks new launch, does not terminate already-running package-launched processes, and does not revoke capabilities already granted to those processes.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_disable
```

Expected: FAIL because disable state is missing.

- [ ] **Step 3: Minimum implementation**

Record `Disabled` lifecycle state in registry and launch check. Do not call process termination or capability revocation for active processes.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_disable
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_registry.rs core/src/package_service.rs
git commit -m "feat(core): disable packages without tearing down live processes"
```

### Task 4.2: Uninstall Tombstone And Live-Process Denial

**Files:**
- Modify: `core/src/package_registry.rs`
- Modify: `core/src/package_service.rs`
- Test: `core/src/package_service.rs`

**Interfaces Consumed:** package process tracking from launch.

**Interfaces Produced:** `PackageService::uninstall`.

- [ ] **Step 1: Write the failing tests**

Add tests that live package process returns `LiveProcessExists`, tombstone removes active locator visibility, tombstone preserves Package history, and same-name fresh install receives a new Package `ObjectId`.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_uninstall_tombstone
```

Expected: FAIL because uninstall is missing.

- [ ] **Step 3: Minimum implementation**

Track package-launched process ids, deny uninstall when any remain live, and commit tombstone lifecycle state.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_uninstall_tombstone
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_registry.rs core/src/package_service.rs
git commit -m "feat(core): tombstone uninstalled package identities"
```

### Task 4.3: Content Reclaim And Schema Retention

**Files:**
- Modify: `core/src/package_content_store.rs`
- Modify: `core/src/package_registry.rs`
- Modify: `core/src/package_service.rs`
- Test: `core/src/package_service.rs`

**Interfaces Consumed:** content retention counts, schema references.

**Interfaces Produced:** content reclaim on uninstall with schema descriptor retention.

- [ ] **Step 1: Write the failing tests**

Add tests that unreferenced executable content is reclaimed, schema descriptor content remains retained when an existing `PackageDefinedObject` references the exact schema revision, and descriptor content becomes reclaimable only after no retained schema reference exists.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_uninstall_retention
```

Expected: FAIL because retention integration is missing.

- [ ] **Step 3: Minimum implementation**

Use content retain/release counts during uninstall and schema-reference scan. Preserve descriptor bytes for retained schema revisions.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_uninstall_retention
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_content_store.rs core/src/package_registry.rs core/src/package_service.rs
git commit -m "feat(core): retain schemas while reclaiming package content"
```

### Task 4.4: Uninstall Recovery

**Files:**
- Modify: `core/src/package_registry.rs`
- Modify: `core/src/package_service.rs`
- Test: `core/src/package_service.rs`

**Interfaces Consumed:** transaction anchor and registry recovery.

**Interfaces Produced:** crash-safe uninstall recovery.

- [ ] **Step 1: Write the failing tests**

Add tests that a crash before tombstone anchor restores old installed state, a crash after tombstone anchor restores tombstoned state, and no recovery exposes both active locator and tombstone for the same Package `ObjectId`.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_uninstall_recovery
```

Expected: FAIL because uninstall recovery is missing.

- [ ] **Step 3: Minimum implementation**

Commit uninstall via the same object-checkpoint/registry anchor mechanism used by install.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_uninstall_recovery
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/package_registry.rs core/src/package_service.rs
git commit -m "feat(core): recover uninstall transactions atomically"
```

### Task 4.5: QEMU Uninstall Scenarios

**Files:**
- Create: `scripts/test-phase13-package-uninstall.py`
- Modify: `core/src/package_acceptance.rs`
- Test: `scripts/test-phase13-package-uninstall.py`

**Interfaces Consumed:** disable/uninstall/recovery.

**Interfaces Produced:** uninstall QEMU scenarios `disable`, `live-process-denied`, `tombstone`, `reinstall-new-identity`, `schema-retained`, `kill-during-uninstall`.

- [ ] **Step 1: Write the failing script**

Required/forbidden highlights:

```text
disable required: PACKAGE_DISABLE_READY, QEMU_OUTCOME success
disable forbidden: PACKAGE_UNINSTALL:TOMBSTONED, PACKAGE_LAUNCH:PROCESS_CREATED_AFTER_DISABLE

live-process-denied required: PACKAGE_UNINSTALL:LIVE_PROCESS_DENIED, QEMU_OUTCOME success
live-process-denied forbidden: PACKAGE_UNINSTALL:TOMBSTONED

tombstone required: PACKAGE_UNINSTALL:TOMBSTONED, PACKAGE_UNINSTALL:CONTENT_RECLAIMED, PACKAGE_UNINSTALL_READY, QEMU_OUTCOME success
tombstone forbidden: PACKAGE_LOCATOR:VISIBLE

reinstall-new-identity required: PACKAGE_REINSTALL_IDENTITY_READY, QEMU_OUTCOME success
reinstall-new-identity forbidden: PACKAGE_REINSTALL:REUSED_TOMBSTONED_ID

schema-retained required: PACKAGE_UNINSTALL:SCHEMA_RETAINED, QEMU_OUTCOME success
schema-retained forbidden: PACKAGE_UNINSTALL:SCHEMA_DESCRIPTOR_RECLAIMED

kill-during-uninstall required: PACKAGE_UNINSTALL_RECOVERY_READY, QEMU_OUTCOME success
kill-during-uninstall forbidden: PACKAGE_LOCATOR:HALF_VISIBLE
```

- [ ] **Step 2: Run script to verify it fails**

```powershell
python scripts/test-phase13-package-uninstall.py --scenario disable
python scripts/test-phase13-package-uninstall.py --scenario live-process-denied
python scripts/test-phase13-package-uninstall.py --scenario tombstone
python scripts/test-phase13-package-uninstall.py --scenario reinstall-new-identity
python scripts/test-phase13-package-uninstall.py --scenario schema-retained
python scripts/test-phase13-package-uninstall.py --scenario kill-during-uninstall
```

Expected: FAIL because script and acceptance paths are missing.

- [ ] **Step 3: Minimum implementation**

Wire QEMU scenarios with exact required/forbidden marker checks.

- [ ] **Step 4: Run script to verify it passes**

```powershell
python scripts/test-phase13-package-uninstall.py --scenario disable
python scripts/test-phase13-package-uninstall.py --scenario live-process-denied
python scripts/test-phase13-package-uninstall.py --scenario tombstone
python scripts/test-phase13-package-uninstall.py --scenario reinstall-new-identity
python scripts/test-phase13-package-uninstall.py --scenario schema-retained
python scripts/test-phase13-package-uninstall.py --scenario kill-during-uninstall
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add scripts/test-phase13-package-uninstall.py core/src/package_acceptance.rs
git commit -m "test(qemu): prove package uninstall recovery and retention"
```

---

## Slice 5: independently-authored-package

### Task 5.1: Compiler Token For PackageDefinedObject

**Files:**
- Modify: `tools/pythc/src/lower.rs`
- Modify: `tools/pythc/tests/lower.rs`
- Test: `tools/pythc/tests/lower.rs`

**Interfaces Consumed:** source literal `2` maps to graph token `b"package-defined"`.

**Interfaces Produced:** pythc can compile `object.create(workspace, 2)`.

- [ ] **Step 1: Write the failing test**

Add a compiler lowering test that compiles a fixture containing `object.create(workspace, 2)` and asserts the generated PythTIG string table contains `package-defined`.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythc object_create_package_defined
```

Expected: FAIL because literal `2` is rejected.

- [ ] **Step 3: Minimum implementation**

Extend `lower_object_kind` to map `Ok(2)` to `b"package-defined"`.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythc object_create_package_defined
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add tools/pythc/src/lower.rs tools/pythc/tests/lower.rs
git commit -m "feat(pythc): lower package-defined object creation token"
```

### Task 5.2: Runtime Constructs PackageDefinedObjectCreateV0

**Files:**
- Modify: `user/pyth-runtime/src/syscalls.rs`
- Test: `user/pyth-runtime/src/syscalls.rs`

**Interfaces Consumed:** `SYSCALL_PACKAGE_CONTEXT`, `PackageRuntimeSchemaBindingV0`, `PackageDefinedObjectCreateV0`.

**Interfaces Produced:** runtime object create path for `b"package-defined"`.

- [ ] **Step 1: Write the failing tests**

Add tests that `object_kind_from_graph(b"package-defined")` returns `OBJECT_KIND_PACKAGE_DEFINED_OBJECT`, runtime builds a 64-byte `PackageDefinedObjectCreateV0`, and `b"unknown"` still returns `HostError::Failed`.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pyth-runtime package_defined_object_create_buffer
```

Expected: FAIL because runtime only admits `b"note"`.

- [ ] **Step 3: Minimum implementation**

When kind token is `package-defined`, call `SYSCALL_PACKAGE_CONTEXT` for schema slot 0, build `PackageDefinedObjectCreateV0` with empty initial state, set `ObjectShellRequest.input_ptr/input_len`, then send `OP_CREATE_OBJECT`.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pyth-runtime package_defined_object_create_buffer
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add user/pyth-runtime/src/syscalls.rs
git commit -m "feat(runtime): create package-defined object request buffers"
```

### Task 5.3: PythCore Validates PackageDefinedObject Create Buffer

**Files:**
- Modify: `core/src/syscall.rs`
- Modify: `core/src/object_service.rs`
- Modify: `core/src/typed_object_format.rs`
- Test: `core/src/syscall.rs`, `core/src/object_service.rs`

**Interfaces Consumed:** `PackageDefinedObjectCreateV0`, `PackageDefinedCreateInput`.

**Interfaces Produced:** `OP_CREATE_OBJECT` dispatch for `OBJECT_KIND_PACKAGE_DEFINED_OBJECT`.

- [ ] **Step 1: Write the failing tests**

Add tests that valid schema id/revision creates `PackageDefinedObject`, invalid schema revision denies, nonzero reserved fields deny, inline state longer than 16 bytes denies, and legacy `Note` create behavior is unchanged.

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p pythos-core package_defined_object_syscall
```

Expected: FAIL because request kind `32` is unsupported.

- [ ] **Step 3: Minimum implementation**

Decode input buffer through existing copy-in validation, check ABI fields, validate retained schema revision, build typed object fields `FIELD_PACKAGE_SCHEMA_REF_V0` and optional `FIELD_PACKAGE_INLINE_STATE_V0`, then call `ObjectService::create_package_defined_object`.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p pythos-core package_defined_object_syscall
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add core/src/syscall.rs core/src/object_service.rs core/src/typed_object_format.rs
git commit -m "feat(core): validate package-defined object creation"
```

### Task 5.4: Independent Package Fixture

**Files:**
- Create: `programs/phase13/create-seed.pyth`
- Create: `programs/phase13/schemas/seed.v0.schema`
- Modify: `scripts/build-phase13-package-fixture.py`
- Test: fixture builder self-test

**Interfaces Consumed:** pythc object kind `2`, package artifact builder.

**Interfaces Produced:** independently authored package artifact with one schema and one tool export.

- [ ] **Step 1: Write the failing fixture self-test**

Self-test must build the package, assert the manifest contains one schema declaration, one launchable export, one object-create capability requirement, and no `app`, `desktop`, `launcher`, `window`, or filesystem authority fields.

- [ ] **Step 2: Run test to verify it fails**

```powershell
python scripts/build-phase13-package-fixture.py --self-test --fixture independent-seed
```

Expected: FAIL because fixture files do not exist.

- [ ] **Step 3: Minimum implementation**

Add the Pyth source and schema descriptor. Build the package through pythc and PythTIG tooling without compiling the fixture into the kernel.

- [ ] **Step 4: Run test to verify it passes**

```powershell
python scripts/build-phase13-package-fixture.py --self-test --fixture independent-seed
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add programs/phase13/create-seed.pyth programs/phase13/schemas/seed.v0.schema scripts/build-phase13-package-fixture.py
git commit -m "test(fixture): add independent Phase 13 package"
```

### Task 5.5: Independent Package QEMU Acceptance

**Files:**
- Create: `scripts/test-phase13-independent-package.py`
- Modify: `core/src/package_acceptance.rs`
- Test: `scripts/test-phase13-independent-package.py`

**Interfaces Consumed:** install, launch, PackageDefinedObject creation, uninstall.

**Interfaces Produced:** final Phase 13 QEMU proof.

- [ ] **Step 1: Write the failing script**

Required markers:

```text
PYTHOS:CORE:INDEPENDENT_PACKAGE:INSTALLED
PYTHOS:CORE:INDEPENDENT_PACKAGE:LAUNCHED
PYTHOS:CORE:INDEPENDENT_PACKAGE:OBJECT_CREATED
PYTHOS:CORE:INDEPENDENT_PACKAGE:UNINSTALLED
PYTHOS:CORE:INDEPENDENT_PACKAGE:INSTANCE_RESTORED
PYTHOS:CORE:INDEPENDENT_PACKAGE_READY
PYTHOS:CORE:PACKAGE_SCHEMA_EXTENSIBILITY_READY
PYTHOS:CORE:PHASE_13_COMPLETE
QEMU_OUTCOME success
```

Forbidden markers:

```text
PYTHOS:PANIC
PYTHOS:CORE:INDEPENDENT_PACKAGE:INSTANCE_LOST
PYTHOS:CORE:INDEPENDENT_PACKAGE:SCHEMA_LOST
PYTHOS:CORE:PACKAGE_SESSION_RUNTIME_READY
PYTHOS:CORE:WAKE_CONTEXT_READY
PYTHOS:CORE:KAI_READY
```

Required order:

```text
INSTALLED -> LAUNCHED -> OBJECT_CREATED -> UNINSTALLED -> INSTANCE_RESTORED -> PHASE_13_COMPLETE
```

- [ ] **Step 2: Run script to verify it fails**

```powershell
python scripts/test-phase13-independent-package.py
```

Expected: FAIL because final independent package flow is not wired.

- [ ] **Step 3: Minimum implementation**

Run the full accepted proof: install Package and SchemaDefinition, launch the tool with explicit object/create grant, create PackageDefinedObject, uninstall package, reboot, inspect retained instance and exact schema revision.

- [ ] **Step 4: Run script to verify it passes**

```powershell
python scripts/test-phase13-independent-package.py
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add scripts/test-phase13-independent-package.py core/src/package_acceptance.rs
git commit -m "test(qemu): prove independent package lifecycle"
```

### Task 5.6: Final Docs And Full Regression

**Files:**
- Modify: `docs/ROADMAP.md`
- Modify: `docs/HANDOVER.md`
- Modify: `docs/technical-overview.md`
- Test: full Phase 13 command set

**Interfaces Consumed:** all slice evidence.

**Interfaces Produced:** final Phase 13 documentation state and stop boundary.

- [ ] **Step 1: Write doc updates**

Record only proven Phase 13 behavior and the exact Phase 13 -> Phase 13.5 stop boundary. State that persistent Pyth session runtime, presentation/input bridges, Kai, WakeContext, networking, AI, and First Waking remain uninvoked.

- [ ] **Step 2: Run full regression**

```powershell
cargo test -p pythos-shared
cargo test -p pythos-core
cargo test -p pythc
cargo test -p pyth-runtime
python -m unittest tests.test_boot_marker_contract tests.test_interface_compatibility_freeze tests.test_iso_image
python scripts/test-boot.py
python scripts/test-phase13-package-format.py
python scripts/test-phase13-package-install.py --all-scenarios
python scripts/test-phase13-package-launch.py --all-scenarios
python scripts/test-phase13-package-uninstall.py --all-scenarios
python scripts/test-phase13-independent-package.py
```

Expected: PASS for every command with final QEMU evidence containing `PYTHOS:CORE:PHASE_13_COMPLETE`.

- [ ] **Step 3: Commit**

```powershell
git add docs/ROADMAP.md docs/HANDOVER.md docs/technical-overview.md
git commit -m "docs: record completed Phase 13 package lifecycle"
```

- [ ] **Step 4: Stop**

Report implementation summary, ABI/surface introduced, denial identities, files changed, tests/QEMU evidence, architectural observations, and exact remaining Phase 13.5 boundary. Do not begin Phase 13.5.

---

## QEMU Scenario Matrix

Every scenario must verify required markers, forbidden markers, order, and `QEMU_OUTCOME success`.

| Script | Scenario | Required terminal marker | Forbidden marker examples |
| --- | --- | --- | --- |
| `scripts/test-phase13-package-format.py` | default | `PYTHOS:CORE:PACKAGE_FORMAT_READY` | `PYTHOS:CORE:PACKAGE_INSTALL:STAGED`, `PYTHOS:PANIC` |
| `scripts/test-phase13-package-install.py` | `success` | `PYTHOS:CORE:PACKAGE_INSTALL_READY` | `PYTHOS:CORE:PACKAGE_SOURCE:DENIED`, `PYTHOS:PANIC` |
| `scripts/test-phase13-package-install.py` | `source-denied` | `PYTHOS:CORE:PACKAGE_SOURCE:DENIED` | `PYTHOS:CORE:PACKAGE_INSTALL:STAGED`, `PYTHOS:CORE:PACKAGE_INSTALL:COMMITTED` |
| `scripts/test-phase13-package-install.py` | `kill-before-anchor` | `PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY_READY` | `PYTHOS:CORE:PACKAGE_TRANSACTION_ANCHOR_READY`, `PYTHOS:CORE:PACKAGE_LOCATOR:VISIBLE` |
| `scripts/test-phase13-package-install.py` | `kill-after-anchor-before-mirror` | `PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY_READY` | `PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY:ABSENT` |
| `scripts/test-phase13-package-install.py` | `mismatched-anchor` | `PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY_READY` | `PYTHOS:CORE:PACKAGE_LOCATOR:VISIBLE_FROM_BAD_ANCHOR` |
| `scripts/test-phase13-package-launch.py` | `success` | `PYTHOS:CORE:PACKAGE_LAUNCH_READY` | `PYTHOS:CORE:PACKAGE_LAUNCH:CAPABILITY_DENIED` |
| `scripts/test-phase13-package-launch.py` | `grant-denied` | `PYTHOS:CORE:PACKAGE_LAUNCH_CAPABILITY_DENIED_READY` | `PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED` |
| `scripts/test-phase13-package-launch.py` | `corrupt-content` | `PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_CORRUPT_DENIED` | `PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_VERIFIED`, `PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED` |
| `scripts/test-phase13-package-launch.py` | `pythtig-denied` | `PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_DENIED` | `PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED` |
| `scripts/test-phase13-package-uninstall.py` | `disable` | `PYTHOS:CORE:PACKAGE_DISABLE_READY` | `PYTHOS:CORE:PACKAGE_UNINSTALL:TOMBSTONED` |
| `scripts/test-phase13-package-uninstall.py` | `live-process-denied` | `PYTHOS:CORE:PACKAGE_UNINSTALL:LIVE_PROCESS_DENIED` | `PYTHOS:CORE:PACKAGE_UNINSTALL:TOMBSTONED` |
| `scripts/test-phase13-package-uninstall.py` | `tombstone` | `PYTHOS:CORE:PACKAGE_UNINSTALL_READY` | `PYTHOS:CORE:PACKAGE_LOCATOR:VISIBLE` |
| `scripts/test-phase13-package-uninstall.py` | `reinstall-new-identity` | `PYTHOS:CORE:PACKAGE_REINSTALL_IDENTITY_READY` | `PYTHOS:CORE:PACKAGE_REINSTALL:REUSED_TOMBSTONED_ID` |
| `scripts/test-phase13-package-uninstall.py` | `schema-retained` | `PYTHOS:CORE:PACKAGE_UNINSTALL:SCHEMA_RETAINED` | `PYTHOS:CORE:PACKAGE_UNINSTALL:SCHEMA_DESCRIPTOR_RECLAIMED` |
| `scripts/test-phase13-package-uninstall.py` | `kill-during-uninstall` | `PYTHOS:CORE:PACKAGE_UNINSTALL_RECOVERY_READY` | `PYTHOS:CORE:PACKAGE_LOCATOR:HALF_VISIBLE` |
| `scripts/test-phase13-independent-package.py` | default | `PYTHOS:CORE:PHASE_13_COMPLETE` | `PYTHOS:CORE:PACKAGE_SESSION_RUNTIME_READY`, `PYTHOS:CORE:KAI_READY`, `PYTHOS:PANIC` |

## Phase 13 Marker Order

The high-level Phase 13 marker tail is:

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

Slice scripts may require detailed markers between these high-level markers, but this order must remain stable once ADR 0073 freezes it.

## Explicit Phase 13 -> Phase 13.5 Boundary

Phase 13 is complete only when package lifecycle and open package-defined ontology pass automated QEMU evidence through `PYTHOS:CORE:PHASE_13_COMPLETE`.

The next milestone is not part of this plan:

```text
packaged persistent Pyth session
-> long-lived supervised ring-3 execution
-> Rust object shell retained as recovery/maintenance fallback
```

Do not plan or implement that milestone inside Phase 13.
