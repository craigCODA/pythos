# PythOS Roadmap: Phase 11 Closure And What's Left

This supersedes the Phase 9/10 planning sections of the older later-phases
sketch with verified actuals, records the Phase 11 closure state, and carries
the remaining phases forward with the naming and ADR conventions established in
practice.

## Status Recap (Verified, Not Assumed)

```text
Phase 1.5 - 8    complete   foundation: boot, memory, scheduling, IPC,
                              capabilities, runtime, GUI, audio, ring-3
Phase 9          complete   general-purpose-process-model
                              ADRs 0037-0043
                              final marker: PYTHOS:CORE:PHASE_9_COMPLETE
Phase 10         complete   general-purpose-storage
                              ADRs 0044-0045
                              final marker: PYTHOS:CORE:PHASE_10_COMPLETE
                              commit range 984e352..55e18d3
Phase 11         recorded   physical-hardware-boot-smoke-test
                              ADR 0046
                              findings: docs/phase-11-real-hardware-findings.md
```

Next unallocated ADR number: **0069**. Hard stop is the Phase 11 -> Phase 12
boundary, already recorded in `AGENTS.md`. Nothing below is authorized to start
without explicit re-invocation, same as every phase before it.

---

## Phase 11: Physical Hardware Boot Smoke Test

Status: recorded on `main`. ADR 0046 and
`docs/phase-11-real-hardware-findings.md` are the Phase 11 closure artifacts.
The later O2 Micro `1217:8620` SDHCI/eMMC and evidence-terminal records add
target-specific physical storage/evidence coverage; they do not broaden Phase
11 into generic hardware support.

### Purpose

Confirm the Phase 1-10 boot chain - the entire foundation, now including the
general process model and general storage - works on real UEFI firmware, not
just QEMU. This is the first phase whose outcome cannot be verified from a diff
or CI log; it depends on a physical machine.

### Preconditions

Phase 10 exit condition reproducible. Nothing else; this phase is deliberately
independent of Phase 9/10's content, only their existence as a stable base to
boot.

### Locked Slice Sequence

1. `real-hardware-target-selection` - pick one owned machine, record UEFI
   firmware vendor/version in ADR 0046.
2. `bootable-usb-creation` - write `target/pythos.iso` to real media.
3. `serial-capture-on-real-hardware` - determine COM1 availability: physical
   port, USB-serial adapter, or framebuffer-only fallback for this pass.
4. `real-hardware-boot-attempt` - boot it, record the outcome exactly as
   observed.
5. `divergence-catalog` - document every difference from OVMF/VMware: ACPI
   shape, memory map, framebuffer format, SMBIOS content.

### Exit Condition

Either the full marker sequence through `MILESTONE_1_COMPLETE`, now including
Phase 9/10's markers, is observed on real hardware, or a specific documented
divergence point is identified. Both are valid, useful outcomes; this phase
does not require success to be complete, only an honest finding.

### Scope Boundary

No driver-writing here. Only prove or disprove that the existing kernel boots
on real firmware.

### Required Artifacts

`docs/phase-11-real-hardware-findings.md`, populated regardless of outcome.
`ADR 0046` for the target hardware/firmware and loader-handoff record.

---

## Phase 12: General-Purpose Filesystem Path Layer

### Purpose

Phase 10 gave PythOS dynamic, capability-gated, quota-enforced typed-object
storage. It did not give it path-based addressing such as `/apps/foo` or
`/home/user/doc`. `ADR 0018`'s object-graph-native direction means this is a
real design fork, not a formality: does PythOS stay object-graph-addressed with
paths as a presentation-layer convenience, or does it grow a real hierarchical
namespace underneath?

Applications and packaging silently assumed this question was already answered.
It is not. Resolve it here before packaging needs an answer it does not have.

### Preconditions

Phase 10 exit condition reproducible.

### Locked Slice Sequence

1. `path-vs-graph-decision` - ADR only, no code. Decide and justify: pure
   object-graph addressing, a thin path layer over the object graph, or a
   genuine POSIX-adjacent hierarchy. Given `ADR 0018`'s existing direction, the
   thin-path-layer option is the default unless a specific reason justifies
   divergence.
2. `path-resolution` - implement whatever slice 1 decided, capability-gated the
   same way every other Phase 3/8/10 resource access is.
