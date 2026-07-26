# Real-Hardware USB Boot Design

## Goal

Make the milestone-1 PythOS boot path start and produce observable progress when booted from a USB stick on a real UEFI PC, not only inside QEMU/OVMF. The concrete success target is: on real hardware, the physical screen shows deterministic loader progress instead of an uninformative black screen, and the loader reads `PYTHCORE.ELF` from the same device it was booted from.

## Background: Observed Failure

The user booted a physically prepared USB SSD (Lexar D70E, GPT, with a valid 1 GB EFI System Partition containing the correct PythOS boot tree) on a real PC.

Observed, in order:

1. Firmware listed the USB as a selectable UEFI boot device.
2. With Secure Boot enabled, the boot was rejected (unsigned `BOOTX64.EFI`).
3. With Secure Boot and legacy/CSM disabled, selecting the USB produced a permanent black screen.

QEMU/OVMF boots the identical image to `PYTHOS:CORE:MILESTONE_1_COMPLETE`. The divergence is real-hardware-specific.

### What was ruled out

- **Missing/incorrect boot files.** The USB ESP already contained a byte-identical PythOS tree (`BOOTX64.EFI` 71680, `PYTHCORE.ELF` 3443480, `INIT.PAK` 33198, `BOOT.CFG`, `FONT.PSF`). Re-deploying the same bytes cannot change the result.
- **Missing partition table / El Torito-only container.** That failure mode applies to the hand-rolled `target/pythos.iso`, which has no MBR/GPT and only boots as optical/CD media. It does **not** apply here: the USB is a proper GPT disk with a real ESP, and firmware did enumerate and offer it.
- **Secure Boot.** Confirmed as a gating factor (unsigned loader) and correctly worked around by disabling it. Out of scope to fix now; signing is a later concern.
- **TCG Opal / drive-security BIOS setting.** Pertains to the internal self-encrypting drive, not USB boot. Not a cause. Leave unchanged.

### Root-cause hypotheses (post Secure-Boot)

The black screen is uninformative **by construction**: every milestone-1 diagnostic (`PYTHOS:LOADER:*`, `PYTHOS:CORE:*`, `FAIL`, `PANIC`) is emitted over **COM1 serial only**, and nothing is drawn to the framebuffer until late in a *successful* boot. A real laptop exposes no serial port, so any early loader/core fault is visually indistinguishable from a hang. PythOS has only ever been validated inside QEMU, where the serial log is observable.

Two concrete, independently plausible real-hardware faults:

1. **Wrong-device filesystem discovery — RULED OUT (2026-07-24).** Initially the leading hypothesis, but on inspection the loader *already* resolves its filesystem from its own boot device: `boot/src/uefi.rs::open_boot_volume` does `HandleProtocol(image_handle, LoadedImage) -> DeviceHandle -> HandleProtocol(DeviceHandle, SimpleFileSystem) -> OpenVolume`, and `elf.rs`/`initrd.rs`/`font.rs` all use it (added in commit `611c8ca`, before the tested USB was built). `LocateProtocol` survives only for GOP. So the loader does not read `PYTHCORE.ELF` from the internal disk, and this is not the black-screen cause. This shifts weight to hypothesis 2 and other real-hardware divergence, and makes the Task 1 paints the primary means of localizing where the boot actually dies.
2. **GOP mode rejection.** The loader prefers 1024×768 and rejects blit-only modes (`boot/src/graphics.rs`). If real firmware offers no mode it accepts, GOP init fails early and silently.

## Scope

Loader-only changes plus their QEMU serial-marker tests. This does **not** change the boot ABI, PythCore behavior beyond what the entry contract already guarantees, runtime/audio/video features, Python services, AI, or Secure Boot signing. It does not add general multi-disk storage support; it narrows discovery to the boot device.

## Design

### Change 1: Visible loader diagnostics (turn the black screen into a signal)

After GOP init succeeds, the loader paints the mapped framebuffer a solid, deterministic color, and repaints a distinct color at each subsequent milestone (`KERNEL_LOADED`, `MEMORY_MAP_READY`, immediately before `ExitBootServices`). This uses only the already-validated direct framebuffer and the existing `PythFramebufferInfo` masks/pitch — no UEFI services, no new ABI.

Result on real hardware:
- Screen stays black → loader never ran, or GOP init failed before first paint.
- Screen turns the first color and stops → loader started and GOP works; failure is after GOP (e.g. kernel-file discovery).
- Screen advances through colors → bisects exactly how far the loader reached with no serial cable.

The failure handler also paints a distinct color so a fault is visible rather than silent.

### Change 2: Boot-device filesystem discovery

Replace `LocateProtocol(SimpleFileSystem)` with the firmware-sanctioned path:
`LoadedImageProtocol(image_handle) -> DeviceHandle -> OpenProtocol(SimpleFileSystem)` on that same device handle.

This guarantees `PYTHCORE.ELF` and `INIT.PAK` are read from the device the loader itself was loaded from — the USB — regardless of how many other filesystems the firmware exposes. Requires adding `LoadedImageProtocol` and `HandleProtocol`/`OpenProtocol` bindings to `boot/src/uefi.rs`. Keep the change bounded; do not add general device enumeration.

## Acceptance

- `python scripts/test-boot.py --slice milestone-1` and `--media iso` still reach `PYTHOS:CORE:MILESTONE_1_COMPLETE` over serial (no regression from the discovery change).
- QEMU visual check: the framebuffer shows the loader progress colors in order before PythCore takes over.
- Real-hardware check (user-driven, not automatable here): booting the F: USB shows loader progress colors instead of a black screen. Honestly recorded as the only evidence real firmware behaves — QEMU success is necessary but not sufficient.

## Honest Limitations

- QEMU cannot prove a specific PC's firmware is satisfied; real UEFI implementations vary. Final confirmation is a boot on the actual machine, read from the physical screen.
- This does not sign the loader; Secure Boot must remain disabled until a signing story exists.
- This is the first slice of real-hardware bring-up (roadmap Phase 11 / "milestone 1.5 substrate"), not its completion. Further real-hardware differences (ACPI, timers, interrupt controller, varied GOP) may surface once the loader is observably reaching PythCore.
