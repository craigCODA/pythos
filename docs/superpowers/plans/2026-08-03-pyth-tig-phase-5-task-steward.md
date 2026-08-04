# PythTIG Phase 5 Task Steward Implementation Plan

**Status:** Proposed future phase pending owner adoption of ADR 0064 and ADR 0065. Do not implement this plan until Phase 0 is reviewed and the owner explicitly invokes this phase.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Run the agent-authority reviewer after every task that changes task permissions.

**Goal:** Implement the hybrid task model: the user creates or approves authoritative tasks, while a deterministic Pyth graph agent observes permitted context and emits explainable proposals without acquiring task authority.

**Architecture:** Task data is stored through the existing typed-object, revision, relationship, and checkpoint path. A bounded task service enforces state transitions. Task Steward is a Pyth graph program granted read-context and create-proposal capabilities only. The user shell owns approval, suspension, revival, completion, and abandonment authority.

**Tech Stack:** Shared task ABI, retained object service adapter, ring-3 Pyth runtime, `pythc`, existing shell and COM2 acceptance harness.

## Global Constraints

- Task Steward never receives user-task-control capability.
- A proposal is not a task and cannot own tools, objects, or capabilities.
- Semantic score and graph relevance never grant authority.
- Every proposal stores its reasons and source event/object ids.
- Rejection or ignored proposals do not change active task state.
- Task state survives reboot through the existing object checkpoint path.
- Version 1 uses deterministic explicit scoring, not embeddings or an LLM.

---

### Task 1: Define task object kinds, states, relations, and ABI

**Files:**
- Create: `shared/src/task_abi.rs`
- Modify: `shared/src/lib.rs`
- Modify: `core/src/shell_objects.rs`
- Modify: `core/src/typed_object_format.rs`
- Modify: `shared/src/pyth_tig/opcode.rs`
- Modify: `shared/src/pyth_tig/verify.rs`

**Interfaces:**
- Produces: stable task kinds/codes, task request/response ABI, task host-op signatures.

- [ ] **Step 1: Write failing ABI tests**

```rust
#[test]
fn task_codes_and_layouts_are_stable() {
    assert_eq!(OBJECT_KIND_TASK, 20);
    assert_eq!(OBJECT_KIND_TASK_PROPOSAL, 21);
    assert_eq!(OBJECT_KIND_TASK_EVENT, 22);
    assert_eq!(OBJECT_KIND_TASK_RELATION, 23);
    assert_eq!(OBJECT_KIND_RELEVANCE_ASSERTION, 24);
    assert_eq!(OBJECT_KIND_CAPABILITY_REQUEST, 25);
    assert_eq!(TaskStatus::Active.code(), 1);
    assert_eq!(TaskStatus::Suspended.code(), 2);
    assert_eq!(TaskProposalKind::NewTask.code(), 1);
    assert_eq!(TaskProposalKind::Continuation.code(), 2);
    assert_eq!(TaskProposalKind::Child.code(), 3);
    assert_eq!(TaskProposalKind::Branch.code(), 4);
    assert_eq!(TaskProposalKind::Related.code(), 5);
    assert_eq!(core::mem::size_of::<TaskRequest>(), 96);
    assert_eq!(core::mem::size_of::<TaskResponse>(), 64);
    assert_eq!(core::mem::size_of::<TaskContextSummary>(), 80);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-shared task_abi
```

- [ ] **Step 3: Implement exact task ABI**

Define task status:

```text
1 Active
2 Suspended
3 Completed
4 Abandoned
```

Define proposal kinds:

```text
1 NewTask
2 Continuation
3 Child
4 Branch
5 Related
```

Define task operations:

```text
1 CreateTask
2 ReadActiveTask
3 AppendTaskEvent
4 CreateProposal
5 ListProposals
6 ApproveProposal
7 RejectProposal
8 SuspendTask
9 ReviveTask
10 CompleteTask
11 AbandonTask
12 ReadContextSummary
```

Define rights:

```text
TASK_RIGHT_READ_CONTEXT
TASK_RIGHT_APPEND_EVENT
TASK_RIGHT_CREATE_PROPOSAL
TASK_RIGHT_APPROVE_PROPOSAL
TASK_RIGHT_CONTROL_STATE
```

`TaskContextSummary` fields:

