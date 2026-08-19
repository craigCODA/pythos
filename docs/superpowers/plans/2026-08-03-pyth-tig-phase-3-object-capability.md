# PythTIG Phase 3 Object and Capability Integration Implementation Plan

**Status:** Complete on `main` as an explicit opt-in proof path. The object
capability flow is exercised by `python scripts\test-pyth-graph-object-flow.py`
and remains outside the default production boot path. This completion does not
authorize Phase 4+ work.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Every host operation begins with a failing unit or acceptance test.

**Goal:** Allow verified graph programs to create, query, inspect, revise, and read history through the existing retained object service while proving capability origin, rights, holder identity, persistence, and denial behavior.

**Architecture:** Object opcodes execute in the ring-3 interpreter and call the existing typed object syscall. Effect nodes write one bounded `HostCallResult`; immediately following `HostResult` nodes extract typed fields. Dynamic object capabilities become graph values only from host results.

**Tech Stack:** Existing object-shell ABI and retained service, PythTIG verifier/runtime, QEMU persistent disk harness.

## Global Constraints

- Do not create a second object store or graph-private persistence format.
- Create/query use workspace capability; inspect/revise/history use per-object capability.
- A workspace capability may not substitute for an object capability.
- Runtime capability handles are not serialized into graph packages or storage snapshots.
- Reboot requires query/rebind before object access.
- Human response formatting remains outside PythCore.

---

### Task 1: Define host-result ABI and object opcode signatures

**Files:**
- Modify: `shared/src/pyth_runtime_abi.rs`
- Modify: `shared/src/pyth_tig/opcode.rs`
- Modify: `shared/src/pyth_tig/verify.rs`
- Modify: `shared/src/pyth_tig/test_support.rs`

**Interfaces:**
- Produces: `HostCallResult`, host-result field codes, verified extraction semantics.

- [ ] **Step 1: Write failing layout and semantic tests**

```rust
#[test]
fn host_call_result_layout_is_stable() {
    assert_eq!(core::mem::size_of::<HostCallResult>(), 112);
    assert_eq!(HOST_RESULT_STATUS, 0);
    assert_eq!(HOST_RESULT_OBJECT_ID, 1);
    assert_eq!(HOST_RESULT_REVISION, 2);
    assert_eq!(HOST_RESULT_CAPABILITY, 3);
    assert_eq!(HOST_RESULT_UTF8, 4);
}

#[test]
fn host_result_must_follow_compatible_host_operation() {
    assert_eq!(
        verify_bytes(&test_support::host_result_without_producer()),
        Err(VerifyError::HostResultInvalid { node: 2 })
    );
    assert_eq!(
        verify_bytes(&test_support::object_create_capability_result_as_u64()),
        Err(VerifyError::TypeMismatch {
            node: 3,
            input: 0,
            expected: PythType::Capability,
            actual: PythType::U64,
        })
    );
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-shared host_call_result host_result_must_follow
```

- [ ] **Step 3: Implement exact host-result contract**

```rust
pub const HOST_RESULT_STATUS: u32 = 0;
pub const HOST_RESULT_OBJECT_ID: u32 = 1;
pub const HOST_RESULT_REVISION: u32 = 2;
pub const HOST_RESULT_CAPABILITY: u32 = 3;
pub const HOST_RESULT_UTF8: u32 = 4;
pub const MAX_HOST_RESULT_BYTES: usize = 64;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostCallResult {
    pub status: u16,
    pub bytes_len: u16,
    pub reserved0: u32,
    pub object_id: u64,
    pub revision: u64,
    pub capability: PackedCapability,
    pub bytes: [u8; MAX_HOST_RESULT_BYTES],
    pub reserved1: [u8; 16],
}
```

The verifier requires each `HostResult` to immediately follow its effectful producer and reference that producer through `input0`. `auxiliary0` selects one field. The field/result-type mapping is exact:

```text
STATUS      -> ErrorCode
OBJECT_ID   -> ObjectId
REVISION    -> RevisionId
CAPABILITY  -> Capability
UTF8        -> Utf8
```

