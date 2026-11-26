# ADR 0115 — NVMe world checkpoints in QEMU

## Decision

The opt-in `world-session-nvme` profile runs the native editable world with an
NVMe backing disk. The ring-3 worker still returns a validated world edit
through the boundary defined by [ADR 0114](0114-native-editable-world.md).
PythCore owns the controller, the write range, and the commit. The
`world-session-probe` profile uses its Virtio world image.

Selection requires the single emulated PCI controller and the Identify
Controller serial `PYTHOS-NVME-WORLD`. The profile uses namespace 1 with
512-byte logical blocks and zero metadata. It checks the protective MBR,
both GPT headers, both entry arrays and their CRCs. The accepted table has
128 entries of 128 bytes. The selected entry must have the fixture's
unique partition GUID `71dff308-92c4-4e88-b4a9-4b9400c87d66`, the Microsoft
Basic Data type, and the hidden attribute. Selection rejects a read-only
or shadow-copy entry, an overlap, or an invalid range. The partition starts
on a 4 KiB boundary. [UEFI GPT layout](https://uefi.org/specs/UEFI/2.10/05_GUID_Partition_Table_Format.html)
[Windows GPT attributes](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-partition_information_gpt)

The first page contains the `PYWNVM01` marker and the fixture's optional
pause revision. Four following 4 KiB pages contain two snapshot and commit
marker pairs. The records use the first 512 bytes of each page and keep the
same body and checksum encoding as ADR 0114. The authorization range
covers these five pages. Only the four checkpoint pages receive Writes.

A commit clears and flushes the target marker, writes and flushes the
snapshot body, verifies the body, then writes, flushes, and verifies the
new marker. The world publishes the edit after the final verification.
Each stage uses one aligned 4 KiB NVMe Write followed by a Flush for the
same namespace. A recovered snapshot must have a matching marker, revision,
and body checksum. The controller is disabled and PCI bus mastering is
confirmed off on controlled exit or error. [NVMe 1.4b](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4b-2020.09.21-Ratified.pdf)

## Acceptance

```powershell
py -3 -B scripts/test-nvme-world.py
```

The harness supplies a regular image file to the emulated controller
with write caching enabled and flushes honored. It commits an object and
its binding, restarts, and interrupts the next edit after the body flush.
Another boot recovers the committed revision and uses its binding. The
harness inspects the checkpoint bytes, checks the serial write and flush
sequence, and compares all bytes outside the four checkpoint pages.
Rejection fixtures cover the wrong controller and partition identities,
an overlapping or short partition, a read-only partition, an invalid backup
header, and a marker for another disk format. The corrupt-header fixture is
attached read-only so OVMF leaves it available for PythCore to validate.

This evidence covers QEMU and process interruption between commit stages.
A physical deployment requires target-specific controller ownership,
partition provisioning, and fresh hardware validation.
