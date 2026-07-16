# Phase 5 Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Phase 5 so Phase 6 is the next roadmap step.

**Architecture:** Add one focused module per Phase 5 slice. Extend `scripts/test-boot.py` and `tests/boot_core_handoff.py` with a new marker slice before implementation, prove the red failure, implement the smallest passing code, run unit tests plus the active QEMU slice, then commit.

**Tech Stack:** Rust `no_std` PythCore modules, Rust host unit tests, Python QEMU marker harness, UEFI loader file loading for `FONT.PSF`.

## Global Constraints

Serial output is the oracle. A compile is not a boot proof.

Phase 5 markers must appear after `PYTHOS:CORE:ASYNC_EVENTS_READY` and before `PYTHOS:CORE:FRAMEBUFFER_READY`.

Every ABI change gets an ADR.

No audio, storage, networking, AI, ring-3, SMP, Open Surface, Patch, or broad Python compatibility.

---

### Task 1: Input Drivers

**Files:** `core/src/input_drivers.rs`, `core/src/main.rs`, `scripts/test-boot.py`, `tests/boot_core_handoff.py`, docs.

- [ ] Add `keyboard-driver` marker tests and verify they fail.
- [ ] Implement fixed keyboard and mouse event decoding behind capability checks.
- [ ] Emit `PYTHOS:CORE:INPUT:KEYBOARD`, `PYTHOS:CORE:INPUT:MOUSE`, and `PYTHOS:CORE:INPUT_DRIVERS_READY`.
- [ ] Run `cargo test -p pythos-core` and `python scripts\test-boot.py --slice keyboard-driver`.
- [ ] Commit.

### Task 2: Input Event Service

**Files:** `core/src/input_events.rs`, `core/src/main.rs`, marker tests, docs.

- [ ] Add `input-event-service` marker tests and verify they fail.
- [ ] Normalize raw keyboard/mouse events into typed input events and require subscriber capability.
- [ ] Emit `PYTHOS:CORE:INPUT:EVENT` and `PYTHOS:CORE:INPUT_EVENT_SERVICE_READY`.
- [ ] Verify and commit.

### Task 3: Software Renderer

**Files:** `core/src/software_renderer.rs`, `core/src/main.rs`, marker tests, docs.

- [ ] Add `software-renderer` marker tests and verify they fail.
- [ ] Implement clipped fill-rect and textless pixel-buffer primitives.
- [ ] Emit `PYTHOS:CORE:RENDER:RECT` and `PYTHOS:CORE:SOFTWARE_RENDERER_READY`.
- [ ] Verify and commit.

### Task 4: Font System

**Files:** `shared/src/boot_protocol.rs`, `boot/src/font.rs`, `boot/src/main.rs`, `boot/src/boot_info.rs`, `core/src/font_system.rs`, VM mapping files, build scripts, marker tests, docs.

- [ ] Add `font-system` marker tests and verify they fail.
- [ ] Load `/PYTHOS/FONT.PSF`, pass it via boot info, map it after `VM_READY`, and parse PSF metadata.
- [ ] Emit `PYTHOS:CORE:FONT:PSF_LOADED` and `PYTHOS:CORE:FONT_SYSTEM_READY`.
- [ ] Verify and commit.

### Task 5: Compositor, Surfaces, Clipping

**Files:** `core/src/shell_objects.rs`, `core/src/compositor.rs`, marker tests, docs.

- [ ] Add `compositor` marker tests and verify they fail.
- [ ] Implement typed objects, presentation bindings, surface composition, and clipping proof.
- [ ] Emit `PYTHOS:CORE:COMPOSITOR:SURFACE`, `PYTHOS:CORE:COMPOSITOR:CLIP`, and `PYTHOS:CORE:COMPOSITOR_READY`.
- [ ] Verify and commit.

### Task 6: Pointer, Focus, Movable Windows

**Files:** `core/src/window_interaction.rs`, marker tests, docs.

- [ ] Add `window-interaction` marker tests and verify they fail.
- [ ] Implement pointer cursor state, focus selection, and movement preserving object ids.
- [ ] Emit `PYTHOS:CORE:POINTER_CURSOR_READY`, `PYTHOS:CORE:WINDOW_FOCUS_READY`, and `PYTHOS:CORE:MOVABLE_WINDOWS_READY`.
- [ ] Verify and commit.

### Task 7: Widgets

**Files:** `core/src/widgets.rs`, marker tests, docs.

- [ ] Add `widgets` marker tests and verify they fail.
- [ ] Implement fixed button activation and text-field input editing over typed widget objects.
- [ ] Emit `PYTHOS:CORE:WIDGET:BUTTON`, `PYTHOS:CORE:WIDGET:TEXT_FIELD`, and `PYTHOS:CORE:WIDGETS_READY`.
- [ ] Verify and commit.

### Task 8: First Applications And Phase Boundary

**Files:** `core/src/shell_apps.rs`, `core/src/framebuffer.rs`, `core/src/main.rs`, marker tests, docs.

- [ ] Add `phase-5-complete` marker tests and verify they fail.
- [ ] Register application launcher, service monitor, Python console, and settings panel as fixed capability-scoped services with typed windows.
- [ ] Render the shell screen through the compositor path.
- [ ] Emit `PYTHOS:CORE:APP:LAUNCHER`, `PYTHOS:CORE:APP:SERVICE_MONITOR`, `PYTHOS:CORE:APP:PYTHON_CONSOLE`, `PYTHOS:CORE:APP:SETTINGS_PANEL`, and `PYTHOS:CORE:PHASE_5_COMPLETE`.
- [ ] Run full ESP/ISO verification, update handover docs, commit, push, and stop at the Phase 5 -> Phase 6 boundary.

