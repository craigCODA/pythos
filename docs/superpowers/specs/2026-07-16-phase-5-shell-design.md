# Phase 5 Shell Design

## Goal

Complete Phase 5 through the roadmap-defined graphical shell boundary while
preserving the strict serial-oracle workflow used by earlier phases.

## Architecture

Phase 5 stays inside the existing milestone directories: `boot/`, `core/`,
`shared/`, `scripts/`, `tests/`, and `docs/`. Each slice adds a focused
PythCore module with host unit tests and a QEMU serial marker. The slice marker
sequence extends the existing boot path after `PYTHOS:CORE:ASYNC_EVENTS_READY`
and before `PYTHOS:CORE:FRAMEBUFFER_READY`.

Input starts as native keyboard/mouse driver proofs. A native input-event
service normalizes raw driver events into typed input events behind capability
checks. The shell model uses stable typed object ids plus replaceable
presentation bindings from the first surface/window.

The renderer and compositor remain software framebuffer code. The font slice
loads `/PYTHOS/FONT.PSF` through an explicit boot-info ABI extension. First
applications are fixed, capability-scoped service records backed by the Phase 4
service lifecycle infrastructure.

## Slice Order

1. `keyboard-driver` / `mouse-driver`
2. `input-event-service`
3. `software-renderer`
4. `font-system`
5. `compositor` / `surfaces` / `clipping`
6. `pointer-cursor` / `window-focus` / `movable-windows`
7. `buttons-and-text-fields`
8. `application-launcher` / `service-monitor` / `python-console` /
   `settings-panel`

## Constraints

Do not add Open Surface, Patch, persistent storage, networking, audio, agent
concepts, ring-3 execution, SMP, broad Python compatibility, or user-authored
GUI environments. Do not claim full security; Phase 5 services are still
kernel-mode logical isolation.

