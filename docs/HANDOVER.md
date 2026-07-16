# PythOS Handover

Current boundary: Phase 7 `revision-history` complete; next slice is
`workspace-objects`.

This file is a session-continuity aid, not the source of truth. Trust the live
repository, the current branch, and QEMU serial output over this file if they
ever disagree.

## Verify First

Run these from `C:\Users\NeverAMoment\pythos` before continuing work:

```powershell
git status --short --branch
git log --oneline -8
python scripts\test-boot.py --slice revision-history
python scripts\test-boot.py --slice graceful-audio-fallback --no-audio-device
python scripts\test-boot.py --slice milestone-1
python scripts\test-boot.py --slice milestone-1 --media iso
```

The successful boot harness output must include `QEMU_OUTCOME success`.
Timeout termination is not success evidence.

## Expected Branch And Stop Point

Expected branch:

```text
milestone/phase7-persistent-object-storage
```

Expected state:

```text
Milestone 1.5 complete
Phase 2 complete
Phase 3 complete
Phase 4 complete
Phase 5 complete
Phase 6 complete
Phase 7 revision-history complete
Next allowed slice: workspace-objects
```

ADR 0022 records the on-disk typed-object format. Do not start object-browser
work, networking, AI, ring-3, SMP, or hardware-expansion work before the
roadmap gate for that slice.

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

## Phase 7 Marker Tail

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
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

## Important Files

Phase 6 and Phase 7 code:

```text
core/src/audio.rs
core/src/block_device.rs
core/src/boot_assets.rs
core/src/cinematic_boot.rs
core/src/framebuffer.rs
core/src/font.rs
core/src/main.rs
core/src/object_relationships.rs
core/src/revision_history.rs
core/src/shell_objects.rs
core/src/storage_journal.rs
core/src/storage_service.rs
core/src/typed_object_format.rs
```

Test and harness code:

```text
.github/workflows/qemu-acceptance.yml
scripts/run-qemu.py
scripts/test-boot.py
tests/boot_core_handoff.py
tests/test_boot_marker_contract.py
tests/test_ci_workflow.py
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
completed persistent object store
filesystem
raw sector persistence
workspace objects
object browser
user-configurable boot themes
physical audio hardware support beyond QEMU AC97
networking
AI inside the trusted core
ring-3 service isolation
SMP
package management
Open Surface
Patch
```

Capability separation is still architectural and cooperative for trusted
kernel-mode services. Do not claim hostile-code isolation until Phase 8 lands.

## Next Phase

Phase 7 is `persistent-object-storage`.

Before continuing Phase 7, re-read:

```text
AGENTS.md
docs/PythOS-SAS-001.md
docs/PythOS-TDD-001.md
docs/ROADMAP.md
```

Then begin only the next Phase 7 slice from the roadmap:

```text
workspace-objects
```

Expected TDD posture for the next Phase 7 slice:

1. Build the first concrete persisted object kind for a saved shell
   layout/session using Phase 5 window object identities.
2. Add or confirm a failing test/harness expectation that a workspace object
   survives through the current typed object and revision-history substrate.
3. Do not implement the object browser, reboot persistence, or sector
   persistence in this slice.
4. Prove both ESP and ISO milestone boots still report `QEMU_OUTCOME success`.
