# Phase 9 General Syscall ABI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Implement the Phase 9 `general-syscall-abi` slice: PythCore freezes a versioned syscall number space, keeps the Phase 8 system-log proof syscall permanent, adds a side-effect-free ABI-info syscall, denies unknown syscall numbers, and proves the behavior through host tests and QEMU markers.

**Architecture:** Add ADR 0038, refactor `core/src/syscall.rs` around a fixed allocation-free syscall table, route dispatch through that table, and emit a new boot proof after dynamic ELF loading. Extend the QEMU marker harness and docs so the next allowed slice is `copy-in-copy-out-policy`.

**Tech Stack:** Rust `no_std` in `core`, Python stdlib marker tests, existing QEMU serial marker harness.

## Global Constraints

Do not accept user pointers.
Do not copy user buffers.
Do not execute the dynamically loaded ELF.
Do not introduce argv/env.
Do not grant dynamic capabilities.
Do not add package loading, networking, updates, hardware expansion, SMP, semantic indexing, local AI, or vision-layer behavior.
Required markers: `PYTHOS:CORE:SYSCALL_ABI:VERSIONED`, `PYTHOS:CORE:SYSCALL_ABI:KNOWN_DISPATCH`, `PYTHOS:CORE:SYSCALL_ABI:UNKNOWN_DENIED`, `PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY`.

---

## File Structure

Create `docs/decisions/0038-phase-9-general-syscall-abi.md` for the ABI freeze-point ADR.

Modify `core/src/syscall.rs` to add ABI version constants, a fixed syscall registry, the ABI-info syscall, unknown-number rejection, and a native self-test used by the boot proof.

Modify `core/src/main.rs` to run the general syscall ABI proof after `DYNAMIC_ELF_LOADING_READY` and before `FRAMEBUFFER_READY`.

Modify `scripts/test-boot.py`, `tests/test_boot_marker_contract.py`, and `tests/boot_core_handoff.py` to add the new slice marker contract.

Modify `docs/ROADMAP.md`, `docs/PythOS-TDD-001.md`, and `AGENTS.md` to mark the slice complete and halt at `copy-in-copy-out-policy`.

---

### Task 1: ADR 0038

**Files:**
- Create: `docs/decisions/0038-phase-9-general-syscall-abi.md`

**Interfaces:**
- Produces: durable syscall versioning policy and permanent number assignments.

- [x] **Step 1: Write ADR 0038**

Add an ADR that freezes ABI `1.0`, reserves `0x5059_0000` for `SYSCALL_ABI_INFO`, preserves `0x5059_0001` as `SYSCALL_SYSTEM_LOG_PROOF`, forbids number reuse, requires ADR updates for additions, and states that user pointer validation remains Phase 9 `copy-in-copy-out-policy`.

- [x] **Step 2: Review ADR for scope leakage**

Confirm the ADR does not define pointer arguments, loaded ELF execution, dynamic process grants, filesystem loading, packages, networking, updates, or SMP.

---

### Task 2: Host Tests for Syscall ABI Registry

**Files:**
- Modify: `core/src/syscall.rs`

**Interfaces:**
- Consumes: existing `SYSCALL_SYSTEM_LOG_PROOF`.
- Produces: `SYSCALL_ABI_INFO`, ABI version constants, registry validation, known and unknown dispatch behavior.

- [x] **Step 1: Write failing tests**

Add tests inside `core/src/syscall.rs`:

```rust
#[test]
fn abi_version_and_info_result_are_stable() {
    assert_eq!(SYSCALL_ABI_MAJOR, 1);
    assert_eq!(SYSCALL_ABI_MINOR, 0);
    assert_eq!(SYSCALL_ABI_INFO, 0x5059_0000);
    assert_eq!(abi_info_result(), 0x5059_0001_0000);
}

#[test]
fn syscall_registry_is_sorted_and_duplicate_free() {
    assert_eq!(validate_syscall_table(SYSCALL_TABLE), Ok(()));
}

#[test]
fn system_log_proof_number_is_permanent() {
    assert_eq!(SYSCALL_SYSTEM_LOG_PROOF, 0x5059_0001);
    let entry = lookup_syscall(SYSCALL_SYSTEM_LOG_PROOF).unwrap();
    assert_eq!(entry.name, "SYSCALL_SYSTEM_LOG_PROOF");
}

#[test]
fn abi_info_dispatch_returns_version_metadata() {
    EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
    assert_eq!(dispatch(SYSCALL_ABI_INFO), Ok(abi_info_result()));
}

#[test]
fn unknown_syscall_number_is_denied_by_registry() {
    EXPECTED_SYSCALL.store(true, Ordering::SeqCst);
    assert_eq!(
        dispatch(0x5059_FFFF),
        Err(SyscallError::UnsupportedNumber)
    );
}
```

