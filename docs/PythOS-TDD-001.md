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
* `kernel-loaded` implements EFI filesystem access, bounded ELF64 validation, physical page allocation for `PT_LOAD` segments, segment copy/zeroing, and `PYTHOS:LOADER:KERNEL_LOADED`.

The active implementation still stops before `INIT.PAK` loading, memory-map handoff, temporary page-table construction, and `ExitBootServices()`.

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
