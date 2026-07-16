# PythOS Agent Instructions

Read `docs/PythOS-SAS-001.md` and `docs/PythOS-TDD-001.md` before editing.

Implement only the active milestone.

At a phase boundary, halt and report after the final slice passes. Do not begin
the next phase's first slice without explicit re-invocation.

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

## Active Milestone

The active branch of work is `milestone/phase4-runtime-selection`.

Verified vertical slices currently include:

```text
OVMF
-> BOOTX64.EFI
-> COM1 initialized
-> PYTHOS:LOADER:ENTER
-> PYTHOS:LOADER:GOP_READY
-> PYTHOS:LOADER:KERNEL_LOADED
-> PYTHOS:LOADER:MEMORY_MAP_READY
-> PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK
-> PYTHOS:CORE:ENTER
-> PYTHOS:CORE:BOOTINFO_VALID
-> PYTHOS:CORE:MEMORY_READY
-> PYTHOS:CORE:GDT_READY
-> PYTHOS:CORE:IDT_READY
-> PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY
-> PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED
-> PYTHOS:CORE:INTERRUPTS_READY
-> PYTHOS:CORE:VM_READY
-> PYTHOS:CORE:EXPECTED_PAGE_FAULT
-> PYTHOS:CORE:IDENTITY_MAP_REMOVED
-> PYTHOS:CORE:BOOTINFO_COMPLETE
-> PYTHOS:CORE:TIMER_READY
-> PYTHOS:CORE:CLOCK_READY
-> PYTHOS:CORE:TASKS_READY
-> PYTHOS:CORE:EXPECTED_PAGE_FAULT
-> PYTHOS:CORE:KERNEL_STACKS_READY
-> PYTHOS:CORE:CONTEXT_SWITCH:TASK_A
-> PYTHOS:CORE:CONTEXT_SWITCH:TASK_B
-> PYTHOS:CORE:CONTEXT_SWITCH:TASK_A
-> PYTHOS:CORE:CONTEXT_SWITCH:TASK_B
-> PYTHOS:CORE:CONTEXT_SWITCH_READY
-> PYTHOS:CORE:SCHEDULER:TASK_A
-> PYTHOS:CORE:SCHEDULER:TASK_B
-> PYTHOS:CORE:SCHEDULER:TASK_A
-> PYTHOS:CORE:SCHEDULER:TASK_B
-> PYTHOS:CORE:SCHEDULER_READY
-> PYTHOS:CORE:IDLE_TASK
-> PYTHOS:CORE:IDLE_TASK_READY
-> PYTHOS:CORE:PREEMPT:TASK_A
-> PYTHOS:CORE:PREEMPT:TASK_B
-> PYTHOS:CORE:PREEMPT:TASK_A
-> PYTHOS:CORE:PREEMPT:TASK_B
-> PYTHOS:CORE:PREEMPT_READY
-> PYTHOS:CORE:TASK_TERMINATED
-> PYTHOS:CORE:TASK_TERMINATION_READY
-> PYTHOS:CORE:SCHEDTEST:TASK_A
-> PYTHOS:CORE:SCHEDTEST:TASK_B
-> PYTHOS:CORE:SCHEDTEST:TASK_C
-> PYTHOS:CORE:SCHEDTEST:TASK_A
-> PYTHOS:CORE:SCHEDTEST:TASK_B
-> PYTHOS:CORE:SCHEDTEST:TASK_C
-> PYTHOS:CORE:SCHEDULER_TESTS_READY
-> PYTHOS:CORE:SERVICE_IDENTITY_READY
-> PYTHOS:CORE:IPC:SEND
-> PYTHOS:CORE:IPC:RECV
-> PYTHOS:CORE:IPC_CHANNELS_READY
-> PYTHOS:CORE:IPC:QUEUE_FULL
-> PYTHOS:CORE:BOUNDED_QUEUES_READY
-> PYTHOS:CORE:IPC:REQUEST
-> PYTHOS:CORE:IPC:REPLY
-> PYTHOS:CORE:IPC:REPLY_TIMEOUT
-> PYTHOS:CORE:REQUEST_REPLY_READY
-> PYTHOS:CORE:CAPABILITY:GRANT
-> PYTHOS:CORE:CAPABILITY:USE
-> PYTHOS:CORE:CAPABILITY_HANDLES_READY
-> PYTHOS:CORE:SHM:READ_ONLY
-> PYTHOS:CORE:SHM:WRITE_DENIED
-> PYTHOS:CORE:SHARED_MEMORY_HANDLES_READY
-> PYTHOS:CORE:PERMISSION:IPC_ALLOWED
-> PYTHOS:CORE:PERMISSION:IPC_DENIED
-> PYTHOS:CORE:PERMISSION_VALIDATION_READY
-> PYTHOS:CORE:CAPABILITY:REVOKE
-> PYTHOS:CORE:CAPABILITY:STALE_DENIED
-> PYTHOS:CORE:REVOCATION_READY
-> PYTHOS:CORE:CAPABILITY:KNOWN_TARGET_DENIED
-> PYTHOS:CORE:NEGATIVE_AUTHORIZATION_READY
-> PYTHOS:CORE:AUDIT:GRANT
-> PYTHOS:CORE:AUDIT:USE
-> PYTHOS:CORE:AUDIT:DENIAL
-> PYTHOS:CORE:AUDIT:REVOCATION
-> PYTHOS:CORE:AUDIT_LOGGING_READY
-> PYTHOS:CORE:PHASE_3_COMPLETE
-> PYTHOS:CORE:RUNTIME_SELECTED
-> PYTHOS:CORE:INIT_PAK_LOADED
-> PYTHOS:CORE:INTERPRETER_BOOTED
-> PYTHOS:CORE:SYSTEM:LOG
-> PYTHOS:CORE:SYSTEM_API_READY
-> PYTHOS:CORE:VALUE_VALIDATION_READY
-> PYTHOS:CORE:SERVICE:READY
-> PYTHOS:CORE:SERVICE_MANAGER_READY
-> PYTHOS:CORE:SERVICE:EXCEPTION
-> PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED
-> PYTHOS:CORE:SERVICE:RESTART
-> PYTHOS:CORE:SERVICE_RESTART_READY
-> PYTHOS:CORE:SERVICE:EVENT
-> PYTHOS:CORE:ASYNC_EVENTS_READY
-> PYTHOS:CORE:INPUT:KEYBOARD
-> PYTHOS:CORE:INPUT:MOUSE
-> PYTHOS:CORE:INPUT_DRIVERS_READY
-> PYTHOS:CORE:INPUT:EVENT
-> PYTHOS:CORE:INPUT_EVENT_SERVICE_READY
-> PYTHOS:CORE:RENDER:RECT
-> PYTHOS:CORE:SOFTWARE_RENDERER_READY
-> PYTHOS:CORE:FONT:PSF_LOADED
-> PYTHOS:CORE:FONT_SYSTEM_READY
-> PYTHOS:CORE:COMPOSITOR:SURFACE
-> PYTHOS:CORE:COMPOSITOR:CLIP
-> PYTHOS:CORE:COMPOSITOR_READY
-> PYTHOS:CORE:POINTER_CURSOR_READY
-> PYTHOS:CORE:WINDOW_FOCUS_READY
-> PYTHOS:CORE:MOVABLE_WINDOWS_READY
-> PYTHOS:CORE:FRAMEBUFFER_READY
-> PYTHOS:CORE:MILESTONE_1_COMPLETE
```

