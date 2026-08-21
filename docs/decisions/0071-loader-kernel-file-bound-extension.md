# ADR 0071: Phase 12 Loader Kernel File Bound Extension

Date: 2026-08-21
Status: Accepted

## Context

ADR 0021 raised the loader's finite `PYTHCORE.ELF` read buffer from 2 MiB to
4 MiB and required any later growth to be justified explicitly. Phase 12 Slice
2 adds the in-kernel object-locator resolver and its QEMU acceptance proof. The
resulting debug `PYTHCORE.ELF` is 4,240,360 bytes, which exceeds the ADR 0021
4 MiB buffer and makes the loader fail before
`PYTHOS:LOADER:KERNEL_LOADED`.

The failure is not a resolver denial and not a PythCore panic. It occurs while
the loader reads the kernel file from the boot volume.

## Decision

Raise the loader's maximum kernel ELF file read buffer from 4 MiB to 8 MiB.
Keep the existing finite bound and keep the existing rejection when UEFI
reports a zero-byte read or exactly fills the maximum buffer. An exactly full
read still means the loader cannot prove it read the complete file.

This does not change the PythCore boot ABI, ELF segment validation, object
locator ABI, typed object layout, capability semantics, or serial marker order.

## Consequences

The Phase 12 Slice 2 debug acceptance image can reach PythCore without
weakening loader validation or accepting unbounded firmware input. Future
kernel growth still has a deterministic loader failure mode at the finite
8 MiB cap, and another increase must again be justified explicitly.
