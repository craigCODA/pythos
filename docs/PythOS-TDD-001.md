# PythOS-TDD-001: Boot Core Handoff Technical Design

Status: Active for milestone 1.

## Required EFI Partition Structure

```text
/EFI/BOOT/BOOTX64.EFI
/PYTHOS/PYTHCORE.ELF
/PYTHOS/INIT.PAK
/PYTHOS/BOOT.CFG
/PYTHOS/FONT.PSF
```

Initial `BOOT.CFG`:

```text
serial=true
log_level=trace
panic=halt
runtime_bundle=/PYTHOS/INIT.PAK
```

The same file set may be packaged as `target/pythos.iso` for UEFI CD-ROM boot.
The ISO is an El Torito no-emulation UEFI image that embeds a FAT16 EFI System
Partition and also exposes the required files through ISO9660 records. This is
packaging only; it does not alter the boot ABI or the milestone marker order.

## Required Loader Sequence

The complete milestone 1 loader sequence is:

1. Initialize UEFI text output where available and COM1 serial output. Emit `PYTHOS:LOADER:ENTER`.
2. Retrieve system table, firmware vendor, firmware revision, loaded image protocol, filesystem protocol, GOP framebuffer information, ACPI RSDP, SMBIOS when available, and UEFI memory map.
3. Select a directly writable GOP mode, preferring 1024 by 768 at 32 bits per pixel. Emit `PYTHOS:LOADER:GOP_READY`.
4. Read and validate `PYTHCORE.ELF`, allocate segments, copy bytes, zero BSS, and reject writable-executable mappings. Emit `PYTHOS:LOADER:KERNEL_LOADED`.
5. Load `INIT.PAK` and `FONT.PSF` into page-aligned physical memory.
6. Construct temporary mappings sufficient for PythCore entry.
7. Retrieve the final memory map with spare capacity and handle a stale `ExitBootServices()` map key by reacquiring and retrying. Emit `PYTHOS:LOADER:MEMORY_MAP_READY`.
8. Call `ExitBootServices()` successfully. After success, use no UEFI boot services. Emit `PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK` through direct serial I/O.
9. Disable maskable interrupts, clear the direction flag, activate the temporary kernel page table, switch to the bootstrap stack, place `PythBootInfo` in `RDI`, and jump to PythCore.

Verified vertical slices:

