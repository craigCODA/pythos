# ADR 0116 — Identity-bound native NVMe world

## Decision

The `world-session-nvme-target` build uses the checkpoint format and commit
ordering from [ADR 0115](0115-nvme-world-checkpoints.md). The build embeds
one controller and partition identity generated from local inventory. On the
first target, Windows identified a WD Blue SN550 `WDS500G2B0C` with PCI
vendor and device `15B7:5009` and subsystem ID `SUBSYS_500915B7`. The inventory reads
the original 20-byte controller serial through Windows' NVMe Identify
property query. The generated identity and ISO are local build outputs.
[WD product brief](https://documents.westerndigital.com/content/dam/doc-library/en_us/assets/public/western-digital/product/internal-drives/wd-blue-nvme-ssd/product-brief-wd-blue-sn550-nvme-ssd.pdf)
[Windows NVMe property queries](https://learn.microsoft.com/en-us/windows/win32/fileio/working-with-nvme-devices)

PythCore requires exactly one matching PCI function. It turns off message
interrupts during setup and polls the same two-entry queues used in QEMU.
Controller readiness waits use the advertised `CAP.TO` interval against
PIT ticks. Command waits have a five-second budget and a watchdog for a
stalled tick source. The first namespace must match the identified
controller serial, logical block format, and namespace size before disk
selection. [NVMe 1.4b](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4b-2020.09.21-Ratified.pdf)

The primary and backup GPT headers must carry the configured disk GUID, and
their partition entry must match the configured unique GUID, attributes and
exact first and last LBA. The physical profile looks for `PYWNTG01` followed
by the disk and partition GUIDs in GPT byte order, with zero padding through
the end of the 512-byte header. This opens the four checkpoint pages only
after the partition has been explicitly prepared for this world. The
targeted header has no test pause field.

The WD boot recording reached the protective MBR check. A read-only Windows
check found a single protective record starting at LBA 1 with a count of
`0xFFFFFFFF`. Both GPT headers and entry tables passed their checksum
checks. The driver accepts either the exact protective count or the
32-bit maximum. The other three MBR records must be empty. The GPT checks
and configured reservation continue to determine which sectors can be written.

On a verified partition awaiting its world header, the framebuffer displays
`PythOS NVMe target verified` and `World header required`. On a validation
failure it displays the failing stage. A physical terminal screen halts
the boot CPU. The emulated fixture uses the ISA debug-exit device so the
acceptance runner can classify the result.

## Build and acceptance

```powershell
python -B scripts/build-nvme-target.py target/wd/identity.json target/wd/boot
python -B scripts/test-nvme-target.py
```

The builder consumes the local controller and GPT inventory and produces a
UEFI-bootable `pythos-wd-sn550-world.iso`. The acceptance harness builds the
same profile for a disposable QEMU disk. It checks a clean preflight for an
unmarked partition and a marker with the wrong identity, rejection of a
different disk and changed partition boundary, and a flushed edit that
reloads on the next boot. It compares disk contents around the checkpoint
pages and requires an observed guest result in each case.

Physical acceptance starts by booting the WD ISO to the verified partition
screen. A later boot with a prepared world header establishes physical
NVMe write and recovery evidence for this controller.
