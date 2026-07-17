# Phase 9 Dynamic ELF Loading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Phase 9 `dynamic-elf-loading` slice: PythCore validates, copies, zero-fills, and maps a dynamically supplied user ELF payload from the new inner `INIT.PAK` bundle without executing it.

**Architecture:** Keep the UEFI boot ABI unchanged. Add ADR 0037's inner bundle parser to `pythos-shared`, update builders to emit runtime + user ELF records, add a PythCore `user_elf` validator/loader, map loaded segments into a user address space, and extend the QEMU marker contract with positive and negative proof markers.

**Tech Stack:** Rust `no_std` in `shared` and `core`, Python stdlib builders/tests, existing QEMU serial marker harness.

## Global Constraints

Do not execute the loaded user ELF in this slice.
Do not define the general syscall ABI.
Do not implement copy-in/copy-out.
Do not load programs from ESP paths or Phase 10 storage.
Do not add package install, networking, hardware expansion, SMP, semantic indexing, local AI, or vision-layer behavior.
Every malformed ELF proof must be denied by the general validation mechanism, not a special fixed path.
Required success markers: `PYTHOS:CORE:USER_ELF:LOADED`, `PYTHOS:CORE:USER_ELF:SEGMENTS_MAPPED`, `PYTHOS:CORE:DYNAMIC_ELF_LOADING_READY`.
Required rejection markers: `PYTHOS:CORE:USER_ELF:REJECTED:BUFFER_RANGE`, `PYTHOS:CORE:USER_ELF:REJECTED:WX_SEGMENT`, `PYTHOS:CORE:USER_ELF:REJECTED:KERNEL_RANGE`.

---

## File Structure

Create `shared/src/init_bundle.rs` for ADR 0037 parsing and checksumming.

Modify `shared/src/lib.rs` to export `init_bundle`.

Modify `scripts/build-image.py` and `scripts/build-iso.py` to wrap the existing ADR 0014 runtime payload plus a minimal generated user ELF payload in the ADR 0037 inner bundle.

Create `core/src/user_elf.rs` for ELF64 validation, rejection reasons, loaded segment metadata, negative self-test payloads, and host tests.

Modify `core/src/runtime_loader.rs` to preserve legacy direct runtime payload loading and add inner-bundle record lookup helpers for runtime and user ELF records.

Modify `core/src/memory/virtual.rs` to map dynamically loaded user program pages into a user address space with user permissions and W^X page attributes.

Modify `core/src/main.rs` to run the Phase 9 proof after Phase 8 capability-boundary markers and before framebuffer completion.

Modify `scripts/test-boot.py` and `tests/boot_core_handoff.py` to add `dynamic-elf-loading` marker acceptance.

---

### Task 1: ADR 0037 Inner Bundle Parser

**Files:**
- Create: `shared/src/init_bundle.rs`
- Modify: `shared/src/lib.rs`

**Interfaces:**
- Produces: `init_bundle::validate(bytes: &[u8]) -> Result<InitBundle<'_>, InitBundleError>`
- Produces: `InitBundle::record(record_type: RecordType) -> Option<Record<'_>>`
- Produces: `RecordType::RuntimePayload` and `RecordType::UserElf`

- [x] **Step 1: Write failing shared tests**

Add tests inside `shared/src/init_bundle.rs`:

```rust
#[test]
fn valid_bundle_exposes_runtime_and_user_elf_records() {
    let bundle = build_bundle(&[(TYPE_RUNTIME_PAYLOAD, b"runtime"), (TYPE_USER_ELF, b"elf")]);
    let parsed = validate(&bundle).unwrap();
    assert_eq!(parsed.record(RecordType::RuntimePayload).unwrap().bytes(), b"runtime");
    assert_eq!(parsed.record(RecordType::UserElf).unwrap().bytes(), b"elf");
}

#[test]
fn record_range_overflow_is_rejected() {
    let mut bundle = build_bundle(&[(TYPE_RUNTIME_PAYLOAD, b"runtime")]);
    bundle[40..48].copy_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(validate(&bundle), Err(InitBundleError::LengthOverflow));
}

#[test]
fn checksum_mismatch_is_rejected() {
    let mut bundle = build_bundle(&[(TYPE_RUNTIME_PAYLOAD, b"runtime")]);
    let last = bundle.len() - 1;
    bundle[last] ^= 0xFF;
    assert_eq!(validate(&bundle), Err(InitBundleError::BadChecksum));
}
```

