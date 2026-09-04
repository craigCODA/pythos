# Physical USB xHCI Boot-Mouse Decode Report

**Date:** 2026-09-04

**Machine:** Lenovo `81VS`

**Firmware Settings:** No change reported for this run

**Branch/Commit:** `agent/hw-white-screen-diagnostic` at `b631d5d`

**Boot Media:** Disk 2, Lexar D70E serial `1026R51254700477`, active FAT32
`PYTHOS_ESP`; deployed `PYTHCORE.ELF` SHA-256
`FBE7D2C7FE8B21EB0A5A68B491B4F239A650F75293A0A73685A7309C919EC31B`

**Writes Performed:** The probe reported no disk writes. Before boot, the
separately authorized Windows deployment replaced eight managed boot-image
files and verified that 111 unrelated USB files remained byte-identical.

## Controller Inventory

| BDF | Vendor | Device | Class | Subclass | Prog IF | BARs | Interpretation |
|---|---|---|---|---|---|---|---|
| `00:10.0` | `1022` | `7914` | `0C` | `03` | `30` | Previously captured by ADR 0078 | AMD xHCI controller on the Lenovo path |

The configured USB device was the previously established Dell/PixArt
`413c:301a` boot mouse with interrupt-IN endpoint `0x81`, DCI 3, and maximum
packet size 4.

## Visible Result

The user directly transcribed these final framebuffer lines:

```text
btn 00 l0 r0 m0
dx -007 dy -007
aux 00
```

The screen's lowercase `l0` left-button field was initially transcribed as
`10`, which is visually plausible in the fixed boot font. The source renders
the literal field as `l0`.

The phone lost power before a photograph or video could be taken. There is no
serial capture from this physical machine. The evidence is therefore direct
user observation plus exact transcription, not media-backed observation.

## Last Proven Layer

One physically received USB boot-mouse report reached the ADR 0087 semantic
decoder and rendered clear button state, signed negative X/Y movement, and the
raw `aux` byte on the Lenovo.
## Interpretation

What is proven:

- The QEMU-accepted ADR 0087 kernel was deployed with hash readback.
- The Lenovo reached the successful decoded-report panel.
- Button bits decoded clear as `00` / left 0 / right 0 / middle 0.
- Both movement bytes decoded as signed values of `-7`.
- The optional fourth byte was retained as raw `aux 00`.
- The boundary stopped after one report without cursor integration.

What is not proven:

- Independent photographic or video confirmation of this run.
- A physical COM1 serial marker sequence.
- A second or recurring interrupt transfer.
- Button transitions, wheel semantics, normal launcher input, or cursor motion.
- Generic support beyond this Lenovo/controller/mouse combination.

Next slice:

- A separately approved recurring-report boundary before any cursor integration.
