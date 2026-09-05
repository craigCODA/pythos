# USB xHCI Configuration Descriptor Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the accepted xHCI device-descriptor diagnostic with one bounded, opt-in configuration-descriptor read that identifies the first interface and interrupt-IN endpoint without configuring or polling it.

**Architecture:** Add `usb-xhci-configuration-probe` on top of `usb-xhci-descriptor-probe`. Reuse the addressed slot, EP0 context, static descriptor DMA page, transfer ring, event ring, and finite polling path for a 9-byte configuration-header read followed by an exact `wTotalLength` read capped at 256 bytes. Parse only standard USB configuration, interface, and endpoint descriptors; render and emit their metadata while preserving `NO_DISK_WRITES` and every earlier feature's marker contract.

**Tech Stack:** Rust `no_std` PythCore, static xHCI DMA storage, COM1 marker oracle, framebuffer diagnostic text, Python QEMU acceptance harness.

**Spec:** `docs/decisions/0083-usb-xhci-device-descriptor-probe.md` Consequences plus the owner-approved 2026-09-02 bounded design recorded in `D:\PythOS-Workspace\CURRENT-STATE.md`.

## Global Constraints

- Keep the feature opt-in and dependent on `usb-xhci-descriptor-probe`.
- Do not issue `SET_CONFIGURATION` or an xHCI Configure Endpoint command.
- Do not parse HID report descriptors, poll endpoints, move a cursor, enter the shell, or claim trackpad support.
- Keep the configuration read at or below 256 bytes and reject larger or malformed descriptor trees with typed errors.
- Use only the static page-aligned DMA page and translated physical address already owned by PythCore.
- Use volatile DMA reads, finite polling, one in-flight control transfer, and fully documented unsafe invariants.
- Preserve existing register, port, swap, command, address, and device-descriptor marker contracts.
- Preserve `PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES` and the default boot path.
- QEMU evidence is not physical-hardware acceptance.

---

### Task 1: Configuration TRBs and bounded descriptor parser

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`

**Interfaces:**
- Consumes: the existing `XhciTrb`, setup/data/status TRB encoding, static descriptor DMA page, and `XhciDriverError`.
- Produces: `configuration_descriptor_setup_trb(length: u16, cycle: bool) -> XhciTrb`, `configuration_descriptor_data_trb(descriptor_phys: u64, length: u16, cycle: bool) -> XhciTrb`, `parse_configuration_descriptor_header(bytes: &[u8; 9]) -> Result<XhciConfigurationDescriptorHeader, XhciDriverError>`, and `parse_configuration_descriptor(bytes: &[u8], total_length: usize) -> Result<XhciConfigurationDescriptorSnapshot, XhciDriverError>`.

- [x] **Step 1: Write failing TRB tests**

Add literal assertions proving the 9-byte setup packet is `80 06 00 02 00 00 09 00`, the exact-length setup encodes `wLength`, and the data TRB uses the supplied physical address and length with IN direction.

- [x] **Step 2: Run the focused test and verify RED**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver::tests::configuration_descriptor -- --nocapture`

Expected: compilation fails because the configuration TRB helpers do not exist.

- [x] **Step 3: Implement the minimal TRB helpers**

Encode `GET_DESCRIPTOR(Configuration)` as immediate setup data and keep IOC only on the status stage.

- [x] **Step 4: Run the focused TRB tests and verify GREEN**

Run the same focused command and require all new TRB tests to pass.

- [x] **Step 5: Write failing parser tests**

Use a hand-authored 34-byte mouse configuration fixture containing configuration, interface, HID, and interrupt-IN endpoint descriptors. Assert total length `34`, configuration value `1`, interface `0`, class/subclass/protocol `03/01/02`, endpoint `0x81`, attributes `0x03`, max packet `4`, and interval `10`. Add independent rejection tests for wrong header type, total length above 256, zero descriptor length, descriptor overrun, missing interface, and missing interrupt-IN endpoint.

- [x] **Step 6: Run parser tests and verify RED**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver::tests::configuration_descriptor -- --nocapture`

Expected: compilation fails because the header/snapshot types and parser functions do not exist.

- [x] **Step 7: Implement the bounded parser and typed errors**

Add `XhciConfigurationDescriptorHeader`, `XhciConfigurationDescriptorSnapshot`, and exact `XhciDriverError` variants for invalid header, oversized total length, malformed descriptor chain, missing interface, and missing interrupt-IN endpoint. Walk descriptors using each descriptor's `bLength`; never index beyond validated `total_length`.

- [x] **Step 8: Run parser tests and verify GREEN**

Run the focused command and require all configuration TRB/parser tests to pass.

- [x] **Step 9: Commit the tested TRB/parser layer**

Run: `git add core/src/usb_xhci_driver.rs docs/superpowers/plans/2026-09-02-usb-xhci-configuration-probe.md && git commit -m "feat(usb): parse bounded configuration descriptors"`

### Task 2: Sequential EP0 configuration reads

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`
- Modify: `core/Cargo.toml`

