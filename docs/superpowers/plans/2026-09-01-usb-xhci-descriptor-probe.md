# USB xHCI Descriptor Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Advance the physically accepted xHCI Address Device diagnostic to one bounded EP0 `GET_DESCRIPTOR(Device)` read.

**Architecture:** Add an opt-in `usb-xhci-descriptor-probe` feature that depends on `usb-xhci-address-probe`. Reuse the existing xHCI command/event rings, addressed slot, EP0 context, and static transfer ring, then queue one setup/data/status control-transfer TD for an 18-byte device descriptor. The framebuffer and COM1 report the transfer completion code plus parsed device descriptor fields while preserving the no-disk-writes boundary.

**Tech Stack:** Rust `no_std` PythCore, xHCI MMIO/DMA, existing framebuffer probe renderer, existing Python QEMU/QMP harness, PowerShell USB deployment.

**Spec:** `docs/decisions/0082-usb-xhci-address-device-probe.md` and Intel xHCI Requirements Specification Rev. 1.22 sections 4.11.2.2, 6.4.1.2-6.4.1.4, 6.4.2.1, and 6.4.5.

## Global Constraints

- Do not claim HID input, pointer movement, non-control endpoint configuration, interrupts, shell mouse support, or trackpad support from this slice.
- Keep all xHCI register access volatile and all waits finite.
- Submit only static page-aligned DMA buffers whose physical addresses are resolved through the active PythCore page tables.
- Keep one in-flight EP0 control transfer.
- Preserve the existing address-probe marker contract unless `usb-xhci-descriptor-probe` is explicitly enabled.
- Preserve `NO_DISK_WRITES`.
- Re-identify `P:` before deployment and copy only built ESP files.

---

### Task 1: Descriptor TRB and Parser Tests

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`

**Interfaces:**
- Consumes: `XhciDmaState.control_ring_phys`, static descriptor DMA buffer, and existing `XhciTrb`
- Produces: `device_descriptor_setup_trb(cycle: bool) -> XhciTrb`, `device_descriptor_data_trb(descriptor_phys: u64, cycle: bool) -> XhciTrb`, `control_status_stage_trb(cycle: bool) -> XhciTrb`, and `parse_device_descriptor(bytes: &[u8; 18]) -> XhciDeviceDescriptorSnapshot`

- [x] **Step 1: Write failing tests**

Add tests proving the setup/data/status TRBs encode one `GET_DESCRIPTOR(Device)` control transfer and that the parser extracts length, type, USB BCD, class/subclass/protocol, EP0 max packet size, vendor ID, product ID, device BCD, string indexes, and configuration count.

- [x] **Step 2: Run red tests**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`

Expected: FAIL because descriptor TRB helpers and descriptor parser do not exist.

- [x] **Step 3: Implement minimal helpers**

Add xHCI transfer TRB constants, one static descriptor DMA page, descriptor-buffer physical translation, volatile zero/read helpers for that page, and the descriptor parser.

- [x] **Step 4: Run green tests**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`

Result: PASS, 18 tests, including descriptor TRB encoding and parser coverage.

### Task 2: EP0 Descriptor Transfer and Screen

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`
- Modify: `core/src/usb_xhci_probe_boot.rs`
- Modify: `core/src/usb_xhci_probe_screen.rs`
- Modify: `core/Cargo.toml`

**Interfaces:**
- Consumes: `run_address_probe` setup sequence and returned addressed EP0 state
- Produces: `run_descriptor_probe(registers: XhciRegisterSnapshot, port_number: u8) -> Result<XhciDescriptorProbeResult, XhciDriverError>` and `render_descriptor_probe(...)`

- [x] **Step 1: Write failing screen tests**

Add tests proving the framebuffer lines include `xhci desc`, the address completion code, descriptor completion code, descriptor length/type, USB BCD, class/sub/protocol, MPS, VID/PID, and configuration count.

- [x] **Step 2: Run red screen tests**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_descriptor_probe_result_for_no_serial_capture -- --nocapture`

Expected: FAIL because descriptor result rendering is not implemented.

- [x] **Step 3: Implement descriptor probe path**

After Address Device succeeds, queue setup, data, and status TRBs on EP0, ring doorbell target endpoint `1` for the selected slot, poll for the transfer event with finite timeout, observe non-success completion without panicking, parse the descriptor buffer, and emit serial markers.

- [x] **Step 4: Run green screen tests**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_descriptor_probe_result_for_no_serial_capture -- --nocapture`

Result: PASS for descriptor success and error framebuffer panels.

### Task 3: Harness, Docs, and Deployment

**Files:**
- Create: `scripts/test-usb-xhci-descriptor-probe.py`
- Create: `docs/decisions/0083-usb-xhci-device-descriptor-probe.md`
- Modify: `README.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Modify: `docs/evidence/index.html`
- Modify: `D:\PythOS-Workspace\CURRENT-STATE.md`

**Interfaces:**
- Consumes: `usb-xhci-descriptor-probe` build
- Produces: `USB_XHCI_DESCRIPTOR_PROBE_TEST_OK`, refreshed USB deployment hash/readback, and a precise physical test instruction

- [x] **Step 1: Write QEMU harness marker expectations**

Build with `--features usb-xhci-descriptor-probe`, boot QEMU with xHCI, detach the simulated boot USB, hotplug the QEMU USB mouse, then require address markers plus descriptor markers through `XHCI_DESCRIPTOR_READY`.

- [x] **Step 2: Run QEMU acceptance**

Run: `py -3 scripts\test-usb-xhci-descriptor-probe.py`

Result: PASS with `USB_XHCI_DESCRIPTOR_PROBE_TEST_OK` and `QEMU_OUTCOME success`; QEMU descriptor transfer CC `1`, length `18`, type `1`, USB BCD `0200`, MPS0 `64`, VID/PID `0627:0001`, configuration count `1`.

- [x] **Step 3: Update docs and current state**

Record the Linux field-kit evidence, the descriptor-probe scope boundary, the QEMU evidence, and the physical test instructions.

- [x] **Step 4: Re-identify and deploy to USB**

Verify `P:` is the expected Lexar `PYTHOS_ESP` target, copy only `image\esp` files, preserve field-kit reports, and read back hashes.

Result: PASS. `P:` was re-identified as Disk 2 Lexar D70E USB, serial `1026R51254700477`, MBR active FAT32 `PYTHOS_ESP`, not Windows boot/system. `chkdsk P:` reported no filesystem problems. Deployment copied only 8 `image\esp` files with no format and no delete pass. Readback reported `USB_XHCI_DESCRIPTOR_VERIFY_OK files:8 bytes:3996680`; deployed `P:\PYTHOS\PYTHCORE.ELF` SHA-256 `CFE2381F38DA91E11A31F021B315B4B12C030DB3F296C8B591BA6DF0A5289924`.

Physical result: PASS with photo-backed evidence. The preserved frame `docs/evidence/2026-09-01-physical-usb-xhci-device-descriptor-success.png` shows `xhci desc`, `no disk writes`, BDF `00 10 00`, vendor/device `1022 7914`, port `05`, slot `01`, Address Device CC `01`, descriptor CC `01`, length `12`, type `01`, USB BCD `0200`, device BCD `0100`, class/sub/protocol `00 00 00`, MPS0 `008`, configuration count `01`, VID/PID `413C 301A`, and scratchpad count `08`. The frame SHA-256 is `4204994560727C63A8F631A05CCECFA68C3FC20189E12A2834E621327FDA61B6`.
