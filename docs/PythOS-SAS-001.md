# PythOS-SAS-001: System Architecture Specification

Status: Approved for implementation.

## Identity

Name: PythOS

Pronunciation:

```text
pie-thoss
```

The permanent wake sequence for a later graphical/audio milestone is:

```text
PythOS initiated.

sssssssssss

We are woken.
```

Do not implement audio in milestone 1.

## Vision

PythOS is a graphical operating system whose primary system and application language is Python. Python is not the first instruction executed by the processor. PythOS uses a deliberately small native foundation for firmware handoff, processor control, memory ownership, virtual memory, interrupts, scheduling, IPC primitives, capability validation, hardware access primitives, and Python runtime bootstrap.

Everything reasonably possible above that boundary belongs in isolated system services, primarily in Python.

## Architecture

```text
Hardware
-> UEFI firmware
-> PythOS UEFI loader
-> PythCore native executive
-> Python runtime environment
-> Python system services
-> Graphical shell
-> Applications, workspaces, semantic objects, automation, and optional AI
```

```text
Applications
Workspace Environment
Python Services
Python Runtime Environment
PythCore Native Executive
Hardware
```

## Binding Principles

PythOS is an operating system first. Boot, storage, desktop operation, settings, application launch, recovery, and shutdown must remain deterministic and functional without AI, cloud connectivity, semantic indexing, voice recognition, internet access, or AI-generated commands.

PythCore is a small trusted computing base. It contains only mechanisms that require privilege and protection. It does not contain semantic search, local language models, ordinary applications, workspace policy, application-specific logic, document parsing, natural-language interpretation, package recommendations, or AI agents.

System functionality is service based. Mature services have identity, dependencies, capabilities, lifecycle state, message inboxes, resource limits, restart policy, versioned interfaces, and health reporting.

Boot is progressive. Class 0 is the native survival core. Class 1 is the visible shell. Class 2 is the essential environment. Class 3 is optional or heavy services. The desktop must not block on optional services.

Authority is capability based. PythOS rejects broad ambient authority. Language models may propose actions but may not invoke privileged kernel operations, install software, access arbitrary objects, access all user data, obtain unrestricted network access, control hardware, modify security policy, or manufacture capabilities.

Files are payloads, not the whole identity of information. PythOS eventually exposes typed semantic objects with object IDs, schema versions, metadata, payload references, relations, revision parents, capability policy, and integrity data.

Workspaces represent ongoing work. A workspace groups objects, tasks, applications, permissions, window state, automation context, assistant context, and recent activity.

The system must be inspectable without AI explanation. Boot phases, service transitions, capability requests, crashes, storage transactions, package changes, update activation, recovery actions, and security-relevant denials should be recorded and exposed.

## Boundaries

Boundary A is the UEFI loader. It starts as an x86-64 EFI application, initializes diagnostics, accesses the EFI system partition, locates GOP, retrieves ACPI and SMBIOS pointers, retrieves the UEFI memory map, loads PythCore ELF, loads the initial runtime bundle, constructs `PythBootInfo`, creates temporary mappings, calls `ExitBootServices()`, enters PythCore, and never regains control.

Boundary B is PythCore. It owns physical page ownership, page allocation, virtual address spaces, descriptor tables, exceptions, interrupt dispatch, timer support, task scheduling, context switching, system calls, IPC, capabilities, runtime bootstrap, panic diagnostics, and controlled hardware primitives.

Boundary C is the initial system runtime. It eventually provides embedded Python, service registration, dependency resolution, service lifecycles, display service, input service, audio service, shell, object storage, workspaces, settings, and applications.

Do not blur these boundaries.

## Initial Target

```text
Architecture: x86-64
Firmware: UEFI
Machine: QEMU q35
Firmware implementation: OVMF
Processor count: 1
Minimum RAM: 512 MiB
Recommended RAM: 2 GiB
Graphics: UEFI GOP framebuffer
Diagnostics: COM1 serial
Kernel executable: ELF64
UEFI loader executable: PE32+ EFI application
```

Native code uses Rust with `#![no_std]` and `#![no_main]`. PythCore targets `x86_64-unknown-none`. Dependency versions must be pinned.

Assembly is allowed only for operations that require it: entry transition, stack switching, descriptor-table loads, exception stubs, context switch, control-register access, future syscall entry, and CPU instructions unavailable through reliable intrinsics.

## Security Rules

Treat all external data as hostile. Validate EFI file lengths, ELF offsets, ELF segment ranges, memory-map descriptor sizes, boot configuration lengths, pixel formats, framebuffer dimensions, arithmetic involving page counts, addresses received through boot structures, and future IPC payload lengths.

Raw pointers crossing a boundary must include address type, length, alignment, lifetime, ownership, and access permissions.

Interrupt handlers must remain small, bounded, nonblocking, and allocation-free where practical.

Capability security must be enforced below Python. During early single-runtime prototypes, capability separation may be logical rather than hostile-code secure, and documentation must say so honestly.

AI remains an untrusted optional planner outside the trusted core.