The loader builds temporary page tables, switches to the bootstrap stack, and jumps to `pythcore_entry` with `PythBootInfo` in `RDI`. PythCore validates the boot ABI, owns physical page classification, installs GDT/TSS/IDT structures, installs allocation-free exception diagnostics, verifies full-register exception-entry preservation through a controlled `INT3`, remaps and masks the legacy PIC interrupt controller, builds replacement kernel-owned page tables, switches `CR3` a second time, proves an address from the old broad loader identity range now faults, revalidates ACPI/SMBIOS/INIT.PAK boot metadata, configures a PIT-backed tick source, exposes a read-only monotonic tick clock, initializes a fixed native task table with the bootstrap task recorded as running, proves the active bootstrap kernel stack has an unmapped guard page through an expected page fault, performs a cooperative context-switch self-test with two alternating native contexts, runs a cooperative round-robin scheduler proof over fixed ready tasks, switches through a fixed idle context only after the ready set is empty, proves IRQ0-forced preemption between spin-only native contexts, exits a fixed native task and proves its terminated slot is no longer selectable, proves three native tasks interleave under timer-forced preemption, assigns service identities independently from task/slot identity and rejects stale identity reuse, sends and receives a fixed typed IPC message between known service identities with exact payload validation, proves a full fixed IPC queue returns an explicit error without dropping queued messages, proves request/reply correlation and explicit reply timeout behavior, grants and validates a kernel-owned capability handle, gates a shared memory region through a read-only capability and denies writes, validates capability rights before privileged IPC send, revokes one capability without affecting another handle, denies a known target/operation without a valid handle, records grant/use/denial/revocation audit events, emits `PYTHOS:CORE:PHASE_3_COMPLETE`, records the Phase 4 custom minimal interpreter decision through `PYTHOS:CORE:RUNTIME_SELECTED`, validates the ADR 0014 runtime source payload inside `INIT.PAK`, boots the ADR 0015 custom-minimal interpreter as a capability-scoped runtime task, executes the ADR 0016 `system.log` host call through a `LOG` capability, validates the ADR 0017 runtime value boundary for type, length, UTF-8, rejected pointer/native-struct shapes, and explicit host-call result representation, transitions the runtime service through a fixed manager-owned ready state, contains a failed service without panicking PythCore, restarts a failed noncritical service into a fresh generation, dispatches a fixed native event only to a ready service, emits `PYTHOS:CORE:ASYNC_EVENTS_READY`, decodes fixed keyboard and mouse input events only through explicit input capabilities, normalizes those raw driver events through a capability-gated native input-event service, proves clipped software rectangle rendering into a bounded pixel buffer, maps and parses the boot-provided `FONT.PSF` through explicit boot-info font fields, composes typed-object-backed presentation surfaces with clipping, proves cursor bounds, z-order focus selection, and moving a focused window without changing object identity, emits `PYTHOS:CORE:MOVABLE_WINDOWS_READY`, renders the post-firmware boot screen, emits `PYTHOS:CORE:MILESTONE_1_COMPLETE`, and reaches deterministic QEMU termination.

