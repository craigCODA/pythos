# First Ring-3 Object Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build ADR 0051's first ring-3 object/capability shell without creating a kernel REPL: `shell.elf` owns human command parsing and presentation, while PythCore exposes typed, capability-gated object, console, and system-control services.

**Architecture:** Split normal boot from verification boot first, so normal boot constructs retained services and launches `shell.elf` without running the proof parade. COM1 remains the boot/test oracle; COM2 is initialized as the interactive shell transport. PythCore validates the current syscall caller, validates caller-supplied capabilities, adapts the existing Phase 10 typed-object/allocator/revision machinery into retained normal-boot state, and never parses human command text. Shell authority is delivered through a read-only launch bootstrap block, per-object authority is kept in a bounded shell-side map, and retained object-service state lives in static normal-boot storage reachable by syscall dispatch.

**Tech Stack:** Rust `no_std` PythCore, Rust `no_std` user shell ELF, shared Rust ABI crate, QEMU q35/OVMF, COM1 file serial log, COM2 TCP serial socket, Python acceptance harness, Cargo feature `verify`.

## Global Constraints

- Read `docs/PythOS-SAS-001.md`, `docs/PythOS-TDD-001.md`, and `docs/decisions/0051-first-ring3-object-shell.md` before editing.
- Implement only the ADR 0051 ring-3 object/capability shell slice and the minimum infrastructure required by this slice.
- Do not begin universal-device support, networking, package management, user-space drivers, AI agents, human `grant`, or packaged `launch`.
- Preserve `verify`: existing serial-marker proofs and `scripts/test-boot.py` must keep running under `--features verify`.
- Normal boot must skip verification-only adversarial/self-test execution while still initializing the scheduler, syscall entry, process identity, storage, object service, COM2, and shell.
- COM1 remains the verification oracle; COM2 carries interactive shell traffic.
- The shell is a ring-3 user process with declared bootstrap capabilities, not a privileged kernel console.
- Human command grammar belongs in `user/shell`; PythCore receives only typed ABI requests.
- Object IDs identify; capabilities authorize.
- Runtime capability handles are not persisted across reboot; fresh handles are minted from stable principal/workspace policy after validated program identity is rebound.
- Fresh handle means no runtime handle was serialized, old user address spaces and process state disappeared during reboot, and the new shell received authority only by querying its workspace in a new runtime capability table. Do not assert numeric handle inequality or invoke an old raw handle across boots unless a boot epoch is added by a separate ADR. Within one boot, handles remain holder-bound and revocable.
- `shell.elf` receives a read-only bootstrap capability block at launch with console, workspace, and system-control capabilities. It keeps a bounded object-id to object-capability map in user space.
- `create` stores the returned object capability. `query` returns fixed `ObjectListEntry { object_id, capability }` records and refreshes the shell map. `inspect`, `revise`, and `history` must use a per-object capability, querying first after reboot if the map has no entry. `CapabilityMap` retains the workspace capability separately from its object entries.
- Every console, object, and system-control syscall must derive authority from the current caller identity and a caller-supplied capability handle.
- Object-service persistence is a two-slot, multi-sector checkpoint ABI recorded in ADR 0052. One-sector snapshots are forbidden for this slice.
- The retained object service has one static normal-boot owner. Syscall dispatch reaches it only through a documented single-core access boundary after initialization and before shell launch.
- Known-object denial must target an object seeded in normal boot and proven on COM1 as existing outside the shell workspace before access is denied by capability validation.
- Normal boot must initialize the production substrate needed by the shell rather than relying on proof-only side effects.
- Every unsafe block requires a documented invariant with address, length, lifetime, ownership, alignment, concurrency, and violation notes.
- Serial output, not screenshots, is the acceptance oracle.
- The temporary kernel object bridge remains explicitly temporary, versioned, typed, and capability-gated until the object service moves to user space.
- COM2 busy polling is acceptable for this slice only and must be documented as a temporary CPU-consuming shell loop.
- Do not claim cryptographic code signing; the shell principal is tied to loader-validated bundle identity and digest, not to a general secure update chain.

---

## File Structure

- Create `docs/decisions/0052-object-shell-service-abi.md`: record the typed object-shell ABI, named user-program bundle record, caller-derived capability enforcement, normal/verify boot split, COM2 scope, and QEMU reboot mechanism.
- Modify `shared/src/lib.rs`: export the typed shell ABI and user-program manifest modules.
- Create `shared/src/object_shell_abi.rs`: define typed operations, response codes, request/response structs, object kind codes, field ids, syscall numbers, packed capability handles, `BootstrapCapabilityBlock`, `ObjectListEntry`, query capacity, and shell object-capability cache capacity.
- Create `shared/src/user_program_manifest.rs`: define the versioned named user ELF manifest payload used for `shell.elf` and adversarial test ELFs.
- Modify `shared/src/init_bundle.rs`: add a versioned `TYPE_NAMED_USER_ELF` record without changing existing ordinal `TYPE_USER_ELF`.
- Modify `scripts/run-qemu.py`: add optional COM2 TCP serial wiring while preserving the first COM1 serial log.
- Create `scripts/test-object-shell.py`: drive COM2, assert COM1 and COM2 evidence, cover forced power loss and actual shell-requested reboot.
- Create `scripts/verify-user-elf.py`: verify shell ELF headers and program segments with `readelf` output.
- Modify `scripts/build-image.py` and `scripts/build-iso.py`: embed `shell.elf` and the adversarial user ELF as named user-program manifest records while preserving existing verify payloads.
- Modify `core/src/serial.rs`: initialize and use COM2 deterministically.
- Modify `core/src/syscall.rs`: preserve existing ABI numbers, capture register args, derive the current caller, and dispatch typed console/object/system-control calls.
- Create `core/src/process_context.rs`: track the active ring-3 process identity, principal id, validated program digest, and bootstrap capability handles for syscalls.
- Create `core/src/normal_init.rs`: extract production initialization from proof routines for normal boot.
- Create `core/src/normal_boot.rs`: hold the normal-boot service initialization and shell launch path separately from verification proofs.
- Create `core/src/retained_services.rs`: own static retained normal-boot service storage and expose a documented single-core syscall access boundary.
- Modify `core/src/main.rs`: route `verify` builds to the existing proof sequence and normal builds to `normal_boot::run`.
- Modify `core/src/runtime_loader.rs`: validate named user-program manifest payloads and preserve existing ordinal user ELF lookup.
- Modify `core/src/user_elf.rs`: keep existing validation and expose launch metadata needed for program identity binding.
- Modify `core/src/user_mode.rs`: add persistent ring-3 entry and defined persistent-process fault handling.
- Modify `core/src/dynamic_object_store.rs`: promote the Phase 10 dynamic store from self-test-only operations into reusable normal-service operations.
- Create `core/src/object_service_checkpoint.rs`: define the ADR 0052 multi-sector object-service checkpoint layout and encode/decode/recovery helpers.
- Modify `core/src/object_relationships.rs`: add durable `belongs-to` workspace relationships for shell authority reconstruction.
- Modify `core/src/revision_history.rs`: expose bounded current/prior revision iteration needed by persistence and history responses.
- Modify `core/src/typed_object_format.rs` and `core/src/shell_objects.rs`: add note object kind and text field support.
- Create `core/src/object_service.rs`: adapt the Phase 10 dynamic object store, typed records, revision history, quota table, and persistence helpers into a retained capability-gated object service.
- Create `user/shell/Cargo.toml`, `user/shell/src/lib.rs`, `user/shell/src/main.rs`, `user/shell/src/commands.rs`, `user/shell/src/capability_map.rs`, `user/shell/src/syscalls.rs`, and `user/shell/linker.ld`: implement the real ring-3 shell.
- Create `user/probes/intruder` and `user/probes/fault-shell`: build concrete adversarial user ELFs instead of manifest-only placeholders.
- Modify root `Cargo.toml`: add `user/shell`.
- Modify `AGENTS.md`: after ADR 0052 is accepted, authorize `user/shell` and `user/probes` only for this ADR 0051 slice while preserving the ban on general ring-3 applications.
- Create `scripts/build-user-shell.py`: build `pythos-user-shell` with shell-only linker flags.
- Modify `docs/ROADMAP.md` and `docs/HANDOVER.md`: record the implemented ADR 0051 slice after it passes.

---

### Task 1: Record ADR 0052 And Normal/Verify Boot Split Test

**Status: COMPLETE (2026-07-26).** Implemented on branch `object-shell` with two
adaptations against the real codebase that this plan's example code did not
anticipate:

- **Address-space construction order.** `UserAddressSpace::build()`'s
  `PageTableBuilder` writes fresh page-table frames through raw physical
  addresses, which only work while the loader's broad low-memory identity map
  is still active. `KernelAddressSpace::activate()` removes that map by design
  (the Phase 1.5 identity-map-removal invariant), so building the user address
  space *after* activating the kernel one faults. Fixed by building both
  address spaces first, then activating only the kernel one — matching the
  verify path's existing order in `pythcore_entry`. `initialize_normal_substrate`
  therefore does this in one step (not two separate helpers as first sketched)
  and emits both `MEMORY_VM_READY` and `RING3_READY` together.
- **`run-qemu.py` exit codes.** `--expect-outcome timeout` still exits with the
  outcome's dedicated code (22, `SCRIPT_EXIT_CODES["timeout"]`), never 0, even
  on an exact match. `scripts/test-normal-fast-boot.py` asserts `expected=22`,
  not `expected=0`.
- Also fixed in passing: `cargo test -p pythos-core --bin pythcore` was broken
  (missing a diverging tail under `cfg(test)` once `qemu_exit::success()` became
  feature-gated in the earlier vertical-loop Slice 1) — now compiles and passes
  228 tests.
- `AGENTS.md` has no literal "writable milestone tree" boundary line to edit (it
  is stale re: current branch state); added the ADR 0051/0052 `user/shell`
  carve-out near the standing rules instead of the assumed line.

**Files:**
- Create: `docs/decisions/0052-object-shell-service-abi.md`
- Create: `scripts/test-normal-fast-boot.py`
- Modify: `core/src/main.rs`
- Create: `core/src/normal_init.rs`
- Create: `core/src/normal_boot.rs`
- Modify: `core/src/syscall.rs`
- Modify: `AGENTS.md`
- Test: `scripts/test-normal-fast-boot.py`, `scripts/test-boot.py`

**Interfaces:**
- Consumes: existing `pythcore_entry`, existing proof sequence, `scripts/run-qemu.py --serial-log`.
- Produces: `syscall::initialize()`, `normal_init::initialize_normal_substrate(boot_info: &'static PythBootInfo, physical_memory: &mut PhysicalMemory) -> Result<NormalBootSubstrate, NormalInitError>`, `normal_boot::run(boot_info: &'static PythBootInfo, physical_memory: &mut PhysicalMemory) -> !`, marker `PYTHOS:CORE:NORMAL_BOOT:FAST_PATH`, marker `PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY`, marker `PYTHOS:CORE:NORMAL_SERVICES_READY`.

- [x] **Step 1: Write ADR 0052**

Create `docs/decisions/0052-object-shell-service-abi.md`:

```markdown
# ADR 0052: Typed Object Shell Service ABI

Status: Accepted

## Context

ADR 0051 selects the first ring-3 object/capability shell as the next design
target. The implementation must not turn PythCore into a command interpreter.
Human command syntax belongs in `shell.elf`; PythCore exposes typed mechanisms.

## Decision

Define a typed object-shell ABI in `shared/src/object_shell_abi.rs`.
`shell.elf` parses command text into typed requests. PythCore accepts typed
requests only after deriving the current caller identity and validating a
caller-supplied capability handle.

Normal boot and verification boot are separate. Verification boot runs the
existing proof sequence and exits through the QEMU oracle. Normal boot skips
proof execution, initializes the production boot substrate, launches
`shell.elf`, and keeps running.

Normal boot initialization order:

```text
memory and kernel address space
interrupts and timer
task/process substrate
ring-3 GDT/TSS state
syscall gate through syscall::initialize()
user address-space support
guarded user stacks
block device
retained object service
COM2
shell address space and bootstrap block
shell entry
```

Proof functions may call the same production initializers, but normal boot must
not depend on proof-only side effects. In particular, syscall MSR setup moves
from `syscall::run_self_test()` into `syscall::initialize()`, and
`run_self_test()` calls that initializer before emitting the existing proof
markers.

Named user programs use a new versioned `TYPE_NAMED_USER_ELF` bundle record.
Existing ordinal `TYPE_USER_ELF` records remain valid for prior verification
payloads.

The initial shell principal is rebound only when the loaded process came from
the loader-validated `shell.elf` manifest record, kernel policy maps that name
to `SHELL_PRINCIPAL_ID`, the ELF digest matches the bundle record, and no other
named record duplicates the shell name or principal id. This is
loader-validated identity binding for the trusted bundle, not full
cryptographic code signing.

PythCore maps a read-only bootstrap block into the shell process at launch:
console capability, workspace capability, system-control capability, ABI
version, and any initial reachable object entries. `create` returns an object
capability; `query` returns fixed `ObjectListEntry { object_id, capability }`
records; the shell stores those capabilities in a bounded shell-side map before
`inspect`, `revise`, or `history`.

Workspace authority is reconstructed from durable object relationships:

```text
object:1042 -> belongs-to -> workspace:shell
object:2001 -> belongs-to -> workspace:external
```

The object service checkpoint stores these `belongs-to` relationships alongside
objects, extents, and revisions. Query grants object capabilities only for
objects related to the caller's workspace.

The object service checkpoint is a two-slot, multi-sector durable ABI:

```text
Slot A
  sector 192: metadata/header, generation, counts, layout version, checksum
  sectors 193-200: object records with object id, allocated extent, and typed record
  sectors 201-204: workspace belongs-to relationships
  sectors 205-216: current/prior revision records
  sector 217: commit marker

Slot B
  sector 224: metadata/header, generation, counts, layout version, checksum
  sectors 225-232: object records with object id, allocated extent, and typed record
  sectors 233-236: workspace belongs-to relationships
  sectors 237-248: current/prior revision records
  sector 249: commit marker

