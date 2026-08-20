# PythOS Threat Model

## Milestone 1 Security Boundary

Milestone 1 does not provide application isolation, hostile-code capability enforcement, Python runtime isolation, storage security, or network security. It proves firmware handoff and PythCore machine ownership under QEMU OVMF.

## Current Phase 10 Boundary

Phase 8 added a bounded hardware-enforced user/kernel authority proof for the
fixed syscall surface. Phase 9 generalized that proof with dynamic user ELF
loading validation, syscall ABI versioning, copy-in/copy-out range checks,
dynamic capability grants, argv/environment delivery, dynamic fault isolation,
and a process-model adversarial suite. Phase 10 generalized storage through a
journaled block allocator, dynamic object counts, fragmentation policy, storage
quotas, serialized writer tokens, and an adversarial storage suite.

This is still not a general hostile-code platform. The current proofs are
bounded acceptance paths, not a production multi-user security product. PythOS
does not yet provide arbitrary third-party application isolation, broad device
mediation, network security, partition/filesystem security, DMA isolation
across physical controllers, package trust, update security, or a generic
hardware support claim.

## PythTIG Phase 7 Boundary

PythTIG Phase 7 makes verified Pyth graph services the normal boot layer, but
it does not move source parsing, prompt parsing, semantic task authority, agent
policy, or hardware authority into PythCore. PythCore accepts canonical typed
graph packages and typed syscalls only.

Threats and mitigations:

| Threat | Mitigation |
| --- | --- |
| Malformed package | Shared verifier rejects before package mapping or ring-3 entry. |
| Compiler bug | Compiler output must pass the shared verifier, golden tests, and interpreter/native differential tests. |
| Capability forgery | Capability origin verification and caller-derived syscall validation reject copied or fabricated handles before mutation. |
| Effect reordering | The graph ABI enforces a single effect-token chain for host-visible effects. |
| Native backend drift | Interpreter/native differential suite compares status, denial class, object revision/history, and marker order. |
| Agent overreach | Task Steward is proposal-only and cannot approve or create authoritative task state without user-held authority. |
| Graph denial loop | Runtime instruction budgets and service-supervisor fault handling bound graph execution. |
| Physical backend variance | Cross-target acceptance compares unchanged package SHA-256, runtime digest, semantic markers, and target-specific evidence. |

The Rust object shell remains as an explicit maintenance and recovery fallback.
Fallback availability does not revive a desktop/window/application authority
model superseded by ADR 0066.

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
