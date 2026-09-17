# Task 6 Final Report — Phase 14 NetworkPort Closeout

Date: 2026-09-15

## Status

Phase 14 `NetworkPort` is accepted at corrected implementation tip
`e1efea0` (`fix(net): align receive status contract`), with final status-only
documentation at `f5e5d81`. The correction maps zero receive capacity to the
ADR 0095-required `BAD_REQUEST`, retains `BUFFER_TOO_SMALL` only for capacities
1 through 1513, and adds syscall/resource boundary tests. The earlier host-test
correction gates the native probe's `no_std`, `no_main`, and panic handler to
non-test builds, so the required host workspace suite runs without changing
target behavior.

The accepted boundary is one opt-in, boot-local, capability-scoped
`NetworkPort` above the privileged `VirtioTransport` adapter. The raw
virtio-net profile and the native NetworkPort QEMU consumer each prove one
bounded 60-byte raw Ethernet TX frame and one bounded 60-byte RX peer exchange;
the NetworkPort consumer performs that exchange through the capability ABI.
ADR 0095 was reviewed and not changed.

## Fresh Verification

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed. |
| `git diff --check` | Passed. |
| `cargo test --workspace --quiet` | Passed; all workspace groups completed with no failures. |
| `uv run --no-project --with pytest python -m pytest tests -q --tb=short` | Passed: 210 tests and 239 subtests. `uv` obtained and used pytest. |
| `py -3 -m unittest tests.test_network_port tests.test_ci_workflow` | Passed: 13 tests. |
| `py -3 scripts/test-virtio-net.py --self-test` | Passed: 7 tests and `VIRTIO_NET_ACCEPTANCE_SELF_TEST_OK`. |
| `py -3 scripts/test-network-port.py --self-test` | Passed: 7 tests and `NETWORK_PORT_ACCEPTANCE_SELF_TEST_OK`. |
| `py -3 -m py_compile scripts/test-virtio-net.py scripts/test-network-port.py` | Passed. |
| `py -3 scripts/test-virtio-net.py` | Passed: QEMU 11.0.50 and `VIRTIO_NET_ACCEPTANCE_OK`. |
| `py -3 scripts/test-network-port.py` | Passed: QEMU 11.0.50 and `NETWORK_PORT_QEMU_ACCEPTANCE_OK`. |
| Default and opt-in targets | Default core, `network-port-probe`, and `virtio-net-probe` target builds passed. |
| Normal-session/default boot | `py -3 scripts/test-normal-fast-boot.py` passed with `NORMAL_FAST_BOOT_TEST_OK`. |

The final review correction was rerun through the focused NetworkPort/syscall
tests, the aggregate workspace and managed Python suites, all target builds,
normal fast boot, and both live QEMU profiles. The live marker profile remains
limited to forged-generation, wrong-holder, and bad-pointer denials; the host
syscall matrix covers missing rights, stale generations, overflow, buffer
permissions, receive capacities, and oversized frames.

The builds emitted pre-existing unused/dead-code warnings only; no warning
failed a required gate.

## Fresh QEMU Evidence

Both live runs used `QEMU emulator version 11.0.50
(v11.0.0-12631-g54e84cdc7a)`.

The exact live invocation was `py -3 scripts/test-virtio-net.py`. It rebuilt
and used these fresh artifact locations:

```text
target/x86_64-unknown-uefi/debug/bootx64.efi
target/x86_64-unknown-none/debug/pythcore
target/x86_64-unknown-none/debug/pythos-user-shell
image/esp
target/virtio-net-probe-com1.log (removed by the harness after validation)
```

The raw profile does not build a separate
`target/x86_64-unknown-none/debug/pythos-user-virtio-net-probe` artifact; its
probe is a PythCore feature profile. The live run used
`--no-virtio-blk --virtio-net`, validated one bounded 60-byte TX frame and one
bounded 60-byte RX peer exchange, and emitted exactly, in order:

```text
PYTHOS:CORE:VIRTIO_NET_PROBE:ENTER
PYTHOS:CORE:VIRTIO_NET_PROBE:PCI_SCAN_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:DEVICE_FOUND
PYTHOS:CORE:VIRTIO_NET_PROBE:LEGACY_TRANSPORT_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=52:54:00:12:34:56
PYTHOS:CORE:VIRTIO_NET_PROBE:FEATURES_NEGOTIATED
PYTHOS:CORE:VIRTIO_NET_PROBE:RX_QUEUE_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:TX_QUEUE_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:TX_FRAME_SENT
PYTHOS:CORE:VIRTIO_NET_PROBE:RX_FRAME_RECEIVED
PYTHOS:CORE:VIRTIO_NET_PROBE:RAW_ETHERNET_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:NO_DISK_WRITES
PYTHOS:CORE:VIRTIO_NET_PROBE:READY
QEMU_OUTCOME success
VIRTIO_NET_PEER_TX_MATCHED
VIRTIO_NET_PEER_RX_DELIVERED
VIRTIO_NET_ACCEPTANCE_OK
```