- [x] **Step 2: Run shared tests to verify failure**

Run: `cargo test -p pythos-shared init_bundle --target x86_64-pc-windows-msvc`

Expected: compile failure because `init_bundle` does not exist.

- [x] **Step 3: Implement parser**

Define constants and types:

```rust
pub const INIT_BUNDLE_MAGIC: &[u8; 16] = b"PYTHOS_BUNDLE_V0";
pub const INIT_BUNDLE_MAJOR: u16 = 0;
pub const INIT_BUNDLE_MINOR: u16 = 0;
pub const INIT_BUNDLE_HEADER_LEN: u32 = 32;
pub const RECORD_ENTRY_LEN: usize = 32;
pub const TYPE_RUNTIME_PAYLOAD: u32 = 0x0000_0001;
pub const TYPE_USER_ELF: u32 = 0x0000_0002;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordType {
    RuntimePayload,
    UserElf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitBundleError {
    TooShort,
    BadMagic,
    UnsupportedMajor,
    BadHeaderLength,
    EmptyRecordTable,
    NonZeroReserved,
    LengthOverflow,
    BadRecordTable,
    UnsupportedRecordType,
    BadRecordRange,
    OverlappingRecords,
    BadChecksum,
}
```

`validate` must parse the fixed header, reject nonzero reserved bytes, bounds-check `header_len + record_count * 32`, validate each record range and checksum, reject overlapping record payloads, and return an `InitBundle` containing up to four records.

- [x] **Step 4: Export module**

Add to `shared/src/lib.rs`:

```rust
pub mod init_bundle;
```

- [x] **Step 5: Run shared tests**

Run: `cargo test -p pythos-shared init_bundle --target x86_64-pc-windows-msvc`

Expected: all `init_bundle` tests pass.

---

### Task 2: Builder Output Uses Inner Bundle

**Files:**
- Modify: `scripts/build-image.py`
- Modify: `scripts/build-iso.py`
- Test: `tests/test_iso_image.py`

**Interfaces:**
- Consumes: ADR 0037 constants from Task 1 conceptually; Python builders duplicate byte layout to keep no build-time Rust dependency.
- Produces: `build_init_bundle(records: list[tuple[int, bytes]]) -> bytes`
- Produces: `build_user_elf_payload() -> bytes`

- [x] **Step 1: Add Python tests for generated inner bundle**

In `tests/test_iso_image.py`, add a test that imports both builders and verifies `INIT.PAK` contains `PYTHOS_BUNDLE_V0`, runtime record type `1`, and user ELF record type `2`.

```python
def test_generated_init_pak_contains_inner_bundle_records(self) -> None:
    import importlib.util
    spec = importlib.util.spec_from_file_location("build_image", ROOT / "scripts" / "build-image.py")
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    init_pak = module.INIT_PAK
    payload = init_pak[module.INIT_PAK_HEADER_LEN:]
    self.assertEqual(payload[:16], b"PYTHOS_BUNDLE_V0")
    self.assertIn((1).to_bytes(4, "little"), payload[32:96])
    self.assertIn((2).to_bytes(4, "little"), payload[32:96])
```

- [x] **Step 2: Run test to verify failure**

Run: `python -m unittest tests.test_iso_image`

Expected: failure because builders still emit direct runtime payload.

- [x] **Step 3: Implement builder helpers**

In both builders add:

```python
INIT_BUNDLE_MAGIC = b"PYTHOS_BUNDLE_V0"
INIT_BUNDLE_HEADER_LEN = 32
INIT_BUNDLE_RECORD_LEN = 32
INIT_BUNDLE_RUNTIME_TYPE = 0x0000_0001
INIT_BUNDLE_USER_ELF_TYPE = 0x0000_0002
USER_ELF_ENTRY = 0x00400000

def build_init_bundle(records: list[tuple[int, bytes]]) -> bytes:
    header_len = INIT_BUNDLE_HEADER_LEN
    record_table_len = len(records) * INIT_BUNDLE_RECORD_LEN
    cursor = header_len + record_table_len
    table = bytearray(record_table_len)
    payloads = bytearray()
    for index, (record_type, payload) in enumerate(records):
        entry = index * INIT_BUNDLE_RECORD_LEN
        table[entry:entry + 4] = record_type.to_bytes(4, "little")
        table[entry + 8:entry + 16] = cursor.to_bytes(8, "little")
        table[entry + 16:entry + 24] = len(payload).to_bytes(8, "little")
        table[entry + 24:entry + 28] = (sum(payload) & 0xFFFFFFFF).to_bytes(4, "little")
        payloads.extend(payload)
        cursor += len(payload)
    header = bytearray(header_len)
    header[: len(INIT_BUNDLE_MAGIC)] = INIT_BUNDLE_MAGIC
    header[16:18] = (0).to_bytes(2, "little")
    header[18:20] = (0).to_bytes(2, "little")
    header[20:24] = header_len.to_bytes(4, "little")
    header[24:26] = len(records).to_bytes(2, "little")
    return bytes(header) + bytes(table) + bytes(payloads)
```

Implement `build_user_elf_payload()` as a minimal ELF64 `ET_EXEC` with two `PT_LOAD` segments: RX text at `0x00400000` and RW data at `0x00401000` where `p_memsz > p_filesz` to prove BSS zeroing.

- [x] **Step 4: Wrap `INIT_PAK` payload**

Change:

```python
INIT_PAK = build_init_pak(build_runtime_payload())
```

to:

```python
INIT_PAK = build_init_pak(
    build_init_bundle(
        [
            (INIT_BUNDLE_RUNTIME_TYPE, build_runtime_payload()),
            (INIT_BUNDLE_USER_ELF_TYPE, build_user_elf_payload()),
        ]
    )
)
```

- [x] **Step 5: Run Python tests**

Run: `python -m unittest tests.test_iso_image`

Expected: pass.

---

### Task 3: Runtime Loader Compatibility

**Files:**
- Modify: `core/src/runtime_loader.rs`
- Test: `core/src/runtime_loader.rs`

**Interfaces:**
- Consumes: `pythos_shared::init_bundle`
- Produces: `runtime_loader::load_user_elf_payload(boot_info: &PythBootInfo) -> Result<&[u8], RuntimeLoadError>`
- Preserves: `runtime_loader::load_init_payload(boot_info: &PythBootInfo) -> Result<RuntimePayload<'_>, RuntimeLoadError>`

- [x] **Step 1: Add compatibility tests**

Add tests proving both legacy direct runtime payload and inner bundle runtime payload validate:

```rust
#[test]
fn inner_bundle_runtime_payload_passes() {
    let payload = build_runtime_payload(HELLO_SERVICE);
    let elf = b"\x7FELFuser";
    let bundle = build_init_pak(&build_inner_bundle(&[(1, &payload), (2, elf)]));
    let runtime = validate_init_payload_bytes(&bundle).unwrap();
    assert!(runtime.source.contains("system.log"));
}

#[test]
fn inner_bundle_user_elf_record_is_exposed() {
    let payload = build_runtime_payload(HELLO_SERVICE);
    let elf = b"\x7FELFuser";
    let bundle = build_init_pak(&build_inner_bundle(&[(1, &payload), (2, elf)]));
    let user_elf = validate_user_elf_payload_bytes(&bundle).unwrap();
    assert_eq!(user_elf, elf);
}
```