sector 250: torn-write test sector
```

Updates write the inactive slot completely, write that slot's commit marker
last, verify the slot, and then treat the highest valid committed generation as
current. Recovery selects the highest valid committed generation. The checksum
covers header metadata, object records, extent records, workspace
relationships, and revision records. The checkpoint preserves each object's
allocated extent and does not serialize runtime capability handles. Restored
access is rebuilt from the validated shell principal and workspace relationship
policy into a new runtime capability table.

The retained object service lives in static normal-boot storage initialized
before shell launch. ADR 0051 is single-core, so syscall dispatch may borrow it
through one documented `retained_services::with_object_service` boundary. If
the shell terminates, the service remains initialized and PythCore enters the
normal idle loop; no automatic shell restart is part of this slice.

The `reboot` command maps to a capability-gated system-control request. The
QEMU target uses an early x86 reset mechanism recorded in this ADR; forced
power loss remains a separate acceptance path.

The current repository instructions initially restrict the writable milestone
tree to `boot/`, `core/`, `shared/`, `scripts/`, `tests/`, and `docs/`, and
forbid ring-3 applications. ADR 0052 updates that active boundary to allow only
`user/shell` and `user/probes` for this first ring-3 shell slice. It does not
authorize general application work.

## Consequences

PythCore does not parse human command grammar.
Any ring-3 process can know syscall numbers, but only a caller holding the
required capability can use a console, object, or system-control operation.
Object persistence uses the retained Phase 10 object path, not a shell-private
sector format.
```

- [x] **Step 2: Write the failing normal-fast-boot test**

Create `scripts/test-normal-fast-boot.py`:

```python
#!/usr/bin/env python
"""Acceptance test for the ADR 0052 normal/verify boot split."""

from __future__ import annotations

import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "normal-fast-boot-com1.log"


def run(command: list[str], expected: int = 0) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout)
    if result.returncode != expected:
        raise AssertionError(f"{command} returned {result.returncode}, expected {expected}")
    return result.stdout


def main() -> int:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none"])
    run([sys.executable, "scripts/build-image.py"])
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--timeout",
            "20",
            "--expect-outcome",
            "timeout",
        ],
        expected=0,
    )
    serial = SERIAL_LOG.read_text(encoding="utf-8", errors="replace")
    required = [
        "PYTHOS:CORE:NORMAL_BOOT:FAST_PATH",
        "PYTHOS:CORE:NORMAL_INIT:MEMORY_VM_READY",
        "PYTHOS:CORE:NORMAL_INIT:INTERRUPTS_TIMER_READY",
        "PYTHOS:CORE:NORMAL_INIT:TASK_PROCESS_READY",
        "PYTHOS:CORE:NORMAL_INIT:RING3_READY",
        "PYTHOS:CORE:NORMAL_INIT:SYSCALL_READY",
        "PYTHOS:CORE:NORMAL_INIT:USER_STACKS_READY",
        "PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY",
        "PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY",
        "PYTHOS:CORE:NORMAL_SERVICES_READY",
        "PYTHOS:CORE:NORMAL_BOOT_ALIVE",
    ]
    for marker in required:
        if marker not in serial:
            raise AssertionError(f"missing {marker}")
    forbidden = [
        "PYTHOS:CORE:PROCESS_MODEL_ADVERSARIAL_READY",
        "PYTHOS:CORE:STORAGE_ADVERSARIAL_SUITE_READY",
        "PYTHOS:CORE:MILESTONE_1_COMPLETE",
    ]
    for marker in forbidden:
        if marker in serial:
            raise AssertionError(f"normal boot ran verification marker {marker}")
    print("NORMAL_FAST_BOOT_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [x] **Step 3: Run the failing normal-fast-boot test**

Run:

```powershell
python scripts\test-normal-fast-boot.py
```

Expected: FAIL because normal boot still reaches the existing event loop only after proof markers.

- [x] **Step 4: Split boot paths without changing proof behavior**

In `core/src/main.rs`, move the existing post-initialization proof sequence into:

```rust
#[cfg(feature = "verify")]
fn run_verification_boot(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut memory::physical::PhysicalMemory,
) -> ! {
    run_existing_proof_sequence(boot_info, physical_memory);
    qemu_exit::success();
}
```

Add the normal route:

```rust
#[cfg(not(feature = "verify"))]
fn run_normal_boot(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut memory::physical::PhysicalMemory,
) -> ! {
    normal_boot::run(boot_info, physical_memory)
}
```

In `core/src/syscall.rs`, extract the proof-only gate setup:

```rust
#[cfg(not(test))]
pub fn initialize() {
    configure_gate();
}
```

Then change `run_self_test()` to call `initialize()` before emitting
`PYTHOS:CORE:SYSCALL:MSRS_READY`. Existing verification markers must remain in
the same order.

Create `core/src/normal_init.rs`:

```rust
use crate::{block_device::BlockDeviceInfo, memory::physical::PhysicalMemory, serial};
use pythos_shared::boot_protocol::PythBootInfo;

pub struct NormalBootSubstrate {
    pub block_device: BlockDeviceInfo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalInitError {
    Memory,
    InterruptsTimer,
    TaskProcess,
    Ring3,
    Syscall,
    UserStacks,
    BlockDevice,
}

#[cfg(not(test))]
pub fn initialize_normal_substrate(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
) -> Result<NormalBootSubstrate, NormalInitError> {
    initialize_memory_and_kernel_address_space(boot_info, physical_memory)
        .map_err(|_| NormalInitError::Memory)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:MEMORY_VM_READY");

    initialize_interrupts_timer_and_clock().map_err(|_| NormalInitError::InterruptsTimer)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:INTERRUPTS_TIMER_READY");

    initialize_task_process_and_kernel_stack_state().map_err(|_| NormalInitError::TaskProcess)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:TASK_PROCESS_READY");

    initialize_ring3_selectors_tss_and_user_address_spaces()
        .map_err(|_| NormalInitError::Ring3)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:RING3_READY");

    crate::syscall::initialize();
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:SYSCALL_READY");

    initialize_guarded_user_stack_pool().map_err(|_| NormalInitError::UserStacks)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:USER_STACKS_READY");

    let block_device = crate::block_device::select_device()
        .map_err(|_| NormalInitError::BlockDevice)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY");

    Ok(NormalBootSubstrate { block_device })
}
```

Each `initialize_*` helper is extracted from the corresponding existing proof
path and performs only production setup. It must not emit self-test/adversarial
markers or deliberately trigger expected faults.

Define the private helper signatures in this task:

```rust
fn initialize_memory_and_kernel_address_space(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
) -> Result<(), NormalInitError>;
fn initialize_interrupts_timer_and_clock() -> Result<(), NormalInitError>;
fn initialize_task_process_and_kernel_stack_state() -> Result<(), NormalInitError>;
fn initialize_ring3_selectors_tss_and_user_address_spaces() -> Result<(), NormalInitError>;
fn initialize_guarded_user_stack_pool() -> Result<(), NormalInitError>;
```

In `core/src/normal_boot.rs` define:

```rust
use crate::{normal_init, serial};
use crate::memory::physical::PhysicalMemory;
use pythos_shared::boot_protocol::PythBootInfo;

#[cfg(not(test))]
pub fn run(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
) -> ! {
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT:FAST_PATH");
    let substrate = match normal_init::initialize_normal_substrate(boot_info, physical_memory) {
        Ok(substrate) => substrate,
        Err(_) => {
            serial::write_line("PYTHOS:PANIC");
            crate::qemu_exit::panic();
        }
    };
    let _ = substrate.block_device;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY");
    serial::write_line("PYTHOS:CORE:NORMAL_SERVICES_READY");
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT_ALIVE");
    loop {
        core::hint::spin_loop();
    }
}
```

The real service initialization and shell launch replace the temporary loop in later tasks.

- [x] **Step 5: Update active agent boundary**

Because the live `AGENTS.md` scope boundary still lists only
`boot/core/shared/scripts/tests/docs` and forbids ring-3 applications, update
that boundary after ADR 0052 is accepted:

```text
The ADR 0051 first-ring3-object-shell slice may additionally create
`user/shell` and `user/probes` only. This does not authorize general ring-3
applications, package management, networking, AI, or universal-device work.
```

- [x] **Step 6: Verify both boot modes**

Run:

```powershell
python scripts\test-normal-fast-boot.py
python scripts\test-boot.py
```

Expected:

```text
NORMAL_FAST_BOOT_TEST_OK
BOOT_TEST_OK
```

- [x] **Step 7: Commit**

```powershell
git add docs\decisions\0052-object-shell-service-abi.md scripts\test-normal-fast-boot.py core\src\main.rs core\src\normal_init.rs core\src\normal_boot.rs core\src\syscall.rs AGENTS.md
git commit -m "feat(boot): split normal boot from verification proofs"
```

---

### Task 2: COM2 UART Initialization And Harness

**Status: COMPLETE (2026-07-26).** Implemented as specified, with two
adaptations:

- Since `serial.rs`'s COM2 items (`init_com2`, `write_byte_com2`,
  `try_read_byte_com2`, `uart_init_sequence`, `UartWrite`, `COM2_BASE`,
  `RECEIVE_READY`) are only called from `normal_boot` (a `verify`-excluded
  module), they are legitimately dead code under `--features verify`. Added
  `#![cfg_attr(feature = "verify", allow(dead_code))]` at the module level
  (matching the existing `cinematic_boot.rs` precedent for the same class of
  build-config-asymmetric dead code) rather than gating each item individually.
- **Real bug found and fixed in `scripts/test-com2-shell-transport.py`
  itself**, not the product code: the original (plan-provided) version used
  `subprocess.Popen(..., stdout=subprocess.PIPE)` and `process.terminate()` to
  end the test early after the socket check. On Windows,
  `Popen.terminate()`/`.kill()` only signal the *direct* child
  (`run-qemu.py`'s own Python process) via `TerminateProcess`, which bypasses
  that process's own `finally` cleanup entirely — so its
  `qemu-system-x86_64.exe` grandchild was never reaped and was left running
  indefinitely after every test run (confirmed: found an orphaned QEMU process
  still alive after the harness "completed"). Fixed by redirecting stdout to
  `DEVNULL` (removing an unrelated pipe-buffer deadlock risk too) and using
  `taskkill /F /T /PID <pid>` on Windows to kill the whole process tree.

**Follow-up corrections (2026-07-26, review pass):**

- **Baud divisor bug:** `uart_init_sequence` wrote DLL `0x03` (divisor 3 =
  38400 baud) while the doc comment claimed divisor 1 (115200 baud). Fixed to
  write `0x01`, actually getting 115200 baud as documented.
- **The smoke test didn't prove COM2 data path works** — it only checked that
  `COM2_READY` appeared and that a byte could be *sent*, never that PythCore
  could read or write one. It would have passed even with broken/missing UART
  reads. Added a temporary non-blocking echo to `normal_boot`'s idle loop
  (`try_read_byte_com2` → `write_byte_com2`, to be replaced by the real shell
  loop in Task 8) and strengthened the test to send a byte and assert the
  exact byte echoes back.
- **The orphan-process fix was Windows-only.** On POSIX, bare
  `process.terminate()` still only kills `run-qemu.py`, not its QEMU child.
  Fixed by launching with `start_new_session=True` and killing the whole
  process group via `os.killpg` on POSIX (Windows keeps `taskkill /F /T`).
- **`outb`'s SAFETY comment was stale**, still claiming callers only pass COM1
  ports even though COM2 now calls it. Corrected to name both ports.

**Files:**
- Modify: `core/src/serial.rs`
- Modify: `core/src/normal_boot.rs`
- Modify: `scripts/run-qemu.py`
- Create: `scripts/test-com2-shell-transport.py`
- Test: `core/src/serial.rs`, `scripts/test-com2-shell-transport.py`

**Interfaces:**
- Consumes: COM1 serial writer and normal boot path from Task 1.
- Produces: `serial::init_com2()`, `serial::write_byte_com2(byte: u8)`, `serial::try_read_byte_com2() -> Option<u8>`, marker `PYTHOS:CORE:COM2_READY`, `scripts/run-qemu.py --shell-port <port>`.

- [x] **Step 1: Write COM2 initialization unit test**

In `core/src/serial.rs`, add a pure init-sequence helper test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn com2_init_sequence_targets_legacy_base() {
        let sequence = uart_init_sequence(COM2_BASE);
        assert_eq!(COM2_BASE, 0x2F8);
        assert_eq!(line_status_port(COM2_BASE), 0x2FD);
        assert_eq!(sequence[0], UartWrite::new(COM2_BASE + 1, 0x00));
        assert_eq!(sequence[1], UartWrite::new(COM2_BASE + 3, 0x80));
        assert_eq!(sequence[2], UartWrite::new(COM2_BASE, 0x03));
        assert_eq!(sequence[3], UartWrite::new(COM2_BASE + 1, 0x00));
        assert_eq!(sequence[4], UartWrite::new(COM2_BASE + 3, 0x03));
        assert_eq!(sequence[5], UartWrite::new(COM2_BASE + 2, 0xC7));
        assert_eq!(sequence[6], UartWrite::new(COM2_BASE + 4, 0x0B));
    }
}
```

- [x] **Step 2: Run the failing COM2 unit test**

Run:

```powershell
cargo test -p pythos-core serial::tests::com2_init_sequence_targets_legacy_base
```

Expected: FAIL because COM2 helpers do not exist.

- [x] **Step 3: Implement COM2 initialization**

In `core/src/serial.rs`, define:

```rust
const COM1_BASE: u16 = 0x3F8;
const COM2_BASE: u16 = 0x2F8;
const LINE_STATUS_OFFSET: u16 = 5;
const RECEIVE_READY: u8 = 0x01;
const TRANSMIT_EMPTY: u8 = 0x20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UartWrite {
    port: u16,
    value: u8,
}

impl UartWrite {
    pub const fn new(port: u16, value: u8) -> Self {
        Self { port, value }
    }
}

pub const fn line_status_port(base: u16) -> u16 {
    base + LINE_STATUS_OFFSET
}

pub const fn uart_init_sequence(base: u16) -> [UartWrite; 7] {
    [
        UartWrite::new(base + 1, 0x00),
        UartWrite::new(base + 3, 0x80),
        UartWrite::new(base, 0x03),
        UartWrite::new(base + 1, 0x00),
        UartWrite::new(base + 3, 0x03),
        UartWrite::new(base + 2, 0xC7),
        UartWrite::new(base + 4, 0x0B),
    ]
}

pub fn init_com2() {
    for write in uart_init_sequence(COM2_BASE) {
        outb(write.port, write.value);
    }
}
```

Keep COM1 behavior unchanged.

- [x] **Step 4: Add COM2 QEMU option**

In `scripts/run-qemu.py`, add:

```python
parser.add_argument("--shell-port", type=int)
```

When `args.shell_port` is set, append this second serial backend after COM1:

```python
command += ["-serial", f"tcp:127.0.0.1:{args.shell_port},server=on,wait=off"]
```

- [x] **Step 5: Write COM2 smoke test**

Create `scripts/test-com2-shell-transport.py`:

```python
#!/usr/bin/env python
"""COM2 transport smoke test for the normal shell path."""

from __future__ import annotations

import socket
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "com2-transport-com1.log"
SHELL_PORT = 4582


def run(command: list[str]) -> None:
    result = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout)
    if result.returncode != 0:
        raise AssertionError(f"{command} failed with {result.returncode}")