3. `path-adversarial-suite` - prove the corresponding attack class is denied
   specifically, not generically. If paths exist, that includes traversal
   attempts such as `../`; if symlink-equivalent relationships exist, that
   includes confusion attacks.

### Exit Condition

A capability-scoped service can resolve and access a stored object via the
chosen addressing scheme, with a specific proven-denied case for the
corresponding attack class.

### Scope Boundary

Do not build a general POSIX filesystem - permissions bits, hard links, mount
points - unless slice 1's ADR specifically concludes that is the right target.
The default assumption is the thinner option.

### Required Artifacts

The slice-1 ADR is the actual deliverable of this phase; everything else is
downstream of that decision.

---

## Phase 13: Applications and Packaging

### Purpose

The first phase where PythOS runs software it did not compile itself.

### Preconditions

Phase 9 is complete. Phase 12 is complete, so the addressing scheme is decided.

### Locked Slice Sequence

1. `package-format` - ADR for what a PythOS package is.
2. `package-install` - install from a local source into Phase 10 storage.
3. `package-launch` - launch an installed package as a Phase 9 process with a
   capability grant set derived from the package's declared needs.
4. `package-uninstall` - clean removal, including storage reclamation and
   capability revocation.
5. `first-third-party-app` - package and run something not written as part of
   the kernel test suite.

### Exit Condition

A package built independently of the kernel test suite installs, launches as a
properly capability-scoped Phase 9 process, and uninstalls cleanly, verified by
an automated test.

### Scope Boundary

No registry, no remote fetch, no dependency resolution between packages. One
local package, full lifecycle, is the bar.

---

## Phase 14: Networking

### Purpose

The parking-lot condition - capabilities implemented, tested, and boring - has
been true since Phase 8. Networking can now build on capability-scoped Phase 9
processes rather than kernel-privileged network-facing code.

### Preconditions

Phase 9, because network-facing code should run as a capability-scoped process,
not kernel-privileged by default.

### Locked Slice Sequence

`nic-driver` -> `link-layer` -> `arp` -> `ip` -> `icmp` -> `udp` -> `tcp` ->
`dns` -> `capability-gated-socket-api` -> `secure-transport`.

### Exit Condition

Two endpoints exchange TCP data, with socket capability denial proven the same
specific way as every other capability gate: a process knows the exact
destination and port, but is still denied without the grant.

---

## Phase 15: Hardware Driver Expansion

### Purpose

Driver expansion is deliberately unsliced beyond "read Phase 11's findings
first." Sequencing driver work before knowing what real hardware actually needs
would be false precision.

### Preconditions

Phase 11's findings document exists and is read first.

---

## Phase 16: Immutable A/B Updates and Recovery Mode

### Purpose

OS-image-level recovery, distinct from Phase 10's object-store-level crash
recovery.

### Preconditions

Phase 13, because packaging informs what an update updates, and Phase 14,
because networking provides transport.

### Locked Slice Sequence

`dual-partition-layout` -> `image-signing-and-verification` ->
`atomic-switch` -> `automatic-rollback-on-boot-failure` ->
`recovery-mode-boot-path`.

---

## Phase 17: SMP

### Purpose

Multiple CPU cores, deliberately last. This is now explicitly a re-audit of
Phases 2 through 16, not just 2 through 8. Phase 9's process model and Phase
10's storage concurrency added new single-core assumptions, including
`storage_concurrency.rs` write-token serialization, that need the same
per-subsystem SMP audit treatment as earlier phases.

### Locked Slice Sequence

`ap-startup` -> `per-cpu-data` -> `spinlocks-and-atomics` ->
`multi-core-scheduler` -> `per-subsystem-smp-audit`.

The final audit covers every phase's negative/adversarial suite through Phase
17, not just through Phase 8.

### Exit Condition

Every phase's original adversarial test suite, from Phase 2 through Phase 17,
passes under real multi-core execution. The exit condition is "nothing
regressed."

---

## What Changed From The Original Later-Phases Sketch

1. Phase 9 and Phase 10 are verified-complete rather than planned.
2. Phase 12, `general-purpose-filesystem-path-layer`, is new. It surfaced
   because Phase 10 delivered object-graph storage without deciding whether
   paths exist on top of it, while Applications and Packaging silently assumed
   an answer that was never given.
3. Every phase after Phase 12 is renumbered by one.
4. SMP's audit scope now explicitly covers everything built through Phase 17.
