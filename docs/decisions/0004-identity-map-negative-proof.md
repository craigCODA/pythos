# ADR 0004: Prove Loader Identity Map Removal With an Expected Page Fault

Date: 2026-07-14

## Status

Accepted

## Context

`PYTHOS:CORE:VM_READY` proved PythCore switched to a new `CR3` and continued
execution, but that alone did not prove the old broad loader identity mapping
was absent. A non-faulting page-table walk can show that an address is
untranslated, but a boot acceptance test should also prove that CPU translation
really faults on that address.

The exception diagnostic slice now provides enough machinery to distinguish an
intended page fault from an unrelated panic or hang.

## Decision

PythCore performs a controlled negative VM proof after `PYTHOS:CORE:VM_READY`:

1. choose `0x0400_0000`, an address inside the old 2 MiB-to-4 GiB identity range,
2. verify the active PythCore page tables do not translate that address,
3. arm the exception handler for exactly one expected page fault at that address,
4. execute a byte read from that address,
5. recover by rewriting the saved `RIP` to an internal assembly recovery label,
6. emit `PYTHOS:CORE:EXPECTED_PAGE_FAULT`, and
7. emit `PYTHOS:CORE:IDENTITY_MAP_REMOVED` after recovery.

If the read succeeds, the proof fails. If a different fault occurs, the normal
diagnostic panic path runs.

## Consequences

The boot serial oracle now proves that the old broad loader identity map is not
active after the PythCore-owned `CR3` switch. The expected-fault recovery path is
intentionally narrow and should not become a general exception recovery model.
