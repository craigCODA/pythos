# Pyth Native Typed Instruction Graph Program Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development for each phase plan. Use superpowers:using-git-worktrees before implementation, superpowers:test-driven-development for every behavior change, and superpowers:verification-before-completion before every completion claim. Steps use checkbox (`- [ ]`) syntax for tracking.

**Status:** Accepted execution-model program. Phases 0, 1, 2, and 3 are
complete on `main` as explicit opt-in proof paths; later phases remain pending
explicit owner invocation. ADR 0064 is accepted as the architecture direction.
ADR 0065 is accepted and the tested PythTIG version 1 package ABI is frozen.

**Goal:** Deliver PythTIG version 1 from canonical graph package through shared verification, ring-3 execution, typed object capability use, custom source compilation, deterministic Task Steward behavior, native x86-64 lowering, reboot recovery, and cross-target evidence.

**Architecture:** Keep PythCore as the privileged capability and hardware substrate. Add a canonical typed instruction graph ABI and shared verifier, execute verified packages in a bounded ring-3 runtime, then compile the same graph to native x86-64 while preserving interpreter semantics. Build task semantics and the Task Steward on top of the existing typed-object and capability services.

**Tech Stack:** Rust `no_std` shared/core/user crates, Rust `std` host tools, existing x86-64 user ELF loader, QEMU q35/OVMF, COM1 evidence oracle, COM2 shell, Python acceptance harnesses, existing typed object and capability ABIs.

## Global Constraints

- Read `docs/superpowers/specs/2026-08-03-pyth-typed-instruction-graph-design.md` before editing.
- Read `docs/PythOS-SAS-001.md`, `docs/PythOS-TDD-001.md`, `docs/HANDOVER.md`, `docs/ROADMAP.md`, ADR 0051, ADR 0052, and the current object-shell implementation before editing.
- Preserve all existing `verify` behavior and acceptance markers.
- No production code without a failing test first.
- No graph value may represent a raw pointer.
- No graph operation may directly access hardware.
- PythCore receives typed graph packages and typed syscalls only; it never parses Pyth source.
- Human syntax belongs in host tools or ring-3 programs.
- Semantic relevance never grants authority.
- Capability values originate only from validated imports or capability-returning host operations.
- Effectful nodes must participate in one validated effect-token chain.
- Invalid graphs fail before ring-3 entry.
- Existing Rust shell/runtime remains available until cutover acceptance passes.
- COM1 serial output is the acceptance oracle; screenshots are supporting evidence only.
- Every unsafe block must document address, length, lifetime, ownership, alignment, concurrency, and violation behavior.
- No phase may claim success from timeout, build success alone, or subagent report alone.

---

## Program decomposition

This program is intentionally split into independently reviewable phase plans. Do not merge phases into one giant implementation session.

| Phase | Plan file | Deliverable | Depends on |
|---|---|---|---|
| 0 | `2026-08-03-pyth-tig-phase-0-foundation.md` | ADRs, AGENTS rules, baseline reconciliation, evidence namespace | Current accepted tree |
| 1 | `2026-08-03-pyth-tig-phase-1-format-verifier.md` | Canonical package ABI and shared verifier | Phase 0 |
| 2 | `2026-08-03-pyth-tig-phase-2-ring3-runtime.md` | Generic ring-3 interpreter executing `SystemLog` | Phase 1 |
| 3 | `2026-08-03-pyth-tig-phase-3-object-capability.md` | Object operations and capability-forgery denial | Phase 2 |
| 4 | `2026-08-03-pyth-tig-phase-4-pythc.md` | Custom Pyth source compiler and golden packages | Phase 1; integrates with 2/3 |
| 5 | `2026-08-03-pyth-tig-phase-5-task-steward.md` | Task objects, hybrid proposal flow, deterministic runtime agent | Phases 3 and 4 |
| 6 | `2026-08-03-pyth-tig-phase-6-native-codegen.md` | x86-64 lowering and interpreter/native differential proof | Phases 3 and 4 |
| 7 | `2026-08-03-pyth-tig-phase-7-cutover-cross-target.md` | Normal-boot cutover gate, reboot restore, hardware-independent acceptance | Phases 5 and 6 |

## Merge order

```text
Phase 0
  |
  v
Phase 1
  |
  +-> Phase 2 -> Phase 3 -+
  +-> Phase 4 ------------+-> Phase 5
                           +-> Phase 6
Phase 5 + Phase 6 -> Phase 7
```

Phase 4 may begin after Phase 1 while Phase 2 is being implemented in another isolated worktree, but it must not merge until its package output passes the exact Phase 1 shared verifier. Phase 2 and Phase 3 edit the runtime/loader path and must merge sequentially.