- [x] **Step 2: Run runtime loader tests to verify failure**

Run: `cargo test -p pythos-core runtime_loader --target x86_64-pc-windows-msvc`

Expected: compile failure for missing `validate_user_elf_payload_bytes`.

- [x] **Step 3: Implement dual path**

In `validate_init_payload_bytes`, after outer `init_pak::validate`, inspect the payload bytes. If `init_bundle::validate(payload)` succeeds, find `RecordType::RuntimePayload` and validate that record with `runtime_payload::validate`. If inner bundle validation fails because magic is absent, preserve the legacy direct runtime-payload path.

Add `validate_user_elf_payload_bytes(bytes)` that validates outer `INIT.PAK`, requires inner bundle validation, then returns the `RecordType::UserElf` record bytes.

- [x] **Step 4: Run runtime loader tests**

Run: `cargo test -p pythos-core runtime_loader --target x86_64-pc-windows-msvc`

Expected: pass.

---

### Task 4: User ELF Validator and Loader

**Files:**
- Create: `core/src/user_elf.rs`
- Modify: `core/src/main.rs`
- Test: `core/src/user_elf.rs`

**Interfaces:**
- Produces: `user_elf::validate(bytes: &[u8]) -> Result<UserElfImage, UserElfError>`
- Produces: `user_elf::run_rejection_self_tests() -> Result<UserElfRejectionProof, UserElfError>`
- Produces: `UserElfError::BufferRange`, `WxSegment`, `KernelRange`, `OverlappingSegments`, `UnsupportedSegment`

- [x] **Step 1: Add validator tests**

Tests must include:

```rust
#[test]
fn minimal_user_elf_validates() {
    let image = validate(&minimal_user_elf()).unwrap();
    assert_eq!(image.entry(), 0x00400000);
}

#[test]
fn segment_exceeding_buffer_is_rejected() {
    assert_eq!(validate(&elf_with_bad_file_range()), Err(UserElfError::BufferRange));
}

#[test]
fn writable_executable_segment_is_rejected() {
    assert_eq!(validate(&elf_with_wx_segment()), Err(UserElfError::WxSegment));
}

#[test]
fn kernel_higher_half_segment_is_rejected() {
    assert_eq!(validate(&elf_with_kernel_range()), Err(UserElfError::KernelRange));
}

#[test]
fn overlapping_page_ranges_are_rejected() {
    assert_eq!(validate(&elf_with_overlapping_segments()), Err(UserElfError::OverlappingSegments));
}

#[test]
fn interp_segment_is_rejected() {
    assert_eq!(validate(&elf_with_interp_segment()), Err(UserElfError::UnsupportedSegment));
}
```

- [x] **Step 2: Run validator tests to verify failure**

Run: `cargo test -p pythos-core user_elf --target x86_64-pc-windows-msvc`

Expected: compile failure because `user_elf` does not exist.

- [x] **Step 3: Implement validation**

Use constants:

```rust
const EI_CLASS_64: u8 = 2;
const EI_DATA_LITTLE_ENDIAN: u8 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 0x3E;
const PT_LOAD: u32 = 1;
const PT_INTERP: u32 = 3;
const PT_DYNAMIC: u32 = 2;
const PF_X: u32 = 0x1;
const PF_W: u32 = 0x2;
const PF_R: u32 = 0x4;
const ELF64_PHDR_SIZE: u16 = 56;
const USER_VIRT_MIN: u64 = 0x0020_0000;
const USER_VIRT_MAX: u64 = 0x0000_8000_0000_0000;
const KERNEL_VIRT_MIN: u64 = 0xFFFF_FFFF_8000_0000;
const MAX_LOAD_SEGMENTS: usize = 8;
```

