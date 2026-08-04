# PythTIG Phase 7 Cutover and Cross-Target Acceptance Implementation Plan

**Status:** Accepted future phase pending prior PythTIG phase evidence and
explicit owner invocation. Do not implement this plan until the owner explicitly
invokes this phase.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. This phase changes the default normal-boot composition only after all earlier acceptance commands pass in the same worktree.

**Goal:** Make Pyth-native graph services the accepted normal-boot execution layer, retain the Rust object shell as an explicit maintenance fallback, prove reboot recovery, and run one unchanged graph package across every currently accepted storage/hardware target.

**Architecture:** Normal boot launches a Pyth session-manager graph through the interpreter or native backend, then launches Task Steward. The Rust shell is available under `legacy-shell` and as a recovery path. Cross-target acceptance compares the package checksum and semantic evidence, not timing or device-specific logs.

**Tech Stack:** Existing normal boot, Pyth runtime/native backend, task/object services, QEMU virtio/AHCI, physical evidence import, expanding universal-boot backend adapters.

## Global Constraints

- Do not delete the Rust shell or verifier boot path.
- Default cutover requires all Phase 1 through Phase 6 acceptance commands to pass fresh.
- Hardware backends may change discovery and transport only; graph package bytes and semantics remain identical.
- Physical target evidence is target-specific and never generalized beyond the tested controller/machine.
- The cross-target harness accepts no screenshot-only success.

---

### Task 1: Build Pyth session manager and typed command interface

**Files:**
- Create: `shared/src/pyth_command_abi.rs`
- Create: `programs/session-manager/main.pyth`
- Create: `programs/session-manager/README.md`
- Modify: `shared/src/pyth_tig/opcode.rs`
- Modify: `shared/src/pyth_tig/verify.rs`
- Modify: `user/pyth-runtime/src/interpreter.rs`
- Modify: `tools/pythc/src/intrinsics.rs`

**Interfaces:**
- Produces: typed command objects and a Pyth-native session manager; no raw text parsing in PythCore.

- [ ] **Step 1: Define typed command objects**

`PythCommand` kinds:

```text
1 ListObjects
2 InspectObject
3 CreateNote
4 ReviseNote
5 ListTasks
6 CreateTask
7 ListProposals
8 ApproveProposal
9 SuspendTask
10 ReviveTask
11 SystemStatus
12 Reboot
```

Commands are typed objects produced by the existing input/UI adapter or maintenance shell. The session manager reads commands, calls typed object/task/system operations, and writes typed `PythCommandResult` objects.

- [ ] **Step 2: Write failing session-manager compiler/runtime test**

A test host supplies `CreateNote` then `ListTasks`; the graph must produce two result objects in order and must not parse raw command strings.

- [ ] **Step 3: Implement command read/result intrinsics**

Add opcodes in the next PythTIG minor version:

```text
0x1500 CommandRead
0x1501 CommandResultEmit
```

Update ADR 0065 minor to 1 while preserving major 1 and version-0 package acceptance. Version 1.1 runtime accepts 1.0 packages; version 1.0 runtime rejects 1.1 packages.

- [ ] **Step 4: Write session manager**

The program executes one command per invocation and returns. A service supervisor relaunches it for the next command. This preserves bounded execution and avoids a permanently blocked interpreter loop.

- [ ] **Step 5: Compile and verify**

```powershell
cargo run -p pythc -- build programs/session-manager/main.pyth -o target/pyth-tig/session-manager.tig
cargo run -p pyth-tig-tool -- verify target/pyth-tig/session-manager.tig
```

- [ ] **Step 6: Commit**

```powershell
git add shared\src\pyth_command_abi.rs shared\src\pyth_tig user\pyth-runtime tools\pythc programs\session-manager docs\decisions\0065-pyth-graph-package-abi.md
git commit -m "feat(pyth-tig): add native typed session manager"
```

---

### Task 2: Add normal-boot Pyth service supervisor and fallback

