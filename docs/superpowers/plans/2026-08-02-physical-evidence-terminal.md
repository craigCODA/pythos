# Physical Evidence Terminal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the opt-in physical evidence terminal from ADR 0063 so a serial-less O2 Micro `1217:8620` boot can show the full Phase 10 marker transcript on the framebuffer.

**Architecture:** Add a shared no-alloc `PYLOG001` evidence-log format, expose it through explicit `PythBootInfo` ABI 0.3 fields, have the UEFI loader mirror loader markers into a 64 KiB buffer, and have PythCore validate, map, append to, and render that same buffer. COM1 remains the QEMU oracle; the terminal is a post-proof visual mirror used only by `evidence-terminal` verification images.

**Tech Stack:** Rust `no_std`, UEFI loader, PythCore x86_64 kernel, shared boot protocol crate, PIT ticks, existing framebuffer/font renderer, Python QEMU harness.

## Global Constraints

- Implement only ADR 0063 physical evidence terminal.
- `evidence-terminal` is opt-in on both `pythos-boot` and `pythos-core`.
- `pythos-core` `evidence-terminal` depends on `verify`; it does not run in normal boot.
- Keep COM1 as the automated QEMU oracle and preserve all existing milestone marker names and order.
- Boot ABI minor becomes `0.3`; do not overload reserved fields silently.
- Evidence buffer total length is exactly 64 KiB and must be page-aligned loader RAM.
- Evidence log magic is `PYLOG001`, version `1`, payload lines are ASCII marker lines with `\n`.
- CRC is CRC-32/ISO-HDLC: reflected polynomial `0xEDB88320`, initial state `0xFFFF_FFFF`, reflected byte updates, stored/displayed as `state ^ 0xFFFF_FFFF`.
- `evidence_log_flags` bit `0x0000_0001` means present; all other bits are invalid.
- Terminal page dwell uses PIT ticks; any CPU spin fallback is O2 Micro `1217:8620`-only evidence timing.
- On physical hardware, `qemu_exit::success()` must leave the final framebuffer visible in its non-returning loop.
- Do not add USB, FAT, partition parsing, filesystem writes, DMA/ADMA, interrupts, hotplug, generic SDHCI, networking, package management, or AI.
- Every unsafe block must include the local invariant comment style already used in the repo.
- A successful compile is not a successful boot; finish with an automated QEMU acceptance test.

---

## File Structure

- Create `shared/src/evidence_log.rs`: shared `PYLOG001` header, constants, CRC-32/ISO-HDLC, append/snapshot validation, and host unit tests.
- Modify `shared/src/lib.rs`: export `pub mod evidence_log;`.
- Modify `shared/src/boot_protocol.rs`: bump ABI minor to `3`, add explicit evidence metadata fields, add `PYTH_EVIDENCE_LOG_FLAG_PRESENT`, validate evidence metadata and reserved fields.
- Modify `boot/Cargo.toml`: add `evidence-terminal = []`.
- Create `boot/src/evidence_log.rs`: UEFI allocation wrapper and loader-side marker append helper.
- Modify `boot/src/main.rs`: initialize the log before `PYTHOS:LOADER:ENTER`, mirror loader markers, and pass the allocation into boot-info population.
- Modify `boot/src/boot_info.rs`: include evidence metadata in `BootInfoInputs` and `PythBootInfo`.
- Modify `boot/src/paging.rs`: keep the evidence buffer reachable through loader handoff mappings when the allocation is not already covered by the existing range.
- Modify `core/Cargo.toml`: add `evidence-terminal = ["verify"]`.
- Create `core/src/evidence_log.rs`: feature-gated attachment to the shared log buffer, backfill of early core markers, and serial mirror hook.
- Modify `core/src/serial.rs`: after COM1 write succeeds, append marker lines to the installed evidence log under `evidence-terminal`.
- Modify `core/src/memory/virtual.rs`: map and validate the evidence buffer under PythCore-owned page tables before the identity-map-removal proof.
- Create `core/src/evidence_terminal.rs`: terminal layout, pagination, status formatting, PIT dwell, final-page render, and tests.
- Modify `core/src/framebuffer.rs`: expose the minimal text drawing surface needed by `evidence_terminal`.
- Modify `core/src/font.rs`: add glyphs and tests for terminal chrome and milestone marker characters.
- Modify `core/src/main.rs`: wire attach/map/render flow and emit `PYTHOS:CORE:EVIDENCE_TERMINAL_READY` after final render.
- Create `scripts/test-evidence-terminal.py`: local QEMU acceptance script for `verify,sdhci-emmc-backend,evidence-terminal`.
- Modify `docs/PythOS-TDD-001.md`: record ABI 0.3 fields, evidence-terminal marker, acceptance command, and target-specific physical interpretation.

### Task 1: Shared Evidence Log Format

**Files:**
- Create: `shared/src/evidence_log.rs`
- Modify: `shared/src/lib.rs`

