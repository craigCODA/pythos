# PythOS Boot-Core Handoff

Date: 2026-07-12

Branch:

```text
milestone/boot-core-handoff
```

Current head:

```text
milestone 1 complete locally; see latest git commit for exact head
```

This document hands off the current implementation state for milestone 1. The project is building a native x86-64 UEFI boot path for PythOS. It is not a Linux distribution, browser simulation, Windows shell, or graphical mockup.

## Verified State

The latest QEMU/OVMF serial capture in `target/boot-serial.log` contains:

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
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

This proves:

* OVMF executes `/EFI/BOOT/BOOTX64.EFI`.
* The freestanding Rust UEFI loader initializes direct COM1 serial output.
* The loader locates and validates a directly writable GOP framebuffer.
* The loader opens the EFI system partition through the loaded image's own device handle.
* The loader reads `/PYTHOS/PYTHCORE.ELF`.
* The loader validates ELF64 `ET_EXEC` for x86-64, rejects unsupported images, rejects writable-executable load segments, allocates pages, copies file bytes, and zeroes segment tails.
* Loaded kernel segment metadata is retained for future mappings.
* The loader reads `/PYTHOS/INIT.PAK`, validates its binary header and checksum, allocates page-aligned storage for it, and records its physical range.
* The loader constructs `PythBootInfo` using the shared ABI.
* The loader discovers and validates ACPI RSDP and SMBIOS entry points from UEFI configuration tables.
* The loader captures a UEFI memory map with spare descriptor capacity.
* The loader calls `ExitBootServices()` successfully, including a stale-key retry path.
* The post-exit marker is emitted by direct serial I/O after UEFI boot services are gone.
* The loader builds temporary page tables (2 MiB-to-4 GiB identity map with the first 2 MiB unmapped, kernel segments at their ELF virtual addresses with W^X leaf permissions, framebuffer under `0xFFFF_C000_0000_0000`, guarded bootstrap stack under `0xFFFF_E000_0000_0000`).
* The loader enables `EFER.NXE`, switches `CR3` and `RSP`, passes `PythBootInfo` in `RDI`, and jumps to `pythcore_entry`.
* PythCore executes at its higher-half link address and emits `PYTHOS:CORE:ENTER` through direct COM1 output.
* PythCore validates the `PythBootInfo` ABI (host-tested logic in the shared crate) and emits `PYTHOS:CORE:BOOTINFO_VALID`.
* PythCore walks the retained UEFI memory descriptors, classifies free versus reserved 4 KiB pages, reserves required loader/core ranges, initializes a fixed bitmap allocator backing store, and emits `PYTHOS:CORE:MEMORY_READY`.
* PythCore installs a minimal 64-bit GDT with kernel code, kernel data, and TSS descriptors, reloads segment registers, loads `TR`, and emits `PYTHOS:CORE:GDT_READY`.
* PythCore installs a 256-entry IDT of panic-loop exception gates and emits `PYTHOS:CORE:IDT_READY`.
* PythCore installs per-vector CPU exception stubs with allocation-free serial diagnostics and emits `PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY`.
* PythCore preserves all general-purpose registers across handled exception entry, aligns the Rust handler call stack, proves the path with a controlled register-heavy `INT3`, and emits `PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED`.
* PythCore remaps the legacy PIC away from CPU exception vectors, masks all IRQ lines, routes external interrupt vectors `0x20..0x2F` through the IDT, and emits `PYTHOS:CORE:INTERRUPTS_READY`.
* PythCore allocates replacement page-table pages from its physical allocator, maps only the required kernel/boot/framebuffer/stack/page-table-management surfaces, switches `CR3` a second time, validates the active layout, and emits `PYTHOS:CORE:VM_READY`.
* PythCore proves the old broad loader identity map is absent by confirming a former identity-range address is untranslated, taking and recovering from the expected page fault, and emitting `PYTHOS:CORE:IDENTITY_MAP_REMOVED`.
* PythCore maps and revalidates ACPI, SMBIOS, and `INIT.PAK` metadata after the second `CR3` switch, confirms unsupported runtime-services pointers are zero, and emits `PYTHOS:CORE:BOOTINFO_COMPLETE`.
* PythCore configures a PIT-backed 100 Hz tick source, unmasks IRQ0, observes one timer interrupt, and emits `PYTHOS:CORE:TIMER_READY`.
* PythCore exposes a read-only monotonic tick counter derived from the timer source and emits `PYTHOS:CORE:CLOCK_READY`.
* PythCore initializes a fixed native task table, records the bootstrap task as running on the current kernel stack, and emits `PYTHOS:CORE:TASKS_READY`.
* PythCore records guarded kernel stack ownership for the bootstrap task, proves the guard page below the active stack faults and recovers through the diagnostic path, and emits `PYTHOS:CORE:KERNEL_STACKS_READY`.
* PythCore switches cooperatively between two fixed native contexts on separate stacks, emits alternating task markers, returns to bootstrap, and emits `PYTHOS:CORE:CONTEXT_SWITCH_READY`.
* PythCore selects fixed ready tasks with a round-robin cursor, drives the cooperative context-switch path through `TASK_A`, `TASK_B`, `TASK_A`, `TASK_B`, and emits `PYTHOS:CORE:SCHEDULER_READY`.
* PythCore proves the scheduler idle path by observing an empty ready set, switching through a fixed idle context once, returning to bootstrap, and emitting `PYTHOS:CORE:IDLE_TASK_READY`.
* PythCore proves timer-driven preemption by switching between spin-only native contexts from IRQ0 without voluntary yields, emitting alternating `PREEMPT:TASK_A` and `PREEMPT:TASK_B` markers, and returning to bootstrap with `PYTHOS:CORE:PREEMPT_READY`.
* PythCore proves task termination by switching into a fixed native task that exits, returning to bootstrap, marking its scheduler slot terminated/reclaimable, and verifying the terminated slot is not selected again.
* PythCore proves the Phase 2 scheduler exit condition with three spin-only native tasks interleaved by timer-forced preemption in deterministic `TASK_A`, `TASK_B`, `TASK_C`, `TASK_A`, `TASK_B`, `TASK_C` order.
* PythCore renders the post-firmware boot screen through the loader-mapped device-region framebuffer (embedded 8x8 font, RGB/BGR/bitmask encoding, bounds-checked writes) and emits `PYTHOS:CORE:FRAMEBUFFER_READY`.
* PythCore emits `PYTHOS:CORE:MILESTONE_1_COMPLETE` after all required milestone-1 markers are emitted in order.
* The QEMU harness observes the terminal success marker, sends QMP `quit`, prints `QEMU_OUTCOME success`, and returns success without relying on timeout termination. A live screendump can be captured with `python scripts/run-qemu.py --screendump target/boot-screen.png`.