**Files:**
- Create: `core/src/pyth_service_supervisor.rs`
- Modify: `core/src/normal_boot.rs`
- Modify: `core/src/main.rs`
- Modify: `Cargo.toml`
- Modify: `scripts/build-image.py`
- Modify: `scripts/build-iso.py`

**Interfaces:**
- Produces: default Pyth service composition and `legacy-shell` fallback feature.

- [ ] **Step 1: Write failing composition tests**

```rust
#[test]
fn default_normal_boot_selects_pyth_services_and_legacy_feature_selects_shell() {
    assert_eq!(normal_program_for_features(false, true), NormalProgram::PythServices);
    assert_eq!(normal_program_for_features(true, false), NormalProgram::LegacyShell);
}

#[test]
fn supervisor_restarts_completed_bounded_service_but_not_fault_loop() {
    let mut supervisor = PythServiceSupervisor::new_for_test();
    supervisor.record_exit(ServiceKind::SessionManager, GraphExitStatus::Ok);
    assert_eq!(supervisor.next_action(), SupervisorAction::RelaunchSessionManager);
    supervisor.record_exit(ServiceKind::SessionManager, GraphExitStatus::Fault);
    assert_eq!(supervisor.next_action(), SupervisorAction::EnterRecoveryShell);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-core pyth_service_supervisor
```

- [ ] **Step 3: Implement feature contract**

Cargo features:

```toml
[features]
default = ["pyth-tig-default"]
pyth-tig-default = []
legacy-shell = []
verify = []
```

`pyth-tig-default` and `legacy-shell` are mutually exclusive at compile time. `verify` preserves current proof behavior and does not launch normal services.

Normal Pyth composition:

```text
restore object/task services
launch session-manager for pending command
launch Task Steward when new context summary is available
return to service supervisor
on bounded success, continue
on user fault, enter legacy recovery shell if built
```

- [ ] **Step 4: Package both paths**

Images contain Pyth runtime, session-manager graph, Task Steward graph, and Rust shell. Boot policy selects one path; fallback does not require rebuilding the image.

- [ ] **Step 5: Run GREEN**

```powershell
cargo test -p pythos-core pyth_service_supervisor normal_boot
python scripts\test-object-shell.py
python scripts\test-pyth-task-steward.py
```

- [ ] **Step 6: Commit**

```powershell
git add core\src\pyth_service_supervisor.rs core\src\normal_boot.rs core\src\main.rs Cargo.toml scripts\build-image.py scripts\build-iso.py
git commit -m "feat(pyth-tig): make Pyth services the normal boot layer"
```

---

### Task 3: Prove default boot, recovery shell, and reboot restore

**Files:**
- Create: `scripts/test-pyth-default-boot.py`
- Modify: `scripts/test-pyth-task-steward.py`
- Modify: `.github/workflows/qemu-acceptance.yml`

**Interfaces:**
- Produces: `PYTH_DEFAULT_BOOT_TEST_OK`.

- [ ] **Step 1: Write default-boot harness**

The harness boots without a graph test control sector and requires:

```text
PYTHOS:CORE:NORMAL_BOOT:FAST_PATH
PYTHOS:PYTHTIG:SESSION_MANAGER_READY
PYTHOS:PYTHTIG:TASK_STEWARD_READY
PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY
```

It submits typed commands to create a note and a task, reboots through the capability-gated system control path, then requires both restored.

- [ ] **Step 2: Add recovery fault case**

A separate test image faults session-manager. Required sequence:

```text
PYTHOS:CORE:CRASH:USER_FAULT
PYTHOS:PYTHTIG:SERVICE_FAULT_CONTAINED
PYTHOS:PYTHTIG:RECOVERY_SHELL_ENTER
PYTHOS:SHELL:READY
```

- [ ] **Step 3: Run RED then implement missing supervisor behavior**

```powershell
python scripts\test-pyth-default-boot.py
```

