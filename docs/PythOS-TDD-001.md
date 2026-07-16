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
5. Load `INIT.PAK` into page-aligned physical memory.
6. Construct temporary mappings sufficient for PythCore entry.
7. Retrieve the final memory map with spare capacity and handle a stale `ExitBootServices()` map key by reacquiring and retrying. Emit `PYTHOS:LOADER:MEMORY_MAP_READY`.
8. Call `ExitBootServices()` successfully. After success, use no UEFI boot services. Emit `PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK` through direct serial I/O.
9. Disable maskable interrupts, clear the direction flag, activate the temporary kernel page table, switch to the bootstrap stack, place `PythBootInfo` in `RDI`, and jump to PythCore.

Verified vertical slices:

* `loader-enter` implements step 1 through COM1 serial output.
* `gop-ready` implements GOP discovery, direct-framebuffer mode selection, and `PYTHOS:LOADER:GOP_READY`.
* `kernel-loaded` implements EFI filesystem access, bounded ELF64 `ET_EXEC` validation, physical page allocation for `PT_LOAD` segments, segment copy/zeroing, loaded segment metadata retention, and `PYTHOS:LOADER:KERNEL_LOADED`.
* `memory-map-ready` implements `INIT.PAK` loading, retained framebuffer/kernel/init metadata, preallocated `PythBootInfo`, UEFI memory-map capture with spare descriptor capacity, and `PYTHOS:LOADER:MEMORY_MAP_READY`.
* `exit-boot-services-ok` implements `ExitBootServices()` with one stale-map-key refresh using the retained memory-map buffer and emits `PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK` through direct serial output after firmware boot services are gone.
* `core-enter` implements loader-owned temporary page tables (a 2 MiB-to-4 GiB identity map with the first 2 MiB left unmapped, kernel segments at their ELF virtual addresses with writable-XOR-executable leaf permissions, the framebuffer under the device region, and a guarded bootstrap stack), `EFER.NXE` enablement, the `CR3`/`RSP` switch, the `RDI` boot-info argument, the jump to `pythcore_entry`, and PythCore's direct-COM1 `PYTHOS:CORE:ENTER`.
* `bootinfo-valid` implements host-tested `PythBootInfo` structure validation in the shared ABI crate plus null/alignment checking in PythCore, emitting `PYTHOS:CORE:BOOTINFO_VALID` on success and `PYTHOS:CORE:BOOTINFO_INVALID` on rejection.
* `memory-ready` implements PythCore-side UEFI memory-descriptor walking, explicit free/reserved page ownership, required loader/core range reservation, a fixed 4 KiB bitmap allocator backing store, and `PYTHOS:CORE:MEMORY_READY`.
* `gdt-ready` installs a minimal 64-bit GDT with kernel code, kernel data, and TSS descriptors, reloads segment registers, loads `TR`, and emits `PYTHOS:CORE:GDT_READY`.
* `idt-ready` installs a 256-entry IDT of panic-loop exception gates and emits `PYTHOS:CORE:IDT_READY`.
* `exceptions-diagnostic` installs per-vector exception stubs for CPU exception vectors 0 through 31 and an allocation-free diagnostic handler that reports vector, error code, `RIP`, `CS`, `RFLAGS`, `RSP`, `SS`, `CR2` for page faults, and current `CR3` over COM1 before entering the panic loop. It emits `PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY`.
* `exception-entry-hardening` extends the exception entry path to preserve all general-purpose registers, align the stack before calling Rust, expose a normalized saved-register frame to the handler, and prove handled exception return by running a controlled register-heavy `INT3` probe. It emits `PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED`.
* `interrupt-controller` remaps the legacy PIC away from CPU exception vectors, masks all IRQ lines, installs external interrupt stubs for vectors `0x20..0x2F`, exposes mask/unmask helpers for later timer work, and emits `PYTHOS:CORE:INTERRUPTS_READY`.
* `vm-ready` implements PythCore-owned replacement page tables, discovers currently active physical backing by walking the loader page tables before replacement, maps linker-defined kernel text/rodata/data regions with W^X permissions, maps boot information, memory map, `INIT.PAK`, the guarded bootstrap stack, framebuffer, and page-table frames needed for validation, switches `CR3` a second time, validates the active root, and emits `PYTHOS:CORE:VM_READY`.
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
* `qemu-exit` replaces timeout-based success with deterministic QEMU outcome classification. The harness starts QMP, watches serial output for terminal success or panic markers, sends QMP `quit` after a terminal outcome, supports `isa-debug-exit` status decoding when available, prints `QEMU_OUTCOME <kind>`, and returns distinct exit codes for success, panic, reset, timeout, and marker-order violation.
* `framebuffer-ready` implements the post-firmware boot screen after descriptor tables are live: an embedded 8x8 diagnostic font, RGB/BGR/bitmask pixel encoding, scanline-pitch-aware bounds-checked drawing through the loader-mapped device-region virtual base, and `PYTHOS:CORE:FRAMEBUFFER_READY`.
* `milestone-1` emits `PYTHOS:CORE:MILESTONE_1_COMPLETE` after all required milestone markers have been observed in order.

