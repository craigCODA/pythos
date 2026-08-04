# PythTIG Phase 2 Ring-3 Runtime Implementation Plan

**Status:** Accepted future phase pending prior PythTIG phase evidence and
explicit owner invocation. Do not implement this plan until the owner explicitly
invokes this phase.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Use TDD for every runtime, loader, and ABI change.

**Goal:** Launch a verified PythTIG package through a generic ring-3 runtime and execute a bounded `SystemLog` graph without changing the default object-shell boot path.

**Architecture:** PythCore validates the graph with the shared verifier, maps it read-only, binds declared capability imports, and launches `pyth-runtime.elf`. The runtime interprets verified nodes with fixed-size storage and typed syscalls. A test-only boot-control sector selects this path.

**Tech Stack:** Rust `no_std` runtime ELF, shared ABI, PythCore loader/mapper, Python QEMU harness.

## Global Constraints

- Default normal boot still launches the existing object shell.
- Invalid graph packages are rejected before user-mode entry.
- The runtime has no allocator in version 1.
- Graph package and bootstrap block are read-only in user space.
- Capability bindings come from PythCore policy, not package bytes.
- The first accepted graph uses only `EffectStart`, `ConstUtf8`, `SystemLog`, and `Return`.

---

### Task 1: Define graph manifest and runtime bootstrap ABI

**Files:**
- Create: `shared/src/pyth_graph_manifest.rs`
- Create: `shared/src/pyth_runtime_abi.rs`
- Modify: `shared/src/lib.rs`
- Modify: `shared/src/init_bundle.rs`

**Interfaces:**
- Produces: `TYPE_PYTH_GRAPH_PACKAGE`, `NamedPythGraphManifest`, `PythGraphBootstrapBlock`, `PythGraphCapabilityBinding`, `GraphExitRecord`.

- [ ] **Step 1: Write failing ABI tests**

In `shared/src/pyth_runtime_abi.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_bootstrap_and_exit_layouts_are_stable() {
        assert_eq!(PYTH_GRAPH_BOOTSTRAP_MAGIC, 0x3154_4F4F_4247_5950);
        assert_eq!(core::mem::size_of::<PythGraphCapabilityBinding>(), 24);
        assert_eq!(core::mem::size_of::<PythGraphBootstrapBlock>(), 816);
        assert_eq!(core::mem::size_of::<GraphExitRecord>(), 32);
        assert_eq!(MAX_PYTH_GRAPH_IMPORTS, 32);
    }
}
```

In `shared/src/pyth_graph_manifest.rs`:

```rust
#[test]
fn graph_manifest_round_trips_name_principal_digest_and_package() {
    let package = b"PYTHTIG1fixture";
    let mut output = [0u8; 256];
    let len = encode_named_pyth_graph(&mut output, b"hello.tig", 0x5059_5448_4752_0001, package).unwrap();
    let manifest = validate_named_pyth_graph(&output[..len]).unwrap();
    assert_eq!(manifest.name(), b"hello.tig");
    assert_eq!(manifest.principal_id(), 0x5059_5448_4752_0001);
    assert_eq!(manifest.package(), package);
    assert_eq!(manifest.package_digest(), digest64(package));
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-shared pyth_runtime_abi pyth_graph_manifest
```

- [ ] **Step 3: Implement exact ABI**

Define:

```rust
pub const TYPE_PYTH_GRAPH_PACKAGE: u32 = 0x0000_0004;
pub const PYTH_GRAPH_BOOTSTRAP_MAGIC: u64 = 0x3154_4F4F_4247_5950;
pub const PYTH_GRAPH_RUNTIME_ABI_MAJOR: u16 = 1;
pub const PYTH_GRAPH_RUNTIME_ABI_MINOR: u16 = 0;
pub const MAX_PYTH_GRAPH_IMPORTS: usize = 32;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythGraphCapabilityBinding {
    pub import_slot: u16,
    pub resource_kind: u16,
    pub reserved0: u32,
    pub rights: u64,
    pub capability: PackedCapability,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PythGraphBootstrapBlock {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub import_count: u16,
    pub reserved0: u16,
    pub package_ptr: u64,
    pub package_len: u64,
    pub instruction_budget: u64,
    pub result_ptr: u64,
    pub imports: [PythGraphCapabilityBinding; MAX_PYTH_GRAPH_IMPORTS],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphExitRecord {
    pub status: u16,
    pub error_code: u16,
    pub last_node: u32,
    pub executed_nodes: u64,
    pub result_type: u16,
    pub reserved0: u16,
    pub reserved1: u32,
    pub result_raw: u64,
}
```