- [x] **Step 2: Run tests to verify failure**

Run: `cargo test -p pythos-core syscall --target x86_64-pc-windows-msvc`

Expected: compile failure for missing ABI constants and registry helpers.

- [x] **Step 3: Implement minimal registry**

Add:

```rust
pub const SYSCALL_ABI_MAJOR: u16 = 1;
pub const SYSCALL_ABI_MINOR: u16 = 0;
pub const SYSCALL_ABI_INFO: u64 = 0x5059_0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyscallDispatchKind {
    AbiInfo,
    SystemLogProof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SyscallEntry {
    number: u64,
    name: &'static str,
    introduced_major: u16,
    introduced_minor: u16,
    dispatch_kind: SyscallDispatchKind,
}
```

Define `SYSCALL_TABLE` with ABI-info and system-log entries, plus `lookup_syscall`, `validate_syscall_table`, and `abi_info_result`.

- [x] **Step 4: Route dispatch through the registry**

Change `dispatch(number)` to return `Result<u64, SyscallError>`. It still rejects unexpected calls first. Then it looks up `number`, returns `UnsupportedNumber` on miss, handles `AbiInfo` by returning `abi_info_result()`, and handles `SystemLogProof` by running the existing bridge and returning `SYSCALL_OK`.

- [x] **Step 5: Update ABI return mapping**

Update `syscall_dispatch_abi` so successful dispatch returns the `u64` produced by `dispatch`. Keep existing error code behavior for unexpected and generic dispatch failures, and add an explicit unsupported-number error code.

- [x] **Step 6: Run tests to verify pass**

Run: `cargo test -p pythos-core syscall --target x86_64-pc-windows-msvc`

Expected: all syscall tests pass.

---

### Task 3: Boot Proof and Marker Contract

**Files:**
- Modify: `core/src/syscall.rs`
- Modify: `core/src/main.rs`
- Modify: `scripts/test-boot.py`
- Modify: `tests/test_boot_marker_contract.py`
- Modify: `tests/boot_core_handoff.py`

**Interfaces:**
- Produces: `syscall::run_general_abi_self_test() -> Result<GeneralSyscallAbiProof, SyscallError>`
- Produces boot slice: `general-syscall-abi`

- [x] **Step 1: Add marker contract tests**

In `tests/test_boot_marker_contract.py`, add constants and assertions proving `GENERAL_SYSCALL_ABI_READY` follows `DYNAMIC_ELF_LOADING_READY` and precedes `FRAMEBUFFER_READY`.

In `tests/boot_core_handoff.py`, add:

```python
def test_general_syscall_abi_markers_are_observed_after_dynamic_elf_loading(self) -> None:
    self.run_boot_slice("general-syscall-abi")
```

In `scripts/test-boot.py`, add:

```python
GENERAL_SYSCALL_ABI_MARKERS = [
    "PYTHOS:CORE:SYSCALL_ABI:VERSIONED",
    "PYTHOS:CORE:SYSCALL_ABI:KNOWN_DISPATCH",
    "PYTHOS:CORE:SYSCALL_ABI:UNKNOWN_DENIED",
    "PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY",
]

SLICE_MARKERS["general-syscall-abi"] = (
    SLICE_MARKERS["dynamic-elf-loading"] + GENERAL_SYSCALL_ABI_MARKERS
)
SLICE_MARKERS["milestone-1"] = insert_before(
    SLICE_MARKERS["milestone-1"],
    "PYTHOS:CORE:FRAMEBUFFER_READY",
    GENERAL_SYSCALL_ABI_MARKERS,
)
```

