# Task 4 Report: UDP Probe Launch Path

## Scope

Added the opt-in `udp-probe = ["verify"]` PythCore launch path for the
existing `udp-probe.elf` native program. The implementation follows the
accepted ICMP launch pattern and changes only the approved core cfg/launch
files plus `core/src/udp_probe.rs`.

## Implementation

- Added `UdpProbeLaunchContract` tests and the finite verified UDP launch.
- Uses the shared program identity `udp-probe.elf` / `0x5059_5544_5000_0001`
  and shared consumer/owner identities `0x5059_5544_4353_0001` /
  `0x5059_5544_4F57_0001`.
- Reuses the existing read-only NetworkPort bootstrap at
  `0x0000_0000_7300_0000`, the existing `READ | SEND` consumer grant, and the
  existing owner-only RESET and consumer-revocation teardown path.
- Preserves the seven UDP acceptance markers in order, ending in
  `PYTHOS:CORE:UDP_READY`.
- Added the same mutual exclusions held by `icmp-probe` for phase/session,
  Virtio/NetworkPort, link-layer, ARP, IPv4, and ICMP profiles.
- Leaves the default `normal-session` selection and all ABI, syscall,
  capability-right, transport, and lifecycle definitions unchanged.

## TDD Evidence

1. Added the test-only UDP module declaration, then ran the UDP feature test
   before declaring the feature. It failed as expected: `pythos-core` did not
   contain feature `udp-probe`.
2. Added the focused UDP test module before the launch contract. The focused
   core test failed as expected with unresolved
   `super::UdpProbeLaunchContract`.
3. Implemented the minimal additive launch path and cfg gates. The focused
   UDP tests then passed: 4 passed, 0 failed.

## Verification

All required commands passed:

```text
cargo test -p pythos-core --bin pythcore
# 857 passed, 0 failed

cargo test -p pythos-core --bin pythcore --features udp-probe
# 857 passed, 0 failed

cargo fmt --all -- --check
cargo clippy -p pythos-core --target x86_64-unknown-none --features udp-probe -- -D warnings
git diff --check
```

The host test invocations emit existing unused-code warnings in unrelated
test-enabled modules; the required strict target Clippy invocation completed
cleanly with `-D warnings`.
