# PythOS Threat Model

## Milestone 1 Security Boundary

Milestone 1 does not provide application isolation, hostile-code capability enforcement, Python runtime isolation, storage security, or network security. It proves firmware handoff and PythCore machine ownership under QEMU OVMF.

## Current Phase 8 Boundary

Phase 8 adds a bounded hardware-enforced user/kernel authority proof for the
fixed current syscall surface. It proves CPL3 entry/return, distinct user CR3
roots, guarded user stacks, guarded shared memory across address spaces, user
fault containment, and syscall-gated capability enforcement where copied
handles and hardware-resource requests are denied before privileged mutation.

This is still not a general hostile-code platform. PythOS does not yet provide
dynamic user process loading, a general userspace ABI, full copy-in/copy-out
policy, broad device mediation, network security, or arbitrary third-party
application isolation.

## Trusted Components

For milestone 1, the trusted components are:

* OVMF until `ExitBootServices()` succeeds
* `BOOTX64.EFI`
* PythCore native code
* the shared boot ABI
* QEMU as the execution platform for acceptance testing

## Untrusted Inputs

Treat as hostile:

* EFI files
* ELF headers and program headers
* boot configuration bytes
* memory-map descriptors
* framebuffer metadata
* ACPI and SMBIOS pointers
* future runtime bundles

## Rules

All external lengths, offsets, addresses, alignments, and arithmetic conversions must be checked before use.

Writable and executable mappings must be rejected.

Panic diagnostics must not allocate and must not depend on Python, storage, networking, or interrupts.

AI is outside the trusted core. A future AI service may propose structured actions but cannot directly invoke privileged kernel operations or manufacture capabilities.
