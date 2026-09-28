# 2026-09-24 Phase 15 Lenovo Network-Identity Evidence

Status: target-specific physical observation recorded

Related decision: [ADR 0105](../decisions/0105-phase-15-network-hardware-identity-probe.md)

Related image: [Lenovo framebuffer capture](2026-09-24-physical-network-identity-lenovo.jpg)

## Test artifact

The probe image was built from merged Phase 15 PR #30 at main commit
`b89b406bbcab3bd2a48e0ef27935dc3656ad71d2` and copied as a new file to:

```text
F:\iso\pythos-phase15-network-hardware.iso
```

ISO size: `20,619,264` bytes

ISO SHA-256:

```text
3F7E5E83A3F23CB74E21A5E29E557124B548899A53E1194B09AF13C966B05CCC
```

The existing Ventoy installation and existing ISO files were preserved. No
existing file under `F:\iso` was overwritten or removed.

## Physical observation

The dedicated `network-hardware-probe` image booted on the Lenovo `81VS` and
reached its terminal framebuffer panel. The panel reported:

| Field | Observed value |
| --- | --- |
| Network-controller count | `1` |
| BDF | `02:00.0` |
| Vendor/device | `10EC:C82F` |
| Subsystem vendor/device | `17AA:C02F` |
| PCI class/subclass/programming interface | `02/80/00` |
| Probe mode | `config read only` |

These values exactly match the recorded Lenovo identity fixture in the probe's
unit tests. The class code is network (`0x02`) with subclass `0x80`, so the
probe's deliberate classification is `OtherNetwork`; it does not label the
device as Wi-Fi. The raw PCI identity is the evidence for later target-specific
work.

The supplied photograph was copied into this evidence directory as
`2026-09-24-physical-network-identity-lenovo.jpg`.

Photograph SHA-256:

```text
9D692BD05CE5AFD1FFEBC35998FEC27DDAEA331F9E72495A658598C030122C90
```

## Claim boundary

This evidence proves that PythOS can boot the dedicated profile on this target,
scan the bounded PCI configuration identity, and render the result. It does
not prove Wi-Fi association, firmware loading, BAR or MMIO reachability, DMA,
bus mastering, interrupts, reset, power management, controller ownership,
Ethernet or Wi-Fi frame movement, a network service, or generalized physical
hardware support.

The QEMU serial acceptance remains the automated implementation oracle. This
photo is target-specific physical evidence that complements that QEMU proof;
it is not a replacement for the serial marker contract.
