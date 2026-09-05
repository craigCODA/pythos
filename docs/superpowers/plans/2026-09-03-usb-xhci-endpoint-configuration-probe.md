# USB xHCI Endpoint Configuration Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the physically accepted configuration-descriptor diagnostic with one bounded, opt-in transition that configures the discovered interrupt-IN endpoint in xHCI, sends USB `SET_CONFIGURATION(1)`, proves both completions, and stops before submitting an interrupt transfer.

**Architecture:** Add `usb-xhci-endpoint-configuration-probe` above `usb-xhci-configuration-probe`. Reuse the addressed slot, EP0 ring, command ring, event ring, and parsed endpoint metadata. Allocate one separate static, page-aligned interrupt transfer ring; build a fresh Input Context with Add Context flags A0 and A3 for endpoint `0x81`; issue Configure Endpoint at command-ring index 3; only after it succeeds, submit the no-data `SET_CONFIGURATION` TD on EP0. Report the output slot/endpoint states, then halt without ringing the interrupt endpoint doorbell.

**Tech Stack:** Rust `no_std` PythCore, static xHCI DMA storage, volatile MMIO/DMA access, COM1 marker oracle, framebuffer diagnostic text, Python QEMU acceptance harness.

**Spec:** `docs/decisions/0084-usb-xhci-configuration-descriptor-probe.md`, USB 2.0 Chapter 9, and xHCI Requirements Specification Revision 1.2b sections 4.3.5, 4.5.4.2, 4.6.6, 6.2.2.2, 6.2.3.2, 6.2.5, and 6.4.3.5.

## Global Constraints

- Keep the feature opt-in and dependent on `usb-xhci-configuration-probe`.
- Configure Endpoint must complete successfully before `SET_CONFIGURATION`; do not send the device request after a failed command.
- Support the discovered root-port interrupt-IN endpoint only. Reject endpoint zero, OUT endpoints, non-interrupt attributes, invalid packet sizes/intervals, and unsupported speeds with typed errors.
- Preserve one in-flight operation, finite polling, static page-aligned physically translated DMA, volatile access, and complete unsafe invariants.
- Create the interrupt transfer ring but do not enqueue a Normal TRB or ring its doorbell.
- Do not read a HID report descriptor, select HID protocol, poll reports, move a cursor, enter the shell, enable interrupts, support the built-in trackpad, or touch storage.
- Preserve all earlier probe markers, default boot behavior, and `PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES`.
- QEMU evidence is not physical-hardware acceptance.

---

### Task 1: Pure endpoint and control-transfer contracts

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`

- [x] Write failing literal tests for endpoint-address-to-DCI mapping (`0x81 -> 3`), FS/LS and HS interval encoding, endpoint-context words, Configure Endpoint command TRB encoding, and the no-data `SET_CONFIGURATION(1)` Setup/Status TRBs.
- [x] Run `cargo test -p pythos-core --bin pythcore usb_xhci_driver::tests::endpoint_configuration -- --nocapture` and require RED for missing production behavior.
- [x] Implement only the pure encoders/validators and typed errors required by those tests.
- [x] Re-run the focused tests and require GREEN.

### Task 2: Static DMA and configured-state sequence

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`
- Modify: `core/Cargo.toml`

- [x] Write failing tests proving the separate interrupt ring is page-aligned, linked, not shared with EP0, and referenced by DCI 3 in both 32-byte and 64-byte Input Context layouts.
- [x] Write a failing control-ring test proving the fourth no-data TD occupies indices 9 and 10 without overwriting the Link TRB.
- [x] Add the static ring, physical translation/alignment validation, context preparation, Configure Endpoint command at command index 3, and bounded `SET_CONFIGURATION` submission.
- [x] Emit stable completion/state markers and return a typed endpoint-configuration result. Require Configure Endpoint success before the USB request.
- [x] Add `usb-xhci-endpoint-configuration-probe = ["usb-xhci-configuration-probe"]` and build both the previous and new feature.

### Task 3: Framebuffer and boot selection

**Files:**
- Modify: `core/src/usb_xhci_probe_boot.rs`
- Modify: `core/src/usb_xhci_probe_screen.rs`

- [x] Write failing success/error screen tests for the new title, endpoint address/DCI/interval, command and SET_CONFIGURATION completion codes, configured slot/endpoint states, typed stage errors, and `no disk writes`.
- [x] Implement the new panel and feature precedence while leaving prior feature-only branches unchanged.
- [x] Re-run focused screen tests and both old/new target builds.

### Task 4: QEMU acceptance

**Files:**
- Create: `scripts/test-usb-xhci-endpoint-configuration-probe.py`

- [x] Clone only the proven detach/hotplug mechanics from the ADR 0084 harness.
- [x] Require every earlier descriptor marker followed by endpoint-context readiness, Configure Endpoint success, `SET_CONFIGURATION` success, configured output states, `NO_DISK_WRITES`, `QEMU_OUTCOME success`, and `USB_XHCI_ENDPOINT_CONFIGURATION_PROBE_TEST_OK`.
- [x] Reject driver errors and any interrupt-report/cursor markers.
- [x] Run the harness to GREEN and re-run the ADR 0084 harness unchanged.

### Task 5: Evidence, regression, and checkpoint

**Files:**
- Create: `docs/decisions/0085-usb-xhci-endpoint-configuration-probe.md`
- Modify: `README.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Modify: `D:\PythOS-Workspace\CURRENT-STATE.md`

- [x] Record the exact ordering, bounded DMA ownership, typed failures, markers, no-write boundary, and QEMU-only status in ADR 0085.
- [x] Run formatting, diff checks, all PythCore tests, old/new cross-target builds, the ADR 0084 and ADR 0085 QEMU harnesses, and `scripts/test-boot.py`. Report unrelated known baseline failures honestly if the broader Python suite is sampled.
- [x] Update project/external state only from fresh evidence.
- [x] Commit normally, push the exact branch, verify local/remote equality, and create/verify a Git bundle checkpoint.
- [x] Stop before USB deployment; re-identify the removable target and obtain deployment approval in a later turn.

## Verification Matrix

- Host tests: pure USB/xHCI encodings, validation, 32/64-byte context layout, separate ring ownership, control-ring capacity, framebuffer success/error panels.
- Cross-target builds: prior configuration descriptor feature and new endpoint-configuration feature.
- QEMU: Configure Endpoint then `SET_CONFIGURATION`, configured output state, no interrupt TD, no disk writes, deterministic success outcome.
- Regression: all PythCore host tests, ADR 0084 configuration harness, standard boot harness.
- Physical hardware: explicitly pending a later re-identified USB deployment and Lenovo/Dell-PixArt boot.

## Non-Goals

- HID report descriptor or class requests
- Interrupt-IN Normal TRB submission or polling
- Mouse report parsing
- Cursor, shell, or UI input
- IRQ/MSI/MSI-X input handling
- Built-in I2C trackpad support
- Storage writes
