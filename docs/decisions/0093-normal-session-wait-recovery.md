# ADR 0093: Normal session operation, interrupt-backed waiting and recovery

Status: accepted design, locally implemented and QEMU-accepted on 2026-09-10;
the branch remains unmerged and unpublished pending final controller review.
The single task/evidence map is
`../superpowers/plans/2026-09-09-phase13-5-slice5-normal-session.md`.
Implementation and acceptance are separately evidenced there.

## Scope and base

Phase 13.5 Slice 5 builds locally from
`cb86242ddc6b8daa1ad5a845801d341a9de4bc46` on
`agent/phase13-5-normal-session`. The Slice 3-4 branch and PR #25 stay unchanged.
This document is the design authority; a single implementation map will carry
task checkboxes and evidence after written-contract review. Do not create a
second copy of this design as a separate specification.

The owner approved direct normal-session boot, interrupt-backed input waiting,
Viewing continuity across commands, and recovery-shell fallback on session
failure. Automatic session restart was explicitly excluded from the recommended
design the owner approved. No remote writes or CI polling are authorized.

ADRs 0064-0066, 0089-0092, the frozen PythTIG V1 package contract, and existing
input, command and presentation record layouts remain authoritative. The target
is the existing single-core x86-64 QEMU q35/OVMF configuration and emulated PS/2
input. This is not a general user-process scheduler or physical-input milestone.

## Why the existing acceptance profile cannot become normal boot unchanged

- `normal_boot::run` currently admits service packages, waits at the legacy
  compatibility launcher, then enters `shell.elf`.
- `viewing_orchestration` requires seven specific events, two immutable command
  fixtures and a terminal result record; the final Enter releases acceptance.
- `SessionCommandHost` proves a command read/result exchange. Its object and task
  operations are denied. Its successful fixture acknowledgement is not evidence
  that a user-requested object was created or changed.
- `session_input` binds exactly one owner before PS/2 producers are enabled.
  It has no live rebinding/reset path. Its fixed ring can overflow and reports
  loss through sequence gaps; finite capacity cannot promise lossless bursts.

Production construction and operation must therefore be separated from the
probe-specific fixture, checkpoint and terminal-result code. Keep the old
acceptance profiles runnable with their original meanings.

## Decision 1: One retained normal-session owner

Prepare and validate the normal-session ELF, graph package, read-only launch
description, user stack, copy map, principal identity and separately granted
capabilities before entering user mode. Bind input before PS/2 publication.

The normal session owns its `SessionViewing`, recognizer, sequence tracking,
snapshot revision and command-admission state for the whole process lifetime.
Graph invocation-local tables are fresh for each command. Successful graph
completion must not reconstruct the session owner. A failed, exhausted or
unknown graph exit requests recovery under the existing shared lifecycle rule.

The loop accepts arbitrary valid delivered events, not an event-count script.
Preserve exact no-timeout Space Space Backspace Backspace activation, its
one-way/idempotent semantics, exclusive Traversal/FocusMark routing, and the
ADR 0092 checked snapshot service. Enter is not a session-termination command.
Reboot starts a fresh session; no session state is serialized to storage.

Use a distinct normal-session launch description. Do not repurpose reserved
fields in either bounded bootstrap/result ABI or require acceptance fixtures
in a normal boot image. Pin any new additive ABI values and byte layout in
the implementation map and shared contract before dependent code is written.

## Decision 2: Interrupt-backed wait, not user-space busy polling

Keep `SESSION_INPUT_TRY_READ` nonblocking and unchanged. Add a separately named,
capability-checked scalar wait operation for the normal session. A wait must
not consume an input event or retain a user buffer pointer.

The implementation map pins this additive call at `0x5059_0152`, introduced in
the advertised general syscall ABI 1.2. Existing syscall numbers, error values,
input/presentation records and graph ABI versions remain unchanged. Update the
old compatibility test's exact minor expectation to 2, not a relaxed version
check; no existing in-tree user binary negotiates an exact 1.1 minor.

The wait contract is readiness, not delivery: the caller rechecks its input
and command sources after a wake. Spurious/unrelated interrupt wakes are legal.
The readiness check and transition to interrupt-enabled CPU sleep must exclude
the check-to-sleep lost-wakeup race on the supported single core. No capability
table borrow, queue-slot borrow or lock may survive across the sleep interval.
Validate caller identity and authority before waiting and again before
returning readiness. An invalid or revoked caller is denied without sleeping
or consuming another owner's data.

