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
- Public merge commit on `main`:
  `0a59547100eace8eca0ac5d08153ef88f213b272`
- Milestone release:
  [PythOS Milestone 1: Physical Persistent Object Storage](https://github.com/craigCODA/pythos/releases/tag/milestone-1-physical-storage)
- Public site:
  [PythOS - Persistent Object Operating System for x86-64](https://craigcoda.github.io/pythos/)
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

## Evidence Terminal Follow-up

On 2026-08-02, `main` gained five evidence-terminal gallery frames showing the
captured acceptance-marker stream across five framebuffer pages:

```text
PythOS Evidence Terminal
page 01/05 count 00000139 drop 00000000 crc 176F4C6E
...
page 05/05 count 00000139 drop 00000000 crc 176F4C6E
```

The five committed screen frames are:

- [Evidence terminal page 1/5](../evidence/2026-08-02-evidence-terminal-page-1.jpg)
- [Evidence terminal page 2/5](../evidence/2026-08-02-evidence-terminal-page-2.jpg)
- [Evidence terminal page 3/5](../evidence/2026-08-02-evidence-terminal-page-3.jpg)
- [Evidence terminal page 4/5](../evidence/2026-08-02-evidence-terminal-page-4.jpg)
- [Evidence terminal page 5/5](../evidence/2026-08-02-evidence-terminal-page-5.jpg)

The terminal frames show the captured path through loader handoff, kernel
initialization, capability gates, scheduler and IPC checks, object persistence,
crash recovery, adversarial storage checks, SDHCI/eMMC acceptance, framebuffer
readiness, and `PYTHOS:CORE:MILESTONE_1_COMPLETE`.

`main` cannot currently regenerate or automatically verify these frames. It
does not contain the evidence-log sources, the `evidence-terminal` Cargo
feature, or `scripts/test-evidence-terminal.py`; that implementation remains
on unmerged branch `agent/physical-evidence-terminal`. The branch reported QEMU
acceptance at implementation commit `5e73e73` while treating physical
validation as the next step. These frames are retained as physical artifact
evidence for the captured milestone path, not as reproducible acceptance from
`main`.

These markers are not a claim that every named subsystem is production-complete,
portable, interrupt-driven, or broadly supported across hardware. ADR 0063
records the evidence-terminal design and its scope boundary.

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

## Evidence Terminal Follow-Up

ADR 0063 adds a later opt-in framebuffer evidence terminal for serial-less
physical capture. It is not part of the two 2026-08-01 physical artifacts above:
those artifacts show the older five-line SDHCI/eMMC backend panel only.

QEMU acceptance for the evidence terminal was recorded on branch
`agent/physical-evidence-terminal` at implementation commit `5e73e73` before
documentation sync:

```powershell
python scripts\test-evidence-terminal.py
```

Successful output included:

```text
PYTHOS:CORE:EVIDENCE_TERMINAL_READY
QEMU_OUTCOME success
EVIDENCE_TERMINAL_TEST_OK
```

That acceptance boot built `pythos-boot` with `evidence-terminal`, built
`pythos-core` with `verify,sdhci-emmc-backend,evidence-terminal`, booted QEMU
with `--no-virtio-blk --sdhci --emmc`, rejected panic/fallback/dropped
transcript markers, and required the PPM framebuffer dump at
`target\evidence-terminal.ppm` to match the evidence-terminal frame palette.

Manual screendump inspection showed terminal page `05/05` with count
`00000138`, dropped count `00000000`, CRC `734FF002`, and final markers through
`PYTHOS:CORE:MILESTONE_1_COMPLETE`.

Physical evidence-terminal validation is still pending. A later O2 Micro
`1217:8620` boot photo or video is required before claiming the full terminal
transcript has been observed on real hardware.

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