**Interfaces:**
- Produces: `EVIDENCE_LOG_TOTAL_BYTES: usize`, `EVIDENCE_LOG_MAGIC: [u8; 8]`, `EVIDENCE_LOG_VERSION: u32`, `MAX_EVIDENCE_LINE_BYTES: usize`, `EvidenceLogHeader`, `EvidenceLogError`, `EvidenceLogSnapshot<'a>`, `initialize(buffer: &mut [u8]) -> Result<(), EvidenceLogError>`, `append_line(buffer: &mut [u8], line: &str) -> Result<(), EvidenceLogError>`, `snapshot(buffer: &[u8]) -> Result<EvidenceLogSnapshot<'_>, EvidenceLogError>`, and `crc32_iso_hdlc(bytes: &[u8]) -> u32`.
- Consumes: no heap allocation, no target-specific code.

- [ ] **Step 1: Add failing tests for initialization and ABI constants.**

```rust
#[test]
fn initializes_header_and_empty_payload() {
    let mut buffer = [0xA5u8; EVIDENCE_LOG_TOTAL_BYTES];
    initialize(&mut buffer).unwrap();
    let snapshot = snapshot(&buffer).unwrap();
    assert_eq!(snapshot.header.magic, EVIDENCE_LOG_MAGIC);
    assert_eq!(snapshot.header.version, EVIDENCE_LOG_VERSION);
    assert_eq!(snapshot.header.capacity as usize, EVIDENCE_LOG_TOTAL_BYTES - core::mem::size_of::<EvidenceLogHeader>());
    assert_eq!(snapshot.header.used, 0);
    assert_eq!(snapshot.header.lines, 0);
    assert_eq!(snapshot.header.dropped, 0);
    assert_eq!(snapshot.header.crc32, 0);
    assert_eq!(snapshot.payload, b"");
}
```

- [ ] **Step 2: Run the focused shared test and verify it fails before implementation.**

Run: `cargo test -p pythos-shared initializes_header_and_empty_payload`

Expected: FAIL because `shared::evidence_log` does not exist.

- [ ] **Step 3: Add failing append, CRC, and overflow tests.**

```rust
#[test]
fn append_line_records_newline_and_crc32_iso_hdlc() {
    let mut buffer = [0u8; EVIDENCE_LOG_TOTAL_BYTES];
    initialize(&mut buffer).unwrap();
    append_line(&mut buffer, "PYTHOS:LOADER:ENTER").unwrap();
    let snapshot = snapshot(&buffer).unwrap();
    assert_eq!(snapshot.payload, b"PYTHOS:LOADER:ENTER\n");
    assert_eq!(snapshot.header.used as usize, snapshot.payload.len());
    assert_eq!(snapshot.header.lines, 1);
    assert_eq!(snapshot.header.dropped, 0);
    assert_eq!(snapshot.header.crc32, crc32_iso_hdlc(snapshot.payload));
}

#[test]
fn crc32_iso_hdlc_matches_check_value() {
    assert_eq!(crc32_iso_hdlc(b"123456789"), 0xCBF4_3926);
}

#[test]
fn full_buffer_increments_dropped_without_mutating_payload() {
    let mut buffer = [0u8; 96];
    initialize(&mut buffer).unwrap();
    append_line(&mut buffer, "PYTHOS:CORE:PHASE_10_COMPLETE").unwrap();
    let expected_prefix = b"PYTHOS:CORE:PHASE_10_COMPLETE\n";
    let (before_used, before_crc) = {
        let before = snapshot(&buffer).unwrap();
        assert_eq!(&before.payload[..expected_prefix.len()], expected_prefix);
        (before.header.used, before.header.crc32)
    };
    while append_line(&mut buffer, "PYTHOS:CORE:FRAMEBUFFER_READY").is_ok() {}
    let after = snapshot(&buffer).unwrap();
    assert!(after.header.dropped > 0);
    assert_eq!(&after.payload[..expected_prefix.len()], expected_prefix);
    assert_ne!(after.header.crc32, 0);
    assert_eq!(before_used as usize, expected_prefix.len());
    assert_eq!(before_crc, crc32_iso_hdlc(expected_prefix));
}
```

- [ ] **Step 4: Implement the shared module with a packed, explicit header.**

```rust
pub const EVIDENCE_LOG_TOTAL_BYTES: usize = 64 * 1024;
pub const EVIDENCE_LOG_MAGIC: [u8; 8] = *b"PYLOG001";
pub const EVIDENCE_LOG_VERSION: u32 = 1;
pub const MAX_EVIDENCE_LINE_BYTES: usize = 128;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceLogHeader {
    pub magic: [u8; 8],
    pub version: u32,
    pub capacity: u32,
    pub used: u32,
    pub lines: u32,
    pub dropped: u32,
    pub crc32: u32,
}
```

- [ ] **Step 5: Implement append semantics exactly.**

