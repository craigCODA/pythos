# PythOS New-Session Handover

This is the first file to read when starting a new PythOS agent session in this
checkout. It is deliberately operational rather than promotional. It records
the live branch state, verified boot evidence, implemented slices, important
files, accepted ADRs, current stop point, and the exact next slice boundary.

Do not treat this file as a substitute for live verification. Use it to orient
quickly, then check the current working tree and serial evidence before making
new claims.

## Start Here In A New Session

Run from:

```powershell
cd C:\Users\NeverAMoment\pythos
```

Required first reads before editing:

```text
AGENTS.md
docs/PythOS-SAS-001.md
docs/PythOS-TDD-001.md
docs/ROADMAP.md
docs/HANDOVER.md
```

Required first live-state checks:

```powershell
git status --short --branch
git log --oneline --decorate -10
Select-String -Path target\boot-serial.log -Pattern "PYTHOS:|QEMU_OUTCOME|BOOT_TEST"
```

Expected state at the time this handover was written:

```text
branch   milestone/phase4-runtime-selection
tracking origin/milestone/phase4-runtime-selection
HEAD     a71ac3d test: extend qemu acceptance timeout
parent   cd4b887 core: deliver runtime service events
status   clean before this handover edit
```

If the branch, HEAD, or worktree differs, trust the live repository over this
file and update the handover before continuing.

## Current Stop Point

PythOS is inside Phase 5. The `keyboard-driver` / `mouse-driver`,
`input-event-service`, `software-renderer`, `font-system`, and `compositor` /
`surfaces` / `clipping` slices are complete; `pointer-cursor` /
`window-focus` / `movable-windows` is the next active slice.

Completed:

```text
Phase 0    reproducible environment
Phase 1    boot core handoff
Phase 1.5  kernel-owned execution substrate
Phase 2    timer and native tasks
Phase 3    IPC and capabilities
Phase 4    runtime-selection
Phase 4    init-pak-loading
Phase 4    interpreter-boot
Phase 4    system-api-surface
Phase 4    value-validation
Phase 4    service-manager
Phase 4    exception-containment
Phase 4    service-restart
Phase 4    async-events
Phase 5    keyboard-driver / mouse-driver
Phase 5    input-event-service
Phase 5    software-renderer
Phase 5    font-system
Phase 5    compositor / surfaces / clipping
```

Next slice:

```text
Phase 5: pointer-cursor / window-focus / movable-windows
```

Do not begin any of the following without explicit re-invocation and roadmap
alignment:

```text
widgets
shell applications
audio
storage
networking
AI
ring-3 isolation
SMP
```

The active Phase 4 sequence is:

```text
runtime-selection        complete
init-pak-loading         complete
interpreter-boot         complete
system-api-surface       complete
value-validation         complete
service-manager          complete
exception-containment    complete
service-restart          complete
async-events             complete
```

## New Agent Instruction Block

Paste this at the start of a fresh agent session if needed:

```text
You are working in C:\Users\NeverAMoment\pythos.

Read AGENTS.md, docs/PythOS-SAS-001.md, docs/PythOS-TDD-001.md,
docs/ROADMAP.md, and docs/HANDOVER.md before editing.

Use the live git tree as the source of truth. First run:

  git status --short --branch
  git log --oneline --decorate -10

The expected current branch is milestone/phase4-runtime-selection, with
origin tracking the same branch. Verify the latest commit with `git log`
instead of trusting this text.

Phase 4 is complete. Phase 5 has completed input drivers, input event
normalization, the software renderer, the font system, and the compositor /
surfaces / clipping proof. The next allowed work is Phase 5 `pointer-cursor` /
`window-focus` / `movable-windows`. Do not start Phase 6, audio, storage,
networking, AI, ring-3, or SMP work.

Serial output is the boot oracle. A compile is not proof. A screenshot is not
proof. Any slice must be enforced by scripts/test-boot.py or an equivalent
automated QEMU acceptance test.

Before claiming anything is complete, run the relevant test command and read
the output. Do not trust stale handover text over target/boot-serial.log.
```

## Last Verified Serial Sequence

The latest `target\boot-serial.log` observed during this handover reaches the
following ordered markers:

