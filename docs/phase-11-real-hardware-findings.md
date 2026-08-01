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
  preserving `NO_DISK_WRITES`. A 2026-07-31 follow-up no-serial framebuffer
  boot of commit `4230c7c` showed `cmd 11`, `norm 8000`, and `eint 0010`.
  `CMD17` was the first read-data command; `norm 0x8000` is the SDHCI error
  interrupt bit, and `eint 0x0010` is the SDHCI data-timeout bit. The next
  diagnostic build programs SDHCI `TIMEOUT_CONTROL` to `0x0E` before CMD17 to
  test whether the O2 Micro controller's reset-default data timeout is too
  short, still without issuing media write commands. A 2026-07-31 no-serial
  framebuffer boot of commit `979190f` on the same physical O2 Micro
  `1217:8620` SDHCI/eMMC laptop reached `emmc read`, preserved
  `no disk writes`, and showed:
  - `OCR` = `0xC0FF8080`
  - `LBA0` = `0x00000000`
  - first dword = `0x00000000`
  - checksum = `0x000006F9`
  - nonzero bytes = `0x0000000C`
  This proves the physical read-only PIO CMD17 data path for LBA 0 on this
  controller after programming `TIMEOUT_CONTROL=0x0E`. It still does not prove
  media writes, partition parsing, filesystem support, generic block-device
  integration, interrupts, DMA/ADMA, or universal SDHCI/eMMC support.
  ADR 0061 adds a separate destructive `hardware-probe-emmc-write` feature for
  disposable hardware. It keeps the default `hardware-probe` image read-only,
  writes exactly one deterministic 512-byte PIO block to command address
  `2048`, polls `CMD13` ready-for-data, reads the same address back with
  `CMD17`, and compares the pattern. On the physical target OCR `0xC0FF8080`
  means high-capacity/block addressing, so command address `2048` is physical
  LBA `2048`. A 2026-07-31 QEMU acceptance boot of commit `3edeffd` emitted
  `PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_READBACK_MATCH_READY` and verified the
  disposable raw image bytes at the OCR-derived host offset. A 2026-07-31
  no-serial framebuffer boot of the same commit on the disposable O2 Micro
  `1217:8620` SDHCI/eMMC laptop showed:
  - `emmc write`
  - `disk writes`
  - `LBA` = `0x00000800`
  - first dword = `0x48545950`
  - write checksum = `0x0000FBD8`
  - readback checksum = `0x0000FBD8`
  - match = `0x01`
  This proves only that this disposable physical eMMC target accepted the
  bounded single-sector PIO `CMD24` write/readback sequence at LBA `2048`.
  It does not prove safe repeated writes, generic eMMC block-device
  integration, partition parsing, filesystem support, object-store persistence
  on eMMC, interrupts, DMA/ADMA, or universal SDHCI support.
- Operator confirmation on 2026-08-01: this exact O2 Micro `1217:8620`
  SDHCI/eMMC laptop is a disposable storage target and has been treated as
  disposable throughout Phase 11 bring-up. Future agents do not need to re-ask
  whether this exact target is disposable before deploying or running the
  already-authorized ADR 0061 `hardware-probe-emmc-write` image against fixed
  command address/LBA `2048`. This confirmation does not authorize broader
  storage writes, other LBAs, repeated-write validation, normal object-store
  persistence on eMMC, DMA/ADMA, interrupt-driven storage, universal SDHCI/eMMC
  support, or writes on any other machine/controller.
- ADR 0062 implementation starts from commit `dcebc2e` and promotes the proven
  single-block PIO path into an opt-in `sdhci-emmc-backend` block backend. The
  physical target remains the same already-confirmed disposable O2 Micro
  `1217:8620` SDHCI/eMMC laptop. Physical backend validation is forbidden until
  QEMU storage acceptance and QEMU object-shell persistence acceptance pass
  against a disposable emulated eMMC image without selecting virtio or AHCI.
- The SDHCI/eMMC backend now has a verify-only no-serial acceptance panel that
  renders only after the Phase 7-10 storage proof path reaches
  `PYTHOS:CORE:PHASE_10_COMPLETE`. The panel text is:
  `PythOS / sdhci emmc backend / phase10 ok / disk writes / capacity <hex>`.
  QEMU acceptance requires
  `PYTHOS:CORE:BLOCK:SDHCI_EMMC_FRAMEBUFFER_ACCEPTANCE_READY` plus the existing
  serial and host-image storage oracles. Physical two-cold-boot evidence on the
  disposable O2 Micro `1217:8620` target is still pending and must not be
  inferred from this QEMU result.
- **Networking (Phase 14):** no NIC driver yet. The laptop's Wi-Fi is a hard
  target; virtio-net (QEMU) / wired Ethernet is the tractable path when
  networking work begins.
- General principle: prefer hardware that QEMU can emulate so the serial/capture
  oracle still applies; treat oracle-less real-hardware-only drivers as scoped
  investigations with explicit uncertainty.
