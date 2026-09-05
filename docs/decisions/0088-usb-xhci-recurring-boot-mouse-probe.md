# ADR 0088: USB xHCI Recurring Boot-Mouse Probe

Date: 2026-09-04

Status: Accepted in QEMU and on the Lenovo 81VS/Dell-PixArt target

## Context

ADR 0087 stops after one decoded USB boot-mouse report. That establishes one
bounded semantic sample, but it does not prove safe DMA-buffer reuse, ordered
press/release observation, transfer-ring wrap, or event-ring cycle-state wrap.
The next diagnostic boundary must prove those mechanics without becoming a
normal input driver or changing the boot ABI.

The interrupt transfer ring has sixteen entries. Entries 0 through 14 are
Normal TRBs and entry 15 is a Link TRB with Toggle Cycle set. Sixteen accepted
reports are therefore the smallest deterministic sequence that crosses the
Link TRB and reuses data entry 0 with the opposite cycle state.

## Decision

Add `usb-xhci-boot-mouse-recurring-probe` as an opt-in feature depending on the
ADR 0087 decode feature. It uses one `XhciInterruptTransferProbeSession` and a
strictly sequential single-in-flight loop:

1. verify that no transfer is already in flight;
2. prepare and publish one Normal TRB and one static 4 KiB report buffer;
3. ring the discovered interrupt-IN endpoint;
4. accept the matching Transfer Event through the shared event consumer;
5. copy and decode the received three- or four-byte prefix;
6. return DMA ownership to the CPU and only then advance the producer; and
7. repeat until exactly sixteen reports have been decoded.

The transfer producer owns `data_index`, `cycle`, and `wrap_count`. Reports 1
through 15 use data indices `0..14` with cycle state 1. Crossing the Link TRB
toggles the producer, and report 16 reuses data index 0 with cycle state 0.
The shared event consumer independently owns `event_index` and
`expected_cycle`. That cursor consumes setup command, control-transfer, and
recurring transfer events, toggles its expected cycle when entry 15 advances
to entry 0, and rejects stale cycle-state events. Because setup events share
the cursor, acceptance requires at least one event-consumer wrap; it does not
require exactly one. No seventeenth report is armed.

The terminal invariant requires all of the following: report count 16,
completed-transfer count 16, and transfer wrap count 1. The result aggregates
signed X/Y totals, latest button state, buttons observed pressed, adjacent
release-after-press transitions, auxiliary-byte presence/latest raw value,
and transfer/event wrap counts.

Failures remain typed as an xHCI driver error, boot-mouse decode error, or
terminal-invariant error. Any failure stops immediately, preserves the last
progress/summary for the framebuffer error panel, and does not emit recurring
or overall readiness. Overall readiness is gated on a successful recurring
result, absence of any recurring failure, and successful final-result render;
rendering an error panel successfully is not probe success.

This boundary does not move or draw a cursor, interpret the auxiliary byte as
a wheel, recognize clicks, route input into the normal input-event service or
compositor, use IRQ/MSI/MSI-X USB input, support hubs, recover hot unplug,
traverse the transfer ring a second time, or write storage.

## Serial and Framebuffer Contract

The sequence starts with:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_SEQUENCE_TARGET=0x0000000000000010
```

Each report then emits, in order, its ordinal, transfer TRB index/cycle,
request/armed markers, completion/actual/captured/raw evidence, transfer-ready,
decoded fields, decode-ready, and report-ready ordinal. Crossing the Link TRB
emits exactly one:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_RING_WRAP=0x0000000000000001
```

Every member of the repeated report marker group must occur exactly sixteen
times in the complete COM1 transcript. Extra occurrences outside the
ordinal-delimited groups, including before ordinal 1 or after the terminal
fields, fail acceptance. The sequence-target marker must occur exactly once
after endpoint setup and immediately before ordinal 1; only whitespace may
separate the complete target marker/value from the ordinal marker.

After report 16, the recorded QEMU run emitted this terminal order and these
values:

```text
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_REPORT_COUNT=0x0000000000000010
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DX_TOTAL_I32=0x0000000000000070
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DY_TOTAL_I32=0x00000000FFFFFFC8
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_BUTTONS_LAST=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_PRESSED_SEEN=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_RELEASED_AFTER_PRESSED=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_SEQUENCE_AUX_PRESENT=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_SEQUENCE_AUX_LAST=0x0000000000000000
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_WRAP_COUNT=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_EVENT_RING_WRAP_COUNT=0x0000000000000001
PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_RECURRING_READY
PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY
PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES
PYTHOS:CORE:USB_XHCI_PROBE_READY
```

The final framebuffer is static and includes `reports 16 wrap 1`, the last
button state, press/release masks, signed motion totals, raw auxiliary evidence,
and `frozen no cursor`.

## Verification

Fresh QEMU acceptance on the final implementation tree used:

```text
py -3 scripts\test-usb-xhci-boot-mouse-recurring-probe.py
```

