# USB xHCI Physical Timeout Stage Diagnostic Plan

> **Execution:** Use `superpowers:executing-plans` in the existing
> `agent/hw-white-screen-diagnostic` worktree. The owner approved this bounded
> follow-up after the first physical ADR 0084 attempt rendered generic error
> `0x0F`.

**Goal:** Preserve the first physical ADR 0084 evidence and make any subsequent
finite xHCI wait timeout identify the exact command or transfer stage on both
serial and framebuffer diagnostics.

**Architecture:** Keep `XhciDriverError::CommandTimeout` as the generic polling
failure, add six stage-specific timeout identities, and remap only that generic
failure at the existing No-op, Enable Slot, Address Device, Device Descriptor,
configuration-header, and full-configuration call sites. Each staged timeout
gets a stable serial marker, a distinct additive screen code, and a short
human-readable framebuffer line. The controller flow, frozen swap baseline,
finite polling limits, DMA layout, and no-write boundary remain unchanged.

## Constraints

- Preserve the boot-USB removal then mouse insertion workflow exactly.
- Preserve `PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES`.
- Do not add `SET_CONFIGURATION`, Configure Endpoint, HID parsing, endpoint
  polling, cursor movement, shell input, or trackpad support.
- Treat the photos as physical discovery/failure evidence, not successful
  configuration-descriptor acceptance.
- Treat QEMU as regression evidence only; rerun the Lenovo test after a verified
  deployment before making any new physical-hardware claim.

## Tasks

1. Copy the two operator-provided photos into `docs/evidence`, verify their
   SHA-256 hashes, and label their chronological order.
2. Add a failing driver contract test for the six timeout identities and a
   failing framebuffer test for the readable stage line.
3. Implement the six additive timeout identities and map the existing finite
   waits at their exact call sites.
4. Update ADR 0084, the evidence index, repository overview, and external
   `D:\PythOS-Workspace\CURRENT-STATE.md` with the deployed-attempt result and
   remaining physical boundary.
5. Run formatting, focused Rust tests, feature builds, the ADR 0083 and ADR 0084
   QEMU harnesses, normal boot, persistent-storage regression, and the available
   repository-wide Python fallback with baseline failures reported honestly.
6. Commit and push the checkpoint. If the USB is attached to the preparation
   workstation, re-identify it before copying the new image and verify exact
   readback hashes; otherwise stop and ask for insertion.

## Expected Diagnostic Codes

| Screen code | Framebuffer stage | Wait type |
|---|---|---|
| `0x2C` | `stage noop command` | command |
| `0x2D` | `stage enable slot` | command |
| `0x2E` | `stage address device` | command |
| `0x2F` | `stage device descriptor` | transfer |
| `0x30` | `stage config header` | transfer |
| `0x31` | `stage config full` | transfer |

## Verification

```powershell
cargo fmt --all -- --check
git diff --check
cargo test -p pythos-core --bin pythcore usb_xhci_driver -- --nocapture
cargo test -p pythos-core --bin pythcore usb_xhci_probe_screen::tests::formats_configuration_probe -- --nocapture
cargo build -p pythos-core --target x86_64-unknown-none --features usb-xhci-configuration-probe
py -3 -m py_compile scripts\test-usb-xhci-configuration-probe.py
py -3 scripts\test-usb-xhci-descriptor-probe.py
py -3 scripts\test-usb-xhci-configuration-probe.py
py -3 scripts\test-boot.py
py -3 scripts\test-persistent-storage.py
py -3 -m unittest discover tests
```

The configuration QEMU harness must end with both `QEMU_OUTCOME success` and
`USB_XHCI_CONFIGURATION_PROBE_TEST_OK`. Physical acceptance remains pending
until the refreshed image produces a new Lenovo result.