* `loader-enter` implements step 1 through COM1 serial output.
* `gop-ready` implements GOP discovery, direct-framebuffer mode selection, and `PYTHOS:LOADER:GOP_READY`.
* `kernel-loaded` implements EFI filesystem access, bounded ELF64 `ET_EXEC` validation, physical page allocation for `PT_LOAD` segments, segment copy/zeroing, loaded segment metadata retention, and `PYTHOS:LOADER:KERNEL_LOADED`.
* `memory-map-ready` implements `INIT.PAK` and `FONT.PSF` loading, retained framebuffer/kernel/init/font metadata, preallocated `PythBootInfo`, UEFI memory-map capture with spare descriptor capacity, and `PYTHOS:LOADER:MEMORY_MAP_READY`.
* `exit-boot-services-ok` implements `ExitBootServices()` with one stale-map-key refresh using the retained memory-map buffer and emits `PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK` through direct serial output after firmware boot services are gone.
* `core-enter` implements loader-owned temporary page tables (a 2 MiB-to-4 GiB identity map with the first 2 MiB left unmapped, kernel segments at their ELF virtual addresses with writable-XOR-executable leaf permissions, the framebuffer under the device region, and a guarded bootstrap stack), `EFER.NXE` enablement, the `CR3`/`RSP` switch, the `RDI` boot-info argument, the jump to `pythcore_entry`, and PythCore's direct-COM1 `PYTHOS:CORE:ENTER`.
* `bootinfo-valid` implements host-tested `PythBootInfo` structure validation in the shared ABI crate plus null/alignment checking in PythCore, emitting `PYTHOS:CORE:BOOTINFO_VALID` on success and `PYTHOS:CORE:BOOTINFO_INVALID` on rejection.
* `memory-ready` implements PythCore-side UEFI memory-descriptor walking, explicit free/reserved page ownership, required loader/core range reservation, a fixed 4 KiB bitmap allocator backing store, and `PYTHOS:CORE:MEMORY_READY`.
* `gdt-ready` installs a minimal 64-bit GDT with kernel code, kernel data, and TSS descriptors, reloads segment registers, loads `TR`, and emits `PYTHOS:CORE:GDT_READY`.
* `idt-ready` installs a 256-entry IDT of panic-loop exception gates and emits `PYTHOS:CORE:IDT_READY`.
* `exceptions-diagnostic` installs per-vector exception stubs for CPU exception vectors 0 through 31 and an allocation-free diagnostic handler that reports vector, error code, `RIP`, `CS`, `RFLAGS`, `RSP`, `SS`, `CR2` for page faults, and current `CR3` over COM1 before entering the panic loop. It emits `PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY`.
* `exception-entry-hardening` extends the exception entry path to preserve all general-purpose registers, align the stack before calling Rust, expose a normalized saved-register frame to the handler, and prove handled exception return by running a controlled register-heavy `INT3` probe. It emits `PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED`.
* `interrupt-controller` remaps the legacy PIC away from CPU exception vectors, masks all IRQ lines, installs external interrupt stubs for vectors `0x20..0x2F`, exposes mask/unmask helpers for later timer work, and emits `PYTHOS:CORE:INTERRUPTS_READY`.
* `vm-ready` implements PythCore-owned replacement page tables, discovers currently active physical backing by walking the loader page tables before replacement, maps linker-defined kernel text/rodata/data regions with W^X permissions, maps boot information, memory map, `INIT.PAK`, `FONT.PSF`, the guarded bootstrap stack, framebuffer, and page-table frames needed for validation, switches `CR3` a second time, validates the active root, and emits `PYTHOS:CORE:VM_READY`.
* `identity-map-removed` verifies that the old broad loader identity map is absent after the PythCore `CR3` switch by checking that `0x0400_0000` is untranslated, arming a one-shot expected page fault for that address, recovering through the exception handler, and emitting `PYTHOS:CORE:EXPECTED_PAGE_FAULT` followed by `PYTHOS:CORE:IDENTITY_MAP_REMOVED`.
* `bootinfo-complete` resolves boot files through `LoadedImage.DeviceHandle`, validates ACPI RSDP and SMBIOS entry points in the loader, passes their physical addresses through `PythBootInfo`, maps the required firmware metadata under PythCore-owned page tables, revalidates ACPI RSDP/root table and SMBIOS checksums in PythCore, validates the binary `INIT.PAK` header and checksum in both loader and core, zeros unsupported runtime-services pointers, and emits `PYTHOS:CORE:BOOTINFO_COMPLETE`.
* `timer` programs the PIT for a fixed 100 Hz tick, unmasks IRQ0, enables interrupts after final boot metadata validation, waits for one observed timer interrupt, and emits `PYTHOS:CORE:TIMER_READY`.
* `monotonic-clock` exposes a read-only monotonic tick counter derived from the timer source without exposing timer reprogramming controls, validates the timer has observed at least one tick, and emits `PYTHOS:CORE:CLOCK_READY`.
* `task-structures` defines the fixed native task table, saved task-register frame, task id, task state enum, and bootstrap running task record without dynamic allocation. It emits `PYTHOS:CORE:TASKS_READY`.
* `kernel-stacks` records guarded kernel stack ownership for the bootstrap task, verifies the active stack pages remain mapped, verifies the guard page below the stack is untranslated, performs a controlled write to that guard page, recovers through the diagnostic page-fault path, and emits `PYTHOS:CORE:KERNEL_STACKS_READY`.
* `context-switch` initializes two fixed native contexts on separate stacks, switches cooperatively from bootstrap to task A to task B to task A to task B and back to bootstrap, and emits `PYTHOS:CORE:CONTEXT_SWITCH_READY` after the alternating task markers prove switch continuity.
* `scheduler` selects fixed ready tasks with a round-robin cursor, drives the existing cooperative context-switch path through `TASK_A`, `TASK_B`, `TASK_A`, `TASK_B`, and emits `PYTHOS:CORE:SCHEDULER_READY`. Priority scheduling is explicitly deferred.
* `idle-task` verifies the empty-ready-set path after the scheduler proof, switches through a fixed idle context once, returns to bootstrap without permanently halting the CPU, and emits `PYTHOS:CORE:IDLE_TASK_READY`.
* `preemption` arms two spin-only native contexts, lets IRQ0 force context switches through the interrupt handler, produces alternating `PREEMPT:TASK_A` and `PREEMPT:TASK_B` markers without voluntary yield points, returns to bootstrap, and emits `PYTHOS:CORE:PREEMPT_READY`.
* `task-termination` switches into a fixed native task that exits back to bootstrap, marks its scheduler slot terminated/reclaimable, verifies the terminated slot is not selected by round-robin scheduling, and emits `PYTHOS:CORE:TASK_TERMINATION_READY`.
* `scheduler-tests` runs three spin-only native contexts through timer-forced preemption, verifies deterministic A/B/C/A/B/C serial ordering under the QEMU marker oracle, returns to bootstrap, and emits `PYTHOS:CORE:SCHEDULER_TESTS_READY`.
* `service-identity` assigns kernel-owned service identities separately from task ids and scheduler slots, proves a reused slot receives a fresh service identity, rejects stale identity lookup, and emits `PYTHOS:CORE:SERVICE_IDENTITY_READY`.
* `ipc-channels` creates a trusted kernel-internal fixed channel between two known service identities, copies a typed bounded message into channel storage, receives it through the peer identity, validates exact type, length, checksum, and payload bytes, and emits `PYTHOS:CORE:IPC:SEND`, `PYTHOS:CORE:IPC:RECV`, and `PYTHOS:CORE:IPC_CHANNELS_READY`.
* `bounded-queues` fills the fixed IPC queue, proves a further send returns the explicit `QueueFull` error instead of silently dropping data, drains the original messages unchanged, and emits `PYTHOS:CORE:IPC:QUEUE_FULL` followed by `PYTHOS:CORE:BOUNDED_QUEUES_READY`.
* `request-reply` sends a typed request, receives and validates it at the responder, sends a typed reply, receives the exact matching reply at the requester, proves a missing reply returns explicit timeout, and emits `PYTHOS:CORE:IPC:REQUEST`, `PYTHOS:CORE:IPC:REPLY`, `PYTHOS:CORE:IPC:REPLY_TIMEOUT`, and `PYTHOS:CORE:REQUEST_REPLY_READY`.
* `capability-handles` implements the ADR 0009 kernel-owned capability table with `{slot, generation}` handles, validates holder/resource/rights through the table entry, rejects use by the wrong holder, and emits `PYTHOS:CORE:CAPABILITY:GRANT`, `PYTHOS:CORE:CAPABILITY:USE`, and `PYTHOS:CORE:CAPABILITY_HANDLES_READY`.
* `shared-memory-handles` gates a fixed shared memory region through the capability table, proves a read-only grant can read the region but cannot write it, preserves the original bytes after denied write, and emits `PYTHOS:CORE:SHM:READ_ONLY`, `PYTHOS:CORE:SHM:WRITE_DENIED`, and `PYTHOS:CORE:SHARED_MEMORY_HANDLES_READY`.
* `permission-validation` wraps a privileged IPC send with capability validation, proves a `SEND` right allows the operation and a handle without `SEND` is denied before enqueue, and emits `PYTHOS:CORE:PERMISSION:IPC_ALLOWED`, `PYTHOS:CORE:PERMISSION:IPC_DENIED`, and `PYTHOS:CORE:PERMISSION_VALIDATION_READY`.
* `revocation` revokes one capability handle, invalidates its stale generation, preserves another handle for the same holder, and emits `PYTHOS:CORE:CAPABILITY:REVOKE`, `PYTHOS:CORE:CAPABILITY:STALE_DENIED`, and `PYTHOS:CORE:REVOCATION_READY`.
* `negative-authorization-tests` proves a task without a valid capability is denied even when it knows the resource and requested operation, and emits `PYTHOS:CORE:CAPABILITY:KNOWN_TARGET_DENIED` and `PYTHOS:CORE:NEGATIVE_AUTHORIZATION_READY`.
* `audit-logging` records grant, use, denial, and revocation events with service identity, resource, operation, and outcome, emits `PYTHOS:CORE:AUDIT:GRANT`, `PYTHOS:CORE:AUDIT:USE`, `PYTHOS:CORE:AUDIT:DENIAL`, `PYTHOS:CORE:AUDIT:REVOCATION`, `PYTHOS:CORE:AUDIT_LOGGING_READY`, and completes Phase 3 with `PYTHOS:CORE:PHASE_3_COMPLETE`.
* `runtime-selection` records ADR 0013's custom minimal interpreter decision as the first Phase 4 runtime gate and emits `PYTHOS:CORE:RUNTIME_SELECTED`. It does not boot an interpreter or execute `INIT.PAK` payload contents.
* `init-pak-loading` validates the ADR 0014 custom-minimal runtime payload inside the already validated `INIT.PAK`, confirms the source is bounded, checksummed, and UTF-8, and emits `PYTHOS:CORE:INIT_PAK_LOADED`. It does not parse or execute the source.
* `interpreter-boot` recognizes the exact ADR 0015 `HelloService` source shape, synthesizes a fixed internal operation plan, assigns the runtime to a native task id and service identity, requires an explicit boot capability, and emits `PYTHOS:CORE:INTERPRETER_BOOTED`. It does not execute `system.*` calls or service lifecycle transitions.
* `system-api-surface` exposes ADR 0016's first `system.*` function, `system.log(message)`, validates a `LOG` capability for the runtime service identity, rejects invalid messages, emits `PYTHOS:CORE:SYSTEM:LOG`, and completes with `PYTHOS:CORE:SYSTEM_API_READY`. It does not implement service readiness.
* `value-validation` applies ADR 0017's explicit runtime value boundary to the current `system.log` argument, validates string type, length, and UTF-8, rejects raw pointer and unchecked native-struct shaped values, proves explicit host-call success/error representation, and emits `PYTHOS:CORE:VALUE_VALIDATION_READY`. It does not implement `self.ready()`, a service manager, general Python objects, exceptions, or Phase 8 copy-in/copy-out.
* `service-manager` consumes the current exact runtime plan's `self.ready()` operation, transitions the runtime service from starting to ready under a fixed service manager, rejects unknown-service and duplicate-ready transitions, emits `PYTHOS:CORE:SERVICE:READY`, and completes with `PYTHOS:CORE:SERVICE_MANAGER_READY`. It does not implement exception containment, restart policy, or async events.
* `exception-containment` records an unhandled runtime exception as a failed state on only the failing managed service, preserves an unrelated ready service, emits `PYTHOS:CORE:SERVICE:EXCEPTION`, and completes with `PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED`. It does not implement restart policy or async events.
* `service-restart` restarts a failed noncritical managed service by moving it to a fresh starting generation, then marks it ready again, emits `PYTHOS:CORE:SERVICE:RESTART`, and completes with `PYTHOS:CORE:SERVICE_RESTART_READY`. It does not implement async event delivery.
* `async-events` dispatches a fixed native event only to a ready managed service, rejects delivery to a failed service, emits `PYTHOS:CORE:SERVICE:EVENT`, and completes Phase 4 with `PYTHOS:CORE:ASYNC_EVENTS_READY`.
* `keyboard-driver` / `mouse-driver` decode fixed raw keyboard and mouse inputs only through explicit input capabilities, emit `PYTHOS:CORE:INPUT:KEYBOARD` and `PYTHOS:CORE:INPUT:MOUSE`, and complete with `PYTHOS:CORE:INPUT_DRIVERS_READY`. They do not implement the input-event service or GUI shell.
* `input-event-service` normalizes raw keyboard and mouse driver events into typed input events only for a subscriber with an input-stream capability, emits `PYTHOS:CORE:INPUT:EVENT`, and completes with `PYTHOS:CORE:INPUT_EVENT_SERVICE_READY`.
* `software-renderer` validates bounded framebuffer-target drawing primitives by filling clipped rectangles into a fixed pixel buffer, emits `PYTHOS:CORE:RENDER:RECT`, and completes with `PYTHOS:CORE:SOFTWARE_RENDERER_READY`. It does not implement fonts, compositing, surfaces, or widgets.
* `font-system` implements ADR 0019's boot-font ABI extension by passing `FONT.PSF` through explicit boot-info fields, reserving and mapping the font range, parsing PSF1 metadata, validating a bounded glyph table, emitting `PYTHOS:CORE:FONT:PSF_LOADED`, and completing with `PYTHOS:CORE:FONT_SYSTEM_READY`. It does not implement text layout, compositing, or shell widgets.
* `compositor` implements ADR 0018's typed object and presentation-binding split for the first window surfaces, composes bounded surface pixels into a bounded framebuffer target, proves clipping at target edges, emits `PYTHOS:CORE:COMPOSITOR:SURFACE` and `PYTHOS:CORE:COMPOSITOR:CLIP`, and completes with `PYTHOS:CORE:COMPOSITOR_READY`. It does not implement pointer movement, focus, widgets, or shell applications.
* `window-interaction` builds on the typed-object compositor surface with bounded cursor state, z-order focus selection, and moving the focused window by mutating only presentation binding, emits `PYTHOS:CORE:POINTER_CURSOR_READY`, `PYTHOS:CORE:WINDOW_FOCUS_READY`, and `PYTHOS:CORE:MOVABLE_WINDOWS_READY`. It does not implement widgets or shell applications.
* `widgets` adds the minimal native widget set over typed widget objects: fixed button activation and bounded text-field input editing, emits `PYTHOS:CORE:WIDGET:BUTTON` and `PYTHOS:CORE:WIDGET:TEXT_FIELD`, and completes with `PYTHOS:CORE:WIDGETS_READY`. It does not implement first-party shell applications.
* `phase-5-complete` registers the fixed application launcher, service monitor, Python console, and settings panel as capability-scoped first-party services with typed windows, renders a fixed shell screen through the compositor path, emits the four `PYTHOS:CORE:APP:*` markers, and completes Phase 5 with `PYTHOS:CORE:PHASE_5_COMPLETE`. It does not implement Phase 6 audio or cinematic boot.
* `audio-device-selection` records ADR 0020's QEMU AC97 target, scans the fixed primary PCI bus for that device, emits either `PYTHOS:CORE:AUDIO:DEVICE_SELECTED` or `PYTHOS:CORE:AUDIO:DEVICE_ABSENT`, and completes with `PYTHOS:CORE:AUDIO_DEVICE_SELECTION_READY`.
* `audio-driver` configures the selected AC97 mixer and bus-master interface when present, emits `PYTHOS:CORE:AUDIO:DRIVER`, and completes with `PYTHOS:CORE:AUDIO_DRIVER_READY`; when absent it emits `PYTHOS:CORE:AUDIO:DRIVER_SKIPPED` and still completes the slice.
* `audio-buffers` builds a page-contained PCM buffer and AC97 buffer-descriptor list for deterministic playback, emits `PYTHOS:CORE:AUDIO:BUFFER`, and completes with `PYTHOS:CORE:AUDIO_BUFFERS_READY`; when audio is absent it emits `PYTHOS:CORE:AUDIO:BUFFER_SKIPPED`.
* `pcm-playback` submits the fixed deterministic PCM asset to the selected driver, emits `PYTHOS:CORE:AUDIO:PCM_PLAYBACK`, and completes with `PYTHOS:CORE:PCM_PLAYBACK_READY`; when audio is absent it emits `PYTHOS:CORE:AUDIO:PCM_SKIPPED`.
* `audio-mixing` mixes the fixed hiss, sub-bass, and tremolo PCM layers into the bounded boot buffer, emits the three `PYTHOS:CORE:AUDIO:MIX:*` markers, and completes with `PYTHOS:CORE:AUDIO_MIXING_READY`.
* `boot-asset-storage` embeds Phase 6 visual frame, PCM shape, and timing/sync assets in PythCore without pulling persistent storage forward, emits the three `PYTHOS:CORE:BOOT_ASSET:*` markers, and completes with `PYTHOS:CORE:BOOT_ASSETS_READY`.
* `audio-visual-sync` renders the compositor-backed wake phrase `PythOS [HISS] We Are Woken` against PIT-derived sync points while audio playback is active or silently skipped, emits `PYTHOS:CORE:BOOT_VISUAL:FRAME` and `PYTHOS:CORE:BOOT_SYNC:AUDIO`, and completes with `PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY`.
* `phase-6-complete` proves the graceful audio fallback boundary, emits `PYTHOS:CORE:AUDIO:FALLBACK_ARMED` when AC97 is present or `PYTHOS:CORE:AUDIO:FALLBACK` when no audio device is configured, emits `PYTHOS:CORE:GRACEFUL_AUDIO_FALLBACK_READY`, and completes Phase 6 with `PYTHOS:CORE:PHASE_6_COMPLETE`. It does not implement persistent storage, user-configurable boot themes, physical audio hardware support, networking, AI, ring-3, or SMP.
* `block-device-driver` attaches an explicit boot ESP plus a non-boot QEMU legacy `virtio-blk` raw storage image, scans the primary PCI bus for vendor `0x1AF4` device `0x1001`, validates the legacy I/O BAR, enables I/O and bus-master command bits, reads bounded capacity and queue metadata, emits `PYTHOS:CORE:BLOCK:DEVICE_SELECTED`, and completes with `PYTHOS:CORE:BLOCK_DEVICE_READY`. It does not implement storage-service mediation, data I/O, journaling, object records, or crash recovery.
* `storage-service` makes the selected block device opaque outside the driver module and exposes a capability-gated storage facade. A service with a valid storage capability can authorize a bounded request and emits `PYTHOS:CORE:STORAGE:ACCESS_GRANTED`; wrong-holder and missing-rights attempts are denied before block access and emit `PYTHOS:CORE:STORAGE:ACCESS_DENIED`; the slice completes with `PYTHOS:CORE:STORAGE_SERVICE_READY`. It does not implement sector I/O, journaling, commit markers, recovery, or object records.
* `append-only-journal` requires a storage-service-authorized write intent to append a monotonic journal record before any write completion can be considered. It emits `PYTHOS:CORE:STORAGE:JOURNAL_APPEND` and completes with `PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY`. It does not implement checksums, commit markers, crash recovery, sector I/O, or object records.
* `checksums-and-commit-markers` adds a stable checksum over committed journal record fields plus an explicit commit marker. Missing commit markers and checksum mismatches are detected as invalid records, then the slice emits `PYTHOS:CORE:STORAGE:CHECKSUM_VALID`, `PYTHOS:CORE:STORAGE:COMMIT_MARKER`, and `PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_READY`. It does not implement crash recovery, sector I/O, or object records.
* `crash-recovery` replays only the valid committed journal prefix and rolls back an invalid tail caused by a missing commit marker or checksum mismatch. It proves an interrupted write recovers to the last committed sequence, emits `PYTHOS:CORE:STORAGE:RECOVERY_REPLAY`, `PYTHOS:CORE:STORAGE:RECOVERY_ROLLBACK`, and completes with `PYTHOS:CORE:CRASH_RECOVERY_READY`. It does not implement object records, typed-object formatting, or object browser work.
* `typed-object-format` implements ADR 0022's fixed on-disk typed object record: magic, format version, record length, stable `ObjectId`, `ObjectKind` code, object schema version, and bounded versioned field slots. It emits `PYTHOS:CORE:OBJECT:STABLE_ID`, `PYTHOS:CORE:OBJECT:VERSIONED_FIELDS`, and completes with `PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY`. It does not implement relationships, revision history, workspace objects, object browser work, or sector persistence.
* `object-relationships` records typed, queryable relationships between known typed objects, including `blocks`, `created-by`, and `depends-on`, while rejecting relationships with unknown endpoints or duplicate edges. It emits `PYTHOS:CORE:OBJECT:RELATIONSHIP`, `PYTHOS:CORE:OBJECT:RELATIONSHIP_QUERY`, and completes with `PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY`. It does not implement revision history, workspace objects, object browser work, or sector persistence.
* `revision-history` retains prior typed-object versions on update, with monotonic timestamp metadata and writer service identity from Phase 3. It emits `PYTHOS:CORE:OBJECT:REVISION_RETAINED`, `PYTHOS:CORE:OBJECT:REVISION_PROVENANCE`, and completes with `PYTHOS:CORE:REVISION_HISTORY_READY`. It does not implement workspace objects, object browser work, or sector persistence.
* `workspace-objects` records ADR 0023's first concrete persistent object kind, `WorkspaceSession`, and stores the Phase 5 shell window layout as bounded ADR 0022 fields. It emits `PYTHOS:CORE:WORKSPACE:SESSION_OBJECT`, `PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT`, and completes with `PYTHOS:CORE:WORKSPACE_OBJECTS_READY`. It does not implement object browser work, reboot persistence, or sector persistence.
* `object-browser` records ADR 0024's minimal inspection app boundary and exposes a fixed Phase 5 typed window over the current object-store substrate. It lists typed objects, inspects typed relationships, inspects revision counts, emits `PYTHOS:CORE:OBJECT_BROWSER:LIST`, `PYTHOS:CORE:OBJECT_BROWSER:DETAIL`, and completes with `PYTHOS:CORE:OBJECT_BROWSER_READY`. It does not implement reboot persistence or sector persistence.
* `save-and-restore-across-reboot` records ADR 0025's fixed checkpoint/recovery sector contract, writes and restores the Phase 7 typed workspace snapshot through virtio-blk sector I/O, and verifies a deliberately killed mid-commit boot recovers to the last committed state. It emits `PYTHOS:CORE:OBJECT_STORE:PERSISTED`, `PYTHOS:CORE:OBJECT_STORE:RESTORED`, and completes Phase 7 with `PYTHOS:CORE:PHASE_7_COMPLETE`. It does not implement a filesystem, dynamic object database, Phase 8 isolation, or any Causal Lens/Patch UI.
* `ring-3-execution` records ADR 0026's first Phase 8 hardware-isolation step: GDT user code/data selectors, a TSS `RSP0`, a DPL3-callable breakpoint gate, and fixed user code/stack pages in the current address space. It enters CPL3 with `iretq`, proves a user-originated trap by checking the ring-3 `CS`/`SS` frame, returns to the saved kernel stack, emits `PYTHOS:CORE:USER_MODE:ENTER`, `PYTHOS:CORE:USER_MODE:RETURN`, and completes with `PYTHOS:CORE:RING3_EXECUTION_READY`. It does not implement separate address spaces, a syscall ABI, user process stacks, service-local runtimes, or hostile-code containment.
* `separate-address-spaces` records ADR 0027's distinct user CR3 proof. PythCore builds a second PML4 root before the first kernel CR3 switch, validates it is distinct from the kernel root, validates the fixed proof code/stack are user-accessible while kernel text/data remain supervisor-only, switches to that root, reruns the CPL3 breakpoint proof, restores the kernel root, emits `PYTHOS:CORE:ADDRESS_SPACE:CREATED`, `PYTHOS:CORE:ADDRESS_SPACE:ISOLATED`, `PYTHOS:CORE:ADDRESS_SPACE:SWITCHED`, `PYTHOS:CORE:ADDRESS_SPACE:RESTORED`, and completes with `PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY`. It does not implement syscall entry, user process stacks, service-local runtimes, guarded shared memory, process termination, quotas, crash containment, or hostile-code capability enforcement.
* `syscall-entry` records ADR 0028's first syscall ABI. PythCore configures x86-64 `syscall`/`sysret` MSRs, enters the gate from the fixed CPL3 proof under the distinct user CR3 root, switches to a fixed kernel syscall stack, dispatches syscall number `0x5059_0001`, proves a capability-gated Phase 3 IPC send, invokes the Phase 4 `system.log` surface with `PythOS [HISS] We Are Woken`, returns through `sysretq`, emits `PYTHOS:CORE:SYSCALL:MSRS_READY`, `PYTHOS:CORE:SYSCALL:ENTER`, `PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK`, `PYTHOS:CORE:SYSCALL:SYSTEM_LOG`, `PYTHOS:CORE:SYSCALL:RETURN`, and completes with `PYTHOS:CORE:SYSCALL_ENTRY_READY`. It does not implement user process stacks, user pointer copy-in/copy-out, service-local runtimes, guarded shared memory, process termination, quotas, crash containment, or hostile-code capability enforcement.
* `user-stacks` records ADR 0029's guarded user-stack layout. PythCore reserves fixed page-aligned user stack slots with supervisor-only guard pages below usable non-executable user stack pages, maps only the usable pages into the distinct user CR3 root, validates stack and guard permissions under that root, reruns the CPL3 proof on the guarded stack pool, emits `PYTHOS:CORE:USER_STACK:ALLOCATED`, `PYTHOS:CORE:USER_STACK:GUARD_PAGE`, and completes with `PYTHOS:CORE:USER_STACKS_READY`. It does not implement dynamic user processes, user pointer copy-in/copy-out, service-local runtimes, guarded shared memory, process termination, quotas, crash containment, or hostile-code capability enforcement.
* `service-local-python-runtimes` records ADR 0030's service-local runtime-instance proof. PythCore boots two runtime instances from the validated Phase 4 source through a shared service-identity table, gives them distinct service identities, task ids, user CR3 roots, and local runtime state slots, rejects cross-service state mutation, emits `PYTHOS:CORE:RUNTIME:LOCAL_INSTANCE`, `PYTHOS:CORE:RUNTIME:ADDRESS_SPACE`, `PYTHOS:CORE:RUNTIME:STATE_ISOLATED`, and completes with `PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY`. It does not implement guarded shared memory, user pointer copy-in/copy-out, process termination, quotas, crash containment, or hostile-code capability enforcement.
* `guarded-shared-memory` records ADR 0031's Phase 8 shared-memory revalidation. PythCore binds reader and writer service identities to distinct user CR3 roots, proves a read-only shared-memory handle can still read the fixed region under those constraints, denies a cross-space write attempt through the wrong holder, verifies the region bytes remain unchanged, emits `PYTHOS:CORE:SHM:RING3_READ`, `PYTHOS:CORE:SHM:CROSS_SPACE_WRITE_DENIED`, and completes with `PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY`. It does not implement user pointer copy-in/copy-out, process termination, quotas, crash containment, or hostile-code capability enforcement.
* `process-termination` records ADR 0032's Phase 8 process lifecycle proof. PythCore tracks a fixed user process record with a task id and user CR3 root, marks it terminated without cooperation, proves it is no longer returned as runnable, reclaims the terminated user address-space page-table frames, verifies the physical allocator free-page count increases by the exact reclaimed frame count, emits `PYTHOS:CORE:PROCESS:TERMINATED`, `PYTHOS:CORE:PROCESS:UNSCHEDULABLE`, `PYTHOS:CORE:PROCESS:ADDRESS_SPACE_RECLAIMED`, and completes with `PYTHOS:CORE:PROCESS_TERMINATION_READY`. It does not implement memory quotas, CPU quotas, crash containment, or hostile-code capability enforcement.
* `memory-quotas` records ADR 0033's Phase 8 kernel-owned memory accounting proof. PythCore registers a service identity, grants an in-quota page charge, denies an over-quota page charge, verifies the denied charge does not mutate recorded usage, emits `PYTHOS:CORE:QUOTA:MEMORY_GRANTED`, `PYTHOS:CORE:QUOTA:MEMORY_DENIED`, and completes with `PYTHOS:CORE:MEMORY_QUOTAS_READY`. It does not implement CPU quotas, crash containment, or hostile-code capability enforcement.
* `cpu-quotas` records ADR 0034's Phase 8 kernel-owned CPU accounting proof. PythCore registers a service identity, records an in-quota tick charge, denies an over-quota tick charge, verifies the denied charge does not mutate recorded usage, emits `PYTHOS:CORE:QUOTA:CPU_TICK`, `PYTHOS:CORE:QUOTA:CPU_THROTTLED`, and completes with `PYTHOS:CORE:CPU_QUOTAS_READY`. It does not implement crash containment or hostile-code capability enforcement.
* `qemu-exit` replaces timeout-based success with deterministic QEMU outcome classification. The harness starts QMP, watches serial output for terminal success or panic markers, sends QMP `quit` after a terminal outcome, supports `isa-debug-exit` status decoding when available, prints `QEMU_OUTCOME <kind>`, and returns distinct exit codes for success, panic, reset, timeout, and marker-order violation.
* `framebuffer-ready` implements the post-firmware boot screen after descriptor tables are live: an embedded 8x8 diagnostic font, RGB/BGR/bitmask pixel encoding, scanline-pitch-aware bounds-checked drawing through the loader-mapped device-region virtual base, and `PYTHOS:CORE:FRAMEBUFFER_READY`.
* `milestone-1` emits `PYTHOS:CORE:MILESTONE_1_COMPLETE` after all required milestone markers have been observed in order.

