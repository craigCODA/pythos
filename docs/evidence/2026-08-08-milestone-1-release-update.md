# Milestone 1 Release Evidence Update — 2026-08-08

Use this text to update the existing `milestone-1-physical-storage` release. This
is a correction and evidence expansion for the existing Milestone 1 release,
not a new milestone or tag.

## Corrected Evidence Terminal Result

The physical PythOS Evidence Terminal was captured on the disposable O2 Micro
`1217:8620` target across five readable pages and a continuous boot video.
Every photographed page reports:

```text
count 00000139
drop 00000000
crc 176F4C6E
```

`count` is displayed as eight-digit hexadecimal. Therefore
`00000139` = `0x139` = **313 decimal markers**. Earlier release/site wording
that described this artifact as 139 markers was incorrect.

Two separate physical boots produced the same 313-marker count, zero dropped
markers, and CRC `176F4C6E`. The physical stream was reconstructed from the
current QEMU stream plus the observed hardware-path differences and independently
recomputes to:

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

The absent `GENERAL_STORAGE:CREATED` marker is consistent with the physical
persistent-storage snapshot sector already being populated. The line previously
suspected through OCR to be `PYTHOS:CORE:PHASE_2_COMPLETE` is visibly
`PYTHOS:CORE:PHASE_7_COMPLETE`.

CRC-32 is not collision-proof, so the repeated CRC is described as strong
reproducibility evidence rather than mathematical proof of byte-for-byte
identity from CRC alone.

## 2026-08-08 Assets

Attach the original files to the existing Milestone 1 release:

| File | Purpose | SHA-256 |
| --- | --- | --- |
| `29821.jpg` | Evidence Terminal page 01/05 | `AAEA43DFD8318CB1C7BA7F29FB04050DF3F1242CAFAB3FE2E12B866CF4F7A3B6` |
| `29822.jpg` | Evidence Terminal page 02/05 | `250BBE4625B11B94DC065B53F07A5AEBE5D9AE6EAA334C737C3C79ADB9E800B4` |
| `29823.jpg` | Evidence Terminal page 03/05 | `E42AFAA8DB47278FB05FE8605F2A3E1C644D36708D9BC8C9C806BBAD492E1F3C` |
| `29824.jpg` | Evidence Terminal page 04/05 | `97C8836735ECFB45E432D9EF2A756C891002267AC3293DBBFD0FE5B71B981DF9` |
| `29825.jpg` | Evidence Terminal page 05/05 | `80E9ED8445D698B4B09597876DB0B96F1E61810B37A2F24B81132503903C68A5` |
| `29820.mp4` | Continuous physical boot / terminal capture | `01AA80BC02F1225D5B9E543A24B4EB96A8584BE15646EE258970C13BABE85979` |

## Repository Status

The evidence-terminal implementation is on `main`, including the Cargo feature,
`core/src/evidence_terminal.rs`, and `scripts/test-evidence-terminal.py`.
Statements that the generator or QEMU harness are absent from `main` are stale
and should not appear in the release description.

COM1 remains the automated QEMU oracle. The physical terminal is the visual
mirror used to make the accepted marker stream inspectable on serial-less
hardware. The physical and QEMU streams can legitimately differ where actual
hardware discovery, audio fallback, and persistent-storage state select
different truthful marker branches.