def wait_for_file_marker(path: Path, marker: str, timeout: float) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            text = path.read_text(encoding="utf-8", errors="replace")
            if marker in text:
                return text
        time.sleep(0.1)
    raise AssertionError(f"missing marker {marker}")


def main() -> int:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none"])
    run([sys.executable, "scripts/build-image.py"])
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    process = subprocess.Popen(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--shell-port",
            str(SHELL_PORT),
            "--timeout",
            "60",
            "--expect-outcome",
            "timeout",
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    try:
        wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:COM2_READY", 20)
        with socket.create_connection(("127.0.0.1", SHELL_PORT), timeout=5) as sock:
            sock.sendall(b"\n")
        print("COM2_TRANSPORT_TEST_OK")
        return 0
    finally:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=3)
        if process.stdout is not None:
            print(process.stdout.read())


if __name__ == "__main__":
    raise SystemExit(main())
```

- [x] **Step 6: Wire COM2 into normal boot and verify**

In `core/src/normal_boot.rs`, call:

```rust
serial::init_com2();
serial::write_line("PYTHOS:CORE:COM2_READY");
```

Run:

```powershell
cargo test -p pythos-core serial
python scripts\test-com2-shell-transport.py
python scripts\test-boot.py
```

Expected:

```text
COM2_TRANSPORT_TEST_OK
BOOT_TEST_OK
```

- [x] **Step 7: Commit**

```powershell
git add core\src\serial.rs core\src\normal_boot.rs scripts\run-qemu.py scripts\test-com2-shell-transport.py
git commit -m "feat(shell): initialize COM2 transport"
```

---

### Task 3: Shared Typed Shell ABI And Named User Program Manifest

**Status: COMPLETE (2026-07-26).** Implemented as specified; verified the
`#[repr(C)]` byte layouts by hand before coding (80/56/16/168 bytes for
`ObjectShellRequest`/`Response`/`ObjectListEntry`/`BootstrapCapabilityBlock`,
accounting for u64/`PackedCapability` alignment padding after the six leading
`u16` fields) — all matched the plan's asserted sizes exactly. One judgment
call: the plan's `UserProgramManifestError` enum has no dedicated
"nonzero reserved field" variant (unlike `init_bundle.rs`'s
`NonZeroReserved`), but the project's own written principle is to reject
nonzero reserved fields rather than silently ignore them. Reused
`UnsupportedVersion` for that case (a nonzero reserved field is a
forward-compatibility signal — an unknown, newer format) rather than
inventing an unauthorized new variant. Added a `NamedUserElf` round-trip test
to `init_bundle.rs` (not in the plan's text) since existing tests only cover
the two pre-existing record types.

**Follow-up hardening (2026-07-26, review pass):**

- The manifest `minor` version was encoded but never checked. Now requires an
  exact `major`/`minor` match (simplest correct policy for a first ABI
  version; no "newer but compatible" semantics are defined yet).
- Size-only assertions can't catch a field reordering that preserves total
  size (e.g. swapping two same-sized fields). Added `core::mem::offset_of!`
  and `align_of` assertions for every field of `ObjectShellRequest`,
  `ObjectShellResponse`, `ObjectListEntry`, and `BootstrapCapabilityBlock` —
  all matched the hand-computed layout from Task 3's original implementation
  exactly (41 shared-crate tests now, up from 36).

**Files:**
- Create: `shared/src/object_shell_abi.rs`
- Create: `shared/src/user_program_manifest.rs`
- Modify: `shared/src/lib.rs`
- Modify: `shared/src/init_bundle.rs`
- Test: `shared/src/object_shell_abi.rs`, `shared/src/user_program_manifest.rs`, `shared/src/init_bundle.rs`

**Interfaces:**
- Consumes: existing `TYPE_USER_ELF` ordinal records.
- Produces: `ObjectShellRequest`, `ObjectShellResponse`, `ObjectListEntry`, `BootstrapCapabilityBlock`, `PackedCapability`, `TYPE_NAMED_USER_ELF`, `NamedUserProgramManifest<'a>`.

- [x] **Step 1: Write ABI tests**

In `shared/src/object_shell_abi.rs`, define tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_response_layouts_are_stable() {
        assert_eq!(OBJECT_SHELL_ABI_MAJOR, 1);
        assert_eq!(OBJECT_KIND_NOTE, 10);
        assert_eq!(FIELD_TEXT, 1);
        assert_eq!(OP_CREATE_OBJECT, 1);
        assert_eq!(OP_QUERY_OBJECTS, 2);
        assert_eq!(OP_INSPECT_OBJECT, 3);
        assert_eq!(OP_REVISE_FIELD, 4);
        assert_eq!(OP_GET_HISTORY, 5);
        assert_eq!(core::mem::size_of::<ObjectShellRequest>(), 80);
        assert_eq!(core::mem::size_of::<ObjectShellResponse>(), 56);
        assert_eq!(core::mem::size_of::<ObjectListEntry>(), 16);
        assert_eq!(core::mem::size_of::<BootstrapCapabilityBlock>(), 168);
    }

    #[test]
    fn packed_capability_round_trips_slot_and_generation() {
        let packed = PackedCapability::from_parts(7, 9);
        assert_eq!(packed.slot(), 7);
        assert_eq!(packed.generation(), 9);
    }
}
```

- [x] **Step 2: Implement shared ABI**

Create:

```rust
pub const OBJECT_SHELL_ABI_MAJOR: u16 = 1;
pub const OBJECT_SHELL_ABI_MINOR: u16 = 0;

pub const SYSCALL_CONSOLE_READ_BYTE: u64 = 0x5059_0100;
pub const SYSCALL_CONSOLE_WRITE_BYTE: u64 = 0x5059_0101;
pub const SYSCALL_OBJECT_REQUEST: u64 = 0x5059_0120;
pub const SYSCALL_SYSTEM_REBOOT: u64 = 0x5059_0130;

pub const OBJECT_KIND_NOTE: u16 = 10;
pub const FIELD_TEXT: u16 = 1;

pub const OP_CREATE_OBJECT: u16 = 1;
pub const OP_QUERY_OBJECTS: u16 = 2;
pub const OP_INSPECT_OBJECT: u16 = 3;
pub const OP_REVISE_FIELD: u16 = 4;
pub const OP_GET_HISTORY: u16 = 5;

pub const STATUS_OK: u16 = 0;
pub const STATUS_DENIED: u16 = 1;
pub const STATUS_NOT_FOUND: u16 = 2;
pub const STATUS_BAD_REQUEST: u16 = 3;
pub const STATUS_BUFFER_TOO_SMALL: u16 = 4;

pub const SHELL_BOOTSTRAP_MAGIC: u64 = 0x3154_4F4F_4259_5350;
pub const MAX_SHELL_OBJECT_CAPS: usize = 8;
pub const MAX_QUERY_RESULTS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackedCapability {
    raw: u64,
}

impl PackedCapability {
    pub const fn from_raw(raw: u64) -> Self {
        Self { raw }
    }

    pub const fn from_parts(slot: u32, generation: u32) -> Self {
        Self { raw: (slot as u64) | ((generation as u64) << 32) }
    }

    pub const fn raw(self) -> u64 {
        self.raw
    }

    pub const fn slot(self) -> u32 {
        self.raw as u32
    }