The framebuffer slice was implemented ahead of memory ownership, GDT, and IDT to make boot progress visible early, then moved after `PYTHOS:CORE:IDT_READY` when those slices landed so the milestone 1 marker order is preserved.

Phase 7 `persistent-object-storage` is complete. Phase 8 `ring-3-execution`, `separate-address-spaces`, `syscall-entry`, `user-stacks`, `service-local-python-runtimes`, `guarded-shared-memory`, `process-termination`, `memory-quotas`, and `cpu-quotas` are complete. ADR 0022 records the on-disk format, ADR 0023 records the workspace-session object kind, ADR 0024 records the object-browser inspection boundary, ADR 0025 records the checkpoint/recovery sector contract, ADR 0026 records the ring-3 execution proof, ADR 0027 records the separate address-space proof, ADR 0028 records the syscall ABI, ADR 0029 records the guarded user-stack layout, ADR 0030 records the service-local runtime-instance proof, ADR 0031 records the guarded shared-memory proof, ADR 0032 records the process-termination proof, ADR 0033 records the memory-quota proof, and ADR 0034 records the CPU-quota proof. The next allowed Phase 8 slice is `crash-containment`. Do not begin networking, AI, SMP, hostile-code isolation beyond the Phase 8 slice sequence, or hardware expansion before their roadmap gates.

