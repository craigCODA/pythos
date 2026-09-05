# USB xHCI Address Device Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Advance the physical USB mouse bring-up from command-ring/slot allocation to a bounded xHCI Address Device diagnostic.

**Architecture:** Add a new opt-in `usb-xhci-address-probe` feature that depends on the already-proven command-ring probe. Reuse the selected hotplugged port, static DMA state, command ring, and event ring, then add page-aligned input/output device contexts plus a default-control transfer ring and issue one Address Device command. The screen reports completion code, assigned USB address, output slot state, EP0 state, port speed, context size, and preserves the no-disk-writes boundary.

**Tech Stack:** Rust `no_std` PythCore, xHCI MMIO/DMA, existing framebuffer probe renderer, existing Python QEMU/QMP harness, PowerShell USB deployment.

**Spec:** `docs/decisions/0081-usb-xhci-command-ring-driver.md` and Intel xHCI Requirements Specification Rev. 1.22 sections 4.3.3, 4.6.5, 6.1, 6.2.2, 6.2.3, and 6.4.3.4.

## Global Constraints

- Do not claim HID input, pointer movement, USB descriptor parsing, endpoint configuration, interrupts, or shell mouse support from this slice.
- Keep all xHCI register access volatile and all waits finite.
- Submit only static page-aligned DMA buffers whose physical addresses are resolved through the active PythCore page tables.
- Support both 32-byte and 64-byte xHCI context sizes from HCCPARAMS1.CSZ.
- Preserve the existing `usb-xhci-command-probe` marker contract unless `usb-xhci-address-probe` is explicitly enabled.
- Preserve `NO_DISK_WRITES`.
- Re-identify `P:` before deployment and copy only the built ESP files.

---

### Task 1: Address Context Helpers

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`
- Test: `core/src/usb_xhci_driver.rs`

**Interfaces:**
- Consumes: `XhciDmaState`, `XhciRegisterSnapshot.hccparams1`, selected root-hub `port_number`, and reset `PORTSC`
- Produces: `context_size_from_hccparams1(hccparams1: u32) -> usize`, `default_control_max_packet_size(portsc: u32) -> Result<u16, XhciDriverError>`, and `prepare_address_device_context(...) -> Result<XhciAddressContextSnapshot, XhciDriverError>`

- [x] **Step 1: Write failing tests**

Add tests proving 32-byte and 64-byte input contexts place A0/A1 flags, root-port slot metadata, EP0 control metadata, transfer-ring dequeue pointer, and DCBAA slot pointer in the expected locations.

- [x] **Step 2: Run red test**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`

Expected: FAIL because the address-device helpers and context buffers do not exist.

- [x] **Step 3: Implement static DMA contexts**

Add page-aligned input context, output device context, and default-control transfer ring storage. Translate and validate the physical addresses, zero the structures, initialize A0/A1, Slot Context, Endpoint 0 Context, and link the control ring.

- [x] **Step 4: Run green test**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`

Expected: PASS for the driver helper tests.

### Task 2: Address Device Command and Screen

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`
- Modify: `core/src/usb_xhci_probe_boot.rs`
- Modify: `core/src/usb_xhci_probe_screen.rs`
- Modify: `core/Cargo.toml`

**Interfaces:**
- Consumes: `run_command_probe` setup sequence and returned Slot ID
- Produces: `run_address_probe(registers: XhciRegisterSnapshot, port_number: u8) -> Result<XhciAddressProbeResult, XhciDriverError>` and `render_address_probe(...)`

- [x] **Step 1: Write failing tests**

Add tests proving Address Device TRB encoding and the final framebuffer lines:

```rust
assert_eq!(screen.line(1), Some("xhci addr"));
assert_eq!(screen.line(10), Some("addr cc 01"));
```

- [x] **Step 2: Run red test**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_address_probe_result_for_no_serial_capture -- --nocapture`

Expected: FAIL because address rendering and result types are not implemented.

- [x] **Step 3: Implement address command path**

Issue the Address Device command after Enable Slot. Ring doorbell 0, poll the command-completion event, record the completion code, and read the output Slot and EP0 context fields after completion.

- [x] **Step 4: Run green test**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_address_probe_result_for_no_serial_capture -- --nocapture`