    pub const fn generation(self) -> u32 {
        (self.raw >> 32) as u32
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectListEntry {
    pub object_id: u64,
    pub capability: PackedCapability,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootstrapCapabilityBlock {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub object_count: u16,
    pub reserved0: u16,
    pub console: PackedCapability,
    pub workspace: PackedCapability,
    pub system_control: PackedCapability,
    pub objects: [ObjectListEntry; MAX_SHELL_OBJECT_CAPS],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectShellRequest {
    pub abi_major: u16,
    pub abi_minor: u16,
    pub operation: u16,
    pub object_kind: u16,
    pub field_id: u16,
    pub reserved0: u16,
    pub authority: PackedCapability,
    pub object_id: u64,
    pub input_ptr: u64,
    pub input_len: u64,
    pub output_ptr: u64,
    pub output_len: u64,
    pub reserved1: u64,
    pub reserved2: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectShellResponse {
    pub status: u16,
    pub reserved0: u16,
    pub object_kind: u16,
    pub field_id: u16,
    pub object_id: u64,
    pub revision: u64,
    pub revision_count: u64,
    pub bytes_written: u64,
    pub capability: PackedCapability,
    pub reserved1: u64,
}
```

- [x] **Step 3: Write named manifest tests**

In `shared/src/user_program_manifest.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_manifest_round_trips_identity_digest_and_elf() {
        let mut bytes = [0u8; 96];
        let len = encode_named_user_program(
            &mut bytes,
            b"shell.elf",
            0x5059_5348_454C_4C01,
            b"\x7FELFpayload",
        )
        .unwrap();
        let manifest = validate_named_user_program(&bytes[..len]).unwrap();

        assert_eq!(manifest.name(), b"shell.elf");
        assert_eq!(manifest.principal_id(), 0x5059_5348_454C_4C01);
        assert_eq!(manifest.elf(), b"\x7FELFpayload");
        assert_eq!(manifest.elf_digest(), digest64(b"\x7FELFpayload"));
    }
}
```

- [x] **Step 4: Implement named user-program manifest**

Create:

```rust
pub const NAMED_USER_PROGRAM_MAGIC: &[u8; 8] = b"PYUPGM01";
pub const NAMED_USER_PROGRAM_MAJOR: u16 = 1;
pub const NAMED_USER_PROGRAM_MINOR: u16 = 0;
pub const NAMED_USER_PROGRAM_HEADER_LEN: usize = 40;

pub const SHELL_PRINCIPAL_ID: u64 = 0x5059_5348_454C_4C01;
pub const INTRUDER_PRINCIPAL_ID: u64 = 0x5059_494E_5452_4401;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserProgramManifestError {
    TooShort,
    BadMagic,
    UnsupportedVersion,
    NameTooLong,
    LengthOverflow,
    BadDigest,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NamedUserProgramManifest<'a> {
    name: &'a [u8],
    principal_id: u64,
    elf_digest: u64,
    elf: &'a [u8],
}
```

Use a deterministic FNV-1a 64-bit `digest64(bytes: &[u8]) -> u64`; document that it is an integrity binding for the trusted boot bundle, not a cryptographic signature.

- [x] **Step 5: Extend INIT bundle compatibly**

In `shared/src/init_bundle.rs`, add:

```rust
pub const TYPE_NAMED_USER_ELF: u32 = 0x0000_0003;
```

Extend `RecordType`:

```rust
NamedUserElf,
```

Do not change `INIT_BUNDLE_MAJOR`, `INIT_BUNDLE_MINOR`, `TYPE_USER_ELF`, ordinal lookup, record header length, or existing tests.

- [x] **Step 6: Run shared tests**

Run:

```powershell
cargo test -p pythos-shared object_shell_abi user_program_manifest init_bundle
```

Expected: PASS.

- [x] **Step 7: Commit**

```powershell
git add shared\src\lib.rs shared\src\object_shell_abi.rs shared\src\user_program_manifest.rs shared\src\init_bundle.rs
git commit -m "feat(shell): define typed object shell ABI"
```

---

### Task 4: Build And Verify Real `shell.elf`

**Status: COMPLETE (2026-07-26).** This is the first *real, compiled* ELF this
project has ever validated — the existing adversarial-probe ELFs
(`scripts/build-image.py::build_user_elf_payload`) are hand-crafted byte
arrays, not `rustc`+`lld` output. Several real, non-obvious issues surfaced
that the plan's text didn't anticipate:

- **Linker script.** Used the project's own *proven* explicit-`PHDRS` pattern
  from `core/linker.ld` (declaring `text`/`rodata`/`data` `PT_LOAD` headers by
  name) instead of the plan's plain `SECTIONS`-only script. Real linkers
  commonly add `PT_GNU_STACK`/`PT_NOTE` beyond `PT_LOAD`, which would have
  failed strict verification; explicit `PHDRS` suppressed them and the very
  first build passed cleanly.
- **`.cargo/config.toml` conflict.** The workspace already sets
  `[target.x86_64-unknown-none] rustflags = ["-Tcore/linker.ld", ...]`
  globally, which would apply to `user/shell` too. Confirmed Cargo's own
  precedence rules: the `RUSTFLAGS` env var (which `build-user-shell.py` sets)
  fully *replaces* — does not merge with — `target.<triple>.rustflags`, so
  this is safe as written.
- **ELF verification: real parser, not `readelf`.** This Windows host has no
  `readelf` at all, and its text output is fragile to regex against besides.
  `scripts/verify-user-elf.py` is a self-contained ELF64 header/program-header
  parser (stdlib only) implementing the full reviewer checklist (ELF64
  x86-64, `ET_EXEC` only, no `PT_INTERP`/`PT_DYNAMIC`, entry inside an
  executable `PT_LOAD`, every segment in the permitted user range, no W^X,
  page-aligned/non-overlapping ranges) — mirroring
  `core/src/user_elf.rs::validate()`'s exact constants and checks, so a
  Python pass here is strong (not proof-level) evidence the same bytes will
  pass the real Rust loader-side validator later.
- **The kernel-side syscall trampoline only forwards the syscall *number*
  today.** `core/src/syscall.rs`'s `syscall_entry_abi` overwrites `rdi` with
  `rax` before calling `syscall_dispatch_abi(number)` — there is no
  multi-argument dispatch yet. Per this task's own scope (it validates a
  ring-3 ELF; it does not execute the boundary — that's Task 8), `syscalls.rs`
  is written correctly against the *intended* standard `syscall` ABI
  (`rax`/`rdi`/`rsi`/`rdx`/`r10`/`r8`) and links cleanly, but nothing in it
  can actually execute until Task 7 extends the kernel-side dispatch.
- **Two `unsafe`/lint fixes applied per review guardrails:** the
  bootstrap-pointer dereference (`bootstrap_capabilities`) has its own
  complete invariant, separate from `syscall5`'s; `syscall5` declares `rcx`
  and `r11` clobbered (the `syscall` instruction unconditionally overwrites
  them) and is not marked `nomem` (PythCore reads/writes caller buffers).
- **Two build-config bugs fixed**, both matching patterns `core/src/main.rs`
  already established: (a) `cargo test` builds this crate for the *host*
  target, where an unconditional `#![no_main]` + `_start` conflicts with
  MSVC's linker (`LNK1561: entry point must be defined`) — fixed with the
  same `cfg_attr(not(test), no_std/no_main)` gating `core/src/main.rs` uses;
  (b) `_start` took a raw pointer without being marked `unsafe`
  (clippy `not_unsafe_ptr_arg_deref`) — fixed to `pub unsafe extern "C" fn`,
  matching `pythcore_entry`'s exact signature shape.

**Files:**
- Modify: `Cargo.toml`
- Create: `user/shell/Cargo.toml`
- Create: `user/shell/src/lib.rs`
- Create: `user/shell/src/main.rs`
- Create: `user/shell/src/commands.rs`
- Create: `user/shell/src/capability_map.rs`
- Create: `user/shell/src/syscalls.rs`
- Create: `user/shell/linker.ld`
- Create: `scripts/build-user-shell.py`
- Create: `scripts/verify-user-elf.py`
- Test: `user/shell/src/commands.rs`, `scripts/verify-user-elf.py`

**Interfaces:**
- Consumes: `pythos_shared::object_shell_abi`.
- Produces: `target/x86_64-unknown-none/debug/pythos-user-shell`, shell parser `parse_command(line: &[u8]) -> Result<Command, CommandError>`.

- [x] **Step 1: Add user shell crate**

Update root `Cargo.toml` members:

```toml
members = [
    "boot",
    "core",
    "shared",
    "user/shell",
]
```

Create `user/shell/Cargo.toml`:

```toml
[package]
name = "pythos-user-shell"
version = "0.1.0"
edition.workspace = true
license.workspace = true
publish.workspace = true
rust-version.workspace = true

[dependencies]
pythos-shared = { path = "../../shared" }

[[bin]]
name = "pythos-user-shell"
path = "src/main.rs"

[lib]
name = "pythos_user_shell"
path = "src/lib.rs"
```

- [x] **Step 2: Write command parser tests in user space**

Create `user/shell/src/commands.rs`:

```rust
use pythos_shared::object_shell_abi::{
    FIELD_TEXT, OBJECT_KIND_NOTE, OP_CREATE_OBJECT, OP_GET_HISTORY, OP_INSPECT_OBJECT,
    OP_QUERY_OBJECTS, OP_REVISE_FIELD, ObjectShellRequest,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandError {
    Empty,
    Unknown,
    BadObjectId,
    TextTooLong,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    Help,
    Reboot,
    Object {
        request: ObjectShellRequest,
        text: [u8; 16],
        text_len: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_human_commands_into_typed_requests() {
        assert_eq!(parse_command(b"help").unwrap(), Command::Help);
        assert_eq!(parse_command(b"reboot").unwrap(), Command::Reboot);
        assert!(matches!(
            parse_command(b"query kind:note").unwrap(),
            Command::Object { request, .. } if request.operation == OP_QUERY_OBJECTS
        ));
        assert!(matches!(
            parse_command(b"create kind:note").unwrap(),
            Command::Object { request, .. } if request.operation == OP_CREATE_OBJECT
        ));
        assert!(matches!(
            parse_command(b"inspect object:1042").unwrap(),
            Command::Object { request, .. } if request.operation == OP_INSPECT_OBJECT && request.object_id == 1042
        ));
        let revised = parse_command(br#"revise object:1042 text="hello""#).unwrap();
        match revised {
            Command::Object { request, text, text_len } => {
                assert_eq!(request.operation, OP_REVISE_FIELD);
                assert_eq!(request.field_id, FIELD_TEXT);
                assert_eq!(&text[..text_len], b"hello");
            }
            _ => panic!("expected object command"),
        }
        assert!(matches!(
            parse_command(b"history object:1042").unwrap(),
            Command::Object { request, .. } if request.operation == OP_GET_HISTORY
        ));
    }

    #[test]
    fn rejects_shell_grammar_errors_before_syscall() {
        assert_eq!(parse_command(b""), Err(CommandError::Empty));
        assert_eq!(parse_command(b"ls /"), Err(CommandError::Unknown));
        assert_eq!(parse_command(b"inspect object:notanumber"), Err(CommandError::BadObjectId));
    }
}
```

- [x] **Step 3: Implement parser and shell presentation**

Implement `parse_command` so every supported human command becomes a `Command` variant. `help` is handled entirely in user space and does not call the object bridge. The parser sets `request.authority = PackedCapability::from_raw(0)`; `run_line` fills the workspace or per-object capability after consulting the shell capability map.

In `user/shell/src/main.rs`, write the shell loop:

```rust
#![no_std]
#![no_main]

use core::panic::PanicInfo;
use pythos_shared::object_shell_abi::{BootstrapCapabilityBlock, PackedCapability};
use pythos_user_shell::{
    capability_map::CapabilityMap,
    commands::{parse_command, Command},
    syscalls,
};

#[unsafe(no_mangle)]
pub extern "C" fn _start(bootstrap_ptr: *const BootstrapCapabilityBlock) -> ! {
    let bootstrap = syscalls::bootstrap_capabilities(bootstrap_ptr);
    let console = bootstrap.console;
    let system_control = bootstrap.system_control;
    let mut object_caps = CapabilityMap::from_bootstrap(&bootstrap);
    let mut line = [0u8; 96];
    syscalls::write_str(console, "PYTHOS:SHELL:READY\r\n");
    syscalls::write_str(console, "PYTHOS:SHELL:POLLING_COM2\r\n");
    syscalls::write_str(console, "pyth> ");
    let mut len = 0usize;
    loop {
        if let Some(byte) = syscalls::read_byte(console) {
            if byte == b'\r' || byte == b'\n' {
                syscalls::write_str(console, "\r\n");
                run_line(console, system_control, &mut object_caps, &line[..len]);
                len = 0;
                syscalls::write_str(console, "pyth> ");
            } else if len < 96 {
                line[len] = byte;
                len += 1;
                syscalls::write_byte(console, byte);
            }
        } else {
            core::hint::spin_loop();
        }
    }
}

fn run_line(
    console: PackedCapability,
    system_control: PackedCapability,
    object_caps: &mut CapabilityMap,
    line: &[u8],
) {
    match parse_command(line) {
        Ok(Command::Help) => syscalls::write_help(console),
        Ok(Command::Reboot) => syscalls::request_reboot(system_control),
        Ok(Command::Object { mut request, text, text_len }) => {
            syscalls::dispatch_object_request(console, object_caps, &mut request, &text[..text_len])
        }
        Err(_) => syscalls::write_str(console, "ERROR unknown-command\r\n"),
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
```

Create `user/shell/src/lib.rs`:

```rust
#![no_std]

pub mod capability_map;
pub mod commands;
pub mod syscalls;
```

Create `user/shell/src/capability_map.rs`. It must retain the workspace
capability separately from object entries, keep at most `MAX_SHELL_OBJECT_CAPS`
object entries, seed itself from `BootstrapCapabilityBlock`, store the object
capability returned by `create`, update entries returned by `query`, and
perform a workspace `query kind:note` before `inspect`, `revise`, or `history`
when the requested object id has no cached object capability.

`dispatch_object_request` must set authority by operation:

```text
OP_CREATE_OBJECT  -> bootstrap workspace capability
OP_QUERY_OBJECTS  -> bootstrap workspace capability
OP_INSPECT_OBJECT -> cached/rebound object capability for request.object_id
OP_REVISE_FIELD   -> cached/rebound object capability for request.object_id
OP_GET_HISTORY    -> cached/rebound object capability for request.object_id
```

If no object capability can be obtained after the refresh query, the shell
still sends the typed request with a zero authority handle only to receive and
present the expected `DENIED missing-capability`; it must not substitute the
workspace capability for per-object operations.

- [x] **Step 4: Add syscall wrappers with documented unsafe assembly**

In `user/shell/src/syscalls.rs`, implement `syscall5` with this invariant immediately before the unsafe block:

```rust
// SAFETY:
// 1. Invariant: the syscall numbers and register ABI are defined by
//    `pythos_shared::object_shell_abi` and ADR 0052.
// 2. Established by: shell build depends on the same shared crate used by
//    PythCore.
// 3. Lifetime: pointer arguments, when present, refer to stack or static
//    objects that remain live until the syscall returns.
// 4. Pointer ownership: mutable output buffers are not aliased across the call.
// 5. Alignment: request and response pointers are naturally aligned Rust values.
// 6. Mapped length: request and response lengths are exact `size_of` values;
//    text/output buffers pass explicit byte lengths.
// 7. Concurrency: this shell is single-threaded in ADR 0051.
// 8. Violation: wrong registers or dangling pointers cause PythCore copy-in or
//    copy-out validation to deny the call or terminate the process.
```

Also implement `bootstrap_capabilities(ptr: *const BootstrapCapabilityBlock)`.
It validates `SHELL_BOOTSTRAP_MAGIC`, ABI major, and `object_count <=
MAX_SHELL_OBJECT_CAPS`, then copies the block into a stack value. The pointer is
read-only user memory supplied in `rdi` by the PythCore launch ABI; the shell
must not fabricate bootstrap capabilities if validation fails.

- [x] **Step 5: Add shell linker and build script**

Create `user/shell/linker.ld`:

```ld
ENTRY(_start)

SECTIONS
{
  . = 0x0000000000400000;
  .text : ALIGN(4096) { *(.text .text.*) }
  .rodata : ALIGN(4096) { *(.rodata .rodata.*) }
  .data : ALIGN(4096) { *(.data .data.*) }
  .bss : ALIGN(4096) { *(.bss .bss.* COMMON) }
}
```

Create `scripts/build-user-shell.py`:

```python
#!/usr/bin/env python
"""Build the ADR 0051 ring-3 shell ELF."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SHELL_LINKER = ROOT / "user" / "shell" / "linker.ld"


def main() -> int:
    env = os.environ.copy()
    env["RUSTFLAGS"] = " ".join(
        [
            "-C", "relocation-model=static",
            "-C", f"link-arg=-T{SHELL_LINKER}",
            "-C", "link-arg=--no-pie",
        ]
    )
    return subprocess.call(
        ["cargo", "build", "-p", "pythos-user-shell", "--target", "x86_64-unknown-none"],
        cwd=ROOT,
        env=env,
    )


if __name__ == "__main__":
    raise SystemExit(main())
```

- [x] **Step 6: Add readelf verification script**

Create `scripts/verify-user-elf.py`:

```python
#!/usr/bin/env python
"""Verify the ring-3 user ELF shape with readelf."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"


def main() -> int:
    header = subprocess.run(["readelf", "-h", str(SHELL)], text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(header.stdout)
    if header.returncode != 0:
        return header.returncode
    program = subprocess.run(["readelf", "-l", str(SHELL)], text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(program.stdout)
    if program.returncode != 0:
        return program.returncode
    if re.search(r"Type:\s+EXEC", header.stdout) is None:
        raise AssertionError("user shell is not ET_EXEC")
    if re.search(r"Entry point address:\s+0x[0-9a-fA-F]+", header.stdout) is None:
        raise AssertionError("user shell has no entry address")
    if "LOAD" not in program.stdout:
        raise AssertionError("user shell has no LOAD segment")
    if "RWE" in program.stdout:
        raise AssertionError("user shell has a writable-executable LOAD segment")
    print("USER_ELF_VERIFY_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [x] **Step 7: Run shell tests and verification**

Run:

```powershell
cargo test -p pythos-user-shell commands
python scripts\build-user-shell.py
python scripts\verify-user-elf.py
```

Expected:

```text
USER_ELF_VERIFY_OK
```

- [x] **Step 8: Commit**

```powershell
git add Cargo.toml user\shell scripts\build-user-shell.py scripts\verify-user-elf.py
git commit -m "feat(shell): build real ring-3 command shell"
```

---

### Task 5: Named Program Packaging And Loader-Validated Principal Binding

**Files:**
- Modify: `scripts/build-image.py`
- Modify: `scripts/build-iso.py`
- Modify: `core/src/runtime_loader.rs`
- Create: `core/src/process_context.rs`
- Modify: `core/src/main.rs`
- Test: `core/src/runtime_loader.rs`, `core/src/process_context.rs`

**Interfaces:**
- Consumes: `TYPE_NAMED_USER_ELF`, `NamedUserProgramManifest`, `SHELL_PRINCIPAL_ID`.
- Produces: `runtime_loader::load_named_user_program(boot_info: &PythBootInfo, name: &[u8]) -> Result<NamedUserProgramManifest<'_>, RuntimeLoadError>`, `process_context::ActiveUserProcess`.

- [ ] **Step 1: Write runtime-loader named-program tests**

In `core/src/runtime_loader.rs`, add:

```rust
#[test]
fn named_user_program_loader_binds_shell_identity_to_manifest() {
    let shell = build_named_user_program(b"shell.elf", SHELL_PRINCIPAL_ID, b"\x7FELFpayload");
    let bundle = build_init_pak(&build_inner_bundle(&[
        (pythos_shared::init_bundle::TYPE_RUNTIME_PAYLOAD, build_runtime_payload(HELLO_SERVICE).as_slice()),
        (pythos_shared::init_bundle::TYPE_NAMED_USER_ELF, shell.as_slice()),
    ]));

    let loaded = validate_named_user_program_payload_bytes(&bundle, b"shell.elf").unwrap();

    assert_eq!(loaded.name(), b"shell.elf");
    assert_eq!(loaded.principal_id(), SHELL_PRINCIPAL_ID);
    assert_eq!(loaded.elf(), b"\x7FELFpayload");
}

#[test]
fn named_user_program_loader_rejects_duplicate_shell_principal_claim() {
    let shell = build_named_user_program(b"shell.elf", SHELL_PRINCIPAL_ID, b"\x7FELFpayload");
    let impostor = build_named_user_program(b"other.elf", SHELL_PRINCIPAL_ID, b"\x7FELFimpostor");
    let bundle = build_init_pak(&build_inner_bundle(&[
        (pythos_shared::init_bundle::TYPE_NAMED_USER_ELF, shell.as_slice()),
        (pythos_shared::init_bundle::TYPE_NAMED_USER_ELF, impostor.as_slice()),
    ]));

    assert_eq!(
        validate_named_user_program_payload_bytes(&bundle, b"shell.elf"),
        Err(RuntimeLoadError::DuplicateProgramPrincipal)
    );
}

#[test]
fn named_user_program_loader_rejects_duplicate_shell_name() {
    let shell = build_named_user_program(b"shell.elf", SHELL_PRINCIPAL_ID, b"\x7FELFpayload");
    let duplicate = build_named_user_program(b"shell.elf", INTRUDER_PRINCIPAL_ID, b"\x7FELFother");
    let bundle = build_init_pak(&build_inner_bundle(&[
        (pythos_shared::init_bundle::TYPE_NAMED_USER_ELF, shell.as_slice()),
        (pythos_shared::init_bundle::TYPE_NAMED_USER_ELF, duplicate.as_slice()),
    ]));

    assert_eq!(
        validate_named_user_program_payload_bytes(&bundle, b"shell.elf"),
        Err(RuntimeLoadError::DuplicateProgramName)
    );
}
```

- [ ] **Step 2: Implement named-program loader**

In `core/src/runtime_loader.rs`, add:

```rust
pub fn load_named_user_program(
    boot_info: &PythBootInfo,
    name: &[u8],
) -> Result<NamedUserProgramManifest<'_>, RuntimeLoadError> {
    let bytes = init_bundle_bytes(boot_info)?;
    validate_named_user_program_payload_bytes(bytes, name)
}

pub fn validate_named_user_program_payload_bytes(
    bytes: &[u8],
    name: &[u8],
) -> Result<NamedUserProgramManifest<'_>, RuntimeLoadError> {
    let payload = init_pak_payload(bytes)?;
    let bundle = init_bundle::validate(payload).map_err(|_| RuntimeLoadError::BadInitBundle)?;
    let mut policy = NamedProgramPolicy::empty();
    let mut selected = None;
    let mut index = 0usize;
    while let Some(record) = bundle.record_at(init_bundle::RecordType::NamedUserElf, index) {
        let manifest = user_program_manifest::validate_named_user_program(record.bytes())
            .map_err(|_| RuntimeLoadError::BadUserElfPayload)?;
        policy.observe(manifest)?;
        if manifest.name() == name {
            selected = Some(manifest);
        }
        index += 1;
    }
    let manifest = selected.ok_or(RuntimeLoadError::MissingUserElfPayload)?;
    enforce_kernel_identity_policy(manifest)?;
    Ok(manifest)
}
```

`enforce_unique_named_program_policy` must scan all `TYPE_NAMED_USER_ELF`
records and reject duplicate names or duplicate principals before returning any
manifest. `enforce_kernel_identity_policy` must bind `b"shell.elf"` to
`SHELL_PRINCIPAL_ID`; a different named record may not claim that principal.
The FNV digest remains an integrity check inside the trusted bundle, not proof
that a program is entitled to choose its own principal. Preserve
`load_user_elf_payload_at` and all ordinal user ELF tests.

- [ ] **Step 3: Package named shell program**

In `scripts/build-image.py` and `scripts/build-iso.py`, add `build_named_user_program(name, principal_id, elf)` mirroring `shared/src/user_program_manifest.rs`.

Append records:

```python
(INIT_BUNDLE_NAMED_USER_ELF_TYPE, build_named_user_program(b"shell.elf", SHELL_PRINCIPAL_ID, SHELL_ELF.read_bytes()))
```

Do not remove the existing ordinal `INIT_BUNDLE_USER_ELF_TYPE` records. The
intruder probe is packaged later in Task 11 after its concrete source and build
script exist.

- [ ] **Step 4: Write process-context tests**

Create `core/src/process_context.rs` with:

```rust
use crate::service_identity::{ServiceId, ServiceIdentityTable};
use crate::tasks::TaskId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveUserProcess {
    service_id: ServiceId,
    principal_id: u64,
    program_digest: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_identity_comes_from_active_process_not_task_slot_constant() {
        let mut identities = ServiceIdentityTable::new();
        let shell_service = identities.register_task(TaskId::new(180)).unwrap();
        let intruder_service = identities.register_task(TaskId::new(181)).unwrap();
        let shell = ActiveUserProcess::new(shell_service, SHELL_PRINCIPAL_ID, 0xAA);
        let intruder = ActiveUserProcess::new(intruder_service, INTRUDER_PRINCIPAL_ID, 0xBB);

        set_current_for_test(shell);
        assert_eq!(current_caller_for_test().unwrap().principal_id(), SHELL_PRINCIPAL_ID);
        set_current_for_test(intruder);
        assert_eq!(current_caller_for_test().unwrap().principal_id(), INTRUDER_PRINCIPAL_ID);
    }
}
```

- [ ] **Step 5: Implement active caller tracking**

Implement:

```rust
pub const fn new(service_id: ServiceId, principal_id: u64, program_digest: u64) -> ActiveUserProcess;
pub const fn service_id(self) -> ServiceId;
pub const fn principal_id(self) -> u64;
pub const fn program_digest(self) -> u64;
pub fn bind_current_process(process: ActiveUserProcess);
pub fn current_caller() -> Result<ActiveUserProcess, ProcessContextError>;
```

If a `static mut` or `UnsafeCell` holds the active process, document this invariant:

```rust
// SAFETY:
// 1. Invariant: ADR 0051 runs one active ring-3 process at a time on one CPU.
// 2. Established by: QEMU target is single-core and persistent shell launch is
//    not preemptively migrated across address spaces in this slice.
// 3. Lifetime: copied identity values outlive the syscall that reads them.
// 4. Pointer ownership: no borrowed process table references escape.
// 5. Alignment: static storage is naturally aligned for `ActiveUserProcess`.
// 6. Mapped length: exactly one process context cell is accessed.
// 7. Concurrency: SMP is out of scope for ADR 0051.
// 8. Violation: concurrent mutation would allow wrong-caller authority checks.
```

- [ ] **Step 6: Run tests**

Run:

```powershell
cargo test -p pythos-core runtime_loader process_context
python scripts\build-user-shell.py
python scripts\build-image.py
python scripts\verify-user-elf.py
```

Expected:

```text
USER_ELF_VERIFY_OK
```

- [ ] **Step 7: Commit**

```powershell
git add scripts\build-image.py scripts\build-iso.py core\src\runtime_loader.rs core\src\process_context.rs core\src\main.rs
git commit -m "feat(shell): bind shell principal to named ELF manifest"
```

---

### Task 6: Promote Phase 10 Object Store Into Retained Service

**Files:**
- Modify: `core/src/dynamic_object_store.rs`
- Create: `core/src/object_service_checkpoint.rs`
- Create: `core/src/retained_services.rs`
- Modify: `core/src/object_relationships.rs`
- Modify: `core/src/revision_history.rs`
- Modify: `core/src/typed_object_format.rs`
- Modify: `core/src/shell_objects.rs`
- Create: `core/src/object_service.rs`
- Modify: `core/src/main.rs`
- Test: `core/src/object_service.rs`, `core/src/dynamic_object_store.rs`, `core/src/object_service_checkpoint.rs`, `core/src/object_relationships.rs`, `core/src/retained_services.rs`

**Interfaces:**
- Consumes: `DynamicObjectStore`, `BlockAllocator`, `StorageQuotaTable`, `RevisionHistory`, `TypedObjectRecord`.
- Produces: `ObjectService::restore_or_initialize(device: BlockDeviceInfo) -> Result<Self, ObjectServiceError>`, `ObjectService::create_object`, `ObjectService::query_objects`, `ObjectService::inspect_object`, `ObjectService::revise_field`, `ObjectService::history`, `retained_services::initialize_object_service`, `retained_services::with_object_service`.

- [ ] **Step 1: Add note kind test**

In `core/src/typed_object_format.rs`:

```rust
#[test]
fn note_kind_round_trips_with_stable_code() {
    let mut record = TypedObjectRecord::new(ObjectId::new(1042), ObjectKind::Note, 1);
    record.push_field(TypedObjectField::new(1, 1, b"hello").unwrap()).unwrap();
    let decoded = TypedObjectRecord::decode(&record.encode()).unwrap();

    assert_eq!(decoded.object_id().raw(), 1042);
    assert_eq!(decoded.object_kind(), ObjectKind::Note);
    assert_eq!(decoded.field(0).unwrap().field_id(), 1);
}
```

- [ ] **Step 2: Add `ObjectKind::Note`**

In `core/src/shell_objects.rs`, add:

```rust
Note,
```

In `core/src/typed_object_format.rs`, encode `ObjectKind::Note` as `10` and decode `10` as `ObjectKind::Note`.

- [ ] **Step 3: Expose dynamic object lookup and iteration**

In `core/src/dynamic_object_store.rs`, add tests:

```rust
#[test]
fn store_returns_existing_typed_object_by_id() {
    let mut store = DynamicObjectStore::new(64, 8).unwrap();
    let record = TypedObjectRecord::new(ObjectId::new(1042), ObjectKind::Note, 1);
    store.create_object(record).unwrap();

    assert_eq!(store.object(ObjectId::new(1042)), Some(record));
    assert_eq!(store.object(ObjectId::new(2001)), None);
}
```

Implement:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicObjectRecord {
    pub object: TypedObjectRecord,
    pub extent: BlockExtent,
}

pub fn object(self, object_id: ObjectId) -> Option<TypedObjectRecord>;
pub fn replace_object(&mut self, object: TypedObjectRecord) -> Result<(), DynamicObjectError>;
pub fn object_records(self) -> [Option<DynamicObjectRecord>; MAX_DYNAMIC_OBJECTS];
pub fn allocator_bitmap(self) -> u64;
pub fn restore_from_records(
    base_sector: u64,
    block_count: u16,
    bitmap: u64,
    objects: [Option<DynamicObjectRecord>; MAX_DYNAMIC_OBJECTS],
) -> Result<Self, DynamicObjectError>;
```

`restore_from_records` validates that every extent is inside the allocator
range, every occupied extent bit is present in `bitmap`, no two object extents
overlap, and no set bitmap bit lacks a corresponding restored extent.

- [ ] **Step 4: Add durable workspace membership relationships**

In `core/src/object_relationships.rs`, extend `RelationshipKind`:

```rust
pub const SHELL_WORKSPACE_OBJECT_ID: u64 = 0x5059_5753_4845_4C01;
pub const EXTERNAL_WORKSPACE_OBJECT_ID: u64 = 0x5059_5753_4558_5401;

BelongsTo,
```

Add tests:

```rust
#[test]
fn belongs_to_relationship_distinguishes_shell_and_external_workspaces() {
    let shell_workspace = object(SHELL_WORKSPACE_OBJECT_ID, ObjectKind::WorkspaceSession);
    let external_workspace = object(EXTERNAL_WORKSPACE_OBJECT_ID, ObjectKind::WorkspaceSession);
    let note = object(1042, ObjectKind::Note);
    let external_note = object(2001, ObjectKind::Note);
    let mut store = RelationshipStore::new();
    store.insert_object(shell_workspace).unwrap();
    store.insert_object(external_workspace).unwrap();
    store.insert_object(note).unwrap();
    store.insert_object(external_note).unwrap();

    store
        .add_relationship(ObjectRelationship::new(
            note.object_id(),
            RelationshipKind::BelongsTo,
            shell_workspace.object_id(),
        ))
        .unwrap();
    store
        .add_relationship(ObjectRelationship::new(
            external_note.object_id(),
            RelationshipKind::BelongsTo,
            external_workspace.object_id(),
        ))
        .unwrap();

    assert_eq!(
        store.query_first(note.object_id(), RelationshipKind::BelongsTo).unwrap().target(),
        shell_workspace.object_id()
    );
    assert_eq!(
        store.query_first(external_note.object_id(), RelationshipKind::BelongsTo).unwrap().target(),
        external_workspace.object_id()
    );
}
```

The shell workspace is represented by stable object id
`SHELL_WORKSPACE_OBJECT_ID`; the known denial fixture uses
`EXTERNAL_WORKSPACE_OBJECT_ID`. Object-service `query` reconstructs authority
from `BelongsTo` relationships, not from object-id ranges or record order.

- [ ] **Step 5: Define two-slot object-service checkpoint helpers**

Create `core/src/object_service_checkpoint.rs` for the ADR 0052 durable layout.
Do not encode this state into one 512-byte sector.

```rust
use crate::block_device::{self, BlockDeviceInfo};

pub const OBJECT_SERVICE_SLOT_A_HEADER_SECTOR: u64 = 192;
pub const OBJECT_SERVICE_SLOT_A_OBJECT_TABLE_SECTOR: u64 = 193;
pub const OBJECT_SERVICE_SLOT_A_RELATIONSHIP_TABLE_SECTOR: u64 = 201;
pub const OBJECT_SERVICE_SLOT_A_REVISION_TABLE_SECTOR: u64 = 205;
pub const OBJECT_SERVICE_SLOT_A_COMMIT_SECTOR: u64 = 217;
pub const OBJECT_SERVICE_SLOT_B_HEADER_SECTOR: u64 = 224;
pub const OBJECT_SERVICE_SLOT_B_OBJECT_TABLE_SECTOR: u64 = 225;
pub const OBJECT_SERVICE_SLOT_B_RELATIONSHIP_TABLE_SECTOR: u64 = 233;
pub const OBJECT_SERVICE_SLOT_B_REVISION_TABLE_SECTOR: u64 = 237;
pub const OBJECT_SERVICE_SLOT_B_COMMIT_SECTOR: u64 = 249;
pub const OBJECT_SERVICE_TORN_SECTOR: u64 = 250;
pub const OBJECT_SERVICE_OBJECT_TABLE_SECTORS: u64 = 8;
pub const OBJECT_SERVICE_RELATIONSHIP_TABLE_SECTORS: u64 = 4;
pub const OBJECT_SERVICE_REVISION_TABLE_SECTORS: u64 = 12;

#[repr(C)]
pub struct ObjectServiceCheckpointHeader {
    pub magic: [u8; 8],
    pub version: u16,
    pub slot: u16,
    pub object_count: u16,
    pub relationship_count: u16,
    pub revision_count: u16,
    pub reserved0: u16,
    pub generation: u64,
    pub object_table_sector: u64,
    pub relationship_table_sector: u64,
    pub revision_table_sector: u64,
    pub commit_sector: u64,
    pub checksum: u64,
}

#[derive(Clone, Copy)]
pub struct ObjectExtentRecord {
    pub extent_start: u64,
    pub extent_len: u16,
    pub object: TypedObjectRecord,
}

#[derive(Clone, Copy)]
pub struct WorkspaceRelationshipRecord {
    pub object_id: ObjectId,
    pub workspace_id: ObjectId,
}

pub struct ObjectServiceSnapshot {
    pub generation: u64,
    pub allocated_bitmap: u64,
    pub objects: [Option<ObjectExtentRecord>; 8],
    pub workspace_relationships: [Option<WorkspaceRelationshipRecord>; 8],
    pub current_revisions: [Option<RevisionRecord>; 4],
    pub prior_revisions: [Option<RevisionRecord>; 8],
}

impl ObjectServiceSnapshot {
    #[cfg(test)]
    pub fn contains_runtime_handle_for_test(&self, handle: PackedCapability) -> bool;
}

pub fn write_object_service_checkpoint(
    device: BlockDeviceInfo,
    snapshot: &ObjectServiceSnapshot,
) -> Result<(), GeneralStoragePersistenceError>;

pub fn read_object_service_checkpoint(
    device: BlockDeviceInfo,
) -> Result<Option<ObjectServiceSnapshot>, GeneralStoragePersistenceError>;
```

Use the repository's existing block-device surface:

```rust
block_device::read_sector(device, sector)?;
block_device::write_sector(device, sector, &bytes)?;
```

Do not introduce a `BlockDevice` trait in this slice. Unit tests for encoding
and recovery can use pure sector-array helpers; QEMU acceptance covers the real
`BlockDeviceInfo` path.

The checkpoint writer reads the current committed slot, chooses the inactive
slot, writes all object, relationship, revision, and header sectors for
`generation + 1`, writes that slot's commit marker last, verifies the written
slot, and leaves the previous committed slot intact. Recovery selects the
highest valid committed generation. The checksum covers header metadata,
object records, extent records, workspace relationship records, and revision
records. The restored dynamic store must preserve each object's allocated
extent. Keep existing Phase 10 self-tests producing their markers.

- [ ] **Step 6: Add object service tests**

Create `core/src/object_service.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_uses_workspace_and_object_capabilities_for_note_flow() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();

        let created = service.create_object(shell, workspace, ObjectKind::Note).unwrap();
        assert_eq!(created.object_id, ObjectId::new(1042));
        assert_eq!(created.revision, 1);

        service.revise_field(shell, created.object_capability, ObjectId::new(1042), 1, b"hello").unwrap();
        let inspected = service.inspect_object(shell, created.object_capability, ObjectId::new(1042)).unwrap();

        assert_eq!(inspected.revision, 2);
        assert_eq!(inspected.field_bytes(1), Some(*b"hello\0\0\0\0\0\0\0\0\0\0\0"));
    }

    #[test]
    fn known_object_id_without_object_capability_is_denied() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let outside = service.create_ungranted_note_for_test(ObjectId::new(2001), b"secret").unwrap();

        assert_eq!(outside.object_id, ObjectId::new(2001));
        assert!(service.object_exists_for_test(ObjectId::new(2001)));
        assert!(service.object_outside_shell_workspace_for_test(ObjectId::new(2001)));
        assert_eq!(
            service.inspect_object(shell, service.test_shell_workspace_capability(), ObjectId::new(2001)),
            Err(ObjectServiceError::Denied)
        );
    }

    #[test]
    fn query_returns_rebound_object_capabilities() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let created = service.create_object(shell, workspace, ObjectKind::Note).unwrap();

        let results = service.query_objects(shell, workspace, ObjectKind::Note).unwrap();

        assert_eq!(results[0].object_id, created.object_id);
        assert_eq!(service.inspect_object(shell, results[0].capability, created.object_id).unwrap().revision, 1);
    }

