# PythOS Roadmap

## Phase 0: Reproducible Environment

Deliver a repository, pinned toolchain, clean build, QEMU script, OVMF discovery, EFI image builder, serial capture, automated smoke test, and contributor instructions.

Exit condition:

```text
A clean checkout can build and launch the EFI loader.
```

## Phase 1: Boot Core Handoff

Deliver the UEFI loader, GOP discovery, ELF loading, `PythBootInfo`, memory-map handoff, `ExitBootServices()`, PythCore entry, physical page ownership, bitmap allocator, GDT, TSS, IDT, panic path, post-firmware framebuffer, and serial acceptance test.

Exit condition:

```text
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

## Phase 1.5: Kernel-Owned Execution Substrate

Before timer and scheduler work, replace transitional loader execution state with
PythCore-owned infrastructure. The locked sequence is:

```text
vm-ready
exceptions-diagnostic
bootinfo-complete
qemu-exit
```

The `vm-ready` slice builds final kernel page tables inside PythCore, switches
`CR3` a second time, removes the broad loader identity map, keeps the first
2 MiB unmapped, preserves W^X kernel mappings, keeps framebuffer and COM1
usable, and emits `PYTHOS:CORE:VM_READY` only after post-switch validation.
This slice is implemented.

The `identity-map-removed` proof verifies the old broad loader identity map is
absent by checking that `0x0400_0000` is untranslated, deliberately reading it,
recovering from the expected page fault, and emitting
`PYTHOS:CORE:IDENTITY_MAP_REMOVED`. This slice is implemented.

The `exceptions-diagnostic` slice makes fault reports actionable without relying
on allocation or locks. This slice is implemented. The remaining
`bootinfo-complete` slice fills and validates ACPI,
SMBIOS, boot-device filesystem resolution, and `INIT.PAK` metadata. The
`qemu-exit` slice replaces timeout-based test termination with deterministic
success, panic, reset, timeout, and marker-order outcomes.

Exit condition:

```text
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY
PYTHOS:CORE:VM_READY
PYTHOS:CORE:EXPECTED_PAGE_FAULT
PYTHOS:CORE:IDENTITY_MAP_REMOVED
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

## Later Phases

Phase 2 adds timer and native tasks on top of the kernel-owned execution substrate. Phase 3 adds IPC and capabilities. Phase 4 embeds the first Python runtime. Phase 5 builds input and graphical shell. Phase 6 adds early native audio and the wake identity. Later phases add object storage, hardware-enforced isolation, package management, updates and recovery, networking, semantic indexing, optional local AI, and controlled physical hardware support.

Deferred during early milestones: broad laptop compatibility, Wi-Fi, Bluetooth, accelerated 3D graphics, Windows or Linux binary compatibility, POSIX completeness, web browser, cloud account system, package marketplace, unrestricted AI control, voice recognition, SMP, hibernation, production secure boot, and full formal verification.

## Parking Lot

Datacenter capability brokering, remote workload orchestration, and cluster or
tenant policy integration are long-term research directions only. They must not
alter Milestone 1.5, Milestone 2, or Milestone 3 scope. Revisit them only after
local IPC and kernel-enforced capabilities are implemented, tested, and boring.