Pending input returns promptly without sleeping. Timer interrupts may wake the
CPU even when no input is ready; this does not authorize an unbounded spin loop
or a synthetic input event. The normal session does bounded work per wake and
returns to sleep when its sources are empty. No new UART IRQ driver, scheduler
queue, SMP support or busy-poll acceptance limit is part of this decision.

Preserve bounded queue publication from IRQ context. IRQ handlers do not parse
commands, interpret controls, validate capabilities, execute graphs or render.
An overflow keeps the existing drop-newest and gap-reporting semantics. A gap
breaks partial activation recognition, not an already-active FocusMark.

## Decision 3: Honest command continuity and a narrow normal-session surface

Normal boot must contain no prefilled CREATE_NOTE fixtures and must not report
an acknowledgement as a completed object/task operation. Graph invocations
must be caused by admitted live commands, with one actual command read/result
exchange and a fresh invocation context each time.

The initial normal-session command surface is a read-only `status`
request over existing COM2 transport, parsed only in ring 3. It uses the existing
`COMMAND_KIND_SYSTEM_STATUS` type and reports real session/input/presentation
state through the unchanged graph command interface. It does not mint object
IDs, write objects, create tasks, or claim full shell-command parity. Unknown
or unsupported commands are explicitly rejected without graph or storage
effects. The existing recovery shell retains its current command surface.

The old Session Manager source acknowledges only CREATE_NOTE. Normal operation
therefore uses a separate `programs/normal-session-manager/main.pyth` source
that handles SYSTEM_STATUS through those same frozen operations, under the
same graph principal. Its isolated artifact is packaged as the normal bundle's
`session-manager.tig`; old probe/compatibility sources and artifacts stay
unchanged. Host tests compile the real source using a dev-only dependency on
the existing local compiler; no third-party or production dependency is added.

Provide an explicit ring-3 `recover` request so the owner can enter that existing
shell without injecting a fault. It ends this session through the same typed
cleanup path as a failure; it is not a magic raw key or a kernel text parser.
No automatic restart follows either kind of transition. These two COM2 commands
are the approved initial surface, not claims that the previous fixture adapter
already implements them.

This boundary deliberately does not migrate every shell command or add a new
graph opcode, source language, semantic action, click/scroll/zoom behavior or
permission bypass. If full command parity is required, revise this contract
explicitly before implementation rather than expanding the host adapter silently.

## Decision 4: Controlled recovery, with no restart loop

Use the existing contained native-fault return mechanism and shared graph-exit
policy. A native fault, fatal input/presentation error, invalid graph exit, or
explicit recovery request leaves the failed session unable to execute again.

The supervisor must, in order:

1. Recover control through a kernel-owned continuation. The existing return
   mechanism clears the active caller and returnable-process state during this
   return, before the supervisor restores and verifies the kernel address-space
   root. Preserve that earlier deauthorization; both must be complete before
   preparing another user entry.
2. Invalidate the
   session's console, input, command and presentation grants. Track every grant
   made during construction so partial launch failures unwind their own grants.
3. Make the old presenter unavailable and keep the input queue unavailable to
   any new session owner. Do not reset, rebind or transfer the queue while IRQ
   publication is live.
4. Enter the existing recovery shell under its own validated root, bootstrap
   and capability set. Bypass the old click-wait launcher on this path: the
   compatibility input consumer cannot take over a session-bound queue.

The recovery shell uses COM2. New PS/2/USB physical keyboard integration is not
implied. Faulted session memory may remain reserved until reboot if safe
reclamation is not already supported; it must be unreachable by the shell and
never advertised as reclaimed. There is only one failed-session-to-shell
transition per boot, so no unbounded restart allocation is permitted.

If safe recovery construction fails, emit a precise fatal diagnostic and enter
safe idle. Do not re-enter the failed session, leave a stale current caller,
or claim that a shell is ready when it was never entered.

## Decision 5: Normal boot and compatibility gates

On a successful normal boot, enter the retained session directly after required
substrate/service construction and the existing bounded boot presentation.
The compatibility launcher is not the normal-session input owner. The session
viewport remains the bounded ADR 0092 projection; no desktop redesign is added.

Keep the explicit legacy-shell selection and its existing boot/command behavior.
Verification and opt-in Slice 1-4 profiles remain independently selectable.
Do not silently rewrite a legacy shell test into a session test: keep a legacy
profile regression and add a distinct default normal-session acceptance path.
Preserve prior serial markers in the profiles whose contracts require them;
new session markers must describe actual transitions, not simulated outcomes.

