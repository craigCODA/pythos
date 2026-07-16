# PythOS Handover

Current boundary: Phase 8 `crash-containment` complete. Halt at the
`crash-containment` -> `capability-enforcement-at-boundary` boundary.

This file is a session-continuity aid, not the source of truth. Trust the live
repository, the current branch, and QEMU serial output over this file if they
ever disagree.

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
Next allowed slice: capability-enforcement-at-boundary
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
CPU-quota proof. ADR 0035 records the Phase 8 crash-containment proof. Do not
start networking, AI, SMP, or hardware-expansion work before their roadmap
gates.

## Phase 6 Summary

Phase 6 replaced the diagnostic-only boot identity with a native cinematic boot
sequence. The wake phrase is:

```text
PythOS [HISS] We Are Woken
```

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
`WorkspaceSession` object kind. It captures the Phase 5 launcher, service
monitor, Python console, and settings panel window object ids plus bounded
presentation geometry in ADR 0022 fields, then proves the session survives
through the current revision-history substrate. It does not implement object
browser work, reboot persistence, or sector persistence.

The object-browser slice records ADR 0024 and implements a minimal Phase 5
app-facing inspection surface over the current object-store substrate. It
creates a typed object-browser window, lists stored typed objects, inspects a
typed relationship target, and inspects retained revision counts. It does not
implement reboot persistence or sector persistence.

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
full hostile-code service isolation
full hostile-code capability enforcement
SMP
package management
Open Surface
Patch
```

Ring-3 execution, the distinct user CR3, the syscall ABI, the guarded
user-stack pool, service-local runtime roots, guarded shared-memory proof,
process-termination proof, quota proofs, and crash-containment proof exist only
for bounded proof paths. Capability separation for services is still not the
full hostile-code boundary. Do not claim hostile-code isolation until the Phase
8 adversarial boundary tests land.

## Next Slice

Phase 8 is `real-hardware-isolation`.

Before continuing Phase 8, re-read:

```text
AGENTS.md
docs/PythOS-SAS-001.md
docs/PythOS-TDD-001.md
docs/ROADMAP.md
```

Then begin only the next Phase 8 slice from the roadmap:

```text
capability-enforcement-at-boundary
```

Expected TDD posture for the next Phase 8 slice:

1. Add a failing automated proof that a hostile user-mode service cannot forge
   or bypass a Phase 3 capability check at the syscall gate.
2. Keep Phase 8 scoped to hardware-enforced isolation. Do not begin networking,
   AI, SMP, or hardware expansion before their roadmap gates.
3. Preserve the Phase 3 capability semantics and Phase 7 storage format unless
   an ADR explicitly records a migration.
4. Do not claim hostile-code isolation until the Phase 8 adversarial boundary
   tests land.
5. Prove both ESP and ISO milestone boots still report `QEMU_OUTCOME success`.
