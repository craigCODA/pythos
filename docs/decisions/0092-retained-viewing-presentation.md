# ADR 0092: Retained Viewing and bounded snapshot presentation

Status: implementation decision for the owner-invoked Phase 13.5 Slices 3 and 4
(2026-09-09); acceptance pending. Does not authorize Slice 5.

## Context and preserved contracts

ADRs 0089, 0090 and 0091 remain authoritative. The owner explicitly invoked
parallel implementation of Slices 3 and 4. The accepted Slice 2 build, bootstrap,
result layouts, marker sequence and two-command graph proof stay available
unchanged. PythTIG and session-input V1 are unchanged. Default boot, production
wait/wakeup, durable state and physical USB acceptance are outside this work.

## Session policy

Hardware-neutral input types, SessionControlInterpreter and ViewingState move
into pythos-shared; core retains compatibility re-exports. A retained ring-3
SessionViewing owner holds one recognizer, one ViewingState and continuity
tracking outside graph-local working tables. It validates each wire event,
observes controls once, applies commands, routes motion exactly once and returns
a by-value receipt and read-only ViewingSnapshot. Keys and buttons also produce
snapshots; raw button state has no semantic action.

All source/kind combinations, tags, flags and reserved fields are checked.
Wire i32 motion is narrowed to existing i8 semantics with checked conversion;
out-of-range motion is a typed error, never truncation or silent clamping.
Expected sequence advances with wrapping_add(1). The first event must be zero
unless GAP_BEFORE explicitly reports loss. GAP_BEFORE clears only incomplete
activation recognition before dispatching its event; active focus, position and
Traversal are retained. Malformed events and unflagged discontinuities clear
partial recognition and poison this owner into a typed recovery-required state;
no later event is dispatched by that owner. This prevents accidental gesture
continuity after callers ignore an error.

## Snapshot transport V1

A new scalar-only syscall SESSION_VIEWING_PRESENT (0x5059_0151) takes:

| Argument | Meaning |
| --- | --- |
| arg0 | opaque presentation capability |
| arg1 | revision, starting at zero, then checked increment by one |
| arg2 | flags: 0 inactive, 1 active; all other bits invalid |
| arg3 | x in low 32 bits, y in high 32 bits |
| arg4 | reserved zero |

Inactive coordinates must be zero; active coordinates must lie within the
kernel-bound extent. Revision exhaustion is a typed denial, not wraparound.
The separate resource 0x1A50_0101 requires SEND, bound to the retained session
ServiceId and granted only to the runtime principal. Neither INPUT authority
nor the graph command binding grants presentation. Normal capability generation,
resource, right, holder and active-caller checks precede every effect. Rejections
leave the accepted revision and displayed snapshot unchanged. Success uses the
existing SYSCALL_OK value and means drawn synchronously, not queued.

The renderer consumes only kernel-owned by-value snapshots; it decodes no input,
recognizes no controls and holds no pointer to user memory. The bounded profile
owns a 640 x 480 black viewport, provided the validated framebuffer contains it.
The initial viewport clear occurs before user entry. Recurring submissions erase
only the prior FocusMark footprint and draw the new four separated L corners;
work is bounded independently of framebuffer size. No text/font mapping is
needed. Validation and surface construction precede pixel mutation, so renderer
failure cannot publish an accepted revision or partially clear the old frame.

Synchronous syscall projection is outside IRQ handlers, but syscall entry masks
interrupts. Its small bounded pixel work is an explicit limitation of this
acceptance bridge, not a production latency claim. The retained user root maps
the framebuffer supervisor-only and non-executable, and verifies that ring 3
cannot access it. No framebuffer address or device capability is given to user
code. A production deferred presenter and scheduling remain later decisions.

## Opt-in profile and acceptance extension

The kernel feature session-viewing-probe includes session-runtime-probe; the
user session-runtime feature session-viewing selects its separate coordinator.
The default runtime coordinator and its V1 acceptance contract remain intact.
Build output is isolated under target/session-viewing-probe.

A separately versioned SessionViewingBootstrapV1 record resides at offset 2048
in the existing read-only bootstrap page, not in V1 reserved fields. It is
64 bytes, aligned to 8: magic u64 (0x3142_5745_4956_5950, PYVIEWB1), major u16
(1), minor u16 (0), reserved0 u32 (0), presentation capability u64, width u32,
height u32, reserved1 [u64; 4] (zero). The caller validates the entire page
subrange before reading, exact identity/version/reserved values, distinct
nonzero capability and the bounded extent. Existing bootstrap still points to
an unchanged V1 result; an independent versioned Viewing result may occupy
offset 2048 in its writable result page. Its exact layout and validation must
be recorded here before implementation is accepted.

The two immutable CREATE_NOTE fixtures, graph package and independent graph
invocation validation are reused. The bounded seven-event script is motion,
Space, Space, Backspace, Backspace, motion, Enter (sequences 0 through 6).
Invocation one runs after the second Space; invocation two after active motion.
Thus recognizer and Viewing ownership cross graph-local reset. Initial snapshot
is revision 0; every accepted event produces revisions 1 through 7. Enter is
only the final acceptance release signal, never a production lifecycle command.

QEMU must observe inactive, activated and moved frames while ring 3 is still
running, then release the final Enter and validate the terminal records. Two
boots prove reset; storage hashes prove no writes. Real emulated PS/2 delivery,
COM1/COM2 ordering and independent pixel oracles jointly establish acceptance.
Timeouts, terminal-only rendering and screenshots alone are not success.
