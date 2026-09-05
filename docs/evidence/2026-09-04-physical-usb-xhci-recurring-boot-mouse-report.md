# Physical USB xHCI Recurring Boot-Mouse Report

**Date:** 2026-09-04

**Machine:** Lenovo `81VS`

**Firmware Settings:** No change reported for this run

**Branch/Commit:** `agent/hw-white-screen-diagnostic` at
`e168c49aeb1eb6fe745c845e2399396e0a658b53`

**Boot Media:** Disk 2, Lexar D70E serial `1026R51254700477`, active FAT32
`PYTHOS_ESP`; deployed `BOOTX64.EFI` SHA-256
`085A02AA250050CB55B065B7842B09CDE5C087291ABD19D83FA05F6197918578`
and `PYTHCORE.ELF` SHA-256
`717390EBD77EE896C188830E60AA2D60E29107469295D855519094203CED46BC`

**Writes Performed:** The running probe reported `no disk writes`. Before the
boot, the separately authorized Windows deployment replaced only eight managed
boot-image files (`4,154,184` bytes), verified every source-to-target hash, and
proved all 108 unrelated USB files retained identical paths and hashes.

## Controller Inventory

| BDF | Vendor | Device | Class | Subclass | Prog IF | BARs | Interpretation |
|---|---|---|---|---|---|---|---|
| `00:10.0` | `1022` | `7914` | `0C` | `03` | `30` | Previously captured by ADR 0078 | AMD xHCI controller on the Lenovo path |

The successful panel identifies port 6, slot 1, endpoint `0x81`, and DCI 3.
The connected device is the previously established Dell/PixArt `413c:301a`
boot mouse with a four-byte maximum interrupt packet.

## Visible Results

```text
PythOS
xhci mouse sequence
no disk writes
bdf 00 10 00
vid did 1022 7914
port 06 slot 01
ep 81 dci 03
reports 16 wrap 1
last 00 l0 r0 m0
seen 01 rel 01
sumx -0061 sumy -0082
aux 00 present 1
frozen no cursor
```

The retained evidence image is:

```text
docs/evidence/2026-09-04-physical-usb-xhci-recurring-boot-mouse-success.png
2744x1235, 3,537,774 bytes
SHA-256 B5802BE845386BDFE37815A7477CF684C8ABFE00115C7ED2F13681821EA47598
```

The original operator upload was
`C:\Users\NeverAMoment\Desktop\Screenshot 2026-09-04 215143.png`; its hash
matched the repository evidence copy. There is no COM1 capture from this
physical run.

## Last Proven Layer

The Lenovo completed the bounded sixteen-report USB boot-mouse sequence,
crossed the interrupt transfer-ring Link TRB once, aggregated physical movement,
observed a left-button press followed by release, and rendered the successful
frozen no-write panel.

## Interpretation

What is proven:

- The QEMU-accepted image was deployed to the freshly identified Lexar with
  source-to-target hash readback before this run.
- The Lenovo configured the physical mouse on port 6, slot 1, endpoint `0x81`,
  DCI 3 and accepted exactly sixteen recurring reports.
- `reports 16 wrap 1` proves one bounded transfer-ring wrap on this target.
- `seen 01 rel 01` proves that bit 0 was observed pressed and then released in
  the same PythOS sequence; `last 00 l0 r0 m0` proves the final sample was
  neutral.
- Signed movement accumulated to X `-61` and Y `-82`.
- The optional fourth byte was present and its last raw value was zero.
- The boundary remained `frozen no cursor` and reported `no disk writes`.

What is not proven:

- Generic USB HID or xHCI support beyond this Lenovo/controller/mouse
  combination.
- A physical COM1 marker transcript or the exact physical event-ring wrap
  count.
- Cursor movement, click actions, wheel semantics, normal input-event routing,
  IRQ-driven input, hub support, hot-unplug recovery, or a second transfer-ring
  wrap.
- Any storage write by PythOS.

Next slice:

- Stop at this accepted recurring-report boundary. Cursor/input integration is
  a separate design and implementation boundary requiring explicit owner
  invocation.
