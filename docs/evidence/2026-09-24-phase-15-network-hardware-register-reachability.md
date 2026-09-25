# Phase 15 Network Register Reachability — QEMU Evidence

**Date:** 2026-09-24
**Profile:** `network-hardware-register-probe`
**ADR:** [0107](../decisions/0107-phase-15-network-hardware-register-reachability.md)
**Source commit:** `8290729`
**Prepared ISO:** `target/pythos-phase15-network-hardware-register-probe-20260924.iso`
**ISO size:** `20,766,720` bytes
**ISO SHA-256:** `9D2597044B4E120A375257A9F652BE836299F065DD6902310ACE56B06D262CE7`

## Automated result

The isolated runner used `-nic none`, exactly one explicit PCI network device,
no network backend, no non-boot Virtio block device, and required one successful
QEMU outcome.

| QEMU model | vendor/device | PCI command/status | BAR | register | observed value |
| --- | --- | ---: | ---: | ---: | ---: |
| `e1000` | `8086:100E` | `0x0000000000000007` | 0 | `0x08` | `0x0000000080080783` |
| `e1000e` | `8086:10D3` | `0x0000000000100007` | 0 | `0x08` | `0x0000000000080283` |

Both runs emitted `FRAMEBUFFER_REGISTER_READY`, `REGISTER_REACHABILITY_READY`,
`PCI_CONFIG_READ_ONLY`, and the final ready marker. The oracle self-test,
focused Python contract tests, full 900-test PythCore suite, `cargo fmt`,
`cargo clippy -D warnings`, and the live two-model QEMU oracle passed.

## Physical status

The Lenovo `81VS` observation is pending. The probe will display and accept either the
guarded Realtek `10EC:C82F` `0x00F4` status read or the explicit
`PCI_MEMORY_SPACE_DISABLED` safe skip. No result will be interpreted as
physical NIC/Wi-Fi support. The existing Phase 15 ISO remains preserved; the
new image is a separate uniquely named ISO.