```rust
pub fn append_line(buffer: &mut [u8], line: &str) -> Result<(), EvidenceLogError> {
    if !line.is_ascii() {
        return Err(EvidenceLogError::NonAscii);
    }
    if line.len() > MAX_EVIDENCE_LINE_BYTES {
        return Err(EvidenceLogError::LineTooLong);
    }
    let mut snapshot = snapshot_mut(buffer)?;
    let needed = line.len().checked_add(1).ok_or(EvidenceLogError::LengthOverflow)?;
    if snapshot.header.used as usize + needed > snapshot.header.capacity as usize {
        snapshot.header.dropped = snapshot.header.dropped.saturating_add(1);
        return Err(EvidenceLogError::Full);
    }
    snapshot.append_bytes(line.as_bytes())?;
    snapshot.append_bytes(b"\n")?;
    snapshot.header.lines = snapshot.header.lines.saturating_add(1);
    Ok(())
}
```

- [ ] **Step 6: Export the module.**

```rust
// shared/src/lib.rs
pub mod evidence_log;
```

- [ ] **Step 7: Run shared tests.**

Run: `cargo test -p pythos-shared evidence_log`

Expected: PASS.

- [ ] **Step 8: Commit the shared format.**

```powershell
git add shared/src/lib.rs shared/src/evidence_log.rs
git commit -m "feat: add shared evidence log format"
```

### Task 2: Boot ABI 0.3 Evidence Metadata

**Files:**
- Modify: `shared/src/boot_protocol.rs`
- Modify: `boot/src/boot_info.rs`
- Modify: `docs/PythOS-TDD-001.md`

**Interfaces:**
- Consumes: `pythos_shared::evidence_log::EVIDENCE_LOG_TOTAL_BYTES`.
- Produces: `PYTH_BOOT_ABI_MINOR == 3`, `PYTH_EVIDENCE_LOG_FLAG_PRESENT: u32 = 0x0000_0001`, `PythBootInfo::evidence_log_phys`, `PythBootInfo::evidence_log_len`, and `PythBootInfo::evidence_log_flags`.

- [ ] **Step 1: Add failing boot-protocol tests for ABI 0.3 and evidence metadata validation.**

```rust
#[test]
fn evidence_log_metadata_boot_abi_minor_is_three() {
    assert_eq!(PYTH_BOOT_ABI_MINOR, 3);
    assert_eq!(PYTH_EVIDENCE_LOG_FLAG_PRESENT, 0x0000_0001);
}

#[test]
fn evidence_log_metadata_absent_requires_zero_metadata() {
    let mut info = valid_boot_info();
    info.evidence_log_phys = 0x80_0000;
    assert_eq!(info.validate(), Err(BootInfoError::BadEvidenceLog));
}

#[test]
fn evidence_log_metadata_present_requires_alignment_and_exact_length() {
    let mut info = valid_boot_info();
    info.evidence_log_flags = PYTH_EVIDENCE_LOG_FLAG_PRESENT;
    info.evidence_log_phys = 0x80_0000;
    info.evidence_log_len = EVIDENCE_LOG_TOTAL_BYTES as u32;
    assert_eq!(info.validate(), Ok(()));

    info.evidence_log_phys = 0x80_0001;
    assert_eq!(info.validate(), Err(BootInfoError::BadEvidenceLog));

    info.evidence_log_phys = 0x80_0000;
    info.evidence_log_len = 4096;
    assert_eq!(info.validate(), Err(BootInfoError::BadEvidenceLog));
}
```

- [ ] **Step 2: Run the focused boot-protocol tests and verify they fail.**

Run: `cargo test -p pythos-shared evidence_log_metadata`

Expected: FAIL because the ABI fields and error variant are absent.

- [ ] **Step 3: Update `PythBootInfo` and validation.**

```rust
pub const PYTH_BOOT_ABI_MINOR: u16 = 3;
pub const PYTH_EVIDENCE_LOG_FLAG_PRESENT: u32 = 0x0000_0001;

pub enum BootInfoError {
    BadMagic,
    UnsupportedAbiMajor,
    BadStructSize,
    NonZeroReserved,
    BadMemoryMap,
    BadFramebuffer,
    BadKernelRange,
    BadStackRange,
    BadFont,
    BadEvidenceLog,
}
```

```rust
pub evidence_log_phys: u64,
pub evidence_log_len: u32,
pub evidence_log_flags: u32,
pub reserved: [u64; 6],
```

- [ ] **Step 4: Update loader boot-info construction with explicit zero evidence metadata by default.**

```rust
evidence_log_phys: inputs.evidence_log_phys,
evidence_log_len: inputs.evidence_log_len,
evidence_log_flags: inputs.evidence_log_flags,
reserved: [0; 6],
```

- [ ] **Step 5: Record the ABI 0.3 contract in `docs/PythOS-TDD-001.md`.**

Add this exact contract text near the boot-info ABI section:

```text
ABI 0.3 consumes part of `PythBootInfo.reserved` for the opt-in ADR 0063
evidence log: `evidence_log_phys: u64`, `evidence_log_len: u32`,
`evidence_log_flags: u32`, and `reserved: [u64; 6]`.
`evidence_log_flags & 0x0000_0001` means the 64 KiB `PYLOG001` evidence buffer
is present. Unknown evidence flags are invalid.
```

- [ ] **Step 6: Run boot-protocol tests.**

