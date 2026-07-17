# PythOS: Verification-Driven Operating System Prototype

PythOS is an x86-64 operating-system prototype built around one rule: claims
about the system must be backed by executable evidence. It boots through UEFI,
takes ownership of memory and execution from firmware, builds a native PythCore
execution substrate, brings up service identity and capability mechanisms,
persists typed objects across reboot, and finishes with a bounded
hardware-enforced ring-3 authority-boundary proof.

This is not a README and not a setup guide. It is the external-facing technical
account of what the current repository proves, how those claims are verified,
and where the boundary of the work still is.

## Status At A Glance

| Verified now | Not yet claimed |
| --- | --- |
| UEFI-to-PythCore handoff | General-purpose desktop |
| Kernel-owned page tables | Full Python compatibility |
| Timer and scheduler proofs | Dynamic application platform |
| Capability enforcement proofs | General filesystem |
| Bounded GUI and audio proofs | Networking |
| Persistent checkpoint proof | Scalable object database |
| Fixed ring-3 boundary proof | Arbitrary third-party programs |

## What PythOS Is

PythOS is intended to be a graphical operating system whose primary system and
application language is Python. Python is not the first instruction executed by
the processor. A small native executive, PythCore, owns the privileged machinery:
firmware handoff, page tables, exceptions, interrupts, scheduling, IPC,
capability validation, syscall entry, and controlled hardware primitives.

The architecture is deliberately layered:

```text
Hardware
-> UEFI firmware
-> PythOS UEFI loader
-> PythCore native executive
-> Python runtime environment (currently a custom-minimal proof runtime)
-> Python system services (currently bounded service proofs)
-> Graphical shell
-> Applications, workspaces, semantic objects, automation, and optional AI
```

The project has completed the roadmap's bounded architecture proofs through
Phase 8, `real-hardware-isolation`. Later phases such as networking, package
management, updates, physical hardware expansion, SMP, semantic indexing, and
optional AI remain intentionally unimplemented.

## Development Method

This repository was built through agent-assisted implementation sessions under
human architectural direction. The important point is not that an agent wrote
large parts of the code; the important point is that the repo treats the live
tree, ADRs, tests, QEMU serial logs, and marker contracts as the source of
truth. Handover text and chat summaries are explicitly subordinate to live
verification.

The project style is vertical-slice driven:

* each slice has a narrow scope boundary;
* ABI-relevant decisions are recorded as ADRs;
* serial output is the boot oracle;
* a successful compile is not considered a successful boot;
* QEMU must report `QEMU_OUTCOME success`;
* every unsafe block documents the invariant it relies on.

That method matters because it makes the current claims inspectable instead of
aspirational.

## What It Proves

### Firmware Handoff And Kernel Ownership

PythOS starts as a UEFI application, loads `PYTHCORE.ELF`, builds boot metadata,
captures the UEFI memory map, exits boot services, switches to the bootstrap
stack, and enters PythCore with a validated `PythBootInfo`.

PythCore then validates the boot ABI, classifies physical memory, installs GDT,
TSS, and IDT structures, configures allocation-free diagnostics, and replaces
the loader's transitional page tables with kernel-owned mappings. A negative
proof deliberately touches an address that should only have existed in the old
loader identity map and accepts success only when the expected page fault is
observed and recovered.

The serial oracle includes markers such as:

```text
PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK
PYTHOS:CORE:BOOTINFO_VALID
PYTHOS:CORE:VM_READY
PYTHOS:CORE:EXPECTED_PAGE_FAULT
PYTHOS:CORE:IDENTITY_MAP_REMOVED
```

### Native Execution Substrate

The kernel proves timer-backed execution, a monotonic tick source, fixed native
task structures, guarded kernel stacks, cooperative context switching,
round-robin scheduling, an idle task, timer-forced preemption, task termination,
and deterministic scheduler interleaving. The preemption proofs are serial
ordered, not inferred from screenshots or timeouts.

Representative markers:

```text
PYTHOS:CORE:PREEMPT:TASK_A
PYTHOS:CORE:PREEMPT:TASK_B
PYTHOS:CORE:PREEMPT_READY
PYTHOS:CORE:SCHEDULER_TESTS_READY
```

### Service Identity, IPC, And Capabilities

Phase 3 establishes service identity independent of task slots, bounded IPC
queues, request/reply behavior, kernel-owned capability handles, shared-memory
handles, permission validation, revocation, negative authorization tests, and
audit logging.

The important claim is not "there is an IPC API." The important claim is that a
service knowing the target resource and operation still cannot act without a
valid capability handle. That claim is verified before Phase 8 as a logical
kernel-mode property, then rechecked later at the hardware boundary.

Representative markers:

```text
PYTHOS:CORE:CAPABILITY:GRANT
PYTHOS:CORE:CAPABILITY:USE
PYTHOS:CORE:PERMISSION:IPC_DENIED
PYTHOS:CORE:CAPABILITY:KNOWN_TARGET_DENIED
PYTHOS:CORE:AUDIT:DENIAL
```

### Runtime And Service Surface

PythOS currently uses a deliberately small custom-minimal interpreter path, not
a full Python implementation. The Phase 4 runtime bundle is validated from
`INIT.PAK`, booted as a capability-scoped runtime task, and allowed to invoke
only the current bounded `system.log` host surface with explicit value
validation.

The wake/system-log message is:

```text
PythOS [HISS] We Are Woken
```

The service-manager proofs add readiness, exception containment, restart, and
async event delivery. This is still a bounded runtime/service proof, not a
general Python runtime or package system.

### GUI, Audio, And Persistent Typed Objects

Phase 5 adds the first graphical shell proofs: input decoding, typed input
events, a software renderer, PSF font handling, compositor surfaces, pointer
and window interaction, widgets, and four first-party app windows. ADR 0018 is
the key design decision: object identity is separated from presentation
binding, so moving a window mutates presentation state without changing the
object.

Phase 6 adds a bounded cinematic boot/audio path using QEMU AC97 and an explicit
no-audio fallback. The wake phrase is rendered and synchronized with the boot
audio path when audio exists, and the fallback path still completes the
milestone when no AC97 device is configured.

Phase 7 adds persistent object storage. It includes a block-device target,
capability-gated storage service, append-only journal, checksums and commit
markers, crash recovery, an on-disk typed-object format, typed relationships,
revision history, workspace-session objects, an object browser, and an
end-to-end save/restore proof across QEMU reboot. It also includes a torn-write
test that kills QEMU during the commit window and verifies recovery to the last
consistent state.

Representative storage markers:

```text
PYTHOS:CORE:OBJECT_STORE:PERSISTED
PYTHOS:CORE:OBJECT_STORE:RESTORED
PYTHOS:CORE:OBJECT_STORE:TORN_WRITE_RECOVERED
PYTHOS:CORE:PHASE_7_COMPLETE
```

### Hardware-Enforced Ring-3 Boundary

Phase 8 is the major security boundary shift. Before Phase 8, capability
separation was architecturally enforced but still ran in kernel mode. Phase 8
makes the proof hardware-backed for the fixed current surface.

The sequence includes:

* CPL3 entry and return through a controlled trap;
* distinct user CR3 roots;
* x86-64 `syscall`/`sysret` entry;
* guarded user stacks;
* service-local runtime instances with distinct roots and state slots;
* guarded shared memory across distinct user roots;
* process termination and address-space reclamation;
* memory and CPU quota checks;
* user-mode crash containment;
* final syscall-boundary capability enforcement.

The final adversarial boundary proof is recorded in ADR 0036. It proves:

* a fixed CPL3 bad-pointer read is contained as a user fault;
* a legitimate syscall-gated capability is accepted before privileged IPC
  mutation;
* a copied handle value used by the wrong service identity is denied with
  `WrongHolder` before IPC mutation;
* a hardware-port-style resource request is denied with `WrongResource` before
  privileged action.

The marker tail for that proof is:

```text
PYTHOS:CORE:CRASH_CONTAINMENT_READY
PYTHOS:CORE:BOUNDARY:BAD_POINTER_CONTAINED
PYTHOS:CORE:BOUNDARY:CAPABILITY_ALLOWED
PYTHOS:CORE:BOUNDARY:FORGERY_DENIED
PYTHOS:CORE:BOUNDARY:HARDWARE_DENIED
PYTHOS:CORE:CAPABILITY_BOUNDARY_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

That is the strongest current claim in the project: for the fixed proof surface,
authority at the user/kernel boundary is enforced by PythCore and the hardware,
not by cooperating service code.

## How Claims Are Verified

Verification is layered.

The QEMU boot harness treats serial output as the oracle. The kernel emits
ordered milestone markers, and `scripts/test-boot.py` fails if required markers
are missing or out of order. `scripts/run-qemu.py` classifies terminal outcomes
as success, panic, reset, timeout, or marker-order violation. Timeout is never
accepted as success evidence.

The main acceptance commands are:

```powershell
python scripts\test-boot.py --slice capability-enforcement-at-boundary --timeout 60
python scripts\test-boot.py --slice graceful-audio-fallback --no-audio-device --timeout 60
python scripts\test-boot.py --slice milestone-1 --timeout 60
python scripts\test-boot.py --slice milestone-1 --media iso --timeout 60
python scripts\test-persistent-storage.py
```

The persistent-storage harness boots, persists typed object state, reboots
against the same storage image, verifies object/relationship/revision metadata,
kills QEMU during a commit window, then verifies torn-write recovery.

The repository also has Rust unit tests, Python harness tests, clippy, rustfmt,
and a GitHub Actions workflow under `.github/workflows/qemu-acceptance.yml`.
The CI workflow runs formatting, Rust unit tests, clippy, Python harness tests,
QEMU milestone acceptance, the no-audio fallback path, ISO boot, and the full
QEMU slice handoff suite.

## What Is Not Claimed

PythOS is not currently a general-purpose desktop OS. The following are not
implemented:

* general-purpose filesystem allocation;
* dynamic object database;
* dynamic user process creation;
* general userspace ABI;
* user pointer copy-in/copy-out policy;
* networking;
* package management;
* immutable A/B updates;
* SMP;
* broad physical hardware support;
* AI inside the trusted core;
* Patch, Open Surface, Causal Lens UI, or semantic indexing.

The Phase 8 boundary proof is real, but it is bounded. It proves the current
fixed ring-3/syscall/capability surface, not arbitrary third-party user
programs or a mature application platform.

## Why This Is Different From "It Boots"

Many hobby operating systems can truthfully say they boot under QEMU. PythOS can
make narrower but stronger claims:

* the loader exits firmware services and PythCore validates the handoff;
* the old broad identity map is proven absent by an expected page fault;
* scheduler and preemption progress are serial-ordered;
* storage recovery is verified across real QEMU reboot and killed mid-commit
  scenarios;
* the final Phase 8 boundary proves bad-pointer containment, copied capability
  denial, and hardware-resource denial at the syscall gate.

The value of the project is the discipline around those claims. The repo does
not ask the reader to believe a status document. It gives them marker contracts,
ADRs, tests, and boot logs that either support a claim or fail the run.

## Where To Look

Primary architecture and scope:

```text
docs/PythOS-SAS-001.md
docs/PythOS-TDD-001.md
docs/ROADMAP.md
docs/HANDOVER.md
docs/THREAT-MODEL.md
```

Key late-phase ADRs:

```text
docs/decisions/0022-on-disk-typed-object-format.md
docs/decisions/0025-phase-7-object-store-checkpoint-recovery.md
docs/decisions/0028-phase-8-syscall-abi.md
docs/decisions/0035-phase-8-crash-containment.md
docs/decisions/0036-phase-8-capability-boundary.md
```

Verification entry points:

```text
scripts/test-boot.py
scripts/run-qemu.py
scripts/test-persistent-storage.py
tests/boot_core_handoff.py
tests/test_qemu_exit.py
tests/test_boot_marker_contract.py
.github/workflows/qemu-acceptance.yml
```
