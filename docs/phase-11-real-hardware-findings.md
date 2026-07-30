# Phase 11 Real-Hardware Findings

Phase 11 ("Physical Hardware Boot Smoke Test") precondition artifact for later
hardware work (Phase 15 driver expansion reads this first). Records what booting
PythOS on real UEFI machines actually revealed, versus QEMU-only assumptions.

## Confirmed

- **PythOS boots on real UEFI hardware.** An AMD laptop boots a prepared USB all
  the way to the milestone-1 cinematic wake screen (`PythOS [HISS] We Are
  Woken`), including the native serpent cinematic. QEMU is no longer the only
  environment.
- **GOP framebuffer works on real hardware** (AMD laptop), including the direct
  pixel-format rendering the cinematic uses.

## The handoff bug real hardware exposed (fixed — ADR 0046)

- The loader's temporary page tables originally identity-mapped only physical
  `2 MiB..4 GiB`. Real firmware may load `BOOTX64.EFI` — and place the
  `AllocatePages` allocations PythCore reads (`PythBootInfo`, memory map,
  `INIT.PAK`) — **above 4 GiB** on machines with more RAM. The instruction after
  `mov cr3` then triple-faults. QEMU and the laptop happened to stay below 4 GiB.
- Fix: identity-map the low **512 GiB** with 1 GiB huge pages (2 MiB pages for
  the low-1 GiB null guard; 2 MiB fallback to 4 GiB when the CPU lacks 1 GiB
  pages). Merged to `main`.
- One desktop still went **magenta** (loader pre-handoff color) before the fix
  was verified on it. Status: the fix is now on `main` but **not yet re-tested on
  that desktop**.

## Debugging without serial (durable lesson)

- Real laptops expose **no COM/serial port**, so the milestone serial oracle is
  invisible. Every early fault looks like a black screen.
- Technique that worked: **paint the raw GOP framebuffer a distinct solid color
  at each boot milestone** (loader and kernel), so the on-screen color is the
  oracle. A format-independent white "liveness" fill as the kernel's first
  instruction distinguishes "handoff triple-faulted" from "kernel running,
  framebuffer mapping wrong." One physical boot's final color pinpoints the
  failing stage.
- Implication for all future real-hardware bring-up: build **on-screen**
  diagnostics; do not rely on serial.

## Constraints observed

- **Secure Boot must be disabled** (the loader is unsigned). Signing is a future
  need before Secure Boot can stay on.
- Diagnostic framebuffer paints currently fire on every boot; gating them to
  failure-only is a deferred cleanup.

## Hardware gaps that drive driver priorities (Phase 15 / Phase 14)

- **Audio:** AC97 (Phase 6) is QEMU-only. The AMD laptop has no AC97; it uses
  Intel HDA and/or I2S codecs behind the AMD ACP. Laptop audio is silent.
  - **Intel HDA** is the tractable next step: QEMU-emulated and WAV-verifiable,
    likely drives the laptop headphone jack. Being built now (ADR 0048).
  - **AMD ACP / I2S** (laptop speakers) is parked: not emulated in our QEMU
    harness, machine-specific, historically very hard. Investigation only, and
    only after HDA works.
