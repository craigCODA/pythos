# ADR 0005: Complete Boot Metadata Through the Boot Device Handle

Date: 2026-07-14

## Status

Accepted

## Context

Milestone 1 proved the boot handoff, but several boot metadata fields were
either zero or loosely sourced. The loader opened files by locating the first
Simple File System protocol exposed by firmware, which is acceptable in a
single-disk QEMU setup but not a durable boot-device rule. `INIT.PAK` was also
only a magic string, which would become ambiguous once Phase 4 starts loading a
runtime payload.

## Decision

The loader resolves the filesystem through its own loaded image handle:

```text
EFI_LOADED_IMAGE_PROTOCOL -> DeviceHandle -> EFI_SIMPLE_FILE_SYSTEM_PROTOCOL
```

ACPI RSDP and SMBIOS entry points are discovered from UEFI configuration tables
while boot services are available. The loader validates their entry-point
checksums, passes physical addresses through `PythBootInfo`, and zeros
unsupported runtime-services pointers.

`INIT.PAK` is now a binary header with magic, major/minor version, declared
header length, declared total length, payload length, payload checksum, and
zeroed reserved bytes. The loader validates it before copying, and PythCore
revalidates it after the second `CR3` switch.

PythCore maps the RSDP, chosen ACPI root system-description table, SMBIOS entry
point, and init bundle under the kernel-owned page tables and emits
`PYTHOS:CORE:BOOTINFO_COMPLETE` only after revalidation succeeds.

## Consequences

The boot path now reads from the actual boot device, not an arbitrary firmware
filesystem. The boot metadata boundary is explicit enough for later slices to
consume without silently changing the ABI. `INIT.PAK` payload execution remains
out of scope until Phase 4.
