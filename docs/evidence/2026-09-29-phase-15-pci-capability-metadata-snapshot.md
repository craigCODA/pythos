# Phase 15 PCI Capability Metadata Snapshot — QEMU and Lenovo Evidence

**ADR:** [0109 — Bounded PCI Capability Metadata Snapshot](../decisions/0109-phase-15-pci-capability-metadata-snapshot.md)

**ADR status:** Accepted

**Evidence scope:** QEMU and Lenovo `81VS` physical framebuffer observation

**Reviewed implementation commit:** `f413ebd3d723770ded9a2297a819ff6397d70ad2`

## Bounded implemented scope

The opt-in `network-hardware-capability-probe` records a bounded, read-only
conventional PCI capability metadata snapshot from the selected QEMU network
controller. It reports the raw capability pointer, list presence, bounded
entry metadata, recognized PM/MSI/PCIe/MSI-X capability IDs and offsets, and
PCI interrupt line/pin metadata. The oracle enforces the exact ID-to-kind and
fixed-header mapping. It does not interpret capability control fields or claim
controller or device readiness.

The probe preserves the existing `VirtioTransport`, transport adapter, and
`NetworkPort` boundaries. It includes no PCI writes, BAR mapping, MMIO or
device-register access, MSI/MSI-X enablement, power control, bus mastering,
DMA, interrupts, reset, queues, packets, sockets, Wi-Fi, or physical
networking.

## Reproduction commands

The complete build plus both-model oracle was run from the repository root:

```powershell
py -3 scripts/test-network-hardware-capability-probe.py
```

That oracle invoked these exact per-model runner argument vectors:

```powershell
& 'C:\Users\NeverAMoment\AppData\Local\Python\pythoncore-3.14-64\python.exe' scripts/run-qemu.py --serial-log 'D:\PythOS-Workspace\repo\pythos\.worktrees\phase15-network-hardware\target\network-hardware-capability-probe-e1000-com1.log' --success-marker PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE_READY --timeout 20 --no-audio-device --no-virtio-blk --network-device e1000 --expect-outcome success

& 'C:\Users\NeverAMoment\AppData\Local\Python\pythoncore-3.14-64\python.exe' scripts/run-qemu.py --serial-log 'D:\PythOS-Workspace\repo\pythos\.worktrees\phase15-network-hardware\target\network-hardware-capability-probe-e1000e-com1.log' --success-marker PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE_READY --timeout 20 --no-audio-device --no-virtio-blk --network-device e1000e --expect-outcome success
```

The SHA-256 values were computed with:

```powershell
Get-FileHash -Algorithm SHA256 -LiteralPath target\network-hardware-capability-probe-e1000-com1.log,target\network-hardware-capability-probe-e1000e-com1.log
```

## Recorded QEMU acceptance

| Model | COM1 log | Bytes | SHA-256 | Recorded result |
|---|---|---:|---|---|
| `e1000` (`8086:100E`) | `target/network-hardware-capability-probe-e1000-com1.log` | 2292 | `9ce1e91769e4b1312598b08bb2daa16b7f61d30f0218e23ffb44c2c5b146f705` | Status `0x0000000000000000`, raw pointer `0x00`, list absent, all summary fields `NONE`, ready marker, `QEMU_OUTCOME success` |
| `e1000e` (`8086:10D3`) | `target/network-hardware-capability-probe-e1000e-com1.log` | 2800 | `2401dbb65ddac21d959d4409e3093a7c3cc1b552963a1594910647fc25ef7968` | Status `0x0000000000000010`, raw pointer `0xC8`, PM `0xC8`, MSI `0xD0`, PCIe `0xE0`, MSI-X `0xA0`, ready marker, `QEMU_OUTCOME success` |

Both runs emitted `PCI_CONFIG_READ_ONLY`, rendered
`FRAMEBUFFER_CAPABILITY_READY`, reached
`PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE_READY`, and produced exactly
one successful QEMU terminal outcome. The complete oracle ended with
`NETWORK_HARDWARE_CAPABILITY_PROBE_TEST_OK`.

The COM1 logs are generated `target/` artifacts rather than tracked source.
Their paths and hashes above bind this record to the exact bytes reviewed from
the named implementation commit.

## Lenovo 81VS physical observation

The owner-authorized ISO
`F:\iso\pythos-phase15-pci-capability-snapshot-20260929.iso` was booted on the
Lenovo `81VS`. The existing Ventoy/ISO contents were preserved. The ISO size
was `20,652,032` bytes and its SHA-256 was
`8F968E0315D6CE46C18E58167D2B273F5EAF8D173477A97B76ACE7488711DDD1`.

The framebuffer reported:

```text
network pci caps
config read only
bdf 02 00 00
vid did 10EC C82F
status 0010
irq line pin FF 01
list present
PM 40
PCIe 70
MSI 50
MSIX NONE
```

This records the Realtek `10EC:C82F` function at BDF `02:00:00`, a present
conventional capability list, PM at `0x40`, PCIe at `0x70`, MSI at `0x50`, and
no MSI-X capability. The probe remained configuration-read-only; it did not
enable any capability, access BAR/MMIO/device registers, configure interrupts,
initialize the controller, enable bus mastering, perform DMA, move packets,
or associate with Wi-Fi.

### Physical photo artifact

- Source: `D:\Downloads\Mobile Devices\20260929_080413.jpg`
- SHA-256: `173F7439A53969C7EF9A790E32470D411F24F436C6C6E644F569FF3483D0EAC1`
- Size: `1,200,398` bytes

The supplied framebuffer photo is the physical acceptance artifact. No serial
log was captured for this framebuffer-only run.

## Acceptance conclusion

ADR 0109 passed its QEMU and owner-authorized Lenovo metadata-observation
gates. The result is target-specific configuration metadata and does not
establish controller readiness, interrupt delivery, physical networking,
Lenovo Wi-Fi, or any later controller operation.