Add `RecordType::PythGraphPackage` without changing existing record codes.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test -p pythos-shared pyth_runtime_abi pyth_graph_manifest init_bundle
```

- [ ] **Step 5: Commit**

```powershell
git add shared\src\lib.rs shared\src\pyth_graph_manifest.rs shared\src\pyth_runtime_abi.rs shared\src\init_bundle.rs
git commit -m "feat(pyth-tig): define graph runtime bundle ABI"
```

---

### Task 2: Build the generic ring-3 interpreter ELF

**Files:**
- Modify: `Cargo.toml`
- Create: `user/pyth-runtime/Cargo.toml`
- Create: `user/pyth-runtime/linker.ld`
- Create: `user/pyth-runtime/src/lib.rs`
- Create: `user/pyth-runtime/src/main.rs`
- Create: `user/pyth-runtime/src/value.rs`
- Create: `user/pyth-runtime/src/interpreter.rs`
- Create: `user/pyth-runtime/src/syscalls.rs`
- Create: `scripts/build-pyth-runtime.py`
- Create: `scripts/verify-pyth-runtime-elf.py`

**Interfaces:**
- Consumes: `VerifiedGraph`, runtime bootstrap ABI, existing `SYSCALL_SYSTEM_LOG` user ABI.
- Produces: `pyth-runtime.elf`, `Interpreter::execute`, typed graph exit.

- [ ] **Step 1: Write failing pure-interpreter test**

In `user/pyth-runtime/src/interpreter.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::pyth_tig::test_support;

    struct RecordingHost {
        logs: [[u8; 16]; 4],
        log_count: usize,
    }

    impl Host for RecordingHost {
        fn system_log(&mut self, _capability: PackedCapability, text: &[u8]) -> Result<(), HostError> {
            let mut slot = [0u8; 16];
            slot[..text.len()].copy_from_slice(text);
            self.logs[self.log_count] = slot;
            self.log_count += 1;
            Ok(())
        }
    }

    #[test]
    fn minimal_log_graph_executes_once_and_returns_unit() {
        let bytes = test_support::minimal_log_package();
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut host = RecordingHost { logs: [[0; 16]; 4], log_count: 0 };
        let imports = [PackedCapability::from_parts(7, 1); MAX_PYTH_GRAPH_IMPORTS];
        let exit = Interpreter::new(verified, &imports, 64).execute(&mut host);
        assert_eq!(exit.status, GRAPH_EXIT_OK);
        assert_eq!(exit.executed_nodes, 4);
        assert_eq!(host.log_count, 1);
        assert_eq!(&host.logs[0][..5], b"hello");
    }
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-user-pyth-runtime interpreter::tests::minimal_log_graph_executes_once_and_returns_unit
```

Expected: FAIL because the crate does not exist.

- [ ] **Step 3: Implement bounded interpreter core**

Define:

```rust
pub const MAX_RUNTIME_VALUES: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    Unit,
    Bool(bool),
    U64(u64),
    I64(i64),
    Slice { offset: u32, len: u32, utf8: bool },
    ObjectId(u64),
    RevisionId(u64),
    TaskId(u64),
    ProposalId(u64),
    Capability(PackedCapability),
    Effect(u64),
    ErrorCode(u16),
}

pub trait Host {
    fn system_log(&mut self, capability: PackedCapability, text: &[u8]) -> Result<(), HostError>;
}

