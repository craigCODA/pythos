# ADR 0090: Session Input Bridge

Date: 2026-09-06

Status: Accepted design; implementation pending

## Context

ADR 0089 defines the semantic ownership of normalized input: root/session
controls interpret activation and the active `ViewingState` owns routing and
cursor state. PythCore needs a smaller privileged mechanism that can deliver
normalized input to one authorized ring-3 consumer without giving hardware,
queue, or framebuffer authority to that consumer.

## Decision

ADR 0089 remains authoritative for all Viewing and activation semantics.
ADR 0090 decides only the privileged delivery mechanism beneath that boundary.

## Fixed ABI

The shared session-input ABI is version 1.0:

```text
SESSION_INPUT_ABI_MAJOR = 1
SESSION_INPUT_ABI_MINOR = 0
SYSCALL_SESSION_INPUT_TRY_READ = 0x5059_0150
SESSION_INPUT_RESULT_EVENT = 0x5059_004F
SESSION_INPUT_RESULT_EMPTY = 0x5059_0150_0000
SESSION_INPUT_RESOURCE_ID = 0x1A50_0100
```

The fixed input resource uses the existing `INPUT` right. The nonblocking
`try_read(input_capability, output_pointer, output_length, 0, 0)` syscall takes
an opaque, generation-checked `PackedCapability`, an exactly sized naturally
aligned writable output buffer, and two zero reserved arguments. `EVENT`
means one record was copied out. `EMPTY` means no event was available and
leaves the output buffer unchanged.

`SessionInputEventV1` is a 40-byte, 8-byte-aligned, little-endian `repr(C)`
record with this exact layout:

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 8 | `sequence: u64` |
| 8 | 2 | `kind: u16` |
| 10 | 2 | `source: u16` |
| 12 | 4 | `flags: u32` |
| 16 | 4 | `value0: i32` |
| 20 | 4 | `value1: i32` |
| 24 | 8 | `reserved0: u64` |
| 32 | 8 | `reserved1: u64` |

The numeric event tags are:

```text
SESSION_INPUT_SOURCE_KEYBOARD = 1
SESSION_INPUT_SOURCE_MOUSE = 2

SESSION_INPUT_KIND_KEY_DOWN = 1
SESSION_INPUT_KIND_RELATIVE_MOTION = 2
SESSION_INPUT_KIND_MOUSE_BUTTON_STATE = 3

SESSION_INPUT_FLAG_GAP_BEFORE = 0x0000_0001
```

All unknown flags and both reserved fields are zero on output. Logical-key
values are explicit PythOS ABI tags, not Rust discriminants, PS/2 scancodes,
or USB usages:

```text
KEY_A..KEY_Z = 0x0001..0x001A
KEY_DIGIT0..KEY_DIGIT9 = 0x0020..0x0029
KEY_ENTER = 0x0030
KEY_ESCAPE = 0x0031
KEY_SPACE = 0x0032
KEY_BACKSPACE = 0x0033
```

For `KEY_DOWN`, `value0` is the logical-key tag and `value1` is zero. For
`RELATIVE_MOTION`, `value0` and `value1` are signed `dx` and `dy`. For
`MOUSE_BUTTON_STATE`, `value0` is left state zero or one and `value1` is zero.
This ABI has no click, double-click, selection, activation, drag, or focus
meaning.

There is exactly one consumer mode per boot. Default boot keeps the existing
compatibility consumer. The opt-in proof binds one exclusive session consumer
before the producer starts, flushes stale queued input, and denies the
compatibility dequeue. No broadcast, live handoff, unbind, multi-seat, or
subscriber revocation is decided here.

The fixed queue retains 16 storage slots with 15 usable entries and drops the
newest candidate when full. Every candidate consumes a wrapping sequence
number, including a drop. The first later delivered discontinuity carries
`GAP_BEFORE`; consumers must not treat input across that gap as continuous.
The receive syscall is nonblocking because no accepted session sleep/wakeup
contract exists yet.

The ring-3 consumer is a finite, opt-in `session-input-probe.elf`. Its input
and COM2 evidence capabilities arrive in `RDI` and `RSI` through a probe-only
launch convention. This is not a durable session bootstrap ABI, a Session
Manager, or a normal-boot cutover.

## Non-goals

This decision does not interpret `Space Space Backspace Backspace`, activate
or deactivate a cursor feature, instantiate or route `ViewingState`, move or
render a FocusMark, change Traversal, change framebuffer behavior, or change
the default launcher/shell path. It does not add PythTIG v1 operations,
persistent sessions, xHCI/USB/HID production delivery, click/drag/scroll/zoom
semantics, storage writes, physical-input support, or a hardware expansion.

## Evidence Boundary

QEMU acceptance will prove emulated PS/2 IRQ-to-ring-3 delivery, not physical
or USB input. It will require separate COM1 and COM2 evidence, no disk writes,
and the existing QEMU success contract. No physical, USB, xHCI, Viewing, or
default-boot evidence is promoted by this decision.
