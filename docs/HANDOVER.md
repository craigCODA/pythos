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
* PythCore renders the post-firmware boot screen through the loader-mapped device-region framebuffer (embedded 8x8 font, RGB/BGR/bitmask encoding, bounds-checked writes) and emits `PYTHOS:CORE:FRAMEBUFFER_READY`.
* PythCore emits `PYTHOS:CORE:MILESTONE_1_COMPLETE` after all required milestone-1 markers are emitted in order. A live screendump can be captured with `python scripts/run-qemu.py --screendump target/boot-screen.png`.

## Current Stop Point

The system currently stops after:

```text
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

The framebuffer slice was implemented before memory/GDT/IDT for early visible boot, then moved after `PYTHOS:CORE:IDT_READY` so the milestone 1 marker order is preserved.

## Important Caveat

The loader-built page tables are still transitional. The 2 MiB-to-4 GiB identity map is writable and executable because the loader itself executes from it across the `CR3` switch; only the kernel image mappings enforce writable XOR executable. The milestone-1 memory slice classifies page ownership and initializes a bitmap allocator, but it does not yet replace the loader page tables with kernel-owned mappings.

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
* `core/src/architecture/x86_64/gdt.rs` installs the minimal GDT and TSS selector.
* `core/src/architecture/x86_64/idt.rs` installs the panic-loop exception IDT.
* `core/src/framebuffer.rs` renders the post-firmware boot screen.

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
* The temporary identity map is writable and executable during the transition; kernel-owned tables replace it in a later slice.
* The embedded 8x8 diagnostic font is a stand-in; `FONT.PSF` loading arrives with a later slice.
* PythCore has not replaced the loader-built page tables with final kernel-owned mappings.
* The physical allocator is initialized but only proves ownership state and bitmap backing; no higher-level kernel heap exists yet.
* The IDT routes exceptions to a minimal panic loop. Detailed exception diagnostics and recovery are later work.

## Next Vertical Slice

The next smallest testable goal is:

```text
retain current verified milestone-1 behavior
-> replace loader-built temporary mappings with kernel-owned page tables
-> keep null/low guard and W^X kernel/device mappings
-> keep serial and framebuffer diagnostics working
-> preserve all milestone-1 markers in QEMU serial capture
```

Recommended test flow:

1. Add a page-table replacement slice that expects all current milestone-1 markers.
2. Make the test fail for the expected reason.
3. Build kernel-owned mappings from the page ownership model, with host-testable pure layout logic where practical.
4. Rerun the exact failing test until it passes.
