# ADR 0107: Phase 15 Network Hardware Register Reachability Probe

**Status:** Proposed for bounded implementation
**Date:** 2026-09-24
**Owners:** PythOS platform and transport-adapter maintainers

## Context

ADR 0105 established a read-only PCI identity observation and ADR 0106
recorded the selected controller's PCI BAR layout. Those slices prove that
PythOS can identify a network controller and decode the address reported in
PCI configuration space, but they deliberately do not prove that a device
register is mapped or readable.

The next question is intentionally smaller than a network driver: can one
fixed, model-specific status register be reached through an already-enabled
memory BAR, using a PythCore page-table mapping, without changing PCI
configuration or operating the controller?

The QEMU reference models expose the Intel `e1000`/`e1000e` device-status
register at offset `0x08`. The Linux Realtek `rtw88` PCI implementation maps
BAR 2 for register access, and its register definitions identify
`REG_SYS_STATUS1` at offset `0x00F4` for the Lenovo `10EC:C82F` controller.
These references identify the bounded observation locations; they do not
authorize copying Linux's device-enable, bus-master, interrupt, reset, DMA, or
packet paths into PythOS.

References:

- QEMU `hw/net/e1000.c`: <https://github.com/qemu/qemu/blob/master/hw/net/e1000.c>
- Linux `rtw88/pci.c`: <https://raw.githubusercontent.com/torvalds/linux/master/drivers/net/wireless/realtek/rtw88/pci.c>
- Linux `rtw88/reg.h`: <https://raw.githubusercontent.com/torvalds/linux/master/drivers/net/wireless/realtek/rtw88/reg.h>
- Linux `rtw8822ce.c` device IDs: <https://github.com/torvalds/linux/blob/master/drivers/net/wireless/realtek/rtw88/rtw8822ce.c>

## Decision

Add a separate opt-in `network-hardware-register-probe` profile. It reuses the
accepted PCI identity scan and BAR decoder, then performs exactly one bounded
32-bit MMIO read from the selected controller's fixed status location.

Before any MMIO mapping or dereference, the probe reads PCI configuration
offset `0x04` and requires the PCI Memory Space Enable bit (`bit 1`) to already
be set. If it is clear, the probe emits a skip result and halts. PythOS never
sets that bit. The probe also requires the selected target's expected memory
BAR slot to decode as a nonzero, 4 KiB-aligned memory BAR and rejects I/O BARs,
malformed pairs, overflow, and unsupported controller IDs.

The bounded target contract is:

| controller | BAR slot | register offset | observation |
| --- | ---: | ---: | --- |
| Intel `8086:100E` (`e1000`) | 0 | `0x08` | device status |
| Intel `8086:10D3` (`e1000e`) | 0 | `0x08` | device status |
| Realtek `10EC:C82F` (Lenovo target) | 2 | `0x00F4` | `REG_SYS_STATUS1` status snapshot |

The kernel maps only the first 4 KiB of the selected BAR into a dedicated
probe-only virtual window with no-execute and cache-disable page attributes.
The read is a single volatile 32-bit load. There are no writes to PCI
configuration or MMIO, no resource claiming, no device enablement, no bus
mastering, no reset, no firmware interaction, no interrupts, no DMA, no
queues, no offloads, and no packet movement or NetworkPort activity.

The privileged implementation remains the PythOS `VirtioTransport` / transport
adapter terminology where that architecture is discussed. This diagnostic is
not a generalized PCI or MMIO abstraction and does not rename any existing
implementation identifiers.

## Lifecycle and evidence contract

The probe's serial markers are ordered as follows:

```text
...:ENTER
...:PCI_SCAN_READY
...:NETWORK_CONTROLLER_FOUND
...:PCI_COMMAND_STATUS=...
...:PCI_MEMORY_SPACE_ENABLED
...:BAR_SLOT=...
...:BAR_TARGET_SELECTED
...:TARGET_INTEL_DEVICE_STATUS or ...:TARGET_REALTEK_SYS_STATUS1
...:REGISTER_OFFSET=...
...:MMIO_MAPPED
...:REGISTER_READ_VALUE=...
...:FRAMEBUFFER_REGISTER_READY
...:REGISTER_REACHABILITY_READY
...:PCI_CONFIG_READ_ONLY
..._READY
```

The safe skip path replaces the enable/mapping/read suffix with one of
`NETWORK_CONTROLLER_NOT_FOUND`, `UNSUPPORTED_CONTROLLER`,
`PCI_MEMORY_SPACE_DISABLED`, or `MMIO_TARGET_INVALID`. When a framebuffer is
available, the bounded result panel emits `FRAMEBUFFER_REGISTER_READY` before
the terminal `REGISTER_REACHABILITY_SKIPPED` marker. The path then emits
`PCI_CONFIG_READ_ONLY` and `READY`.

`REGISTER_REACHABILITY_READY` means only that the one fixed volatile read
completed without a fault under the explicit mapping and configuration gates.
It is not evidence of a working network link, firmware state, initialized
controller, network datapath, or Wi-Fi association.

## Acceptance

1. Host tests prove target selection, PCI memory-enable gating, memory-BAR
   validation, offset/window bounds, and rejection of I/O or malformed BARs.
2. Synthetic marker tests reject reordered, duplicated, malformed, or
   write/control evidence and accept the bounded read and skip transcripts.
3. QEMU runs with exactly one explicit `e1000` or `e1000e`, `-nic none`, and no
   network backend. The oracle requires the fixed `0x08` read, the framebuffer
   result marker, and forbids writes, DMA, interrupts, reset, queues, packets,
   and NetworkPort markers.
4. The physical Lenovo run uses a newly named ISO under `F:\iso` and preserves
   the existing Phase 15 ISO. It records either the fixed `0xF4` read or the
   specification-safe `PCI_MEMORY_SPACE_DISABLED` skip; neither result is
   interpreted as physical NIC/Wi-Fi support.
5. The default build and the already accepted identity/BAR profiles remain
   unchanged.

## Explicit non-goals and next boundary

This ADR does not add a network driver, `VirtioTransport` implementation,
NetworkPort consumer, physical NIC support, Wi-Fi association, Lenovo firmware
handling, modern Virtio PCI capability handling, interrupts, MSI/MSI-X,
multiqueue, offloads, DMA, packet movement, queues, sockets, protocols,
multiple consumers, zero-copy leases, persistent state, or generalized PCI/MMIO
types. Any controller-control or register-write work requires a later ADR and
acceptance gate.