Validation must use `checked_add` and `checked_mul` for all offset, file-size,
memory-size, table-size, and virtual-address arithmetic. Segment overlap checks
must compare page-rounded ranges, not raw byte ranges.

- [x] **Step 4: Add `mod user_elf;`**

In `core/src/main.rs`, add:

```rust
mod user_elf;
```

- [x] **Step 5: Run validator tests**

Run: `cargo test -p pythos-core user_elf --target x86_64-pc-windows-msvc`

Expected: pass.

---

### Task 5: Map Loaded User ELF Into a User Address Space

**Files:**
- Modify: `core/src/memory/virtual.rs`
- Modify: `core/src/user_elf.rs`

**Interfaces:**
- Consumes: `user_elf::UserElfImage`
- Produces: `UserAddressSpace::build_with_user_elf(allocator: &mut PhysicalMemory, boot_info: &PythBootInfo, image: &UserElfImage, bytes: &[u8]) -> Result<Self, VmError>`
- Produces: `user_elf::LoadedUserElf { entry: u64, segment_count: usize, bss_zeroed: bool }`

- [x] **Step 1: Add mapping and BSS tests**

In `core/src/user_elf.rs`, add a host-test helper that loads a segment into a mutable byte buffer and proves `p_memsz - p_filesz` is zero:

```rust
#[test]
fn bss_remainder_is_zero_filled() {
    let image = validate(&minimal_user_elf()).unwrap();
    let mut destination = [0xA5u8; 4096];
    copy_segment_for_test(&minimal_user_elf(), image.segment(1).unwrap(), &mut destination).unwrap();
    assert!(destination[4..16].iter().all(|&byte| byte == 0));
}
```

- [x] **Step 2: Run test to verify failure**

Run: `cargo test -p pythos-core bss_remainder_is_zero_filled --target x86_64-pc-windows-msvc`

Expected: compile failure for missing helper.

- [x] **Step 3: Implement loading helpers**

Add `copy_segment_for_test` under `#[cfg(test)]` and a non-test `copy_segment_to_physical` with full unsafe invariant comments. Both must zero the whole destination page range before copying `p_filesz` bytes.

- [x] **Step 4: Implement user page mapping**

In `memory/virtual.rs`, add a private `map_user_physical_range` method on `PageTableBuilder` that maps physical pages with `PTE_USER` and caller-selected `PTE_WRITE`/`PTE_NO_EXECUTE`.

Add `UserAddressSpace::build_with_user_elf` that starts from the same base mappings as `build`, then allocates pages for each user ELF segment, copies and zero-fills bytes, maps each segment's page range with W^X permissions, and records table frames.

- [x] **Step 5: Run host tests**

Run: `cargo test -p pythos-core user_elf --target x86_64-pc-windows-msvc`

Expected: pass.

---

### Task 6: Boot Proof and Marker Contract

**Files:**
- Modify: `core/src/main.rs`
- Modify: `scripts/test-boot.py`
- Modify: `tests/boot_core_handoff.py`

**Interfaces:**
- Consumes: `runtime_loader::load_user_elf_payload`
- Consumes: `user_elf::validate`
- Consumes: `UserAddressSpace::build_with_user_elf`
- Produces boot slice: `dynamic-elf-loading`

- [x] **Step 1: Add marker contract tests**

In `tests/boot_core_handoff.py`, add:

```python
def test_dynamic_elf_loading_markers_are_observed_after_capability_boundary(self) -> None:
    self.run_boot_slice("dynamic-elf-loading")
```

In `scripts/test-boot.py`, add:

