# PythOS Boot-Core Handoff

Date: 2026-07-12

Branch:

```text
milestone/boot-core-handoff
```

Current head:

```text
3501f1f boot: exit UEFI boot services
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

## Current Stop Point

PythCore is loaded into memory, but it is not executing yet.

The loader currently stops after:

```text
PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK
```

The following required markers are not implemented or verified yet:

```text
PYTHOS:CORE:ENTER
PYTHOS:CORE:BOOTINFO_VALID
PYTHOS:CORE:MEMORY_READY
PYTHOS:CORE:GDT_READY
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

Do not claim milestone 1 completion until all required markers are emitted in order from a clean QEMU run.

## Important Caveat

The loader exits UEFI boot services before it has created and activated loader-owned temporary kernel page tables or switched to a kernel bootstrap stack.

The current post-exit serial marker is a real machine-control milestone, but it is not sufficient for PythCore entry. The next implementation must build the entry mappings and stack contract before jumping to the higher-half kernel entry point.

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
* Most core memory, GDT, TSS, IDT, exception, panic, and framebuffer modules remain future milestone-1 work.

Build and test:

* `scripts/build-image.py` builds the FAT EFI system partition tree.
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
python -m unittest tests.boot_core_handoff
python scripts/test-boot.py --slice exit-boot-services-ok
```

`make test-boot` is not the source of truth on this Windows host because `make` is not installed. Use the Python test commands directly unless a compatible `make` is added intentionally.

## Known Risks and Gaps

* The EFI filesystem is currently discovered through `LocateProtocol()` rather than `LoadedImageProtocol -> DeviceHandle -> SimpleFileSystemProtocol`. This is acceptable for the current single-disk QEMU slice, but should be fixed before multi-disk assumptions are introduced.
* QEMU termination is still timeout-based. Milestone completion should move toward a deterministic debug-exit device or controlled shutdown path.
* ACPI RSDP and SMBIOS discovery are not yet populated in `PythBootInfo`.
* `INIT.PAK` is loaded but not interpreted or format-validated.
* No loader-owned temporary page tables exist yet.
* No kernel bootstrap stack switch exists yet.
* No transition to `pythcore_entry` exists yet.
* PythCore has not validated `PythBootInfo`.
* PythCore has not converted UEFI memory descriptors into page ownership states.
* The physical page allocator, GDT, TSS, IDT, exception path, panic path, and post-UEFI framebuffer renderer remain unimplemented.

## Next Vertical Slice

The next smallest testable goal is:

```text
retain current verified loader behavior
-> create loader-owned temporary page tables
-> map the loaded PythCore segments at their ELF virtual addresses
-> map PythBootInfo
-> map the retained UEFI memory-map copy
-> map INIT.PAK
-> map the framebuffer under the planned device region
-> allocate and map a bootstrap stack
-> switch CR3 and RSP
-> clear DF and disable maskable interrupts
-> pass PythBootInfo in RDI
-> jump to pythcore_entry
-> emit PYTHOS:CORE:ENTER
```

Recommended test flow:

1. Add a `core-enter` slice to `scripts/test-boot.py` and `tests/boot_core_handoff.py` that expects all current loader markers plus `PYTHOS:CORE:ENTER`.
2. Make that test fail for the expected reason.
3. Implement temporary page-table construction and the assembly handoff.
4. Add direct COM1 output in PythCore before any allocator, GDT, IDT, or framebuffer work.
5. Rerun the exact failing `core-enter` test until it passes.

Do not proceed to memory ownership, GDT, IDT, or framebuffer rendering until `PYTHOS:CORE:ENTER` is reproducible through QEMU serial capture.