```rust
#[repr(C)]
pub struct TaskContextSummary {
    pub active_task_id: u64,
    pub matching_suspended_task_id: u64,
    pub dominant_object_kind: u16,
    pub dominant_tool_domain: u16,
    pub proposal_kind: u16,
    pub event_count: u16,
    pub active_match_count: u16,
    pub candidate_match_count: u16,
    pub tool_domain_changed: u16,
    pub reserved0: u16,
    pub confidence_score: u64,
    pub candidate_tag_hash: u64,
    pub source_event_ids: [u64; 4],
}
```

Add `TaskContextRead` opcode `0x1205` to ADR 0065, the design spec, and opcode signatures. It consumes `[Effect, Capability]` and yields host results: active task id, candidate task id, confidence U64, proposal kind U64, and reason Utf8.

- [ ] **Step 4: Add typed object kinds**

Extend `ObjectKind` encode/decode with codes 20 through 25. Do not change existing codes.

- [ ] **Step 5: Run GREEN**

```powershell
cargo test -p pythos-shared task_abi pyth_tig
cargo test -p pythos-core typed_object_format shell_objects
```

- [ ] **Step 6: Commit**

```powershell
git add shared\src\task_abi.rs shared\src\lib.rs shared\src\pyth_tig core\src\shell_objects.rs core\src\typed_object_format.rs docs\decisions\0065-pyth-graph-package-abi.md docs\superpowers\specs\2026-08-03-pyth-typed-instruction-graph-design.md
git commit -m "feat(tasks): define hybrid task and proposal ABI"
```

---

### Task 2: Implement authoritative task service on typed objects

**Files:**
- Create: `core/src/task_service.rs`
- Modify: `core/src/object_service.rs`
- Modify: `core/src/retained_services.rs`
- Modify: `core/src/syscall.rs`
- Modify: `core/src/main.rs`

**Interfaces:**
- Produces: `TaskService`, typed task syscall, authoritative state-transition validation.

- [ ] **Step 1: Write failing state-machine tests**

```rust
#[test]
fn proposal_does_not_change_active_task_until_user_approval() {
    let mut service = TaskService::new_for_test();
    let user = service.user_caller();
    let steward = service.steward_caller();
    let user_control = service.user_task_control_capability();
    let steward_propose = service.steward_proposal_capability();

    let task_a = service.create_task(user, user_control, b"Universal Boot").unwrap();
    let proposal = service.create_proposal(
        steward,
        steward_propose,
        TaskProposalKind::NewTask,
        task_a.task_id,
        0,
        85,
        b"Semantic Task Runtime",
        b"recent context diverged",
    ).unwrap();

    assert_eq!(service.active_task_id(), Some(task_a.task_id));
    assert!(!service.task_exists_for_title(b"Semantic Task Runtime"));

    let task_b = service.approve_proposal(user, user_control, proposal.proposal_id, true).unwrap();
    assert_eq!(service.task_status(task_a.task_id), TaskStatus::Suspended);
    assert_eq!(service.task_status(task_b.task_id), TaskStatus::Active);
}

#[test]
fn steward_cannot_create_approve_or_change_task_state() {
    let mut service = TaskService::new_for_test();
    let steward = service.steward_caller();
    let proposal_cap = service.steward_proposal_capability();

    assert_eq!(service.create_task(steward, proposal_cap, b"forged"), Err(TaskServiceError::Denied));
    assert_eq!(service.approve_proposal(steward, proposal_cap, 1, true), Err(TaskServiceError::Denied));
    assert_eq!(service.suspend_task(steward, proposal_cap, 1), Err(TaskServiceError::Denied));
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-core task_service
```

- [ ] **Step 3: Implement task service**

`TaskService` is an adapter over the retained `ObjectService`; it does not own a separate persistence backend.

```rust
pub struct TaskService<'a> {
    objects: &'a mut ObjectService,
    capabilities: CapabilityTable,
    active_task_id: Option<u64>,
}
```

Every operation creates/revises typed records and appends a `TaskEvent`. `ApproveProposal` atomically:

1. validates user approval capability;
2. verifies proposal is pending;
3. creates or resumes the target task according to proposal kind;
4. optionally suspends the current task;
5. records TaskRelation;
6. marks proposal approved;
7. commits one object-service checkpoint.

If any step fails before commit marker, recovery restores the previous consistent state.

- [ ] **Step 4: Add typed task syscall**

Add `SYSCALL_TASK_REQUEST`. It uses the same caller-derived copy-in/copy-out rules as object requests. Task Steward calls may create proposals or read context only. User shell calls may control state only with user authority.

- [ ] **Step 5: Persist and restore active task identity**

