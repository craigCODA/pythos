# SDHCI/eMMC Identity Screen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render the no-write SDHCI/eMMC PCI identity on the laptop framebuffer when serial is unavailable.

**Architecture:** Keep PCI discovery in `storage_probe.rs`, add a small `hardware_probe_screen.rs` formatter that selects the relevant controller and builds fixed text lines, and expose a constrained framebuffer text-panel function from `framebuffer.rs`. The hardware-probe boot path paints the existing final color first, then overlays the identity panel and emits a serial marker when rendering succeeds.

**Tech Stack:** Rust `no_std`, PythOS fixed framebuffer renderer, existing PCI config-space probe, existing Python QEMU harness.

## Global Constraints

- Implement only the approved probe identity-screen slice.
- Do not read, write, reset, initialize, mount, select, or persist against internal storage.
- Preserve existing hardware-probe color semantics and serial markers.
- Use fixed-size buffers and existing framebuffer validation.
- Every rendered character must have a fixed boot glyph.
- Do not claim physical eMMC support from QEMU evidence.

---

### Task 1: Identity-Line Formatter

**Files:**
- Create: `core/src/hardware_probe_screen.rs`
- Modify: `core/src/main.rs`
- Test: `cargo test -p pythos-core hardware_probe_screen`

**Interfaces:**
- Consumes: `storage_probe::StorageProbeReport`, `StorageController`, `StorageControllerKind`, and `MemoryBar`.
- Produces: `ProbeScreen::line_count() -> usize`, `ProbeScreen::line(index: usize) -> Option<&str>`, and `build_screen(report: &StorageProbeReport) -> ProbeScreen`.

- [ ] **Step 1: Write failing formatter tests**

```rust
let screen = build_screen(&report);
assert_eq!(screen.line(1), Some("sdhci emmc"));
assert_eq!(screen.line(4), Some("bdf 02 04 00"));
assert_eq!(screen.line(7), Some("bar0 00000000FEBC0000"));
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pythos-core hardware_probe_screen`
Expected: FAIL because `build_screen` and `ProbeScreen` do not exist.

- [ ] **Step 3: Implement fixed formatter**

Implement fixed `ProbeLine` buffers, hex writers for 8-bit, 16-bit, and 64-bit values, SDHCI-preferred controller selection, and no-storage fallback lines.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p pythos-core hardware_probe_screen`
Expected: PASS.

### Task 2: Framebuffer Text Panel

**Files:**
- Modify: `core/src/framebuffer.rs`
- Modify: `core/src/font.rs`
- Test: `cargo test -p pythos-core framebuffer font`

**Interfaces:**
- Consumes: `&[&str]` produced from `ProbeScreen`.
- Produces: `framebuffer::render_hardware_probe_lines(framebuffer: &PythFramebufferInfo, lines: &[&str]) -> Result<(), ()>`.

- [ ] **Step 1: Write failing framebuffer and glyph tests**

```rust
render_hardware_probe_lines(&info, &["PythOS", "sdhci emmc"]).unwrap();
for byte in b"PythOSsdhci emmc0123456789ABCDEF" {
    assert!(font::glyph(*byte).is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pythos-core framebuffer font`
Expected: FAIL because the render function and probe glyphs are missing.

- [ ] **Step 3: Implement panel renderer and glyphs**

Add digits, `B`, `D`, `f`, `g`, and `m` glyphs. Add `render_hardware_probe_lines` that clears to a dark background and draws the title at scale 3 and detail lines at scale 2.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p pythos-core framebuffer font`
Expected: PASS.

### Task 3: Boot Integration and Acceptance

**Files:**
- Modify: `core/src/hardware_probe_boot.rs`
- Modify: `scripts/test-hardware-probe.py`
- Test: `python scripts/test-hardware-probe.py`

**Interfaces:**
- Consumes: `hardware_probe_screen::render`.
- Produces: `PYTHOS:CORE:HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY`.

- [ ] **Step 1: Write failing acceptance expectation**

Add `PYTHOS:CORE:HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY` to the required marker order before `NO_DISK_WRITES`.

- [ ] **Step 2: Run test to verify it fails**

Run: `python scripts/test-hardware-probe.py`
Expected: FAIL because the marker is not emitted yet.

- [ ] **Step 3: Render the identity panel in probe boot**

After the final probe color is painted, call the screen renderer. Emit
`FRAMEBUFFER_IDENTITY_READY` on success and `FRAMEBUFFER_IDENTITY_FAILED` on
render failure.

- [ ] **Step 4: Run acceptance and compile gates**

Run:

```powershell
cargo test -p pythos-core hardware_probe_screen
cargo test -p pythos-core framebuffer font
python -m unittest tests.test_qemu_exit
python scripts/test-hardware-probe.py
cargo build -p pythos-core --target x86_64-unknown-none --features verify
```

Expected: all pass.