Run: `cargo test -p pythos-shared boot_protocol`

Expected: PASS.

- [ ] **Step 7: Commit the ABI change.**

```powershell
git add shared/src/boot_protocol.rs boot/src/boot_info.rs docs/PythOS-TDD-001.md
git commit -m "feat: add evidence log boot abi"
```

### Task 3: Loader Evidence Buffer and Marker Mirroring

**Files:**
- Modify: `boot/Cargo.toml`
- Create: `boot/src/evidence_log.rs`
- Modify: `boot/src/main.rs`
- Modify: `boot/src/boot_info.rs`
- Modify: `boot/src/paging.rs`

**Interfaces:**
- Consumes: `pythos_shared::evidence_log::{EVIDENCE_LOG_TOTAL_BYTES, initialize, append_line}` and `PYTH_EVIDENCE_LOG_FLAG_PRESENT`.
- Produces: `boot::evidence_log::AllocatedEvidenceLog`, `boot::evidence_log::write_marker(&mut Option<AllocatedEvidenceLog>, &str)`, and `BootInfoInputs::evidence_log: Option<&AllocatedEvidenceLog>`.

- [ ] **Step 1: Add the loader feature gate.**

```toml
[features]
evidence-terminal = []
```

- [ ] **Step 2: Add failing loader-side unit tests for boot-info evidence fields.**

```rust
#[test]
fn boot_info_populates_present_evidence_metadata() {
    let log = TestEvidenceLog {
        physical_start: 0x80_0000,
        len: EVIDENCE_LOG_TOTAL_BYTES as u32,
    };
    let ptr = allocated_boot_info_with_inputs(Some(&log)).unwrap();
    // SAFETY:
    // 1. Invariant: `allocated_boot_info_with_inputs` returns a pointer to one
    //    initialized `PythBootInfo` in test-owned storage.
    // 2. Established by: the helper constructs and retains the backing object
    //    for the duration of this assertion.
    // 3. Lifetime: valid until the test function exits.
    // 4. Pointer ownership: the helper owns the backing object.
    // 5. Alignment: the helper stores a real `PythBootInfo`.
    // 6. Mapped length: exactly one `PythBootInfo`.
    // 7. Concurrency: single-threaded unit test.
    // 8. Violation: a bad helper pointer would make the test fail or fault.
    let info = unsafe { &*ptr };
    assert_eq!(info.evidence_log_phys, 0x80_0000);
    assert_eq!(info.evidence_log_len, EVIDENCE_LOG_TOTAL_BYTES as u32);
    assert_eq!(info.evidence_log_flags, PYTH_EVIDENCE_LOG_FLAG_PRESENT);
}
```

- [ ] **Step 3: Run the focused loader boot-info test and verify it fails.**

Run: `cargo test -p pythos-boot boot_info_populates_present_evidence_metadata`

Expected: FAIL because `BootInfoInputs` has no evidence-log field.

- [ ] **Step 4: Implement `AllocatedEvidenceLog::allocate`.**

```rust
pub(crate) struct AllocatedEvidenceLog {
    physical_start: u64,
    len: u32,
    ptr: *mut u8,
}

impl AllocatedEvidenceLog {
    pub(crate) fn allocate(system_table: *mut EfiSystemTable) -> Result<Self, ()> {
        let boot_services = uefi::boot_services(system_table).map_err(|_| ())?;
        let mut physical_start: uefi::EfiPhysicalAddress = 0;
        // SAFETY:
        // 1. Invariant: `boot_services` points to the active UEFI boot services
        //    table and `physical_start` is a valid out-parameter.
        // 2. Established by: `uefi::boot_services()` from the firmware system table.
        // 3. Lifetime: allocation remains owned by the loader until PythCore handoff.
        // 4. Pointer ownership: firmware writes only the output physical address.
        // 5. Alignment: UEFI page allocation returns 4 KiB-aligned memory.
        // 6. Mapped length: exactly `EVIDENCE_LOG_TOTAL_BYTES / 4096` pages.
        // 7. Concurrency: single-threaded loader.
        // 8. Violation: invalid boot services would call through a bad pointer.
        let status = unsafe {
            ((*boot_services).allocate_pages)(
                uefi::EFI_ALLOCATE_ANY_PAGES,
                uefi::EFI_LOADER_DATA,
                EVIDENCE_LOG_TOTAL_BYTES / 4096,
                &mut physical_start,
            )
        };
        if status != uefi::EFI_SUCCESS || physical_start == 0 {
            return Err(());
        }
        let ptr = physical_start as *mut u8;
        // SAFETY:
        // 1. Invariant: `ptr` is the page-aligned allocation returned above.
        // 2. Established by: successful UEFI `allocate_pages`.
        // 3. Lifetime: retained until PythCore handoff.
        // 4. Pointer ownership: loader owns the allocation.
        // 5. Alignment: UEFI page allocation provides page alignment.
        // 6. Mapped length: `EVIDENCE_LOG_TOTAL_BYTES`.
        // 7. Concurrency: single-threaded loader.
        // 8. Violation: a wrong length could corrupt adjacent loader memory.
        let buffer = unsafe { core::slice::from_raw_parts_mut(ptr, EVIDENCE_LOG_TOTAL_BYTES) };
        pythos_shared::evidence_log::initialize(buffer).map_err(|_| ())?;
        Ok(Self {
            physical_start,
            len: EVIDENCE_LOG_TOTAL_BYTES as u32,
            ptr,
        })
    }
}
```