Active task identity is a stable Task object relationship from the workspace/session object, not a persisted runtime capability. After reboot, fresh task-control handles are minted from user principal policy.

- [ ] **Step 6: Run GREEN and storage tests**

```powershell
cargo test -p pythos-core task_service object_service syscall retained_services
python scripts\test-persistent-storage.py
```

- [ ] **Step 7: Commit**

```powershell
git add core\src\task_service.rs core\src\object_service.rs core\src\retained_services.rs core\src\syscall.rs core\src\main.rs
git commit -m "feat(tasks): enforce authoritative hybrid task state"
```

---

### Task 3: Implement deterministic context summary and explainable relevance

**Files:**
- Create: `core/src/task_context.rs`
- Modify: `core/src/task_service.rs`
- Modify: `core/src/object_service.rs`
- Test: `core/src/task_context.rs`

**Interfaces:**
- Produces: `summarize_context`, explicit score components, `RelevanceAssertion` objects.

- [ ] **Step 1: Write failing scoring tests**

```rust
#[test]
fn score_crosses_threshold_only_for_sustained_context_change() {
    let active = TaskFingerprint::new(OBJECT_KIND_NOTE, TOOL_DOMAIN_STORAGE, tag("universal-boot"));
    let short_shift = [
        event(OBJECT_KIND_TASK, TOOL_DOMAIN_GRAPH, tag("semantic")),
        event(OBJECT_KIND_TASK, TOOL_DOMAIN_GRAPH, tag("semantic")),
    ];
    assert!(summarize_context(active, &short_shift, None).confidence_score < 70);

    let sustained = [
        event(OBJECT_KIND_TASK, TOOL_DOMAIN_GRAPH, tag("semantic")),
        event(OBJECT_KIND_TASK, TOOL_DOMAIN_GRAPH, tag("semantic")),
        event(OBJECT_KIND_TASK, TOOL_DOMAIN_GRAPH, tag("semantic")),
        event(OBJECT_KIND_TASK, TOOL_DOMAIN_GRAPH, tag("semantic")),
        event(OBJECT_KIND_TASK, TOOL_DOMAIN_GRAPH, tag("semantic")),
        event(OBJECT_KIND_TASK, TOOL_DOMAIN_GRAPH, tag("semantic")),
        event(OBJECT_KIND_TASK, TOOL_DOMAIN_GRAPH, tag("semantic")),
        event(OBJECT_KIND_TASK, TOOL_DOMAIN_GRAPH, tag("semantic")),
    ];
    let summary = summarize_context(active, &sustained, None);
    assert_eq!(summary.confidence_score, 85);
    assert_eq!(summary.proposal_kind, TaskProposalKind::NewTask);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-core task_context
```

- [ ] **Step 3: Implement exact scoring**

Use the eight most recent permitted TaskEvents:

```text
+40 when at least 5 of 8 events share a candidate tag different from active task tag
+25 when at least 5 of 8 events use a different dominant tool domain
+20 when at least 5 of 8 events use a different dominant object kind
+15 when a suspended task matches the candidate tag
```

Score is capped at 100. Proposal threshold is 70. Proposal kind:

```text
matching suspended task exists -> Continuation
candidate events link to parent objective -> Child
candidate events mark alternative method -> Branch
candidate shares objects but not objective -> Related
otherwise -> NewTask
```

