# ADR 0002: Replace Loader Page Tables Before Framebuffer Rendering

Date: 2026-07-14

## Status

Accepted

## Context

Milestone 1 completed the firmware handoff and proved PythCore execution after
`ExitBootServices()`, but execution initially depended on loader-owned page
tables. Those tables contained a broad 2 MiB-to-4 GiB identity map so the loader
could survive the first `CR3` switch into PythCore.

Scheduler, timer, IPC, and runtime work should not be built on that transitional
mapping model.

## Decision

PythCore now allocates replacement page-table pages from its physical allocator,
discovers the active physical backing of existing mappings by walking the loader
page tables, and builds a kernel-owned address space before framebuffer
rendering.

The replacement address space:

1. maps linker-defined PythCore rodata, text, and data/BSS ranges,
2. enforces W^X permissions for those kernel regions,
3. maps the active guarded bootstrap stack,
4. maps `PythBootInfo`, the retained memory map, and `INIT.PAK`,
5. maps the framebuffer at the device-region virtual address,
6. maps only the page-table frames required for post-switch validation,
7. keeps the first 2 MiB unmapped,
8. omits the broad loader identity map,
9. switches `CR3` a second time, and
10. emits `PYTHOS:CORE:VM_READY` only after active-layout validation succeeds.

Loader page-table frames are not reclaimed in this slice because PythCore does
not yet retain exact ownership records for those frames.

## Consequences

The milestone serial order now includes:

```text
PYTHOS:CORE:IDT_READY
PYTHOS:CORE:VM_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

The broad loader identity map is no longer active when the boot screen renders.
Future work can add exception diagnostics, boot-information completion, and
deterministic QEMU exit on top of PythCore-owned virtual memory.
