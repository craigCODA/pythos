# PythOS Handover

Current boundary: ADR 0063 physical evidence terminal is implemented on `main`
from the former `agent/physical-evidence-terminal` line and is QEMU-accepted
through `scripts\test-evidence-terminal.py`. Phase 10 remains complete. Main
also carries five 2026-08-02 evidence-terminal gallery images showing 139
ordered markers, zero dropped markers, and CRC `176F4C6E`. Those frames are
retained as physical artifact evidence for the disposable O2 Micro `1217:8620`
target, while COM1/QEMU acceptance remains the automated oracle.

The earlier Phase 11 targeted SDHCI/eMMC backend panel validation remains
recorded on branch `feature/sdhci-emmc-backend`. Two operator-supplied physical
boot artifacts from the disposable O2 Micro `1217:8620` laptop reached the
final Phase 10 backend panel: the JPG is the first run and the MP4 is the
second run.

This file is a session-continuity aid, not the source of truth. Trust the live
repository, the current branch, and QEMU serial output over this file if they
ever disagree.

## Interface Model Correction (2026-08-05)

ADR 0066 supersedes the desktop-shell authority portions of ADRs 0018, 0023,
0024, 0049, and 0053. PythOS must not adopt applications, windows, launchers,
desktops, settings panels, widgets, or conventional file navigation as its
authoritative user model.

Existing Phase 5 marker names, object-kind codes, test-contract names, and
checkpoint/replay formats remain compatibility evidence only. Retain useful
input, rendering, font, composition, pointer, diagnostic, console, and
object-inspection substrate, but do not extend launcher, widget, window-shell,
desktop, or first-party application work unless a later owner-approved phase
explicitly authorizes it.

## PythTIG Owner Adoption Status (2026-08-04)

The former branch `docs/pythtig-phase0-from-physical-evidence` imported the
proposed Pyth Native Typed Instruction Graph program as docs-only Phase 0
adoption material on top of `agent/physical-evidence-terminal`. That proposal
material is now merged into `main` after the ADR 0063 evidence-terminal
implementation merge.

Owner review accepted ADR 0064 as the PythTIG architecture direction. ADR 0065
is accepted, and the tested PythTIG version 1 package ABI is frozen as of
2026-08-08. Phase 2 ring-3 runtime acceptance is complete. Phase 3
object/capability integration is implemented as an explicit opt-in proof path
that uses the retained object service; it does not change the default
production object-shell boot path and does not authorize Phase 4+ work.

Imported proposal artifacts live under:

```text
docs/pyth-tig/
docs/superpowers/specs/2026-08-03-pyth-typed-instruction-graph-design.md
docs/superpowers/plans/2026-08-03-pyth-typed-instruction-graph-master-plan.md
docs/superpowers/plans/2026-08-03-pyth-tig-phase-*.md
```

The PythTIG ADRs are renumbered against the live repository:

```text
ADR 0064: Pyth Native Typed Instruction Graph
ADR 0065: Pyth Graph Package ABI
```

This merge keeps the ADR 0063 evidence-terminal implementation baseline intact
and records the full reconciliation in
`docs/pyth-tig/PHASE-0-RECONCILIATION-REPORT.md`. Do not implement the Pyth
source language, compiler, Task Steward, native backend, production cutover, or
later marker/CI behavior from this proposal until the owner explicitly invokes
the corresponding phase.

## SDHCI/eMMC PIO Backend (2026-08-01, branch `feature/sdhci-emmc-backend`)

ADR 0062 adds an opt-in polling single-block PIO SDHCI/eMMC backend behind the
existing `BlockDeviceInfo` surface. The backend keeps virtio and AHCI intact,
uses recursive PCI discovery, maps SDHCI BAR0 into an uncacheable
supervisor-only MMIO window before the kernel CR3 switch, initializes/selects
the eMMC card once per boot, derives capacity from EXT_CSD, and dispatches
single-sector CMD17/CMD24 requests through bounded polling loops.

Supported and verified:

- legacy virtio-blk in QEMU
- polling AHCI in QEMU
- polling single-block PIO SDHCI/eMMC in QEMU
- polling single-block PIO SDHCI/eMMC reaching the final Phase 10 backend
  panel across two cold-boot runs on the one disposable O2 Micro `1217:8620`
  laptop

Physical evidence status:

- the ADR 0062 two-cold-boot physical panel gate is recorded for this one
  disposable O2 Micro `1217:8620` target only

Recorded physical evidence:

- `D:\Downloads\20260801_171744.jpg` (first run, SHA-256
  `9886EDD5D79A1BE50A887C38EB3CB9A90896D619D7B341AB098FFEB48D904122`)
  shows:

```text
PythOS
sdhci emmc backend
phase10 ok
disk writes
capacity 000000000747C000
```

- `D:\Downloads\20260801_171753.mp4` (second run, SHA-256
  `DC178998ECFE6F3349A29930C083A61545817421963EB8D265DC96D0604C900E`)
  shows:

```text
PythOS
sdhci emmc backend
phase10 ok
disk writes
capacity 000000000747C000
```

- Screen-only frame:
  `docs/evidence/2026-08-01-physical-sdhci-emmc-backend-boot1.jpg`
- Screen-only frame from the second run:
  `docs/evidence/2026-08-01-physical-sdhci-emmc-backend-boot2.jpg`
- Milestone evidence page:
  `docs/milestones/2026-08-01-physical-emmc-phase10.md`

Not implemented:

- interrupts, DMA/ADMA, multi-block I/O, hotplug
- generic SD/SDHCI support
- partitions or filesystems
- PythOS-native boot/storage format
- interactive physical shell pending built-in input support

Backend-specific QEMU acceptance:

```powershell
python scripts\test-sdhci-emmc-block-device.py
python scripts\test-object-shell.py --backend sdhci-emmc
```

