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

The active branch of work is `milestone/boot-core-handoff`.

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
```

The loader currently stops after successfully calling `ExitBootServices()` and emitting the post-exit serial marker.

Do not jump to PythCore handoff until loader-owned temporary page tables, bootstrap stack ownership, and the `RDI`/`RSP` entry contract are correct and reproducible through QEMU serial capture.

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
