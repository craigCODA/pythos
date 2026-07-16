# Phase 4 Runtime Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Select the first PythOS embedded Python runtime using empirical evidence and make the boot oracle enforce the Phase 4 runtime-selection gate.

**Architecture:** Keep ADR 0012 as the standalone sequencing precondition. Evaluate RustPython, MicroPython, and a custom minimal interpreter in separate evidence files with one disqualifying smoke test each, then record the chosen runtime in a separate ADR before emitting `PYTHOS:CORE:RUNTIME_SELECTED`.

**Tech Stack:** Rust `no_std`, x86_64 QEMU/OVMF, Python unittest boot harness, candidate runtime build tools, Markdown ADRs.

## Global Constraints

Implement only the active milestone.
Do not invent or silently change an ABI.
Do not add future features to the active milestone.
Every unsafe block requires a documented invariant.
Every milestone requires an automated QEMU acceptance test.
Serial output is the test oracle for early boot.
A successful compile is not a successful boot.
A screenshot is not sufficient evidence.
Record architecture changes as ADRs.
Do not claim full security where only logical isolation exists.
AI remains outside the trusted core.
Phase 4 optimizes for the least painful Phase 8 migration, not the fastest first print.
Do not add interpreter code to `core/` during `runtime-selection`.

---

### Task 1: Red Marker Test For Runtime Selection

**Files:**
- Modify: `scripts/test-boot.py`
- Modify: `tests/boot_core_handoff.py`

**Interfaces:**
- Consumes: existing `audit-logging` marker sequence ending at `PYTHOS:CORE:PHASE_3_COMPLETE`
- Produces: new boot slice `python scripts\test-boot.py --slice runtime-selection`

- [ ] **Step 1: Add the failing marker expectation**

Add a `runtime-selection` slice that requires:

```text
PYTHOS:CORE:PHASE_3_COMPLETE
PYTHOS:CORE:RUNTIME_SELECTED
PYTHOS:CORE:FRAMEBUFFER_READY
```

Also insert `PYTHOS:CORE:RUNTIME_SELECTED` into the full milestone marker list after `PYTHOS:CORE:PHASE_3_COMPLETE` and before `PYTHOS:CORE:FRAMEBUFFER_READY`.

- [ ] **Step 2: Add the unittest wrapper**

Add this test method to `tests/boot_core_handoff.py`:

```python
def test_runtime_selection_marker_is_observed_after_phase_3_complete(self):
    self.run_slice("runtime-selection")
```

- [ ] **Step 3: Run the red test**

Run:

```powershell
python scripts\test-boot.py --slice runtime-selection
```

Expected result:

```text
missing marker: PYTHOS:CORE:RUNTIME_SELECTED
```

- [ ] **Step 4: Commit**

```powershell
git add scripts\test-boot.py tests\boot_core_handoff.py
git commit -m "test: require runtime selection marker"
```

### Task 2: RustPython Evidence

**Files:**
- Create: `docs/research/runtime-selection/rustpython.md`

**Interfaces:**
- Produces: candidate evidence for the runtime-selection ADR

- [ ] **Step 1: Create an isolated smoke-test directory**

Run:

```powershell
New-Item -ItemType Directory -Force target\runtime-selection\rustpython
```

- [ ] **Step 2: Attempt the smallest dependency build**

Create a temporary Cargo project under `target\runtime-selection\rustpython` and attempt to build a minimal crate that imports the selected RustPython crate with default features disabled. Do not commit generated Cargo files.

Record the exact command, exit code, and whether `std`, filesystem, threading, dynamic loading, or allocator assumptions appear.

- [ ] **Step 3: Write the evidence file**

Create `docs/research/runtime-selection/rustpython.md` with these headings:

```markdown
# RustPython Runtime Evidence

## Smoke Test

## Host-Controlled Embedding Surface

## no_std Or Bare-Metal Feasibility

## Memory And Allocator Assumptions

## Threading And OS Assumptions

## Native Boundary Shape

## Phase 8 Migration Cost

## Acceptance Or Rejection Reason
```

- [ ] **Step 4: Commit**

```powershell
git add docs\research\runtime-selection\rustpython.md
git commit -m "docs: record rustpython runtime evidence"
```

### Task 3: MicroPython Evidence

**Files:**
- Create: `docs/research/runtime-selection/micropython.md`

**Interfaces:**
- Produces: candidate evidence for the runtime-selection ADR

- [ ] **Step 1: Create an isolated smoke-test directory**

Run:

```powershell
New-Item -ItemType Directory -Force target\runtime-selection\micropython
```

- [ ] **Step 2: Attempt the smallest C build or port probe**

Attempt to compile the smallest MicroPython embedding or minimal port object with the local Windows toolchain. Do not commit generated sources, object files, or downloaded runtime trees.

Record the exact command, exit code, C compiler used, build-system assumptions, object files produced, and Rust-to-C boundary cost.

- [ ] **Step 3: Write the evidence file**

Create `docs/research/runtime-selection/micropython.md` with these headings:

```markdown
# MicroPython Runtime Evidence

## Smoke Test

## Host-Controlled Embedding Surface

## no_std Or Bare-Metal Feasibility

## Memory And Allocator Assumptions

## Threading And OS Assumptions

## Native Boundary Shape

## Phase 8 Migration Cost

## Acceptance Or Rejection Reason
```

- [ ] **Step 4: Commit**

```powershell
git add docs\research\runtime-selection\micropython.md
git commit -m "docs: record micropython runtime evidence"
```

### Task 4: Custom Minimal Interpreter Evidence

**Files:**
- Create: `docs/research/runtime-selection/custom-minimal-interpreter.md`

**Interfaces:**
- Produces: candidate evidence for the runtime-selection ADR

- [ ] **Step 1: Create an isolated smoke-test directory**

Run:

```powershell
New-Item -ItemType Directory -Force target\runtime-selection\custom-minimal
```

- [ ] **Step 2: Prototype only the target proof shape**

Write a temporary host-only prototype under `target\runtime-selection\custom-minimal` that recognizes this exact source shape:

```python
class HelloService(Service):
    async def start(self):
        system.log("hello from Python")
        self.ready()
```

The prototype may be a parser/interpreter sketch rather than a real Python implementation. Record the amount of parser, object model, async/event model, and compatibility work required for this path to be credible.

- [ ] **Step 3: Write the evidence file**

Create `docs/research/runtime-selection/custom-minimal-interpreter.md` with these headings:

```markdown
# Custom Minimal Interpreter Runtime Evidence

## Smoke Test

## Host-Controlled Embedding Surface

## no_std Or Bare-Metal Feasibility

## Memory And Allocator Assumptions

## Threading And OS Assumptions

## Native Boundary Shape

## Phase 8 Migration Cost

## Acceptance Or Rejection Reason
```

- [ ] **Step 4: Commit**

```powershell
git add docs\research\runtime-selection\custom-minimal-interpreter.md
git commit -m "docs: record custom interpreter runtime evidence"
```

### Task 5: Runtime Selection ADR

**Files:**
- Create: `docs/decisions/0013-python-runtime-selection.md`

**Interfaces:**
- Consumes: all three evidence files
- Produces: accepted runtime decision for later Phase 4 slices

- [ ] **Step 1: Write the ADR**

Create `docs/decisions/0013-python-runtime-selection.md` with these exact
sections:

```text
# ADR 0013: Python Runtime Selection
Date: 2026-07-16
## Status
## Context
## Decision
## Evidence
## Rejected Options
## Consequences
```

Set `## Status` to `Accepted`.

In `## Context`, state that Phase 4 requires a Python runtime embedded behind a
narrow, capability-gated `system.*` API and later migrated behind Phase 8
hardware isolation.