- **Storage:** ADR 0054 adds a QEMU-verified polling AHCI backend, selected only
  when legacy virtio-blk is absent. Physical AHCI testing on the development PC
  remains off limits because its AHCI-visible drive contains live data.
  Probe-only hardware builds on the blank laptop identified an SDHCI/eMMC-class
  PCI function at BDF `01:00.0`, vendor/device `1217:8620`,
  class/subclass/programming-interface `08/05/01`, BAR0
  `0x00000000E3B01000`, BAR5 `0x0000000000000000`. ADR 0057 adds only a
  read-only BAR0 register snapshot for this controller. A 2026-07-30
  no-serial framebuffer boot reached `sdhci regs`, preserved `no disk writes`,
  and read:
  - `PRESENT_STATE` (`BAR0+0x24`) = `0x01FF00F0`
  - `CAPABILITIES_LOW` (`BAR0+0x40`) = `0x25FCC8BF`
  - `CAPABILITIES_HIGH` (`BAR0+0x44`) = `0x00002077`
  - `MAX_CURRENT_CAPABILITIES` (`BAR0+0x48`) = `0x005800C8`
  - `SLOT_INTERRUPT_STATUS/HOST_CONTROLLER_VERSION` (`BAR0+0xFC`) =
    `0x06030000`
  This proves physical BAR0 register visibility on that laptop, not media
  access.
  A follow-up 2026-07-30 no-serial framebuffer boot of commit `124911f`
  reached `sdhci init`, preserved `no disk writes`, and read:
  - selected controller count = `0x0000000000000002`
  - BDF = `01:00.0`
  - vendor/device = `1217:8620`
  - class/subclass/programming-interface = `08/05/01`
  - BAR0 = `0x00000000E8B01000`
  - reset control = `0x00`
  - clock control = `0x0003`
  - power control = `0x0F`
  - present state = `0x01FF0000`
  - interrupt status = `0x00000000`
  This proves the physical SDHCI controller accepted the bounded reset,
  internal-clock, and bus-power initialization sequence from ADR 0058. It does
  not prove eMMC media support.
  ADR 0059 adds a bounded eMMC identification probe after initialization. A
  2026-07-30 QEMU acceptance boot with `sdhci-pci` plus an attached `emmc`
  device emitted OCR/RCA/CID/CSD markers and
  `PYTHOS:CORE:HARDWARE_PROBE:EMMC_IDENTIFICATION_READY`, while preserving
  `PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES`. A follow-up 2026-07-30
  no-serial framebuffer boot of commit `e808bda` reached the physical eMMC
  identification screen on the same SDHCI/eMMC laptop and showed:
  - `OCR` = `0xC0FF8080`
  - `CID0` = `0x00D35C77`
  - `CID1` = `0x34471800`
  - `CSD0` = `0xEF8A4040`
  This proves the physical eMMC device answered the bounded identification
  command sequence after SDHCI initialization, still with no block data path and
  no disk writes. Partition discovery, filesystems, interrupt-driven storage,
  block reads, block writes, and DMA isolation remain later work.
  ADR 0060 adds one bounded read-only PIO block transfer after identification:
  select RCA `1`, set block length `512`, read LBA `0` with `CMD17`, and report
  only first dword/checksum/nonzero byte count. A 2026-07-30 QEMU acceptance
  boot with a disposable patterned eMMC image emitted:
  - `PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:FIRST_DWORD=0x0000000003020100`
  - `PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:CHECKSUM=0x000000000000FF00`
  - `PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:NONZERO_BYTES=0x00000000000001FE`
  - `PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ONLY_BLOCK_READY`
  - `PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES`
  This proves the QEMU SDHCI/eMMC PIO read path for one sector. It does not yet
  prove the physical O2 Micro `1217:8620` eMMC data path. A 2026-07-30
  no-serial framebuffer boot of commit `c418320` reached the physical eMMC
  read path after OCR `0xC0FF8080` and showed `emmc read err` with
  `err 00000003`. That means a command-path failure during the read-only
  sequence, not a media write. The `c418320` screen did not identify whether
  `CMD7`, `CMD16`, or `CMD17` failed, so the follow-up diagnostic build adds
  no-serial `cmd`, `norm`, and `eint` fields for command-path failures while
  preserving `NO_DISK_WRITES`.
- **Networking (Phase 14):** no NIC driver yet. The laptop's Wi-Fi is a hard
  target; virtio-net (QEMU) / wired Ethernet is the tractable path when
  networking work begins.
- General principle: prefer hardware that QEMU can emulate so the serial/capture
  oracle still applies; treat oracle-less real-hardware-only drivers as scoped
  investigations with explicit uncertainty.
