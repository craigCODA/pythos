# ADR 0106: Phase 15 Read-Only Network PCI BAR Layout Probe

Status: Accepted
Date: 2026-09-24

## Context

ADR 0105 established a bounded, read-only PCI identity probe for the QEMU
`e1000`/`e1000e` models and the Lenovo `81VS` target. That slice deliberately
stopped before reading BARs. The next useful hardware question is whether the
network controller's PCI configuration reports a bounded BAR layout that can
be recorded without touching controller registers.

This is a configuration-observation question, not a request to operate the
device. A BAR value is an address/type assignment reported by PCI configuration
space. It is not proof that the address is mapped, reachable, safe to access,
or suitable for a future network implementation.

## Decision

Add a separate opt-in `network-hardware-bar-probe` profile. It reuses the
accepted identity scan and reads only the six standard type-0 BAR slots at
configuration offsets `0x10` through `0x24` for the selected network
controller. It records the raw low dword, any paired high dword, the decoded
I/O or memory type, the 32/64-bit pairing, and the decoded address value as
configuration metadata.

The six offsets are exactly `0x10`, `0x14`, `0x18`, `0x1c`, `0x20`, and
`0x24`.

The decoder is bounded and pure:

- an all-zero slot is unimplemented;
- an I/O BAR records its raw value and masked I/O base;
- a 32-bit memory BAR records its raw value and masked memory base;
- a below-1-MiB memory BAR (memory type bits `01`) is reported distinctly;
- a 64-bit memory BAR consumes the next slot and records both dwords; and
- a reserved type or an invalid 64-bit BAR in slot 5 is reported as malformed
  rather than being followed or guessed.

The probe does not write PCI configuration, probe BAR size by writing all ones,
enable memory or I/O space, enable bus mastering, map a BAR, dereference a BAR,
read a device register, reset or power-manage a controller, allocate DMA,
configure queues, load firmware, enable interrupts, send or receive frames, or
claim controller ownership.

For the bounded decoder fixture, raw low dword `0x0008_0002` represents a
below-1-MiB memory BAR and decodes to base `0x0008_0000`; the low type bits are
retained in the raw field and removed only when computing the base.

The accepted `network-hardware-probe` identity profile remains unchanged. The
new profile has its own marker and framebuffer contract so the identity slice
does not silently acquire a new completion meaning.

`VirtioTransport`, "transport adapter", and `NetworkPort` remain the PythOS
architectural names. This ADR does not add a PCI abstraction, physical-device
memory model, network driver, network consumer, or PythTIG ABI change.

## QEMU oracle

The existing `e1000` and `e1000e` runner option remains the only QEMU device
selection. Each acceptance run uses `-nic none` and exactly one explicit PCI
network device. The oracle requires one matching network controller and a
complete six-slot bounded BAR report. It checks the model's BAR type and
64-bit-pairing invariants and validates the raw-field marker format, but does
not freeze allocator-dependent physical base addresses across QEMU versions.

The QEMU runs remain identity-only plus configuration observation. No netdev
peer or packet behavior is configured or asserted.

## Physical observation gate

After both QEMU runs pass, the same read-only image may be observed on the
Lenovo `81VS`. Physical evidence records the target's raw BAR fields and the
same non-claim boundary. A physical result remains target-specific evidence;
it does not establish BAR reachability, MMIO operation, firmware behavior, DMA,
interrupts, Wi-Fi operation, Ethernet operation, or generalized hardware
support.

## Scope exclusions

This ADR does not authorize BAR size probing, BAR mapping, MMIO or register
access, PCI command writes, bus mastering, DMA, interrupts or MSI/MSI-X, reset,
power management, firmware, controller ownership, queue setup, offloads,
multiqueue, frame movement, Wi-Fi association, a second transport, a new
`NetworkPort` consumer, sockets or protocols, persistent state, or a
generalized PCI or physical-hardware abstraction.

The existing storage `hardware-probe`, Phase 14 `VirtioTransport`, transport
adapter, `NetworkPort`, PythTIG v1 ABI, capability model, and Phase 14
acceptance claims remain unchanged. Later Phase 15 work still requires its own
scope decision.

The boundary contract is no PCI configuration writes, BAR-size writes, BAR
mapping or dereference, MMIO or device-register reads, DMA, interrupts, reset,
firmware, bus mastering, queue setup, frame movement, or controller operation.

## Acceptance

Focused Rust tests must cover I/O, 32-bit memory, below-1-MiB memory, 64-bit
paired memory, unimplemented slots, reserved types, and malformed slot-5
pairing. Python contract tests must prove the separate feature/profile,
ordered markers, `-nic none`, exactly one explicit QEMU device, and absence of
MMIO, DMA, interrupt, bus-master, reset, firmware, frame, and configuration-
write claims.

The live QEMU harness passed once for `e1000` and once for `e1000e`, each with
one controller, a complete bounded BAR report, one successful QEMU outcome, and
the read-only marker. The owner then performed the Lenovo physical observation;
the target-specific values and photo hash are recorded in
`docs/evidence/2026-09-24-phase-15-network-hardware-bar-layout.md`. The slice
is accepted with the same configuration-observation boundary and does not
broaden into BAR reachability or device operation.
