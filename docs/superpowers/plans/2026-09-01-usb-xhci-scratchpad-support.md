# USB xHCI Scratchpad Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Advance ADR 0081 past the physical `err 00000006` failure by supporting xHCI scratchpad buffers in the opt-in command-ring diagnostic.

**Architecture:** Keep the existing `usb-xhci-command-probe` path opt-in and single-controller. Decode `HCSPARAMS2` scratchpad count, allocate static page-aligned scratchpad page storage plus a static scratchpad pointer array, write the pointer array into `DCBAA[0]`, and keep the existing No-op/Enable Slot command path unchanged.

**Tech Stack:** Rust `no_std` PythCore, xHCI MMIO, static DMA buffers, PowerShell USB deployment.

**Spec:** `docs/decisions/0081-usb-xhci-command-ring-driver.md`

## Global Constraints

- Do not claim physical command-ring acceptance until the refreshed image is booted on the target and shows `xhci cmd` or a readable `xhci cmd err` panel.
- Do not submit stack or ordinary heap memory to DMA hardware.
- Every scratchpad buffer must be page-aligned, physically translated, and written through the static scratchpad pointer array.
- Keep all xHCI waits finite and all hardware failures typed.
- Preserve `NO_DISK_WRITES`.
- Re-identify `P:` before USB deployment and copy only `image\esp` files.

---

### Task 1: Scratchpad DMA State

**Files:**
- Modify: `core/src/usb_xhci_driver.rs`
- Test: `core/src/usb_xhci_driver.rs`

**Interfaces:**
- Consumes: `scratchpad_buffer_count(hcsparams2: u32) -> u16`
- Produces: `scratchpad_support_required(hcsparams2: u32) -> Result<usize, XhciDriverError>` and `XhciDmaState::scratchpad_count`

- [x] **Step 1: Write failing tests**

```rust
#[test]
fn accepts_physical_amd_style_scratchpad_count() {
    let hcsparams2 = 0x0800_0000;

    assert_eq!(scratchpad_support_required(hcsparams2), Ok(1));
}

#[test]
fn rejects_scratchpad_counts_above_static_diagnostic_capacity() {
    let hcsparams2 = 0x1000_0000;

    assert_eq!(
        scratchpad_support_required(hcsparams2),
        Err(XhciDriverError::UnsupportedScratchpadBuffers)
    );
}
```

- [x] **Step 2: Run RED check**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`

Observed: failed because `scratchpad_support_required`, the scratchpad-aware
`prepare_dma_state` signature, scratchpad state fields, and pointer helpers did
not exist.

- [x] **Step 3: Implement static scratchpad DMA**

Add a static scratchpad pointer array and 32 page-aligned scratchpad buffers. Extend `prepare_dma_state` to zero the pointer array, translate each scratchpad page, validate page alignment, write the page addresses into the pointer array, and place the scratchpad pointer-array physical address in `DCBAA[0]` when `scratchpad_count > 0`.

- [x] **Step 4: Run GREEN check**

Run: `cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture`

Observed: 9 focused driver tests passed.

### Task 2: QEMU and Deployment Evidence

**Files:**
- Modify: `README.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Modify: `docs/decisions/0081-usb-xhci-command-ring-driver.md`
- Modify: `docs/evidence/index.html`
- Modify: `D:\PythOS-Workspace\CURRENT-STATE.md`

**Interfaces:**
- Consumes: successful Task 1 build and `scripts/test-usb-xhci-command-probe.py`
- Produces: refreshed USB deployment evidence and next physical test instructions

- [x] **Step 1: Verify QEMU**

Run: `py -3 scripts\test-usb-xhci-command-probe.py`

Observed: `USB_XHCI_COMMAND_PROBE_TEST_OK`, `QEMU_OUTCOME success`, and
`XHCI_SCRATCHPAD_COUNT=0`.

- [x] **Step 2: Re-identify USB target**

Run read-only PowerShell checks for `P:` volume, partition, disk, and root contents.

Observed: Disk 2 Lexar D70E USB, serial `1026R51254700477`, MBR active FAT32
`PYTHOS_ESP`, not Windows boot/system; `chkdsk P:` found no filesystem
problems.

- [x] **Step 3: Deploy and verify readback**

Observed: copied only files from `image\esp` to `P:\`, preserving unrelated
root files. Readback reported
`USB_XHCI_SCRATCHPAD_VERIFY_OK files:8 bytes:3949296`; deployed
`P:\PYTHOS\PYTHCORE.ELF` SHA-256 is
`5E65C5A697A443369CB9AAC11E4AADAB7A26888B920EC89F43BEC5F33CF8CC44`.

- [x] **Step 4: Update docs**

Record the physical `err 00000006` root cause, scratchpad support addition, QEMU verification, deployment marker, and the next physical boot instruction.
