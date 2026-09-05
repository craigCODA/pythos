# USB xHCI Command Ring Driver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in xHCI driver diagnostic that takes the first controlled write/DMA step after the swap-port proof by initializing the controller command/event rings and proving an Enable Slot command completion.

**Architecture:** Keep ADR 0078-0080 probe features read-only unless the new feature is enabled. Add a separate `usb-xhci-command-probe` feature that reuses the discovered AMD/QEMU xHCI BAR mapping and hotplugged-port observation, then performs bounded controller setup with static page-aligned DMA buffers, a polled event ring, and framebuffer/COM1 evidence. HID descriptor parsing, endpoint polling, pointer movement, interrupts, and storage writes remain out of scope.

**Tech Stack:** Rust `no_std` PythCore, existing xHCI probe module, existing PythCore active-address translation for DMA, existing framebuffer text renderer, existing Python QEMU/QMP harness.

**Spec:** `docs/decisions/0080-usb-xhci-swap-port-probe.md` and this plan.

## Global Constraints

- Preserve `usb-xhci-probe`, `usb-xhci-port-probe`, and `usb-xhci-swap-probe` marker contracts unless `usb-xhci-command-probe` is explicitly enabled.
- Do not claim USB HID support, trackpad support, endpoint polling, IRQ-driven input, cursor movement, shell mouse input, or generic USB support from this slice.
- Use volatile MMIO accesses for all xHCI registers and finite polling loops for all controller waits.
- Submit only page-aligned static DMA buffers whose physical addresses are resolved through `memory::virtual::translate_active_address`.
- Keep the diagnostic no-write with respect to storage media.
- Record the new behavior as an ADR and update README/technical evidence boundaries after QEMU acceptance.

---

### Task 1: Command/Event Ring Encoding

**Files:**
- Create: `core/src/usb_xhci_driver.rs`
- Modify: `core/src/main.rs`
- Modify: `core/Cargo.toml`

**Interfaces:**
- Consumes: `usb_xhci_probe::XhciRegisterSnapshot`, `usb_xhci_probe::XhciPortChange`
- Produces: `usb_xhci_driver::XhciCommandProbeResult`, `usb_xhci_driver::command_trb_control(trb_type: u8, cycle: bool) -> u32`, `usb_xhci_driver::scratchpad_buffer_count(hcsparams2: u32) -> u16`

- [x] **Step 1: Write failing Rust tests**

Add tests in `core/src/usb_xhci_driver.rs`:

```rust
#[test]
fn command_trb_control_encodes_type_and_cycle_bit() {
    assert_eq!(command_trb_control(9, true), (9 << 10) | 1);
    assert_eq!(command_trb_control(23, false), 23 << 10);
}

#[test]
fn scratchpad_count_uses_high_and_low_hcsparams2_fields() {
    let hcsparams2 = (0b10101 << 21) | (0b01010 << 27);
    assert_eq!(scratchpad_buffer_count(hcsparams2), 0b10101_01010);
}
```

- [x] **Step 2: Run red test**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`
Expected: FAIL because `usb_xhci_driver` helpers are not implemented.

- [x] **Step 3: Implement minimal pure helpers and feature gate**

Add the `usb-xhci-command-probe` feature depending on `usb-xhci-swap-probe`. Define xHCI TRB type constants, completion-code extraction, scratchpad-count decoding, and typed command-probe errors.

- [x] **Step 4: Run green test**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`
Expected: PASS for the new pure helper tests.

### Task 2: Bounded xHCI Command Ring Diagnostic

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`
- Modify: `core/src/usb_xhci_probe_boot.rs`
- Modify: `core/src/usb_xhci_probe.rs`

**Interfaces:**
- Consumes: `memory::virtual::translate_active_address`, `usb_xhci_probe::XHCI_MMIO_VIRT`, `XhciRegisterSnapshot`, hotplugged port number
- Produces: `usb_xhci_driver::run_command_probe(registers: XhciRegisterSnapshot, port_number: u8) -> Result<XhciCommandProbeResult, XhciDriverError>`

- [x] **Step 1: Write failing Rust tests**

Add tests proving:

```rust
#[test]
fn event_completion_decodes_successful_enable_slot_slot_id() {
    let event = XhciTrb::new(0x1000, 0, 1 << 24, (33 << 10) | (7 << 24) | 1);
    assert_eq!(event.completion_code(), 1);
    assert_eq!(event.slot_id(), 7);
    assert_eq!(event.trb_type(), 33);
}
```

- [x] **Step 2: Run red test**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`
Expected: FAIL because command completion decoding is not implemented.

- [x] **Step 3: Implement bounded driver path**

Add page-aligned static buffers for DCBAA, command ring, event ring, ERST, and scratchpad pointers. In the feature path, stop and reset the controller, wait for reset/CNR completion, configure DCBAAP/CRCR/ERST/CONFIG, start the controller, optionally reset the connected root port, enqueue an Enable Slot command, ring host doorbell 0, poll event ring entry 0 for a command-completion event, validate completion code, and report the returned slot id. All waits must have finite limits.

- [x] **Step 4: Run green test**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`
Expected: PASS for helper and event decode tests.

### Task 3: Framebuffer, QEMU Harness, and Docs

**Files:**
- Modify: `core/src/usb_xhci_probe_screen.rs`
- Create: `scripts/test-usb-xhci-command-probe.py`
- Create: `docs/decisions/0081-usb-xhci-command-ring-driver.md`
- Modify: `README.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Modify: `docs/evidence/index.html`

**Interfaces:**
- Consumes: `XhciCommandProbeResult`
- Produces: `PYTHOS:CORE:USB_XHCI_PROBE:XHCI_COMMAND_RING_READY`, `PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENABLE_SLOT_READY`, `USB_XHCI_COMMAND_PROBE_TEST_OK`

- [x] **Step 1: Write harness expecting new markers**

The harness builds with `--features usb-xhci-command-probe`, boots with `qemu-xhci`, simulates boot USB detach and later mouse hotplug exactly like ADR 0080, then requires ordered markers through command ring setup and Enable Slot completion.

- [x] **Step 2: Run harness after implementation**

Run: `py -3 scripts\test-usb-xhci-command-probe.py`
Observed: PASS with `USB_XHCI_COMMAND_PROBE_TEST_OK` and `QEMU_OUTCOME success`.

- [x] **Step 3: Implement screen and docs**

Render a final `xhci cmd` panel that includes the selected port, completion code, returned slot id, and no-disk-writes status. Document the QEMU-only driver-layer proof and explicitly state remaining unproven HID/cursor work.

- [x] **Step 4: Run full verification**

Run:

```powershell
cargo fmt --check
cargo test -p pythos-core --bin pythcore usb_xhci_probe -- --nocapture
cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture
py -3 scripts\test-usb-xhci-probe.py
py -3 scripts\test-usb-xhci-port-probe.py
py -3 scripts\test-usb-xhci-swap-probe.py
py -3 scripts\test-usb-xhci-command-probe.py
py -3 -m compileall scripts
git diff --check
```

Expected: all commands exit 0, each QEMU script reports `QEMU_OUTCOME success`, and the new script reports `USB_XHCI_COMMAND_PROBE_TEST_OK`.
