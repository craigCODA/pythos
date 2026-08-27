# PythOS: Verification-Driven Operating System Prototype

PythOS is an x86-64 operating-system prototype built around one rule: claims
about the system must be backed by executable evidence. It boots through UEFI,
takes ownership of memory and execution from firmware, builds a native PythCore
execution substrate, brings up service identity and capability mechanisms,
persists typed objects across QEMU reboots, runs a capability-controlled ring-3
object shell, and verifies storage through virtio, AHCI, and opt-in
SDHCI/eMMC block backends in QEMU. `main` also contains the accepted PythTIG
version 1 graph-package implementation through Phase 7 cutover/cross-target
evidence, followed by the Phase 12 capability-scoped object locator and the
Phase 13 local package lifecycle, launch-authority, uninstall, and
package-defined schema extensibility proofs.

The current checked-in stop boundary is Phase 13 -> Phase 13.5: ADR 0069,
`docs/semantic-checkpoint-contract.md`, ADR 0070, ADR 0071, ADR 0072,
ADR 0073, `PYTHOS:CORE:PHASE_12_COMPLETE`, and
`PYTHOS:CORE:PHASE_13_COMPLETE` are recorded. Phase 13.5 package-session
runtime, presentation/input bridges, WakeContext/Waking, Kai, networking, and
AI work remain unimplemented and require explicit owner invocation. The
SDHCI/eMMC backend has
target-specific physical evidence on the confirmed disposable O2 Micro
`1217:8620` target. ADR 0063's evidence terminal is implemented on `main` with
QEMU acceptance through `scripts/test-evidence-terminal.py`. On 2026-08-08 the
terminal was captured on the physical O2 Micro target across five readable
pages showing `count 00000139`, `drop 00000000`, and CRC `176F4C6E`. The count
field is hexadecimal, so `0x139` is 313 decimal markers. Two separate physical
boots reproduced the same count, zero-drop state, and CRC, and the
reconstructed hardware-path stream recomputes to 313 markers with CRC
`176F4C6E`.

ADR 0074 adds an opt-in physical wake diagnostic. Its QEMU harness,
`scripts/test-physical-wake-diagnostic.py`, boots the verify image to the Phase
6 wake screen, waits for `PYTHOS:CORE:PHYSICAL_WAKE:READY`, injects `wake` plus
Enter through QMP, and requires `PYTHOS:CORE:PHYSICAL_WAKE:ACCEPTED`. On
2026-08-26 the same diagnostic image was copied to the USB ESP and the operator
reported physical acceptance after typing `wake` plus Enter on the current boot
machine. That proves only this diagnostic polling path on that machine, not
generic USB HID, trackpad input, IRQ-driven input, or shell keyboard control.

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
| Bounded presentation and audio proofs | Networking |
| Phase 10 typed-object storage in QEMU | Scalable object database |
| Ring-3 object shell in QEMU | Arbitrary third-party programs |
| Polling AHCI backend in QEMU | Broad physical hardware support |
| Polling SDHCI/eMMC backend in QEMU | Generic SDHCI/eMMC support |
| Physical SDHCI/eMMC backend evidence on O2 Micro `1217:8620` | Physical interactive shell input |
| Evidence terminal implemented and QEMU-accepted on `main` | Replacement of COM1 as automated oracle |
| Five-page physical terminal capture: 313 markers, zero drops, CRC `176F4C6E` | Bit-identical physical/QEMU transcripts |
| ADR 0074 physical wake diagnostic QEMU-accepted and operator-accepted on one boot machine | Generic keyboard, USB HID, trackpad, IRQ-driven input, or shell keyboard control |
| PythTIG Phase 1-7 implementation and acceptance records on `main` | Later PythTIG phases or AI authority |
| ADR 0069/0070/0072 object-locator decision, resolver implementation, and adversarial suite | POSIX paths as authoritative object identity |
| ADR 0073 and Phase 13 local package lifecycle through `PYTHOS:CORE:PHASE_13_COMPLETE` | Remote registries, dependency solving, persistent package sessions, or general desktop apps |

## What PythOS Is

PythOS is intended to be a graphical operating system whose primary system and
tool language is Python. Python is not the first instruction executed by
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
-> Typed task and object environment
-> Typed objects, executable tool objects, semantic relationships, projections,
   automation, and optional AI