- [ ] **Step 4: Run GREEN and prior acceptance**

```powershell
python scripts\test-pyth-default-boot.py
python scripts\test-pyth-native-codegen.py
python scripts\test-pyth-task-steward.py
python scripts\test-object-shell.py
python scripts\test-boot.py
```

- [ ] **Step 5: Add CI and commit**

```yaml
- name: Pyth default normal boot acceptance
  run: python scripts/test-pyth-default-boot.py
```

```powershell
git add scripts\test-pyth-default-boot.py scripts\test-pyth-task-steward.py .github\workflows\qemu-acceptance.yml
git commit -m "test(pyth-tig): prove default boot and recovery fallback"
```

---

### Task 4: Build cross-target backend adapter harness

**Files:**
- Create: `scripts/pyth_cross_target.py`
- Create: `scripts/test-pyth-cross-target.py`
- Create: `docs/pyth-tig/CROSS-TARGET-MATRIX.md`
- Modify: `scripts/run-qemu.py`

**Interfaces:**
- Produces: normalized evidence records for virtio, AHCI, SDHCI/eMMC physical logs, and future NVMe adapters.

- [ ] **Step 1: Write failing normalization tests**

```python
def test_normalizes_device_specific_markers_without_hiding_semantic_failures():
    virtio = normalize_log(VIRTIO_LOG, backend="virtio")
    ahci = normalize_log(AHCI_LOG, backend="ahci")
    assert virtio.package_checksum == ahci.package_checksum
    assert virtio.semantic_markers == ahci.semantic_markers
    assert virtio.backend == "virtio"
    assert ahci.backend == "ahci"
```

- [ ] **Step 2: Implement adapter CLI**

```text
python scripts/pyth_cross_target.py qemu --backend virtio --package <tig> --output <json>
python scripts/pyth_cross_target.py qemu --backend ahci --package <tig> --output <json>
python scripts/pyth_cross_target.py physical-log --backend sdhci-emmc --package <tig> --log <serial.txt> --output <json>
python scripts/pyth_cross_target.py physical-log --backend nvme --package <tig> --log <serial.txt> --output <json>
```

Normalized JSON:

```json
{
  "backend": "ahci",
  "target": "qemu-q35",
  "package_checksum": "hex",
  "package_valid": true,
  "runtime_enter": true,
  "runtime_exit_status": 0,
  "semantic_markers": ["PROGRAM_LOG hello", "ACCEPTANCE_COMPLETE"],
  "storage_restore": true,
  "raw_log_sha256": "hex"
}
```

The adapter does not manufacture missing markers. Physical logs must contain serial evidence from the exact package checksum.

- [ ] **Step 3: Implement current target matrix**

`CROSS-TARGET-MATRIX.md` begins with rows:

```text
QEMU virtio-blk      automated
QEMU AHCI            automated
O2 Micro SDHCI/eMMC  physical-log evidence when captured with PythTIG package
NVMe                 pending universal-boot backend acceptance
other Intel/AMD      added only after boot/backend evidence
Apple Intel          added only after boot/backend evidence
Apple silicon        outside x86-64 PythTIG v1
```

"Pending" is a matrix state, not a completion claim.

- [ ] **Step 4: Run current automated targets**

```powershell
python scripts\test-pyth-cross-target.py --automated-only
```

Expected: virtio and AHCI semantic records match and print `PYTH_CROSS_TARGET_TEST_OK`.

- [ ] **Step 5: Commit**

```powershell
git add scripts\pyth_cross_target.py scripts\test-pyth-cross-target.py scripts\run-qemu.py docs\pyth-tig\CROSS-TARGET-MATRIX.md
git commit -m "test(pyth-tig): compare unchanged package across backends"
```

---

### Task 5: Add physical PythTIG evidence capture path