The allocation must use UEFI `allocate_pages` for 4 KiB alignment, not pool allocation.

- [ ] **Step 5: Add loader marker helper and replace direct marker writes in `boot/src/main.rs`.**

```rust
#[cfg(feature = "evidence-terminal")]
fn loader_marker(log: &mut Option<evidence_log::AllocatedEvidenceLog>, marker: &str) {
    serial::write_line(marker);
    if let Some(log) = log.as_mut() {
        let _ = log.append_line(marker);
    }
}

#[cfg(not(feature = "evidence-terminal"))]
fn loader_marker(_log: &mut Option<()>, marker: &str) {
    serial::write_line(marker);
}
```

`PYTHOS:LOADER:ENTER` must be emitted through this helper after COM1 init and after the evidence buffer allocation attempt.

- [ ] **Step 6: Keep allocation failure visible.**

In an `evidence-terminal` loader build, if allocation fails, write `PYTHOS:LOADER:EVIDENCE_LOG_ALLOC_FAILED` to COM1, continue booting with zero evidence metadata, and let PythCore fail the evidence-terminal acceptance path after `PYTHOS:CORE:BOOTINFO_VALID`.

- [ ] **Step 7: Pass evidence metadata through both `populate` calls.**

The stale-map-key retry path must pass the same `AllocatedEvidenceLog` reference as the first populate call.

- [ ] **Step 8: Ensure loader handoff mappings include the evidence buffer.**

If `paging::build` does not already cover the evidence allocation through its temporary identity map, add an explicit page-mapping input for the log range and map it writable, non-executable.

- [ ] **Step 9: Run loader tests and a boot-only build.**

Run:

```powershell
cargo test -p pythos-boot boot_info
cargo build -p pythos-boot --target x86_64-unknown-uefi --features evidence-terminal
```

Expected: PASS.

- [ ] **Step 10: Commit loader integration.**

```powershell
git add boot/Cargo.toml boot/src/evidence_log.rs boot/src/main.rs boot/src/boot_info.rs boot/src/paging.rs
git commit -m "feat: mirror loader markers to evidence log"
```

### Task 4: PythCore Evidence Log Attachment and Mapping

**Files:**
- Modify: `core/Cargo.toml`
- Create: `core/src/evidence_log.rs`
- Modify: `core/src/main.rs`
- Modify: `core/src/serial.rs`
- Modify: `core/src/memory/virtual.rs`

**Interfaces:**
- Consumes: `PythBootInfo` evidence metadata and `pythos_shared::evidence_log`.
- Produces: `evidence_log::attach_from_boot_info(&PythBootInfo) -> Result<(), EvidenceLogAttachError>`, `evidence_log::append_marker(marker: &str)`, and `memory::virtual::KernelAddressSpace::build(..., evidence_log_mapping: Option<(u64, u64, u64)>)`.

- [ ] **Step 1: Add the core feature gate.**

```toml
evidence-terminal = ["verify"]
```

- [ ] **Step 2: Add failing tests for the serial mirror hook.**

```rust
#[test]
fn serial_mirror_after_line_written_appends_after_install() {
    let mut buffer = [0u8; EVIDENCE_LOG_TOTAL_BYTES];
    pythos_shared::evidence_log::initialize(&mut buffer).unwrap();
    evidence_log::install_for_test(&mut buffer).unwrap();
    serial::after_line_written_for_test("PYTHOS:CORE:FRAMEBUFFER_READY");
    let snapshot = pythos_shared::evidence_log::snapshot(&buffer).unwrap();
    assert_eq!(snapshot.payload, b"PYTHOS:CORE:FRAMEBUFFER_READY\n");
}
```

- [ ] **Step 3: Run the focused test and verify it fails.**

Run: `cargo test -p pythos-core serial_mirror_after_line_written_appends_after_install --features evidence-terminal`

Expected: FAIL because `core::evidence_log` and the serial hook do not exist.

- [ ] **Step 4: Add a testable post-serial hook in `core/src/serial.rs`.**

```rust
pub fn write_line(line: &str) {
    write_str(line);
    write_str("\r\n");
    after_line_written(line);
}

fn after_line_written(line: &str) {
    #[cfg(feature = "evidence-terminal")]
    crate::evidence_log::append_marker(line);
}

#[cfg(test)]
pub(crate) fn after_line_written_for_test(line: &str) {
    after_line_written(line);
}
```

- [ ] **Step 5: Implement a single-core evidence-log attachment.**