**Interfaces:**
- Consumes: `XhciDescriptorProbeResult`, `XhciCommandProbeState`, the control/event rings, `poll_transfer_completion`, and the static descriptor DMA page.
- Produces: `XhciConfigurationProbeResult` and `run_configuration_probe(registers: XhciRegisterSnapshot, port_number: u8) -> Result<XhciConfigurationProbeResult, XhciDriverError>`.

- [x] **Step 1: Write failing control-ring progression tests**

Add a pure test proving three sequential control TDs occupy TRB ranges `0..=2`, `3..=5`, and `6..=8`, and that a start index unable to fit three TRBs returns `ControlRingExhausted`.

- [x] **Step 2: Run the focused test and verify RED**

Run the focused driver test command and require failure because the bounded control-ring allocator does not exist.

- [x] **Step 3: Implement sequential control-read submission**

Refactor device-descriptor submission through a bounded `submit_ep0_control_read` helper that advances a caller-owned control-ring index. Keep cycle state `true` because this slice never wraps the 16-entry ring. Read the device descriptor at indices `0..=2`, the 9-byte configuration header at `3..=5`, and the exact full descriptor at `6..=8`.

- [x] **Step 4: Implement the configuration probe result path**

Require successful Address Device and device-descriptor completion before the configuration reads. Validate `wTotalLength` before issuing the second configuration request. Emit header/full completion codes and parsed fields, then emit `XHCI_CONFIGURATION_READY` only after parsing succeeds.

- [x] **Step 5: Add the feature declaration**

Add `usb-xhci-configuration-probe = ["usb-xhci-descriptor-probe"]` with an explicit non-goal comment.

- [x] **Step 6: Run focused tests and feature build**

Run:

```powershell
cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture
cargo build -p pythos-core --target x86_64-unknown-none --features usb-xhci-configuration-probe
```

Require the driver tests and cross-target build to pass.

- [x] **Step 7: Commit the sequential EP0 probe layer**

Run: `git add core/Cargo.toml core/src/usb_xhci_driver.rs docs/superpowers/plans/2026-09-02-usb-xhci-configuration-probe.md && git commit -m "feat(usb): read xHCI configuration descriptors"`

### Task 3: Boot selection and framebuffer result

**Files:**
- Modify: `core/src/usb_xhci_probe_boot.rs`
- Modify: `core/src/usb_xhci_probe_screen.rs`

**Interfaces:**
- Consumes: `run_configuration_probe`, `XhciConfigurationProbeResult`, and existing descriptor/swap fallback rendering.
- Produces: `build_configuration_probe_screen(...)`, `render_configuration_probe(...)`, and configuration-feature precedence over the device-descriptor-only path.

- [x] **Step 1: Write failing framebuffer tests**

Add a literal result fixture and assert the panel contains `xhci cfg`, address/device/config completion codes, total length, configuration/interface counts, `03 01 02`, endpoint `81`, attributes `03`, MPS `0004`, interval `010`, scratchpad count, and `no disk writes`. Add an error-panel test using a typed configuration error.