**Files:**
- Create: `scripts/prepare-pyth-physical-image.py`
- Create: `scripts/verify-pyth-physical-log.py`
- Create: `docs/pyth-tig/PHYSICAL-EVIDENCE-PROCEDURE.md`
- Modify: `docs/pyth-tig/CROSS-TARGET-MATRIX.md`

**Interfaces:**
- Produces: reproducible physical image manifest and log verification without claiming untested targets.

- [ ] **Step 1: Prepare exact image manifest**

The script builds one image and writes:

```json
{
  "git_head": "...",
  "package_path": "target/pyth-tig/session-manager.tig",
  "package_checksum": "...",
  "image_sha256": "...",
  "expected_markers": [
    "PYTHOS:PYTHTIG:PACKAGE_VALID",
    "PYTHOS:PYTHTIG:RUNTIME_ENTER",
    "PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY"
  ]
}
```

- [ ] **Step 2: Verify imported physical log**

`verify-pyth-physical-log.py` accepts the manifest and serial log, verifies package checksum marker, marker order, zero drop count when the evidence terminal is used, runtime exit/ready status, and raw log SHA-256. It prints `PYTH_PHYSICAL_LOG_VERIFY_OK` only on complete evidence.

- [ ] **Step 3: Document capture**

The procedure requires exact machine/controller identity, cold-boot count, image hash, package checksum, serial/evidence-terminal pages, and explicit exclusions. Do not generalize one O2 Micro result into generic SDHCI/eMMC support.

- [ ] **Step 4: Commit**

```powershell
git add scripts\prepare-pyth-physical-image.py scripts\verify-pyth-physical-log.py docs\pyth-tig\PHYSICAL-EVIDENCE-PROCEDURE.md docs\pyth-tig\CROSS-TARGET-MATRIX.md
git commit -m "docs(pyth-tig): define physical graph evidence capture"
```

---

### Task 6: Final documentation, claim boundary, and whole-program review

**Files:**
- Modify: `docs/ROADMAP.md`
- Modify: `docs/HANDOVER.md`
- Modify: `docs/THREAT-MODEL.md`
- Create: `docs/pyth-tig/ARCHITECTURE.md`
- Create: `docs/pyth-tig/ACCEPTANCE.md`
- Modify: public-site documentation only after private acceptance passes

**Interfaces:**
- Produces: final internal handover and bounded public claim text.

- [ ] **Step 1: Document accepted architecture**

Record:

```text
Pyth source is compiled by a custom host compiler into canonical typed graphs.
The same verified graph runs through a bounded ring-3 interpreter or a custom
x86-64 backend. PythCore supplies capabilities and typed services; it does not
parse Pyth source or infer task authority. Task Steward emits explainable
proposals and cannot approve them. The Rust shell remains a maintenance fallback.
```

- [ ] **Step 2: Update threat model**

Add threats and mitigations:

```text
malformed package -> shared verifier before mapping
compiler bug -> verifier and golden/differential tests
capability forgery -> origin verification and caller-derived syscall validation
effect reordering -> single effect-token chain
native backend drift -> interpreter/native differential suite
agent overreach -> proposal-only principal policy
graph denial loop -> instruction budget and service supervisor
physical backend variance -> unchanged package checksum and target-specific evidence
```

- [ ] **Step 3: Run complete final verification**

Run every command in the master plan Whole-program final verification section from a clean worktree. Save full output under this plan's SDD workspace.

- [ ] **Step 4: Dispatch final reviewers**

Dispatch:

```text
architecture reviewer
security reviewer
evidence reviewer
final integration reviewer
universal-boot liaison reviewer
```

No implementation agent performs the final approval.

- [ ] **Step 5: Commit documentation only after clean review**

```powershell
git add docs\ROADMAP.md docs\HANDOVER.md docs\THREAT-MODEL.md docs\pyth-tig
git commit -m "docs(pyth-tig): record version 1 acceptance boundary"
```

---

## Phase 7 and program verification

Run the complete master-plan command list. The program is not complete unless every fresh command passes and the final reviewers find no load-bearing issue.
