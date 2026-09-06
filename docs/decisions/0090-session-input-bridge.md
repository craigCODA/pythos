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

The mechanism has one fixed shared ABI version 1.0. Its nonblocking
`SYSCALL_SESSION_INPUT_TRY_READ` takes an opaque, generation-checked
`PackedCapability`, an exactly sized naturally aligned writable output buffer,
and two zero reserved arguments. It returns either one fixed 40-byte
`SessionInputEventV1` record or `EMPTY`; `EMPTY` leaves the output buffer
unchanged. The fixed input resource is `0x1A50_0100` with the existing `INPUT`
right. Its event tags cover only key-down, relative motion, and left-button
state, with explicit logical-key values rather than PS/2 or USB codes.

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