The storage test boots from ISO with `--no-virtio-blk --sdhci --emmc`, rejects
virtio/AHCI selection markers, requires `DEVICE_SELECTED_SDHCI_EMMC`, runs the
Phase 7-10 storage proofs twice against the same disposable eMMC image, checks
host image signatures, and captures the verify-only framebuffer panel marker
`PYTHOS:CORE:BLOCK:SDHCI_EMMC_FRAMEBUFFER_ACCEPTANCE_READY`.

The USB ESP for physical validation was refreshed on 2026-08-01 by replacing
only `EFI` and `PYTHOS` on Disk 2 Partition 2, label `PYTHOS_ESP`, preserving
`NvVars` and system metadata. Expected physical panel after a successful
verify boot:

```text
PythOS
sdhci emmc backend
phase10 ok
disk writes
capacity <hex>
```

Do not infer physical backend support from QEMU. The physical panel gate is now
recorded for the disposable O2 Micro `1217:8620` target only; do not generalize
it to other SDHCI/eMMC controllers or to interactive physical shell use.

## Physical Evidence Terminal (2026-08-02)

ADR 0063 adds an opt-in framebuffer evidence terminal for serial-less physical
capture. The UEFI loader allocates a page-aligned 64 KiB `PYLOG001` evidence
buffer, mirrors loader markers into it, passes it through explicit
`PythBootInfo` ABI 0.3 fields, and PythCore validates, maps, appends to, and
renders the same accepted marker transcript after the Phase 10 storage proof.
COM1 remains the automated oracle; the framebuffer terminal is a visual mirror
for evidence capture.

QEMU acceptance originally recorded at implementation commit `5e73e73`:

```powershell
python scripts\test-evidence-terminal.py
```

Post-merge validation from `main` on 2026-08-04 also passed with the same
command after resolving the `docs/HANDOVER.md` conflict.

Successful output included:

```text
PYTHOS:CORE:EVIDENCE_TERMINAL_READY
QEMU_OUTCOME success
EVIDENCE_TERMINAL_TEST_OK
```

The acceptance boot uses:

```text
pythos-boot: evidence-terminal
pythos-core: verify,sdhci-emmc-backend,evidence-terminal
QEMU storage: --no-virtio-blk --sdhci --emmc
success marker: PYTHOS:CORE:EVIDENCE_TERMINAL_READY
screendump: target\evidence-terminal.ppm
```

The harness requires the PPM screendump to match the evidence-terminal frame
palette and expected title/status/body row glyph structure, not merely to be
non-empty or palette-colored.

The committed physical gallery artifact shows:

```text
PythOS Evidence Terminal
page 01/05 count 00000139 drop 00000000 crc 176F4C6E
...
page 05/05 count 00000139 drop 00000000 crc 176F4C6E
PYTHOS:CORE:PHASE_10_COMPLETE
PYTHOS:CORE:BLOCK:SDHCI_EMMC_FRAMEBUFFER_ACCEPTANCE_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

Committed screen frames:

- `docs/evidence/2026-08-02-evidence-terminal-page-1.jpg`
- `docs/evidence/2026-08-02-evidence-terminal-page-2.jpg`
- `docs/evidence/2026-08-02-evidence-terminal-page-3.jpg`
- `docs/evidence/2026-08-02-evidence-terminal-page-4.jpg`
- `docs/evidence/2026-08-02-evidence-terminal-page-5.jpg`

After the ADR 0063 merge, `main` contains the evidence-log sources,
`evidence-terminal` Cargo feature, and `scripts/test-evidence-terminal.py`
harness needed to regenerate the automated QEMU evidence-terminal acceptance
path. The five JPG frames are retained as committed physical artifact evidence,
not as a bit-for-bit output that the QEMU harness is expected to reproduce.

Implementation notes:

- `PYTHOS:CORE:EVIDENCE_TERMINAL_READY` is emitted only after the terminal
  renders and the snapshot reports `dropped == 0`.
- After `PYTHOS:CORE:EVIDENCE_TERMINAL_READY`, PythCore waits one bounded
  evidence-terminal capture dwell before `qemu_exit::success()` so QEMU can
  take the screendump while the final frame is still guest-visible.
- `PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED` is a failing path, not success.
- The evidence buffer is mapped at fixed high kernel virtual window
  `0xFFFF_C000_1003_0000`, writable/NX in the kernel root, and supervisor-only
  in user roots so serial hooks can append while user CR3 roots are active
  without colliding with low or user ELF virtual ranges.
- Page dwell and ready-marker capture dwell use PIT ticks with a bounded spin
  fallback documented as verified only for the O2 Micro `1217:8620` evidence
  path.

This is evidence for the captured milestone path, not a claim that every named
subsystem marker is production-complete, portable, interrupt-driven, or broadly
supported across hardware. COM1 remains the automated QEMU oracle, and the
physical evidence-terminal claim remains scoped to the disposable O2 Micro
`1217:8620` target.

## Branch `object-shell` (2026-07-27, unmerged)

ADR 0051/0052's first ring-3 object shell (`shell.elf`) is fully implemented
and verified on branch `object-shell`, not yet merged to `main`. All 12 tasks
of `docs/superpowers/plans/2026-07-26-first-ring3-object-shell.md` are
complete, including reboot-durable object-service persistence and Task 11
stack-guard-page hardening. That plan document is the authoritative record —
see its Task 9-12 status notes for exact architecture, protocol, and
verification detail. Verify with `python scripts\test-object-shell.py`,
`python scripts\test-normal-fast-boot.py`, `python scripts\test-boot.py
--slice milestone-1`, `python scripts\test-com2-shell-transport.py`, and
`python scripts\test-persistent-storage.py`. For the ADR 0054 AHCI backend
extension, also run `python scripts\test-ahci-block-device.py`. Do not merge or
build on this branch without re-reading that plan document's Task 12 section
first.

## Interactive Object-Shell Launcher (2026-07-28, branch `object-shell`)

ADR 0066 later supersedes ADR 0053's launcher authority model. Treat this
section as historical and transitional input-gate evidence only; do not extend
it into a conventional launcher, desktop, application, or window model without
a new owner-approved phase.

ADR 0053 (see `docs/decisions/0053-interactive-object-shell-launcher.md`)
closes the interactive-input gate ADR 0047 deferred. Normal boot now: plays
the existing boot cinematic + AC97 audio, renders an "Enter Shell" tile with
a real mouse cursor, and blocks until a real (or QMP-injected) left click
lands on that tile before launching `shell.elf` — same shell, same
persistence, same everything already verified in Tasks 1-12, just with the
front door open. QEMU-only in scope for input: when ADR 0053 was accepted,
real hardware still could not run this branch because of a separate
block-storage gap. ADR 0054 later partially closed the storage side of that
gap for QEMU AHCI, but physical SATA hardware validation remains separate.

New: `core/src/ps2.rs` (real PS/2 keyboard/mouse controller driver — the
first code in this tree touching real input hardware), `core/src/
launcher_screen.rs` (kernel-mode click-to-launch poll loop, runs strictly
before the existing one-shot ring-3 entry), `fill_rect`/cursor-sprite
primitives in `core/src/framebuffer.rs`, `scripts/launcher_click.py` (shared
QMP-injection helper), `scripts/test-normal-boot-interactive.py` (proves the
real IRQ1/IRQ12 hardware path fires, not just synthetic decode bytes).

To watch and click it yourself:
```powershell
python scripts\build-image.py
python scripts\run-qemu.py --display gtk --audio-backend dsound --serial-log target\manual-full-boot.log --timeout 120 --expect-outcome timeout
```
Watch the cinematic play with audio, then move the mouse and click "Enter
Shell" — QEMU's GTK window captures host input when focused.

Verify with everything in the section above, plus
`python scripts\test-normal-boot-interactive.py`. `test-normal-fast-boot.py`,
`test-com2-shell-transport.py`, and `test-object-shell.py` all now inject a
real click via `scripts/launcher_click.py` before waiting for the shell to
come up (`test-object-shell.py`'s reboot test injects it twice — once per
boot). `test-boot.py` and `test-persistent-storage.py` are unaffected
(verify-feature builds compile `normal_boot` out entirely).

Out of scope, unchanged: real-hardware (USB HID) input and HDA-in-normal-boot
(AC97 is the committed baseline). Physical SATA validation, NVMe, filesystems,
and partition discovery remain later work.

## Polling AHCI Storage Backend (2026-07-28, branch `object-shell`)

ADR 0054 (see `docs/decisions/0054-polling-ahci-block-backend.md`) adds a
second block backend behind the existing sector API. Default QEMU boots still
select legacy `virtio-blk`; AHCI is selected only when virtio is absent.
PythCore now discovers an AHCI controller by PCI class/subclass/prog-if, maps
BAR5/ABAR into a fixed uncacheable kernel MMIO window before the VM switch, and
uses one polling command slot for single-sector ATA `READ DMA EXT` /
`WRITE DMA EXT` requests. The legacy `PYTHOS:CORE:BLOCK:DEVICE_SELECTED`
marker remains, with backend-specific `_VIRTIO` / `_AHCI` markers added.

Verify AHCI specifically with:

```powershell
python scripts\test-ahci-block-device.py
```

That test boots with `--no-virtio-blk --ahci`, uses a separate
`target\ahci-store.img`, asserts `PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI`,
runs the Phase 7-10 storage proofs to `PYTHOS:CORE:MILESTONE_1_COMPLETE`, then
boots the same image a second time to prove persisted object/general-storage
state is restored over AHCI. The runner support is in `scripts/run-qemu.py` via
`--ahci`, `--ahci-storage-image`, and `--no-virtio-blk`.

Still out of scope: NVMe, interrupt-driven storage, MSI/MSI-X, Local
APIC/IOAPIC, multi-bus PCI enumeration, filesystems, partition discovery,
hotplug, IOMMU/DMA isolation, package management, networking, updates, SMP,
and AI.

## Real-Hardware Boot Status (2026-07-25)

PythOS now boots on real UEFI hardware, not only QEMU. A laptop boots a USB all
the way to the milestone-1 cinematic wake screen (`PythOS [HISS] We Are Woken`).
See ADR 0046 and `docs/superpowers/{specs,plans}/2026-07-24-real-hardware-usb-boot*`.

- The loader identity-maps the low 512 GiB (1 GiB huge pages) so machines that
  load the loader / boot structures above 4 GiB survive the `CR3` handoff.
- Early loader/core milestones paint the framebuffer solid colors; PythCore's
  first instruction is a format-independent white "liveness" paint. On a serial-
  less machine the last color shown localizes how far boot reached.
- Known-deferred: one desktop still stops at the loader's magenta pre-handoff
  color; the 512 GiB map is the leading fix but is unverified on that box. The
  laptop is the working real-hardware oracle for now.
- Secure Boot must stay disabled (loader is unsigned). Diagnostic paints fire on
  every boot, including successful ones; gating them to failures is a follow-up.

## Intel HDA Audio (2026-07-26, branch `hda-audio`)

An Intel HDA (Azalia) audio backend is implemented alongside AC97 (ADR 0048,
`docs/phase-11-real-hardware-findings.md`, plan
`docs/superpowers/plans/2026-07-25-intel-hda-audio.md`). PythCore discovers the
controller, maps its MMIO into the kernel address space at VM-build time,
resets it, enumerates the codec's output path (DAC + pin) via the Immediate
Command Interface, and plays the boot audio through an output stream (verified
by the link-position register advancing). Enabled with `python
scripts/run-qemu.py ... --hda` (needs a longer `--timeout`); default milestone-1
is unaffected because HDA init is skipped when no controller is present. AMD
ACP/I2S (laptop speakers) is parked; real-hardware audio (headphone jack) is
unverified pending a laptop boot. Branch not yet merged to `main`.

## Verify First

Run these from `C:\Users\NeverAMoment\pythos` before continuing work:

```powershell
git status --short --branch
git log --oneline -8
python scripts\test-boot.py --slice object-browser
python scripts\test-boot.py --slice save-and-restore-across-reboot
python scripts\test-boot.py --slice ring-3-execution
python scripts\test-boot.py --slice separate-address-spaces
python scripts\test-boot.py --slice syscall-entry
python scripts\test-boot.py --slice user-stacks
python scripts\test-boot.py --slice service-local-python-runtimes
python scripts\test-boot.py --slice guarded-shared-memory
python scripts\test-boot.py --slice process-termination
python scripts\test-boot.py --slice memory-quotas
python scripts\test-boot.py --slice cpu-quotas
python scripts\test-boot.py --slice crash-containment
python scripts\test-boot.py --slice capability-enforcement-at-boundary
python scripts\test-persistent-storage.py
python scripts\test-boot.py --slice graceful-audio-fallback --no-audio-device
python scripts\test-boot.py --slice milestone-1
python scripts\test-boot.py --slice milestone-1 --media iso
```

The successful boot harness output must include `QEMU_OUTCOME success`.
Timeout termination is not success evidence.

## Expected Branch And Stop Point

Expected branch:

```text
milestone/phase8-real-hardware-isolation
```

Expected state:

```text
Milestone 1.5 complete
Phase 2 complete
Phase 3 complete
Phase 4 complete
Phase 5 complete
Phase 6 complete
Phase 7 complete
Phase 8 ring-3-execution complete
Phase 8 separate-address-spaces complete
Phase 8 syscall-entry complete
Phase 8 user-stacks complete
Phase 8 service-local-python-runtimes complete
Phase 8 guarded-shared-memory complete
Phase 8 process-termination complete
Phase 8 memory-quotas complete
Phase 8 cpu-quotas complete
Phase 8 crash-containment complete
Phase 8 capability-enforcement-at-boundary complete
Next allowed work: none inside Phase 8; Later Phases require explicit
re-invocation and a detailed roadmap section
```

ADR 0022 records the on-disk typed-object format. ADR 0023 records the
workspace-session object kind. ADR 0024 records the object-browser inspection
boundary. ADR 0025 records the Phase 7 checkpoint/recovery sector contract. ADR
0026 records the Phase 8 ring-3 execution proof. ADR 0027 records the Phase 8
separate address-space proof. ADR 0028 records the Phase 8 syscall ABI. ADR
0029 records the Phase 8 guarded user-stack layout. ADR 0030 records the Phase
8 service-local runtime-instance proof. ADR 0031 records the Phase 8 guarded
shared-memory proof. ADR 0032 records the Phase 8 process-termination proof.
ADR 0033 records the Phase 8 memory-quota proof. ADR 0034 records the Phase 8
CPU-quota proof. ADR 0035 records the Phase 8 crash-containment proof. ADR
0036 records the Phase 8 capability-boundary proof. Do not start networking,
AI, SMP, package management, updates, or hardware-expansion work before their
roadmap gates.

## Phase 6 Summary

Phase 6 replaced the diagnostic-only boot identity with a native cinematic boot
sequence. The wake phrase is:

```text
PythOS [HISS] We Are Woken
```

Reopened and enriched by ADR 0047 (2026-07-25): the native cinematic now renders
a glowing "Black/Violet/Electric-Blue" serpent that forms in, shimmers, ignites
an energy orb at the awakening beat, and resolves into the "PythOS / We Are
Woken" title — a native port of the authored reference (the `.mp4`/HTML is a
visual reference only, never embedded). It is a compact ~3.5 s (wall-clock
bounded via `cinematic_boot::CINEMATIC_TICKS`) so the whole boot stays inside the
20 s milestone-1 budget. Renderer primitives live in `core/src/framebuffer.rs`
(gradient, additive glow dots, serpent path, shimmer, orb, glow text). Not yet
ported from the reference: the "PYTHOS" code-rain intro and the embedded PCM
soundtrack (audio is still AC97/QEMU-only and silent on the AMD laptop). Plan:
`docs/superpowers/plans/2026-07-25-native-cinematic-enhancement.md`.

The implementation deliberately stays bounded:

* Audio target is QEMU AC97 only.
* Audio device choice is recorded in ADR 0020.
* Loader debug ELF file-size growth is recorded in ADR 0021.
* Visual, PCM, and sync assets are embedded in PythCore because Phase 7
  persistent storage does not exist yet.
* The no-audio path must still complete the visual sequence and milestone boot.

## Completed Phase 6 Slices

```text
audio-device-selection
audio-driver
audio-buffers
pcm-playback
audio-mixing
boot-asset-storage
audio-visual-sync
graceful-audio-fallback
```

## Completed Phase 7 Slices

```text
block-device-driver
storage-service
append-only-journal
checksums-and-commit-markers
crash-recovery
typed-object-format
object-relationships
revision-history
workspace-objects
object-browser
save-and-restore-across-reboot
```

## Completed Phase 8 Slices

```text
ring-3-execution
separate-address-spaces
syscall-entry
user-stacks
service-local-python-runtimes
guarded-shared-memory
process-termination
memory-quotas
cpu-quotas
crash-containment
capability-enforcement-at-boundary
```

The first storage slice attaches a bounded raw QEMU storage image as a non-boot
legacy `virtio-blk` PCI device. The ESP is now attached explicitly as an
`ide-hd` boot device so OVMF does not try to boot the empty storage disk. PythCore
selects vendor `0x1AF4` device `0x1001`, validates the legacy I/O BAR, enables
I/O and bus-master command bits, reads capacity and queue metadata, and emits
the block-device markers. This slice does not implement raw sector I/O,
storage-service mediation, journaling, object records, or crash recovery.

The storage-service slice makes the selected block device opaque outside
`block_device` and authorizes storage intents through Phase 3 capability
handles. It proves a valid holder can authorize bounded storage access and that
wrong-holder or missing-rights attempts are denied before block access. It does
not implement raw sector I/O, append-only journaling, object records, or crash
recovery.

The append-only-journal slice requires a storage-service-authorized write
intent to append a monotonic journal record before any write completion can be
considered. It proves read requests are not journaled as writes and a full
journal rejects new records without overwriting old ones. It does not implement
checksums, commit markers, crash recovery, raw sector I/O, or object records.

The checksums-and-commit-markers slice adds a stable checksum over committed
journal record fields plus an explicit commit marker. It proves missing commit
markers and checksum mismatches are detected as invalid records instead of
silently accepted. It does not implement crash recovery, raw sector I/O, or
object records.

The crash-recovery slice replays only the committed journal prefix and rolls
back the first invalid tail plus everything after it. It proves an interrupted
write without a commit marker recovers to the last committed sequence and that
checksum mismatch tails are also rolled back. It does not implement typed
objects, raw sector I/O, or object browser work.

The typed-object-format slice records ADR 0022 and implements the fixed
little-endian object record with magic, format version, record length, stable
`ObjectId`, `ObjectKind` code, object schema version, and bounded versioned
field slots. It rejects invalid format inputs and preserves ADR 0018's identity
versus presentation split. It does not implement relationships, revision
history, workspace objects, object browser work, or sector persistence.

The object-relationships slice records typed, queryable relationships between
known typed object records. It covers `blocks`, `created-by`, and `depends-on`,
rejects unknown endpoints and duplicate edges, and proves lookup by source and
relationship kind. It does not implement revision history, workspace objects,
object browser work, or sector persistence.

The revision-history slice keeps prior versions when an object is updated and
records a monotonic timestamp plus writer service identity for each retained
revision. It proves the current object advances while revision 1 and revision 2
remain queryable with their original metadata. It does not implement workspace
objects, object browser work, or sector persistence.

The workspace-objects slice records ADR 0023 and implements the
`WorkspaceSession` object kind. It captures Phase 5 compatibility projection
object ids plus bounded presentation geometry in ADR 0022 fields, then proves
the session survives through the current revision-history substrate. It does
not implement object browser work, reboot persistence, or sector persistence.

The object-browser slice records ADR 0024 and implements a minimal Phase 5
compatibility inspection surface over the current object-store substrate. It
creates a typed object-browser projection, lists stored typed objects, inspects
a typed relationship target, and inspects retained revision counts. It does
not implement reboot persistence or sector persistence.

The save-and-restore-across-reboot slice records ADR 0025 and implements the
Phase 7 end-to-end persistence proof. PythCore writes the typed workspace
snapshot through virtio-blk sector I/O into a committed checkpoint sector,
restores the same object, relationship, revision count, and writer identity on
the next boot, and clears a deliberately torn tail sector after the harness
kills QEMU during the commit window. It does not implement a filesystem,
dynamic object database, Causal Lens UI, Patch, networking, or multi-user
access control.

The ring-3-execution slice records ADR 0026 and implements the first Phase 8
hardware-isolation proof. PythCore installs CPL3 GDT selectors, sets
`TSS.RSP0`, maps fixed user code and stack proof pages in the current address
space, enters ring 3 with `iretq`, accepts a user-originated breakpoint trap,
verifies the ring-3 `CS`/`SS` trap frame, returns to a saved kernel stack, and
emits `PYTHOS:CORE:RING3_EXECUTION_READY`. It does not implement separate
address spaces, syscall entry, user process stacks, service-local runtimes, or
hostile-code containment.

The separate-address-spaces slice records ADR 0027 and implements the first
distinct user CR3 proof. PythCore builds a second PML4 root before the first
kernel CR3 switch, validates it is distinct from the kernel root, validates the
fixed proof code and stack are user-accessible while kernel text and data remain
supervisor-only, switches to that root, reruns the CPL3 breakpoint proof,
restores the kernel root, and emits
`PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY`. It does not implement syscall
entry, user process stacks, service-local runtimes, guarded shared memory,
process termination, quotas, crash containment, or hostile-code capability
enforcement.

The syscall-entry slice records ADR 0028 and implements the first defined
syscall ABI. PythCore configures `syscall`/`sysret` MSRs, enters the gate from
the fixed CPL3 proof while running under the distinct user CR3 root, switches to
a fixed kernel syscall stack, dispatches syscall number `0x5059_0001`, proves a
capability-gated Phase 3 IPC send, invokes the Phase 4 `system.log` surface with
`PythOS [HISS] We Are Woken`, returns through `sysretq`, and emits
`PYTHOS:CORE:SYSCALL_ENTRY_READY`. It does not implement user process stacks,
user pointer copy-in/copy-out, service-local runtimes, guarded shared memory,
process termination, quotas, crash containment, or hostile-code capability
enforcement.

The user-stacks slice records ADR 0029 and implements a fixed guarded
user-stack pool. PythCore reserves page-aligned stack slots with a
supervisor-only guard page below each usable non-executable user stack page,
maps only the usable pages into the distinct user CR3 root, validates both the
stack and guard permissions under that root, reruns the CPL3 proof on the
guarded stack pool, and emits `PYTHOS:CORE:USER_STACKS_READY`. It does not
implement dynamic user processes, user pointer copy-in/copy-out,
service-local runtimes, guarded shared memory, process termination, quotas,
crash containment, or hostile-code capability enforcement.

The service-local-python-runtimes slice records ADR 0030 and implements the
first service-local runtime-instance proof. PythCore boots two runtime
instances from the validated Phase 4 source through a shared service-identity
table, assigns distinct service identities, task ids, user CR3 roots, and local
runtime state slots, rejects cross-service state mutation, and emits
`PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY`. It does not implement guarded
shared memory, user pointer copy-in/copy-out, process termination, quotas,
crash containment, or hostile-code capability enforcement.

The guarded-shared-memory slice records ADR 0031 and revalidates Phase 3
shared-memory capability semantics under distinct Phase 8 user roots. PythCore
binds reader and writer service identities to different user CR3 roots, proves
a read-only shared-memory handle can still read the fixed region, denies a
cross-space write attempt through the wrong holder, verifies the region bytes
remain unchanged, and emits `PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY`. It does
not implement user pointer copy-in/copy-out, process termination, quotas, crash
containment, or hostile-code capability enforcement.

The process-termination slice records ADR 0032 and proves a fixed user process
can be forcibly terminated without cooperation. PythCore tracks the process by
task id and user CR3 root, marks it terminated, proves it is no longer returned
as runnable, reclaims the terminated user address-space page-table frames,
verifies the physical allocator free-page count increases by the exact
reclaimed frame count, and emits
`PYTHOS:CORE:PROCESS_TERMINATION_READY`. It does not implement memory quotas,
CPU quotas, crash containment, or hostile-code capability enforcement.

The memory-quotas slice records ADR 0033 and proves kernel-owned memory
accounting keyed by service identity. PythCore grants an in-quota page charge,
denies an over-quota page charge, verifies the denied charge does not mutate
recorded usage, and emits `PYTHOS:CORE:MEMORY_QUOTAS_READY`. It does not
implement CPU quotas, crash containment, or hostile-code capability
enforcement.

The cpu-quotas slice records ADR 0034 and proves kernel-owned CPU accounting
keyed by service identity. PythCore records an in-quota tick charge, denies an
over-quota tick charge, verifies the denied charge does not mutate recorded
usage, and emits `PYTHOS:CORE:CPU_QUOTAS_READY`. It does not implement crash
containment or hostile-code capability enforcement.

The crash-containment slice records ADR 0035 and proves a fixed user-mode crash
is contained as a service failure. PythCore runs a CPL3 illegal-instruction
probe, diagnoses it through the exception path as a user fault, terminates only
the faulting service process, preserves a peer service process, and emits
`PYTHOS:CORE:CRASH_CONTAINMENT_READY`. It does not implement capability
forgery resistance at the syscall boundary.

The capability-enforcement-at-boundary slice records ADR 0036 and closes Phase
8. PythCore contains a fixed CPL3 bad-pointer page fault, validates legitimate
syscall-gated capability use before IPC mutation, denies a copied handle value
from the wrong service identity, denies hardware-resource repurposing, and
emits `PYTHOS:CORE:CAPABILITY_BOUNDARY_READY`. It does not implement a
general-purpose userspace ABI, copy-in/copy-out, networking, package
management, SMP, or hardware expansion.

## Phase 8 Marker Tail

The normal AC97-enabled milestone path includes this ordered tail after
`PYTHOS:CORE:PHASE_5_COMPLETE`:

```text
PYTHOS:CORE:AUDIO:DEVICE_SELECTED
PYTHOS:CORE:AUDIO_DEVICE_SELECTION_READY
PYTHOS:CORE:AUDIO:DRIVER
PYTHOS:CORE:AUDIO_DRIVER_READY
PYTHOS:CORE:AUDIO:BUFFER
PYTHOS:CORE:AUDIO_BUFFERS_READY
PYTHOS:CORE:AUDIO:PCM_PLAYBACK
PYTHOS:CORE:PCM_PLAYBACK_READY
PYTHOS:CORE:AUDIO:MIX:HISS
PYTHOS:CORE:AUDIO:MIX:SUB_BASS
PYTHOS:CORE:AUDIO:MIX:TREMOLO
PYTHOS:CORE:AUDIO_MIXING_READY
PYTHOS:CORE:BOOT_ASSET:VISUAL
PYTHOS:CORE:BOOT_ASSET:PCM
PYTHOS:CORE:BOOT_ASSET:SYNC
PYTHOS:CORE:BOOT_ASSETS_READY
PYTHOS:CORE:BOOT_VISUAL:FRAME
PYTHOS:CORE:BOOT_SYNC:AUDIO
PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY
PYTHOS:CORE:AUDIO:FALLBACK_ARMED
PYTHOS:CORE:GRACEFUL_AUDIO_FALLBACK_READY
PYTHOS:CORE:PHASE_6_COMPLETE
PYTHOS:CORE:BLOCK:DEVICE_SELECTED
PYTHOS:CORE:BLOCK_DEVICE_READY
PYTHOS:CORE:STORAGE:ACCESS_GRANTED
PYTHOS:CORE:STORAGE:ACCESS_DENIED
PYTHOS:CORE:STORAGE_SERVICE_READY
PYTHOS:CORE:STORAGE:JOURNAL_APPEND
PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY
PYTHOS:CORE:STORAGE:CHECKSUM_VALID
PYTHOS:CORE:STORAGE:COMMIT_MARKER
PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_READY
PYTHOS:CORE:STORAGE:RECOVERY_REPLAY
PYTHOS:CORE:STORAGE:RECOVERY_ROLLBACK
PYTHOS:CORE:CRASH_RECOVERY_READY
PYTHOS:CORE:OBJECT:STABLE_ID
PYTHOS:CORE:OBJECT:VERSIONED_FIELDS
PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY
PYTHOS:CORE:OBJECT:RELATIONSHIP
PYTHOS:CORE:OBJECT:RELATIONSHIP_QUERY
PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY
PYTHOS:CORE:OBJECT:REVISION_RETAINED
PYTHOS:CORE:OBJECT:REVISION_PROVENANCE
PYTHOS:CORE:REVISION_HISTORY_READY
PYTHOS:CORE:WORKSPACE:SESSION_OBJECT
PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT
PYTHOS:CORE:WORKSPACE_OBJECTS_READY
PYTHOS:CORE:OBJECT_BROWSER:LIST
PYTHOS:CORE:OBJECT_BROWSER:DETAIL
PYTHOS:CORE:OBJECT_BROWSER_READY
PYTHOS:CORE:OBJECT_STORE:PERSISTED
PYTHOS:CORE:OBJECT_STORE:RESTORED
PYTHOS:CORE:PHASE_7_COMPLETE
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:RING3_EXECUTION_READY
PYTHOS:CORE:ADDRESS_SPACE:CREATED
PYTHOS:CORE:ADDRESS_SPACE:ISOLATED
PYTHOS:CORE:ADDRESS_SPACE:SWITCHED
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:ADDRESS_SPACE:RESTORED
PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY
PYTHOS:CORE:SYSCALL:MSRS_READY
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:SYSCALL:ENTER
PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK
PYTHOS:CORE:SYSTEM:LOG
PYTHOS:CORE:SYSCALL:SYSTEM_LOG
PYTHOS:CORE:SYSCALL:RETURN
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:SYSCALL_ENTRY_READY
PYTHOS:CORE:USER_STACK:ALLOCATED
PYTHOS:CORE:USER_STACK:GUARD_PAGE
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:USER_STACKS_READY
PYTHOS:CORE:RUNTIME:LOCAL_INSTANCE
PYTHOS:CORE:RUNTIME:ADDRESS_SPACE
PYTHOS:CORE:RUNTIME:STATE_ISOLATED
PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY
PYTHOS:CORE:SHM:RING3_READ
PYTHOS:CORE:SHM:CROSS_SPACE_WRITE_DENIED
PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY
PYTHOS:CORE:PROCESS:TERMINATED
PYTHOS:CORE:PROCESS:UNSCHEDULABLE
PYTHOS:CORE:PROCESS:ADDRESS_SPACE_RECLAIMED
PYTHOS:CORE:PROCESS_TERMINATION_READY
PYTHOS:CORE:QUOTA:MEMORY_GRANTED
PYTHOS:CORE:QUOTA:MEMORY_DENIED
PYTHOS:CORE:MEMORY_QUOTAS_READY
PYTHOS:CORE:QUOTA:CPU_TICK
PYTHOS:CORE:QUOTA:CPU_THROTTLED
PYTHOS:CORE:CPU_QUOTAS_READY
PYTHOS:CORE:CRASH:USER_FAULT
PYTHOS:CORE:CRASH:SERVICE_TERMINATED
PYTHOS:CORE:CRASH:PEER_ALIVE
PYTHOS:CORE:CRASH_CONTAINMENT_READY
PYTHOS:CORE:BOUNDARY:BAD_POINTER_CONTAINED
PYTHOS:CORE:BOUNDARY:CAPABILITY_ALLOWED
PYTHOS:CORE:BOUNDARY:FORGERY_DENIED
PYTHOS:CORE:BOUNDARY:HARDWARE_DENIED
PYTHOS:CORE:CAPABILITY_BOUNDARY_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