```

The project has completed the roadmap's bounded architecture proofs through
Phase 13, `applications-and-packaging`, in QEMU. `main` also contains the first
persistent ring-3 object shell, an opt-in polling SDHCI/eMMC backend, the
PythTIG Phase 1-7 acceptance implementation, the Phase 12 object-locator
resolver plus adversarial denial suite, and the Phase 13 local package
lifecycle. Later implementation work such as Phase 13.5 persistent package
sessions, networking, updates, broad physical hardware expansion, SMP, semantic
indexing, and optional AI remains intentionally unimplemented until explicitly
invoked.

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

### Presentation, Audio, And Persistent Typed Objects

Phase 5 adds bounded presentation-substrate proofs: input decoding, typed input
events, a software renderer, PSF font handling, compositor surfaces, pointer
delivery, focus/movement over projected surfaces, typed action controls, and
diagnostic or policy-inspection projections. ADR 0018 records the useful
substrate decision that object identity is separated from presentation binding.
ADR 0066 supersedes the desktop-shell authority portions of ADR 0018 and
related documents: the old window/widget/app names remain compatibility marker
labels, not the authoritative PythOS user model.

Phase 6 adds a bounded cinematic boot/audio path using QEMU AC97 and an explicit
no-audio fallback. The wake phrase is rendered and synchronized with the boot
audio path when audio exists, and the fallback path still completes the
milestone when no AC97 device is configured.

ADR 0074 adds a separate opt-in diagnostic at the Phase 6 wake screen. The
diagnostic initializes only the first PS/2 controller port for polling, leaves
IRQ1 masked, does not enable mouse streaming, overlays the typed wake buffer and
recent raw bytes on the framebuffer, and accepts only exact `wake` plus Enter.
It is a bring-up diagnostic, not a login gate or a general input service.

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

### Dynamic Process And Storage Extensions

Phase 9 extends the fixed Phase 8 boundary with dynamic user ELF loading,
general syscall ABI versioning, copy-in/copy-out pointer validation, dynamic
capability grants, argv/environment delivery, general fault isolation, and a
process-model adversarial suite. These are still bounded proofs, but they move
the project beyond a single hardcoded ring-3 transition.

Phase 10 extends the object store with a journaled block allocator, dynamic
object create/delete, explicit fragmentation/reuse policy, per-service storage
quotas, serialized concurrent writes, and adversarial storage recovery. The
Phase 10 marker is:

```text
PYTHOS:CORE:PHASE_10_COMPLETE
```

Phase 12 slice 2 adds the internal `object-locator 0.1` resolver ABI. It
resolves bounded locator segments through typed name-binding relationships,
rejects `.` and `..` during grammar validation, validates namespace traversal
authority separately from final-object authority, and returns typed identity,
revision, and relationship-path information. The Slice 2 marker is:

```text
PYTHOS:CORE:OBJECT_LOCATOR_RESOLUTION_READY
PYTHOS:CORE:PATH_ADVERSARIAL_SUITE_READY
PYTHOS:CORE:PHASE_12_COMPLETE
```

The normal object-shell path uses COM2 as an interactive transport and proves a
create/inspect/revise/history/reboot/restore lifecycle over the same typed
object storage model.

### PythTIG And Semantic Checkpoints

ADR 0064 accepts PythTIG, the Pyth Native Typed Instruction Graph, as the
future execution-model architecture direction. ADR 0065 freezes the tested
version 1 package ABI; ADR 0068 records the compatible version 1.1 command ABI
and service-admission extension. PythTIG Phase 1 through Phase 7 are merged to
`main` through the cutover/cross-target line, with the Rust object shell
retained as a maintenance and recovery fallback.

The PythTIG claim is still bounded. PythCore accepts typed graph packages and
typed syscalls; it does not parse Pyth source, human command text, semantic
prompts, or agent policy. Task Steward can propose but cannot approve or mutate
authoritative task state without user-held authority. Cross-target claims
require unchanged graph package bytes, matching runtime digest, normalized
semantic marker comparison, and target-specific evidence.

PR #10 records a build-orchestration fix for the PythTIG one-shot QEMU
harnesses: test images package an isolated no-default PythCore ELF instead of
trusting Cargo's shared final binary path. That fix does not change package
bytes, marker contracts, runtime ABI, or boot semantics.

Phase 12 slice 1 then records the namespace decision. ADR 0069 chooses a
capability-scoped object locator namespace: locator strings may look path-like
for manifests and diagnostics, but canonical identity remains typed object
identity and authority remains capability based. The same ADR accepts
`docs/semantic-checkpoint-contract.md` as the comparison language for future
parallel build and evidence lanes. ADR 0070 implements Slice 2 path resolution
through the internal `object-locator 0.1` resolver, and ADR 0071 records the
finite loader read-bound increase needed for that debug acceptance image. ADR
0072 adds the Slice 3 adversarial suite over the same resolver ABI, proving
denials for empty segments, stale bindings, missing segments, missing traversal
authority, missing final authority, name collisions, link confusion, and
global-root fallback assumptions. Phase 12 completes at
`PYTHOS:CORE:PHASE_12_COMPLETE`.

Phase 13 records ADR 0073 as the frozen local package lifecycle and schema
extensibility ABI. It installs local package artifacts into retained package
storage, persists package manifests, launchable exports, schema definitions,
and declared capability requirements, and launches installed PythTIG exports
only when explicit supplied capabilities satisfy those requirements. The QEMU
acceptance suite proves package format validation, transactional install and
restore, launch denial boundaries, disable/uninstall policy, live-process
preservation, tombstone/reinstall identity behavior, package-defined object
creation through the real ring-3 Pyth runtime and `SYSCALL_OBJECT_REQUEST`, and
schema descriptor retention after uninstall. The independent package proof
finishes at:

```text
PYTHOS:CORE:INDEPENDENT_PACKAGE_READY
PYTHOS:CORE:PACKAGE_SCHEMA_EXTENSIBILITY_READY
PYTHOS:CORE:PHASE_13_COMPLETE
```

### Block Backends And Physical Evidence

The original persistent-storage path uses legacy virtio-blk in QEMU. Later
backend work adds polling AHCI in QEMU and an opt-in polling single-block PIO
SDHCI/eMMC backend. The SDHCI/eMMC backend is selected only when the QEMU test
boots from ISO with virtio disabled and no AHCI storage disk, and the tests
reject fallback markers so a passing run cannot be explained by another disk.

The disposable O2 Micro `1217:8620` laptop has target-specific physical Phase 10
backend evidence and later five-page evidence-terminal validation. The terminal
status line's `count 00000139` is hexadecimal, meaning 313 decimal markers, with
zero drops and CRC `176F4C6E`. The physical marker stream differs from QEMU only
where the observed hardware/audio/storage state selects different truthful
branches. The modeled physical stream closes exactly at 313 markers and CRC
`176F4C6E`.

See:

- [Physical SDHCI/eMMC Phase 10 evidence](milestones/2026-08-01-physical-emmc-phase10.md)
- [2026-08-08 physical evidence-terminal validation](evidence/2026-08-08-physical-evidence-terminal.md)
- [ADR 0074 physical wake diagnostic](decisions/0074-physical-wake-diagnostic.md)

This is a target-specific physical result, not a generic hardware-support claim.

## How Claims Are Verified

Verification is layered.

The QEMU boot harness treats serial output as the oracle. The kernel emits
ordered milestone markers, and `scripts/test-boot.py` fails if required markers
are missing or out of order. `scripts/run-qemu.py` classifies terminal outcomes
as success, panic, reset, timeout, or marker-order violation. Timeout is never
accepted as success evidence.

The evidence-terminal path mirrors accepted markers into a bounded framebuffer
transcript. `scripts/test-evidence-terminal.py` requires ordered milestone
markers, rejects panic/fallback/dropped-transcript conditions, and validates the
terminal screendump's expected glyph structure. It supplements COM1; it does not
replace COM1 as the automated oracle.

The main acceptance commands include:

```powershell
cargo fmt --check
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify -- -D warnings
cargo test -p pythos-core
python scripts\test-boot.py --slice milestone-1
python scripts\test-boot.py --slice milestone-1 --media iso
python scripts\test-persistent-storage.py
python scripts\test-normal-fast-boot.py
python scripts\test-com2-shell-transport.py
python scripts\test-object-shell.py
python scripts\test-ahci-block-device.py
python scripts\test-sdhci-emmc-block-device.py
python scripts\test-object-shell.py --backend sdhci-emmc
python scripts\test-evidence-terminal.py
python scripts\test-physical-wake-diagnostic.py
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
implemented or not claimed:

* conventional desktop-shell authority as the user model;
* general-purpose filesystem allocation;
* networking;
* remote package registries, dependency solving, or package updates;
* immutable A/B updates;
* SMP;
* broad physical hardware support;
* generic SDHCI/eMMC support;
* interrupt-driven or DMA-backed storage;
* partitions or filesystems on the SDHCI/eMMC target;
* POSIX paths as authoritative object identity;
* persistent package-session runtime or presentation/input bridges;
* WakeContext, First Waking, or Kai;
* later PythTIG phases beyond the merged Phase 7 acceptance line;
* generic physical keyboard, USB HID, trackpad, or IRQ-driven input support;
* physical interactive object-shell use through built-in keyboard or trackpad;
* a requirement that physical and QEMU evidence transcripts be bit-identical;
* CRC-32 as collision-proof proof of transcript identity;
* AI inside the trusted core;
* Patch, Open Surface, Causal Lens UI, or semantic indexing.

The Phase 8 through Phase 13 proofs and the PythTIG Phase 1-7 work are real,
but they are bounded. They prove the current ring-3/syscall/capability/storage
and graph-package/package-lifecycle surfaces, not a mature application
platform, ambient filesystem behavior, remote package distribution, or broad
hardware compatibility.

## Why This Is Different From "It Boots"

