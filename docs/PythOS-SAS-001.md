# PythOS-SAS-001: System Architecture Specification

Status: Approved for implementation.

## Identity

Name: PythOS

Pronunciation:

```text
pie-thoss
```

The permanent wake sequence for the graphical/audio milestone is:

```text
PythOS [HISS] We Are Woken
```

Do not implement audio before Phase 6. Phase 6 owns only the bounded QEMU AC97
boot-audio target; audio beyond that target remains later hardware-expansion
work.

## Vision

PythOS is an anti-desktop operating environment built around typed objects,
tasks, and explicit relationships in an authored world. PythCore owns the
privileged mechanisms for firmware handoff, memory, interrupts, scheduling,
IPC, capabilities, and device access. Higher-level work belongs in isolated
services. Pyth is the source language for native procedures and PythTIG is
the typed graph package format. [Native program architecture](decisions/0064-pyth-native-typed-instruction-graph.md)

Kai is the work session that uses the vault's durable state, procedure
versions, continuations, and evidence. The bounded Kai boot proof restores
a vault-selected task after PythCore starts, evaluates a pulse in a
protected task, and publishes a checked continuation. The authored world
projects task and object state. [Current Kai checkpoint](HANDOVER.md#current-kai-checkpoint)

## Architecture

```text
Hardware
-> UEFI firmware
-> PythOS UEFI loader
-> PythCore native executive
-> Protected native runtime and typed services
-> Vault-backed tasks, procedures, relations, and evidence
-> Projections of the authored world and optional planning services
```

## Binding Principles

PythOS is an operating system first. Boot, storage, task operation,
policy/object inspection, tool invocation, recovery, and shutdown must remain
deterministic and functional without AI, cloud connectivity, semantic indexing,
voice recognition, internet access, or AI-generated commands.

PythCore is a small trusted computing base. It contains only mechanisms that require privilege and protection. It does not contain semantic search, local language models, ordinary applications, workspace policy, application-specific logic, document parsing, natural-language interpretation, package recommendations, or AI agents.

System functionality is service based. Mature services have identity, dependencies, capabilities, lifecycle state, message inboxes, resource limits, restart policy, versioned interfaces, and health reporting.

Boot is progressive. Class 0 is the native survival core. Class 1 is the
visible task environment. Class 2 is the essential object/tool environment.
Class 3 is optional or heavy services. The active task environment must not
block on optional services.

Authority is capability based. PythOS rejects broad ambient authority. Language models may propose actions but may not invoke privileged kernel operations, install software, access arbitrary objects, access all user data, obtain unrestricted network access, control hardware, modify security policy, or manufacture capabilities.

Files are payloads, not the whole identity of information. PythOS eventually exposes typed semantic objects with object IDs, schema versions, metadata, payload references, relations, revision parents, capability policy, and integrity data.

Task environments represent ongoing work. A task environment groups typed
objects, tasks, executable tool objects, permissions, projection state,
automation context, assistant context, and recent activity.

ADR 0066 is binding for the interface model. PythOS must not adopt
applications, windows, launchers, desktops, settings panels, widgets, or
conventional file navigation as its authoritative user model. Rendering
primitives may visually resemble familiar controls, but they must project typed
objects, task state, semantic relationships, executable tool objects, and
capability-permitted actions.

[ADR 0121](decisions/0121-kai-native-interaction.md) applies the anti-desktop
boundary to Kai's relational model. Reject POSIX or Unix derivation in object
identity, relationships, runtime, or authority and reassess the affected design.

The system must be inspectable without AI explanation. Boot phases, service transitions, capability requests, crashes, storage transactions, package changes, update activation, recovery actions, and security-relevant denials should be recorded and exposed.

## Boundaries

Boundary A is the UEFI loader. It starts as an x86-64 EFI application, initializes diagnostics, accesses the EFI system partition, locates GOP, retrieves ACPI and SMBIOS pointers, retrieves the UEFI memory map, loads PythCore ELF, loads the initial runtime bundle, constructs `PythBootInfo`, creates temporary mappings, calls `ExitBootServices()`, enters PythCore, and never regains control.

Boundary B is PythCore. It owns physical page ownership, page allocation, virtual address spaces, descriptor tables, exceptions, interrupt dispatch, timer support, task scheduling, context switching, system calls, IPC, capabilities, runtime bootstrap, panic diagnostics, and controlled hardware primitives.

Boundary C is the protected system runtime. Its responsibilities include
typed procedure execution, service registration and lifecycle, object and
task access, policy inspection, and display, input, and audio services.
The deployed proof profiles and pending work are recorded in the handover.

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

PythCore enforces capability checks below the protected runtime. Document
which boundaries have been verified with hostile code and which provide
only logical isolation.

AI remains an untrusted optional planner outside the trusted core.