pub struct Interpreter<'a> {
    graph: VerifiedGraph<'a>,
    imports: &'a [PackedCapability; MAX_PYTH_GRAPH_IMPORTS],
    values: [Option<Value>; MAX_RUNTIME_VALUES],
    budget: u64,
}
```

Implement only the Phase 2 opcodes. Every node dispatch decrements budget before execution. `SystemLog` resolves the capability import by verified slot and passes a string-table slice to the host.

- [ ] **Step 4: Implement no_std entry and syscall host**

`_start` validates the bootstrap magic, ABI version, import count, package pointer/length, instruction budget, and result pointer. It decodes and verifies the package again in user space, executes it, writes `GraphExitRecord`, then calls a bounded graph-exit syscall or spins after returning.

The runtime does not trust PythCore merely because the kernel verified the package; the duplicate user-space verification is defense in depth and semantic reference behavior.

- [ ] **Step 5: Add linker/build/ELF verification**

Use the existing shell linker pattern with entry `_start` at `0x0000000000500000`. `verify-pyth-runtime-elf.py` requires ET_EXEC, at least one LOAD segment, and no RWE segment.

- [ ] **Step 6: Run GREEN**

```powershell
cargo test -p pythos-user-pyth-runtime
python scripts\build-pyth-runtime.py
python scripts\verify-pyth-runtime-elf.py
```

Expected:

```text
PYTH_RUNTIME_ELF_VERIFY_OK
```

- [ ] **Step 7: Commit**

```powershell
git add Cargo.toml user\pyth-runtime scripts\build-pyth-runtime.py scripts\verify-pyth-runtime-elf.py
git commit -m "feat(pyth-tig): build bounded ring3 interpreter"
```

---

### Task 3: Package, verify, map, and launch graph runtime

**Files:**
- Create: `core/src/pyth_graph_loader.rs`
- Create: `core/src/pyth_runtime_launch.rs`
- Modify: `core/src/main.rs`
- Modify: `core/src/normal_boot.rs`
- Modify: `core/src/runtime_loader.rs`
- Modify: `core/src/user_mode.rs`
- Modify: `scripts/build-image.py`
- Modify: `scripts/build-iso.py`
- Modify: `scripts/build-pyth-graph.py`

**Interfaces:**
- Produces: `load_named_pyth_graph`, `build_pyth_graph_bootstrap`, `launch_pyth_runtime`, control-sector mode `PYTGCTL1/1`.

- [ ] **Step 1: Write failing loader tests**

In `core/src/pyth_graph_loader.rs`:

```rust
#[test]
fn loader_accepts_valid_named_graph_and_rejects_invalid_before_launch() {
    let valid = test_support::bundle_with_named_graph(b"hello.tig", test_support::minimal_log_package());
    let loaded = validate_named_pyth_graph_payload_bytes(&valid, b"hello.tig").unwrap();
    assert_eq!(loaded.manifest.name(), b"hello.tig");
    assert_eq!(loaded.verified.package().header().node_count, 4);

    let invalid = test_support::bundle_with_named_graph(b"bad.tig", test_support::package_with_effect_fork());
    assert_eq!(
        validate_named_pyth_graph_payload_bytes(&invalid, b"bad.tig"),
        Err(PythGraphLoadError::Verify(VerifyError::EffectFork { producer: 0 }))
    );
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-core pyth_graph_loader
```

- [ ] **Step 3: Implement loader and launch contracts**

Implement:

```rust
pub struct LoadedPythGraph<'a> {
    pub manifest: NamedPythGraphManifest<'a>,
    pub verified: VerifiedGraph<'a>,
}

pub fn load_named_pyth_graph(
    boot_info: &PythBootInfo,
    name: &[u8],
) -> Result<LoadedPythGraph<'_>, PythGraphLoadError>;
```

Before mapping, emit one of:

```text
PYTHOS:PYTHTIG:PACKAGE_VALID name:hello.tig
PYTHOS:PYTHTIG:PACKAGE_REJECTED code:<stable-code>
```

Add a test-only boot control sector:

```text
sector 96
magic  PYTGCTL1
mode 1 launch hello.tig through pyth-runtime.elf
mode 0 default object shell
```

Clear the sector after reading so the next boot returns to default.

`build_pyth_graph_bootstrap` maps the package read-only, creates one SystemLog capability binding for import slot 0, sets instruction budget 64, maps a writable `GraphExitRecord`, and maps the bootstrap read-only.

- [ ] **Step 4: Package runtime and graph**

`build-image.py` and `build-iso.py` package:

```text
named user ELF: pyth-runtime.elf with principal 0x5059_5448_5254_0001
named graph: hello.tig with principal 0x5059_5448_4752_0001
```

`scripts/build-pyth-graph.py` invokes `pyth-tig-tool emit-minimal-log` and writes `target/pyth-tig/hello.tig`.

- [ ] **Step 5: Run focused tests**

```powershell
cargo test -p pythos-core pyth_graph_loader pyth_runtime_launch
python scripts\build-pyth-runtime.py
python scripts\build-pyth-graph.py
python scripts\build-image.py
```

Expected: PASS/build success. Do not claim boot success yet.

- [ ] **Step 6: Commit**

```powershell
git add core\src\pyth_graph_loader.rs core\src\pyth_runtime_launch.rs core\src\main.rs core\src\normal_boot.rs core\src\runtime_loader.rs core\src\user_mode.rs scripts\build-image.py scripts\build-iso.py scripts\build-pyth-graph.py
git commit -m "feat(pyth-tig): launch verified graph runtime"
```

---

### Task 4: Add end-to-end runtime acceptance

**Files:**
- Create: `scripts/test-pyth-graph-runtime.py`
- Modify: `.github/workflows/qemu-acceptance.yml`

**Interfaces:**
- Produces: `PYTH_GRAPH_RUNTIME_TEST_OK`.

- [ ] **Step 1: Write acceptance harness**

Create a harness that:

1. Builds runtime, graph, boot, core, and image.
2. Writes `PYTGCTL1` mode 1 to sector 96.
3. Boots QEMU with COM1 log.
4. Requires this ordered sequence:

```text
PYTHOS:PYTHTIG:PACKAGE_VALID name:hello.tig
PYTHOS:PYTHTIG:BOOTSTRAP_BOUND
PYTHOS:PYTHTIG:RUNTIME_ENTER
PYTHOS:PYTHTIG:PROGRAM_LOG hello
PYTHOS:PYTHTIG:RUNTIME_EXIT status:0 executed:4
```

5. Rejects `PYTHOS:PANIC`, `PACKAGE_REJECTED`, or timeout as success.
6. Prints `PYTH_GRAPH_RUNTIME_TEST_OK` only after QEMU exits through a dedicated successful graph-test outcome.

- [ ] **Step 2: Run RED**

```powershell
python scripts\test-pyth-graph-runtime.py
```

Expected: FAIL until kernel and runtime marker/report wiring is complete.

- [ ] **Step 3: Finish exit-report syscall and markers**

Add a typed graph-exit syscall to the shared runtime ABI and `core/src/syscall.rs`. It validates the current caller, copies in exactly one `GraphExitRecord`, emits the runtime-exit marker, terminates only the graph process, and exits QEMU successfully only in graph-test mode. Default normal boot remains alive.

- [ ] **Step 4: Run GREEN and preserved tests**

```powershell
python scripts\test-pyth-graph-runtime.py
python scripts\test-object-shell.py
python scripts\test-boot.py
```

Expected:

```text
PYTH_GRAPH_RUNTIME_TEST_OK
OBJECT_SHELL_TASK8_TEST_OK
OBJECT_SHELL_TASK10_LIFECYCLE_BEFORE_REBOOT_OK
OBJECT_SHELL_TASK11_STRESS_ADVERSARIAL_OK
OBJECT_SHELL_TASK9_REBOOT_TEST_OK
OBJECT_SHELL_TASK10_PERSISTENCE_AFTER_REBOOT_OK
BOOT_TEST_OK
```

- [ ] **Step 5: Add CI step and commit**

Add:

```yaml
- name: Pyth graph runtime acceptance
  run: python scripts/test-pyth-graph-runtime.py