- [ ] **Step 4: Run GREEN**

```powershell
cargo test -p pythos-shared pyth_runtime_abi pyth_tig
```

- [ ] **Step 5: Commit**

```powershell
git add shared\src\pyth_runtime_abi.rs shared\src\pyth_tig\opcode.rs shared\src\pyth_tig\verify.rs shared\src\pyth_tig\test_support.rs
git commit -m "feat(pyth-tig): define typed host call results"
```

---

### Task 2: Add object host operations to the interpreter

**Files:**
- Modify: `user/pyth-runtime/src/interpreter.rs`
- Modify: `user/pyth-runtime/src/value.rs`
- Modify: `user/pyth-runtime/src/syscalls.rs`

**Interfaces:**
- Consumes: existing `ObjectShellRequest`/`ObjectShellResponse` syscall ABI.
- Produces: `Host` object methods and graph `Object*` execution.

- [ ] **Step 1: Write failing interpreter test**

```rust
#[test]
fn object_create_revise_and_inspect_propagate_dynamic_capability() {
    let bytes = test_support::object_note_flow_package();
    let package = PythGraphPackage::decode(&bytes).unwrap();
    let verified = verify_package(&package).unwrap();
    let mut host = RecordingObjectHost::new();
    let imports = test_support::workspace_imports(PackedCapability::from_parts(4, 1));

    let exit = Interpreter::new(verified, &imports, 128).execute(&mut host);

    assert_eq!(exit.status, GRAPH_EXIT_OK);
    assert_eq!(host.create_count, 1);
    assert_eq!(host.revise_count, 1);
    assert_eq!(host.inspect_count, 1);
    assert_eq!(host.last_revise_capability, PackedCapability::from_parts(9, 2));
    assert_eq!(host.last_text(), b"hello");
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-user-pyth-runtime object_create_revise_and_inspect
```

- [ ] **Step 3: Extend `Host` and interpreter dispatch**

Add:

```rust
pub trait Host {
    fn system_log(&mut self, capability: PackedCapability, text: &[u8]) -> Result<(), HostError>;
    fn object_create(&mut self, workspace: PackedCapability, object_kind: u16) -> Result<HostCallResult, HostError>;
    fn object_query(&mut self, workspace: PackedCapability, object_kind: u16) -> Result<HostCallResult, HostError>;
    fn object_inspect(&mut self, object: PackedCapability, object_id: u64) -> Result<HostCallResult, HostError>;
    fn object_revise(&mut self, object: PackedCapability, object_id: u64, field_id: u16, value: &[u8]) -> Result<HostCallResult, HostError>;
    fn object_history(&mut self, object: PackedCapability, object_id: u64) -> Result<HostCallResult, HostError>;
}
```

The interpreter stores the latest result by producer node index in a bounded host-result table. `HostResult` extracts only from its referenced producer. A later operation cannot read an unrelated result.

- [ ] **Step 4: Implement typed syscall adapters**

`syscalls.rs` constructs `ObjectShellRequest`, supplies exact input/output lengths, validates response status, and copies bounded query/inspect output into `HostCallResult.bytes`. It never formats human text.

- [ ] **Step 5: Run GREEN**

```powershell
cargo test -p pythos-user-pyth-runtime
```

- [ ] **Step 6: Commit**

```powershell
git add user\pyth-runtime\src\interpreter.rs user\pyth-runtime\src\value.rs user\pyth-runtime\src\syscalls.rs
git commit -m "feat(pyth-tig): execute typed object host operations"
```

---

### Task 3: Bind graph imports to existing object-service policy

**Files:**
- Modify: `core/src/pyth_runtime_launch.rs`
- Modify: `core/src/object_service.rs`
- Modify: `core/src/syscall.rs`
- Modify: `core/src/process_context.rs`

**Interfaces:**
- Produces: package-import policy binding for system log, workspace, and object capabilities.

- [ ] **Step 1: Write failing policy tests**

