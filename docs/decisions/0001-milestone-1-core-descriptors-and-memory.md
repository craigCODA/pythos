# ADR 0001: Complete Milestone 1 With Core-Owned Page State And Descriptor Tables

Date: 2026-07-12

## Status

Accepted

## Context

Milestone 1 requires PythCore to prove post-firmware machine ownership through serial markers, not screenshots or successful compilation alone. The framebuffer slice landed early for visible boot feedback, but the accepted marker order requires memory ownership, GDT, and IDT readiness before `PYTHOS:CORE:FRAMEBUFFER_READY`.

The cinematic PythOS initiation HTML/MP4 is visual direction only. Milestone 1 may not add video playback, audio, browser logic, Python runtime behavior, services, AI, or shell features.

## Decision

PythCore completes milestone 1 by:

1. walking the loader-retained UEFI memory map,
2. classifying 4 KiB pages as free or reserved,
3. reserving required loader/core ranges before exposing conventional pages,
4. initializing a fixed bitmap allocator backing store,
5. installing a minimal 64-bit GDT with kernel code, kernel data, and TSS descriptors,
6. installing a 256-entry IDT of panic-loop exception gates,
7. rendering the framebuffer boot screen after `PYTHOS:CORE:IDT_READY`,
8. emitting `PYTHOS:CORE:MILESTONE_1_COMPLETE` only after all required markers are emitted in order.

The loader-built page tables remain transitional. This milestone records logical page ownership and descriptor-table readiness, but does not claim hostile-code isolation or final kernel-owned virtual memory.

## Consequences

The QEMU serial acceptance test can prove the full milestone-1 boot path through `PYTHOS:CORE:MILESTONE_1_COMPLETE`.

Later work must replace the temporary loader mappings with kernel-owned page tables, add detailed exception diagnostics, and build runtime services above PythCore rather than expanding the trusted core.
