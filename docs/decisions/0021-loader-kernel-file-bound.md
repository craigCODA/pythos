# ADR 0021: Loader Kernel File Read Bound

## Status

Accepted

## Context

Phase 6 adds native cinematic boot, AC97 audio code, embedded boot assets, and
audio/visual sync logic to PythCore. The debug `PYTHCORE.ELF` copied into the
ESP exceeded the loader's earlier 2 MiB read buffer and caused the loader to
fail before `PYTHOS:LOADER:KERNEL_LOADED`.

The loader must keep a finite read bound. Unbounded firmware file reads are not
acceptable in early boot.

## Decision

Raise the loader's maximum kernel ELF file read buffer from 2 MiB to 4 MiB.
Keep the existing rejection when UEFI reports a zero-byte read or exactly fills
the maximum buffer, because an exactly full read still means the loader cannot
prove it read the whole file.

## Consequences

The Phase 6 debug kernel can boot without weakening ELF validation or accepting
unbounded input. Future kernel growth still has a clear failure mode at the
finite 4 MiB cap, and a later size increase must be justified explicitly.