In `## Decision`, keep exactly one of these sentences:

```text
PythOS will use RustPython as its first embedded Python runtime.
PythOS will use MicroPython as its first embedded Python runtime.
PythOS will use a custom minimal interpreter as its first embedded Python runtime.
```

In `## Evidence`, write one paragraph for each candidate with its smoke-test
command, exit code, and conclusion.

In `## Rejected Options`, name the two rejected candidates and write one
specific technical rejection reason for each.

In `## Consequences`, list the implementation costs accepted by the chosen
runtime and the Phase 8 migration risks that remain.

- [ ] **Step 2: Check for missing rejection reasons**

Run:

```powershell
Select-String -Path docs\decisions\0013-python-runtime-selection.md -Pattern "Rejected Options|RustPython|MicroPython|custom"
```

Expected: all three candidates appear, with two explicit rejection reasons.

- [ ] **Step 3: Commit**

```powershell
git add docs\decisions\0013-python-runtime-selection.md
git commit -m "docs: select phase 4 python runtime"
```

### Task 6: Emit Runtime Selection Marker

**Files:**
- Modify: `core/src/main.rs`
- Modify: `docs/PythOS-TDD-001.md`
- Modify: `docs/ROADMAP.md`
- Modify: `AGENTS.md`
- Modify: `docs/HANDOVER.md`

**Interfaces:**
- Consumes: accepted ADR 0013
- Produces: `PYTHOS:CORE:RUNTIME_SELECTED`

- [ ] **Step 1: Emit the marker after Phase 3 completion**

In `core/src/main.rs`, emit:

```text
PYTHOS:CORE:RUNTIME_SELECTED
```

after `PYTHOS:CORE:PHASE_3_COMPLETE` and before framebuffer initialization. Do not add interpreter code.

- [ ] **Step 2: Update docs**

Document `runtime-selection` as complete in `docs/ROADMAP.md`, `docs/PythOS-TDD-001.md`, `AGENTS.md`, and `docs/HANDOVER.md`. State explicitly that no interpreter has booted yet.

- [ ] **Step 3: Run focused verification**

Run:

```powershell
python scripts\test-boot.py --slice runtime-selection
python -m unittest tests.boot_core_handoff.BootCoreHandoffTest.test_runtime_selection_marker_is_observed_after_phase_3_complete
```

Expected: both pass and the serial log contains `PYTHOS:CORE:RUNTIME_SELECTED`.

- [ ] **Step 4: Run full verification**

Run:

```powershell
cargo fmt --check
cargo test -p pythos-core
cargo test -p pythos-shared
cargo clippy -p pythos-core --target x86_64-unknown-none -- -D warnings
cargo clippy -p pythos-boot --target x86_64-unknown-uefi -- -D warnings
python scripts\test-boot.py --slice milestone-1
python scripts\test-boot.py --slice milestone-1 --media iso
```

Expected: all commands pass and both QEMU paths print `QEMU_OUTCOME success`.

- [ ] **Step 5: Commit**

```powershell
git add core\src\main.rs docs\PythOS-TDD-001.md docs\ROADMAP.md AGENTS.md docs\HANDOVER.md
git commit -m "core: record runtime selection gate"
```

### Task 7: Push And Stop Before `init-pak-loading`

**Files:**
- No source changes expected

**Interfaces:**
- Produces: pushed `milestone/phase4-runtime-selection` branch

- [ ] **Step 1: Check status**

Run:

```powershell
git status --short --branch
```

Expected: clean working tree on `milestone/phase4-runtime-selection`.

- [ ] **Step 2: Push branch**

Run:

```powershell
git push -u origin milestone/phase4-runtime-selection
```

- [ ] **Step 3: Stop**

Report the selected runtime, rejected candidates, verification commands, and branch/commit status. Do not begin `init-pak-loading` without explicit re-invocation.