    #[test]
    fn snapshot_does_not_serialize_runtime_capability_handles() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let created = service.create_object(shell, workspace, ObjectKind::Note).unwrap();
        let snapshot = service.encode_snapshot_for_test().unwrap();

        assert!(!snapshot.contains_runtime_handle_for_test(created.object_capability));
    }

    #[test]
    fn restored_shell_regains_authority_by_workspace_query() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let created = service.create_object(shell, workspace, ObjectKind::Note).unwrap();
        let snapshot = service.encode_snapshot_for_test().unwrap();

        let restored = ObjectService::decode_snapshot_for_test(snapshot).unwrap();
        let restored_shell = restored.test_shell_caller();
        let restored_workspace = restored.test_shell_workspace_capability();
        let restored_entries = restored.query_objects(restored_shell, restored_workspace, ObjectKind::Note).unwrap();
        let restored_cap = restored_entries[0].capability;

        assert_eq!(restored_entries[0].object_id, created.object_id);
        assert!(restored.inspect_object(restored_shell, restored_cap, ObjectId::new(1042)).is_ok());
    }

    #[test]
    fn capability_handles_remain_holder_bound_and_revocable_within_one_boot() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let intruder = service.test_intruder_caller();
        let workspace = service.test_shell_workspace_capability();
        let created = service.create_object(shell, workspace, ObjectKind::Note).unwrap();

        assert_eq!(
            service.inspect_object(intruder, created.object_capability, ObjectId::new(1042)),
            Err(ObjectServiceError::Denied)
        );
        service.revoke_object_capability_for_test(shell, created.object_capability).unwrap();
        assert_eq!(
            service.inspect_object(shell, created.object_capability, ObjectId::new(1042)),
            Err(ObjectServiceError::Denied)
        );
    }
}
```

- [ ] **Step 7: Implement object service**

`ObjectService` must contain:

```rust
pub struct ObjectCreateResult {
    pub object_id: ObjectId,
    pub revision: u64,
    pub object_capability: PackedCapability,
}

