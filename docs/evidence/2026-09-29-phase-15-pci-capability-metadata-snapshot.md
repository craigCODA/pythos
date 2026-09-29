# Phase 15 PCI Capability Metadata Snapshot — QEMU Evidence

**ADR:** [0109 — Bounded PCI Capability Metadata Snapshot](../decisions/0109-phase-15-pci-capability-metadata-snapshot.md)

**ADR status:** Proposed for owner review

**Evidence scope:** QEMU only

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

## Lenovo 81VS remains pending

No Lenovo `81VS` evidence is recorded for ADR 0109. The QEMU Intel-model
observations do not establish the meaning of the Lenovo Realtek `10EC:C82F`
capabilities. Task 6 remains pending because `F:\iso` and the physical Lenovo
target are unavailable for this repository-only fix wave. No physical ISO was
built, copied, booted, or observed, and existing Ventoy/ISO contents were not
touched.