Normal operation has no terminal acceptance event and no dependence on QEMU
exit hardware. The external acceptance harness alone decides when it has
observed sufficient live behavior and performs bounded process-tree cleanup.
A harness timeout or a screenshot alone is not success.

## Acceptance gates

Host behavior tests must independently exercise:

- Ready-before-wait, input arriving at the sleep boundary, spurious/timer wakes,
  invalid/revoked/wrong-holder authority, and no data consumption on denial.
- More events and commands than the former seven-event/two-command fixtures,
  with real idle intervals, exact graph results and fresh invocation-local data.
- Activation spanning an idle interval and separate commands, active FocusMark
  movement after another wake, queue overflow/gap behavior, and revision bounds.
- Cleanup order, partial-grant rollback, stale-grant denial, kernel-root/caller
  restoration, one-way recovery, and refusal to restart the failed owner.
- Separate normal and legacy boot selection and preserved bounded probe ABIs.

The new automated QEMU harness must boot the ordinary session image twice,
prove a live ring-3 session remains present while idle, inject real emulated
PS/2 input around waits, submit multiple live read-only status commands, and
validate command replies and pre-exit inactive/active/moved pixels. Session
state must reset on the second boot, with disposable storage hashes unchanged.

A separate fault acceptance image must inject a genuine ring-3 native fault
after successful live input/presentation. It must prove the kernel root and
caller are restored, grants are invalidated, and the existing recovery shell
actually handles a read-only COM2 command. Prove explicit recovery separately.
Test hooks must not be present in the ordinary session image.

Run host workspace/Python tests, formatting, strict affected-profile Clippy,
boot/storage/normal-legacy acceptance, Slice 1 input delivery, Slice 2 normal and
fault probes, legacy Viewing, and integrated Slice 3-4 acceptance. Add the
normal-session gates without dropping existing CI protection. Do not inspect
hosted CI or publish the branch unless the owner later requests it.

## Non-goals and stop conditions

No automatic restart, durable sessions, broad command migration, USB/xHCI
session delivery, new physical acceptance, network/update/AI work, SMP, general
user-process scheduling, later PythTIG phases or new graph semantics.

Stop if the wait cannot satisfy the single-core wakeup and authority invariants,
the recovery shell requires live input rebinding, a new syscall would collide
with another active contract, or a frozen layout/old acceptance must be weakened.
Bring the specific conflict back to the owner instead of silently broadening
this slice. Local QEMU acceptance will not mean production-hardware acceptance
or completion of the whole operating system.

## Local preparation evidence

- Base and PR branch: `cb86242ddc6b8daa1ad5a845801d341a9de4bc46`.
- Worktree: `.worktrees/phase13-5-normal-session`.
- Fresh `cargo test --workspace --quiet`: 1029 passed; pre-existing host warnings
  remain. Log: `target/slice5-baseline-rust.log`.
- Fresh `uv run --no-project --with pytest python -m pytest tests -q --tb=short`:
  176 passed and 148 subtests passed.
- No implementation, new QEMU acceptance, remote mutation or physical write
  has occurred in preparing this design.

## Local implementation and acceptance evidence

- `scripts/test-normal-session.py` passes a separate native-fault boot and an
  ordinary two-boot acceptance. The ordinary run proves live idle/wake/status,
  exact inactive/active/moved pixels, explicit recovery, recovery-shell `help`,
  six captures, fresh per-boot state, byte-exact raw COM2 capture, and unchanged
  disposable storage.
- The native-fault run proves the captured vector 6 context, ELF-derived RIP,
  restored kernel/caller state, checked grant cleanup, and actual recovery-shell
  command handling. The normal-only fault context diagnostic is emitted only
  after restoration and before the recovery marker.
- Fresh closeout passed 1,070 Rust tests, 195 Python tests plus 229 subtests, all
  16 required predecessor QEMU gates, both strict normal user-binary profiles,
  and five strict complete kernel proof profiles.
- The actual default-normal core strict Clippy check fails with 871 dead-code
  diagnostics; the separately run compatibility baseline fails with 874. This
  is baseline debt plus profile-induced unused compatibility code, not an
  identical inherited diagnostic set, and no warning suppression was added.
- No remote mutation, merge, publication, USB write, physical acceptance, or
  later-phase implementation is claimed.