pub struct ObjectInspection {
    pub object: TypedObjectRecord,
    pub revision: u64,
}

impl ObjectInspection {
    pub fn field_bytes(&self, field_id: u16) -> Option<[u8; 16]> {
        let mut index = 0usize;
        while index < self.object.field_count() {
            if let Some(field) = self.object.field(index)
                && field.field_id() == field_id
            {
                return Some(field.value());
            }
            index += 1;
        }
        None
    }
}

pub struct ObjectService {
    objects: DynamicObjectStore,
    relationships: RelationshipStore,
    revisions: RevisionHistory,
    capabilities: CapabilityTable,
    quotas: StorageQuotaTable,
    shell_workspace: ResourceId,
}

#[cfg(test)]
impl ObjectService {
    pub fn new_for_test() -> Self;
    pub fn test_shell_caller(&self) -> ActiveUserProcess;
    pub fn test_intruder_caller(&self) -> ActiveUserProcess;
    pub fn test_shell_workspace_capability(&self) -> PackedCapability;
    pub fn object_exists_for_test(&self, object_id: ObjectId) -> bool;
    pub fn object_outside_shell_workspace_for_test(&self, object_id: ObjectId) -> bool;
    pub fn create_ungranted_note_for_test(
        &mut self,
        object_id: ObjectId,
        text: &[u8],
    ) -> Result<ObjectCreateResult, ObjectServiceError>;
    pub fn encode_snapshot_for_test(&self) -> Result<ObjectServiceSnapshot, ObjectServiceError>;
    pub fn decode_snapshot_for_test(snapshot: ObjectServiceSnapshot) -> Result<Self, ObjectServiceError>;
    pub fn object_capability_for_test(
        &self,
        caller: ActiveUserProcess,
        object_id: ObjectId,
    ) -> Result<PackedCapability, ObjectServiceError>;
    pub fn revoke_object_capability_for_test(
        &mut self,
        caller: ActiveUserProcess,
        capability: PackedCapability,
    ) -> Result<(), ObjectServiceError>;
}
```

Rules:

```text
Create/query require workspace capability.
Inspect/revise/history require per-object capability.
Create of a note allocates through DynamicObjectStore, records revision 1 in RevisionHistory, and records `object -> BelongsTo -> SHELL_WORKSPACE_OBJECT_ID`.
Revise writes a new TypedObjectRecord field value and records a retained revision.
Query returns only objects with a durable `BelongsTo` relationship to the caller's workspace as bounded `ObjectListEntry` records containing object id plus freshly rebound object capability.
Known ungranted object 2001 exists in the normal-boot store and has `object:2001 -> BelongsTo -> EXTERNAL_WORKSPACE_OBJECT_ID`.
Persist/restore use the ADR 0052 two-slot multi-sector ObjectServiceSnapshot checkpoint and Phase 10 commit-marker semantics.
Runtime CapabilityHandle values are never serialized.
```

- [ ] **Step 8: Add retained service owner**

Create `core/src/retained_services.rs`:

```rust
pub fn initialize_object_service(service: ObjectService) -> Result<(), RetainedServiceError>;

pub fn with_object_service<R>(
    f: impl FnOnce(&mut ObjectService) -> R,
) -> Result<R, RetainedServiceError>;
```

Back it with one static `MaybeUninit<ObjectService>` plus an initialized flag.
Document the unsafe invariant at the only raw access point:

```rust
// SAFETY:
// 1. Invariant: object service storage is initialized exactly once before
//    shell launch and syscall dispatch.
// 2. Established by: normal_boot calls initialize_object_service before
//    enter_persistent_user_process, and verify boot does not use this path.
// 3. Lifetime: storage is static and lives for the whole boot.
// 4. Pointer ownership: with_object_service grants one mutable borrow for the
//    duration of one syscall dispatch closure.
// 5. Alignment: MaybeUninit<ObjectService> provides ObjectService alignment.
// 6. Mapped length: exactly one ObjectService object is accessed.
// 7. Concurrency: ADR 0051 is single-core and does not re-enter syscalls while
//    one object-service borrow is active.
// 8. Violation: concurrent access could corrupt object state or grant authority
//    to the wrong caller.
```

If the shell terminates or faults, the retained service remains initialized and
PythCore enters the normal idle loop without restarting the shell.

- [ ] **Step 9: Run focused tests**

Run:

```powershell
cargo test -p pythos-core typed_object_format dynamic_object_store object_service_checkpoint object_service retained_services
python scripts\test-persistent-storage.py
```

Expected:

```text
PERSISTENT_STORAGE_TEST_OK
```

- [ ] **Step 10: Commit**

```powershell
git add core\src\dynamic_object_store.rs core\src\object_service_checkpoint.rs core\src\retained_services.rs core\src\object_relationships.rs core\src\revision_history.rs core\src\typed_object_format.rs core\src\shell_objects.rs core\src\object_service.rs core\src\main.rs
git commit -m "feat(objects): promote Phase 10 store for normal services"
```

---

### Task 7: Caller-Derived Typed Syscalls

**Files:**
- Modify: `core/src/syscall.rs`
- Modify: `core/src/process_context.rs`
- Modify: `core/src/object_service.rs`
- Modify: `core/src/normal_boot.rs`
- Modify: `core/src/retained_services.rs`
- Test: `core/src/syscall.rs`, `core/src/object_service.rs`

**Interfaces:**
- Consumes: `ObjectShellRequest`, `ObjectShellResponse`, `ObjectService`, `ActiveUserProcess`.
- Produces: console syscalls, object request syscall, system reboot syscall, caller-derived denial marker `PYTHOS:CORE:OBJECT_SYSCALL:CALLER_DENIED`.

- [ ] **Step 1: Write syscall caller-denial tests**

In `core/src/syscall.rs`, add:

```rust
#[test]
fn object_request_denies_intruder_without_borrowing_shell_authority() {
    let mut service = ObjectService::new_for_test();
    let shell = service.test_shell_caller();
    let intruder = service.test_intruder_caller();
    let workspace = service.test_shell_workspace_capability();

    let request = ObjectShellRequest {
        abi_major: OBJECT_SHELL_ABI_MAJOR,
        abi_minor: OBJECT_SHELL_ABI_MINOR,
        operation: OP_CREATE_OBJECT,
        object_kind: OBJECT_KIND_NOTE,
        field_id: 0,
        reserved0: 0,
        authority: workspace,
        object_id: 0,
        input_ptr: 0,
        input_len: 0,
        output_ptr: 0,
        output_len: 0,
        reserved1: 0,
        reserved2: 0,
    };
    assert_eq!(dispatch_object_request_for_test(&mut service, shell, request).status, STATUS_OK);
    assert_eq!(
        dispatch_object_request_for_test(&mut service, intruder, request).status,
        STATUS_DENIED
    );
}

#[test]
fn console_write_requires_console_capability_from_current_caller() {
    let mut identities = ServiceIdentityTable::new();
    let shell_service = identities.register_task(TaskId::new(180)).unwrap();
    let intruder_service = identities.register_task(TaskId::new(181)).unwrap();
    let shell = ActiveUserProcess::new(shell_service, SHELL_PRINCIPAL_ID, 0xAA);
    let intruder = ActiveUserProcess::new(intruder_service, INTRUDER_PRINCIPAL_ID, 0xBB);
    let console = bootstrap_console_capability_for_test(shell);

    assert_eq!(dispatch_console_write_for_test(shell, console, b'X'), Ok(SYSCALL_OK));
    assert_eq!(
        dispatch_console_write_for_test(intruder, console, b'X'),
        Err(SyscallError::Capability(CapabilityError::WrongHolder))
    );
}
```

- [ ] **Step 2: Capture syscall arguments**

Change syscall entry assembly to pass `rax`, `rdi`, `rsi`, `rdx`, `r10`, and `r8` into `syscall_dispatch_abi`:

```asm
mov r9, r8
mov r8, r10
mov rcx, rdx
mov rdx, rsi
mov rsi, rdi
mov rdi, rax
call syscall_dispatch_abi
```

Update:

```rust
#[repr(C)]
pub struct SyscallArgs {
    pub number: u64,
    pub arg0: u64,
    pub arg1: u64,
    pub arg2: u64,
    pub arg3: u64,
    pub arg4: u64,
}
```

- [ ] **Step 3: Separate proof-only syscall expectation from normal syscalls**

Keep `EXPECTED_SYSCALL` around the existing Phase 8 proof syscall. Add:

```rust
fn dispatch(args: SyscallArgs) -> Result<u64, SyscallError> {
    let entry = lookup_syscall(args.number).ok_or(SyscallError::UnsupportedNumber)?;
    if entry.proof_only && !EXPECTED_SYSCALL.swap(false, Ordering::SeqCst) {
        return Err(SyscallError::UnexpectedSyscall);
    }
    dispatch_known(entry.dispatch_kind, args)
}
```

Set `proof_only: true` for `SYSCALL_SYSTEM_LOG_PROOF` and `proof_only: false` for console/object/system-control calls.

- [ ] **Step 4: Implement capability-gated console syscalls**

In `core/src/capabilities.rs`, add:

```rust
use pythos_shared::object_shell_abi::PackedCapability;

impl CapabilityHandle {
    pub const fn from_parts(slot: u32, generation: u32) -> Self {
        Self { slot, generation }
    }

    pub const fn from_packed(packed: PackedCapability) -> Self {
        Self::from_parts(packed.slot(), packed.generation())
    }