Many hobby operating systems can truthfully say they boot under QEMU. PythOS can
make narrower but stronger claims:

* the loader exits firmware services and PythCore validates the handoff;
* the old broad identity map is proven absent by an expected page fault;
* scheduler and preemption progress are serial-ordered;
* storage recovery is verified across real QEMU reboot and killed mid-commit
  scenarios;
* the ring-3 object shell persists typed, versioned objects across reboot in
  QEMU;
* SDHCI/eMMC backend tests reject virtio/AHCI fallback and inspect the backing
  eMMC image;
* physical evidence shows the Phase 10 SDHCI/eMMC path and later the full
  five-page evidence terminal on the disposable O2 Micro `1217:8620` target;
* the physical terminal records 313 accepted markers, zero drops, and CRC
  `176F4C6E`, with the modeled hardware stream independently recomputing to the
  same count and CRC;
* two separate physical boots reproduced the same terminal header;
* ADR 0074's opt-in physical wake diagnostic is QEMU-accepted and has one
  operator-reported physical acceptance of `wake` plus Enter on the current USB
  boot machine;
* the Phase 8 boundary proves bad-pointer containment, copied capability
  denial, and hardware-resource denial at the syscall gate;
* PythTIG packages are verified before ring-3 entry and compared across
  interpreter/native/cross-target evidence with normalized semantic markers;
* Phase 12 names the object-locator namespace and checkpoint contract before
  package lifecycle state can depend on path-like spelling.

The value of the project is the discipline around those claims. The repo does
not ask the reader to believe a status document. It gives them marker contracts,
ADRs, tests, boot logs, and physical artifacts that either support a claim or
fail the run.

## Where To Look

Primary architecture and scope:

```text
docs/PythOS-SAS-001.md
docs/PythOS-TDD-001.md
docs/ROADMAP.md
docs/ROADMAP-LATER-PHASES.md
docs/HANDOVER.md
docs/THREAT-MODEL.md
docs/milestones/2026-08-01-physical-emmc-phase10.md
docs/evidence/2026-08-08-physical-evidence-terminal.md
docs/semantic-checkpoint-contract.md
docs/pyth-tig/ACCEPTANCE.md
```

Key late-phase ADRs:

```text
docs/decisions/0022-on-disk-typed-object-format.md
docs/decisions/0025-phase-7-object-store-checkpoint-recovery.md
docs/decisions/0028-phase-8-syscall-abi.md
docs/decisions/0035-phase-8-crash-containment.md
docs/decisions/0036-phase-8-capability-boundary.md
docs/decisions/0044-phase-10-block-allocator-format.md
docs/decisions/0045-phase-10-fragmentation-policy.md
docs/decisions/0051-first-ring3-object-shell.md
docs/decisions/0052-object-shell-service-abi.md
docs/decisions/0054-polling-ahci-block-backend.md
docs/decisions/0062-polling-sdhci-emmc-block-backend.md
docs/decisions/0063-physical-evidence-terminal.md
docs/decisions/0064-pyth-native-typed-instruction-graph.md
docs/decisions/0065-pyth-graph-package-abi.md
docs/decisions/0068-pythtig-command-abi-and-service-admission.md
docs/decisions/0069-phase-12-object-locator-and-semantic-checkpoints.md
docs/decisions/0070-phase-12-object-locator-resolution-abi.md
docs/decisions/0071-loader-kernel-file-bound-extension.md
docs/decisions/0072-phase-12-path-adversarial-suite.md
docs/decisions/0073-phase-13-package-lifecycle-and-schema-extensibility.md
docs/decisions/0074-physical-wake-diagnostic.md
```

Verification entry points:

```text
scripts/test-boot.py
scripts/run-qemu.py
scripts/test-persistent-storage.py
scripts/test-object-shell.py
scripts/test-ahci-block-device.py
scripts/test-sdhci-emmc-block-device.py
scripts/test-evidence-terminal.py
scripts/test-physical-wake-diagnostic.py
tests/boot_core_handoff.py
tests/test_qemu_exit.py
tests/test_boot_marker_contract.py
.github/workflows/qemu-acceptance.yml
```