```rust
#[cfg(feature = "evidence-terminal")]
pub fn attach_from_boot_info(boot_info: &PythBootInfo) -> Result<(), EvidenceLogAttachError> {
    if boot_info.evidence_log_flags & PYTH_EVIDENCE_LOG_FLAG_PRESENT == 0 {
        return Err(EvidenceLogAttachError::Absent);
    }
    let len = usize::try_from(boot_info.evidence_log_len).map_err(|_| EvidenceLogAttachError::BadLength)?;
    let ptr = boot_info.evidence_log_phys as *mut u8;
    // SAFETY:
    // 1. Invariant: `ptr..ptr+len` is the 64 KiB loader-owned evidence buffer
    //    advertised by validated ABI 0.3 boot info.
    // 2. Established by: `PythBootInfo::validate` checked alignment, flags, and
    //    exact length before this function is called.
    // 3. Lifetime: loader allocations are retained for the boot proof.
    // 4. Pointer ownership: PythCore owns the allocation after handoff.
    // 5. Alignment: page alignment is validated by the boot protocol.
    // 6. Mapped length: exactly `len` bytes.
    // 7. Concurrency: single-core verification path.
    // 8. Violation: a bad pointer corrupts memory or faults before rendering.
    let buffer = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
install(buffer)
}
```

- [ ] **Step 6: Backfill early core markers before installing the serial hook.**

After `PYTHOS:CORE:BOOTINFO_VALID`, attach and append:

```rust
evidence_log::append_marker("PYTHOS:CORE:ENTER");
evidence_log::append_marker("PYTHOS:CORE:BOOTINFO_VALID");
```

Only after those two backfilled lines should `serial::write_line` mirror later core markers automatically.

- [ ] **Step 7: Map the evidence buffer into the PythCore page tables.**

Extend `KernelAddressSpace::build` with an `evidence_log_mapping` argument. For `evidence-terminal`, pass:

```rust
Some((
    boot_info.evidence_log_phys,
    boot_info.evidence_log_phys,
    u64::from(boot_info.evidence_log_len),
))
```

Map it writable and non-executable with `tables.map_physical_range(virt, phys, len, PTE_WRITE | PTE_NO_EXECUTE)`.

- [ ] **Step 8: Validate active mapping.**

In `KernelAddressSpace::validate_active`, when the evidence-present flag is set, require `translate_active(boot_info.evidence_log_phys).is_ok()`.

- [ ] **Step 9: Run core evidence and VM tests.**

Run:

```powershell
cargo test -p pythos-core evidence_log --features evidence-terminal
cargo test -p pythos-core serial_mirror --features evidence-terminal
cargo test -p pythos-core virtual
```

Expected: PASS.

- [ ] **Step 10: Commit core attachment and mapping.**

```powershell
git add core/Cargo.toml core/src/evidence_log.rs core/src/main.rs core/src/serial.rs core/src/memory/virtual.rs
git commit -m "feat: attach core evidence log"
```

### Task 5: Framebuffer Terminal Renderer

**Files:**
- Create: `core/src/evidence_terminal.rs`
- Modify: `core/src/framebuffer.rs`
- Modify: `core/src/font.rs`
- Modify: `core/src/main.rs`

**Interfaces:**
- Consumes: `pythos_shared::evidence_log::EvidenceLogSnapshot<'_>`, `PythFramebufferInfo`, and `architecture::x86_64::timer::ticks()`.
- Produces: `evidence_terminal::render(snapshot: &EvidenceLogSnapshot<'_>, framebuffer: &PythFramebufferInfo) -> Result<(), EvidenceTerminalError>`, `evidence_terminal::page_count(snapshot, terminal_rows) -> usize`, and marker `PYTHOS:CORE:EVIDENCE_TERMINAL_READY`.

- [ ] **Step 1: Add failing pagination and status-format tests.**

```rust
#[test]
fn page_count_uses_rows_remaining_after_chrome() {
    let lines = 73;
    assert_eq!(page_count_for_lines(lines, 20), 5);
}

#[test]
fn status_line_formats_count_drop_and_crc_as_hex() {
    let line = format_status_line(1, 4, 242, 0, 0x8A31_C04E);
    assert_eq!(line, "page 01/04 count 000000F2 drop 00000000 crc 8A31C04E");
}
```

- [ ] **Step 2: Add failing glyph coverage test.**

```rust
#[test]
fn terminal_glyphs_cover_marker_charset() {
    for byte in b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:_-/ >" {
        assert!(font::glyph(*byte).is_some(), "missing glyph for {}", *byte as char);
    }
}
```

- [ ] **Step 3: Run focused renderer tests and verify they fail.**

Run: `cargo test -p pythos-core evidence_terminal terminal_glyphs_cover_marker_charset --features evidence-terminal`

Expected: FAIL because the renderer and missing glyphs are absent.

- [ ] **Step 4: Implement terminal geometry.**

Use these constants:

```rust
const GLYPH_W: u64 = 8;
const GLYPH_H: u64 = 8;
const SCALE: u64 = 1;
const MARGIN_X: u64 = 24;
const MARGIN_Y: u64 = 24;
const ROW_GAP: u64 = 2;
const CHROME_ROWS: usize = 3;
```

