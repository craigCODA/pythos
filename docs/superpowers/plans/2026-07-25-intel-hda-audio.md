# Intel HDA Audio — Implementation Plan

> Implements ADR 0048. Adds an Intel HDA (Azalia) audio backend alongside AC97,
> developed and verified against QEMU `intel-hda`. AMD ACP/I2S is parked until
> HDA works.

**Status: COMPLETE (2026-07-26).** All slices done and verified in QEMU
(`--hda`): controller discovery, MMIO mapping, reset + CORB/RIRB, codec
enumeration (via the Immediate Command Interface — CORB/RIRB ring sequencing
misbehaved in QEMU after the first command), output stream + PCM playback
(link-position advances = DMA fetching samples), and routing the existing boot
audio through HDA. Default milestone-1 (ESP + ISO) and the no-audio fallback are
unaffected; the `--hda` boot needs a longer timeout due to device init.
Deferred, per ADR 0048: preferring HDA over AC97 is moot on the real target (the
AMD laptop has no AC97, so HDA is the sole backend); AMD ACP/I2S for laptop
speakers remains a parked investigation. Real-hardware audio unverified until a
laptop boot checks the headphone jack.

**Goal:** Real PCM audio through an Intel HDA controller, selected in preference
to AC97 when present, so the boot cinematic can sound on HDA hardware (and the
laptop headphone jack) — verified in QEMU via serial markers and WAV capture.

**Design:** `docs/decisions/0048-intel-hda-audio.md`.
**Findings precondition:** `docs/phase-11-real-hardware-findings.md`.

## Global Constraints

- Do not regress AC97: when only AC97 is present, Phase 6 markers/behavior are
  unchanged. Keep the graceful silent fallback.
- HDA registers are MMIO: map the controller BAR into the device virtual region;
  every MMIO access is bounds-checked and documented (unsafe checklist).
- Serial marker per slice; where sound is produced, verify with QEMU WAV capture.
- QEMU success is necessary but not sufficient for real-hardware sound.
- No AMD ACP/I2S work in this plan (parked, ADR 0048).

## Slices

### Slice 1: Controller discovery + QEMU wiring
- Add an opt-in `intel-hda` + `hda-output` device to the QEMU harness (a
  `--hda` flag / new test slice) so existing AC97 tests are untouched.
- Scan PCI for class 0x04 subclass 0x03; read the 64-bit MMIO BAR0 base.
- Extend `AudioDeviceSelection` with an `Hda` arm; prefer HDA when found.
- Emit `PYTHOS:CORE:AUDIO:HDA:CONTROLLER_FOUND` (+ absent marker). No register
  access yet.

### Slice 2: MMIO map + controller reset + CORB/RIRB
- Map the HDA MMIO BAR into the device virtual region.
- Read GCAP, take the controller out of reset, allocate and program CORB/RIRB
  ring buffers. Emit `PYTHOS:CORE:AUDIO:HDA:CONTROLLER_READY`.

### Slice 3: Codec enumeration
- Enumerate the codec on the link, walk the widget graph, locate an output
  converter (DAC) and a driven pin/output. Emit
  `PYTHOS:CORE:AUDIO:HDA:CODEC_READY`.

### Slice 4: Stream setup + PCM playback
- Configure an output stream: format, BDL, sample buffer; wire the DAC→pin path;
  start the stream. Reuse the Phase 6 fixed PCM asset. Emit
  `PYTHOS:CORE:AUDIO:HDA:PCM_PLAYBACK`; verify non-silent WAV capture.

### Slice 5: Route boot audio + selection integration
- Route the cinematic boot audio through the selected backend (HDA or AC97).
- Full verification: fmt, both builds, both clippy, milestone-1 (+iso),
  graceful-audio-fallback, the new HDA slice, WAV check. Update ROADMAP/HANDOVER.

## Real-Hardware Verification (user-driven, later)

Once HDA plays in QEMU, boot the laptop and check whether its headphone jack
produces sound (speakers likely need ACP/I2S — parked). Record honestly; the
laptop is the only real-firmware evidence.
