# Phase 15 Network PCI BAR Layout — Lenovo Evidence

**Date:** 2026-09-24
**Target:** Lenovo `81VS` physical target
**Profile:** `network-hardware-bar-probe`
**Source commit:** `1c7c4b6` (`test: add QEMU network BAR oracle`)
**ISO:** `F:\iso\pythos-phase15-network-hardware-bar-probe-1c7c4b6.iso`
**ISO size:** `20,633,600` bytes
**ISO SHA-256:** `2AFBE42D6F37A010C1B09B82A40DD56B36C8735255DE423F3235BEF5C3674041`

## Physical observation

The owner booted the new ISO on the Lenovo and supplied the committed frame
`2026-09-24-physical-network-bar-layout-lenovo.jpg`.

The framebuffer reports:

| Field | Observed value |
| --- | --- |
| configuration access | `config read only` |
| BDF | `02:00:00` |
| vendor/device | `10EC:C82F` |
| class/subclass/prog-if | `02:80:00` |
| BAR0 raw low / high | `00001001` / `NONE` |
| BAR0 decoded | `IO`, base `0x0000000000001000` |
| BAR1 | unimplemented (`00000000`) |
| BAR2 raw low / high | `E8A00004` / `00000000` |
| BAR2 decoded | `MEM64`, base `0x00000000E8A00000` |
| BAR3 | omitted because it is BAR2's consumed 64-bit high half |
| BAR4 / BAR5 | unimplemented (`00000000`) |

The image demonstrates the bounded configuration observation and the expected
64-bit BAR pairing rule. It does not demonstrate that either reported address
is mapped or reachable.

## Local QEMU gate

Before the physical boot, the reviewed branch passed the bounded QEMU oracle:

- the pure decoder and focused contract suite passed;
- live `e1000` and `e1000e` runs each found exactly one expected controller;
- each run used `-nic none` plus one explicit PCI device and ended with one
  `QEMU_OUTCOME success`;
- the oracle rejected malformed layouts, out-of-range slots, forbidden
  operations, and extra network backends.

## Claim boundary

This is target-specific PCI configuration evidence only. The probe did not
write PCI configuration, size a BAR by writing all ones, enable memory or I/O
space, enable bus mastering, map or dereference a BAR, read a device register,
configure queues, allocate DMA, enable interrupts, load firmware, reset the
controller, access Ethernet or Wi-Fi datapaths, or claim controller ownership.

The observation therefore does not claim BAR reachability, MMIO operation,
Ethernet support, Wi-Fi support, firmware behavior, DMA, interrupts, queue
operation, or generalized physical-hardware support.

## Evidence hash

The committed photo is `1,932,014` bytes with SHA-256
`2B661985F7C35294E9CF09D7087C7C77852683A66A4D07E614B9A27F6CFC070C`.

The exact source image and photo are retained separately from the earlier
Phase 15 identity-probe evidence. The existing Ventoy media was preserved;
only the newly named ISO above was added to `F:\iso`.

See [ADR 0106](../decisions/0106-phase-15-network-hardware-bar-layout-probe.md)
for the decision and exclusions.