```text
PYTHOS:LOADER:ENTER
PYTHOS:LOADER:GOP_READY
PYTHOS:LOADER:KERNEL_LOADED
PYTHOS:LOADER:MEMORY_MAP_READY
PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK
PYTHOS:CORE:ENTER
PYTHOS:CORE:BOOTINFO_VALID
PYTHOS:CORE:MEMORY_READY
PYTHOS:CORE:GDT_READY
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY
PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED
PYTHOS:CORE:INTERRUPTS_READY
PYTHOS:CORE:VM_READY
PYTHOS:CORE:EXPECTED_PAGE_FAULT
PYTHOS:CORE:IDENTITY_MAP_REMOVED
PYTHOS:CORE:BOOTINFO_COMPLETE
PYTHOS:CORE:TIMER_READY
PYTHOS:CORE:CLOCK_READY
PYTHOS:CORE:TASKS_READY
PYTHOS:CORE:EXPECTED_PAGE_FAULT
PYTHOS:CORE:KERNEL_STACKS_READY
PYTHOS:CORE:CONTEXT_SWITCH:TASK_A
PYTHOS:CORE:CONTEXT_SWITCH:TASK_B
PYTHOS:CORE:CONTEXT_SWITCH:TASK_A
PYTHOS:CORE:CONTEXT_SWITCH:TASK_B
PYTHOS:CORE:CONTEXT_SWITCH_READY
PYTHOS:CORE:SCHEDULER:TASK_A
PYTHOS:CORE:SCHEDULER:TASK_B
PYTHOS:CORE:SCHEDULER:TASK_A
PYTHOS:CORE:SCHEDULER:TASK_B
PYTHOS:CORE:SCHEDULER_READY
PYTHOS:CORE:IDLE_TASK
PYTHOS:CORE:IDLE_TASK_READY
PYTHOS:CORE:PREEMPT:TASK_A
PYTHOS:CORE:PREEMPT:TASK_B
PYTHOS:CORE:PREEMPT:TASK_A
PYTHOS:CORE:PREEMPT:TASK_B
PYTHOS:CORE:PREEMPT_READY
PYTHOS:CORE:TASK_TERMINATED
PYTHOS:CORE:TASK_TERMINATION_READY
PYTHOS:CORE:SCHEDTEST:TASK_A
PYTHOS:CORE:SCHEDTEST:TASK_B
PYTHOS:CORE:SCHEDTEST:TASK_C
PYTHOS:CORE:SCHEDTEST:TASK_A
PYTHOS:CORE:SCHEDTEST:TASK_B
PYTHOS:CORE:SCHEDTEST:TASK_C
PYTHOS:CORE:SCHEDULER_TESTS_READY
PYTHOS:CORE:SERVICE_IDENTITY_READY
PYTHOS:CORE:IPC:SEND
PYTHOS:CORE:IPC:RECV
PYTHOS:CORE:IPC_CHANNELS_READY
PYTHOS:CORE:IPC:QUEUE_FULL
PYTHOS:CORE:BOUNDED_QUEUES_READY
PYTHOS:CORE:IPC:REQUEST
PYTHOS:CORE:IPC:REPLY
PYTHOS:CORE:IPC:REPLY_TIMEOUT
PYTHOS:CORE:REQUEST_REPLY_READY
PYTHOS:CORE:CAPABILITY:GRANT
PYTHOS:CORE:CAPABILITY:USE
PYTHOS:CORE:CAPABILITY_HANDLES_READY
PYTHOS:CORE:SHM:READ_ONLY
PYTHOS:CORE:SHM:WRITE_DENIED
PYTHOS:CORE:SHARED_MEMORY_HANDLES_READY
PYTHOS:CORE:PERMISSION:IPC_ALLOWED
PYTHOS:CORE:PERMISSION:IPC_DENIED
PYTHOS:CORE:PERMISSION_VALIDATION_READY
PYTHOS:CORE:CAPABILITY:REVOKE
PYTHOS:CORE:CAPABILITY:STALE_DENIED
PYTHOS:CORE:REVOCATION_READY
PYTHOS:CORE:CAPABILITY:KNOWN_TARGET_DENIED
PYTHOS:CORE:NEGATIVE_AUTHORIZATION_READY
PYTHOS:CORE:AUDIT:GRANT
PYTHOS:CORE:AUDIT:USE
PYTHOS:CORE:AUDIT:DENIAL
PYTHOS:CORE:AUDIT:REVOCATION
PYTHOS:CORE:AUDIT_LOGGING_READY
PYTHOS:CORE:PHASE_3_COMPLETE
PYTHOS:CORE:RUNTIME_SELECTED
PYTHOS:CORE:INIT_PAK_LOADED
PYTHOS:CORE:INTERPRETER_BOOTED
PYTHOS:CORE:SYSTEM:LOG
PYTHOS:CORE:SYSTEM_API_READY
PYTHOS:CORE:VALUE_VALIDATION_READY
PYTHOS:CORE:SERVICE:READY
PYTHOS:CORE:SERVICE_MANAGER_READY
PYTHOS:CORE:SERVICE:EXCEPTION
PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED
PYTHOS:CORE:SERVICE:RESTART
PYTHOS:CORE:SERVICE_RESTART_READY
PYTHOS:CORE:SERVICE:EVENT
PYTHOS:CORE:ASYNC_EVENTS_READY
PYTHOS:CORE:INPUT:KEYBOARD
PYTHOS:CORE:INPUT:MOUSE
PYTHOS:CORE:INPUT_DRIVERS_READY
PYTHOS:CORE:INPUT:EVENT
PYTHOS:CORE:INPUT_EVENT_SERVICE_READY
PYTHOS:CORE:RENDER:RECT
PYTHOS:CORE:SOFTWARE_RENDERER_READY
PYTHOS:CORE:FONT:PSF_LOADED
PYTHOS:CORE:FONT_SYSTEM_READY
PYTHOS:CORE:COMPOSITOR:SURFACE
PYTHOS:CORE:COMPOSITOR:CLIP
PYTHOS:CORE:COMPOSITOR_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

Two `PYTHOS:CORE:EXPECTED_PAGE_FAULT` markers are intentional:

```text
first  -> identity-map-removed negative proof
second -> kernel stack guard-page proof
```

The final successful QEMU runs also reported:

```text
QEMU_OUTCOME success
BOOT_TEST_OK
```

## Last Targeted Verification Set

These commands passed immediately before the compositor implementation commit:

```powershell
cargo fmt --check
cargo test -p pythos-core
cargo clippy -p pythos-core --target x86_64-unknown-none -- -D warnings
python -m unittest tests.test_boot_marker_contract
python scripts\test-boot.py --slice compositor
git diff --check
```

Important notes:

* `python -m unittest tests.boot_core_handoff` is long because it repeatedly
  boots QEMU for each slice. Expect several minutes.
* Do not run multiple QEMU boot tests in parallel; the harness uses fixed QMP
  and runtime resources.
* `make test-boot` is not the source of truth on this Windows host unless
  `make` is intentionally installed and wired to the same Python commands.
* `target\pythos.iso` is the current ISO artifact path.
* `target\boot-serial.log` is overwritten by QEMU test runs.

## Latest Commits To Know

Current recent history:

```text
a71ac3d test: extend qemu acceptance timeout
cd4b887 core: deliver runtime service events
3677c04 core: restart failed runtime services
eb33aba core: contain runtime service exceptions
96782a7 core: add phase 4 service manager
782725e core: update runtime proof log message
68a04c7 docs: refresh value validation handover
602a593 core: validate runtime boundary values
50e189e docs: refresh phase 4 handover
df0b24f core: add capability checked system api surface
bf54362 core: stabilize preemption marker ordering
700038c core: boot custom minimal interpreter
```

Recent commits especially important to the current boundary:

```text
a71ac3d test: extend qemu acceptance timeout
```

This kept the acceptance harness aligned with the completed Phase 4 boot path.
The direct QEMU boots were already deterministic, but the full 48-boot handoff
suite could exceed the old 10-second runner timeout under load after the final
runtime-service markers were added. The runner now exposes a named
`DEFAULT_TIMEOUT_SECONDS` contract with a unit guard.

```text
cd4b887 core: deliver runtime service events
```

This completed the Phase 4 `async-events` slice and therefore the Phase 4
runtime sequence. It proves a fixed native event dispatches only to a ready
managed service, rejects delivery to a failed service, emits
`PYTHOS:CORE:SERVICE:EVENT`, and completes with
`PYTHOS:CORE:ASYNC_EVENTS_READY`.

```text
3677c04 core: restart failed runtime services
```

This completed the Phase 4 `service-restart` slice. It restarts a failed
noncritical service into a fresh starting generation and returns it to ready
without reboot.

```text
eb33aba core: contain runtime service exceptions
```

This completed the Phase 4 `exception-containment` slice. It records an
unhandled runtime exception as a failed state on only the failing service while
preserving an unrelated ready service.

```text
96782a7 core: add phase 4 service manager
```

This completed the Phase 4 `service-manager` slice. It consumes the exact
runtime plan's `self.ready()` operation and transitions the runtime service
from starting to ready under a fixed manager.

```text
602a593 core: validate runtime boundary values
```

This completed the Phase 4 `value-validation` slice. It changed runtime
operations to carry untrusted boundary values, added a small native validation
layer for the current `system.log` argument, rejects unsupported types,
oversized/empty/non-UTF-8 strings, raw-pointer-shaped values, and unchecked
native-struct-shaped values, models explicit host-call returned/rejected
results, and emits `PYTHOS:CORE:VALUE_VALIDATION_READY`.

```text
bf54362 core: stabilize preemption marker ordering
```

This fixed an intermittent Phase 2 preemption marker race. The producer used to
advance `PREEMPT_STEP` into a timer-switchable state before emitting the task
marker. A timer tick in that small window could switch to task B before task A's
marker appeared, causing strict `TASK_A/TASK_B/TASK_A/TASK_B` marker tests to
fail intermittently. The fix arms preemption only after the first task A marker
is written and stores later switchable states only after the corresponding
marker is emitted. The test contract was not weakened.

```text
df0b24f core: add capability checked system api surface
```

This completed the Phase 4 `system-api-surface` slice. It added the first
`system.*` host call, `system.log(message)`, behind a `LOG` capability and
emitted `PYTHOS:CORE:SYSTEM:LOG` followed by
`PYTHOS:CORE:SYSTEM_API_READY`.

## What Exists Now

PythOS currently boots as a native x86-64 UEFI system under QEMU/OVMF. The
loader and PythCore are real Rust freestanding code paths, not an HTML mockup
or hosted simulation.

Verified boot foundation:

* OVMF loads `/EFI/BOOT/BOOTX64.EFI`.
* The loader initializes direct COM1 serial diagnostics.
* The loader discovers a directly writable GOP framebuffer.
* The loader resolves boot files through the loaded image's own device handle.
* The loader reads `/PYTHOS/PYTHCORE.ELF`.
* The loader validates ELF64 `ET_EXEC` for x86-64.
* The loader rejects writable-executable kernel load segments.
* The loader copies file bytes and zeroes segment tails.
* The loader loads and validates `/PYTHOS/INIT.PAK`.
* The loader discovers and validates ACPI RSDP and SMBIOS entry points.
* The loader captures the final UEFI memory map.
* The loader calls `ExitBootServices()` successfully.
* The post-exit serial marker is emitted without UEFI boot services.
* The loader builds temporary handoff page tables and a guarded bootstrap stack.
* The loader enables NXE, switches `CR3` and `RSP`, passes `PythBootInfo` in
  `RDI`, and jumps to PythCore.

Verified PythCore substrate:

* PythCore enters higher-half execution and emits direct COM1 markers.
* PythCore validates `PythBootInfo`.
* PythCore classifies physical page ownership from the UEFI memory map.
* PythCore initializes a fixed bitmap allocator backing store.
* PythCore installs GDT, TSS, IDT, exception diagnostics, and hardened entry.
* PythCore remaps/masks the legacy PIC and routes interrupt stubs.
* PythCore builds kernel-owned page tables and switches `CR3` a second time.
* PythCore removes the broad loader identity map from active translation.
* PythCore proves the old broad identity range faults as expected.
* PythCore revalidates ACPI, SMBIOS, and `INIT.PAK` after `VM_READY`.
* PythCore configures a PIT-backed 100 Hz timer and monotonic tick counter.
* PythCore initializes native task structures and guarded kernel stacks.
* PythCore proves cooperative context switching.
* PythCore proves round-robin scheduling over fixed native tasks.
* PythCore proves idle path behavior.
* PythCore proves timer-forced preemption.
* PythCore proves task termination and slot exclusion.
* PythCore proves deterministic multi-task scheduler-test ordering.

Verified Phase 3 mechanisms:

* Service identities are kernel-owned and separate from task ids and slots.
* IPC channels carry fixed typed bounded messages between known service
  identities.
* Full queues return explicit errors without dropping queued messages.
* Request/reply works and missing replies return explicit timeout.
* Capability handles are kernel-owned `{slot, generation}` tokens.
* Capability validation checks holder, resource, rights, and generation.
* Shared memory is capability gated.
* Read-only shared-memory grants deny writes and preserve original bytes.
* Privileged IPC send checks capabilities before enqueue.
* Revocation invalidates one stale handle without harming unrelated handles.
* Negative authorization proof denies a known target and operation without a
  valid capability.
* Audit logging records grant, use, denial, and revocation outcomes.

Verified Phase 4 so far:

* ADR 0012 records the deliberate kernel-mode prototype through Phase 7 and the
  required Phase 8 migration into hardware-isolated address spaces.
* ADR 0013 selects a custom minimal interpreter over RustPython/MicroPython for
  the first runtime path.
* ADR 0014 defines the runtime payload inside `INIT.PAK`.
* ADR 0015 defines the exact-shape custom-minimal interpreter bootstrap.
* ADR 0016 defines the initial `system.*` API surface.
* ADR 0017 defines the current runtime value-validation boundary.
* `INIT.PAK` validates the inner runtime source payload.
* The custom-minimal interpreter recognizes the exact `HelloService` source
  shape and creates a fixed internal operation plan.
* The runtime receives a native task id, service identity, and explicit boot
  capability.
* `system.log(message)` requires a `LOG` capability.
* `system.log(message)` validates a bounded nonempty UTF-8 string value.
* Raw pointer and unchecked native-struct shaped boundary values are rejected.
* Host calls have explicit returned/rejected value-level result states.
* The successful runtime path emits `PYTHOS:CORE:SYSTEM:LOG`.
* The successful value proof emits `PYTHOS:CORE:VALUE_VALIDATION_READY`.
* `self.ready()` transitions the runtime service to ready under a fixed manager.
* A failed runtime service is contained without panicking PythCore.
* A failed noncritical service can restart into a fresh generation.
* A fixed native event dispatches only to a ready managed service.
* The successful Phase 4 path emits `PYTHOS:CORE:ASYNC_EVENTS_READY`.

Verified Phase 5 so far:

* ADR 0018 records the native Phase 5 input-event service and shell object
  contract.
* ADR 0019 records the planned `FONT.PSF` boot-info extension for the next
  slice.
* Fixed keyboard and mouse driver proofs decode input only through explicit
  input capabilities.
* A native input-event service normalizes those raw events into typed input
  events for a capability-holding subscriber.
* The software renderer fills clipped rectangles into a bounded native pixel
  buffer and emits `PYTHOS:CORE:SOFTWARE_RENDERER_READY`.
* ADR 0019 appends explicit `font_phys` and `font_len` boot-info fields and
  increments the boot ABI minor version to 2.
* The loader loads a deterministic PSF1 `/PYTHOS/FONT.PSF`, PythCore reserves
  and maps the font range, and the font-system proof validates PSF metadata
  before emitting `PYTHOS:CORE:FONT_SYSTEM_READY`.
* Typed shell drawable objects now carry stable object ids and typed kinds
  separately from presentation bindings.
* The compositor composes bounded surfaces into a bounded framebuffer target,
  proves target-edge clipping, and emits `PYTHOS:CORE:COMPOSITOR_READY`.

## What Does Not Exist Yet

Do not imply these are implemented:

* General Python language compatibility.
* General parser or bytecode execution.
* General Python object model or broad native/Python value conversion beyond
  the current `system.log` string boundary.
* General service discovery or dependency resolution.
* Dynamic service spawning from arbitrary Python source.
* Broad service restart policy beyond the fixed noncritical proof.
* General async runtime or coroutine scheduler.
* Full GUI shell.
* Pointer cursor state, focus, movable windows, widgets, or shell applications.
* Audio driver or cinematic boot sequence.
* Persistent object storage.
* Networking.
* Ring-3 execution.
* Separate user address spaces.
* Syscall ABI.
* Hardware-backed hostile-code isolation.
* SMP.
* AI service or agent control.

Current Python/runtime reality is intentionally narrow:

```text
validated INIT.PAK runtime payload
-> exact HelloService source recognition
-> fixed internal operation plan
-> capability-scoped runtime identity
-> first host call: system.log(message)
-> validated bounded UTF-8 runtime string value
-> fixed service manager ready transition
-> contained failed service
-> fixed failed-service restart
-> fixed native event delivery to ready service
```

It is not a general Python runtime yet.

## Active Slice: Phase 5 pointer-cursor / window-focus / movable-windows

Phase 4 is complete. The next allowed roadmap work is Phase 5 `pointer-cursor`,
`window-focus`, and `movable-windows`.

Roadmap intent:

```text
Add standard window interaction primitives on top of the completed typed-object
and presentation-binding compositor surface: cursor state, focus selection, and
window movement that preserves object identity.
```

Recommended first scope for the next agent:

1. Start with a failing marker test in `scripts/test-boot.py` and
   `tests/boot_core_handoff.py`.
2. Add a new slice such as:

   ```powershell
   python scripts\test-boot.py --slice window-interaction
   ```

3. The first run should fail because the new completion marker is missing.
4. Keep the marker order after `PYTHOS:CORE:COMPOSITOR_READY` and before
   `PYTHOS:CORE:FRAMEBUFFER_READY`.
5. Preserve ADR 0018's object-id and presentation-binding split.
6. Preserve the completed Phase 4 runtime proof path.

Do not let Phase 5 entry become:

```text
general Python interpreter
GUI
IPC transport redesign
syscall ABI
audio
storage
networking
```

## Accepted ADRs To Read Before Touching Related Areas

```text
docs/decisions/0001-boot-protocol-abi.md
docs/decisions/0002-kernel-owned-page-tables.md
docs/decisions/0003-bootinfo-complete-firmware-metadata.md
docs/decisions/0004-deterministic-qemu-exit.md
docs/decisions/0005-exception-entry-hardening.md
docs/decisions/0006-task-control-block-invariants.md
docs/decisions/0007-round-robin-scheduler-and-task-layout.md
docs/decisions/0008-service-identity.md
docs/decisions/0009-capability-token-representation.md
docs/decisions/0010-capability-revocation-semantics.md
docs/decisions/0011-ipc-channel-boundaries.md
docs/decisions/0012-phase-4-kernel-mode-runtime-sequencing.md
docs/decisions/0013-python-runtime-selection.md
docs/decisions/0014-init-pak-runtime-payload.md
docs/decisions/0015-custom-minimal-interpreter-bootstrap.md
docs/decisions/0016-system-api-surface.md
docs/decisions/0017-runtime-value-validation.md
docs/decisions/0018-phase-5-shell-object-and-input-contracts.md
docs/decisions/0019-font-psf-bootinfo-extension.md
```

Most relevant for Phase 5 entry:

```text
0012  kernel-mode runtime sequencing and Phase 8 migration cost
0013  custom minimal interpreter choice
0014  runtime payload ABI
0015  exact-shape interpreter bootstrap
0016  initial system.* API surface
0017  runtime value-validation boundary
0018  Phase 5 shell object and input contract
0019  FONT.PSF boot-info extension
```

## Important Files By Area

Loader:

```text
boot/src/main.rs
boot/src/serial.rs
boot/src/uefi.rs
boot/src/firmware.rs
boot/src/graphics.rs
boot/src/elf.rs
boot/src/initrd.rs
boot/src/font.rs
boot/src/memory_map.rs
boot/src/boot_info.rs
boot/src/exit_boot_services.rs
```

Shared ABI:

```text
shared/src/boot_protocol.rs
shared/src/init_pak.rs
shared/src/runtime_payload.rs
```

Core entry and boot metadata:

```text
core/src/main.rs
core/src/boot_metadata.rs
core/src/framebuffer.rs
core/src/font_system.rs
core/src/shell_objects.rs
core/src/compositor.rs
core/src/input_drivers.rs
core/src/input_events.rs
core/src/software_renderer.rs
core/linker.ld
```

Memory and architecture:

```text
core/src/memory/physical.rs
core/src/memory/virtual.rs
core/src/architecture/x86_64/gdt.rs
core/src/architecture/x86_64/idt.rs
core/src/architecture/x86_64/exceptions.rs
core/src/architecture/x86_64/interrupts.rs
core/src/architecture/x86_64/timer.rs
core/src/architecture/x86_64/clock.rs
```

Scheduler and tasks:

```text
core/src/tasks.rs
core/src/kernel_stacks.rs
core/src/context_switch.rs
core/src/scheduler.rs
```

IPC and capabilities:

```text
core/src/service_identity.rs
core/src/ipc_channels.rs
core/src/capabilities.rs
core/src/shared_memory.rs
core/src/permission_validation.rs
core/src/audit.rs
```

Phase 4 runtime:

```text
core/src/runtime_loader.rs
core/src/interpreter.rs
core/src/system_api.rs
core/src/value_validation.rs
core/src/service_manager.rs
shared/src/runtime_payload.rs
docs/research/runtime-selection/
```

Build and test:

```text
scripts/build-image.py
scripts/build-iso.py
scripts/run-qemu.py
scripts/test-boot.py
tests/boot_core_handoff.py
tests/test_boot_marker_contract.py
tests/test_iso_image.py
tests/test_qemu_exit.py
```

Docs and scope:

```text
AGENTS.md
docs/PythOS-SAS-001.md
docs/PythOS-TDD-001.md
docs/ROADMAP.md
docs/THREAT-MODEL.md
docs/HANDOVER.md
docs/vision/
```

## Testing Rules

Always distinguish these levels:

```text
cargo fmt/check       style and formatting only
cargo test           host unit tests only
cargo clippy         lint only
scripts/test-boot.py QEMU-backed serial oracle
boot_core_handoff    repeated QEMU acceptance across slices
```

A slice is not complete because it compiles. It is complete only when the
QEMU-backed marker oracle proves the required serial sequence.

Use these commands for a normal slice completion gate:

```powershell
cargo fmt --check
cargo test -p pythos-shared
cargo test -p pythos-core
cargo clippy -p pythos-core --target x86_64-unknown-none -- -D warnings
cargo clippy -p pythos-boot --target x86_64-unknown-uefi -- -D warnings
python scripts\test-boot.py --slice <active-slice>
python scripts\test-boot.py --slice milestone-1
python scripts\test-boot.py --slice milestone-1 --media iso
python -m unittest tests.test_iso_image tests.test_boot_marker_contract tests.test_qemu_exit
python -m unittest tests.boot_core_handoff
git diff --check
```

For the next slice, replace `<active-slice>` with the actual slice name added
to `scripts/test-boot.py`.

## Deterministic QEMU Exit Contract

`scripts/run-qemu.py` classifies terminal outcomes and the harness uses those
outcomes as part of acceptance.

Expected success line:

```text
QEMU_OUTCOME success
```

Exit-code contract:

```text
success                 0
panic                   20
reset                   21
timeout                 22
marker-order-violation  23
```

Timeout is not success. If a test times out, treat it as a failure even if
earlier markers looked healthy.

## Boot Artifacts

Current generated paths:

```text
image\esp\
target\pythos.iso
target\boot-serial.log
target\boot-screen.png
```

The bootable ISO path is:

```text
target\pythos.iso
```

Regenerate and test it with:

```powershell
python scripts\build-iso.py --output target\pythos.iso
python scripts\test-boot.py --slice milestone-1 --media iso
```

Do not assume a stale ISO reflects current code after edits. Rebuild through
the test command before claiming it boots.

## Known Risks And Gaps

Current accepted limitations:

* The custom minimal interpreter is deliberately tiny and exact-shape.
* It recognizes the current `HelloService` source shape; it is not full Python.
* Phase 4 service lifecycle behavior is a fixed proof, not a general service
  registry.
* Async event delivery is a fixed native event proof, not a general coroutine
  runtime.
* No kernel heap exists beyond fixed/static structures and existing allocator
  proofs.
* Loader page-table frames are inactive after `VM_READY` but not reclaimed.
* Exception diagnostics are serial-first and intentionally allocation-free.
* GUI output is still a diagnostic framebuffer boot screen, not a desktop.
* Capability separation is still kernel-mode logical enforcement, not hostile
  ring-3 isolation. Phase 8 exists for hardware enforcement.
* QEMU/OVMF is the target. Physical hardware support is not claimed.

## Scope Guardrails

Follow these strictly:

* Implement only the active slice.
* Do not silently change an ABI.
* Record ABI-relevant decisions as ADRs.
* Every `unsafe` block needs its invariant documentation.
* Do not add future-phase infrastructure just because it is convenient.
* Do not use `docs/vision/` as justification for code before the roadmap phase
  that naturally owns it.
* AI remains outside the trusted core.
* Do not claim full security where only logical isolation exists.

Specific things not allowed in the next slice:

```text
general GUI shell applications
audio
storage
networking
package management
agent concepts
workspace/proposal concepts from vision docs
general Python compatibility
ring-3
syscall ABI
SMP
```

## Phase Boundary Rule

`AGENTS.md` contains an explicit phase-boundary rule:

```text
At a phase boundary, halt and report after the final slice passes. Do not begin
the next phase's first slice without explicit re-invocation.
```

This mattered after Phase 3 and will matter again after Phase 4. Within a phase,
sequential slice execution is allowed when the user explicitly says to continue.
Across a phase boundary, stop and report.

## If The Next Agent Continues With Phase 5

Suggested disciplined flow:

1. Confirm live state:

   ```powershell
   git status --short --branch
   git log --oneline --decorate -10
   ```

2. Read the ADRs:

   ```text
   docs/decisions/0012-phase-4-kernel-mode-runtime-sequencing.md
   docs/decisions/0013-python-runtime-selection.md
   docs/decisions/0014-init-pak-runtime-payload.md
   docs/decisions/0015-custom-minimal-interpreter-bootstrap.md
   docs/decisions/0016-system-api-surface.md
   docs/decisions/0017-runtime-value-validation.md
   docs/decisions/0018-phase-5-shell-object-and-input-contracts.md
   docs/decisions/0019-font-psf-bootinfo-extension.md
   ```

3. Add the failing test first:

   ```text
   scripts/test-boot.py
   tests/boot_core_handoff.py
   ```

4. Expected initial failure:

   ```text
   missing marker for the Phase 5 window-interaction slice
   ```

5. Implement only cursor state, focus selection, and movement preserving
   object ids. Do not build widgets or shell applications early.

6. Keep the marker order:

   ```text
   PYTHOS:CORE:INTERPRETER_BOOTED
   PYTHOS:CORE:SYSTEM:LOG
   PYTHOS:CORE:SYSTEM_API_READY
   PYTHOS:CORE:VALUE_VALIDATION_READY
   PYTHOS:CORE:SERVICE_MANAGER_READY
   PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED
   PYTHOS:CORE:SERVICE_RESTART_READY
   PYTHOS:CORE:ASYNC_EVENTS_READY
   PYTHOS:CORE:INPUT_DRIVERS_READY
   PYTHOS:CORE:INPUT_EVENT_SERVICE_READY
   PYTHOS:CORE:SOFTWARE_RENDERER_READY
   PYTHOS:CORE:FONT_SYSTEM_READY
   PYTHOS:CORE:COMPOSITOR_READY
   PYTHOS:CORE:FRAMEBUFFER_READY
   ```

7. Run the full verification set.

8. Update:

   ```text
   AGENTS.md
   docs/PythOS-TDD-001.md
   docs/ROADMAP.md
   docs/HANDOVER.md
   ```

9. Commit and push only after verification.

## Final Current Summary

Fair state description:

```text
Firmware boot                      complete
Kernel handoff                     complete
Kernel-owned VM substrate          complete
Exception diagnostics              complete
Deterministic QEMU exit            complete
Timer and monotonic clock          complete
Native tasks and preemption        complete
Scheduler acceptance tests         complete
IPC and capability mechanisms      complete
Negative authorization proof       complete
Audit logging                      complete
Runtime selection                  complete
INIT.PAK runtime payload loading   complete
Custom minimal interpreter boot    complete
First capability-checked system.*  complete
Runtime value validation           complete
Python service manager             complete
Python exception containment       complete
Python service restart             complete
Python async event delivery        complete
Phase 5 input drivers              complete
Phase 5 input event service        complete
Phase 5 software renderer          complete
Phase 5 font system                complete
Phase 5 compositor                 complete
GUI shell                          in progress
Audio/cinematic boot               not started
Persistent object storage          not started
Ring-3 isolation                   not started
```

The base is no longer only a loader. PythCore boots after UEFI, owns its own
execution substrate, schedules native tasks, enforces local kernel-mode
capabilities, runs the intentionally narrow Python-native runtime path, and has
entered Phase 5 graphical-shell groundwork. The next work is the Phase 5
`pointer-cursor` / `window-focus` / `movable-windows` slice.
