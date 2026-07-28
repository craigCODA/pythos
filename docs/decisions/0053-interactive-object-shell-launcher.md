# ADR 0053: Interactive Object-Shell Launcher (Cinematic + Audio + Click-To-Enter)

Status: Accepted

## Context

Normal boot (`normal_boot.rs`, ADR 0052's "fast path") launched straight into
`shell.elf` with zero visual or audio output — a deliberate, previously
documented tradeoff. ADR 0049 explicitly parked cinematic/audio reintegration
"until the loop lands," and ADR 0047 explicitly deferred exactly this
decision: *"No interactive 'Initiate/Awakening' login gate... needs input
handling at the shell threshold — a separate, later decision, not built
here."*

With ADR 0051/0052's object shell complete (reboot-durable persistence,
guard-page-hardened syscall stack, adversarial fault isolation — Tasks 1-12),
the loop has landed. This ADR is that deferred decision: normal boot now
plays the existing cinematic and AC97 audio, then requires a real mouse click
on an "Enter Shell" tile before launching the shell, using a newly-built PS/2
keyboard/mouse driver — the first code in this tree that performs real input
hardware I/O.

## Decision

1. **Reuse the cinematic and AC97 audio pipeline as-is.** Both already
   existed, fully proven, in the verify-only boot path
   (`cinematic_boot::run_synced_sequence`, `audio.rs`'s AC97 chain). Normal
   boot now calls the same sequence, under normal-boot-specific marker names
   (`PYTHOS:CORE:NORMAL_BOOT:*`) rather than the verify path's names, since
   `scripts/test-normal-fast-boot.py` already asserted the verify-only names
   never appear in a normal boot log — that assertion is the oracle proving
   the two boot paths stay distinct, and is preserved. Failure here
   soft-skips (a machine with no audio device is a legitimate QEMU
   configuration) rather than panicking, unlike the verify path's fail-fast
   policy.
2. **HDA is explicitly out of scope for this ADR.** AC97 needs no
   address-space wiring (pure port I/O); HDA needs its MMIO window mapped via
   `KernelAddressSpace::build`, which `normal_init.rs` still hardcodes to
   `None`. AC97 is the committed baseline; HDA-in-normal-boot remains a
   stretch goal for later.
3. **A new, real PS/2 controller driver (`core/src/ps2.rs`), QEMU-only in
   scope.** Nothing in this tree previously performed real input-device port
   I/O — `input_drivers.rs`/`input_events.rs` were (and remain) pure
   decode-logic proofs fed hardcoded bytes. `ps2.rs` does the real controller
   handshake (self-test, port enable, mouse streaming-mode enable), and wires
   IRQ1 (keyboard) and IRQ12 (mouse) into `interrupts.rs`'s existing
   PIC-vector dispatch. Real-hardware input (USB HID) remains out of scope,
   the same precedent ADR 0049 already set for real-hardware audio and input
   alike — and real hardware can't run this branch at all yet regardless (no
   AHCI/NVMe block driver, a separate, unrelated gap).
