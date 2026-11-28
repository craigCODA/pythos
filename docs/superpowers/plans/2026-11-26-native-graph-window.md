# Native PythTIG Window Implementation Plan

## Goal

Boot into a live, draggable native window with graph-authored drawing and a
placement that survives a QEMU restart.

## Architecture

Use the bounded ring-3 graph worker, retained input, a timed damage-based
framebuffer presenter, and an asynchronous placement journal on disposable
emulated NVMe. [ADR 0118](../../decisions/0118-native-graph-window.md)

## Graph and scene

- [x] Build `user/probes/graph-window/window.pyth` into a named graph package.
- [x] Run it through the stock verifier and interpreter in the new worker.
- [x] Verify strict scene validation and graph-supplied drawing data with
  `cargo test -p pythos-user-graph-window`.

## Pointer, presentation, and storage

- [x] In the `world-session-graph-window` boot profile, present one window
  on the native backdrop. Give the title bar pointer capture and persist the
  resulting pose at release.
- [x] Track timed frame completion and animation independently of pointer
  motion and the pending NVMe transaction.
- [x] In `scripts/test-graph-window.py`, demonstrate idle animation, capture
  and release, continued drawing during a held save, durable restoration, and
  preservation of bytes outside the four checkpoint pages.

## Regression checks

- [x] Run the original native world and NVMe tests for the existing profiles.
- [x] Run the formatting check, Rust host suites, Python suite, normal boot,
  and persistent-storage recovery checks.
