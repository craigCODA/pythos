# Physical SDHCI/eMMC Phase 10 Evidence

Date: 2026-08-01

Status: target-specific physical acceptance recorded. A first-run photo and a
second-run video both show the final SDHCI/eMMC Phase 10 backend panel on the
confirmed disposable O2 Micro `1217:8620` target.

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

## Physical Artifacts

The raw photo and video are intentionally not committed to git. The video is
143,974,709 bytes, which is too large for a normal GitHub-tracked file. The raw
artifacts should be published as release assets or external evidence artifacts
if this milestone is promoted publicly.

- First-run local artifact: `D:\Downloads\20260801_171744.jpg`
- First-run raw SHA-256:
  `9886EDD5D79A1BE50A887C38EB3CB9A90896D619D7B341AB098FFEB48D904122`
- Second-run local artifact: `D:\Downloads\20260801_171753.mp4`
- Second-run duration: 15.89 seconds
- Second-run resolution: 3840 by 2160
- Second-run frame rate: 59.27 fps
- Second-run raw SHA-256:
  `DC178998ECFE6F3349A29930C083A61545817421963EB8D265DC96D0604C900E`

Committed first-run screen-only frame:
[2026-08-01-physical-sdhci-emmc-backend-boot1.jpg](../evidence/2026-08-01-physical-sdhci-emmc-backend-boot1.jpg)

First-run frame SHA-256:
`0E804EC38610BFF3AD982737EBF02E432DBC3B0858A573B4E1753D9611A55732`

Committed second-run screen-only frame:
[2026-08-01-physical-sdhci-emmc-backend-boot2.jpg](../evidence/2026-08-01-physical-sdhci-emmc-backend-boot2.jpg)

Second-run frame SHA-256:
`7B8D115E4D7D1E3FC9DF1077F2A4F69DCB28372CF03E157FF6EBF6510609EC25`

## Observed Physical Screen

The final panel visible in both physical artifacts says:

```text
PythOS
sdhci emmc backend
phase10 ok
disk writes
capacity 000000000747C000
```

This is evidence that the opt-in polling SDHCI/eMMC backend reached the final
Phase 10 verify-storage acceptance panel across two physical runs on the
disposable O2 Micro `1217:8620` target.

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

- generic SDHCI, SD-card, or eMMC compatibility;
- interrupt-driven storage;
- DMA, ADMA, SDMA, or multi-block I/O;
- partitions or filesystems;
- safe writes on any machine other than the confirmed disposable target;
- interactive physical object-shell use through built-in keyboard or trackpad.

## Completed Gate

The planned two-cold-boot physical Phase 10 backend panel gate is recorded for
the one disposable O2 Micro `1217:8620` target. The next work is either
publication/merge work for this branch or a separate, explicitly scoped
physical-input or broader-hardware milestone.

## Implementation Method

This work was implemented with agent assistance under human architectural
direction. The trusted record is the repository history, ADRs, QEMU acceptance
commands, serial markers, host-image checks, and physical evidence artifacts;
chat summaries are not authoritative.