The no-audio fallback path replaces the present-device markers with:

```text
PYTHOS:CORE:AUDIO:DEVICE_ABSENT
PYTHOS:CORE:AUDIO_DEVICE_SELECTION_READY
PYTHOS:CORE:AUDIO:DRIVER_SKIPPED
PYTHOS:CORE:AUDIO_DRIVER_READY
PYTHOS:CORE:AUDIO:BUFFER_SKIPPED
PYTHOS:CORE:AUDIO_BUFFERS_READY
PYTHOS:CORE:AUDIO:PCM_SKIPPED
PYTHOS:CORE:PCM_PLAYBACK_READY
PYTHOS:CORE:AUDIO:FALLBACK
PYTHOS:CORE:GRACEFUL_AUDIO_FALLBACK_READY
PYTHOS:CORE:PHASE_6_COMPLETE
PYTHOS:CORE:BLOCK:DEVICE_SELECTED
PYTHOS:CORE:BLOCK_DEVICE_READY
PYTHOS:CORE:STORAGE:ACCESS_GRANTED
PYTHOS:CORE:STORAGE:ACCESS_DENIED
PYTHOS:CORE:STORAGE_SERVICE_READY
PYTHOS:CORE:STORAGE:JOURNAL_APPEND
PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY
PYTHOS:CORE:STORAGE:CHECKSUM_VALID
PYTHOS:CORE:STORAGE:COMMIT_MARKER
PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_READY
PYTHOS:CORE:STORAGE:RECOVERY_REPLAY
PYTHOS:CORE:STORAGE:RECOVERY_ROLLBACK
PYTHOS:CORE:CRASH_RECOVERY_READY
PYTHOS:CORE:OBJECT:STABLE_ID
PYTHOS:CORE:OBJECT:VERSIONED_FIELDS
PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY
PYTHOS:CORE:OBJECT:RELATIONSHIP
PYTHOS:CORE:OBJECT:RELATIONSHIP_QUERY
PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY
PYTHOS:CORE:OBJECT:REVISION_RETAINED
PYTHOS:CORE:OBJECT:REVISION_PROVENANCE
PYTHOS:CORE:REVISION_HISTORY_READY
PYTHOS:CORE:WORKSPACE:SESSION_OBJECT
PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT
PYTHOS:CORE:WORKSPACE_OBJECTS_READY
PYTHOS:CORE:OBJECT_BROWSER:LIST
PYTHOS:CORE:OBJECT_BROWSER:DETAIL
PYTHOS:CORE:OBJECT_BROWSER_READY
PYTHOS:CORE:OBJECT_STORE:PERSISTED
PYTHOS:CORE:OBJECT_STORE:RESTORED
PYTHOS:CORE:PHASE_7_COMPLETE
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:RING3_EXECUTION_READY
PYTHOS:CORE:ADDRESS_SPACE:CREATED
PYTHOS:CORE:ADDRESS_SPACE:ISOLATED
PYTHOS:CORE:ADDRESS_SPACE:SWITCHED
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:ADDRESS_SPACE:RESTORED
PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY
PYTHOS:CORE:SYSCALL:MSRS_READY
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:SYSCALL:ENTER
PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK
PYTHOS:CORE:SYSTEM:LOG
PYTHOS:CORE:SYSCALL:SYSTEM_LOG
PYTHOS:CORE:SYSCALL:RETURN
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:SYSCALL_ENTRY_READY
PYTHOS:CORE:USER_STACK:ALLOCATED
PYTHOS:CORE:USER_STACK:GUARD_PAGE
PYTHOS:CORE:USER_MODE:ENTER
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:USER_STACKS_READY
PYTHOS:CORE:RUNTIME:LOCAL_INSTANCE
PYTHOS:CORE:RUNTIME:ADDRESS_SPACE
PYTHOS:CORE:RUNTIME:STATE_ISOLATED
PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY
PYTHOS:CORE:SHM:RING3_READ
PYTHOS:CORE:SHM:CROSS_SPACE_WRITE_DENIED
PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY
PYTHOS:CORE:PROCESS:TERMINATED
PYTHOS:CORE:PROCESS:UNSCHEDULABLE
PYTHOS:CORE:PROCESS:ADDRESS_SPACE_RECLAIMED
PYTHOS:CORE:PROCESS_TERMINATION_READY
PYTHOS:CORE:QUOTA:MEMORY_GRANTED
PYTHOS:CORE:QUOTA:MEMORY_DENIED
PYTHOS:CORE:MEMORY_QUOTAS_READY
PYTHOS:CORE:QUOTA:CPU_TICK
PYTHOS:CORE:QUOTA:CPU_THROTTLED
PYTHOS:CORE:CPU_QUOTAS_READY
PYTHOS:CORE:CRASH:USER_FAULT
PYTHOS:CORE:CRASH:SERVICE_TERMINATED
PYTHOS:CORE:CRASH:PEER_ALIVE
PYTHOS:CORE:CRASH_CONTAINMENT_READY
PYTHOS:CORE:BOUNDARY:BAD_POINTER_CONTAINED
PYTHOS:CORE:BOUNDARY:CAPABILITY_ALLOWED
PYTHOS:CORE:BOUNDARY:FORGERY_DENIED
PYTHOS:CORE:BOUNDARY:HARDWARE_DENIED
PYTHOS:CORE:CAPABILITY_BOUNDARY_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

## Important Files

Phase 6, Phase 7, and Phase 8 code:

```text
core/src/architecture/x86_64/gdt.rs
core/src/architecture/x86_64/idt.rs
core/src/architecture/x86_64/tss.rs
core/src/audio.rs
core/src/block_device.rs
core/src/boot_assets.rs
core/src/cinematic_boot.rs
core/src/framebuffer.rs
core/src/font.rs
core/src/main.rs
core/src/object_browser.rs
core/src/persistent_objects.rs
core/src/object_relationships.rs
core/src/revision_history.rs
core/src/service_runtimes.rs
core/src/shell_objects.rs
core/src/storage_journal.rs
core/src/storage_service.rs
core/src/syscall.rs
core/src/typed_object_format.rs
core/src/user_mode.rs
core/src/user_stacks.rs
core/src/workspace_objects.rs
```

Test and harness code:

```text
.github/workflows/qemu-acceptance.yml
scripts/run-qemu.py
scripts/test-ahci-block-device.py
scripts/test-boot.py
scripts/test-persistent-storage.py
tests/boot_core_handoff.py
tests/test_boot_marker_contract.py
tests/test_ci_workflow.py
tests/test_persistent_object_storage.py
tests/test_qemu_exit.py
```

Docs and ADRs:

```text
AGENTS.md
docs/ROADMAP.md
docs/PythOS-SAS-001.md
docs/PythOS-TDD-001.md
docs/decisions/0020-phase-6-ac97-audio-target.md
docs/decisions/0021-loader-kernel-file-bound.md
docs/decisions/0022-on-disk-typed-object-format.md
docs/decisions/0023-workspace-session-object-kind.md
docs/decisions/0024-object-browser-inspection-app.md
docs/decisions/0025-phase-7-object-store-checkpoint-recovery.md
docs/decisions/0026-phase-8-ring3-execution.md
docs/decisions/0027-phase-8-separate-address-spaces.md
docs/decisions/0028-phase-8-syscall-abi.md
docs/decisions/0029-phase-8-user-stacks.md
docs/decisions/0030-phase-8-service-local-runtimes.md
docs/decisions/0031-phase-8-guarded-shared-memory.md
docs/decisions/0032-phase-8-process-termination.md
docs/decisions/0033-phase-8-memory-quotas.md
docs/decisions/0034-phase-8-cpu-quotas.md
docs/decisions/0035-phase-8-crash-containment.md
docs/decisions/0036-phase-8-capability-boundary.md
docs/decisions/0051-first-ring3-object-shell.md
docs/decisions/0052-object-shell-service-abi.md
docs/decisions/0053-interactive-object-shell-launcher.md
docs/decisions/0054-polling-ahci-block-backend.md
```

Boot artifacts:

```text
target\esp
target\pythos.iso
target\boot-serial.log
target\pythos-store.img
```

The ISO path is:

```text
C:\Users\NeverAMoment\pythos\target\pythos.iso
```

## What Does Not Exist Yet

Do not assume any of the following exists:

```text
filesystem
general-purpose file allocation
dynamic object database
user-configurable boot themes
physical audio hardware support beyond QEMU AC97
networking
AI inside the trusted core
dynamic user process stacks
user pointer copy-in/copy-out
general-purpose hostile-code service isolation
general-purpose userspace ABI
SMP
package management
Open Surface
Patch
```

Ring-3 execution, the distinct user CR3, the syscall ABI, the guarded
user-stack pool, service-local runtime roots, guarded shared-memory proof,
process-termination proof, quota proofs, crash-containment proof, and
capability-boundary proof exist only for bounded proof paths. Phase 8 proves the
current hardware-backed authority boundary for those paths. Do not claim a
general-purpose userspace ABI or arbitrary hostile-code service environment
until a later phase defines one.

## Next Boundary

Phase 8 is complete. Later Phases are unordered in `docs/ROADMAP.md` and need a
fresh detailed roadmap section before implementation.

Before starting later-phase work, re-read:

```text
AGENTS.md
docs/PythOS-SAS-001.md
docs/PythOS-TDD-001.md
docs/ROADMAP.md
```

Do not begin networking, package management, updates, AI, SMP, or hardware
expansion by momentum. Pick one Later Phase, write its detailed slice sequence
and required artifacts, then start with a failing automated test.
