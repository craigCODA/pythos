# PythTIG Phase 0 Reconciliation Report

Date: 2026-08-04

## Status

This is the historical Phase 0 reconciliation record. PythTIG Phase 0 is
merged, owner review accepted ADR 0064 as the architecture direction, and ADR
0065 is now accepted with the tested PythTIG version 1 package ABI frozen as of
2026-08-08. Subsequent work on `main` implemented PythTIG Phase 1 through Phase
7, including the host compiler, Task Steward, native backend, and
cutover/cross-target acceptance line. Later PythTIG phases remain gated and
require explicit owner re-invocation.

## Worktree

- Original review worktree:
  `C:\Users\NeverAMoment\pythos\.worktrees\pythtig-phase0-from-physical-evidence`
- Original review branch: `docs/pythtig-phase0-from-physical-evidence`
- Original base branch: `agent/physical-evidence-terminal`
- Original base HEAD: `809f8bd docs: point evidence terminal record at hardened commit`
- Merge target: `main`, after `agent/physical-evidence-terminal` was merged
  and validated.

## Imported Documents

- `docs/superpowers/specs/2026-08-03-pyth-typed-instruction-graph-design.md`
- `docs/superpowers/plans/2026-08-03-pyth-typed-instruction-graph-master-plan.md`
- `docs/superpowers/plans/2026-08-03-pyth-tig-phase-0-foundation.md`
- `docs/superpowers/plans/2026-08-03-pyth-tig-phase-1-format-verifier.md`
- `docs/superpowers/plans/2026-08-03-pyth-tig-phase-2-ring3-runtime.md`
- `docs/superpowers/plans/2026-08-03-pyth-tig-phase-3-object-capability.md`
- `docs/superpowers/plans/2026-08-03-pyth-tig-phase-4-pythc.md`
- `docs/superpowers/plans/2026-08-03-pyth-tig-phase-5-task-steward.md`
- `docs/superpowers/plans/2026-08-03-pyth-tig-phase-6-native-codegen.md`
- `docs/superpowers/plans/2026-08-03-pyth-tig-phase-7-cutover-cross-target.md`
- `docs/pyth-tig/README.md`
- `docs/pyth-tig/acceptance/marker-contract.md`
- `docs/pyth-tig/acceptance/test-matrix.md`
- `docs/pyth-tig/acceptance/definition-of-done.md`
- `docs/pyth-tig/PHASE-0-RECONCILIATION-REPORT.md`

## AGENTS Rules

- Root `AGENTS.md` records PythTIG adoption guardrails.
- `shared/src/pyth_tig/AGENTS.md`
- `core/src/pyth_tig/AGENTS.md`
- `user/pyth-runtime/AGENTS.md`
- `user/agents/task-steward/AGENTS.md`
- `tools/pythc/AGENTS.md`
- `scripts/AGENTS.md`

These scoped files reserve future rules only. They do not create source
modules, require directories to exist yet, or authorize implementation before
explicit phase invocation.

## Renamed ADRs

- Proposed ADR 0053 became
  `docs/decisions/0064-pyth-native-typed-instruction-graph.md`.
- Proposed ADR 0054 became `docs/decisions/0065-pyth-graph-package-abi.md`.

Reason: the live repository already contains:

- `docs/decisions/0053-interactive-object-shell-launcher.md`
- `docs/decisions/0054-polling-ahci-block-backend.md`

ADR 0063 is already used by the physical evidence terminal, so the imported
PythTIG ADR sequence starts at ADR 0064.

## Corrected Assumptions

- The original PythTIG handoff was a proposal package, not an executable
  implementation branch.
- PythTIG is now accepted as an architecture direction through ADR 0064, and
  Phase 1 through Phase 7 are implemented on `main`.
- ADR 0065's package ABI is accepted and frozen for tested version 1 layouts.
  Incompatible byte-format changes require a new accepted ADR and a new major
  package version.
- The physical-evidence review baseline was `agent/physical-evidence-terminal`,
  not the older `main` snapshot.
- After the merge sequence, ADR 0063's implementation files, Cargo features,
  and acceptance harness are present on `main`.
- The earlier `main` claim-boundary correction was merged first, then replaced
  by the stronger post-merge statement: gallery artifacts are present, and the
  evidence-terminal QEMU acceptance path is reproducible from `main`.
- The Phase 0 marker-contract script and CI step from the unreconciled handoff
  are deferred because this pass is documentation-only.
- The root `AGENTS.md` active-milestone narrative predates the current
  Phase 10/physical-evidence baseline. This branch adds PythTIG guardrails but
  does not rewrite the entire historical active-milestone block.

## Live Symbol Evidence