The harness built only the recurring feature path, detached the simulated boot
USB, delayed mouse hotplug, and injected one deterministic QMP input action
after each new armed marker. It observed ordinals `1..16`, indices
`0..14,0`, cycles `1..1,0`, sixteen requested/armed/raw/transfer-ready/
decode-ready/report-ready groups and exactly one transfer wrap. The normative
event-consumer requirement is at least one wrap; this recorded run observed
exactly one. It also observed the terminal values above and exactly one
readiness/no-write terminal. It ended with:

```text
QEMU_OUTCOME success
USB_XHCI_BOOT_MOUSE_RECURRING_PROBE_TEST_OK
```

The final review fix wave first proved the missing contracts RED. The focused
Rust test failed to compile because the terminal-readiness gate did not exist.
The four focused Python oracle tests produced 34 expected failures: missing or
misordered target acceptance, repeated-marker extras before and after the
ordinal window, and terminal-invariant false acceptance. After the minimal
fix, the gate test passed, all four Python oracle tests passed, all 22 USB probe
screen tests passed, and all 15 marker-action/runner tests passed. The failure
cases cover late driver, decode, and terminal-invariant results and prove none
can satisfy the terminal-readiness policy or oracle.

A narrow follow-up regression then proved the oracle still accepted the target
between report ordinals 1 and 2, as well as a non-whitespace marker between the
target and ordinal 1. The corrected oracle now requires the first ordinal after
endpoint setup to be 1 and the target to be immediately adjacent apart from
whitespace. All four oracle self-tests and all eight marker-action tests passed,
and the complete recurring QEMU probe passed again on that exact oracle.

The same final tree retained QEMU acceptance for endpoint configuration, the
one-shot raw interrupt report, and the one-shot decoded report; normal boot
ended with `BOOT_TEST_OK` / `QEMU_OUTCOME success`; persistent storage ended
with `PERSISTENT_STORAGE_TEST_OK` and its required success outcomes; and all
698 PythCore host tests passed. The prescribed `py -3 -m pytest tests` could
not start because the installed Python 3.14 has no `pytest`; stdlib discovery
reproduced the same two failures and one error in 116 Phase 13-oriented tests
recorded before this fix wave.

Both static scanners passed their own self-tests. The repository scan retained
the recorded baseline totals of 219 scanner-unrecognized unsafe-comment sites
and four bounded-poll findings; this wave added no unsafe line and no scanner
finding.

The final recurring harness rebuilt `image/esp`. The repository does not
produce a `target/esp` directory; the freshly verified build outputs and COM1
log have these SHA-256 values:

```text
image/esp/EFI/BOOT/BOOTX64.EFI                         085A02AA250050CB55B065B7842B09CDE5C087291ABD19D83FA05F6197918578
image/esp/PYTHOS/PYTHCORE.ELF                          717390EBD77EE896C188830E60AA2D60E29107469295D855519094203CED46BC
target/usb-xhci-boot-mouse-recurring-probe-com1.log    C5F16ADBC17E266EFCEC2CDFF210D0E35A609E22091A865E82FE6F5E565AE1CA
```

The final COM1 log is 35,346 bytes. It contains the sequence target exactly
once before ordinal 1, exactly sixteen global occurrences of every repeated
group marker, and zero driver, decode, or terminal-invariant failure markers.

## Physical Verification

The exact QEMU-accepted eight-file image was deployed to the freshly identified
Lexar D70E boot USB at commit `e168c49aeb1eb6fe745c845e2399396e0a658b53`.
All source-to-target hashes matched and all 108 unrelated USB files retained
identical before/after hashes.

The Lenovo `81VS` then rendered the successful frozen recurring panel on AMD
xHCI `1022:7914`, port 6, slot 1, endpoint `0x81`, DCI 3. It reported `reports
16 wrap 1`, final buttons `00`, `seen 01 rel 01`, signed totals X `-61` and Y
`-82`, `aux 00 present 1`, `frozen no cursor`, and `no disk writes`. Therefore
the same physical sequence observed a left-button press and its later release;
the absence of a click action or visible cursor is the intended diagnostic
boundary, not a decode failure.

The retained photo is
`docs/evidence/2026-09-04-physical-usb-xhci-recurring-boot-mouse-success.png`,
SHA-256
`B5802BE845386BDFE37815A7477CF684C8ABFE00115C7ED2F13681821EA47598`.
The full target-specific evidence and limitations are recorded in
`docs/evidence/2026-09-04-physical-usb-xhci-recurring-boot-mouse-report.md`.
There is no physical COM1 transcript.

## Consequences

PythOS now has deterministic emulator proof plus target-specific physical proof
for bounded recurring boot-mouse transport, ordered decode/aggregation, a
transfer-ring cycle transition, signed movement, and a press/release sequence.
This does not establish generic USB HID support, physical COM1 marker ordering,
the physical event-ring wrap count, cursor or click behavior, wheel semantics,
normal input routing, IRQ-driven input, hub support, hot-unplug recovery, a
second transfer-ring wrap, or PythOS storage writes.
