# PythOS Handover

Current boundary: Phase 6 complete, stop before Phase 7.

This file is a session-continuity aid, not the source of truth. Trust the live
repository, the current branch, and QEMU serial output over this file if they
ever disagree.

## Verify First

Run these from `C:\Users\NeverAMoment\pythos` before continuing work:

```powershell
git status --short --branch
git log --oneline -8
python scripts\test-boot.py --slice phase-6-complete
python scripts\test-boot.py --slice graceful-audio-fallback --no-audio-device
python scripts\test-boot.py --slice milestone-1
python scripts\test-boot.py --slice milestone-1 --media iso
```

The successful boot harness output must include `QEMU_OUTCOME success`.
Timeout termination is not success evidence.

## Expected Branch And Stop Point

Expected branch:

```text
milestone/phase6-cinematic-boot
```

Expected state:

```text
Milestone 1.5 complete
Phase 2 complete
Phase 3 complete
Phase 4 complete
Phase 5 complete
Phase 6 complete
Next allowed phase: Phase 7 persistent-object-storage
```

Stop here unless explicitly re-invoked for Phase 7. Do not start
`block-device-driver` or any storage work from this handover alone.

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
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

## Important Files

Phase 6 code:

```text
core/src/audio.rs
core/src/boot_assets.rs
core/src/cinematic_boot.rs
core/src/framebuffer.rs
core/src/font.rs
core/src/main.rs
core/src/shell_objects.rs
```

Test and harness code:

```text
scripts/run-qemu.py
scripts/test-boot.py
tests/boot_core_handoff.py
tests/test_boot_marker_contract.py
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
```

The ISO path is:

```text
C:\Users\NeverAMoment\pythos\target\pythos.iso
```

## What Does Not Exist Yet

Do not assume any of the following exists:

```text
persistent object storage
block device driver
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

Before any Phase 7 code lands, re-read:

```text
AGENTS.md
docs/PythOS-SAS-001.md
docs/PythOS-TDD-001.md
docs/ROADMAP.md
```

Then begin only the first Phase 7 slice from the roadmap:

```text
block-device-driver
```

Expected TDD posture for the first Phase 7 slice:

1. Add or confirm a failing test/harness expectation for the first storage
   marker.
2. Implement only the minimal virtual block device support needed by that
   slice.
3. Preserve the entire Phase 6 marker chain.
4. Prove both ESP and ISO milestone boots still report `QEMU_OUTCOME success`.