| Mechanism | Live evidence |
|---|---|
| evidence-terminal boot feature | `boot/Cargo.toml` |
| evidence-terminal core feature | `core/Cargo.toml` |
| loader evidence log | `boot/src/evidence_log.rs` |
| shared evidence log ABI | `shared/src/evidence_log.rs` |
| core evidence log attach/render source | `core/src/evidence_log.rs` |
| framebuffer evidence terminal | `core/src/evidence_terminal.rs` |
| evidence-terminal QEMU harness | `scripts/test-evidence-terminal.py` |
| SDHCI/eMMC backend feature | `core/Cargo.toml` |
| object-shell ABI | `shared/src/object_shell_abi.rs` |
| `TYPE_NAMED_USER_ELF` | `shared/src/init_bundle.rs` |
| `load_named_user_program` | `core/src/runtime_loader.rs` |
| `enter_persistent_user_process` | `core/src/user_mode.rs` |
| `with_object_service` | `core/src/retained_services.rs` |
| `SYSCALL_OBJECT_REQUEST` | `shared/src/object_shell_abi.rs` |

## Current Authoritative Baseline

Object shell:

- ADR 0051 and ADR 0052 are accepted.
- `user/shell` and the object-shell ABI are present in this repository line.
- COM2 remains the interactive object-shell transport.
- Normal boot remains distinct from verification boot.

Evidence terminal:

- ADR 0063 is accepted and implemented on `main`.
- The merged repository contains `boot/src/evidence_log.rs`,
  `shared/src/evidence_log.rs`, `core/src/evidence_log.rs`,
  `core/src/evidence_terminal.rs`, the `evidence-terminal` Cargo features, and
  `scripts/test-evidence-terminal.py`.
- QEMU acceptance passes on merged `main` with `QEMU_OUTCOME success` and
  `EVIDENCE_TERMINAL_TEST_OK`.
- The five committed JPG frames remain physical artifact evidence scoped to the
  disposable O2 Micro `1217:8620` target; they are not a broadened hardware
  support claim.

Persistence path:

- Phase 10 dynamic storage proofs and SDHCI/eMMC target-limited physical panel
  evidence are recorded in `docs/HANDOVER.md`, ADR 0062, ADR 0063, and
  milestone docs.

Normal boot path:

- COM1 is still the boot and verification oracle.
- The evidence terminal is an opt-in visual capture path, not a replacement
  for COM1 serial acceptance.

## Unresolved Conflicts

- No repository merge conflicts remain from the Phase 0 adoption sequence.
- The historical `docs/HANDOVER.md` conflict between `main` and
  `agent/physical-evidence-terminal` was resolved by first merging the
  main-only claim correction, then merging the ADR 0063 implementation, then
  merging this PythTIG docs-only proposal.
- Remaining decisions are Phase 1 invocation and ABI-freeze decisions, not Git
  conflicts.

## Baseline Test Results

Command run from the original isolated worktree after docs-only import on
2026-08-04, then run again from merged `main` after the ADR 0063 merge:

```powershell
python scripts\test-evidence-terminal.py
```

Result: passed.

Key output:

```text
PYTHOS:CORE:BLOCK:SDHCI_EMMC_CONTROLLER_FOUND
PYTHOS:CORE:BLOCK:SDHCI_EMMC_CARD_READY
PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC
PYTHOS:CORE:PHASE_10_COMPLETE
PYTHOS:CORE:BLOCK:SDHCI_EMMC_FRAMEBUFFER_ACCEPTANCE_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
PYTHOS:CORE:EVIDENCE_TERMINAL_READY
QEMU_OUTCOME success
EVIDENCE_TERMINAL_TEST_OK
```

The harness built:

```text
pythos-boot: evidence-terminal
pythos-core: verify,sdhci-emmc-backend,evidence-terminal
```

Both runs built and verified the user shell ELF, built the ISO, booted QEMU
with `--no-virtio-blk --sdhci --emmc`, and captured
`target\evidence-terminal.ppm`.

## Proposed First Implementation Phase

If the owner invokes PythTIG implementation, the first implementation phase is
Phase 1: the smallest canonical typed instruction graph package and shared
verifier slice.

Phase 1 must begin in a fresh isolated worktree and with tests first.

## Decisions Still Requiring Owner Approval

- Whether to invoke PythTIG Phase 1 as the next active implementation phase.
- Whether the first Phase 1 implementation corpus proves ADR 0065's package ABI,
  type/opcode set, bounds, and verifier passes as written or requires revision.
- Whether to freeze ADR 0065 as permanent stable ABI after Phase 1 evidence.
- Whether PythTIG should sit alongside, precede, or replace the current
  Phase 12 package/application work.