```python
DYNAMIC_ELF_LOADING_MARKERS = [
    "PYTHOS:CORE:USER_ELF:REJECTED:BUFFER_RANGE",
    "PYTHOS:CORE:USER_ELF:REJECTED:WX_SEGMENT",
    "PYTHOS:CORE:USER_ELF:REJECTED:KERNEL_RANGE",
    "PYTHOS:CORE:USER_ELF:LOADED",
    "PYTHOS:CORE:USER_ELF:SEGMENTS_MAPPED",
    "PYTHOS:CORE:DYNAMIC_ELF_LOADING_READY",
]

SLICE_MARKERS["dynamic-elf-loading"] = (
    SLICE_MARKERS["capability-enforcement-at-boundary"] + DYNAMIC_ELF_LOADING_MARKERS
)
SLICE_MARKERS["milestone-1"] = insert_before(
    SLICE_MARKERS["milestone-1"],
    "PYTHOS:CORE:FRAMEBUFFER_READY",
    DYNAMIC_ELF_LOADING_MARKERS,
)
```

- [x] **Step 2: Run marker test to verify failure**

Run: `python -m unittest tests.boot_core_handoff.BootCoreHandoffTest.test_dynamic_elf_loading_markers_are_observed_after_capability_boundary`

Expected: failure because markers are not emitted yet.

- [x] **Step 3: Integrate boot proof**

In `core/src/main.rs`, after the Phase 8 capability-boundary proof succeeds and before `PYTHOS:CORE:FRAMEBUFFER_READY`, run:

```rust
let rejection_proof = match user_elf::run_rejection_self_tests() {
    Ok(proof) => proof,
    Err(_) => {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
};
if rejection_proof.buffer_range_denied {
    serial::write_line("PYTHOS:CORE:USER_ELF:REJECTED:BUFFER_RANGE");
}
if rejection_proof.wx_segment_denied {
    serial::write_line("PYTHOS:CORE:USER_ELF:REJECTED:WX_SEGMENT");
}
if rejection_proof.kernel_range_denied {
    serial::write_line("PYTHOS:CORE:USER_ELF:REJECTED:KERNEL_RANGE");
}

let user_elf_bytes = match runtime_loader::load_user_elf_payload(boot_info) {
    Ok(bytes) => bytes,
    Err(_) => {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
};
let user_elf = match user_elf::validate(user_elf_bytes) {
    Ok(image) => image,
    Err(_) => {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
};
serial::write_line("PYTHOS:CORE:USER_ELF:LOADED");
if memory::r#virtual::UserAddressSpace::build_with_user_elf(
    &mut physical_memory,
    boot_info,
    &user_elf,
    user_elf_bytes,
)
.is_err()
{
    serial::write_line("PYTHOS:PANIC");
    qemu_exit::panic();
}
serial::write_line("PYTHOS:CORE:USER_ELF:SEGMENTS_MAPPED");
serial::write_line("PYTHOS:CORE:DYNAMIC_ELF_LOADING_READY");
```

- [x] **Step 4: Run focused tests**

Run:

```powershell
cargo test -p pythos-shared --target x86_64-pc-windows-msvc
cargo test -p pythos-core user_elf --target x86_64-pc-windows-msvc
python -m unittest tests.test_iso_image
python -m unittest tests.test_boot_marker_contract
```

Expected: all pass.

- [x] **Step 5: Run QEMU slice**

Run: `python scripts\test-boot.py --slice dynamic-elf-loading --timeout 60`

Expected:

```text
QEMU_OUTCOME success
BOOT_TEST_OK
```

- [x] **Step 6: Run Phase 8 regression slice**

Run: `python scripts\test-boot.py --slice capability-enforcement-at-boundary --timeout 60`

Expected:

```text
QEMU_OUTCOME success
BOOT_TEST_OK
```

---

## Self-Review Notes

Spec coverage: ADR 0037, legacy runtime compatibility, positive ELF loading, buffer overflow denial, W+X denial, kernel-range denial, BSS zero-fill, overlap rejection, unsupported dynamic segment rejection, QEMU markers, and Phase 8 regression are each mapped to tasks.

Placeholder scan: no task uses TBD/TODO/fill-in wording. Every command has an expected outcome.

Type consistency: `init_bundle`, `runtime_loader`, `user_elf`, and `UserAddressSpace::build_with_user_elf` names are consistent across tasks.
