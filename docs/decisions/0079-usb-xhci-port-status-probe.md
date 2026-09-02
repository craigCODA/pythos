# ADR 0079: USB xHCI Port Status Probe

Date: 2026-08-31
Status: Accepted

## Context

ADR 0078 proved that the current physical target can boot a no-write PythOS USB
diagnostic, discover AMD xHCI `1022:7914` at `00:10.0`, map BAR0 into
PythCore-owned page tables, and read the xHCI capability and operational header
registers with volatile MMIO reads.

The next USB mouse question is still below enumeration: can PythOS read the
xHCI extended-capability pointer, observe any legacy ownership semaphore, and
read the port-status register set that will later tell us which root port sees
the external Dell/PixArt USB mouse.

## Decision

Add `usb-xhci-port-probe` as a separate opt-in feature depending on
`usb-xhci-probe`. The existing `usb-xhci-probe` feature and ADR 0078 acceptance
boundary remain unchanged. When the port probe is enabled, PythCore still uses
the existing PCI/xHCI discovery and BAR0 mapping path, then additionally:

1. decodes max ports from `HCSPARAMS1`;
2. decodes the xHCI extended-capability pointer from `HCCPARAMS1`;
3. scans the bounded extended-capability chain for USB Legacy Support;
4. reports BIOS-owned and OS-owned semaphore bits if the legacy capability is
   present;
5. reads up to eight xHCI `PORTSC` and `PORTPMSC` register pairs;
6. renders a fixed framebuffer panel headed `xhci ports`;
7. emits `PYTHOS:CORE:USB_XHCI_PROBE:XHCI_PORT_STATUS_READY` and
   `PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES`.

This diagnostic does not reset xHCI, claim BIOS/OS ownership, allocate command
or event rings, write doorbells, enable interrupts, enumerate USB devices, poll
USB endpoints, parse HID descriptors or reports, move a cursor, or touch
storage.

`scripts/test-usb-xhci-port-probe.py` is the QEMU acceptance harness. It builds
the PythCore image with `--features usb-xhci-port-probe`, attaches
`qemu-xhci`, attaches a QEMU USB mouse to that xHCI controller, disables
ordinary block devices, and requires ordered serial markers for the inherited
header-register probe plus max-port, xECP, legacy-capability, port-count,
port-one `PORTSC`/`PORTPMSC`, framebuffer, no-write, and success markers.

The accepted QEMU run reported:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:HCSPARAMS1=0x0000000008001040
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:HCCPARAMS1=0x0000000000087001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:USBSTS=0x0000000000000009
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:MAX_PORTS=0x0000000000000008
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_REGISTER_BASE=0x0000000000000440
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_SNAPSHOT_LIMIT=0x0000000000000008
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:EXT_CAP_DWORD_OFFSET=0x0000000000000008
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:EXT_CAP_BYTE_OFFSET=0x0000000000000020
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:LEGACY_CAP_ABSENT
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:COUNT=0x0000000000000008
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:NUMBER=0x0000000000000005
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:PORTSC=0x0000000000000E03
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:PORTPMSC=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_PORT_STATUS_READY
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
PYTHOS:CORE:USB_XHCI_PROBE_READY
```

In that QEMU configuration, the attached USB mouse was visible as a changed
root-port status value. This proves only the emulated port-register observation
path.

After QEMU acceptance on 2026-08-31, the candidate was written to the verified
`P:` USB ESP target without formatting or a delete pass. The target was
re-identified as Disk 2, Partition 1, Lexar D70E USB, serial
`1026R51254700477`, MBR, active FAT32 `PYTHOS_ESP`, not Windows boot/system.
Read-only `chkdsk P:` reported no filesystem problems. Source-to-target
readback reported `USB_XHCI_PORT_VERIFY_OK files:8 bytes:3840440`; the
deployed `P:\PYTHOS\PYTHCORE.ELF` SHA-256 was
`447D1F9CA8D97F8000F0905566628AE3B959C212E4E2B33C558220E436D94320`. Existing
root files such as `LINUX-USB-MOUSE-MAP.SH` and the Linux mouse-map archive
were preserved.

## Consequences

This creates the next independently testable USB mouse layer without starting
the destructive or stateful parts of xHCI. A physical boot photograph of the
`xhci ports` panel can now answer whether the real AMD xHCI controller exposes
the same port register layer and whether any root port reports the attached
external USB mouse.

The remaining USB mouse work is still: controlled xHCI ownership/initialization,
DMA-safe command and event rings, device enumeration, endpoint setup, HID
descriptor/report parsing, pointer-event integration, and physical cursor
movement proof.
