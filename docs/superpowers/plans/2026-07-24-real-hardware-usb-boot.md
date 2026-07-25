# Real-Hardware USB Boot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the milestone-1 loader produce observable on-screen progress on a real UEFI PC and read `PYTHCORE.ELF` from the device it was booted from, so a USB boot stops failing as a silent black screen.

**Architecture:** Loader-only changes. Add direct-framebuffer progress paints keyed to existing loader milestones, and switch filesystem discovery from `LocateProtocol(SimpleFileSystem)` to `LoadedImage -> DeviceHandle -> SimpleFileSystem`. Verify with the existing QEMU serial-marker oracle plus a visual framebuffer check, then redeploy to the F: USB ESP.

**Tech Stack:** Rust `no_std` UEFI loader, existing `PythFramebufferInfo` ABI, QEMU/OVMF, PowerShell elevated deploy to the physical ESP.

**Design:** `docs/superpowers/specs/2026-07-24-real-hardware-usb-boot-design.md`

## Global Constraints

Implement only the active milestone.
Do not invent or silently change an ABI.
Do not add future features to the active milestone.
Every unsafe block requires a documented invariant.
Serial output is the test oracle for early boot; a screenshot is not sufficient evidence.
A successful compile is not a successful boot.
Do not change Secure Boot signing, ACPI, timers, or general multi-disk storage in this slice.
AI remains outside the trusted core.

---

### Task 1: Loader Framebuffer Progress Paint

**Files:**
- Modify: `boot/src/graphics.rs` (or add `boot/src/fb_debug.rs`)
- Modify: `boot/src/main.rs`

**Interfaces:**
- Consumes: the `PythFramebufferInfo` returned by `initialize_gop`.
- Produces: deterministic solid-color fills at `GOP_READY`, `KERNEL_LOADED`, `MEMORY_MAP_READY`, pre-`ExitBootServices`, and in `fail()`.

- [ ] Add a bounded `fill_framebuffer(fb, rgb)` that writes only within `byte_length`, honors `pixels_per_scanline` pitch, and supports RGB/BGR/masked formats. Document the single `unsafe` framebuffer-write block with the full invariant checklist.
- [ ] Call it after each loader milestone with a distinct color, and a unique color in `fail()`.
- [ ] `cargo build -p pythos-boot --target x86_64-unknown-uefi` and `cargo clippy ... -- -D warnings` clean.
- [ ] `python scripts/test-boot.py --slice milestone-1` still reaches `PYTHOS:CORE:MILESTONE_1_COMPLETE` (paint must not disturb serial markers or the framebuffer handed to PythCore).
- [ ] Visually confirm in QEMU (`-display` on) that the progress colors appear in order.

### Task 2: Boot-Device Filesystem Discovery — ALREADY IMPLEMENTED

**Finding (2026-07-24):** This was already done in commit `611c8ca` ("core: complete
boot metadata validation"), before this plan was written. `boot/src/uefi.rs`
provides `open_boot_volume(system_table, image_handle)`, which resolves
`HandleProtocol(image_handle, LoadedImage) -> DeviceHandle ->
HandleProtocol(DeviceHandle, SimpleFileSystem) -> OpenVolume`. `elf.rs`,
`initrd.rs`, and `font.rs` all read through it; `locate_protocol` remains only for
GOP (a legitimately firmware-global protocol). No code change is needed.

**Consequence for the diagnosis:** the "loader reads PYTHCORE.ELF from the wrong
disk" hypothesis is already mitigated in-tree, and the loader on the tested USB
(built after `611c8ca`) already had it. So it is unlikely to be the cause of the
real-hardware black screen. The live candidates become early GOP failure/mode
rejection and other real-hardware divergence — which Task 1's paints now localize.

- [x] Filesystem access uses the loaded-image device handle, not `LocateProtocol` (pre-existing, `611c8ca`).
- [x] Bounded `LoadedImageProtocol` + device-handle `SimpleFileSystem` resolution with documented `unsafe` (pre-existing).
- [x] `elf.rs`, `initrd.rs`, `font.rs` route through `open_boot_volume` (pre-existing).
- [x] `--slice milestone-1` and `--media iso` reach `MILESTONE_1_COMPLETE` (verified during Task 1).
- [ ] Optional debt: no ADR records this switch; backfill one in Task 3 or a follow-up.

### Task 3: ADR, Docs, Redeploy, Verify

**Files:**
- Create: `docs/decisions/00NN-loader-boot-device-filesystem.md` (next free ADR number)
- Modify: `docs/HANDOVER.md`

**Interfaces:**
- Documents: the discovery-source change and the framebuffer-diagnostic contract.
- Produces: refreshed F: USB ESP.

- [ ] Write the ADR recording the switch to loaded-image device-handle discovery (context, decision, security/compat/testing impact, rollback).
- [ ] Note the loader progress-color contract in `docs/HANDOVER.md`.
- [ ] `cargo fmt --check`, both target builds, both clippy targets, `python -m unittest tests.boot_core_handoff`, `python scripts/test-boot.py --slice milestone-1`, `--media iso`.
- [ ] Rebuild `image/esp`, rerun the elevated deploy to disk 2 / partition 2 (F: USB ESP) with its existing USB + ESP-type safety guards.
- [ ] Commit in small, purpose-scoped commits with no co-author trailer.

## Real-Hardware Verification (user-driven)

Not automatable on this host. After redeploy, the user boots the USB with Secure Boot **off** and reports the on-screen result:

- Black screen → loader not reached or GOP failed before first paint.
- First color then stall → loader ran, GOP works; investigate next stage.
- Colors advance → record how far; this is the real signal the QEMU log cannot provide.

Record the observed screen state honestly as the only evidence of real-firmware behavior. QEMU success remains necessary but not sufficient.

## Real-Hardware Results (2026-07-25)

- **Laptop: full success.** The USB booted through the loader, the CR3 handoff,
  all of PythCore early init, and reached the milestone-1 cinematic wake screen
  (`PythOS [HISS] We Are Woken`). First confirmed real-hardware boot; this laptop
  is now the working real-hardware oracle.
- **Desktop: magenta at pre-handoff.** Screen stopped at the loader's
  `COLOR_EXIT` (painted after `ExitBootServices`, before `enter_pythcore`) with
  no PythCore white liveness paint. That isolates the fault to the `CR3` switch
  / first PythCore instruction. Diagnosed as the loader's identity map covering
  only `2 MiB .. 4 GiB` while this firmware places the loader / boot structures
  above 4 GiB.
- **Fix applied (ADR 0046):** loader now identity-maps the low 512 GiB using
  1 GiB huge pages (2 MiB pages for the low 1 GiB null guard; 2 MiB fallback to
  4 GiB when the CPU lacks 1 GiB pages). QEMU milestone-1 and the laptop still
  pass. **Not yet verified on the desktop** — deferred; the fix is well-reasoned
  but the desktop is the only place it can be confirmed.
- **Deferred follow-ups:** verify the 512 GiB fix on the desktop; optionally gate
  the diagnostic paints to failure-only so healthy boots go straight to the wake
  screen.
