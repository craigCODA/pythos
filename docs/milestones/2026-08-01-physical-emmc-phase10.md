# Physical SDHCI/eMMC Phase 10 Evidence

Date: 2026-08-01

Status: partial physical acceptance. One physical boot reached the final
SDHCI/eMMC Phase 10 backend panel. The two-cold-boot gate is still open until a
second cold boot is captured without reimaging the device.

## Repository State

- Branch at record time: `feature/sdhci-emmc-backend`
- Branch HEAD at record time:
  `85a0d3aa74aff0425684149eedcaca2033c18371`
- Binary-affecting physical acceptance implementation commit:
  `057fc37199f510d4e6da07f577143a430bb3eb81`
- ADR: [ADR 0062](../decisions/0062-polling-sdhci-emmc-block-backend.md)
- Hardware findings:
  [Phase 11 Real-Hardware Findings](../phase-11-real-hardware-findings.md)

## Hardware

- Controller: O2 Micro SDHCI/eMMC
- PCI identity: vendor/device `1217:8620`
- BDF observed during bring-up: `01:00.0`
- Class/subclass/programming interface: `08/05/01`
- Target status: confirmed disposable storage target for this SDHCI/eMMC work

## Architecture

```text
UEFI firmware
-> BOOTX64.EFI
-> PythCore verify path
-> recursive PCI SDHCI/eMMC discovery
-> uncacheable SDHCI BAR0 MMIO mapping
-> eMMC initialization and EXT_CSD capacity discovery
-> BlockDeviceInfo::SdhciEmmc
-> Phase 7-10 storage and object-store proof path
-> framebuffer acceptance panel
```

The physical panel is rendered only after the verify storage path reaches
`PYTHOS:CORE:PHASE_10_COMPLETE`. QEMU remains the automated oracle through
COM1 serial and host-image signature checks; the physical laptop currently has
no serial capture path.

## Video Artifact

The raw video is intentionally not committed to git because it is 143,974,709
bytes, which is too large for a normal GitHub-tracked file. It should be
published as a release asset or external video artifact if this milestone is
promoted publicly.

- Local artifact: `D:\Downloads\20260801_171753.mp4`
- Duration: 15.89 seconds
- Resolution: 3840 by 2160
- Frame rate: 59.27 fps
- SHA-256:
  `DC178998ECFE6F3349A29930C083A61545817421963EB8D265DC96D0604C900E`

Committed screen-only frame:
[2026-08-01-physical-sdhci-emmc-backend-boot1.jpg](../evidence/2026-08-01-physical-sdhci-emmc-backend-boot1.jpg)

Frame SHA-256:
`7B8D115E4D7D1E3FC9DF1077F2A4F69DCB28372CF03E157FF6EBF6510609EC25`

## Observed Physical Screen

The final panel visible in the video says:

```text
PythOS
sdhci emmc backend
phase10 ok
disk writes
capacity 000000000747C000
```

This is evidence that the opt-in polling SDHCI/eMMC backend reached the final
Phase 10 verify-storage acceptance panel on the disposable physical O2 Micro
`1217:8620` target once.

## QEMU Acceptance Backing This Image

Before the physical image was deployed, the branch passed:

```powershell
cargo fmt --check
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify,sdhci-emmc-backend -- -D warnings
cargo test -p pythos-core
cargo test --workspace --exclude pythos-boot --exclude pythos-core
python scripts\test-emmc-write-probe.py
python scripts\test-ahci-block-device.py
python scripts\test-sdhci-emmc-block-device.py
python scripts\test-sdhci-emmc-block-device.py
python scripts\test-persistent-storage.py
python scripts\test-normal-fast-boot.py
python scripts\test-com2-shell-transport.py
python scripts\test-object-shell.py
python scripts\test-object-shell.py --backend sdhci-emmc
python scripts\test-object-shell.py --backend sdhci-emmc
python scripts\test-boot.py --slice milestone-1
python scripts\test-boot.py --slice milestone-1 --media iso
```

The SDHCI/eMMC QEMU tests selected
`PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC`, rejected virtio and AHCI
selection markers, reached `PYTHOS:CORE:PHASE_10_COMPLETE` and
`PYTHOS:CORE:MILESTONE_1_COMPLETE`, and confirmed the expected object/general
storage signatures in the backing eMMC image.

## Not Claimed

This record does not claim:

- two-cold-boot physical acceptance yet;
- generic SDHCI, SD-card, or eMMC compatibility;
- interrupt-driven storage;
- DMA, ADMA, SDMA, or multi-block I/O;
- partitions or filesystems;
- safe writes on any machine other than the confirmed disposable target;
- interactive physical object-shell use through built-in keyboard or trackpad.

## Remaining Gate

Cold boot the same disposable target again without reimaging it and capture the
same final panel. After that second boot is recorded, update this page and
`docs/phase-11-real-hardware-findings.md` from partial physical acceptance to
two-cold-boot physical acceptance.

## Implementation Method

This work was implemented with agent assistance under human architectural
direction. The trusted record is the repository history, ADRs, QEMU acceptance
commands, serial markers, host-image checks, and physical evidence artifacts;
chat summaries are not authoritative.