- [x] **Step 2: Run the screen tests and verify RED**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_configuration_probe -- --nocapture`

Expected: compilation fails because configuration screen builders do not exist.

- [x] **Step 3: Implement framebuffer rendering**

Use the existing fixed diagnostic-line builder and do not add dynamic allocation.

- [x] **Step 4: Run screen tests and verify GREEN**

Run the same focused command and require the success/error panel tests to pass.

- [x] **Step 5: Wire boot feature precedence**

When `usb-xhci-configuration-probe` is enabled, run/render it instead of the device-descriptor-only path. Keep all previous feature-only branches unchanged when configuration probing is disabled.

- [x] **Step 6: Build both old and new features**

Run:

```powershell
cargo build -p pythos-core --target x86_64-unknown-none --features usb-xhci-descriptor-probe
cargo build -p pythos-core --target x86_64-unknown-none --features usb-xhci-configuration-probe
```

Require both builds to pass.

- [x] **Step 7: Commit boot and framebuffer integration**

Run: `git add core/src/usb_xhci_probe_boot.rs core/src/usb_xhci_probe_screen.rs docs/superpowers/plans/2026-09-02-usb-xhci-configuration-probe.md && git commit -m "feat(usb): render configuration probe results"`

### Task 4: QEMU acceptance harness

**Files:**
- Create: `scripts/test-usb-xhci-configuration-probe.py`

**Interfaces:**
- Consumes: the existing descriptor harness flow, QEMU xHCI USB-storage detach, and QEMU USB-mouse hotplug.
- Produces: stable terminal line `USB_XHCI_CONFIGURATION_PROBE_TEST_OK`.

- [x] **Step 1: Write the harness before the feature is complete**

Require every ADR 0083 marker plus configuration header/full completion codes, total length `34`, configuration value `1`, interface count `1`, interface class/subclass/protocol `03/01/02`, endpoint `0x81`, attributes `0x03`, max packet `4`, live QEMU interval `7`, `XHCI_CONFIGURATION_READY`, `NO_DISK_WRITES`, and final probe readiness.

- [x] **Step 2: Run the harness and verify RED**

Run: `py -3 scripts\test-usb-xhci-configuration-probe.py`

Expected: failure on missing feature/markers before boot integration is complete.

- [x] **Step 3: Complete only the minimal boot/marker integration required by the harness**

Do not add `SET_CONFIGURATION`, Configure Endpoint, HID parsing, or polling.

- [x] **Step 4: Run the harness and verify GREEN**

Run the same command and require `QEMU_OUTCOME success` plus `USB_XHCI_CONFIGURATION_PROBE_TEST_OK`.

- [x] **Step 5: Commit the QEMU acceptance harness**

Run: `git add scripts/test-usb-xhci-configuration-probe.py docs/superpowers/plans/2026-09-02-usb-xhci-configuration-probe.md && git commit -m "test(usb): accept xHCI configuration probe"`

### Task 5: ADR, project state, and regression verification

**Files:**
- Create: `docs/decisions/0084-usb-xhci-configuration-descriptor-probe.md`
- Modify: `README.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Modify: `docs/evidence/index.html`
- Modify: `D:\PythOS-Workspace\CURRENT-STATE.md`

**Interfaces:**
- Consumes: fresh test output and exact QEMU markers from Tasks 1-4.
- Produces: an honest QEMU-only state record and physical-deployment handoff.

- [x] **Step 1: Record ADR 0084 and QEMU evidence**

Document the two-stage bounded read, 256-byte cap, typed failures, exact markers, no-write boundary, and explicit physical-pending status.

- [x] **Step 2: Update project/state documentation**

Record that configuration/interface/endpoint metadata is discovered but the device is not configured and the endpoint is not polled. Set the next action to verified USB deployment and physical configuration-descriptor acceptance, not HID input.

- [x] **Step 3: Run formatting, static, focused, and regression checks**

Run:

```powershell
cargo fmt --check
git diff --check
py -3 -m py_compile scripts\test-usb-xhci-configuration-probe.py
cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture
cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_configuration_probe -- --nocapture
py -3 scripts\test-usb-xhci-descriptor-probe.py
py -3 scripts\test-usb-xhci-configuration-probe.py
py -3 scripts\test-boot.py
```

Require success markers and `QEMU_OUTCOME success` from each QEMU harness. Record any unrelated baseline failures without mislabeling them as configuration-probe regressions.

- [x] **Step 4: Stop before physical deployment**

Report the QEMU-accepted image hash and ask the owner to insert the USB. Re-identify the exact disk, volume, bus, partition flags, and filesystem before any copy. Do not deploy or claim physical acceptance in this task.

- [x] **Step 5: Commit the QEMU-only acceptance record**

Run: `git add README.md docs/TECHNICAL-OVERVIEW.md docs/decisions/0084-usb-xhci-configuration-descriptor-probe.md docs/evidence/index.html docs/superpowers/plans/2026-09-02-usb-xhci-configuration-probe.md && git commit -m "docs(usb): record configuration probe acceptance"`

## Verification Matrix

- Host tests: configuration TRB encoding, bounded descriptor parsing, control-ring progression, framebuffer success/error panels.
- QEMU no-write probe: device descriptor plus configuration/interface/interrupt-IN endpoint metadata, `NO_DISK_WRITES`, deterministic success outcome.
- Existing descriptor probe: unchanged marker contract and success.
- Existing verification boot: `BOOT_TEST_OK` and `QEMU_OUTCOME success`.
- Physical hardware: explicitly pending until separately authorized deployment and operator evidence.

## Non-Goals

- `SET_CONFIGURATION`
- xHCI Configure Endpoint
- HID or report-descriptor parsing
- Interrupt or polled mouse reports
- Cursor or shell input
- Built-in I2C trackpad support
- Storage writes
- Generic USB support claims