## Current Stop Point

The system currently stops after:

```text
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

The framebuffer slice was implemented before memory/GDT/IDT for early visible boot, then moved after `PYTHOS:CORE:IDT_READY` so the milestone 1 marker order is preserved.

## Important Caveat

The loader-built page tables are still used for the initial handoff into PythCore, but PythCore now replaces them with kernel-owned page tables before framebuffer rendering. The replacement tables omit the broad 2 MiB-to-4 GiB identity map, preserve the low guard, map linker-defined kernel regions with W^X permissions, and keep only required boot metadata, firmware metadata, stack, framebuffer, and page-table-management surfaces. A controlled expected page fault against `0x0400_0000` verifies the old broad identity map is not active. Loader page-table frames are left allocated for later reclamation.

## Relevant Files

Loader:

* `boot/src/main.rs` coordinates the verified boot slices.
* `boot/src/serial.rs` provides direct COM1 output.
* `boot/src/uefi.rs` contains bounded UEFI table/protocol helpers.
* `boot/src/firmware.rs` discovers and validates ACPI and SMBIOS configuration-table entries.
* `boot/src/graphics.rs` discovers GOP and converts framebuffer metadata.
* `boot/src/elf.rs` validates and loads `PYTHCORE.ELF`.
* `boot/src/initrd.rs` loads `INIT.PAK`.
* `boot/src/memory_map.rs` captures and refreshes the UEFI memory map.
* `boot/src/boot_info.rs` allocates and populates `PythBootInfo`.
* `boot/src/exit_boot_services.rs` performs final memory-map handling and `ExitBootServices()`.

Shared ABI:

* `shared/src/boot_protocol.rs` defines `PythBootInfo`, `PythFramebufferInfo`, constants, and pixel formats.
* `shared/src/init_pak.rs` defines and validates the milestone-1.5 binary `INIT.PAK` header.

Core:

* `core/linker.ld` links PythCore into the intended higher-half image region.
* `core/src/main.rs` defines the current placeholder `pythcore_entry`.
* `core/src/boot_metadata.rs` revalidates firmware and init-bundle metadata after `VM_READY`.
* `core/src/memory/physical.rs` owns milestone-1 page classification and fixed bitmap initialization.
* `core/src/memory/virtual.rs` owns milestone-1.5 kernel page-table replacement and the second `CR3` switch.
* `core/src/architecture/x86_64/gdt.rs` installs the minimal GDT and TSS selector.
* `core/src/architecture/x86_64/idt.rs` installs the panic-loop exception IDT.
* `core/src/framebuffer.rs` renders the post-firmware boot screen.

Build and test:

* `scripts/build-image.py` builds the FAT EFI system partition tree.
* `scripts/build-iso.py` builds `target/pythos.iso` as a UEFI El Torito bootable ISO containing the required PythOS boot files.
* `scripts/run-qemu.py` starts QEMU with OVMF and serial capture.
* `scripts/test-boot.py` checks marker ordering for individual vertical slices.
* `tests/boot_core_handoff.py` runs QEMU-backed slice tests.

Docs:

* `AGENTS.md` contains permanent implementation rules.
* `docs/PythOS-SAS-001.md` records system architecture.
* `docs/PythOS-TDD-001.md` records the active milestone technical design.
* `docs/ROADMAP.md` records phased scope.
* `docs/THREAT-MODEL.md` records early security boundaries.

## Verification Commands

Known passing commands on the current host:

```powershell
cargo fmt --check
cargo build -p pythos-boot --target x86_64-unknown-uefi
cargo build -p pythos-core --target x86_64-unknown-none
cargo clippy -p pythos-boot --target x86_64-unknown-uefi -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none -- -D warnings
python -m unittest tests.test_iso_image
python scripts/build-iso.py --output target/pythos.iso
python scripts/test-boot.py --slice milestone-1 --media iso
python -m unittest tests.boot_core_handoff
python scripts/test-boot.py --slice exit-boot-services-ok
```

`make test-boot` is not the source of truth on this Windows host because `make` is not installed. Use the Python test commands directly unless a compatible `make` is added intentionally.

## Known Risks and Gaps

* `INIT.PAK` is validated but not interpreted or executed.
* The embedded 8x8 diagnostic font is a stand-in; `FONT.PSF` loading arrives with a later slice.
* Loader page-table pages are no longer active after `PYTHOS:CORE:VM_READY`, but they are not reclaimed yet.
* The physical allocator is initialized but only proves ownership state and bitmap backing; no higher-level kernel heap exists yet.
* Exception diagnostics are serial-only. The only recovery path is the narrow expected page-fault harness used by the identity-map negative proof.

## Milestone 1.5: Kernel-Owned Execution Substrate

Before timer or scheduler work, the next phase must replace transitional loader
execution state with PythCore-owned infrastructure. Step zero is keeping the
boot media byte-stable and the repository clean/tracked: generated ESP payloads
must be written in binary mode, the ISO and ESP paths must validate the same
`INIT.PAK` bytes, and the branch must remain pushed to its remote.

The `exceptions-diagnostic`, `vm-ready`, `identity-map-removed`, `bootinfo-complete`, and `qemu-exit` slices are implemented. Milestone 1.5 is complete. The Phase 2 `exception-entry-hardening`, `interrupt-controller`, `timer`, `monotonic-clock`, `task-structures`, `kernel-stacks`, `context-switch`, `scheduler`, `idle-task`, `preemption`, `task-termination`, and `scheduler-tests` slices are implemented on the current branch. Phase 2 is complete.

```text
next: Phase 3 service-identity ADR gate
```

The `vm-ready` proof now covers:

```text
current milestone-1 boot
-> PythCore allocates and owns replacement page tables
-> PythCore maps exact ELF segments
-> kernel code is executable and read-only
-> kernel read-only data is non-writable
-> kernel writable data is non-executable
-> framebuffer remains mapped and writable
-> COM1 direct I/O still works
-> boot information and memory map remain accessible
-> active kernel stack remains mapped with a guard page
-> first 2 MiB remains unmapped
-> broad loader identity mapping is absent
-> PythCore switches CR3 a second time
-> post-switch validation probe succeeds
-> PYTHOS:CORE:VM_READY
-> expected page fault proves old broad identity address is unreachable
-> PYTHOS:CORE:IDENTITY_MAP_REMOVED
-> ACPI, SMBIOS, boot-device filesystem path, and INIT.PAK validation succeed
-> PYTHOS:CORE:BOOTINFO_COMPLETE
-> framebuffer output survives the switch
-> ESP and ISO boot paths continue to pass
-> QEMU_OUTCOME success is reported without timeout termination
```

The required serial order keeps every existing loader and core marker and adds:

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
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

Recommended implementation boundary:

```rust
pub struct KernelAddressSpace {
    root_table_phys: PhysAddr,
    mappings: KernelMappings,
}

impl KernelAddressSpace {
    pub fn build(
        allocator: &mut PageAllocator,
        boot_info: &PythBootInfo,
    ) -> Result<Self, VmError>;

    pub unsafe fn activate(&self);

    pub fn validate_active_layout(
        &self,
        boot_info: &PythBootInfo,
    ) -> Result<(), VmError>;
}
```

Next, begin Phase 3 with the required ADR gate before `service-identity`: record
TCB invariants, capability token representation, and revocation semantics. Do
not implement IPC channels, bounded queues, request/reply, Python, or later
phase work while opening `service-identity`.
