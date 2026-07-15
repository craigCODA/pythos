# PythOS Agent Instructions

Read `docs/PythOS-SAS-001.md` and `docs/PythOS-TDD-001.md` before editing.

Implement only the active milestone.

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

The active branch of work is `milestone/exception-entry-hardening`.

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
-> PYTHOS:CORE:VM_READY
-> PYTHOS:CORE:EXPECTED_PAGE_FAULT
-> PYTHOS:CORE:IDENTITY_MAP_REMOVED
-> PYTHOS:CORE:BOOTINFO_COMPLETE
-> PYTHOS:CORE:FRAMEBUFFER_READY
-> PYTHOS:CORE:MILESTONE_1_COMPLETE
```

The loader builds temporary page tables, switches to the bootstrap stack, and jumps to `pythcore_entry` with `PythBootInfo` in `RDI`. PythCore validates the boot ABI, owns physical page classification, installs GDT/TSS/IDT structures, installs allocation-free exception diagnostics, verifies full-register exception-entry preservation through a controlled `INT3`, builds replacement kernel-owned page tables, switches `CR3` a second time, proves an address from the old broad loader identity range now faults, revalidates ACPI/SMBIOS/INIT.PAK boot metadata, renders the post-firmware boot screen, emits `PYTHOS:CORE:MILESTONE_1_COMPLETE`, and reaches deterministic QEMU termination.

Milestone 1.5: kernel-owned execution substrate is complete. Phase 2 has begun with `exception-entry-hardening`. Do not begin timer, scheduler, IPC, Python runtime, desktop, audio, storage, networking, AI, or hardware-expansion work until the Phase 2 prerequisite hardening slice is verified and merged. After that, the next locked slice is `interrupt-controller`.

For `vm-ready`, PythCore builds and owns replacement page tables, switches `CR3` a second time, removes the broad loader identity mapping from active translation, keeps the first 2 MiB unmapped, preserves W^X kernel mappings, retains framebuffer and COM1 access, keeps boot information and the memory map accessible, retains a guarded active kernel stack, and emits `PYTHOS:CORE:VM_READY` only after post-switch validation. The follow-up `identity-map-removed` proof deliberately reads from an address that should only have been reachable through the old broad identity map, recovers from the expected page fault, and emits `PYTHOS:CORE:IDENTITY_MAP_REMOVED`. Loader page-table frames are not reclaimed in this slice.

The required Milestone 1.5 marker order extends the existing sequence with:

```text
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY
PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED
PYTHOS:CORE:VM_READY
PYTHOS:CORE:EXPECTED_PAGE_FAULT
PYTHOS:CORE:IDENTITY_MAP_REMOVED
PYTHOS:CORE:BOOTINFO_COMPLETE
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