## Controller execution rules

- [ ] Create one isolated worktree per phase using `superpowers:using-git-worktrees`.
- [ ] Create the phase-specific SDD ledger with `scripts/sdd-workspace`.
- [ ] Read one phase plan once and create one todo per numbered task.
- [ ] Run the phase preflight before dispatching an implementer.
- [ ] Dispatch one implementation subagent at a time within a worktree.
- [ ] Run task review after every task.
- [ ] Run specialized gate review at each phase boundary.
- [ ] Run the phase's complete verification matrix before merge.
- [ ] Merge only in the dependency order above.
- [ ] After all phases, dispatch one most-capable whole-program reviewer.

## Safe parallelism

Allowed parallel work:

- Phase 4 compiler work and Phase 2 runtime work after Phase 1 is merged, in separate worktrees.
- Read-only architecture, threat-model, evidence, and documentation reviews.
- Independent fuzz-corpus generation that does not edit shared code.
- Universal-boot backend work that does not edit PythTIG ABI, verifier, runtime, or package semantics.

Forbidden parallel work:

- Two implementation agents editing `shared/src/pyth_tig`.
- Runtime and object-bridge agents editing the same syscall/loader files.
- Compiler and verifier agents changing opcode/type semantics independently.
- Two agents updating the same AGENTS.md or ADR.
- Any automatic merge of subagent branches without full verification.

## Required specialized gates

### Gate A: ABI and canonicalization

After Phase 1, the ABI reviewer verifies exact sizes, offsets, endian behavior, canonical encoding, unknown-version rejection, and compatibility tests.

### Gate B: Runtime authority

After Phase 3, the security reviewer verifies that malformed graphs fail before launch, capability constants are impossible, wrong-holder handles are denied, effect order is enforced, and user faults remain contained.

### Gate C: Compiler semantic equivalence

After Phase 4, the compiler reviewer verifies that source typing, graph typing, and shared verifier semantics are identical and that no compiler-only bypass exists.

### Gate D: Agent authority

After Phase 5, the task-agent reviewer proves that Task Steward can emit proposals but cannot create, approve, suspend, revive, merge, complete, or abandon a task.

### Gate E: Native differential proof

After Phase 6, the runtime reviewer compares interpreter and native results for success, denial, object revision, loop-budget exhaustion, and fault cases.

### Gate F: Final evidence and claim boundary

After Phase 7, the evidence reviewer maps every public claim to a command, marker sequence, artifact, supported target, and explicit exclusion.

## Whole-program final verification

Run from a clean worktree after all merges:

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python scripts\test-boot.py
python scripts\test-persistent-storage.py
python scripts\test-normal-fast-boot.py
python scripts\test-object-shell.py
python scripts\test-pyth-tig-format.py
python scripts\test-pyth-graph-runtime.py
python scripts\test-pyth-graph-object-flow.py
python scripts\test-pythc.py
python scripts\build-pyth-graph.py
python scripts\test-pyth-native-codegen.py
python scripts\test-pyth-default-boot.py
python scripts\test-pyth-cross-target.py --automated-only
python scripts\verify-pyth-physical-log.py --self-test
```

Expected terminal lines:

```text
BOOT_TEST_OK
PERSISTENT_STORAGE_TEST_OK
NORMAL_FAST_BOOT_TEST_OK
OBJECT_SHELL_TASK8_TEST_OK
OBJECT_SHELL_TASK10_LIFECYCLE_BEFORE_REBOOT_OK
OBJECT_SHELL_TASK11_STRESS_ADVERSARIAL_OK
OBJECT_SHELL_TASK9_REBOOT_TEST_OK
OBJECT_SHELL_TASK10_PERSISTENCE_AFTER_REBOOT_OK
PYTH_TIG_FORMAT_TEST_OK
PYTH_GRAPH_RUNTIME_TEST_OK
PYTH_GRAPH_OBJECT_FLOW_TEST_OK
PYTHC_TEST_OK
PYTH_GRAPH_TASK_STEWARD_READY
PYTH_GRAPH_SESSION_MANAGER_READY
PYTH_NATIVE_CODEGEN_TEST_OK
PYTH_DEFAULT_BOOT_TEST_OK
PYTH_CROSS_TARGET_TEST_OK
PYTH_PHYSICAL_LOG_SELF_TEST_OK
```

## Program completion boundary

Completion means the commands above ran fresh with zero failures and the final reviewer found no load-bearing gaps. It does not mean self-hosting, broad hardware support, networking, package management, or an LLM-driven runtime agent.