Compute columns as `(framebuffer.width as u64 - MARGIN_X * 2) / (GLYPH_W * SCALE)` and rows as `(framebuffer.height as u64 - MARGIN_Y * 2) / (GLYPH_H * SCALE + ROW_GAP)`.

- [ ] **Step 5: Render the terminal page sequence.**

For each page, clear the framebuffer to a dark terminal background, draw:

```text
PythOS Evidence Terminal
page NN/MM count XXXXXXXX drop XXXXXXXX crc XXXXXXXX

> PYTHOS:...
```

Wrap lines only at character boundaries. If a wrapped marker would exceed the page, continue it on the next page with a leading space instead of a second `>` prefix.

- [ ] **Step 6: Use PIT ticks for dwell.**

After each non-final page, wait exactly `200` PIT ticks. If `architecture::x86_64::timer::ticks()` does not advance for one bounded probe interval, use a documented spin fallback guarded by `#[cfg(feature = "evidence-terminal")]` and comment that it is verified only on O2 Micro `1217:8620`.

- [ ] **Step 7: Emit the post-render success marker after the final page is visible.**

In `core/src/main.rs`, order the end of the verification path as:

```rust
serial::write_line("PYTHOS:CORE:FRAMEBUFFER_READY");
serial::write_line("PYTHOS:CORE:MILESTONE_1_COMPLETE");
#[cfg(feature = "evidence-terminal")]
{
    let snapshot = match evidence_log::snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
    };
    if evidence_terminal::render(&snapshot, &boot_info.framebuffer).is_err() {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
    if snapshot.header.dropped == 0 {
        serial::write_line("PYTHOS:CORE:EVIDENCE_TERMINAL_READY");
    } else {
        serial::write_line("PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED");
        qemu_exit::panic();
    }
}
qemu_exit::success();
```

If `snapshot()` or `render()` fails, emit `PYTHOS:PANIC` and call `qemu_exit::panic()`.

- [ ] **Step 8: Run renderer and framebuffer tests.**

Run:

```powershell
cargo test -p pythos-core evidence_terminal --features evidence-terminal
cargo test -p pythos-core framebuffer
cargo test -p pythos-core font
```

Expected: PASS.

- [ ] **Step 9: Commit terminal rendering.**

```powershell
git add core/src/evidence_terminal.rs core/src/framebuffer.rs core/src/font.rs core/src/main.rs
git commit -m "feat: render physical evidence terminal"
```

### Task 6: QEMU Acceptance Harness

**Files:**
- Create: `scripts/test-evidence-terminal.py`
- Modify: `docs/PythOS-TDD-001.md`

**Interfaces:**
- Consumes: `scripts/run-qemu.py --success-marker`, boot/core feature gates, and `scripts/build-iso.py`.
- Produces: `EVIDENCE_TERMINAL_TEST_OK` and screendump `target/evidence-terminal.ppm`.

- [ ] **Step 1: Create the acceptance script with exact build commands.**

```python
def build_boot_iso() -> None:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi", "--features", "evidence-terminal"])
    run([
        "cargo",
        "build",
        "-p",
        "pythos-core",
        "--target",
        "x86_64-unknown-none",
        "--features",
        "verify,sdhci-emmc-backend,evidence-terminal",
    ])
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-iso.py"])
```

- [ ] **Step 2: Boot with the SDHCI/eMMC backend and delayed success marker.**

```python
run([
    sys.executable,
    "scripts/run-qemu.py",
    "--iso",
    str(ISO),
    "--serial-log",
    str(SERIAL_LOG),
    "--timeout",
    "75",
    "--screendump",
    str(SCREENDUMP),
    "--success-marker",
    "PYTHOS:CORE:EVIDENCE_TERMINAL_READY",
    "--no-virtio-blk",
    "--sdhci",
    "--emmc",
    "--emmc-image",
    str(EMMC_IMAGE),
    "--expect-outcome",
    "success",
])
```

- [ ] **Step 3: Assert existing milestone markers remain ordered and present.**

Require these markers in serial:

```python
REQUIRED_MARKERS = (
    "PYTHOS:LOADER:ENTER",
    "PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK",
    "PYTHOS:CORE:BOOTINFO_VALID",
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC",
    "PYTHOS:CORE:OBJECT_STORE:PERSISTED",
    "PYTHOS:CORE:OBJECT_STORE:RESTORED",
    "PYTHOS:CORE:GENERAL_STORAGE:PERSISTED",
    "PYTHOS:CORE:GENERAL_STORAGE:RESTORED",
    "PYTHOS:CORE:PHASE_10_COMPLETE",
    "PYTHOS:CORE:FRAMEBUFFER_READY",
    "PYTHOS:CORE:MILESTONE_1_COMPLETE",
    "PYTHOS:CORE:EVIDENCE_TERMINAL_READY",
)
```

Also reject:

```python
FORBIDDEN_MARKERS = (
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO",
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI",
    "PYTHOS:PANIC",
)
```

- [ ] **Step 4: Assert dropped-transcript boots cannot pass.**

