# Physical Input Event Diagnostic Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an opt-in verify-only diagnostic that samples post-wake physical keyboard bytes, normalizes a small key set into typed events, and reports raw bytes plus keys on framebuffer and COM1.

**Architecture:** Add a separate `physical-input-event-diagnostic` feature and module instead of widening ADR 0074's exact wake gate. The diagnostic reuses the existing keyboard-only PS/2 polling setup, converts raw make bytes into `InputEventKind::KeyDown` values for `Space`, `Backspace`, `W`, `A`, `K`, `E`, and `Enter`, and accepts only the sequence `space space backspace backspace wake enter`.

**Tech Stack:** Rust `no_std` PythCore, existing framebuffer text renderer, existing PS/2 polling helper, existing Python QEMU/QMP harness utilities.

**Spec:** `docs/decisions/0074-physical-wake-diagnostic.md` and `D:\PythOS-Workspace\CURRENT-STATE.md`.

## Global Constraints

- The diagnostic must require `verify` and must not run in default or normal boot.
- Keep ADR 0074's `physical-wake-diagnostic` behavior unchanged.
- Do not add USB HID, trackpad support, IRQ-driven physical input, shell keyboard control, login/auth, or generic hardware claims.
- Preserve existing serial boot markers and the default QEMU acceptance path.
- Add host-side tests before implementation and run `cargo fmt --check` before push.

---

### Task 1: Diagnostic State Machine

**Files:**
- Create: `core/src/physical_input_diagnostic.rs`
- Modify: `core/src/main.rs`
- Modify: `core/src/ps2.rs`
- Modify: `core/Cargo.toml`

**Interfaces:**
- Consumes: `ps2::initialize_keyboard_polling()`, `ps2::poll_raw_output_byte()`, `input_drivers::KeyCode`, `input_events::normalize`
- Produces: `physical_input_diagnostic::run(framebuffer: &PythFramebufferInfo)`, `InputDiagnostic::feed_raw_byte(byte: u8) -> InputDiagnosticStep`

- [x] **Step 1: Write failing Rust tests**

Add tests in `core/src/physical_input_diagnostic.rs` proving:

```rust
#[test]
fn set1_space_backspace_wake_sequence_accepts() {
    let mut diagnostic = InputDiagnostic::new();
    let bytes = [0x39, 0x39, 0x0E, 0x0E, 0x11, 0x1E, 0x25, 0x12, 0x1C];
    for byte in &bytes[..bytes.len() - 1] {
        assert_eq!(diagnostic.feed_raw_byte(*byte).result, InputDiagnosticResult::Waiting);
    }
    assert_eq!(diagnostic.feed_raw_byte(bytes[bytes.len() - 1]).result, InputDiagnosticResult::Accepted);
    assert_eq!(diagnostic.text_bytes(), b"wake");
}

#[test]
fn set2_release_bytes_are_ignored_before_acceptance() {
    let mut diagnostic = InputDiagnostic::new();
    let bytes = [0x29, 0xF0, 0x29, 0x29, 0x66, 0x66, 0x1D, 0x1C, 0x42, 0x24, 0x5A];
    for byte in &bytes[..bytes.len() - 1] {
        assert_ne!(diagnostic.feed_raw_byte(*byte).result, InputDiagnosticResult::Accepted);
    }
    assert_eq!(diagnostic.feed_raw_byte(bytes[bytes.len() - 1]).result, InputDiagnosticResult::Accepted);
}
```

- [x] **Step 2: Run red test**

Run: `cargo test -p pythos-core physical_input`
Result: FAIL because `InputDiagnostic`, `InputDiagnosticResult`, and framebuffer formatting helpers were not implemented.

- [x] **Step 3: Implement minimal state machine and feature gate**

Add the `physical-input-event-diagnostic` feature, compile error requiring `verify`, shared PS/2 polling gates, and the state machine that decodes the small set-1/set-2 key set.

