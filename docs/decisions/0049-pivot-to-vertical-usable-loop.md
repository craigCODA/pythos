# ADR 0049: Pivot To One Vertical, Human-Usable Loop

Status: Accepted

## Context

PythOS's real identity is now clear: it is a **capability-secured, object-native
microkernel with Python as the application layer**, not a "Python OS." The Rust
core (PythCore) owns memory, scheduling, hardware, processes, and syscalls;
programs run in isolated CR3 address spaces; authority is capabilities rather
than ambient permissions; data is typed, versioned objects with relationships;
Python sits above the trusted core. That combination — the capability model and
the object system — is the substance. The serpent cinematic gives it personality.

The problem is structural, not a lack of features: **most components prove
themselves and then stop.** Concretely —

- The kernel calls `qemu_exit::success()` after `MILESTONE_1_COMPLETE`; a real OS
  would not exit there.
- The interpreter recognizes exactly one hardcoded `HelloService` source.
- The shell widgets run inside self-tests, not a live session.
- The shell "apps" are fixed representations.

Everything is a beautifully verified sequence of proofs that terminate. Nothing
stays running. The current roadmap continues to complete phases *horizontally*
(Phase 12 paths, 13 apps, 14 networking), which adds more proofs on the same
foundation.

## Decision

Reprioritize near-term work around delivering **one complete, continuously
running, human-usable loop**, as a *vertical* slice through the already-proven
subsystems:

```text
Boot
 -> persistent graphical shell
 -> launch an isolated Python application
 -> create or edit a typed object
 -> save it
 -> reboot
 -> restore the application and the object (with revision history)
```

One working instance of that loop exercises the kernel, compositor, input, the
interpreter, capabilities, storage, and the object model **as one coherent
system** — converting "an impressive collection of kernel proofs" into "a small
operating system that actually works."

Sequenced moves (detailed in the plan):

1. **Separate verification mode from normal boot.** Preserve every existing
   proof/test behind a verification path; a *normal* boot enters a persistent
   event loop instead of exiting via `qemu_exit::success()`. Small, and it
   unblocks everything else.
2. **A real general-purpose Python interpreter in ring 3** (the long pole).
   Port a MicroPython-class runtime into a ring-3 process that reaches PythCore
   only through a narrow, capability-gated syscall API. This is the critical
   path; the loop is blocked on it.
3. **One genuine graphical shell.** Real keyboard/mouse events reach visible
   windows; buttons act; text fields accept input; windows stay movable.
4. **The object system as the application model.** Applications create, link,
   and revise typed objects and receive capabilities only for the objects they
   may access — not files and folders.
5. **One real application: the object browser.** List stored objects, open one,
   edit a field, show revision history, survive a reboot.
6. **Target one reference machine (the laptop).** But build and demo the loop in
   **QEMU first** — real-laptop input is USB HID, a separate driver gap
   analogous to audio; do not let the physical-machine demo block proving the
   loop.

Positioning:

> PythOS is a capability-secured, object-native operating system where isolated
> Python applications work with persistent, versioned objects instead of
> unrestricted files and system access.

## Consequences

- This is a deliberate **re-sequencing** of the roadmap (Phases 12–13 pulled
  into a vertical demo), recorded here rather than drifted into.
- All existing acceptance is preserved: verification mode keeps the serial
  marker oracle and every proof; the normal boot path is the new surface.
- The **interpreter is the critical-path long pole** and should be resourced
  first; the other moves mostly connect things that already exist.
- **Real-hardware input (USB HID) is out of scope** for the QEMU demo of the
  loop; it becomes its own driver milestone, like HDA audio.
- HDA audio and the serpent cinematic are personality/hardware and are **not on
  this critical path** — they stay done, not extended, until the loop lands.
- Success criterion (the demo that proves the system): boot, open the object
  browser, type something, save, reboot, and see the same object and its
  revision history return — exercising kernel, UI, Python, permissions, storage,
  and identity as one system.

## Alternatives Considered

- **Continue horizontal phase completion** (12 → 13 → 14): rejected for now; it
  produces more proofs rather than a running system.
- **Rewrite from scratch:** rejected earlier — a from-scratch OS would re-derive
  the same native-core-hosts-Python architecture and discard ten proven phases.
- **Keep the toy interpreter and fake the loop:** rejected; without a real
  interpreter the loop is theater, and the interpreter is exactly the missing
  substance.
