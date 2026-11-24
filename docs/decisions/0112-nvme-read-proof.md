# ADR 0112 — NVMe read proof in QEMU

## Decision

The opt-in `nvme-read-probe` profile proves a native PythCore read from a
single emulated NVMe namespace after `ExitBootServices`. The probe uses the
bridge-aware PCI discovery in [ADR 0055](0055-probe-only-emmc-storage-discovery.md)
and a cache-disabled, supervisor-only MMIO mapping. The harness presents a
separate read-only QEMU raw image behind a PCIe root port.

Selection requires a single NVMe function with PCI identity `1b36:0010`.
Identify Controller must return the proof serial `PYTHOS-NVME-READ` before
namespace or I/O commands. The probe uses namespace 1 with 512-byte logical
blocks and zero metadata. It checks the namespace size before submitting the
read of LBA 23. The controller has one admin queue pair and one I/O queue
pair, both with two entries and a single outstanding command. Queue memory
and the data page are aligned kernel DMA buffers. Completion handling checks
the phase, queue number, command identifier, and status within a finite poll
budget. The probe disables the controller and confirms PCI bus mastering
is off before reporting success. [NVMe 1.4b](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4b-2020.09.21-Ratified.pdf)

The profile terminates at the read proof. GPT selection, physical controller
ownership, writes, flush ordering, and the vault's on-disk layout need their
own slices. The current vault proof uses its existing marked Virtio image.

## Acceptance

Run `py -3 -B scripts/test-nvme-read.py`. The harness builds the profile from
source and boots the resulting ISO with a disposable fixture. It checks the
ordered serial transcript, the guest's ISA debug-exit code, and the SHA-256 of the
returned 512 bytes against a host-generated block. It compares the disk
before and after boot. Separate runs verify missing hardware, a mismatched
controller serial, and a namespace too small for LBA 23.