Until relocation support exists, the loader must reject `ET_DYN` kernel images.

## Kernel Entry Contract

Conceptual entry:

```rust
#[no_mangle]
pub unsafe extern "C" fn pythcore_entry(
    boot_info: *const PythBootInfo,
) -> !;
```

At entry, `RDI` points to `PythBootInfo`, `RSP` points to the bootstrap stack, the direction flag is clear, maskable interrupts are disabled, the kernel image is mapped, the framebuffer is mapped, the memory map remains available, no UEFI boot services remain valid, and COM1 remains directly accessible.

PythCore must immediately emit `PYTHOS:CORE:ENTER`.

## Boot Information ABI

`PythBootInfo` and `PythFramebufferInfo` live in `shared/src/boot_protocol.rs`.
ADR 0019 appends explicit `font_phys` and `font_len` fields and increments the
boot ABI minor version to 2.

Magic:

```text
0x5059_5448_424F_4F54
```

ASCII:

```text
PYTHBOOT
```

Incompatible structure changes increment `abi_major`. Compatible appended fields increment `abi_minor`. `struct_size` must be validated. Unsupported major versions must be rejected. Reserved values must initially be zero. Every address field must identify whether it is a physical address, virtual address, or firmware runtime pointer.

## Initial Virtual Address Plan

```text
0x0000_0000_0000_0000  unmapped null and low guard
0x0000_0000_0020_0000  future user image region
0xFFFF_8000_0000_0000  higher-half physical direct map
0xFFFF_C000_0000_0000  device and framebuffer mappings
0xFFFF_D000_0000_0000  kernel heap reservation
0xFFFF_E000_0000_0000  kernel stacks with guards
0xFFFF_FF00_0000_0000  page-table management reservation
0xFFFF_FFFF_8000_0000  PythCore image
```

Leave the first 2 MiB unmapped after final mappings. Enforce writable XOR executable. Do not consider direct-mapped memory automatically allocatable.

## Acceptance Test

Test name:

```text
boot_core_handoff
```

The ESP-directory QEMU path remains the default test medium. The ISO medium must
preserve the same serial oracle:

```powershell
python scripts/test-boot.py --slice milestone-1 --media iso
```