The summary includes four source event ids. `TaskService::read_context_summary` writes a `RelevanceAssertion` only when Task Steward requests it and has read-context capability.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test -p pythos-core task_context task_service
```

- [ ] **Step 5: Commit**

```powershell
git add core\src\task_context.rs core\src\task_service.rs core\src\object_service.rs
git commit -m "feat(tasks): summarize context with explainable scores"
```

---

### Task 4: Add task host operations to Pyth runtime and compiler

**Files:**
- Modify: `user/pyth-runtime/src/interpreter.rs`
- Modify: `user/pyth-runtime/src/syscalls.rs`
- Modify: `tools/pythc/src/intrinsics.rs`
- Modify: `tools/pythc/src/typecheck.rs`
- Modify: `tools/pythc/src/lower.rs`
- Modify: `shared/src/pyth_tig/verify.rs`

**Interfaces:**
- Produces: `task.context`, `task.propose`, and proposal-only runtime behavior.

- [ ] **Step 1: Write failing compiler/runtime test**

```rust
#[test]
fn steward_program_reads_context_and_emits_proposal() {
    let source = include_str!("../../../programs/task-steward/main.pyth");
    let bytes = compile_source(source).unwrap();
    let verified = verify_bytes(&bytes).unwrap();
    let mut host = RecordingTaskHost::with_summary(summary(score: 85, candidate: 42));
    let exit = Interpreter::new(verified, &host.imports(), 256).execute(&mut host);
    assert_eq!(exit.status, GRAPH_EXIT_OK);
    assert_eq!(host.proposals.len(), 1);
    assert_eq!(host.proposals[0].candidate_task_id, 42);
    assert_eq!(host.proposals[0].score, 85);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythc steward_program
cargo test -p pythos-user-pyth-runtime steward_program
```

- [ ] **Step 3: Implement task host operations**

`task.context(read_cap)` returns active/candidate task ids, score, kind, and reason through context-sensitive HostResult fields. `task.propose(proposal_cap, kind, candidate_task, score, reason)` calls `SYSCALL_TASK_REQUEST` operation `CreateProposal`.

The verifier requires TaskProposalEmit imports to have `TASK_RIGHT_CREATE_PROPOSAL`; it never accepts `TASK_RIGHT_APPROVE_PROPOSAL` or `TASK_RIGHT_CONTROL_STATE` for the Task Steward manifest policy.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test -p pythc
cargo test -p pythos-user-pyth-runtime
cargo test -p pythos-shared pyth_tig
```

- [ ] **Step 5: Commit**

```powershell
git add user\pyth-runtime tools\pythc shared\src\pyth_tig
git commit -m "feat(tasks): expose proposal-only task intrinsics"
```

---

### Task 5: Write and compile Task Steward

**Files:**
- Create: `programs/task-steward/main.pyth`
- Create: `programs/task-steward/README.md`
- Modify: `scripts/build-pyth-graph.py`
- Modify: `scripts/build-image.py`
- Modify: `core/src/pyth_runtime_launch.rs`

**Interfaces:**
- Produces: `target/pyth-tig/task-steward.tig`, principal and import policy.

- [ ] **Step 1: Write the Pyth program**

```text
program task_steward principal 0x5059544853540001 {
    import log: capability<system.log, write>;
    import context: capability<task.context, read>;
    import proposals: capability<task.proposal, create>;

    fn main() -> unit {
        let score: u64 = task.context_score(context);
        if score < 70 {
            system.log(log, "task-context-stable");
            return;
        } else {
            let candidate: task_id = task.context_candidate(context);
            let kind: u64 = task.context_kind(context);
            let reason: utf8 = task.context_reason(context);
            task.propose(proposals, kind, candidate, score, reason);
            system.log(log, "task-proposal-created");
            return;
        }
    }
}
```

Add the context accessor intrinsics as immediate HostResult accessors tied to the preceding `task.context` call.

- [ ] **Step 2: Compile and verify**

```powershell
cargo run -p pythc -- check programs/task-steward/main.pyth
cargo run -p pythc -- build programs/task-steward/main.pyth -o target/pyth-tig/task-steward.tig
cargo run -p pyth-tig-tool -- verify target/pyth-tig/task-steward.tig
```

Expected: `PYTH_TIG_VERIFY_OK`.

- [ ] **Step 3: Bind exact imports**

PythCore policy for principal `0x5059544853540001` binds only:

```text
system.log write
task.context read
task.proposal create
```

A package requesting task approval or state-control rights is rejected before launch.

- [ ] **Step 4: Commit**

```powershell
git add programs\task-steward scripts\build-pyth-graph.py scripts\build-image.py core\src\pyth_runtime_launch.rs
git commit -m "feat(tasks): add deterministic Task Steward graph program"
```

---

### Task 6: Extend shell for explicit user task authority

**Files:**
- Modify: `user/shell/src/commands.rs`
- Modify: `user/shell/src/syscalls.rs`
- Modify: `user/shell/src/main.rs`
- Modify: `shared/src/object_shell_abi.rs`
- Modify: `core/src/normal_boot.rs`

**Interfaces:**
- Produces: task/proposal commands and user-only authority flow.

- [ ] **Step 1: Write failing parser tests**

```rust
#[test]
fn parses_task_and_proposal_commands() {
    assert!(matches!(parse_command(br#"task new "Universal Boot""#).unwrap(), Command::TaskNew { .. }));
    assert!(matches!(parse_command(b"proposal list").unwrap(), Command::ProposalList));
    assert!(matches!(parse_command(b"proposal approve 3001 suspend-current").unwrap(), Command::ProposalApprove { proposal_id: 3001, suspend_current: true }));
    assert!(matches!(parse_command(b"task suspend 2001").unwrap(), Command::TaskSuspend { task_id: 2001 }));
    assert!(matches!(parse_command(b"task revive 2001").unwrap(), Command::TaskRevive { task_id: 2001 }));
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-user-shell commands::tests::parses_task_and_proposal_commands
```

- [ ] **Step 3: Implement shell commands**

Supported commands:

```text
task new "<title>"
task active
task list
task event tag:<hex> tool:<u16> object-kind:<u16>
task suspend <task-id>
task revive <task-id>
task complete <task-id>
task abandon <task-id>
proposal list
proposal approve <proposal-id> keep-current
proposal approve <proposal-id> suspend-current
proposal reject <proposal-id>
```

The shell bootstrap receives user task-control capability. Task Steward never receives it.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test -p pythos-user-shell
python scripts\test-object-shell.py
```

- [ ] **Step 5: Commit**

```powershell
git add user\shell shared\src\object_shell_abi.rs core\src\normal_boot.rs
git commit -m "feat(tasks): add explicit user task controls"
```

---

### Task 7: Prove hybrid task, proposal, denial, and reboot behavior

**Files:**
- Create: `scripts/test-pyth-task-steward.py`
- Modify: `.github/workflows/qemu-acceptance.yml`

**Interfaces:**
- Produces: `PYTH_TASK_STEWARD_TEST_OK`.

- [ ] **Step 1: Write acceptance harness**

The harness:

1. Boots the object shell with persistent storage.
2. Creates Task A `Universal Boot`.
3. Appends two semantic events and proves no proposal.
4. Appends eight sustained semantic/graph events.
5. Launches Task Steward graph.
6. Requires one proposal with score 85 and four source-event ids.
7. Proves active task is still Task A.
8. Runs a test-only Steward attempt to call `CreateTask` and requires denial.
9. User approves proposal with `suspend-current`.
10. Requires Task A suspended and Task B active.
11. Reboots.
12. Requires tasks, proposal decision, relevance assertion, and active task restored.
13. User revives Task A and requires Task B remains available.

Required markers:

```text
PYTHOS:PYTHTIG:TASK_CONTEXT_STABLE
PYTHOS:PYTHTIG:TASK_DIVERGENCE_DETECTED score:85
PYTHOS:PYTHTIG:TASK_PROPOSAL_CREATED
PYTHOS:PYTHTIG:TASK_AUTHORITY_UNCHANGED
PYTHOS:PYTHTIG:TASK_DIRECT_CREATE_DENIED
PYTHOS:PYTHTIG:TASK_PROPOSAL_APPROVED
PYTHOS:PYTHTIG:TASK_SUSPENDED
PYTHOS:PYTHTIG:TASK_CREATED
PYTHOS:PYTHTIG:TASK_STATE_RESTORED
PYTHOS:PYTHTIG:TASK_REVIVED
PYTHOS:PYTHTIG:TASK_STEWARD_ACCEPTANCE_COMPLETE
```

- [ ] **Step 2: Run RED**

```powershell
python scripts\test-pyth-task-steward.py
```

- [ ] **Step 3: Finish missing runtime/service markers only**

Do not weaken assertions. Add bounded markers at the state transition points after authority checks and committed persistence.

- [ ] **Step 4: Run GREEN and preserved suites**

```powershell
python scripts\test-pyth-task-steward.py
python scripts\test-pyth-graph-object-flow.py
python scripts\test-object-shell.py
python scripts\test-persistent-storage.py
python scripts\test-boot.py
```

Expected all success lines.

- [ ] **Step 5: Add CI and commit**

```yaml
- name: Pyth Task Steward acceptance
  run: python scripts/test-pyth-task-steward.py
```

```powershell
git add scripts\test-pyth-task-steward.py .github\workflows\qemu-acceptance.yml
git commit -m "test(tasks): prove hybrid Task Steward authority"
```

---

## Phase 5 verification

```powershell
cargo fmt --all -- --check
cargo test -p pythos-shared
cargo test -p pythos-core task_service task_context
cargo test -p pythos-user-pyth-runtime
cargo test -p pythos-user-shell
cargo test -p pythc
python scripts\test-pyth-task-steward.py
python scripts\test-pyth-graph-object-flow.py
python scripts\test-object-shell.py
python scripts\test-persistent-storage.py
python scripts\test-boot.py
```

Dispatch `prompts/task-steward-reviewer.md` and `prompts/security-reviewer.md`. Any path that allows Task Steward to mutate authoritative task state blocks the phase.
