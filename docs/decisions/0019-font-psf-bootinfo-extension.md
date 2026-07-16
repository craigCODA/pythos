# ADR 0019: FONT.PSF Bootinfo Extension

Date: 2026-07-16

## Status

Accepted

## Context

Phase 5 replaces the embedded diagnostic 8x8 font with `FONT.PSF` loading.
The ESP and ISO already contain `/PYTHOS/FONT.PSF`, but the loader did not
previously load it or pass it through `PythBootInfo`.

Passing this through a reserved field would silently change the boot ABI and
violate the project rule against invented ABI changes.

## Decision

`PythBootInfo` gains explicit `font_phys` and `font_len` fields, and the boot
ABI minor version increments. The loader reads `/PYTHOS/FONT.PSF`, validates
that it is nonempty and bounded, copies it into page-aligned loader-owned
physical memory, and passes its physical address and byte length to PythCore.

PythCore reserves the font page range during physical-memory classification,
maps the font bytes into the replacement kernel-owned page tables, validates
the PSF header, and uses the parsed glyph metadata for Phase 5 font-system
proofs.

## Consequences

The boot ABI now has an explicit font payload contract. Later phases can change
font format or add multiple fonts only through another ADR and ABI update.

`FONT.PSF` is boot UI data only. This ADR does not add storage, package
management, theming, or user-provided fonts.

