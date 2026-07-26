# ADR 0048: Adopt Intel HDA Audio; Park AMD ACP/I2S

Status: Accepted

## Context

Phase 6 gave PythOS a working audio path, but only over **AC97** (ADR 0020),
which is emulated by QEMU and almost never present on modern real hardware. On
the confirmed real-hardware target — an AMD laptop — the boot cinematic plays
visually but is **silent**, because the machine has no AC97 device.

Real audio needs a modern backend. Two candidates, very different in tractability:

- **Intel HDA (High Definition Audio / "Azalia").** Well-specified: a PCI
  controller (class 0x04, subclass 0x03) with a memory-mapped register set,
  CORB/RIRB command ring, codec enumeration over a widget graph, and stream
  DMA via buffer descriptor lists. Crucially, **QEMU emulates it**
  (`intel-hda` + `hda-output`) and can capture PCM output to a WAV file, so it
  is fully verifiable against the existing serial + audio-capture oracle. Many
  real machines also expose an HDA codec for the headphone jack.
- **AMD ACP / I2S.** Modern AMD laptop *speakers* frequently hang off I2S codecs
  behind the AMD Audio Co-Processor, configured through ACPI/platform-specific
  tables — not a standard HDA controller. This stack is intensely
  machine-specific and historically very hard (the Linux `sof-amd`/ACP effort
  took years). It is also **not emulated in our x86 QEMU harness**, so there is
  no oracle to develop against.

This is Phase 15 (Hardware Driver Expansion) work, whose precondition is that
Phase 11's findings document exists and is read first. That findings document is
created alongside this ADR (`docs/phase-11-real-hardware-findings.md`), so this
pulls a bounded audio subset of Phase 15 forward with the gate satisfied.

## Decision

1. **Adopt Intel HDA as a second audio backend**, developed and verified against
   QEMU `intel-hda` with WAV capture. Extend `core/src/audio.rs`'s existing
   selection surface (`AudioDeviceSelection` / `AudioDriver` / `AudioBuffers` /
   `PcmPlayback`) with HDA variants rather than replacing AC97.
2. **Device preference:** prefer HDA when a controller is present, fall back to
   AC97, then to the silent path. Existing AC97 behavior and its Phase 6 markers
   are preserved when only AC97 is present.
3. **Verification stays QEMU-first:** each HDA slice reaches a serial marker and,
   where it produces sound, is checked via QEMU's WAV capture. QEMU success is
   necessary but not sufficient for real-hardware sound.
4. **Park AMD ACP / I2S** (laptop speakers) until HDA works. It is not testable
   in our harness, is historically hard, and its payoff on a from-scratch OS is
   uncertain. When HDA lands, revisit as a scoped **investigation** — first read
   what the laptop actually exposes (does it present an HDA codec for the jack?
   what does its ACP/PCI/ACPI look like?) before committing to build it.

## Consequences

- PythOS gains real audio in the QEMU dev loop and, likely, the laptop's
  headphone jack (jacks commonly route through an HDA codec).
- HDA registers are memory-mapped, so PythCore must map the controller's MMIO
  BAR into its device virtual region (like the framebuffer) — new plumbing AC97
  did not need (AC97 used port I/O).
- Laptop *speaker* audio may remain silent until (and unless) the ACP/I2S
  investigation succeeds; that is an explicit, recorded limitation, not a
  regression.
- The audio abstraction grows a backend enum arm; the boot cinematic's audio can
  route through whichever backend is selected.

## Alternatives Considered

- **Replace AC97 with HDA:** rejected; AC97 is the working QEMU-verified path and
  keeping it as fallback costs little and de-risks the migration.
- **Build ACP/I2S now:** rejected; no oracle, machine-specific, uncertain
  payoff — a research spike, not a build task, and only after HDA works.
- **Leave audio AC97-only:** rejected by the owner; the real-hardware target
  needs a modern backend.
