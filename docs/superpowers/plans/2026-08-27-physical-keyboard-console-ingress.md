# Physical Keyboard Console Ingress Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the existing ring-3 object shell receive bounded physical keyboard input through the existing console syscall path.

**Architecture:** PythCore remains the only code that reads i8042 ports. The shell keeps its `SYSCALL_CONSOLE_READ_BYTE` interface; the kernel returns COM2 bytes first, then feature-gated physical keyboard bytes translated from existing `KeyCode` events into ASCII/control bytes.

**Tech Stack:** Rust `no_std` kernel code, Rust host unit tests, Python QEMU harnesses, PythOS normal boot.

**Spec:** `docs/decisions/0075-physical-input-event-diagnostic.md` plus the user-approved follow-up scope in this worktree.

## Global Constraints

- Do not grant ring-3 direct I/O port access.
- Preserve COM1 as boot/verification oracle and COM2 as the interactive shell transport.
- Preserve existing normal boot and verification markers.
- Feature-gate physical keyboard console ingress.
- Claim only the i8042/PS2 keyboard path until separate USB/HID/trackpad evidence exists.

---

### Task 1: Console Byte Translation

**Files:**
- Create: `core/src/physical_keyboard_console.rs`
- Modify: `core/src/main.rs`

**Interfaces:**
- Consumes: `crate::input_drivers::KeyCode`
- Produces: `pub(crate) fn keycode_to_console_byte(key: KeyCode) -> Option<u8>`

- [x] **Step 1: Write the failing test**

Add tests proving alpha keys, digits, `Space`, `Enter`, and `Backspace` map to lowercase ASCII, digits, space, carriage return, and delete/backspace.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p pythos-core physical_keyboard_console --quiet`

- [x] **Step 3: Write minimal implementation**

Implement `keycode_to_console_byte` with explicit `KeyCode` matches. Do not add layout, shift, or modifier behavior.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p pythos-core physical_keyboard_console --quiet`

### Task 2: Feature-Gated Syscall Ingress

**Files:**
- Modify: `core/Cargo.toml`
- Modify: `core/src/syscall.rs`
- Modify: `core/src/ps2.rs`
- Modify: `core/src/main.rs`

**Interfaces:**
- Consumes: `serial::try_read_byte_com2()`, `ps2::poll_raw_output_byte()`, `physical_keyboard_console::keycode_to_console_byte`
- Produces: feature `physical-keyboard-console`, serial markers `PYTHOS:CORE:PHYSICAL_KEYBOARD_CONSOLE:READY` and `PYTHOS:CORE:PHYSICAL_KEYBOARD_CONSOLE:BYTE`

- [x] **Step 1: Write the failing test**

Add a host test for a helper that prefers COM2 bytes and falls back to physical keyboard bytes.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p pythos-core console_read --quiet`

- [x] **Step 3: Write minimal implementation**

Add the feature flag, compile the new module for test or `physical-keyboard-console`, and have `dispatch_console_read` return COM2 first, then the next mapped i8042 keyboard byte.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p pythos-core console_read --quiet`

### Task 3: Normal-Boot Acceptance Harness

**Files:**
- Create: `scripts/test-physical-keyboard-console.py`

**Interfaces:**
- Consumes: QEMU QMP key injection helpers
- Produces: `PHYSICAL_KEYBOARD_CONSOLE_TEST_OK`

- [x] **Step 1: Write the QEMU harness**

Build normal boot with `physical-keyboard-console`, click through the launcher, wait for the shell prompt on COM2, type `help` through QMP keyboard events, and require shell help output on COM2.

- [x] **Step 2: Run harness to verify feature acceptance after syscall ingress was complete**

Run: `py -3 scripts/test-physical-keyboard-console.py`

- [x] **Step 3: Confirm no extra QMP helper was required**

Use the existing `launcher_click.press_qcode_keys` helper instead of adding a
duplicate QMP keyboard path.

- [x] **Step 4: Rerun harness to verify it passes**

Run: `py -3 scripts/test-physical-keyboard-console.py`

### Task 4: Documentation

**Files:**
- Create: `docs/decisions/0076-physical-keyboard-console-ingress.md`
- Modify: `README.md`
- Modify: `docs/technical-notes.md` or the existing current technical status document if present

**Interfaces:**
- Consumes: QEMU harness result and prior ADR 0075 boundary
- Produces: a narrow claim statement for the i8042 shell-ingress slice

- [x] **Step 1: Document the decision**

Write ADR 0076 stating that the kernel owns the i8042 port reads and exposes only console bytes through the existing capability-gated syscall.

- [x] **Step 2: Update current docs**

Add the QEMU acceptance command and the real-hardware follow-up boundary.

- [x] **Step 3: Run doc/status checks**

Run the final verification commands and update `D:\PythOS-Workspace\CURRENT-STATE.md` only after acceptance passes.