The framebuffer slice was implemented ahead of memory ownership, GDT, and IDT to make boot progress visible early, then moved after `PYTHOS:CORE:IDT_READY` when those slices landed so the milestone 1 marker order is preserved.

The active implementation stops after the boot screen renders and the milestone completion marker is emitted. Exception diagnostics are serial-only and allocation-free; richer fault recovery, scheduling, IPC, runtime bootstrap, and hostile-code isolation remain later work.

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

The `milestone-1` slice requires `PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY` before `PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED`, `PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED` before `PYTHOS:CORE:INTERRUPTS_READY`, `PYTHOS:CORE:INTERRUPTS_READY` before `PYTHOS:CORE:VM_READY`, `PYTHOS:CORE:IDENTITY_MAP_REMOVED` after the first expected page fault, `PYTHOS:CORE:BOOTINFO_COMPLETE` after identity-map removal, `PYTHOS:CORE:TIMER_READY` after bootinfo completion, `PYTHOS:CORE:CLOCK_READY` after timer readiness, `PYTHOS:CORE:TASKS_READY` after clock readiness, `PYTHOS:CORE:KERNEL_STACKS_READY` after the second expected page fault, `PYTHOS:CORE:CONTEXT_SWITCH_READY` after the alternating context markers, `PYTHOS:CORE:SCHEDULER_READY` after the round-robin scheduler markers, `PYTHOS:CORE:IDLE_TASK_READY` after the idle task marker, `PYTHOS:CORE:PREEMPT_READY` after the alternating preemption markers, `PYTHOS:CORE:TASK_TERMINATION_READY` after the task-termination marker, `PYTHOS:CORE:SCHEDULER_TESTS_READY` after the three-task scheduler-test markers, `PYTHOS:CORE:SERVICE_IDENTITY_READY` after scheduler tests, `PYTHOS:CORE:IPC_CHANNELS_READY` after the IPC send/receive markers, `PYTHOS:CORE:BOUNDED_QUEUES_READY` after the queue-full marker, `PYTHOS:CORE:REQUEST_REPLY_READY` after the request/reply markers, `PYTHOS:CORE:CAPABILITY_HANDLES_READY` after capability grant/use, `PYTHOS:CORE:SHARED_MEMORY_HANDLES_READY` after the shared-memory markers, `PYTHOS:CORE:PERMISSION_VALIDATION_READY` after permission validation, `PYTHOS:CORE:REVOCATION_READY` after revocation, `PYTHOS:CORE:NEGATIVE_AUTHORIZATION_READY` after the known-target denial proof, `PYTHOS:CORE:PHASE_3_COMPLETE` after audit logging, `PYTHOS:CORE:RUNTIME_SELECTED` after `PYTHOS:CORE:PHASE_3_COMPLETE`, `PYTHOS:CORE:INIT_PAK_LOADED` after `PYTHOS:CORE:RUNTIME_SELECTED`, `PYTHOS:CORE:INTERPRETER_BOOTED` after `PYTHOS:CORE:INIT_PAK_LOADED`, `PYTHOS:CORE:SYSTEM_API_READY` after `PYTHOS:CORE:SYSTEM:LOG`, `PYTHOS:CORE:VALUE_VALIDATION_READY` after `PYTHOS:CORE:SYSTEM_API_READY`, `PYTHOS:CORE:SERVICE_MANAGER_READY` after `PYTHOS:CORE:SERVICE:READY`, `PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED` after `PYTHOS:CORE:SERVICE:EXCEPTION`, and `PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED` before `PYTHOS:CORE:FRAMEBUFFER_READY`.

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
