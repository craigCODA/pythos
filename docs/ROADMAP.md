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

## Later Phases

Phase 2 adds timer and native tasks. Phase 3 adds IPC and capabilities. Phase 4 embeds the first Python runtime. Phase 5 builds input and graphical shell. Phase 6 adds early native audio and the wake identity. Later phases add object storage, hardware-enforced isolation, package management, updates and recovery, networking, semantic indexing, optional local AI, and controlled physical hardware support.

Deferred during early milestones: broad laptop compatibility, Wi-Fi, Bluetooth, accelerated 3D graphics, Windows or Linux binary compatibility, POSIX completeness, web browser, cloud account system, package marketplace, unrestricted AI control, voice recognition, SMP, hibernation, production secure boot, and full formal verification.