    pub const fn raw(self) -> u64 {
        u64::from(self.slot) | (u64::from(self.generation) << 32)
    }
}
```

`SYSCALL_CONSOLE_READ_BYTE` and `SYSCALL_CONSOLE_WRITE_BYTE` take `arg0` as `PackedCapability::raw()`. PythCore derives:

```rust
let caller = process_context::current_caller()?;
let handle = CapabilityHandle::from_packed(PackedCapability::from_raw(args.arg0));
console_capabilities.validate(caller.service_id(), handle, CONSOLE_COM2_RESOURCE, RightsMask::new(RightsMask::READ))?;
```

Use `RightsMask::WRITE` for write. Only after validation may PythCore read or write COM2.

- [ ] **Step 5: Implement typed object request syscall**

`SYSCALL_OBJECT_REQUEST` arguments:

```text
arg0: pointer to ObjectShellRequest in caller address space
arg1: sizeof(ObjectShellRequest)
arg2: pointer to ObjectShellResponse in caller address space
arg3: sizeof(ObjectShellResponse)
arg4: reserved zero
```

Use the existing copy-in/copy-out policy to validate the request and response buffers before raw dereference. Reject bad pointers before touching object state.

For `OP_QUERY_OBJECTS`, `request.output_ptr` and `request.output_len` describe
a caller-writable array of `ObjectListEntry`. `output_len` must be at least
`MAX_QUERY_RESULTS * size_of::<ObjectListEntry>()` or PythCore returns
`STATUS_BUFFER_TOO_SMALL`. The response `bytes_written` is the exact number of
entry bytes copied out. The shell consumes those entries to refresh its
object-capability map; PythCore does not format a human object list.

Dispatch only typed operations:

```rust
OP_CREATE_OBJECT => object_service.create_object(caller, request.authority, ObjectKind::Note)
OP_QUERY_OBJECTS => object_service.query_objects(caller, request.authority, ObjectKind::Note, output)
OP_INSPECT_OBJECT => object_service.inspect_object(caller, request.authority, ObjectId::new(request.object_id))
OP_REVISE_FIELD => object_service.revise_field(caller, request.authority, ObjectId::new(request.object_id), request.field_id, input)
OP_GET_HISTORY => object_service.history(caller, request.authority, ObjectId::new(request.object_id))
```

In normal boot, dispatch obtains the service through:

```rust
retained_services::with_object_service(|service| {
    dispatch_object_request_to_service(service, caller, request, input, output)
})?
```

Do not keep a local `ObjectService` variable in `normal_boot` after shell launch
and then separately mutate another service instance from syscalls.

PythCore must not inspect command strings such as `create kind:note`.

- [ ] **Step 6: Implement capability-gated reboot syscall**

`SYSCALL_SYSTEM_REBOOT` takes `arg0` as the caller's system-control capability. Validate `SYSTEM_CONTROL_RESOURCE` with `RightsMask::WRITE`, emit:

```text
PYTHOS:SHELL:REBOOT_REQUESTED
PYTHOS:CORE:SYSTEM:REBOOTING
```

Then call the QEMU reset helper added in Task 9.

- [ ] **Step 7: Run tests**

Run:

```powershell
cargo test -p pythos-core syscall process_context object_service user_copy
python scripts\test-boot.py
```

Expected:

```text
BOOT_TEST_OK
```

- [ ] **Step 8: Commit**

```powershell
git add core\src\syscall.rs core\src\process_context.rs core\src\object_service.rs core\src\normal_boot.rs core\src\retained_services.rs
git commit -m "feat(shell): gate typed syscalls by current caller"
```

---

### Task 8: Persistent Shell Launch And Fault Outcome

**Ordering constraint added during Task 1 review (2026-07-26):** the shell's
`UserAddressSpace` (built via `build_with_user_elf`) **must** be constructed —
and its read-only `BootstrapCapabilityBlock` mapping prepared — *before* the
normal-boot kernel address space activates, not at this task's point in the
sequence. `PageTableBuilder`'s raw-physical-address page-table writes require
the loader's broad low-memory identity map, which
`KernelAddressSpace::activate()` removes by design (the Phase 1.5
identity-map-removal invariant, `core/src/normal_init.rs`). Building it here —
long after Task 1's kernel activation — reproduces the exact page fault Task 1
hit and fixed for the proof-only `UserAddressSpace::build()` call. When
implementing this task, move shell address-space construction into the same
early phase as `normal_init::initialize_normal_substrate` (alongside the
kernel and proof user address spaces), retain the built (but not yet activated)
shell `UserAddressSpace` through to this task's launch step, and activate it
only when actually entering ring 3.

**Files:**
- Modify: `core/src/user_mode.rs`
- Modify: `core/src/normal_boot.rs`
- Modify: `core/src/user_elf.rs`
- Modify: `core/src/runtime_loader.rs`
- Test: `core/src/user_mode.rs`, `scripts/test-object-shell.py`

**Interfaces:**
- Consumes: named shell manifest, retained `ObjectService`, `ActiveUserProcess`, COM2, typed syscalls.
- Produces: marker `PYTHOS:SHELL:RING3_ENTER`, marker `PYTHOS:SHELL:FAULT_TERMINATED`, `BootstrapCapabilityBlock`, `user_mode::enter_persistent_user_process(process, entry, user_stack_top, bootstrap_user_ptr) -> !`.

- [ ] **Step 1: Write persistent fault policy test**

In `core/src/user_mode.rs`, add a pure policy test:

```rust
#[test]
fn persistent_user_fault_terminates_faulting_service() {
    let mut identities = ServiceIdentityTable::new();
    let shell_service = identities.register_task(TaskId::new(180)).unwrap();
    let peer_service = identities.register_task(TaskId::new(182)).unwrap();
    let shell = ActiveUserProcess::new(shell_service, SHELL_PRINCIPAL_ID, 0xAA);
    let peer = ActiveUserProcess::new(peer_service, 0x5059_5045_4552_0001, 0xCC);
    let outcome = classify_persistent_user_fault_for_test(shell, peer);

    assert_eq!(outcome.terminated_principal, SHELL_PRINCIPAL_ID);
    assert!(outcome.peer_alive);
}
```

- [ ] **Step 2: Add persistent ring-3 entry**

In `core/src/user_mode.rs`, add:

```rust
#[cfg(not(test))]
pub fn enter_persistent_user_process(
    process: ActiveUserProcess,
    entry: u64,
    user_stack_top: u64,
    bootstrap_user_ptr: u64,
) -> ! {
    process_context::bind_current_process(process);
    tss::set_ring0_stack(kernel_trap_stack_top());
    serial::write_line("PYTHOS:SHELL:RING3_ENTER");
    unsafe {
        ring3_enter_forever_abi(entry, user_stack_top, bootstrap_user_ptr);
    }
    loop {
        core::hint::spin_loop();
    }
}
```

Document the unsafe invariant before `ring3_enter_forever_abi`, including that
`entry` is the validated user ELF entry point, `user_stack_top` is a mapped
guarded user stack, and `bootstrap_user_ptr` is a user-readable,
kernel-owned/read-only mapping containing a valid `BootstrapCapabilityBlock`.

- [ ] **Step 3: Define fault handling outcome**

For a persistent shell fault, emit:

```text
PYTHOS:CORE:CRASH:USER_FAULT
PYTHOS:SHELL:FAULT_TERMINATED
PYTHOS:CORE:CRASH:PEER_ALIVE
```

Terminate only the faulting user process and leave PythCore alive. The peer is
the normal supervisor process record created before shell launch with principal
`NORMAL_SUPERVISOR_PRINCIPAL_ID`; it remains schedulable/alive after shell
termination. PythCore then clears the active process context and enters the
normal idle loop with retained services still initialized. Do not use the
controlled breakpoint recovery path from the proof-only user-mode tests as the
persistent-shell success path.

- [ ] **Step 4: Launch shell in normal boot**

In `core/src/normal_boot.rs`, after object service initialization:

```rust
let shell_program = runtime_loader::load_named_user_program(boot_info, b"shell.elf")?;
let shell_elf = user_elf::validate(shell_program.elf())?;
let shell_process = process_context::ActiveUserProcess::from_manifest(shell_service, shell_program);
let launch = build_user_elf_address_space_from_image(physical_memory, boot_info, shell_program.elf())?;
let bootstrap = retained_services::with_object_service(|service| {
    build_shell_bootstrap_block(shell_process, service)
})??;
let bootstrap_user_ptr = map_read_only_bootstrap_block(&mut launch.address_space, &bootstrap)?;
user_mode::enter_persistent_user_process(
    shell_process,
    shell_elf.entry(),
    launch.user_stack_top(),
    bootstrap_user_ptr,
);
```

`build_shell_bootstrap_block` grants console, workspace, and system-control
capabilities to the shell process. It includes per-object entries only for
objects reachable through the shell workspace policy. The mapped page is
read-only in the shell address space and kernel-owned in PythCore. The shell's
`syscalls::bootstrap_capabilities(bootstrap_ptr)` reads this block; it must not
synthesize capabilities from constants.

- [ ] **Step 5: Run focused tests**

Run:

```powershell
cargo test -p pythos-core user_mode runtime_loader user_elf process_context
python scripts\test-object-shell.py
```

Expected before Task 10: FAIL after `PYTHOS:SHELL:READY` if object responses are not fully wired. COM1 must contain `PYTHOS:SHELL:RING3_ENTER`.

- [ ] **Step 6: Commit**

```powershell
git add core\src\user_mode.rs core\src\normal_boot.rs core\src\user_elf.rs core\src\runtime_loader.rs
git commit -m "feat(shell): launch persistent shell process"
```

---

### Task 9: System Reboot And Acceptance Harness

**Files:**
- Modify: `core/src/qemu_exit.rs`
- Modify: `core/src/syscall.rs`
- Create: `scripts/test-object-shell.py`
- Test: `scripts/test-object-shell.py`

**Interfaces:**
- Consumes: `SYSCALL_SYSTEM_REBOOT`, COM2 transport.
- Produces: `system_reboot_qemu() -> !`, forced power-loss test path, actual reboot test path.

- [ ] **Step 1: Add QEMU reset helper**

In `core/src/qemu_exit.rs`, add:

```rust
pub fn reboot_qemu() -> ! {
    // SAFETY:
    // 1. Invariant: writing 0xFE to port 0x64 requests a reset on the ADR 0052
    //    QEMU q35/i8042 target.
    // 2. Established by: ADR 0052 limits this reboot mechanism to the current
    //    QEMU profile and system-control syscall validates authority first.
    // 3. Lifetime: the instruction has no borrowed-memory lifetime.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable to port I/O.
    // 7. Concurrency: ADR 0051 remains single-core.
    // 8. Violation: unsupported hardware may ignore the request and spin.
    unsafe {
        asm!("out dx, al", in("dx") 0x64u16, in("al") 0xFEu8, options(nomem, nostack, preserves_flags));
    }
    loop {
        core::hint::spin_loop();
    }
}
```

- [ ] **Step 2: Write object-shell acceptance test**

Create `scripts/test-object-shell.py` with two phases:

```python
#!/usr/bin/env python
"""End-to-end ADR 0051 object shell test."""

from __future__ import annotations

import socket
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "object-shell-com1.log"
TRANSCRIPT = TARGET / "object-shell-com2.log"
STORAGE = TARGET / "object-shell-store.img"
SHELL_PORT = 4582


@dataclass
class Result:
    com1: str
    com2: str


def run(command: list[str]) -> None:
    result = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout)
    if result.returncode != 0:
        raise AssertionError(f"{command} failed with {result.returncode}")


def wait_for_socket_text(sock: socket.socket, marker: str, timeout: float = 20) -> str:
    deadline = time.monotonic() + timeout
    text = ""
    while time.monotonic() < deadline:
        try:
            data = sock.recv(4096)
        except socket.timeout:
            continue
        if data:
            text += data.decode("utf-8", errors="replace")
            if marker in text:
                return text
    raise AssertionError(f"missing COM2 marker {marker}; transcript={text!r}")


def wait_for_file_marker(path: Path, marker: str, timeout: float = 20) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            text = path.read_text(encoding="utf-8", errors="replace")
            if marker in text:
                return text
        time.sleep(0.1)
    raise AssertionError(f"missing COM1 marker {marker}")


def wait_for_file_marker_count(path: Path, marker: str, count: int, timeout: float = 30) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            text = path.read_text(encoding="utf-8", errors="replace")
            if text.count(marker) >= count:
                return text
        time.sleep(0.1)
    raise AssertionError(f"missing {count} occurrences of COM1 marker {marker}")


def start_qemu() -> subprocess.Popen[str]:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    return subprocess.Popen(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--shell-port",
            str(SHELL_PORT),
            "--storage-image",
            str(STORAGE),
            "--timeout",
            "90",
            "--expect-outcome",
            "timeout",
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )


def connect_shell() -> socket.socket:
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        try:
            sock = socket.create_connection(("127.0.0.1", SHELL_PORT), timeout=1)
            sock.settimeout(1)
            return sock
        except OSError:
            time.sleep(0.2)
    raise AssertionError("COM2 shell socket never became reachable")


def stop_qemu(process: subprocess.Popen[str]) -> None:
    process.terminate()
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=3)
    if process.stdout is not None:
        print(process.stdout.read())


def session(commands: list[str], verify_actual_reboot: bool) -> Result:
    process = start_qemu()
    transcript = ""
    reboot_requested = False
    try:
        wait_for_file_marker(SERIAL_LOG, "PYTHOS:SHELL:RING3_ENTER", 30)
        with connect_shell() as sock:
            transcript += wait_for_socket_text(sock, "PYTHOS:SHELL:READY")
            transcript += wait_for_socket_text(sock, "pyth> ")
            for command in commands:
                sock.sendall((command + "\n").encode("ascii"))
                marker = "REBOOTING" if command == "reboot" else "pyth> "
                transcript += wait_for_socket_text(sock, marker)
                if command == "reboot" and verify_actual_reboot:
                    reboot_requested = True
                    break
        if reboot_requested:
            wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:SYSTEM:REBOOTING", 10)
            wait_for_file_marker_count(SERIAL_LOG, "PYTHOS:LOADER:ENTER", 2, 30)
            with connect_shell() as rebooted:
                transcript += wait_for_socket_text(rebooted, "PYTHOS:SHELL:READY", 30)
                transcript += wait_for_socket_text(rebooted, "pyth> ", 30)
        return Result(SERIAL_LOG.read_text(encoding="utf-8", errors="replace"), transcript)
    finally:
        stop_qemu(process)