Milestone 1 success markers, in order:

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
PYTHOS:CORE:POINTER_CURSOR_READY
PYTHOS:CORE:WINDOW_FOCUS_READY
PYTHOS:CORE:MOVABLE_WINDOWS_READY
PYTHOS:CORE:WIDGET:BUTTON
PYTHOS:CORE:WIDGET:TEXT_FIELD
PYTHOS:CORE:WIDGETS_READY
PYTHOS:CORE:APP:LAUNCHER
PYTHOS:CORE:APP:SERVICE_MONITOR
PYTHOS:CORE:APP:PYTHON_CONSOLE
PYTHOS:CORE:APP:SETTINGS_PANEL
PYTHOS:CORE:PHASE_5_COMPLETE
PYTHOS:CORE:AUDIO:DEVICE_SELECTED
PYTHOS:CORE:AUDIO_DEVICE_SELECTION_READY
PYTHOS:CORE:AUDIO:DRIVER
PYTHOS:CORE:AUDIO_DRIVER_READY
PYTHOS:CORE:AUDIO:BUFFER
PYTHOS:CORE:AUDIO_BUFFERS_READY
PYTHOS:CORE:AUDIO:PCM_PLAYBACK
PYTHOS:CORE:PCM_PLAYBACK_READY
PYTHOS:CORE:AUDIO:MIX:HISS
PYTHOS:CORE:AUDIO:MIX:SUB_BASS
PYTHOS:CORE:AUDIO:MIX:TREMOLO
PYTHOS:CORE:AUDIO_MIXING_READY
PYTHOS:CORE:BOOT_ASSET:VISUAL
PYTHOS:CORE:BOOT_ASSET:PCM
PYTHOS:CORE:BOOT_ASSET:SYNC
PYTHOS:CORE:BOOT_ASSETS_READY
PYTHOS:CORE:BOOT_VISUAL:FRAME
PYTHOS:CORE:BOOT_SYNC:AUDIO
PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY
PYTHOS:CORE:AUDIO:FALLBACK_ARMED
PYTHOS:CORE:GRACEFUL_AUDIO_FALLBACK_READY
PYTHOS:CORE:PHASE_6_COMPLETE
PYTHOS:CORE:BLOCK:DEVICE_SELECTED
PYTHOS:CORE:BLOCK_DEVICE_READY
PYTHOS:CORE:STORAGE:ACCESS_GRANTED
PYTHOS:CORE:STORAGE:ACCESS_DENIED
PYTHOS:CORE:STORAGE_SERVICE_READY
PYTHOS:CORE:STORAGE:JOURNAL_APPEND
PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY
PYTHOS:CORE:STORAGE:CHECKSUM_VALID
PYTHOS:CORE:STORAGE:COMMIT_MARKER
PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_READY
PYTHOS:CORE:STORAGE:RECOVERY_REPLAY
PYTHOS:CORE:STORAGE:RECOVERY_ROLLBACK
PYTHOS:CORE:CRASH_RECOVERY_READY
PYTHOS:CORE:OBJECT:STABLE_ID
PYTHOS:CORE:OBJECT:VERSIONED_FIELDS
PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY
PYTHOS:CORE:OBJECT:RELATIONSHIP
PYTHOS:CORE:OBJECT:RELATIONSHIP_QUERY
PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY
PYTHOS:CORE:OBJECT:REVISION_RETAINED
PYTHOS:CORE:OBJECT:REVISION_PROVENANCE
PYTHOS:CORE:REVISION_HISTORY_READY
PYTHOS:CORE:WORKSPACE:SESSION_OBJECT
PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT
PYTHOS:CORE:WORKSPACE_OBJECTS_READY
PYTHOS:CORE:OBJECT_BROWSER:LIST
PYTHOS:CORE:OBJECT_BROWSER:DETAIL
PYTHOS:CORE:OBJECT_BROWSER_READY
PYTHOS:CORE:OBJECT_STORE:PERSISTED
PYTHOS:CORE:OBJECT_STORE:RESTORED
PYTHOS:CORE:PHASE_7_COMPLETE
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:RING3_EXECUTION_READY
PYTHOS:CORE:ADDRESS_SPACE:CREATED
PYTHOS:CORE:ADDRESS_SPACE:ISOLATED
PYTHOS:CORE:ADDRESS_SPACE:SWITCHED
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:ADDRESS_SPACE:RESTORED
PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY
PYTHOS:CORE:SYSCALL:MSRS_READY
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:SYSCALL:ENTER
PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK
PYTHOS:CORE:SYSTEM:LOG
PYTHOS:CORE:SYSCALL:SYSTEM_LOG
PYTHOS:CORE:SYSCALL:RETURN
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:SYSCALL_ENTRY_READY
PYTHOS:CORE:USER_STACK:ALLOCATED
PYTHOS:CORE:USER_STACK:GUARD_PAGE
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:USER_STACKS_READY
PYTHOS:CORE:RUNTIME:LOCAL_INSTANCE
PYTHOS:CORE:RUNTIME:ADDRESS_SPACE
PYTHOS:CORE:RUNTIME:STATE_ISOLATED
PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY
PYTHOS:CORE:SHM:RING3_READ
PYTHOS:CORE:SHM:CROSS_SPACE_WRITE_DENIED
PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY
PYTHOS:CORE:PROCESS:TERMINATED
PYTHOS:CORE:PROCESS:UNSCHEDULABLE
PYTHOS:CORE:PROCESS:ADDRESS_SPACE_RECLAIMED
PYTHOS:CORE:PROCESS_TERMINATION_READY
PYTHOS:CORE:QUOTA:MEMORY_GRANTED
PYTHOS:CORE:QUOTA:MEMORY_DENIED
PYTHOS:CORE:MEMORY_QUOTAS_READY
PYTHOS:CORE:QUOTA:CPU_TICK
PYTHOS:CORE:QUOTA:CPU_THROTTLED
PYTHOS:CORE:CPU_QUOTAS_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

Failure markers:

```text
PYTHOS:LOADER:FAIL
PYTHOS:PANIC
PYTHOS:CORE:BOOTINFO_INVALID
PYTHOS:CORE:MEMORY_INVALID
```

The `loader-enter` slice asserts only `PYTHOS:LOADER:ENTER` and still fails on any failure marker.

The `gop-ready` slice asserts:

```text
PYTHOS:LOADER:ENTER
PYTHOS:LOADER:GOP_READY
```

It also fails on any failure marker.

The `kernel-loaded` slice asserts:

```text
PYTHOS:LOADER:ENTER
PYTHOS:LOADER:GOP_READY
PYTHOS:LOADER:KERNEL_LOADED
```

It also fails on any failure marker.

The `memory-map-ready` slice asserts:

```text
PYTHOS:LOADER:ENTER
PYTHOS:LOADER:GOP_READY
PYTHOS:LOADER:KERNEL_LOADED
PYTHOS:LOADER:MEMORY_MAP_READY
```

It also fails on any failure marker.

The `exit-boot-services-ok` slice asserts:

```text
PYTHOS:LOADER:ENTER
PYTHOS:LOADER:GOP_READY
PYTHOS:LOADER:KERNEL_LOADED
PYTHOS:LOADER:MEMORY_MAP_READY
PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK
```

It also fails on any failure marker.

The `core-enter` slice asserts:

```text
PYTHOS:LOADER:ENTER
PYTHOS:LOADER:GOP_READY
PYTHOS:LOADER:KERNEL_LOADED
PYTHOS:LOADER:MEMORY_MAP_READY
PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK
PYTHOS:CORE:ENTER
```

It also fails on any failure marker.

The `bootinfo-valid` slice appends `PYTHOS:CORE:BOOTINFO_VALID` to the `core-enter` assertions. The `framebuffer-ready` slice appends `PYTHOS:CORE:FRAMEBUFFER_READY` to the `bootinfo-valid` assertions. Both fail on any failure marker. Slice assertions are subsequence checks, so later markers interleaving between them (for example `PYTHOS:CORE:MEMORY_READY` before `PYTHOS:CORE:FRAMEBUFFER_READY`) keep earlier slices passing.

The `exceptions-diagnostic` slice asserts the full sequence through:

```text
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY
```

The `exception-entry-hardening` slice asserts the full sequence through:

```text
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY
PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED
```

The `interrupt-controller` slice asserts the full sequence through:

```text
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY
PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED
PYTHOS:CORE:INTERRUPTS_READY
```

The `vm-ready` slice asserts the full sequence through:

```text
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY
PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED
PYTHOS:CORE:INTERRUPTS_READY
PYTHOS:CORE:VM_READY
```

The `identity-map-removed` slice asserts the full sequence through:

```text
PYTHOS:CORE:VM_READY
PYTHOS:CORE:EXPECTED_PAGE_FAULT
PYTHOS:CORE:IDENTITY_MAP_REMOVED
```

The `bootinfo-complete` slice asserts the full sequence through:

```text
PYTHOS:CORE:IDENTITY_MAP_REMOVED
PYTHOS:CORE:BOOTINFO_COMPLETE
```

The `timer` slice asserts the full sequence through:

```text
PYTHOS:CORE:BOOTINFO_COMPLETE
PYTHOS:CORE:TIMER_READY
```

The `monotonic-clock` slice asserts the full sequence through:

```text
PYTHOS:CORE:TIMER_READY
PYTHOS:CORE:CLOCK_READY
```

The `task-structures` slice asserts the full sequence through:

```text
PYTHOS:CORE:CLOCK_READY
PYTHOS:CORE:TASKS_READY
```

The `kernel-stacks` slice asserts the full sequence through:

```text
PYTHOS:CORE:TASKS_READY
PYTHOS:CORE:EXPECTED_PAGE_FAULT
PYTHOS:CORE:KERNEL_STACKS_READY
```

The `context-switch` slice asserts the full sequence through:

```text
PYTHOS:CORE:KERNEL_STACKS_READY
PYTHOS:CORE:CONTEXT_SWITCH:TASK_A
PYTHOS:CORE:CONTEXT_SWITCH:TASK_B
PYTHOS:CORE:CONTEXT_SWITCH:TASK_A
PYTHOS:CORE:CONTEXT_SWITCH:TASK_B
PYTHOS:CORE:CONTEXT_SWITCH_READY
```