- [x] **Step 4: Run green test**

Run: `cargo test -p pythos-core physical_input`
Result: PASS, 8 tests.

### Task 2: Framebuffer and Serial Reporting

**Files:**
- Modify: `core/src/framebuffer.rs`
- Modify: `core/src/font.rs`
- Modify: `core/src/physical_input_diagnostic.rs`

**Interfaces:**
- Consumes: `InputDiagnostic::raw_bytes()`, `InputDiagnostic::text_bytes()`, `InputDiagnostic::key_events()`
- Produces: `framebuffer::render_physical_input_diagnostic(...)`

- [x] **Step 1: Write failing renderer tests**

Add tests for:

```rust
assert_eq!(format_physical_input_keys_line(&[KeyCode::Space, KeyCode::Backspace, KeyCode::W], &mut buffer).unwrap(), "keys sp bs w");
assert_eq!(format_physical_input_text_line(b"wake", &mut buffer).unwrap(), "text wake_");
```

- [x] **Step 2: Run red test**

Run: `cargo test -p pythos-core physical_input`
Result: FAIL because the framebuffer formatting functions did not exist.

- [x] **Step 3: Implement panel and serial event markers**

Render title, instruction, text buffer, key-event log, raw-byte log, and status. Emit `PYTHOS:CORE:PHYSICAL_INPUT:RAW:<hex>`, `PYTHOS:CORE:PHYSICAL_INPUT:KEY:<name>`, `READY`, `REJECTED`, and `ACCEPTED` on COM1.

- [x] **Step 4: Run green test**

Run: `cargo test -p pythos-core physical_input`
Result: PASS, 8 tests.

### Task 3: QEMU Acceptance Harness and Docs

**Files:**
- Create: `scripts/test-physical-input-event-diagnostic.py`
- Modify: `scripts/launcher_click.py`
- Modify: `README.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Create: `docs/decisions/0075-physical-input-event-diagnostic.md`

**Interfaces:**
- Consumes: `launcher_click.press_qcode_keys(...)`
- Produces: `PHYSICAL_INPUT_EVENT_DIAGNOSTIC_TEST_OK`

- [x] **Step 1: Write harness expecting new markers**

The harness builds PythCore with `--features verify,physical-input-event-diagnostic`, waits for `PYTHOS:CORE:PHYSICAL_INPUT:READY`, injects `spc spc backspace backspace w a k e ret`, and requires ordered key markers plus `PYTHOS:CORE:PHYSICAL_INPUT:ACCEPTED`.

- [x] **Step 2: Run harness red if code is absent**

Run: `python scripts\test-physical-input-event-diagnostic.py`
Result before using the correct launcher on this machine: FAIL because the Windows `python` Store alias was first on PATH. Reran with `py -3`.

- [x] **Step 3: Complete docs**

Document the feature as QEMU-accepted when the harness passes, and explicitly state that physical acceptance is still pending until the user boots and types the same sequence on hardware.

- [x] **Step 4: Run full verification**

Run:

```powershell
cargo fmt --check
cargo test -p pythos-core physical_input
cargo test -p pythos-core physical_wake
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify,physical-input-event-diagnostic -- -D warnings
python scripts\test-physical-wake-diagnostic.py
python scripts\test-physical-input-event-diagnostic.py
```

Expected: all pass with no formatting failure.

Observed verification:

```text
cargo fmt --check
PASS

cargo test -p pythos-core
587 passed

cargo clippy -p pythos-core --target x86_64-unknown-none --features verify,physical-input-event-diagnostic -- -D warnings
PASS

py -3 scripts\test-physical-wake-diagnostic.py
PHYSICAL_WAKE_DIAGNOSTIC_TEST_OK
QEMU_OUTCOME success

py -3 scripts\test-physical-input-event-diagnostic.py
PHYSICAL_INPUT_EVENT_DIAGNOSTIC_TEST_OK
QEMU_OUTCOME success
```
