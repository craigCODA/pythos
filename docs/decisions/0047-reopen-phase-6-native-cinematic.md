# ADR 0047: Reopen Phase 6 For A Richer Native Boot Cinematic

Status: Accepted

## Context

Phase 6 ("Cinematic Boot and Voice") is COMPLETE. It replaced the diagnostic
boot screen with a native identity sequence: three scaled text frames
(`PythOS` -> `[HISS]` -> `We Are Woken`, `core/src/boot_assets.rs`) drawn through
the compositor and timed against procedurally-synthesized audio (layered hiss,
sub-bass, tremolo in `core/src/audio.rs`). Its defining constraint, stated in
`docs/ROADMAP.md`, is that the sequence "must be natively implemented, not the
existing prototype HTML boot animation."

The project owner has an authored cinematic (a "Black / Violet / Blue" piece,
with a sigil, motion, and title formation) that exists only as a prototype HTML
player wrapping a rendered `.mp4`. The current native sequence is correct for
Phase 6's exit condition but is visually far simpler than that authored intent.

A path was explored and rejected this session: transcode the `.mp4` into a
compact frame codec, ship it as an ESP asset, and play it back in PythCore. That
was discarded because it directly contradicts Phase 6's "native, not the HTML
prototype" mandate, and because it would require a new boot-ABI field, a new
on-disk asset format, and an in-kernel decoder — none of which Phase 6 sanctions.

Phase 6 is a completed, scope-locked phase. Per the roadmap's own rule
("If a slice's exit condition cannot be reached without touching a forbidden
area, stop and raise an ADR proposal instead of expanding scope silently"), any
change to it is recorded here for explicit owner acceptance before code lands.

## Decision (Proposed)

Reopen Phase 6 for a bounded enhancement of the **native** cinematic only. The
authored `.mp4`/HTML is used solely as a **visual reference** for a from-scratch
native reproduction; it is not embedded, decoded, or shipped.

In scope:

- Richer native visuals through the existing `software_renderer` / `compositor`
  / `framebuffer` path: the Black/Violet/Electric-Blue palette, a background
  gradient/glow, a "P" sigil, and animated formation of the existing wake text
  with easing and timing that evokes the authored piece.
- Keep the existing audio design (hiss / sub-bass / tremolo) and the
  audio-visual sync contract; keep the graceful no-audio fallback.
- Preserve every existing serial marker and the milestone-1 boot sequence
  unchanged (`PYTHOS:CORE:BOOT_VISUAL:FRAME`, `PYTHOS:CORE:BOOT_SYNC:AUDIO`,
  through `MILESTONE_1_COMPLETE`).

Explicitly out of scope (unchanged from Phase 6, or deferred to their own
decisions):

- No embedded video, no video codec, no new ESP asset file, no `PythBootInfo`
  ABI change.
- No user-configurable boot themes (still barred by Phase 6's scope boundary);
  this is a single richer built-in sequence.
- No interactive "Initiate / Awakening" login gate. That idea (the HTML's
  entry button as an unlock surface) needs input handling at the shell
  threshold and is a separate, later decision — not built here.
- No networking, no agent, no interpreter changes.

## Consequences

- The boot identity better matches the authored design intent while staying a
  native, from-scratch implementation consistent with Phase 6's mandate.
- The wake phrase and the milestone-1 serial oracle are unchanged, so
  `python scripts/test-boot.py --slice milestone-1` and the graceful-audio
  fallback test must still pass with no marker regression.
- No durable ABI, asset format, or theme surface is introduced, so nothing here
  becomes a forward-compatibility obligation.
- Verification is visual in QEMU plus a real-hardware check on the laptop (the
  laptop already renders the current wake screen); QEMU success is necessary but
  not sufficient, as with all real-hardware-facing work.
- Rollback is reverting to the current three-frame `boot_assets` sequence.
- On acceptance, `docs/ROADMAP.md` Phase 6 and `docs/HANDOVER.md` are updated to
  note the reopened-and-re-completed cinematic, and an implementation plan is
  written under `docs/superpowers/plans/`.

## Alternatives Considered

- **Embed the authored video** (codec + ESP asset + ABI field): rejected;
  contradicts the Phase 6 "native, not the HTML prototype" mandate and adds
  durable ABI/format surface for a boot animation.
- **Leave Phase 6 as-is**: rejected by the owner; the current sequence does not
  reflect the authored cinematic intent.
- **Build the interactive gate now**: deferred; it is a distinct feature
  (input, shell-threshold placement, future auth surface) and does not belong in
  a bounded cinematic-visual enhancement.
