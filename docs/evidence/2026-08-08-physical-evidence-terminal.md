# Milestone 1 Physical Evidence Terminal Validation

Date: 2026-08-08

Status: target-specific physical validation recorded on the disposable O2 Micro
`1217:8620` laptop. The evidence-terminal implementation and QEMU acceptance
harness are present on `main`; COM1 remains the automated QEMU oracle.

## Physical Capture Set

The 2026-08-08 physical capture contains five readable terminal-page photos and
one continuous real-hardware boot video.

| Terminal page | Operator source file | SHA-256 |
| --- | --- | --- |
| 01/05 | `29821.jpg` | `AAEA43DFD8318CB1C7BA7F29FB04050DF3F1242CAFAB3FE2E12B866CF4F7A3B6` |
| 02/05 | `29822.jpg` | `250BBE4625B11B94DC065B53F07A5AEBE5D9AE6EAA334C737C3C79ADB9E800B4` |
| 03/05 | `29823.jpg` | `E42AFAA8DB47278FB05FE8605F2A3E1C644D36708D9BC8C9C806BBAD492E1F3C` |
| 04/05 | `29824.jpg` | `97C8836735ECFB45E432D9EF2A756C891002267AC3293DBBFD0FE5B71B981DF9` |
| 05/05 | `29825.jpg` | `80E9ED8445D698B4B09597876DB0B96F1E61810B37A2F24B81132503903C68A5` |
| continuous boot video | `29820.mp4` | `01AA80BC02F1225D5B9E543A24B4EB96A8584BE15646EE258970C13BABE85979` |

The raw MP4 is intentionally kept out of normal git history. The milestone
release is the intended distribution point for the original-resolution physical
capture files.

## Terminal Header

Every photographed page reports the same status header:

```text
count 00000139
drop 00000000
crc 176F4C6E
```

The terminal formatter uses decimal only for the two-digit page number fields.
It uses eight-digit hexadecimal formatting for `count`, `drop`, and `crc`.
Therefore `count 00000139` means `0x139`, or **313 decimal markers**. It does
not mean 139 markers.

The implementation is in `core/src/evidence_terminal.rs`. Its
`format_status_line` function calls `write_dec2` for the page fields and
`write_hex8` for count, drop, and CRC. The unit test
`status_line_formats_count_drop_and_crc_as_hex` fixes that contract explicitly.

## Physical Stream Reconstruction

The physical marker stream was reconstructed from the current QEMU stream plus
the hardware-path differences visible on the O2 Micro target. Recomputing the
evidence-log CRC over that modeled ordered stream closes exactly on the physical
terminal header:

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

The absence of `GENERAL_STORAGE:CREATED` is consistent with the physical
persistent-storage snapshot sector already being populated rather than a fresh
QEMU image taking the create path.

The photographed line previously suspected to be
`PYTHOS:CORE:PHASE_2_COMPLETE` is `PYTHOS:CORE:PHASE_7_COMPLETE`. There is no
unexplained Phase 2 marker in this physical transcript.

## Repeatability

Two separate physical boots produced the same terminal header: hexadecimal
count `00000139` (313 decimal), zero drops, and CRC `176F4C6E`. Because the CRC
covers the concatenated accepted marker text including order, the repeated
header is strong evidence that the ordered stream was reproduced
deterministically across the two boots. CRC-32 is not collision-proof, so this
is stated as strong reproducibility evidence rather than mathematical proof of
byte-for-byte identity by CRC alone.

## Repository Acceptance

`main` contains the opt-in `evidence-terminal` feature and
`scripts/test-evidence-terminal.py`. The QEMU harness builds the terminal path,
requires ordered milestone markers, rejects panic/fallback/dropped-transcript
conditions, and validates terminal glyph structure in the framebuffer
screendump.

Run:

```powershell
python scripts\test-evidence-terminal.py
```

The physical transcript is not required to be textually identical to the QEMU
transcript because hardware discovery, audio fallback, and persistent-storage
state can select different truthful marker branches. The evidence contract is
that each reported stream is internally complete, ordered, zero-drop, and
consistent with the executed hardware path.

## Claim Boundary

This evidence validates the Milestone 1 evidence-terminal path on one disposable
O2 Micro `1217:8620` target. It does not claim generic SDHCI/eMMC support,
broad PC compatibility, interrupt-driven or DMA-backed storage, physical
keyboard/trackpad shell interaction, or production completeness of every
subsystem named by an acceptance marker.