```rust
#[test]
fn graph_import_policy_binds_declared_workspace_rights_only() {
    let graph = test_support::verified_object_note_flow();
    let process = test_support::graph_process(0x5059_5448_4752_0002);
    let service = ObjectService::new_for_test();
    let bootstrap = build_pyth_graph_bootstrap_for_test(process, &graph, &service).unwrap();

    assert_eq!(bootstrap.import_count, 2);
    assert_eq!(bootstrap.imports[0].resource_kind, RESOURCE_SYSTEM_LOG);
    assert_eq!(bootstrap.imports[1].resource_kind, RESOURCE_OBJECT_WORKSPACE);
    assert_eq!(bootstrap.imports[1].rights, RIGHTS_CREATE | RIGHTS_QUERY);
}

#[test]
fn undeclared_or_excess_rights_are_not_bound() {
    let graph = test_support::verified_graph_requesting_hardware_resource();
    assert_eq!(
        build_pyth_graph_bootstrap_for_test(test_support::graph_process(7), &graph, &ObjectService::new_for_test()),
        Err(PythRuntimeLaunchError::ImportPolicyDenied { slot: 0 })
    );
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-core graph_import_policy
```

- [ ] **Step 3: Implement policy binding**

PythCore maps import names/resource kinds to policy. Version 1 accepts only:

```text
system.log       RESOURCE_SYSTEM_LOG      WRITE
object.workspace RESOURCE_OBJECT_WORKSPACE CREATE|QUERY
```

Per-object capabilities are never initial imports. They come from object-service results. Unknown resources and excess rights reject launch before `RUNTIME_ENTER`.

- [ ] **Step 4: Preserve caller-derived validation**

The object syscall continues deriving the current process identity. Passing a valid handle owned by another process returns `STATUS_DENIED` and emits:

```text
PYTHOS:PYTHTIG:CAPABILITY_FORGERY_DENIED
```

Only the runtime process currently bound in `process_context` can use its bootstrap imports.

- [ ] **Step 5: Run GREEN**

```powershell
cargo test -p pythos-core pyth_runtime_launch object_service syscall process_context
python scripts\test-object-shell.py
```

- [ ] **Step 6: Commit**

```powershell
git add core\src\pyth_runtime_launch.rs core\src\object_service.rs core\src\syscall.rs core\src\process_context.rs
git commit -m "feat(pyth-tig): bind graph imports to object policy"
```

---

### Task 4: Build object-flow graph fixtures

**Files:**
- Modify: `tools/pyth-tig-tool/src/main.rs`
- Modify: `tools/pyth-tig-tool/src/encode.rs`
- Create: `tools/pyth-tig-tool/src/object_fixtures.rs`
- Modify: `scripts/build-pyth-graph.py`

**Interfaces:**
- Produces: `object-create.tig`, `object-restore.tig`, `object-known-denied.tig`, `object-forgery.tig`.

- [ ] **Step 1: Write fixture verification tests**

```rust
#[test]
fn every_object_fixture_is_canonical_and_verified() {
    for bytes in [
        object_create_package(),
        object_restore_package(),
        object_known_denied_package(),
        object_forgery_package(),
    ] {
        let package = PythGraphPackage::decode(&bytes).unwrap();
        verify_package(&package).unwrap();
    }
}
```

`object-forgery.tig` itself must be structurally valid. It imports a capability slot whose test-only binding is intentionally a copied handle owned by another principal; denial occurs at syscall validation, not verifier validation.

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pyth-tig-tool object_fixture
```

- [ ] **Step 3: Implement fixture commands**

Add CLI:

```text
emit-object-create <path>
emit-object-restore <path>
emit-object-known-denied <path>
emit-object-forgery <path>
```

The create graph performs:

```text
create note -> extract object id/capability -> revise text "hello" -> inspect -> log success -> return
```

The restore graph performs:

```text
query notes -> extract first object id/capability -> inspect -> history -> log success -> return
```

The known-denied graph attempts object id `2001` using the workspace capability and expects denial status.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test -p pyth-tig-tool
python scripts\build-pyth-graph.py --all-object-fixtures
```

- [ ] **Step 5: Commit**

