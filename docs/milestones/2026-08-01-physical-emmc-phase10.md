# Physical SDHCI/eMMC Phase 10 Evidence

Original physical backend validation: 2026-08-01  
Physical evidence-terminal validation: 2026-08-08

Status: target-specific physical acceptance recorded on the confirmed disposable
O2 Micro `1217:8620` target. The original first-run photo and second-run video
show the Phase 10 SDHCI/eMMC backend panel. The later 2026-08-08 capture shows
the full five-page Milestone 1 evidence terminal on the same physical target.

## Repository State

- Original SDHCI/eMMC public merge commit:
  `0a59547100eace8eca0ac5d08153ef88f213b272`
- ADR 0062:
  [Polling SDHCI/eMMC Block Backend](../decisions/0062-polling-sdhci-emmc-block-backend.md)
- ADR 0063:
  [Physical Evidence Terminal](../decisions/0063-physical-evidence-terminal.md)
- Current physical terminal record:
  [2026-08-08 Physical Evidence Terminal Validation](../evidence/2026-08-08-physical-evidence-terminal.md)
- Milestone release:
  [PythOS Milestone 1: Physical Persistent Object Storage](https://github.com/craigCODA/pythos/releases/tag/milestone-1-physical-storage)
- Public site:
  [PythOS](https://craigcoda.github.io/pythos/)

`main` contains the evidence-log implementation, the `evidence-terminal` Cargo
feature, `core/src/evidence_terminal.rs`, and
`scripts/test-evidence-terminal.py`. The earlier statements that the generator
or harness were absent from `main` are superseded.

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
-> framebuffer acceptance / evidence terminal
```

QEMU remains the automated oracle through COM1 serial and host-image checks.
The physical laptop has no captured serial path, so the opt-in evidence terminal
mirrors the accepted marker stream to the framebuffer for inspectable physical
capture.

## 2026-08-01 Physical Backend Panel

The final panel visible in both original physical artifacts says:

```text
PythOS
sdhci emmc backend
phase10 ok
disk writes
capacity 000000000747C000
```

This is evidence that the polling SDHCI/eMMC backend reached the final Phase 10
verify-storage acceptance panel across two physical runs on the disposable O2
Micro `1217:8620` target.

Original artifact hashes:

- First-run raw photo SHA-256:
  `9886EDD5D79A1BE50A887C38EB3CB9A90896D619D7B341AB098FFEB48D904122`
- Second-run raw video SHA-256:
  `DC178998ECFE6F3349A29930C083A61545817421963EB8D265DC96D0604C900E`
- Committed first-run screen-only frame SHA-256:
  `0E804EC38610BFF3AD982737EBF02E432DBC3B0858A573B4E1753D9611A55732`
- Committed second-run screen-only frame SHA-256:
  `7B8D115E4D7D1E3FC9DF1077F2A4F69DCB28372CF03E157FF6EBF6510609EC25`

Committed frames:

- [First physical backend frame](../evidence/2026-08-01-physical-sdhci-emmc-backend-boot1.jpg)
- [Second physical backend frame](../evidence/2026-08-01-physical-sdhci-emmc-backend-boot2.jpg)

## 2026-08-08 Physical Evidence Terminal

Five physical photos capture pages 01/05 through 05/05. Every page reports:

```text
count 00000139
drop 00000000
crc 176F4C6E
```

The status formatter uses `write_dec2` for page numbers and `write_hex8` for
count, drop, and CRC. Therefore `00000139` is hexadecimal:

```text
0x139 = 313 decimal markers
```

Any earlier publication describing this terminal as “139 ordered markers” was
incorrect and is superseded by this record.

The photographed line previously suspected through OCR to be
`PYTHOS:CORE:PHASE_2_COMPLETE` is visibly
`PYTHOS:CORE:PHASE_7_COMPLETE`.

The physical stream reconstructed from the current QEMU stream plus the observed
hardware-path differences recomputes exactly to:

```text
physical_model_count=313
physical_model_crc=176F4C6E
```

The physical-vs-QEMU marker differences are:

```text
AUDIO:HDA:CONTROLLER_ABSENT -> AUDIO:HDA:CONTROLLER_FOUND
+ AUDIO:HDA:CONTROLLER_MAPPED
+ AUDIO:HDA:INIT_FAILED
AUDIO:DEVICE_SELECTED -> AUDIO:DEVICE_ABSENT
AUDIO:DRIVER -> AUDIO:DRIVER_SKIPPED
AUDIO:BUFFER -> AUDIO:BUFFER_SKIPPED
AUDIO:PCM_PLAYBACK -> AUDIO:PCM_SKIPPED
AUDIO:FALLBACK_ARMED -> AUDIO:FALLBACK
- GENERAL_STORAGE:CREATED
```

The missing `GENERAL_STORAGE:CREATED` marker is consistent with the physical
persistent-storage snapshot sector already being populated instead of a fresh
QEMU image taking the create path.

Two separate physical boots produced the same hexadecimal count, zero-drop
state, and CRC. Since CRC-32 can collide, that repetition is stated as strong
reproducibility evidence rather than mathematical proof of byte-for-byte
identity from CRC alone.

The original 2026-08-08 physical files and their SHA-256 hashes are recorded in
[the dedicated validation record](../evidence/2026-08-08-physical-evidence-terminal.md).

## QEMU Acceptance

The evidence-terminal implementation is reproducibly exercised from `main`:

```powershell
python scripts\test-evidence-terminal.py
```

The acceptance path builds the terminal feature, boots QEMU with the SDHCI/eMMC
backend selected, requires ordered milestone markers, rejects panic/fallback and
dropped-transcript conditions, and validates the evidence-terminal framebuffer
glyph structure.

The broader storage backing includes:

```powershell
cargo fmt --check
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify -- -D warnings
cargo test -p pythos-core
python scripts\test-sdhci-emmc-block-device.py
python scripts\test-persistent-storage.py
python scripts\test-object-shell.py --backend sdhci-emmc
python scripts\test-boot.py --slice milestone-1
python scripts\test-boot.py --slice milestone-1 --media iso
python scripts\test-evidence-terminal.py
```

The physical transcript is not required to be textually identical to the QEMU
transcript. Hardware discovery, audio fallback, and pre-existing persistent
storage can truthfully select different accepted marker branches. The evidence
contract is that the displayed physical stream is complete, ordered, zero-drop,
and consistent with the hardware path actually executed.

## Not Claimed

This record does not claim:

- generic SDHCI, SD-card, or eMMC compatibility;
- interrupt-driven storage;
- DMA, ADMA, SDMA, or multi-block I/O;
- partitions or filesystems;
- safe writes on any machine other than the confirmed disposable target;
- physical interactive object-shell use through built-in keyboard or trackpad;
- bit-identical physical and QEMU transcript content;
- collision-proof identity from CRC-32 alone;
- production completeness for every subsystem named by an acceptance marker.

## Completed Gate

The target-specific physical Phase 10 backend gate and the later physical
Milestone 1 evidence-terminal capture are both recorded for the disposable O2
Micro `1217:8620` target. Future physical-hardware claims require their own
explicit target and verification boundary.

## Implementation Method

This work was implemented with agent assistance under human architectural
direction. The trusted record is the repository history, ADRs, QEMU acceptance
commands, serial markers, host-image checks, and physical evidence artifacts;
chat summaries are not authoritative.
