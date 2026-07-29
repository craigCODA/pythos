# eMMC/SDHCI Hardware Probe Design

## Goal

Build a boot image for the eMMC target PC that can answer one question safely:
does PythCore see an SDHCI/eMMC-class storage controller, or some other storage
controller, without initializing or touching the storage device?

## Scope

In scope:

- A `hardware-probe` PythCore build feature.
- Read-only PCI configuration-space scanning.
- Bounded classification of storage-related PCI functions.
- Serial markers with controller type and PCI identity fields.
- Solid framebuffer colors for real hardware that lacks COM1.
- A QEMU acceptance script proving the probe path runs and the normal storage
  and shell paths do not start.

Out of scope:

- SDHCI/eMMC command execution.
- AHCI physical testing on the current PC.
- NVMe queues.
- Controller MMIO access.
- PCI configuration writes.
- BAR sizing writes.
- DMA setup.
- Disk reads or writes.
- Filesystems, partitions, package management, networking, AI, SMP, or general
  hardware abstraction work.

## Probe Contract

The probe runs after:

```text
PYTHOS:CORE:ENTER
PYTHOS:CORE:BOOTINFO_VALID
PYTHOS:CORE:MEMORY_READY
PYTHOS:CORE:GDT_READY
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY
```

The probe then emits:

```text
PYTHOS:CORE:HARDWARE_PROBE:ENTER
PYTHOS:CORE:HARDWARE_PROBE:PCI_SCAN_READY
PYTHOS:CORE:HARDWARE_PROBE:STORAGE_COUNT=<hex>
PYTHOS:CORE:HARDWARE_PROBE:STORAGE_CONTROLLER_FOUND
PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:<kind>
PYTHOS:CORE:HARDWARE_PROBE:BUS=<hex>
PYTHOS:CORE:HARDWARE_PROBE:DEVICE=<hex>
PYTHOS:CORE:HARDWARE_PROBE:FUNCTION=<hex>
PYTHOS:CORE:HARDWARE_PROBE:VENDOR=<hex>
PYTHOS:CORE:HARDWARE_PROBE:DEVICE_ID=<hex>
PYTHOS:CORE:HARDWARE_PROBE:CLASS=<hex>
PYTHOS:CORE:HARDWARE_PROBE:SUBCLASS=<hex>
PYTHOS:CORE:HARDWARE_PROBE:PROG_IF=<hex>
PYTHOS:CORE:HARDWARE_PROBE:BAR0=<hex>
PYTHOS:CORE:HARDWARE_PROBE:BAR5=<hex>
PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES
PYTHOS:CORE:HARDWARE_PROBE_READY
```

If no storage controller is found, it emits:

```text
PYTHOS:CORE:HARDWARE_PROBE:NO_STORAGE_CONTROLLER
```

If an SDHCI/eMMC candidate is found, it emits:

```text
PYTHOS:CORE:HARDWARE_PROBE:STORAGE:SDHCI_EMMC_CANDIDATE
```

## Screen Contract

Framebuffer colors are coarse, hardware-visible diagnostics:

- violet: the probe boot path started
- green: at least one SDHCI/eMMC candidate was found
- blue: storage was found, but no SDHCI/eMMC candidate was found
- red: no storage controller was found

These colors are not a complete oracle. They are for serial-less real hardware
triage. The automated oracle remains COM1 serial in QEMU.

## Acceptance

`scripts/test-hardware-probe.py` builds the loader and a `hardware-probe`
PythCore image, packages the existing ESP tree, boots QEMU, waits for
`PYTHOS:CORE:HARDWARE_PROBE_READY`, and asserts:

- the probe markers appear in order
- a QEMU legacy virtio storage controller is discovered by the read-only probe
- `PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES` is emitted
- the normal block-device, object-store, and shell-entry markers are absent

The target PC boot remains user-driven. The accepted evidence from that machine
is the final screen color and any serial text the user can capture.
