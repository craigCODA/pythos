# Phase 15 Network-Hardware Identity Probe Plan

## Goal

Add one small, opt-in Phase 15 hardware slice: read-only PCI identity evidence
for QEMU `e1000` and `e1000e`, while keeping the Lenovo Wi-Fi target and all
network-device control work out of scope.

## Architecture and boundaries

- Add a dedicated `network-hardware-probe` feature and early boot path.
- Keep the existing storage `hardware-probe` and its acceptance contract
  unchanged.
- The existing storage `hardware-probe` remains unchanged.
- Copy the proven bounded CF8/CFC traversal into the dedicated probe instead
  of refactoring a generalized PCI abstraction.
- Read identity fields only: BDF, vendor/device, subsystem vendor/device,
  class/subclass/prog-if, and bounded result state. Do not read BARs.
- Render serial and framebuffer identity evidence, then halt.
- Preserve `VirtioTransport`, transport adapter, `NetworkPort`, and all Phase
  14 ABI and transport behavior.
- Do not add Wi-Fi frames/firmware, network datapath or packet datapath, MMIO control, DMA, bus
mastering, interrupts, reset, modern Virtio, physical claims, or generalized
hardware abstraction. It is not physical hardware support.

## Write scope

1. `core/Cargo.toml`: add the empty `network-hardware-probe` feature.
2. `core/src/main.rs`: add feature conflicts, module declarations, and early
   dispatch.
3. `core/src/network_hardware_probe.rs`: add the private bounded scan,
   classification, report, subsystem fields, and serial formatting.
4. `core/src/network_hardware_probe_boot.rs`: add the identity-only boot and
   halt path.
5. `core/src/network_hardware_probe_screen.rs`: add the fixed framebuffer
   panel.
6. `scripts/run-qemu.py`: add explicit `e1000`/`e1000e` selection using
   `--network-device {e1000,e1000e}` and `-nic none` so no implicit NIC makes
   the count pass accidentally.
7. `scripts/test-network-hardware-probe.py`: add self-tests and two live QEMU
   runs.
8. `tests/test_network_hardware_probe.py`: lock the ADR, feature, marker, and
   non-claim contract.

Do not modify `core/src/storage_probe.rs`, the Phase 14 network code, or the
existing storage probe harness for this slice.

## Marker contract

The dedicated image must emit, in order:

```text
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:ENTER
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PCI_SCAN_READY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_COUNT=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_CONTROLLER_FOUND
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_KIND:ETHERNET
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:BUS=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:DEVICE=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:FUNCTION=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:VENDOR=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:DEVICE_ID=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBSYSTEM_VENDOR=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBSYSTEM_DEVICE=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:CLASS=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBCLASS=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PROG_IF=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_IDENTITY_READY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PCI_CONFIG_READ_ONLY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE_READY
```

QEMU must report count one. Its expected model values are `e1000` =
`0x8086:0x100E`, `e1000e` = `0x8086:0x10D3`, and both `0x02/0x00/0x00`.
Wi-Fi behavior remains deferred.

## Verification sequence

1. Run focused Rust unit tests for classification, the RTL8822CE fixture
   `10EC:C82F / 17AA:C02F` with class `02/80`, invalid functions, and bounded
   overflow.
2. Run `py -3 scripts/test-network-hardware-probe.py --self-test` and
   `py -3 -m unittest tests.test_network_hardware_probe`.
3. Build and run `py -3 scripts/test-network-hardware-probe.py`; it must pass
   both QEMU device models with one controller and no forbidden evidence.
4. Run format, diff, focused CI contract tests, and the full workspace/Python
   suites before acceptance.

## Deliberately deferred

Physical Lenovo Wi-Fi behavior, firmware, association, frame movement, BAR
register reachability, controller ownership/control, interrupts, DMA, modern
Virtio, hardware memory models, and all follow-up ADR items remain deferred.
