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
PYTHOS:CORE:VM_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

This proves:

* OVMF executes `/EFI/BOOT/BOOTX64.EFI`.
* The freestanding Rust UEFI loader initializes direct COM1 serial output.
* The loader locates and validates a directly writable GOP framebuffer.
* The loader opens the EFI system partition in the current QEMU single-disk setup.
* The loader reads `/PYTHOS/PYTHCORE.ELF`.
* The loader validates ELF64 `ET_EXEC` for x86-64, rejects unsupported images, rejects writable-executable load segments, allocates pages, copies file bytes, and zeroes segment tails.
* Loaded kernel segment metadata is retained for future mappings.
* The loader reads `/PYTHOS/INIT.PAK`, allocates page-aligned storage for it, and records its physical range.
* The loader constructs `PythBootInfo` using the shared ABI.
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
* PythCore allocates replacement page-table pages from its physical allocator, maps only the required kernel/boot/framebuffer/stack/page-table-management surfaces, switches `CR3` a second time, validates the active layout, and emits `PYTHOS:CORE:VM_READY`.
* PythCore renders the post-firmware boot screen through the loader-mapped device-region framebuffer (embedded 8x8 font, RGB/BGR/bitmask encoding, bounds-checked writes) and emits `PYTHOS:CORE:FRAMEBUFFER_READY`.
* PythCore emits `PYTHOS:CORE:MILESTONE_1_COMPLETE` after all required milestone-1 markers are emitted in order. A live screendump can be captured with `python scripts/run-qemu.py --screendump target/boot-screen.png`.

## Current Stop Point

The system currently stops after:

```text
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

The framebuffer slice was implemented before memory/GDT/IDT for early visible boot, then moved after `PYTHOS:CORE:IDT_READY` so the milestone 1 marker order is preserved.

## Important Caveat

The loader-built page tables are still used for the initial handoff into PythCore, but PythCore now replaces them with kernel-owned page tables before framebuffer rendering. The replacement tables omit the broad 2 MiB-to-4 GiB identity map, preserve the low guard, map linker-defined kernel regions with W^X permissions, and keep only required boot metadata, stack, framebuffer, and page-table-management surfaces. Loader page-table frames are left allocated for later reclamation.

## Relevant Files

Loader:

* `boot/src/main.rs` coordinates the verified boot slices.
* `boot/src/serial.rs` provides direct COM1 output.
* `boot/src/uefi.rs` contains bounded UEFI table/protocol helpers.
* `boot/src/graphics.rs` discovers GOP and converts framebuffer metadata.
* `boot/src/elf.rs` validates and loads `PYTHCORE.ELF`.
* `boot/src/initrd.rs` loads `INIT.PAK`.
* `boot/src/memory_map.rs` captures and refreshes the UEFI memory map.
* `boot/src/boot_info.rs` allocates and populates `PythBootInfo`.
* `boot/src/exit_boot_services.rs` performs final memory-map handling and `ExitBootServices()`.

Shared ABI:

* `shared/src/boot_protocol.rs` defines `PythBootInfo`, `PythFramebufferInfo`, constants, and pixel formats.

Core:

* `core/linker.ld` links PythCore into the intended higher-half image region.
* `core/src/main.rs` defines the current placeholder `pythcore_entry`.
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

* The EFI filesystem is currently discovered through `LocateProtocol()` rather than `LoadedImageProtocol -> DeviceHandle -> SimpleFileSystemProtocol`. This is acceptable for the current single-disk QEMU slice, but should be fixed before multi-disk assumptions are introduced.
* QEMU termination is still timeout-based. Milestone completion should move toward a deterministic debug-exit device or controlled shutdown path.
* ACPI RSDP and SMBIOS discovery are not yet populated in `PythBootInfo`.
* `INIT.PAK` is loaded but not interpreted or format-validated.
* The embedded 8x8 diagnostic font is a stand-in; `FONT.PSF` loading arrives with a later slice.
* Loader page-table pages are no longer active after `PYTHOS:CORE:VM_READY`, but they are not reclaimed yet.
* The physical allocator is initialized but only proves ownership state and bitmap backing; no higher-level kernel heap exists yet.
* The IDT routes exceptions to a minimal panic loop. Detailed exception diagnostics and recovery are later work.

## Milestone 1.5: Kernel-Owned Execution Substrate

Before timer or scheduler work, the next phase must replace transitional loader
execution state with PythCore-owned infrastructure. Step zero is keeping the
boot media byte-stable and the repository clean/tracked: generated ESP payloads
must be written in binary mode, the ISO and ESP paths must validate the same
`INIT.PAK` bytes, and the branch must remain pushed to its remote.

The `vm-ready` slice is implemented. The remaining locked sequence is:

```text
exceptions-diagnostic
bootinfo-complete
qemu-exit
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
-> framebuffer output survives the switch
-> ESP and ISO boot paths continue to pass
```

The required serial order keeps every existing loader and core marker and adds:

```text
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:VM_READY
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

Next, add allocation-free exception diagnostics for INT3, invalid
opcode, page fault, general-protection fault, and double-fault containment. Then
complete ACPI RSDP, SMBIOS, boot-device filesystem resolution, and `INIT.PAK`
validation. Finally, replace timeout-based QEMU termination with deterministic
debug-exit outcomes for success, panic, unexpected reset, timeout, and marker
ordering failure.

Only after Milestone 1.5 should timer interrupts, native tasks, and scheduling
begin.