```

Commit:

```powershell
git add scripts\test-pyth-graph-runtime.py core\src\syscall.rs shared\src\pyth_runtime_abi.rs .github\workflows\qemu-acceptance.yml
git commit -m "test(pyth-tig): prove ring3 graph execution"
```

---

### Task 5: Prove rejection, budget, and fault containment

**Files:**
- Modify: `scripts/test-pyth-graph-runtime.py`
- Modify: `tools/pyth-tig-tool/src/mutate.rs`
- Modify: `user/pyth-runtime/src/interpreter.rs`
- Modify: `core/src/pyth_graph_loader.rs`

**Interfaces:**
- Produces: rejection-before-entry, node-budget exhaustion, runtime fault containment evidence.

- [ ] **Step 1: Add negative test cases**

Extend the harness with three separate boots:

```text
invalid package       -> PACKAGE_REJECTED and no RUNTIME_ENTER
loop budget package   -> RUNTIME_EXIT status:budget-exhausted
fault runtime image   -> USER_FAULT, RUNTIME_FAULT_CONTAINED, peer alive
```

- [ ] **Step 2: Run RED**

```powershell
python scripts\test-pyth-graph-runtime.py
```

Expected: FAIL on the new cases.

- [ ] **Step 3: Implement bounded loop and fault outcomes**

Add `GRAPH_EXIT_BUDGET_EXHAUSTED = 2`. The interpreter checks budget before each node. The fault image replaces `pyth-runtime.elf` with a test ELF containing `ud2`, preserving the trusted runtime program name/principal only in that separate test image.

Required markers:

```text
PYTHOS:PYTHTIG:PACKAGE_REJECTED
PYTHOS:PYTHTIG:BUDGET_EXHAUSTED
PYTHOS:CORE:CRASH:USER_FAULT
PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED
PYTHOS:CORE:CRASH:PEER_ALIVE
```

- [ ] **Step 4: Run GREEN**

```powershell
python scripts\test-pyth-graph-runtime.py
cargo test -p pythos-user-pyth-runtime
cargo test -p pythos-core pyth_graph_loader
```

- [ ] **Step 5: Commit**

```powershell
git add scripts\test-pyth-graph-runtime.py tools\pyth-tig-tool\src\mutate.rs user\pyth-runtime\src\interpreter.rs core\src\pyth_graph_loader.rs
git commit -m "test(pyth-tig): prove rejection budget and fault containment"
```

---

## Phase 2 verification

```powershell
cargo fmt --all -- --check
cargo test -p pythos-shared
cargo test -p pythos-user-pyth-runtime
cargo test -p pythos-core pyth_graph_loader pyth_runtime_launch
cargo clippy -p pythos-user-pyth-runtime -p pythos-core --all-targets -- -D warnings
python scripts\test-pyth-tig-format.py
python scripts\test-pyth-graph-runtime.py
python scripts\test-object-shell.py
python scripts\test-boot.py
```

Dispatch the runtime reviewer and security reviewer. Phase 3 may start after both approve.