- [x] **Step 2: Run marker tests to verify failure**

Run: `python -m unittest tests.test_boot_marker_contract`

Expected: failure because the markers are not in the contract yet or not emitted yet.

- [x] **Step 3: Add the native ABI self-test**

In `core/src/syscall.rs`, add `GeneralSyscallAbiProof` and `run_general_abi_self_test()` that validates the table, dispatches `SYSCALL_ABI_INFO`, dispatches `SYSCALL_SYSTEM_LOG_PROOF`, and proves `0x5059_FFFF` returns `UnsupportedNumber`.

- [x] **Step 4: Emit boot markers**

In `core/src/main.rs`, immediately after `PYTHOS:CORE:DYNAMIC_ELF_LOADING_READY`, run the self-test and emit:

```text
PYTHOS:CORE:SYSCALL_ABI:VERSIONED
PYTHOS:CORE:SYSCALL_ABI:KNOWN_DISPATCH
PYTHOS:CORE:SYSCALL_ABI:UNKNOWN_DENIED
PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY
```

Panic on failure.

- [x] **Step 5: Run marker tests to verify pass**

Run: `python -m unittest tests.test_boot_marker_contract`

Expected: pass.

---

### Task 4: Documentation and Boundary Update

**Files:**
- Modify: `docs/ROADMAP.md`
- Modify: `docs/PythOS-TDD-001.md`
- Modify: `AGENTS.md`

**Interfaces:**
- Produces: current-state documentation that says `general-syscall-abi` is complete and the next allowed slice is `copy-in-copy-out-policy`.

- [x] **Step 1: Update roadmap**

Mark Phase 9 `general-syscall-abi` complete with ADR 0038 and the four markers. Do not mark `copy-in-copy-out-policy` complete.

- [x] **Step 2: Update TDD and AGENTS**

Add the slice description and marker order. Change the halt boundary to Phase 9 `general-syscall-abi` -> `copy-in-copy-out-policy`.

- [x] **Step 3: Run documentation marker greps**

Run: `rg -n "GENERAL_SYSCALL_ABI_READY|copy-in-copy-out-policy|ADR 0038" AGENTS.md docs`

Expected: the new boundary and ADR references are present, and no later Phase 9 slice is claimed complete.

---

### Task 5: Verification and Commit

**Files:**
- Verify all touched files.

- [x] **Step 1: Format**

Run: `cargo fmt --check`

Expected: pass.

- [x] **Step 2: Host tests**

Run:

```powershell
cargo test -p pythos-shared --target x86_64-pc-windows-msvc
cargo test -p pythos-core --target x86_64-pc-windows-msvc
python -m unittest tests.test_boot_marker_contract
```

Expected: pass.

- [x] **Step 3: Clippy**

Run:

```powershell
cargo clippy -p pythos-core --target x86_64-unknown-none -- -D warnings
cargo clippy -p pythos-boot --target x86_64-unknown-uefi -- -D warnings
```

Expected: pass.

- [x] **Step 4: QEMU slices**

Run:

```powershell
python scripts\test-boot.py --slice general-syscall-abi --timeout 60
python scripts\test-boot.py --slice dynamic-elf-loading --timeout 60
python scripts\test-boot.py --slice milestone-1 --timeout 60
python scripts\test-boot.py --slice milestone-1 --media iso --timeout 60
```

Expected each run includes:

```text
QEMU_OUTCOME success
BOOT_TEST_OK
```

- [x] **Step 5: Commit and push**

Run:

```powershell
git status --short
git add -A
git commit -m "feat: add phase 9 general syscall abi"
git push
```

Expected: commit succeeds and branch pushes.

---

## Self-Review Notes

Spec coverage: ADR 0038, ABI version constants, permanent number assignments, registry dispatch, unknown-number denial, QEMU markers, and the next slice boundary are mapped to tasks.

Placeholder scan: no task uses TBD/TODO/fill-in wording. Commands and expected outcomes are explicit.

Type consistency: `SYSCALL_ABI_INFO`, `SYSCALL_SYSTEM_LOG_PROOF`, `GeneralSyscallAbiProof`, and `run_general_abi_self_test` names are consistent across tasks.