The exact NetworkPort live invocation was `py -3
scripts/test-network-port.py`. It rebuilt and used these fresh artifact
locations:

```text
target/x86_64-unknown-uefi/debug/bootx64.efi
target/x86_64-unknown-none/debug/pythcore
target/x86_64-unknown-none/debug/pythos-user-shell
target/x86_64-unknown-none/debug/pythos-user-network-port-probe
image/esp
target/network-port-probe-com1.log (removed by the harness after validation)
```

The NetworkPort live profile also used `--no-virtio-blk --virtio-net`,
validated one bounded 60-byte TX frame and one bounded 60-byte RX peer
exchange, and completed `NETWORK_PORT_QEMU_ACCEPTANCE_OK`. Before terminal
success its live oracle combined consumer and kernel serial streams and
required exactly once, in this order:

```text
PYTHOS:CORE:NETWORK_PORT:BOOTSTRAPPED
PYTHOS:CORE:NETWORK_PORT:DESCRIBE_OK
PYTHOS:CORE:NETWORK_PORT:TX_OK
PYTHOS:CORE:NETWORK_PORT:RX_OK
PYTHOS:CORE:NETWORK_PORT:FORGED_DENIED
PYTHOS:CORE:NETWORK_PORT:WRONG_HOLDER_DENIED
PYTHOS:CORE:NETWORK_PORT:BAD_BUFFER_DENIED
PYTHOS:CORE:NETWORK_PORT:TEARDOWN_REVOKED
PYTHOS:CORE:NETWORK_PORT_READY
```

The fresh NetworkPort live oracle also validated `QEMU_OUTCOME success`, exact
bounded peer TX/RX bytes, and the absence of panic, timeout, transport failure,
reset, and storage-path evidence. Both live harnesses removed their temporary
COM1 serial logs after validating them; consequently
`target/virtio-net-probe-com1.log` and
`target/network-port-probe-com1.log` were absent after the runs. The
snapshot-backed IDE UEFI ESP is boot media only; neither profile attaches a
non-boot virtio data disk.

## Independent Diff and Scope Review

Reviewed `d9c09b9..f5e5d81`, covering the ABI, adapter refactor,
NetworkPort runtime/syscall, native consumer, feature/image wiring, host
acceptance, and host-test correction. `git diff --check d9c09b9..d08dd23`
passed.

- `core/src/virtio_net.rs` uses `fence(Ordering::SeqCst)` in `dma_fence` with a
  device-visible DMA ordering comment. There is no `compiler_fence` in the
  VirtioTransport or NetworkPort implementation and no compiler-only ordering
  claim for the adapter.
- A targeted added-line review found no introduced PythOS `Driver` architecture
  abstraction; `VirtioTransport` remains the privileged adapter.
- The normal-session/default path remains separate from the opt-in profiles.
  No physical NIC/Wi-Fi, link-layer, protocol/socket, production-service,
  zero-copy, persistent-network, or PythTIG work was introduced.

## Deferred Finding and Stop Boundary

The SDD ledger retains the Task 4 minor finding: the bootstrap-writable unit
assertion is tautological. Live address-space mapping validation and focused
QEMU evidence independently cover the read-only policy. No ADR item was
resolved or changed.

The explicit deferred follow-up inventory is unchanged and unresolved:

- resource-id evolution and reuse beyond the accepted one-port boot-local rule;
- capability-right evolution;
- syscall renumbering and other ABI variants;
- receive-buffer sizing and maximum-copy/copy-policy changes;
- runtime capability import and Pyth/service consumer selection;
- teardown policy beyond terminal ABI v1;
- modern or physical transports, including physical NIC/Wi-Fi, modern Virtio
  PCI, interrupts, MSI/MSI-X, multiqueue, and offloads;
- protocols and sockets;
- multi-consumer packet distribution, zero-copy, and persistent network state;
- Phase 15 and physical Wi-Fi; and
- any PythTIG v1 change.

Stop here. Link-layer work and Phase 15 remain later, explicitly invoked work.

## Documentation Contract Correction — Final Result

The closeout status text retained the raw `nic-driver` compatibility statements
alongside the accepted NetworkPort boundary. The repository's existing
documentation contract requires selected literals to remain contiguous. The
minimal final corrections joined those literals in the current status sections;
no implementation, ABI, ADR, acceptance harness, or test was changed.

The final repository-managed gate on the completed documentation tree passed:

```text
cargo fmt --all -- --check                 PASS
git diff --check                            PASS
cargo test --workspace --quiet             PASS
uv run --no-project --with pytest python -m pytest tests -q --tb=short
210 passed, 239 subtests passed
```
