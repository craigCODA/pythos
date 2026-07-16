# PythOS Handover

Current boundary: Phase 7 `storage-service` complete; next slice is
`append-only-journal`.

This file is a session-continuity aid, not the source of truth. Trust the live
repository, the current branch, and QEMU serial output over this file if they
ever disagree.

## Verify First

Run these from `C:\Users\NeverAMoment\pythos` before continuing work:

```powershell
git status --short --branch
git log --oneline -8
python scripts\test-boot.py --slice storage-service
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
Phase 7 storage-service complete
Next allowed slice: append-only-journal
```

Do not start `checksums-and-commit-markers`, crash recovery, typed-object
persistence, object browser work, networking, AI, ring-3, SMP, or
hardware-expansion work before the roadmap gate for that slice.

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

## Phase 6 Marker Tail

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
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

## Important Files

Phase 6 code:

```text
core/src/audio.rs
core/src/block_device.rs
core/src/boot_assets.rs
core/src/cinematic_boot.rs
core/src/framebuffer.rs
core/src/font.rs
core/src/main.rs
core/src/shell_objects.rs
core/src/storage_service.rs
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
persistent object storage
append-only journal
checksummed commit markers
crash recovery
on-disk typed object format
filesystem
storage recovery
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
append-only-journal
```

Expected TDD posture for the next Phase 7 slice:

1. Add or confirm a failing test/harness expectation for the append-only journal
   marker.
2. Implement only a journal-first write intent path over the storage service.
3. Do not implement checksums, commit markers, recovery, or object records in
   this slice.
4. Prove both ESP and ISO milestone boots still report `QEMU_OUTCOME success`.
