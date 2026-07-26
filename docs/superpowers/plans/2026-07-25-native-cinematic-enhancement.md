# Native Boot Cinematic Enhancement — Implementation Plan

> Implements ADR 0047 (Accepted). Reopens Phase 6 to enrich the **native**
> cinematic only. The authored `.mp4`/HTML is a visual reference; nothing is
> embedded or decoded.

**Goal:** Replace the three flat green text frames with a native
Black/Violet/Electric-Blue cinematic depicting a glowing serpent that moves and
coils into position, while keeping the wake phrase, the audio
(hiss/sub-bass/tremolo), the audio-visual sync, the graceful no-audio fallback,
and every serial marker unchanged.

## Cinematic Beats (owner brief, 2026-07-25)

1. A snake-like figure moves/slithers around the screen, coiling into position.
   **"PythOS"** appears during this movement.
2. The serpent settles to face the user and hisses — **"Hsssss"** (the `[HISS]`
   beat).
3. Settled, it declares **"We Are Woken."**

The serpent is a stylized, procedural, glowing figure (a tapered chain of
light-segments along a serpentine path, with a facing head) — not a photoreal
animal. The on-screen hiss text may render "Hsssss", but the canonical identity
string `PythOS [HISS] We Are Woken` (`boot_assets::WAKE_PHRASE`, wired into
Phase 4/8 and tests) is preserved unchanged.

**Design:** `docs/decisions/0047-reopen-phase-6-native-cinematic.md`

**Architecture:** All work is native PythCore rendering through the existing
`core/src/framebuffer.rs` `Surface` + `core/src/software_renderer.rs` +
`core/src/compositor.rs` path, driven by `core/src/cinematic_boot.rs` on the PIT
timeline, with timing/asset data in `core/src/boot_assets.rs`. No new ABI, no ESP
asset, no video, no themes, no interactive gate.

## Global Constraints

- Preserve the wake phrase `PythOS [HISS] We Are Woken` exactly.
- Preserve all serial markers and the milestone-1 sequence; the serial oracle
  (`python scripts/test-boot.py --slice milestone-1`) must not regress.
- Preserve the graceful no-audio path
  (`--slice graceful-audio-fallback --no-audio-device`).
- Keep the audio-visual sync invariant in `cinematic_boot::validate_sync`
  consistent whenever the visual timeline changes; do not silently break it.
- Every framebuffer write stays bounds-checked against `byte_length`; document
  any new `unsafe` with the full invariant checklist.
- No new ABI, ESP asset, boot theme, gate/input, networking, or interpreter
  change (per ADR 0047 scope).
- A successful compile is not a successful boot; verify visually in QEMU and,
  where noted, on the real laptop.

---

### Slice 1: Palette + gradient/glow background

**Files:** `core/src/framebuffer.rs` (+ palette consts).

- Add the Black / Violet / Electric-Blue palette constants.
- Add a background fill that is a vertical (and/or radial) gradient/glow instead
  of the flat `clear(BACKGROUND)`, computed per-pixel through the existing
  `encode`/`put_pixel` path (no alpha needed yet).
- Point `render_cinematic_boot_frame` at the new background.
- Verify: builds/clippy clean; `--slice milestone-1` still reaches
  `MILESTONE_1_COMPLETE`; QEMU screenshot shows the new background.

### Slice 2: The "P" sigil

**Files:** `core/src/framebuffer.rs`.

- Draw a large centered "P" sigil. First cut: a scaled glyph with a soft glow
  (drawn as concentric dimmer passes around the mark); upgrade to a geometric
  mark only if the glyph is too crude.
- Verify: builds/clippy clean; no marker regression; QEMU screenshot shows the
  sigil over the gradient.

### Slice 3: Alpha blend + animated text formation

**Files:** `core/src/framebuffer.rs`, `core/src/boot_assets.rs`,
`core/src/cinematic_boot.rs`.

- Add an alpha-blend pixel op that blends a color toward the computed background
  at that pixel (so text/sigil can fade and form in), still fully bounds-checked.
- Expand the visual timeline from 3 discrete frames to a finer sequence of
  animation steps (fade-in / assemble of the wake text with easing), updating
  `boot_assets` timeline data and `cinematic_boot::run_synced_sequence` to drive
  it. Keep emitting `PYTHOS:CORE:BOOT_VISUAL:FRAME` per step.
- Verify: builds/clippy clean; `--slice milestone-1` passes; QEMU capture shows
  the text forming in.

### Slice 4: Retune audio-visual sync

**Files:** `core/src/boot_assets.rs`, `core/src/cinematic_boot.rs`,
`core/src/audio.rs` (timing only if needed).

- Recompute `sync_points` for the new timeline so `validate_sync` holds, keeping
  the hiss/sub-bass/tremolo audio aligned to the visual beats.
- Keep `PYTHOS:CORE:BOOT_SYNC:AUDIO` and the existing audio design; do not add
  new audio sources.
- Verify: `--slice milestone-1` passes; `--slice graceful-audio-fallback
  --no-audio-device` still completes the visual sequence and boot.

### Slice 5: Verify, fallback, docs

**Files:** `docs/ROADMAP.md`, `docs/HANDOVER.md`, this plan.

- Full check: `cargo fmt --check`, both target builds, both clippy targets,
  `python -m unittest` for the cinematic/boot tests, `--slice milestone-1`,
  `--media iso`, and the no-audio fallback.
- Real-hardware visual check on the laptop (user-driven): the cinematic renders
  from USB; recorded honestly as the only real-firmware evidence.
- Update `docs/ROADMAP.md` Phase 6 and `docs/HANDOVER.md` to note the
  reopened-and-re-completed cinematic (per ADR 0047).
- Commit in small, purpose-scoped commits, no co-author trailer.

## Iteration Note (Look-and-Feel)

The authored `.mp4`/HTML cannot be seen from the build host beyond
"Black/Violet/Electric-Blue + sigil." Each visual slice is tuned against QEMU
screenshots plus owner feedback on the key beats (what moves, when the "P"
appears, how the text forms), iterating until it matches the authored intent.

## Real-Hardware Verification (user-driven)

After the visual slices, the user boots the USB (Secure Boot off) on the laptop
and confirms the cinematic renders. Audio remains silent on the AMD laptop (no
HDA driver); that is a known, separately-tracked gap, not a regression here.
QEMU success is necessary but not sufficient.

**Confirmed (2026-07-25):** the `boot-cinematic-native` build (compact ~3.5 s,
serpent + shimmer + head orb + title; pre-fix loader, `BOOTX64.EFI` 71680,
`PYTHCORE.ELF` 3546432) boots successfully on the real AMD laptop from USB. Audio
silent as expected. The laptop is the working real-hardware oracle for the
cinematic. Not yet verified on the desktop (that needs the 512 GiB handoff fix
from `real-hardware-usb-boot` folded in).
