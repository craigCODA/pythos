# ADR 0054: Polling AHCI Block Backend

Status: Accepted

## Context

The `object-shell` branch now depends on reboot-durable storage for the ring-3
object shell, but its block backend was still QEMU legacy `virtio-blk`. That is
fine for the existing verification path and remains the default harness device,
but it does not help with common real SATA hardware. AHCI is the tractable next
storage backend because it is PCI-discoverable, documented, and QEMU-emulated.

QEMU's q35 boot ESP is already attached through a SATA path, so the test
configuration must attach a separate, explicit AHCI controller for PythOS's
non-boot object-store image. Otherwise a naive AHCI scan could select the boot
ESP and corrupt the firmware/loader image.

NVMe, MSI/MSI-X, Local APIC/IOAPIC, partition discovery, filesystems, DMA
isolation, and broad real-hardware storage support are separate later work.

## Decision

1. **Keep virtio-blk as the default backend and add AHCI as fallback.**
   `BlockDeviceInfo` is now backend-tagged (`Virtio` or `Ahci`). The legacy
   `PYTHOS:CORE:BLOCK:DEVICE_SELECTED` marker is preserved, with backend
   markers added as `PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO` or
   `PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI`.
2. **Discover AHCI by PCI class code and BAR5 ABAR.** PythCore scans the
   primary PCI bus for class `0x01`, subclass `0x06`, programming interface
   `0x01`, validates that BAR5 is a memory BAR, and exposes
   `PYTHOS:CORE:BLOCK:AHCI_CONTROLLER_FOUND` when a controller is present.
3. **Map AHCI MMIO before the PythCore VM switch.** AHCI BAR5 is discovered
   before `KernelAddressSpace::build` in both verify and normal boot, then
   mapped into a fixed uncacheable kernel virtual window. The existing HDA-style
   pre-switch MMIO pattern is reused.
4. **Use a bounded polling AHCI command path.** The first AHCI driver supports
   one ready plain-SATA port, one command slot, 1 KiB command-list alignment,
   256-byte received-FIS alignment, 128-byte command-table alignment, and
   single-sector ATA `READ DMA EXT` / `WRITE DMA EXT` requests. It enables AHCI
   mode, programs `PxCLB`/`PxFB`, starts FIS receive and command processing,
   submits via `PxCI`, and polls task-file/command-completion registers.
5. **Expose only the current object-store proof window.** AHCI does not expose
   disk capacity in controller registers. This first slice deliberately reports
   the same bounded 16 MiB storage window used by the existing QEMU storage
   proofs. Full ATA IDENTIFY capacity discovery and partition policy are later
   work.
6. **Add a QEMU AHCI acceptance path.** `scripts/run-qemu.py` now supports
   `--ahci`, `--ahci-storage-image`, and `--no-virtio-blk`. The AHCI test uses
   an explicit `ich9-ahci` controller at PCI slot `0x5` so PythCore finds the
   non-boot storage image before q35's default boot SATA controller.

## Consequences

- Default QEMU boots still select virtio-blk and preserve existing Phase 7-10
  storage behavior.
- `scripts/test-ahci-block-device.py` boots with virtio disabled, selects AHCI,
  proves the existing object-store/general-storage sectors persist through a
  second AHCI-backed boot, and requires `PYTHOS:CORE:MILESTONE_1_COMPLETE`.
- This is a real polling AHCI backend, but it is still QEMU-verified. Physical
  SATA hardware remains a follow-up validation target, not a claim of universal
  hardware support.
- No interrupt-driven storage path exists yet. MSI/MSI-X, Local APIC/IOAPIC,
  multi-bus PCI enumeration, NVMe, filesystems, partition discovery, hotplug,
  DMA remapping/IOMMU isolation, package management, networking, updates, SMP,
  and AI remain out of scope.

## Alternatives Considered

- **Leave storage virtio-only.** Rejected because it keeps the object-shell
  branch tied to QEMU-only storage and leaves no AHCI oracle for later physical
  SATA validation.
- **Implement NVMe first.** Deferred. NVMe is the more common modern storage
  target, but it is a larger queue/admin-command design than the bounded AHCI
  polling path needed for the current storage proofs.
- **Implement interrupt-driven AHCI now.** Deferred. The current kernel still
  uses legacy PIC-era boot proof machinery; MSI/MSI-X and APIC work would widen
  the slice beyond block-backend selection.
- **Discover full disk capacity now.** Deferred. ATA IDENTIFY and partition
  policy are useful later, but the current object-store proof uses fixed low
  sectors inside a 16 MiB raw image.
