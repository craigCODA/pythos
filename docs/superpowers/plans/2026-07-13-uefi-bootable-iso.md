# UEFI Bootable ISO Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and verify a UEFI bootable `target/pythos.iso` for the existing milestone-1 PythOS boot path.

**Architecture:** Add a pure-Python ISO builder that embeds a FAT16 EFI System Partition as an El Torito no-emulation UEFI boot image. Extend existing QEMU and boot-test scripts to boot either the existing ESP directory or the new ISO while preserving serial-output assertions.

**Tech Stack:** Python standard library, Rust target artifacts, QEMU/OVMF, ISO9660 El Torito, FAT16.

## Global Constraints

Implement only the active milestone.
Do not invent or silently change an ABI.
Do not add future features to the active milestone.
Serial output is the test oracle for early boot.
A successful compile is not a successful boot.
A screenshot is not sufficient evidence.
AI remains outside the trusted core.

---

### Task 1: ISO Builder Structure

**Files:**
- Create: `scripts/build-iso.py`
- Create: `tests/test_iso_image.py`

**Interfaces:**
- Produces: `python scripts/build-iso.py --output target/pythos.iso`

- [x] Write a failing structural test that imports `scripts/build-iso.py`, builds a small ISO in a temporary directory, and asserts `CD001`, `EL TORITO SPECIFICATION`, and UEFI platform ID `0xEF`.
- [x] Run `python -m unittest tests.test_iso_image` and confirm it fails because the script is missing.
- [x] Implement FAT16 ESP image generation and ISO9660 El Torito wrapping.
- [x] Run `python -m unittest tests.test_iso_image` and confirm it passes.

### Task 2: QEMU ISO Boot Path

**Files:**
- Modify: `scripts/run-qemu.py`
- Modify: `scripts/test-boot.py`
- Modify: `tests/boot_core_handoff.py`

**Interfaces:**
- Consumes: `target/pythos.iso`
- Produces: `python scripts/test-boot.py --slice milestone-1 --media iso`

- [x] Add a failing QEMU acceptance test for `--media iso`.
- [x] Run `python scripts/test-boot.py --slice milestone-1 --media iso` and confirm it fails because `--media` or ISO booting is not implemented.
- [x] Add `--iso` to `run-qemu.py` and `--media esp|iso` to `test-boot.py`.
- [x] Add a unittest method covering ISO milestone boot.
- [x] Run `python scripts/test-boot.py --slice milestone-1 --media iso` and confirm it passes.

### Task 3: Docs And Verification

**Files:**
- Modify: `docs/HANDOVER.md`
- Modify: `docs/PythOS-TDD-001.md`

**Interfaces:**
- Documents: `target/pythos.iso` build and boot commands.

- [x] Document the ISO command and serial-test requirement.
- [x] Run `cargo fmt --check`, target builds, clippy, host tests, `python -m unittest tests.boot_core_handoff`, `python -m unittest tests.test_iso_image`, and `python scripts/test-boot.py --slice milestone-1 --media iso`.
- [x] Commit with no co-author trailer.