The `scheduler` slice asserts the full sequence through:

```text
PYTHOS:CORE:CONTEXT_SWITCH_READY
PYTHOS:CORE:SCHEDULER:TASK_A
PYTHOS:CORE:SCHEDULER:TASK_B
PYTHOS:CORE:SCHEDULER:TASK_A
PYTHOS:CORE:SCHEDULER:TASK_B
PYTHOS:CORE:SCHEDULER_READY
```

The `idle-task` slice asserts the full sequence through:

```text
PYTHOS:CORE:SCHEDULER_READY
PYTHOS:CORE:IDLE_TASK
PYTHOS:CORE:IDLE_TASK_READY
```

The `preemption` slice asserts the full sequence through:

```text
PYTHOS:CORE:IDLE_TASK_READY
PYTHOS:CORE:PREEMPT:TASK_A
PYTHOS:CORE:PREEMPT:TASK_B
PYTHOS:CORE:PREEMPT:TASK_A
PYTHOS:CORE:PREEMPT:TASK_B
PYTHOS:CORE:PREEMPT_READY
```

The `task-termination` slice asserts the full sequence through:

```text
PYTHOS:CORE:PREEMPT_READY
PYTHOS:CORE:TASK_TERMINATED
PYTHOS:CORE:TASK_TERMINATION_READY
```

The `scheduler-tests` slice asserts the full sequence through:

```text
PYTHOS:CORE:TASK_TERMINATION_READY
PYTHOS:CORE:SCHEDTEST:TASK_A
PYTHOS:CORE:SCHEDTEST:TASK_B
PYTHOS:CORE:SCHEDTEST:TASK_C
PYTHOS:CORE:SCHEDTEST:TASK_A
PYTHOS:CORE:SCHEDTEST:TASK_B
PYTHOS:CORE:SCHEDTEST:TASK_C
PYTHOS:CORE:SCHEDULER_TESTS_READY
```

The `service-identity` slice asserts the full sequence through:

```text
PYTHOS:CORE:SCHEDULER_TESTS_READY
PYTHOS:CORE:SERVICE_IDENTITY_READY
```

The `ipc-channels` slice asserts the full sequence through:

```text
PYTHOS:CORE:SERVICE_IDENTITY_READY
PYTHOS:CORE:IPC:SEND
PYTHOS:CORE:IPC:RECV
PYTHOS:CORE:IPC_CHANNELS_READY
```

The `bounded-queues` slice asserts the full sequence through:

```text
PYTHOS:CORE:IPC_CHANNELS_READY
PYTHOS:CORE:IPC:QUEUE_FULL
PYTHOS:CORE:BOUNDED_QUEUES_READY
```

The `request-reply` slice asserts the full sequence through:

```text
PYTHOS:CORE:BOUNDED_QUEUES_READY
PYTHOS:CORE:IPC:REQUEST
PYTHOS:CORE:IPC:REPLY
PYTHOS:CORE:IPC:REPLY_TIMEOUT
PYTHOS:CORE:REQUEST_REPLY_READY
```

The `capability-handles` slice asserts the full sequence through:

```text
PYTHOS:CORE:REQUEST_REPLY_READY
PYTHOS:CORE:CAPABILITY:GRANT
PYTHOS:CORE:CAPABILITY:USE
PYTHOS:CORE:CAPABILITY_HANDLES_READY
```

The `shared-memory-handles` slice asserts the full sequence through:

```text
PYTHOS:CORE:CAPABILITY_HANDLES_READY
PYTHOS:CORE:SHM:READ_ONLY
PYTHOS:CORE:SHM:WRITE_DENIED
PYTHOS:CORE:SHARED_MEMORY_HANDLES_READY
```

The `permission-validation` slice asserts the full sequence through:

```text
PYTHOS:CORE:SHARED_MEMORY_HANDLES_READY
PYTHOS:CORE:PERMISSION:IPC_ALLOWED
PYTHOS:CORE:PERMISSION:IPC_DENIED
PYTHOS:CORE:PERMISSION_VALIDATION_READY
```

The `revocation` slice asserts the full sequence through:

```text
PYTHOS:CORE:PERMISSION_VALIDATION_READY
PYTHOS:CORE:CAPABILITY:REVOKE
PYTHOS:CORE:CAPABILITY:STALE_DENIED
PYTHOS:CORE:REVOCATION_READY
```

The `negative-authorization-tests` slice asserts the full sequence through:

```text
PYTHOS:CORE:REVOCATION_READY
PYTHOS:CORE:CAPABILITY:KNOWN_TARGET_DENIED
PYTHOS:CORE:NEGATIVE_AUTHORIZATION_READY
```

The `audit-logging` slice asserts the full sequence through:

```text
PYTHOS:CORE:NEGATIVE_AUTHORIZATION_READY
PYTHOS:CORE:AUDIT:GRANT
PYTHOS:CORE:AUDIT:USE
PYTHOS:CORE:AUDIT:DENIAL
PYTHOS:CORE:AUDIT:REVOCATION
PYTHOS:CORE:AUDIT_LOGGING_READY
PYTHOS:CORE:PHASE_3_COMPLETE
```

The `runtime-selection` slice asserts the full sequence through:

```text
PYTHOS:CORE:PHASE_3_COMPLETE
PYTHOS:CORE:RUNTIME_SELECTED
```

The `init-pak-loading` slice asserts the full sequence through:

```text
PYTHOS:CORE:RUNTIME_SELECTED
PYTHOS:CORE:INIT_PAK_LOADED
```

The `interpreter-boot` slice asserts the full sequence through:

```text
PYTHOS:CORE:INIT_PAK_LOADED
PYTHOS:CORE:INTERPRETER_BOOTED
```

The `system-api-surface` slice asserts the full sequence through:

```text
PYTHOS:CORE:INTERPRETER_BOOTED
PYTHOS:CORE:SYSTEM:LOG
PYTHOS:CORE:SYSTEM_API_READY
```

The `value-validation` slice asserts the full sequence through:

```text
PYTHOS:CORE:SYSTEM_API_READY
PYTHOS:CORE:VALUE_VALIDATION_READY
```

The `service-manager` slice asserts the full sequence through:

```text
PYTHOS:CORE:VALUE_VALIDATION_READY
PYTHOS:CORE:SERVICE:READY
PYTHOS:CORE:SERVICE_MANAGER_READY
```

The `exception-containment` slice asserts the full sequence through:

```text
PYTHOS:CORE:SERVICE_MANAGER_READY
PYTHOS:CORE:SERVICE:EXCEPTION
PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED
```

The `service-restart` slice asserts the full sequence through:

```text
PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED
PYTHOS:CORE:SERVICE:RESTART
PYTHOS:CORE:SERVICE_RESTART_READY
```

The `async-events` slice asserts the full sequence through:

```text
PYTHOS:CORE:SERVICE_RESTART_READY
PYTHOS:CORE:SERVICE:EVENT
PYTHOS:CORE:ASYNC_EVENTS_READY
```

The `keyboard-driver` and `mouse-driver` slices assert the full sequence through:

```text
PYTHOS:CORE:ASYNC_EVENTS_READY
PYTHOS:CORE:INPUT:KEYBOARD
PYTHOS:CORE:INPUT:MOUSE
PYTHOS:CORE:INPUT_DRIVERS_READY
```

The `input-event-service` slice asserts the full sequence through:

```text
PYTHOS:CORE:INPUT_DRIVERS_READY
PYTHOS:CORE:INPUT:EVENT
PYTHOS:CORE:INPUT_EVENT_SERVICE_READY
```

The `software-renderer` slice asserts the full sequence through:

```text
PYTHOS:CORE:INPUT_EVENT_SERVICE_READY
PYTHOS:CORE:RENDER:RECT
PYTHOS:CORE:SOFTWARE_RENDERER_READY
```

The `font-system` slice asserts the full sequence through:

```text
PYTHOS:CORE:SOFTWARE_RENDERER_READY
PYTHOS:CORE:FONT:PSF_LOADED
PYTHOS:CORE:FONT_SYSTEM_READY
```

The `compositor` slice asserts the full sequence through:

```text
PYTHOS:CORE:FONT_SYSTEM_READY
PYTHOS:CORE:COMPOSITOR:SURFACE
PYTHOS:CORE:COMPOSITOR:CLIP
PYTHOS:CORE:COMPOSITOR_READY
```

The `window-interaction` slice asserts the full sequence through:

```text
PYTHOS:CORE:COMPOSITOR_READY
PYTHOS:CORE:POINTER_CURSOR_READY
PYTHOS:CORE:WINDOW_FOCUS_READY
PYTHOS:CORE:MOVABLE_WINDOWS_READY
```

The `widgets` slice asserts the full sequence through:

```text
PYTHOS:CORE:MOVABLE_WINDOWS_READY
PYTHOS:CORE:WIDGET:BUTTON
PYTHOS:CORE:WIDGET:TEXT_FIELD
PYTHOS:CORE:WIDGETS_READY
```

The `phase-5-complete` slice asserts the full sequence through:

```text
PYTHOS:CORE:WIDGETS_READY
PYTHOS:CORE:APP:LAUNCHER
PYTHOS:CORE:APP:SERVICE_MONITOR
PYTHOS:CORE:APP:PYTHON_CONSOLE
PYTHOS:CORE:APP:SETTINGS_PANEL
PYTHOS:CORE:PHASE_5_COMPLETE
```

