# ADR 0109: Phase 15 Bounded PCI Capability Metadata Snapshot

**Status:** Proposed for owner review
**Date:** 2026-09-29
**Owners:** PythOS platform and transport-adapter maintainers

## Context

The accepted Phase 15 opening slices now establish four deliberately separate
facts about the selected network controller:

1. ADR 0105 identifies the PCI function.
2. ADR 0106 records its standard PCI BAR configuration.
3. ADR 0107 proves one fixed register observation when PCI Memory Space Enable
   is already set.
4. ADR 0108 records one bounded Memory-Space-Enable set/read/restore
   experiment.

Those slices do not establish which optional PCI capability structures the
controller advertises. That metadata is useful before any later controller
operation is considered, but reading it does not require enabling a capability,
mapping a BAR, touching a device register, or handling an interrupt.

The next question is therefore narrow:

> Can PythOS safely snapshot the selected controller's bounded conventional PCI
> capability-list metadata and interrupt-routing metadata without changing PCI
> configuration or operating the controller?

The QEMU `e1000` and `e1000e` models remain the deterministic reference targets.
The Lenovo `81VS` result remains target-specific evidence for Realtek
`10EC:C82F`; QEMU does not establish the meaning of that Realtek controller's
capabilities.

## Decision

Add a separate opt-in `network-hardware-capability-probe` profile. It reuses
the accepted identity and PCI configuration access boundary and performs only
read-only accesses to the selected function's conventional PCI configuration
header and capability list.

The probe reads and reports:

- BDF, vendor/device, subsystem, class, subclass, and programming interface;
- the PCI status Capabilities List indicator;
- the interrupt line and interrupt pin bytes from the standard header;
- the conventional capability-list pointer at configuration offset `0x34`;
- for each bounded capability entry, its capability ID, configuration offset,
  next-pointer value, and a bounded header classification;
- presence and offset of the recognized PCI Power Management (`0x01`), PCIe
  (`0x10`), MSI (`0x05`), and MSI-X (`0x11`) capability IDs.

The probe does not interpret or change capability control fields. It does not
enable MSI/MSI-X, change power state, configure PCIe, or infer device readiness
from capability presence.

### Traversal contract

The conventional list walk is bounded and fails closed:

- the initial pointer must be zero or a 4-byte-aligned configuration offset in
  the conventional capability range `0x40..=0xFC`;
- each nonzero next pointer must satisfy the same range and alignment rule;
- no entry may be visited twice;
- the walk has a fixed maximum entry count of 48;
- a malformed pointer, repeated offset, or exhausted bound produces a visible
  malformed/safe-skip result and never a ready result;
- a zero pointer terminates the list successfully;
- if the status register says no capability list is present, the probe emits a
  deterministic empty-list result rather than treating that as a controller
  failure.

The probe records the bounded header span for recognized capability types only
when the capability-specific header minimum is available within configuration
space. Unknown capability IDs are recorded as opaque entries with their ID,
offset, next pointer, and no inferred semantic length. This avoids inventing a
generic capability layout for capabilities not covered by this ADR.

All configuration reads are ordinary read-only PCI configuration accesses. No
BAR is mapped or dereferenced, and no MMIO or device-register access occurs.

## Lifecycle and evidence contract

The accepted profiles remain independent. This profile does not call the
register-reachability or MSE experiment path and must not alter the ordering or
behavior of ADRs 0105-0108.

The ready-path transcript is ordered as follows:

```text
...:ENTER
...:PCI_SCAN_READY
...:NETWORK_CONTROLLER_FOUND
...:PCI_CONFIG_HEADER_READY
...:PCI_CAPABILITIES_STATUS=0x................
...:PCI_CAPABILITY_POINTER=0x..
...:PCI_CAPABILITY_LIST_PRESENT or ...:PCI_CAPABILITY_LIST_ABSENT
...:PCI_CAPABILITY_ENTRY=...
...:PCI_CAPABILITY_SUMMARY=...
...:PCI_INTERRUPT_METADATA=...
...:PCI_CONFIG_READ_ONLY
...:FRAMEBUFFER_CAPABILITY_READY
..._READY
```

The implementation plan will assign the exact marker tokens and fixed
framebuffer text, but the ordering and terminal distinction are part of this
design:

- `READY` means the bounded configuration snapshot completed and the result
  panel rendered;
- an empty list is a successful bounded observation when the header says no
  list is present;
- malformed pointers, repeated entries, unsupported controller identity, or
  missing required framebuffer output produce a terminal diagnostic result and
  never emit `READY`;
- no result is interpreted as evidence of link state, firmware state, device
  readiness, power control, interrupt delivery, packet movement, or Wi-Fi.

## Acceptance

1. Pure tests prove capability-pointer range/alignment validation, loop
   rejection, maximum-entry enforcement, empty-list handling, unknown-ID
   preservation, and recognized-capability header bounds.
2. Static contract tests reject PCI configuration writes, BAR writes, MMIO,
   device-register access, bus mastering, DMA, interrupts, MSI/MSI-X enablement,
   reset, queues, packets, sockets, NetworkPort markers, and Wi-Fi behavior.
3. QEMU runs with exactly one explicit `e1000` or `e1000e`, `-nic none`, no
   backend peer, no non-boot block device, and one `QEMU_OUTCOME success`. The
   oracle requires the ordered capability snapshot, framebuffer result,
   `PCI_CONFIG_READ_ONLY`, and the terminal ready marker.
4. The Lenovo observation, if separately authorized after QEMU acceptance,
   uses a uniquely named ISO under `F:\iso`, preserves all existing images, and
   records the capability result and final framebuffer. It does not proceed to
   power control, interrupt setup, controller initialization, DMA, queues,
   packet movement, or Wi-Fi association.
5. Existing identity, BAR, register-reachability, MSE, default, and
   normal-session profiles remain unchanged.

## Explicit non-goals and next boundary

This ADR does not create a generalized PCI configuration API, PCI capability
framework, modern Virtio PCI capability implementation, MMIO abstraction,
network driver, `VirtioTransport` implementation, `NetworkPort` consumer,
physical NIC support, Wi-Fi association, firmware handling, power management,
interrupt handling, MSI/MSI-X enablement, bus mastering, DMA, reset,
multiqueue, offloads, queues, packet movement, sockets, protocols, multiple
consumers, zero-copy leases, or persistent network state.

The PythOS architectural names remain `VirtioTransport`, transport adapter,
and `NetworkPort`. This diagnostic is a privileged PCI configuration probe; it
does not introduce `Driver` as a PythOS architectural abstraction.

Any capability-specific control, device initialization, power transition,
interrupt setup, DMA, or packet operation requires a later ADR and a separate
acceptance gate.