4. **The interactive stage runs entirely in kernel mode, strictly before the
   existing one-shot ring-3 entry.** `user_mode::enter_persistent_user_process`
   → `ring3_enter_forever_abi` does `iretq` into ring 3 and is `-> !` — it
   never returns to Rust except via fault handling, which terminates rather
   than resumes. The click-driven launch is achieved by moving *when* that
   unchanged, one-shot call happens (after a confirmed click, from a new
   `launcher_screen.rs` kernel-mode poll loop on the kernel's own CR3), never
   by making ring-3 entry itself interruptible or resumable.
5. **The interactive loop busy-polls.** `launcher_screen::run_until_click`
   spins on `ps2::poll_event()`, the same class of temporary, CPU-consuming
   pattern already accepted for COM2's transport in the ADR 0051/0052 plan.
   Not a long-term design, acceptable for this slice.
6. **Real IRQ delivery is proven live, not just asserted on synthetic
   bytes.** `scripts/test-normal-boot-interactive.py` injects a real
   keystroke and mouse click via QEMU's QMP `input-send-event`, through
   QEMU's *emulated* PS/2 controller, and asserts guarded first-fire markers
   (`PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED`/`MOUSE_IRQ_FIRED`) plus the full
   click-to-shell-launch marker sequence. This is a materially stronger proof
   than feeding bytes directly into decode functions, since it exercises the
   real hardware IRQ path end to end.

## Consequences

- Every acceptance script that boots through normal boot to a working shell
  (`test-normal-fast-boot.py`, `test-com2-shell-transport.py`,
  `test-object-shell.py`) now needs to inject a real click via QMP before the
  shell launches — this was wider than originally scoped (only
  `test-normal-fast-boot.py` was anticipated) and is handled by a shared
  helper, `scripts/launcher_click.py`, reused across all three rather than
  duplicated. `test-object-shell.py`'s reboot test injects the click twice
  (once per boot). `test-boot.py` (the verify-feature harness) and
  `test-persistent-storage.py` (also verify-feature) are unaffected —
  `normal_boot` is compiled out entirely under `--features verify`.
- A real PS/2 driver bug was found and fixed during implementation: IRQ12
  (mouse) lives on the slave PIC, and the slave's interrupts only ever reach
  the CPU through the master PIC's cascade line, IRQ2. Unmasking IRQ12 alone
  is not sufficient; IRQ2 must also be unmasked. Documented in `ps2.rs` at
  the point of the fix.
- A known, accepted simplification: `RawInputEvent::MouseMoved`'s `dx`/`dy`
  are `i8` (matching the pre-existing `MouseDriver::decode` interpretation),
  not the full signed 9-bit range the PS/2 protocol's sign/overflow bits in
  byte 0 would allow. Sufficient for a slow, deliberate menu click; not
  sufficient for fast, precise pointing.
- A known, accepted simplification: `MouseAssembler`'s 3-byte packet framing
  only re-validates alignment while in its `Byte0` state (checking the
  protocol's "always 1" bit-3 convention). One stray leading byte costs at
  most one dropped/misread packet before the stream naturally resynchronizes
  on the next byte that fails that check — confirmed live against QEMU,
  where one extra byte was observed arriving via the first post-unmask mouse
  interrupt. Not chased further to a guaranteed-zero-loss design.
- `font::glyph`'s curated character subset (no 'j', 'g', 'm', etc.) means the
  launcher tile reads "Enter Shell," not "Enter Object Shell" — a cosmetic
  constraint of existing, unextended code, not a new gap.
- This closes the interactive-input decision ADR 0047 explicitly deferred.
  What remains explicitly out of scope, unchanged from ADR 0049's precedent:
  real-hardware (USB HID) input, and — separately — real-hardware block
  storage (no AHCI/NVMe driver), which already blocks this whole branch from
  running on real hardware regardless of input.

2026-07-28 note: ADR 0054 later adds a QEMU-verified polling AHCI backend.
The real-hardware storage warning above was true when this ADR was accepted;
physical SATA validation, NVMe, partitions, and filesystems remain later work.

## Alternatives Considered

- **Skip the interactive stage; just play cinematic + audio then launch
  immediately.** Rejected — the user explicitly asked for a click-to-enter
  interaction, and ADR 0047 had already flagged this as the natural next
  decision once input handling existed.
- **Build a full window manager / widget-driven menu system for the
  launcher.** Rejected as disproportionate for one static tile;
  `window_interaction.rs`'s `WindowSlot::contains` hit-test *pattern* is
  reused (reimplemented locally, not by pulling in `DrawableObject`/
  `ObjectId` machinery the static tile doesn't need).
- **Interrupt-driven wakeup instead of busy-polling the launcher loop.**
  Deferred — matches the already-accepted COM2 busy-poll precedent; revisit
  if/when this stage needs to coexist with other CPU-bound work.
