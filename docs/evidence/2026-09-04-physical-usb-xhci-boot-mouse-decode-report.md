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

## Visible Results

The first retained physical screenshot shows the motion result:

```text
btn 00 l0 r0 m0
dx -007 dy -007
aux 00
```

```text
C:\Users\NeverAMoment\Desktop\Screenshot 2026-09-04 112914.png
SHA-256 5E31E4BF9E5E6571E2A11BB5B86023B3475EC8135747CB339246B262C1A8B4AE
```

The screen's lowercase `l0` left-button field can resemble `10` in the fixed
boot font. The source renders the literal field as `l0`.

The second retained screenshot shows a separate dock-topology left-button
state with no movement:

```text
btn 01 l1 r0 m0
dx 000 dy 000
aux 00

C:\Users\NeverAMoment\Desktop\Screenshot 2026-09-04 142212.png
SHA-256 6990E21E42BB2CA3B347DA9B8E81A60C3572E21408737A20AEF328E211149D31
```

A later one-shot boot showed a neutral state after the operator released a
button that had already been held before attachment. That observation does not
prove that PythOS observed a release transition: no preceding pressed report
was accepted in that boot, so there is no accepted pressed state in the same
sequence against which to compare the neutral sample.

Both named screenshot files were rehashed during the ADR 0088 checkpoint and
matched the recorded SHA-256 values. There is no COM1 serial capture from this
physical machine.

## Last Proven Layer

Physical USB boot-mouse reports reached the ADR 0087 semantic decoder and
rendered signed movement and a left-button state in separate one-shot Lenovo
boots. They do not form a recurring sequence.

## Interpretation

What is proven:

- The QEMU-accepted ADR 0087 kernel was deployed with hash readback.
- The Lenovo reached the successful decoded-report panel.
- Button bits decoded clear as `00` / left 0 / right 0 / middle 0 in the
  motion screenshot.
- Both movement bytes decoded as signed values of `-7`.
- The optional fourth byte was retained as raw `aux 00`.
- A separate dock-topology screenshot decoded `btn 01 l1 r0 m0` with zero
  movement.
- The boundary stopped after one report without cursor integration.

What is not proven:

- A physical COM1 serial marker sequence.
- A second or recurring interrupt transfer.
- A PythOS-observed release transition: the later neutral observation had no
  preceding accepted pressed report in the same boot.
- Click semantics, wheel semantics, normal launcher input, or cursor motion.
- Generic support beyond this Lenovo/controller/mouse combination.

Next slice:

- Physically validate the separately approved ADR 0088 recurring-report image
  before any cursor integration.