Expected: PASS.

### Task 3: Harness, Docs, and Deployment

**Files:**
- Create: `scripts/test-usb-xhci-address-probe.py`
- Modify: `README.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Modify: `docs/decisions/0081-usb-xhci-command-ring-driver.md`
- Modify: `docs/evidence/index.html`
- Modify: `D:\PythOS-Workspace\CURRENT-STATE.md`

**Interfaces:**
- Consumes: `usb-xhci-address-probe` build
- Produces: `USB_XHCI_ADDRESS_PROBE_TEST_OK`, refreshed USB deployment hash/readback, and a precise next physical test instruction

- [x] **Step 1: Write harness marker expectations**

Build the feature, boot QEMU with xHCI and a hotplugged USB mouse, require command-ring markers plus `XHCI_ADDRESS_CONTEXT_READY`, `XHCI_ADDRESS_DEVICE_CC=`, `XHCI_DEVICE_ADDRESS=`, `XHCI_SLOT_STATE=`, `XHCI_EP0_STATE=`, and `XHCI_ADDRESS_DEVICE_READY`.

- [x] **Step 2: Run QEMU acceptance**

Run: `py -3 scripts\test-usb-xhci-address-probe.py`

Expected: PASS with `USB_XHCI_ADDRESS_PROBE_TEST_OK` and `QEMU_OUTCOME success`.

- [x] **Step 3: Update docs and current state**

Record the physical command-ring success photo and the new address-device scope boundary.

- [x] **Step 4: Re-identify and deploy to USB**

Verify `P:` is the expected Lexar `PYTHOS_ESP` removable target, copy only `image\esp` files, and read back hashes.

## Observed Evidence

- RED: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`
  failed before the address helpers, context buffers, result type, TRB encoder,
  and renderer existed.
- GREEN: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`
  passed with the address helper tests included.
- GREEN: `cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_address_probe -- --nocapture`
  passed for the address success and error framebuffer screens.
- QEMU: `py -3 scripts\test-usb-xhci-address-probe.py` passed with
  `USB_XHCI_ADDRESS_PROBE_TEST_OK` and `QEMU_OUTCOME success`.
- QEMU address result: Address Device CC `0x1`, device address `0x1`, slot
  state `0x2`, EP0 state `0x1`, context size `0x20`, port speed `0x3`, max
  packet size `0x40`, and `NO_DISK_WRITES`.
- Deployment: `P:` re-identified as Disk 2, Lexar D70E USB, serial
  `1026R51254700477`, MBR active FAT32 `PYTHOS_ESP`, not Windows boot/system,
  and writable.
- Deployment method: copied only current `image\esp` files to `P:\` with no
  format and no delete pass; preserved root files remained on the drive.
- Readback: `USB_XHCI_ADDRESS_VERIFY_OK files:8 bytes:3977256`, deployed
  `P:\PYTHOS\PYTHCORE.ELF` SHA-256
  `E666859BFEE4FE6162690F3D8860E24992492441F859AEE6C8F4FC14DDBC3D53`.
- Physical result: operator-provided photo
  `C:\Users\NeverAMoment\Desktop\Screenshot 2026-08-31 225344.png`, retained as
  `docs\evidence\2026-09-01-physical-usb-xhci-address-device-success.png`,
  SHA-256 `8A4D2D6D8F74AEE88D2B535F4447CBDC338E590944F0D8130E7B6FD6476A6D5A`,
  shows `xhci addr`, `no disk writes`, BDF `00 10 00`, vendor/device
  `1022 7914`, port `05`, slot `01`, No-op CC `01`, Enable Slot CC `01`,
  Address Device CC `01`, device address `01`, slot state `02`, EP0 state
  `01`, speed `02`, context size `32`, max packet size `0008`, `PORTSC`
  `00220A03`, and scratchpad count `08`.
- Next slice: first EP0 GET_DESCRIPTOR request and descriptor-byte report; no
  HID/cursor/trackpad claim yet.