def main() -> int:
    run([sys.executable, "scripts/build-user-shell.py"])
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none"])
    run([sys.executable, "scripts/build-image.py"])
    STORAGE.parent.mkdir(parents=True, exist_ok=True)
    STORAGE.write_bytes(b"\0" * (16 * 1024 * 1024))

    first = session(["create kind:note", 'revise object:1042 text="hello"', "inspect object:2001"], False)
    assert "CREATED object:1042 revision:1" in first.com2
    assert "COMMITTED revision:2" in first.com2
    assert "DENIED missing-capability" in first.com2
    assert "PYTHOS:CORE:OBJECT_SERVICE:EXTERNAL_OBJECT_READY object:2001" in first.com1
    assert "PYTHOS:CORE:OBJECT_SERVICE:OUTSIDE_WORKSPACE object:2001" in first.com1
    assert "PYTHOS:CORE:OBJECT_SERVICE:CAPABILITY_DENIED object:2001" in first.com1
    assert "PYTHOS:CORE:OBJECT_SYSCALL:CALLER_DENIED" not in first.com1

    restored = session(["query kind:note", "inspect object:1042", "history object:1042", "reboot"], True)
    assert "object:1042 kind:note" in restored.com2
    assert 'text="hello" revision:2' in restored.com2
    assert "history object:1042 revisions:2" in restored.com2
    assert "PYTHOS:SHELL:IDENTITY_RESTORED" in restored.com1
    assert "PYTHOS:CORE:REBOOT:USER_PROCESS_STATE_CLEARED" in restored.com1
    assert "PYTHOS:CORE:REBOOT:USER_ADDRESS_SPACES_REBUILT" in restored.com1
    assert "PYTHOS:SHELL:WORKSPACE_CAPABILITY_REBOUND" in restored.com1
    assert "PYTHOS:CORE:SYSTEM:REBOOTING" in restored.com1
    assert restored.com1.count("PYTHOS:LOADER:ENTER") >= 2
    assert restored.com2.count("PYTHOS:SHELL:READY") >= 2

    TRANSCRIPT.write_text(first.com2 + restored.com2, encoding="utf-8", errors="replace")
    print("OBJECT_SHELL_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 3: Run the failing acceptance test**

Run:

```powershell
python scripts\test-object-shell.py
```

Expected: FAIL until Task 10 completes response text, persistence, and actual reboot behavior.

- [ ] **Step 4: Commit**

```powershell
git add core\src\qemu_exit.rs core\src\syscall.rs scripts\test-object-shell.py
git commit -m "test(shell): add object shell reboot acceptance"
```

---

### Task 10: End-To-End Typed Object Shell Flow

**Files:**
- Modify: `core/src/object_service.rs`
- Modify: `core/src/syscall.rs`
- Modify: `user/shell/src/main.rs`
- Modify: `user/shell/src/commands.rs`
- Modify: `user/shell/src/syscalls.rs`
- Test: `scripts/test-object-shell.py`, `core/src/object_service.rs`, `user/shell/src/commands.rs`

**Interfaces:**
- Consumes: typed object syscall and retained object service.
- Produces: exact shell transcript for create, revise, denied known object, inspect, and history.

- [ ] **Step 1: Ensure shell formats all responses in user space**

In `user/shell/src/syscalls.rs`, convert `ObjectShellResponse` statuses to text:

```text
STATUS_OK + OP_CREATE_OBJECT -> CREATED object:<id> revision:<revision>
STATUS_OK + OP_QUERY_OBJECTS -> object:<id> kind:note
STATUS_OK + OP_REVISE_FIELD -> COMMITTED revision:<revision>
STATUS_OK + OP_INSPECT_OBJECT -> text="<text>" revision:<revision>
STATUS_OK + OP_GET_HISTORY -> history object:<id> revisions:<count>
STATUS_DENIED -> DENIED missing-capability
STATUS_NOT_FOUND -> DENIED missing-capability
STATUS_BAD_REQUEST -> ERROR bad-request
STATUS_BUFFER_TOO_SMALL -> ERROR buffer-too-small
```

PythCore returns typed fields only; it does not format these strings. For
`OP_QUERY_OBJECTS`, `syscalls.rs` reads `ObjectListEntry` records from the
query output buffer, prints object ids/kinds, and stores each returned
capability in `CapabilityMap`.

- [ ] **Step 2: Finish typed syscall execution**

In `core/src/syscall.rs`, make `SYSCALL_OBJECT_REQUEST` copy in `ObjectShellRequest`, call `object_service`, copy out `ObjectShellResponse`, and return `SYSCALL_OK` for handled request statuses. A denied object operation is a successful syscall with `response.status = STATUS_DENIED`.

- [ ] **Step 3: Finish object service persistence hooks**

Persist after `create_object` and `revise_field`. During normal service
initialization, seed known external object `2001`, prove it exists outside the
shell workspace, and emit:

```text
PYTHOS:CORE:OBJECT_SERVICE:EXTERNAL_OBJECT_READY object:2001
PYTHOS:CORE:OBJECT_SERVICE:OUTSIDE_WORKSPACE object:2001
```

When an inspect/revise/history request for that object reaches capability
validation and fails, emit:

```text
PYTHOS:CORE:OBJECT_SERVICE:CAPABILITY_DENIED object:2001
```

On restore, emit:

```text
PYTHOS:SHELL:IDENTITY_RESTORED
PYTHOS:CORE:REBOOT:USER_PROCESS_STATE_CLEARED
PYTHOS:CORE:REBOOT:USER_ADDRESS_SPACES_REBUILT
PYTHOS:SHELL:WORKSPACE_CAPABILITY_REBOUND
PYTHOS:SHELL:OBJECT_RESTORED
```

On first boot with no object snapshot, emit:

```text
PYTHOS:SHELL:IDENTITY_BOOTSTRAPPED
PYTHOS:SHELL:WORKSPACE_CAPABILITY_GRANTED
```

- [ ] **Step 4: Run object shell acceptance**

Run:

```powershell
python scripts\test-object-shell.py
```

Expected:

```text
OBJECT_SHELL_TEST_OK
```

- [ ] **Step 5: Run preserved verification suites**

Run:

```powershell
python scripts\test-boot.py
python scripts\test-persistent-storage.py
python scripts\test-normal-fast-boot.py
```

Expected:

```text
BOOT_TEST_OK
PERSISTENT_STORAGE_TEST_OK
NORMAL_FAST_BOOT_TEST_OK
```

- [ ] **Step 6: Commit**

```powershell
git add core\src\object_service.rs core\src\syscall.rs user\shell\src scripts\test-object-shell.py
git commit -m "feat(shell): complete typed object shell flow"
```

---

### Task 11: Adversarial Caller And Fault Tests

**Files:**
- Modify: `scripts/test-object-shell.py`
- Modify: `scripts/build-image.py`
- Modify: `scripts/build-iso.py`
- Modify: `Cargo.toml`
- Modify: `core/src/normal_boot.rs`
- Modify: `core/src/syscall.rs`
- Create: `user/probes/intruder/Cargo.toml`
- Create: `user/probes/intruder/src/main.rs`
- Create: `user/probes/fault-shell/Cargo.toml`
- Create: `user/probes/fault-shell/src/main.rs`
- Create: `scripts/build-user-probes.py`
- Test: `scripts/test-object-shell.py`

**Interfaces:**
- Consumes: named intruder program, current caller context, typed object syscall.
- Produces: marker `PYTHOS:CORE:OBJECT_SYSCALL:CALLER_DENIED`, marker `PYTHOS:SHELL:FAULT_TERMINATED`.

- [ ] **Step 1: Add intruder execution to test harness**

Extend `scripts/test-object-shell.py` with a control-sector writer:

```python
SECTOR_SIZE = 512
SHELL_CONTROL_SECTOR = 95
SHELL_CONTROL_MAGIC = b"PYSHCTL1"
CONTROL_RUN_INTRUDER = 1
CONTROL_RUN_FAULT_SHELL = 2


def write_shell_control(image: Path, mode: int) -> None:
    sector = bytearray(SECTOR_SIZE)
    sector[0:8] = SHELL_CONTROL_MAGIC
    sector[8:10] = mode.to_bytes(2, "little")
    with image.open("r+b") as handle:
        handle.seek(SHELL_CONTROL_SECTOR * SECTOR_SIZE)
        handle.write(sector)
```

In `core/src/normal_boot.rs`, read sector 95 before launching `shell.elf`. This
sector is outside the ADR 0052 checkpoint slots at sectors 192-250 and before
the dynamic object extent base at sector 96. Mode `1` launches `intruder.elf`,
makes the same
`SYSCALL_OBJECT_REQUEST` number with the shell workspace handle value, and
asserts denial. Clear sector 95 after reading the mode so the next boot returns
to the normal shell path.

- [ ] **Step 2: Add intruder marker assertions**

The harness must require:

```text
PYTHOS:CORE:OBJECT_SYSCALL:CALLER_DENIED
PYTHOS:CORE:DYNAMIC_CAPABILITY:NO_GRANT_DENIED
```

The harness must reject:

```text
PYTHOS:SHELL:OBJECT_CREATED_BY_INTRUDER
```

- [ ] **Step 3: Add concrete adversarial probe ELFs**

Create `user/probes/intruder/src/main.rs` as a `no_std`, `no_main` program that
imports `pythos_shared::object_shell_abi`, builds an `ObjectShellRequest` for
`OP_CREATE_OBJECT`, intentionally places the shell workspace raw handle value
from its argv/env test input into `request.authority`, invokes
`SYSCALL_OBJECT_REQUEST` with documented unsafe syscall assembly, and spins
after the syscall returns. It must not share shell code or a bootstrap block.

Add both probe crates to the root workspace, or build them via
`cargo build --manifest-path user/probes/intruder/Cargo.toml` and
`cargo build --manifest-path user/probes/fault-shell/Cargo.toml` with explicit
workspace exclusion. Do not embed synthetic byte arrays in
`scripts/build-image.py`.

Create `user/probes/fault-shell/src/main.rs`:

```rust
#![no_std]
#![no_main]

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // SAFETY:
    // 1. Invariant: `ud2` deliberately raises an invalid-opcode user fault for
    //    the ADR 0051 persistent-shell fault acceptance path.
    // 2. Established by: this program is packaged only as `fault-shell.elf` and
    //    launched only when the harness writes CONTROL_RUN_FAULT_SHELL.
    // 3. Lifetime: no borrowed memory is involved.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable.
    // 6. Mapped length: not applicable.
    // 7. Concurrency: ADR 0051 is single-core.
    // 8. Violation: if launched accidentally it terminates only its user process.
    unsafe {
        core::arch::asm!("ud2", options(nomem, nostack));
    }
    loop {
        core::hint::spin_loop();
    }
}
```

Create `scripts/build-user-probes.py` to build both probe crates with the same
user-ELF linker policy as `scripts/build-user-shell.py`. Package only the
intruder ELF into the normal object-shell image:

```python
(INIT_BUNDLE_NAMED_USER_ELF_TYPE, build_named_user_program(b"intruder.elf", INTRUDER_PRINCIPAL_ID, INTRUDER_ELF.read_bytes()))
```

For the fault acceptance run,
`scripts/build-image.py --shell-elf target/x86_64-unknown-none/debug/pythos-fault-shell`
builds a separate test image that packages the fault body under the unique
trusted name `shell.elf` with `SHELL_PRINCIPAL_ID`. Do not package both the real
shell and a second fault program with the shell principal into the same bundle.

- [ ] **Step 4: Add persistent shell fault probe**

Mode `2` boots the fault-test image where `shell.elf` contains the
invalid-instruction body, so the loader-validated shell principal is still
bound through the normal `shell.elf` name. It asserts:

```text
PYTHOS:CORE:CRASH:USER_FAULT
PYTHOS:SHELL:FAULT_TERMINATED
PYTHOS:CORE:CRASH:PEER_ALIVE
```

- [ ] **Step 5: Run adversarial acceptance**

Run:

```powershell
python scripts\build-user-probes.py
python scripts\test-object-shell.py
python scripts\test-boot.py
```

Expected:

```text
OBJECT_SHELL_TEST_OK
BOOT_TEST_OK
```

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml scripts\test-object-shell.py scripts\build-image.py scripts\build-iso.py scripts\build-user-probes.py user\probes core\src\normal_boot.rs core\src\syscall.rs
git commit -m "test(shell): prove object shell caller isolation"
```

---

### Task 12: Documentation And Final Verification

**Files:**
- Modify: `docs/ROADMAP.md`
- Modify: `docs/HANDOVER.md`
- Test: `scripts/test-object-shell.py`, `scripts/test-normal-fast-boot.py`, `scripts/test-boot.py`, `scripts/test-persistent-storage.py`

**Interfaces:**
- Consumes: completed ADR 0051 and ADR 0052 implementation.
- Produces: updated roadmap/handover stating verified behavior and remaining boundaries.

- [ ] **Step 1: Update roadmap**

In `docs/ROADMAP.md`, record:

```text
ADR 0051 first-ring3-object-shell launches a validated `shell.elf` as a
ring-3 process during normal boot. The shell parses human command text in user
space, submits typed object requests through ADR 0052, and receives only
capability-gated responses. COM1 remains the oracle; COM2 carries the shell
session. The object flow uses the retained Phase 10 object path and proves
create, revise, denied known-object access, restore after forced power loss,
and capability-gated reboot.
```

- [ ] **Step 2: Update handover**

In `docs/HANDOVER.md`, add:

```text
Current boundary: ADR 0051 and ADR 0052 complete. Verification:
`python scripts\test-object-shell.py`,
`python scripts\test-normal-fast-boot.py`,
`python scripts\test-boot.py`, and
`python scripts\test-persistent-storage.py`.

Do not start package management, networking, universal-device work, human
`grant`, packaged `launch`, or AI agent runtime without a new design artifact.
The typed object bridge remains a temporary kernel-backed adapter until the
object service is moved out of PythCore.
```

- [ ] **Step 3: Run final verification**

Run:

```powershell
python scripts\test-object-shell.py
python scripts\test-normal-fast-boot.py
python scripts\test-boot.py
python scripts\test-persistent-storage.py
```

Expected:

```text
OBJECT_SHELL_TEST_OK
NORMAL_FAST_BOOT_TEST_OK
BOOT_TEST_OK
PERSISTENT_STORAGE_TEST_OK
```

- [ ] **Step 4: Commit**

```powershell
git add docs\ROADMAP.md docs\HANDOVER.md
git commit -m "docs(shell): record ADR 0051 object shell completion"
```

---

## Self-Review

- The plan no longer creates a kernel REPL: command parsing and presentation live in `user/shell`.
- PythCore receives typed ABI requests only.
- Console, object, and reboot operations validate the current syscall caller and caller-supplied capabilities.
- A second ring-3 principal using the same syscall number is denied.
- The object path promotes the existing Phase 10 dynamic object store, typed object records, revision history, storage quotas, and persistence helpers.
- No shell-private one-note sector store is introduced.
- Program authority is tied to validated named manifest identity and digest, not to a constant task slot.
- Missing authority is tested against known object `2001`, not against a missing object id.
- Normal boot is split from verification boot before shell launch.
- Forced power loss and actual shell-requested reboot are distinct acceptance paths.
- COM2 initialization is explicit and separately tested.
- User shell unsafe syscall assembly includes the required invariant.
- Persistent shell faults terminate only the faulting service and leave PythCore alive.
- Named ELF records are versioned and existing ordinal `TYPE_USER_ELF` records remain compatible.
- `readelf` verification covers ET_EXEC and writable-executable segment rejection.
- The shell busy-poll loop is recorded as temporary ADR 0051 behavior.
