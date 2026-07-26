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

### Slice 2: Real Python interpreter in ring 3 (the long pole)

- Decision spike + ADR: adopt a MicroPython-class runtime (vs. growing the
  custom interpreter). Port it `no_std`, into a ring-3 process, with a narrow
  capability-gated `system.*`/syscall surface (no ambient host access).
- Prove it runs an *arbitrary* small Python program (not the hardcoded
  `HelloService`), calling one capability-gated host function.
- Exit: an arbitrary Python snippet supplied at runtime executes in ring 3 and
  performs a capability-checked host call.

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
