# ADR 0046: Real-Hardware Loader Handoff Identity Map And Boot Diagnostics

Status: Accepted

## Context

Milestone 1 was only ever validated inside QEMU/OVMF, where every diagnostic
(`PYTHOS:LOADER:*`, `PYTHOS:CORE:*`, `FAIL`, `PANIC`) is emitted over COM1
serial. A real laptop and desktop expose no serial port, so any early
loader/core fault on real hardware is visually indistinguishable from a hang:
the screen is black and there is nothing to read.

Booting a physically prepared USB (GPT, valid ESP, byte-identical boot tree) on
real UEFI machines revealed two things QEMU could not:

1. On one desktop the screen stopped at the loader's final pre-handoff state and
   never entered PythCore. Diagnosis narrowed this to the loader's temporary
   page tables: `boot/src/paging.rs` identity-mapped only physical `2 MiB .. 4
   GiB`. Real firmware may load `BOOTX64.EFI` — and place the `AllocatePages`
   allocations PythCore reads (`PythBootInfo`, the retained UEFI memory map,
   `INIT.PAK`) — **above 4 GiB** on machines with more RAM. The instruction
   fetched immediately after `mov cr3` then hits an unmapped address and
   triple-faults. QEMU (and at least one laptop) keep everything below 4 GiB, so
   they never hit this.
2. Without an on-screen signal there was no way to localize *where* a real-
   hardware boot died, because serial is invisible on these machines.

## Decision

Two loader/core changes, both confined to the milestone-1 boot path with no ABI
change:

1. **Extend the loader's identity map to the low 512 GiB.** `map_identity` maps
   the low 1 GiB with 2 MiB pages (preserving the sub-2 MiB null/low-address
   guard hole), then covers `1 GiB .. 512 GiB` with 1 GiB huge pages when
   `CPUID.80000001h:EDX.PDPE1GB` is set — a single PDPT, no extra table-pool
   cost. CPUs without 1 GiB pages fall back to the prior 4 GiB ceiling with
   2 MiB pages. 512 GiB is one PML4 entry and covers all plausible
   consumer/workstation RAM.

2. **Paint the framebuffer at each early milestone** (`boot/src/fb_debug.rs`,
   `core/src/fb_debug.rs`). The loader paints solid colors at GOP ready, kernel
   loaded, memory-map ready, and immediately before `ExitBootServices`, plus a
   distinct color in `fail()`/panic. PythCore does a format-independent white
   "liveness" paint as its very first instruction — before serial or reading
   `boot_info` — then repaints at boot-info-valid, memory-ready, IDT-ready, and
   after its own CR3 switch. Each paint writes only within the framebuffer
   bounds through the already-validated direct framebuffer, using no UEFI
   services and no new ABI. On real hardware the color reached bisects exactly
   how far boot progressed with no serial cable.

## Consequences

- A machine whose firmware loads the loader or PythCore's boot structures above
  4 GiB now survives the `CR3` switch instead of silently triple-faulting.
- On real hardware a black screen becomes a signal: the last color shown names
  the last milestone reached; PythCore's white liveness block distinguishes
  "handoff faulted" (loader's last color persists) from "PythCore is running"
  (white appears) without a serial port.
- The identity map now covers regions that may not be backed by RAM (MMIO holes,
  unpopulated space). This is safe: entries are created but never accessed;
  PythCore only touches real allocations and the framebuffer.
- Verified: milestone 1 still reaches `PYTHOS:CORE:MILESTONE_1_COMPLETE` in QEMU,
  and a laptop boots the full milestone-1 cinematic wake screen
  (`PythOS [HISS] We Are Woken`) from USB — the first confirmed real-hardware
  boot. One desktop still stops at the loader's magenta pre-handoff state; the
  512 GiB identity map is the leading fix but is not yet verified on that
  specific machine (deferred; see the real-hardware-usb-boot plan).
- These are temporary loader-owned tables and diagnostic scaffolding for
  bring-up. The diagnostic paints currently fire on every boot, including
  successful ones; gating them to failures only is a deferred follow-up.