```powershell
git add tools\pyth-tig-tool scripts\build-pyth-graph.py
git commit -m "test(pyth-tig): build object capability fixtures"
```

---

### Task 5: Add persistent object-flow and denial acceptance

**Files:**
- Create: `scripts/test-pyth-graph-object-flow.py`
- Modify: `scripts/build-image.py`
- Modify: `.github/workflows/qemu-acceptance.yml`

**Interfaces:**
- Produces: `PYTH_GRAPH_OBJECT_FLOW_TEST_OK`.

- [ ] **Step 1: Write the end-to-end harness**

The harness uses one persistent storage image and four isolated boots:

1. `object-create.tig`
2. `object-restore.tig`
3. `object-known-denied.tig`
4. `object-forgery.tig` with a test-only copied-handle bootstrap binding

Required evidence:

```text
PYTHOS:PYTHTIG:OBJECT_CREATED object:1042 revision:1
PYTHOS:PYTHTIG:OBJECT_REVISED object:1042 revision:2
PYTHOS:PYTHTIG:OBJECT_INSPECTED object:1042 revision:2
PYTHOS:PYTHTIG:OBJECT_REBOUND object:1042
PYTHOS:PYTHTIG:OBJECT_HISTORY object:1042 revisions:2
PYTHOS:PYTHTIG:OBJECT_KNOWN_DENIED object:2001
PYTHOS:PYTHTIG:CAPABILITY_FORGERY_DENIED
PYTHOS:PYTHTIG:OBJECT_FLOW_ACCEPTANCE_COMPLETE
```

Reject any created object by the forgery principal, any serialized runtime handle, panic, or timeout-as-success.

- [ ] **Step 2: Run RED**

```powershell
python scripts\test-pyth-graph-object-flow.py
```

- [ ] **Step 3: Finish marker and test-image wiring**

`build-image.py --pyth-graph-package <path>` packages exactly one selected test graph under `active.tig`. `normal_boot` loads `active.tig` when control mode 1 is set. The forgery run uses a separate test control mode 2 that binds a copied workspace handle value to the wrong graph principal; production mode never exposes this option.

- [ ] **Step 4: Run GREEN and preserved tests**

```powershell
python scripts\test-pyth-graph-object-flow.py
python scripts\test-persistent-storage.py
python scripts\test-object-shell.py
python scripts\test-boot.py
```

Expected:

```text
PYTH_GRAPH_OBJECT_FLOW_TEST_OK
PERSISTENT_STORAGE_TEST_OK
OBJECT_SHELL_TASK8_TEST_OK
OBJECT_SHELL_TASK10_LIFECYCLE_BEFORE_REBOOT_OK
OBJECT_SHELL_TASK11_STRESS_ADVERSARIAL_OK
OBJECT_SHELL_TASK9_REBOOT_TEST_OK
OBJECT_SHELL_TASK10_PERSISTENCE_AFTER_REBOOT_OK
BOOT_TEST_OK
```

- [ ] **Step 5: Add CI and commit**

```yaml
- name: Pyth graph object and capability acceptance
  run: python scripts/test-pyth-graph-object-flow.py
```

```powershell
git add scripts\test-pyth-graph-object-flow.py scripts\build-image.py core\src\normal_boot.rs .github\workflows\qemu-acceptance.yml
git commit -m "test(pyth-tig): prove object capability flow and restore"
```

---

## Phase 3 verification

```powershell
cargo fmt --all -- --check
cargo test -p pythos-shared
cargo test -p pythos-user-pyth-runtime
cargo test -p pythos-core pyth_runtime_launch object_service syscall
cargo test -p pyth-tig-tool
python scripts\test-pyth-tig-format.py
python scripts\test-pyth-graph-runtime.py
python scripts\test-pyth-graph-object-flow.py
python scripts\test-object-shell.py
python scripts\test-persistent-storage.py
python scripts\test-boot.py
```

Dispatch the security reviewer and runtime reviewer. Do not proceed to Task Steward until wrong-holder denial, dynamic capability origin, and reboot rebind are approved.
