# Milestone 1 Boot Complete Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the active `milestone/boot-core-handoff` marker sequence through `PYTHOS:CORE:MILESTONE_1_COMPLETE`.

**Architecture:** Keep PythCore as a small native survival core. Finish only milestone-1 mechanisms: physical page ownership, bitmap allocator, GDT/TSS load, IDT load, framebuffer marker ordering, and serial acceptance evidence. The cinematic HTML/MP4 remains visual direction only and is not embedded in core.

**Tech Stack:** Rust 2024 `#![no_std]`/`#![no_main]`, x86-64 UEFI loader, PythCore `x86_64-unknown-none`, QEMU/OVMF serial acceptance tests, Python test harness.

## Global Constraints

Implement only the active milestone.
Do not invent or silently change an ABI.
Do not add future features to the active milestone.
Every unsafe block requires the documented eight-part invariant.
Every milestone requires an automated QEMU acceptance test.
Serial output is the test oracle for early boot.
A successful compile is not a successful boot.
A screenshot is not sufficient evidence.
Do not claim full security where only logical isolation exists.
AI remains outside the trusted core.

---

### Task 1: Memory-Ready Slice

**Files:**
- Modify: `scripts/test-boot.py`
- Modify: `tests/boot_core_handoff.py`
- Modify: `core/src/main.rs`
- Modify: `core/src/memory/physical.rs`

**Interfaces:**
- Consumes: `PythBootInfo` fields already validated by `core/src/boot_info.rs`
- Produces: `memory::physical::initialize(&PythBootInfo) -> Result<PhysicalMemory, MemoryError>`

- [ ] Add `memory-ready` expected markers after `BOOTINFO_VALID` and before `FRAMEBUFFER_READY`.
- [ ] Run `python scripts/test-boot.py --slice memory-ready` and confirm it fails on missing `PYTHOS:CORE:MEMORY_READY`.
- [ ] Implement UEFI descriptor walking, page ownership classification, required range reservation, and a 4 KiB bitmap allocator.
- [ ] Emit `PYTHOS:CORE:MEMORY_READY` on success and `PYTHOS:CORE:MEMORY_INVALID` on failure.
- [ ] Run host tests and `python scripts/test-boot.py --slice memory-ready`.

### Task 2: GDT/TSS Slice

**Files:**
- Modify: `scripts/test-boot.py`
- Modify: `tests/boot_core_handoff.py`
- Modify: `core/src/main.rs`
- Modify: `core/src/architecture/x86_64/gdt.rs`
- Modify: `core/src/architecture/x86_64/tss.rs`

**Interfaces:**
- Consumes: memory initialization success
- Produces: `architecture::x86_64::gdt::initialize() -> Result<(), ()>`

- [ ] Add `gdt-ready` expected markers after `MEMORY_READY`.
- [ ] Run `python scripts/test-boot.py --slice gdt-ready` and confirm it fails on missing `PYTHOS:CORE:GDT_READY`.
- [ ] Install a minimal 64-bit GDT with kernel code, kernel data, and TSS descriptors.
- [ ] Load `gdtr`, reload segment registers, and load `tr`.
- [ ] Emit `PYTHOS:CORE:GDT_READY` only after successful install.

### Task 3: IDT Slice

**Files:**
- Modify: `scripts/test-boot.py`
- Modify: `tests/boot_core_handoff.py`
- Modify: `core/src/main.rs`
- Modify: `core/src/architecture/x86_64/idt.rs`
- Modify: `core/src/architecture/x86_64/exceptions.rs`

**Interfaces:**
- Consumes: GDT/TSS initialization success
- Produces: `architecture::x86_64::idt::initialize() -> Result<(), ()>`

- [ ] Add `idt-ready` expected markers after `GDT_READY`.
- [ ] Run `python scripts/test-boot.py --slice idt-ready` and confirm it fails on missing `PYTHOS:CORE:IDT_READY`.
- [ ] Install a bounded IDT with exception gates pointing to panic-loop stubs.
- [ ] Load `idtr`.
- [ ] Emit `PYTHOS:CORE:IDT_READY` only after successful install.

### Task 4: Milestone Completion

**Files:**
- Modify: `scripts/test-boot.py`
- Modify: `tests/boot_core_handoff.py`
- Modify: `core/src/main.rs`
- Modify: `docs/PythOS-TDD-001.md`
- Modify: `docs/HANDOVER.md`
- Modify: `docs/decisions/`

**Interfaces:**
- Consumes: `MEMORY_READY`, `GDT_READY`, and `IDT_READY`
- Produces: final ordered serial sequence ending in `PYTHOS:CORE:MILESTONE_1_COMPLETE`

- [ ] Run `python scripts/test-boot.py --slice milestone-1` and confirm it fails before the final marker exists.
- [ ] Move framebuffer rendering and `FRAMEBUFFER_READY` after `IDT_READY`.
- [ ] Emit `PYTHOS:CORE:MILESTONE_1_COMPLETE` after framebuffer readiness.
- [ ] Record architecture changes as ADRs under `docs/decisions/`.
- [ ] Run `cargo fmt --check`, target builds/clippy, `python -m unittest tests.boot_core_handoff`, and `python scripts/test-boot.py --slice milestone-1`.
