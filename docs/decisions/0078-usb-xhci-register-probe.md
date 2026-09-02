# ADR 0078: USB xHCI Register Probe

Date: 2026-08-30
Status: Accepted

## Context

The normal SDHCI/eMMC diagnostic candidate reached `stage 37` / `ring3 enter`
on the current physical target, but the visible `Enter Shell` tile did not move
with the trackpad. That is not a boot failure and not proof of working pointer
input: the tile is retained framebuffer content, and the existing `ps2.rs`
mouse path is legacy PS/2/IRQ12 support for QEMU-style input.

Linux reconnaissance from the same machine identified the built-in trackpad as
I2C HID (`ELAN0666:00 04F3:304B`) and the external Dell/PixArt USB mouse as
USB HID VID:PID `413c:301a` behind AMD xHCI controller `1022:7914` at
`0000:00:10.0`, class `0c:03:30`, BAR0 `0x00000000E8C68000`. The Linux
collector script `scripts/linux-usb-mouse-map.sh` is discovery-only; it does
not implement PythOS USB support.

## Decision

Add an opt-in `usb-xhci-probe` Cargo feature for a no-write USB host-controller
diagnostic image. The feature is mutually exclusive with `verify` and
`hardware-probe`. When enabled, PythCore:

1. scans PCI/PCIe buses for USB controllers;
2. classifies UHCI, OHCI, EHCI, xHCI, and other USB class devices;
3. selects the first xHCI controller;
4. decodes BAR0 as a 32-bit or 64-bit memory BAR;
5. maps the selected BAR0 page into the PythCore-owned kernel address space at
   `0xFFFFC00010040000` with supervisor-only, no-execute, cache-disabled flags;
6. reads the xHCI capability header and operational-register header fields with
   volatile MMIO reads;
7. renders a fixed framebuffer identity panel and emits `NO_DISK_WRITES`.

The diagnostic does not reset xHCI, take BIOS/OS ownership, allocate command or
event rings, enable interrupts, enumerate USB devices, poll endpoints, parse HID
reports, or move a cursor.

`scripts/test-usb-xhci-probe.py` is the QEMU acceptance harness. It builds the
probe image with `--features usb-xhci-probe`, disables ordinary block devices,
attaches `qemu-xhci`, and requires ordered serial markers for PCI scan,
xHCI classification, BAR0 discovery, explicit MMIO mapping, xHCI header
registers, framebuffer identity, `NO_DISK_WRITES`, and
`PYTHOS:CORE:USB_XHCI_PROBE_READY`. It rejects normal block-device, shell,
hardware-probe, and register-error markers.

The QEMU run proved the default high xHCI BAR path directly:

```text
PYTHOS:CORE:USB_XHCI_PROBE:BAR0=0x000000C000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:MMIO_VIRT=0xFFFFC00010040000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_MMIO_MAPPED
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:CAPLENGTH=0x0000000000000040
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:HCIVERSION=0x0000000000000100
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:HCSPARAMS1=0x0000000008001040
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:DBOFF=0x0000000000002000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:RTSOFF=0x0000000000001000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI:USBSTS=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
PYTHOS:CORE:USB_XHCI_PROBE_READY
```

During bring-up, QEMU showed that a separate 16-bit MMIO read at xHCI offset
`0x02` returned zero even though the first 32-bit capability DWORD contained
`0x01000040`. The accepted probe therefore decodes CAPLENGTH and HCIVERSION
from the same 32-bit volatile read before continuing.

After QEMU acceptance on 2026-08-30, the candidate was written to the verified
`P:` USB ESP target without formatting. Source-to-target readback reported
`USB_XHCI_PROBE_VERIFY_OK files:8 bytes:3814456`; the deployed
`P:\PYTHOS\PYTHCORE.ELF` SHA-256 was
`479588E4268C65E6F03EECAEF0534D7D5F4ADEEF9EBA0B1DD50D3549BF67D0AA`.

On 2026-08-31, the deployed no-write probe booted on the physical target. The
operator-provided framebuffer photo
`docs/evidence/2026-08-31-physical-usb-xhci-register-probe.png` shows:

```text
PythOS
xhci regs
no disk writes
count 0000000000000002
bdf 00 10 00
vid did 1022 7914
class sub if 0C 03 30
bar0 00000000E8C68000
caplen 20
hciver 0100
hcs1 08000820
hcc1 014040C3
sts 00000009
```

The source image path was
`C:\Users\NeverAMoment\Desktop\Screenshot 2026-08-31 181247.png`; the copied
evidence file SHA-256 is
`EF950D8A3B9804635C99BDF04C49026A87912F79E2FDC13A393FAA21CF0481C8`.

## Consequences

Default builds are unchanged. The probe creates the first PythOS xHCI
controller/register evidence layer and fixes the earlier high-BAR assumption by
mapping device MMIO explicitly instead of relying on the loader identity map.
The physical target now has photo-backed no-write xHCI controller/register
reachability evidence for AMD `1022:7914`.

This does not prove physical USB mouse movement, generic USB HID support,
trackpad support, USB device enumeration, endpoint polling, HID report parsing,
interrupt delivery, or DMA ring correctness.