The Phase 6 slices extend the `phase-5-complete` assertion sequence in order:

```text
PYTHOS:CORE:AUDIO:DEVICE_SELECTED
PYTHOS:CORE:AUDIO_DEVICE_SELECTION_READY
PYTHOS:CORE:AUDIO:DRIVER
PYTHOS:CORE:AUDIO_DRIVER_READY
PYTHOS:CORE:AUDIO:BUFFER
PYTHOS:CORE:AUDIO_BUFFERS_READY
PYTHOS:CORE:AUDIO:PCM_PLAYBACK
PYTHOS:CORE:PCM_PLAYBACK_READY
PYTHOS:CORE:AUDIO:MIX:HISS
PYTHOS:CORE:AUDIO:MIX:SUB_BASS
PYTHOS:CORE:AUDIO:MIX:TREMOLO
PYTHOS:CORE:AUDIO_MIXING_READY
PYTHOS:CORE:BOOT_ASSET:VISUAL
PYTHOS:CORE:BOOT_ASSET:PCM
PYTHOS:CORE:BOOT_ASSET:SYNC
PYTHOS:CORE:BOOT_ASSETS_READY
PYTHOS:CORE:BOOT_VISUAL:FRAME
PYTHOS:CORE:BOOT_SYNC:AUDIO
PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY
PYTHOS:CORE:AUDIO:FALLBACK_ARMED
PYTHOS:CORE:GRACEFUL_AUDIO_FALLBACK_READY
PYTHOS:CORE:PHASE_6_COMPLETE
```

The `graceful-audio-fallback` slice runs QEMU without the AC97 device and asserts the absent-device variant:

```text
PYTHOS:CORE:AUDIO:DEVICE_ABSENT
PYTHOS:CORE:AUDIO_DEVICE_SELECTION_READY
PYTHOS:CORE:AUDIO:DRIVER_SKIPPED
PYTHOS:CORE:AUDIO_DRIVER_READY
PYTHOS:CORE:AUDIO:BUFFER_SKIPPED
PYTHOS:CORE:AUDIO_BUFFERS_READY
PYTHOS:CORE:AUDIO:PCM_SKIPPED
PYTHOS:CORE:PCM_PLAYBACK_READY
PYTHOS:CORE:AUDIO:FALLBACK
PYTHOS:CORE:GRACEFUL_AUDIO_FALLBACK_READY
PYTHOS:CORE:PHASE_6_COMPLETE
PYTHOS:CORE:BLOCK:DEVICE_SELECTED
PYTHOS:CORE:BLOCK_DEVICE_READY
PYTHOS:CORE:STORAGE:ACCESS_GRANTED
PYTHOS:CORE:STORAGE:ACCESS_DENIED
PYTHOS:CORE:STORAGE_SERVICE_READY
PYTHOS:CORE:STORAGE:JOURNAL_APPEND
PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY
PYTHOS:CORE:STORAGE:CHECKSUM_VALID
PYTHOS:CORE:STORAGE:COMMIT_MARKER
PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_READY
PYTHOS:CORE:STORAGE:RECOVERY_REPLAY
PYTHOS:CORE:STORAGE:RECOVERY_ROLLBACK
PYTHOS:CORE:CRASH_RECOVERY_READY
PYTHOS:CORE:OBJECT:STABLE_ID
PYTHOS:CORE:OBJECT:VERSIONED_FIELDS
PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY
PYTHOS:CORE:OBJECT:RELATIONSHIP
PYTHOS:CORE:OBJECT:RELATIONSHIP_QUERY
PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY
PYTHOS:CORE:OBJECT:REVISION_RETAINED
PYTHOS:CORE:OBJECT:REVISION_PROVENANCE
PYTHOS:CORE:REVISION_HISTORY_READY
PYTHOS:CORE:WORKSPACE:SESSION_OBJECT
PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT
PYTHOS:CORE:WORKSPACE_OBJECTS_READY
PYTHOS:CORE:OBJECT_BROWSER:LIST
PYTHOS:CORE:OBJECT_BROWSER:DETAIL
PYTHOS:CORE:OBJECT_BROWSER_READY
PYTHOS:CORE:OBJECT_STORE:PERSISTED
PYTHOS:CORE:OBJECT_STORE:RESTORED
PYTHOS:CORE:PHASE_7_COMPLETE
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:RING3_EXECUTION_READY
PYTHOS:CORE:ADDRESS_SPACE:CREATED
PYTHOS:CORE:ADDRESS_SPACE:ISOLATED
PYTHOS:CORE:ADDRESS_SPACE:SWITCHED
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:ADDRESS_SPACE:RESTORED
PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY
PYTHOS:CORE:SYSCALL:MSRS_READY
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:SYSCALL:ENTER
PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK
PYTHOS:CORE:SYSTEM:LOG
PYTHOS:CORE:SYSCALL:SYSTEM_LOG
PYTHOS:CORE:SYSCALL:RETURN
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:SYSCALL_ENTRY_READY
PYTHOS:CORE:USER_STACK:ALLOCATED
PYTHOS:CORE:USER_STACK:GUARD_PAGE
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:USER_STACKS_READY
PYTHOS:CORE:RUNTIME:LOCAL_INSTANCE
PYTHOS:CORE:RUNTIME:ADDRESS_SPACE
PYTHOS:CORE:RUNTIME:STATE_ISOLATED
PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY
PYTHOS:CORE:SHM:RING3_READ
PYTHOS:CORE:SHM:CROSS_SPACE_WRITE_DENIED
PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY
PYTHOS:CORE:PROCESS:TERMINATED
PYTHOS:CORE:PROCESS:UNSCHEDULABLE
PYTHOS:CORE:PROCESS:ADDRESS_SPACE_RECLAIMED
PYTHOS:CORE:PROCESS_TERMINATION_READY
PYTHOS:CORE:QUOTA:MEMORY_GRANTED
PYTHOS:CORE:QUOTA:MEMORY_DENIED
PYTHOS:CORE:MEMORY_QUOTAS_READY
PYTHOS:CORE:QUOTA:CPU_TICK
PYTHOS:CORE:QUOTA:CPU_THROTTLED
PYTHOS:CORE:CPU_QUOTAS_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

The `milestone-1` slice requires `PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY` before `PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED`, `PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED` before `PYTHOS:CORE:INTERRUPTS_READY`, `PYTHOS:CORE:INTERRUPTS_READY` before `PYTHOS:CORE:VM_READY`, `PYTHOS:CORE:IDENTITY_MAP_REMOVED` after the first expected page fault, `PYTHOS:CORE:BOOTINFO_COMPLETE` after identity-map removal, `PYTHOS:CORE:TIMER_READY` after bootinfo completion, `PYTHOS:CORE:CLOCK_READY` after timer readiness, `PYTHOS:CORE:TASKS_READY` after clock readiness, `PYTHOS:CORE:KERNEL_STACKS_READY` after the second expected page fault, `PYTHOS:CORE:CONTEXT_SWITCH_READY` after the alternating context markers, `PYTHOS:CORE:SCHEDULER_READY` after the round-robin scheduler markers, `PYTHOS:CORE:IDLE_TASK_READY` after the idle task marker, `PYTHOS:CORE:PREEMPT_READY` after the alternating preemption markers, `PYTHOS:CORE:TASK_TERMINATION_READY` after the task-termination marker, `PYTHOS:CORE:SCHEDULER_TESTS_READY` after the three-task scheduler-test markers, `PYTHOS:CORE:SERVICE_IDENTITY_READY` after scheduler tests, `PYTHOS:CORE:IPC_CHANNELS_READY` after the IPC send/receive markers, `PYTHOS:CORE:BOUNDED_QUEUES_READY` after the queue-full marker, `PYTHOS:CORE:REQUEST_REPLY_READY` after the request/reply markers, `PYTHOS:CORE:CAPABILITY_HANDLES_READY` after capability grant/use, `PYTHOS:CORE:SHARED_MEMORY_HANDLES_READY` after the shared-memory markers, `PYTHOS:CORE:PERMISSION_VALIDATION_READY` after permission validation, `PYTHOS:CORE:REVOCATION_READY` after revocation, `PYTHOS:CORE:NEGATIVE_AUTHORIZATION_READY` after the known-target denial proof, `PYTHOS:CORE:PHASE_3_COMPLETE` after audit logging, `PYTHOS:CORE:RUNTIME_SELECTED` after `PYTHOS:CORE:PHASE_3_COMPLETE`, `PYTHOS:CORE:INIT_PAK_LOADED` after `PYTHOS:CORE:RUNTIME_SELECTED`, `PYTHOS:CORE:INTERPRETER_BOOTED` after `PYTHOS:CORE:INIT_PAK_LOADED`, `PYTHOS:CORE:SYSTEM_API_READY` after `PYTHOS:CORE:SYSTEM:LOG`, `PYTHOS:CORE:VALUE_VALIDATION_READY` after `PYTHOS:CORE:SYSTEM_API_READY`, `PYTHOS:CORE:SERVICE_MANAGER_READY` after `PYTHOS:CORE:SERVICE:READY`, `PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED` after `PYTHOS:CORE:SERVICE:EXCEPTION`, `PYTHOS:CORE:SERVICE_RESTART_READY` after `PYTHOS:CORE:SERVICE:RESTART`, `PYTHOS:CORE:ASYNC_EVENTS_READY` after `PYTHOS:CORE:SERVICE:EVENT`, `PYTHOS:CORE:INPUT_DRIVERS_READY` after `PYTHOS:CORE:INPUT:MOUSE`, `PYTHOS:CORE:INPUT_EVENT_SERVICE_READY` after `PYTHOS:CORE:INPUT:EVENT`, `PYTHOS:CORE:SOFTWARE_RENDERER_READY` after `PYTHOS:CORE:RENDER:RECT`, `PYTHOS:CORE:FONT_SYSTEM_READY` after `PYTHOS:CORE:FONT:PSF_LOADED`, `PYTHOS:CORE:COMPOSITOR_READY` after `PYTHOS:CORE:COMPOSITOR:CLIP`, `PYTHOS:CORE:MOVABLE_WINDOWS_READY` after `PYTHOS:CORE:WINDOW_FOCUS_READY`, `PYTHOS:CORE:WIDGETS_READY` after `PYTHOS:CORE:WIDGET:TEXT_FIELD`, `PYTHOS:CORE:PHASE_5_COMPLETE` after `PYTHOS:CORE:APP:SETTINGS_PANEL`, and `PYTHOS:CORE:PHASE_5_COMPLETE` before `PYTHOS:CORE:FRAMEBUFFER_READY`.

Phase 6 further requires `PYTHOS:CORE:AUDIO_DEVICE_SELECTION_READY` after `PYTHOS:CORE:AUDIO:DEVICE_SELECTED`, `PYTHOS:CORE:AUDIO_DRIVER_READY` after `PYTHOS:CORE:AUDIO:DRIVER`, `PYTHOS:CORE:AUDIO_BUFFERS_READY` after `PYTHOS:CORE:AUDIO:BUFFER`, `PYTHOS:CORE:PCM_PLAYBACK_READY` after `PYTHOS:CORE:AUDIO:PCM_PLAYBACK`, `PYTHOS:CORE:AUDIO_MIXING_READY` after the three `PYTHOS:CORE:AUDIO:MIX:*` markers, `PYTHOS:CORE:BOOT_ASSETS_READY` after the three `PYTHOS:CORE:BOOT_ASSET:*` markers, `PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY` after `PYTHOS:CORE:BOOT_SYNC:AUDIO`, `PYTHOS:CORE:GRACEFUL_AUDIO_FALLBACK_READY` after the audio fallback marker, `PYTHOS:CORE:PHASE_6_COMPLETE` after graceful fallback, and `PYTHOS:CORE:PHASE_6_COMPLETE` before `PYTHOS:CORE:FRAMEBUFFER_READY`.

Phase 7 requires `PYTHOS:CORE:BLOCK:DEVICE_SELECTED` after `PYTHOS:CORE:PHASE_6_COMPLETE`, `PYTHOS:CORE:BLOCK_DEVICE_READY` after the selected-device marker, `PYTHOS:CORE:STORAGE:ACCESS_GRANTED` after `PYTHOS:CORE:BLOCK_DEVICE_READY`, `PYTHOS:CORE:STORAGE:ACCESS_DENIED` after the granted marker, `PYTHOS:CORE:STORAGE_SERVICE_READY` after the denied marker, `PYTHOS:CORE:STORAGE:JOURNAL_APPEND` after `PYTHOS:CORE:STORAGE_SERVICE_READY`, `PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY` after the journal append marker, `PYTHOS:CORE:STORAGE:CHECKSUM_VALID` after `PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY`, `PYTHOS:CORE:STORAGE:COMMIT_MARKER` after the checksum marker, `PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_READY` after the commit marker, `PYTHOS:CORE:STORAGE:RECOVERY_REPLAY` after `PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_READY`, `PYTHOS:CORE:STORAGE:RECOVERY_ROLLBACK` after the recovery replay marker, `PYTHOS:CORE:CRASH_RECOVERY_READY` after the recovery rollback marker, `PYTHOS:CORE:OBJECT:STABLE_ID` after `PYTHOS:CORE:CRASH_RECOVERY_READY`, `PYTHOS:CORE:OBJECT:VERSIONED_FIELDS` after the stable-id marker, `PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY` after the versioned-fields marker, `PYTHOS:CORE:OBJECT:RELATIONSHIP` after `PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY`, `PYTHOS:CORE:OBJECT:RELATIONSHIP_QUERY` after the relationship marker, `PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY` after the relationship-query marker, `PYTHOS:CORE:OBJECT:REVISION_RETAINED` after `PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY`, `PYTHOS:CORE:OBJECT:REVISION_PROVENANCE` after the retained marker, `PYTHOS:CORE:REVISION_HISTORY_READY` after the provenance marker, `PYTHOS:CORE:WORKSPACE:SESSION_OBJECT` after `PYTHOS:CORE:REVISION_HISTORY_READY`, `PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT` after the workspace session marker, `PYTHOS:CORE:WORKSPACE_OBJECTS_READY` after the workspace layout marker, `PYTHOS:CORE:OBJECT_BROWSER:LIST` after `PYTHOS:CORE:WORKSPACE_OBJECTS_READY`, `PYTHOS:CORE:OBJECT_BROWSER:DETAIL` after the browser-list marker, `PYTHOS:CORE:OBJECT_BROWSER_READY` after the browser-detail marker, `PYTHOS:CORE:OBJECT_STORE:PERSISTED` after `PYTHOS:CORE:OBJECT_BROWSER_READY`, `PYTHOS:CORE:OBJECT_STORE:RESTORED` after the persisted marker, `PYTHOS:CORE:PHASE_7_COMPLETE` after the restored marker, and `PYTHOS:CORE:PHASE_7_COMPLETE` before `PYTHOS:CORE:FRAMEBUFFER_READY`.

Phase 8 currently requires `PYTHOS:CORE:USER_MODE:ENTER` after `PYTHOS:CORE:PHASE_7_COMPLETE`, `PYTHOS:CORE:USER_MODE:RETURN` after the enter marker, `PYTHOS:CORE:RING3_EXECUTION_READY` after the return marker, `PYTHOS:CORE:ADDRESS_SPACE:CREATED` after `PYTHOS:CORE:RING3_EXECUTION_READY`, `PYTHOS:CORE:ADDRESS_SPACE:ISOLATED` after the created marker, `PYTHOS:CORE:ADDRESS_SPACE:SWITCHED` after the isolated marker, a second `PYTHOS:CORE:USER_MODE:ENTER`/`PYTHOS:CORE:USER_MODE:RETURN` pair after the address-space switch, `PYTHOS:CORE:ADDRESS_SPACE:RESTORED` after the second return marker, `PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY` after the restored marker, `PYTHOS:CORE:SYSCALL:MSRS_READY` after `PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY`, `PYTHOS:CORE:SYSCALL:ENTER` after MSR setup, `PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK` after syscall entry, `PYTHOS:CORE:SYSCALL:SYSTEM_LOG` after the capability check, `PYTHOS:CORE:SYSCALL:RETURN` after the system-log marker, `PYTHOS:CORE:SYSCALL_ENTRY_READY` after syscall return, `PYTHOS:CORE:USER_STACK:ALLOCATED` after `PYTHOS:CORE:SYSCALL_ENTRY_READY`, `PYTHOS:CORE:USER_STACK:GUARD_PAGE` after the allocation marker, `PYTHOS:CORE:USER_STACKS_READY` after the third Phase 8 user-mode return marker, `PYTHOS:CORE:RUNTIME:LOCAL_INSTANCE` after `PYTHOS:CORE:USER_STACKS_READY`, `PYTHOS:CORE:RUNTIME:ADDRESS_SPACE` after the local-instance marker, `PYTHOS:CORE:RUNTIME:STATE_ISOLATED` after the runtime address-space marker, `PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY` after the state-isolated marker, `PYTHOS:CORE:SHM:RING3_READ` after `PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY`, `PYTHOS:CORE:SHM:CROSS_SPACE_WRITE_DENIED` after the ring-3 read marker, `PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY` after the denied write marker, `PYTHOS:CORE:PROCESS:TERMINATED` after `PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY`, `PYTHOS:CORE:PROCESS:UNSCHEDULABLE` after the terminated marker, `PYTHOS:CORE:PROCESS:ADDRESS_SPACE_RECLAIMED` after the unschedulable marker, `PYTHOS:CORE:PROCESS_TERMINATION_READY` after the address-space-reclaimed marker, `PYTHOS:CORE:QUOTA:MEMORY_GRANTED` after `PYTHOS:CORE:PROCESS_TERMINATION_READY`, `PYTHOS:CORE:QUOTA:MEMORY_DENIED` after the memory-granted marker, `PYTHOS:CORE:MEMORY_QUOTAS_READY` after the memory-denied marker, `PYTHOS:CORE:QUOTA:CPU_TICK` after `PYTHOS:CORE:MEMORY_QUOTAS_READY`, `PYTHOS:CORE:QUOTA:CPU_THROTTLED` after the CPU-tick marker, `PYTHOS:CORE:CPU_QUOTAS_READY` after the CPU-throttled marker, and `PYTHOS:CORE:CPU_QUOTAS_READY` before `PYTHOS:CORE:FRAMEBUFFER_READY`.

`scripts/run-qemu.py --expect-outcome success` must print:

```text
QEMU_OUTCOME success
```

The runner exit-code contract is:

```text
success                 0
panic                   20
reset                   21
timeout                 22
marker-order-violation  23
```