Milestone 1.5, Phase 2, Phase 3, Phase 4, and Phase 5 `keyboard-driver` / `mouse-driver`, `input-event-service`, `software-renderer`, `font-system`, `compositor` / `surfaces` / `clipping`, and `pointer-cursor` / `window-focus` / `movable-windows` are complete. The next active slice is Phase 5 `buttons-and-text-fields`. Do not begin Phase 6 or any audio, storage, networking, AI, ring-3, SMP, or hardware-expansion work without explicit re-invocation.

For `vm-ready`, PythCore builds and owns replacement page tables, switches `CR3` a second time, removes the broad loader identity mapping from active translation, keeps the first 2 MiB unmapped, preserves W^X kernel mappings, retains framebuffer and COM1 access, keeps boot information and the memory map accessible, retains a guarded active kernel stack, and emits `PYTHOS:CORE:VM_READY` only after post-switch validation. The follow-up `identity-map-removed` proof deliberately reads from an address that should only have been reachable through the old broad identity map, recovers from the expected page fault, and emits `PYTHOS:CORE:IDENTITY_MAP_REMOVED`. Loader page-table frames are not reclaimed in this slice.

The required Milestone 1.5 marker order extends the existing sequence with:

```text
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
PYTHOS:CORE:POINTER_CURSOR_READY
PYTHOS:CORE:WINDOW_FOCUS_READY
PYTHOS:CORE:MOVABLE_WINDOWS_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

The QEMU harness must report `QEMU_OUTCOME success` for successful ESP and ISO milestone boots. Timeout termination is not valid success evidence.

Do not land a slice that regresses any already-verified marker in QEMU serial capture.

## Scope Boundary

The first milestone may contain only:

```text
boot/
core/
shared/
scripts/
tests/
docs/
```

Do not add embedded Python, MicroPython, services, semantic storage, networking, audio, widgets, package management, AI, SMP, ring-3 applications, or broad hardware support.

## Unsafe Rust Policy

Every `unsafe` block must document:

1. the invariant being relied upon
2. who established that invariant
3. the permitted lifetime of the invariant
4. pointer ownership
5. expected alignment
6. expected mapped length
7. concurrency assumptions
8. consequences of violation

Keep unsafe regions small. Do not wrap large functions in `unsafe`.