The acceptance script must fail if the dropped marker appears:

```python
if "PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED" in output:
    raise AssertionError("evidence terminal dropped transcript lines")
```

- [ ] **Step 5: Verify the screendump is the evidence terminal frame.**

```python
assert_screendump_shows_evidence_terminal(serial)
```

- [ ] **Step 6: Run the acceptance script.**

Run: `python scripts/test-evidence-terminal.py`

Expected: PASS with `EVIDENCE_TERMINAL_TEST_OK` and `QEMU_OUTCOME success`.

- [ ] **Step 7: Commit the acceptance harness.**

```powershell
git add scripts/test-evidence-terminal.py docs/PythOS-TDD-001.md
git commit -m "test: add evidence terminal acceptance"
```

### Task 7: Final Verification and Documentation Sync

**Files:**
- Modify: `docs/superpowers/specs/2026-08-02-physical-evidence-terminal-design.md`
- Modify: `docs/HANDOVER.md`
- Modify: `docs/milestones/2026-08-01-physical-emmc-phase10.md`

**Interfaces:**
- Consumes: completed Tasks 1-6.
- Produces: current handover/status text that says evidence-terminal is implemented only after QEMU acceptance passes, and still target-specific until physical video evidence is supplied.

- [ ] **Step 1: Run formatting and host tests.**

```powershell
cargo fmt --check
cargo test -p pythos-shared evidence_log
cargo test -p pythos-shared boot_protocol
cargo test -p pythos-boot boot_info
cargo test -p pythos-core evidence_log --features evidence-terminal
cargo test -p pythos-core evidence_terminal --features evidence-terminal
cargo test -p pythos-core framebuffer
cargo test -p pythos-core font
```

Expected: PASS.

- [ ] **Step 2: Run target builds and clippy.**

```powershell
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify,evidence-terminal -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify,sdhci-emmc-backend,evidence-terminal -- -D warnings
cargo build -p pythos-boot --target x86_64-unknown-uefi --features evidence-terminal
cargo build -p pythos-core --target x86_64-unknown-none --features verify,sdhci-emmc-backend,evidence-terminal
```

Expected: PASS.

- [ ] **Step 3: Run boot acceptance.**

```powershell
python scripts/test-evidence-terminal.py
```

Expected: PASS with `EVIDENCE_TERMINAL_TEST_OK`, `QEMU_OUTCOME success`, serial markers through `PYTHOS:CORE:EVIDENCE_TERMINAL_READY`, and a non-empty `target/evidence-terminal.ppm`.

- [ ] **Step 4: Run baseline acceptance that must remain unchanged.**

```powershell
python scripts/test-sdhci-emmc-block-device.py
```

Expected: PASS with `SDHCI_EMMC_BLOCK_DEVICE_TEST_OK`; existing success marker remains `PYTHOS:CORE:MILESTONE_1_COMPLETE` for this script.

- [ ] **Step 5: Run safety scripts.**

```powershell
python C:\Users\NeverAMoment\.codex\skills\pythos-kernel-engineer\scripts\check-storage-constants.py .
python C:\Users\NeverAMoment\.codex\skills\pythos-kernel-engineer\scripts\verify-driver-timeouts.py .
python C:\Users\NeverAMoment\.codex\skills\pythos-kernel-engineer\scripts\scan-unsafe-rust.py .
```

Expected: PASS or report only pre-existing baseline findings outside the files touched by ADR 0063.

- [ ] **Step 6: Update docs with exact acceptance output.**

Add the successful commands, marker endpoints, screendump path, and current commit SHA to `docs/HANDOVER.md` and the Phase 10 milestone page. Keep wording explicit that physical evidence-terminal validation has not occurred until the user boots the image and supplies the new photo/video.

- [ ] **Step 7: Commit final implementation state.**

```powershell
git add shared/src boot/src core/src scripts/test-evidence-terminal.py docs/PythOS-TDD-001.md docs/HANDOVER.md docs/milestones/2026-08-01-physical-emmc-phase10.md docs/superpowers/specs/2026-08-02-physical-evidence-terminal-design.md
git commit -m "feat: add physical evidence terminal"
```

## Self-Review

- Spec coverage: ADR 0063 and the approved design are covered by Tasks 1-7: shared format, ABI 0.3, loader ownership, PythCore mapping, terminal renderer, PIT dwell, QEMU acceptance, and documentation sync.
- Scope boundary: no task adds USB, FAT, partitions, filesystem writes, DMA/ADMA, interrupts, hotplug, networking, package management, AI, or generic physical SDHCI support.
- ABI boundary: Task 2 defines every new boot-info field and the only valid evidence-log flag; Tasks 3-4 consume those fields explicitly.
- Oracle boundary: Task 6 keeps COM1 serial as the acceptance oracle and uses `PYTHOS:CORE:EVIDENCE_TERMINAL_READY` only so the screendump occurs after terminal rendering.
- Physical boundary: Task 7 keeps the new physical claim pending until the user supplies evidence from the O2 Micro `1217:8620` boot.
