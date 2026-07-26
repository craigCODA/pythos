# Vertical Usable Loop — Implementation Plan

> Implements ADR 0049. Turn the proven subsystems into one continuously running,
> human-usable loop, demonstrated in QEMU first. Preserve every existing proof
> behind a verification path.

**Goal (the demo that proves the system):** Boot into a persistent shell, launch
an isolated Python app (the object browser), create/edit a typed object, save it,
reboot, and see the same object and its revision history restored.

**Design:** `docs/decisions/0049-pivot-to-vertical-usable-loop.md`.

## Global Constraints

- Preserve verification mode: all existing serial-marker proofs and tests keep
  running under an explicit verification path; do not delete proofs.
- The normal (non-verification) boot must not call `qemu_exit::success()` — it
  enters a persistent event loop.
- Isolation and capability rules are not relaxed: the Python app runs in ring 3
  and reaches the kernel only through the capability-gated syscall API.
- Demo target is QEMU (PS/2 input, virtio-blk storage). Real-laptop input (USB
  HID) is a separate later milestone; do not block the loop on it.
- Each slice has an observable result (a serial marker in verification mode
  and/or a visible on-screen effect in normal mode).

## Slices

### Slice 1: Verification / normal boot split (move #1)

- Introduce a boot mode selector (build feature or boot-arg): `verify` runs the
  existing proof sequence and exits via `qemu_exit::success()` exactly as today;
  `normal` skips the self-tests after core init and enters a persistent event
  loop (idle + scheduler tick) that never exits.
- Keep the milestone-1 serial oracle intact under `verify`.
- Exit: `verify` reproduces `MILESTONE_1_COMPLETE`; `normal` boots and stays
  running (observable: it does not exit; a heartbeat marker/frame continues).

### Slice 2: Python interpreter in ring 3 (the long pole) — reordered per ADR 0050

Superseded ordering: the interpreter does **not** come first. The host-object
seam is defined and proven in Rust *before* the port, so MicroPython binds to a
fixed target. Sub-slices, in order:

**2a — SSE/FPU userspace enablement (start cold; small but corruption-prone).**
- Confirm the kernel emits no SSE (`objdump -d pythcore | grep -i xmm` empty);
  design against the fact, not the target default.
- `fninit` + CR0 (clear EM, set MP) + CR4.OSFXSR/OSXMMEXCPT once at boot.
- Eager FXSAVE/FXRSTOR on user-task context switch (no lazy CR0.TS/#NM —
  CVE-2018-3665). AVX pinned off both toolchains so FXSAVE is complete.
- TSS.RSP0 = the running user task's kernel stack on every entry to ring 3, so a
  timer tick mid-C switches stacks safely.
- Exit: a ring-3 task doing hardware-float + a preemption survives with FPU state
  intact — proven by an ordered marker sequence.

**2b — Host-object seam in Rust (was "slice 4"; do it before the port).**
- Implement the ADR 0050 seam (~9–10 ops incl. opaque integer handles,
  `retain`/`release`, `type_of`, identity) against the existing typed-object
  store (ADR 0022 / revisions / relationships).
- Completion criterion is the oracle, not "looks reasonable": a Rust harness
  drives every seam op against the store and emits an ordered marker sequence.
- Exit: the marker harness passes for every seam operation.

**2c — Port MicroPython behind the seam.**
- Vendor at a pinned commit; link `-static -no-pie` (ET_EXEC);
  `-msse2 -mno-avx…`; `MICROPY_FLOAT_IMPL_NONE`; set `MP_STATE_THREAD(stack_top)`.
- Instrument syscall / interpreter / GC entry-exit markers **before** dropping
  the C blob in.
- Exit: `eval_str` runs an arbitrary Python snippet in ring 3 that performs a
  capability-checked host call through the seam.

With 2b done, plan slice 4 (object system as app model) is largely subsumed; the
remaining slices (3 shell, 5 object browser, 6 round trip) are unchanged.

### Slice 3: Live graphical shell

- Wire real keyboard/mouse events through the input service to the compositor so
  they reach focused windows; buttons invoke actions, text fields accept input,
  windows move — in a persistent session, not a self-test.
- Exit: on-screen, a user can focus a window, click a button that does
  something, and type into a field.

### Slice 4: Object system as the application model

- Expose object create/link/revise + capability-scoped access to a ring-3 app
  through the syscall API: an app receives capabilities only for the objects it
  may touch.
- Exit: a ring-3 app creates a typed object, edits a field (new revision), and
  is denied access to an object it holds no capability for.

### Slice 5: The object browser as a real application

- Turn the object-browser proof into a live app: list stored objects, open one,
  edit a field, show revision history — driven by real input, rendered live.
- Exit: the browser is usable on screen against the live object store.

### Slice 6: The round trip

- Persist the created/edited object through storage, reboot, and restore the app
  and the object with its revision history — the full loop, end to end.
- Exit: reboot demo — same object + revision history return; proven by an
  automated reboot test plus the on-screen result.

## Real-Hardware Note (later)

Once the loop works in QEMU, a real-laptop demo additionally needs USB HID
input. That is a separate driver milestone (like HDA audio) and is explicitly not
a blocker for proving the loop.
