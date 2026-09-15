# Phase 14 Hybrid `NetworkPort` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the accepted Phase 14 hybrid `NetworkPort` boundary on top of the existing legacy QEMU `virtio-net-pci` transport adapter, prove the fixed capability ABI through an opt-in native QEMU consumer, and leave default boot, PythTIG v1, physical networking, and later protocol work unchanged.

**Architecture:** Keep the privileged legacy Virtio PCI mechanics in `VirtioTransport`, which fulfills the Virtio specification's driver role without becoming a new PythOS `Driver` abstraction. Add one runtime-only capability-scoped `NetworkPort` resource above it. The resource performs bounded copy-in/copy-out Ethernet frame operations through a fixed request/response syscall ABI. A dedicated opt-in native probe receives the capability through a read-only bootstrap block and drives the first acceptance path. Ethernet and protocol semantics remain above the port boundary.

**Tech Stack:** Rust 2024 `no_std` PythCore and shared ABI types, existing capability table and user-buffer validation, x86-64 syscall dispatch, legacy transitional virtio-net PCI I/O, static bounded DMA queues, native ring-3 probe ELF, Python QEMU acceptance harness, and existing QEMU `isa-debug-exit` markers.

**Spec:** `docs/superpowers/specs/2026-09-15-phase-14-network-port-hybrid-design.md`

**ADR:** `docs/decisions/0095-phase-14-network-port-capability-abi.md`

## Global Constraints

- Implement only the accepted ADR 0095 ABI and the already-accepted hybrid design; do not reopen architecture or add a generalized hardware abstraction.
- Preserve `VirtioTransport`, `transport adapter`, and `NetworkPort` as PythOS architectural names. Specification references may use Virtio's `driver`, `DRIVER`, and `DRIVER_OK` terminology. Do not perform a repo-wide identifier rename.
- Keep the existing legacy transitional QEMU `virtio-net-pci` transport adapter. Do not add modern Virtio PCI capabilities, MMIO, MSI/MSI-X, interrupts, multiqueue, offloads, physical NIC/Wi-Fi behavior, or generalized reset behavior.
- Keep descriptor preparation, buffer ownership, virtqueue population/`avail.idx` exposure, `DRIVER_OK`, notification, and completion consumption as distinct transitions. Initialization may populate queues and expose `avail.idx` before `DRIVER_OK`; notification and device consumption remain gated by `DRIVER_OK`; service operations remain gated by `Operational`.
- Use actual device-visible/DMA memory ordering (`fence` or equivalent transport contract), not wording or code that claims a compiler-only fence is the Virtio ordering mechanism.
- Keep the runtime resource ephemeral and capability-authorized. Use the exact resource namespace, rights, syscall number, packed layouts, frame bounds, bootstrap block, reset/teardown rules, statuses, and marker sequence in ADR 0095. Do not add rights, persistent state, resource reuse, multi-consumer distribution, zero-copy leases, or a PythTIG extension.
- Keep the first consumer opt-in and native. Default and normal-session boot paths must remain unchanged, and production boot must not expose the test-only network capability.
- Do not add sockets, IP/ARP/DHCP/ICMP/UDP/TCP/DNS semantics, a production service, protocol multiplexing, or a Lenovo network-hardware probe. Those are outside this implementation and/or Phase 15.
- Follow the existing copy-in/copy-out policy: validate capability and all buffer shape/permission/overflow conditions before transport mutation; never submit user or stack memory to DMA; keep receive nonblocking and fixed at the ADR's 1514-byte output contract.
- Preserve the frozen PythTIG v1 ABI and existing implementation identifiers unless a local transport-adapter refactor requires a narrowly scoped name change.

## File and responsibility map

- `shared/src/network_port_abi.rs`: exact versioned request, response, description, bootstrap, operation, status, state, flag, namespace, and marker constants with layout tests.
- `shared/src/lib.rs`, `shared/src/user_program_manifest.rs`, `Cargo.toml`: expose the shared ABI and name/principal identity for the native probe.
- `core/src/virtio_net.rs`: retain the legacy PCI transport implementation as `VirtioTransport`, correct device-visible ordering, and add bounded copy-oriented TX/RX operations without leaking the private Virtio header.
- `core/src/network_port.rs`: own the one runtime-only `NetworkPort`, resource lifecycle, rights policy, frame bounds, teardown/revocation, and transport delegation.
- `core/src/syscall.rs`: register and dispatch `SYSCALL_NETWORK_PORT_REQUEST` using existing capability and user-buffer validation/copy helpers.
- `core/src/main.rs`, `core/Cargo.toml`, `core/src/network_port_probe.rs`: opt-in feature, address-space/bootstrap preparation, native probe launch, serial markers, and QEMU exit behavior.
- `Cargo.toml`, `user/probes/network-port/**`, `scripts/build-network-port-probe.py`, `scripts/build-image.py`: build and package the named native consumer without changing default image contents.
- `scripts/test-network-port.py`, `tests/test_network_port.py`, `.github/workflows/qemu-acceptance.yml`: self-test, live QEMU peer, exact marker/negative-path oracle, and CI wiring.
- `docs/decisions/0095-phase-14-network-port-capability-abi.md`, `docs/HANDOVER.md`, `docs/ROADMAP.md`, `docs/TECHNICAL-OVERVIEW.md`, `README.md`, and workspace `CURRENT-STATE.md` only as final status/verification closeout; do not revise ADR decisions.

---

### Task 1: Add the frozen shared `NetworkPort` ABI

**Files:**

- Create: `shared/src/network_port_abi.rs`
- Modify: `shared/src/lib.rs`
- Modify: `shared/src/user_program_manifest.rs`
- Modify: `Cargo.toml`
- Test: `shared/src/network_port_abi.rs`

**Interfaces:**

- Define `NETWORK_PORT_ABI_MAJOR/MINOR`, `SYSCALL_NETWORK_PORT_REQUEST`, the `NetworkPortRequestV1`, `NetworkPortResponseV1`, `NetworkPortDescriptionV1`, and `NetworkPortBootstrapV1` `#[repr(C)]` types exactly at the offsets and sizes in ADR 0095.
- Define the four operations, eight statuses, three states, two transport flags, resource-id namespace, bootstrap magic, frame limits, and exact marker strings from ADR 0095.
- Add `NETWORK_PORT_PROBE_PROGRAM_NAME` and `NETWORK_PORT_PROBE_PRINCIPAL_ID` without changing existing named-program identities.
- Provide compile-time or unit assertions for size/alignment/offset-sensitive layouts, reserved-zero constructors, and safe little-endian-independent field access through Rust layout rather than ad hoc wire encoders.

- [ ] Add failing layout, constants, reserved-field, and marker tests.
- [ ] Implement only the accepted shared ABI and manifest constants.
- [ ] Run `cargo test -p pythos-shared network_port_abi -- --nocapture` and `cargo fmt --all -- --check`.
- [ ] Commit `feat(net): add NetworkPort shared ABI`.

### Task 2: Finish the `VirtioTransport` adapter boundary

**Files:**

- Modify: `core/src/virtio_net.rs`
- Test: `core/src/virtio_net.rs`

**Interfaces and invariants:**

- Make the existing privileged implementation architecturally explicit as `VirtioTransport`; preserve a local compatibility alias only where needed by existing raw-transport tests or probe code.
- Retain legacy transitional PCI discovery, MAC-only negotiation, static bounded queues, private ten-byte no-offload header, raw Ethernet frame validation, and current QEMU behavior.
- Replace compiler-only ordering claims and calls with device-visible/DMA ordering around descriptor writes, available-ring publication, used-ring observation, and notification I/O.
- Keep initialization ordering explicit: prepare descriptors/buffers; populate queues and expose allowed `avail.idx` values; set `DRIVER_OK`; make the post-`DRIVER_OK` notification decision subject to suppression rules; only then admit operational completion consumption.
- Add bounded `transmit(frame_bytes)` and nonblocking `try_receive_into(output)` operations that copy into/from private DMA slots. Do not return a borrowed DMA frame to the service layer and do not expose the Virtio header.
- Ensure no TX service operation can take an “initialization” branch: service TX starts only after `Operational`. Initial pre-`DRIVER_OK` population is an RX setup concern.
- Keep reset/failed ownership clearing sufficient for the current legacy transport and compatible with the terminal first-ABI teardown; do not add physical-device reset policy.

- [ ] Add/adjust focused tests for pre-`DRIVER_OK` `avail.idx` exposure, notification gating, post-`DRIVER_OK` consumption, TX steady-state flow, nonblocking RX empty behavior, and device-visible ordering terminology/code.
- [ ] Implement the adapter changes without changing the raw transport acceptance contract.
- [ ] Run `cargo test -p pythos-core virtio_net -- --nocapture`, `cargo fmt --all -- --check`, and `git diff --check`.
- [ ] Commit `refactor(net): expose VirtioTransport adapter lifecycle`.

### Task 3: Implement the runtime `NetworkPort` and syscall dispatch

**Files:**

- Create: `core/src/network_port.rs`
- Modify: `core/src/main.rs`
- Modify: `core/src/syscall.rs`
- Test: `core/src/network_port.rs`, `core/src/syscall.rs`

**Interfaces and invariants:**

- Allocate at most one boot-local resource id in `0x4E50_0000_0000_0000 | sequence`, never reuse it, and bind it to the initialized `VirtioTransport` only after `Operational`.
- Implement the exact rights policy: `READ` for describe/receive, `SEND` for transmit, `WRITE` for kernel-owner reset; deny capability failures before user-buffer validation or queue mutation and return stable user-facing `DENIED`.
- Implement `DESCRIBE`, `SEND`, `TRY_RECEIVE`, and terminal administrative `RESET` with exact request/response shapes and pointer rules from ADR 0095. `TRY_RECEIVE` is nonblocking; empty does not consume or recycle an RX slot.
- Propagate `NOT_READY`, `FAILED`, and `TRANSPORT_ERROR` without exposing partial frames. Revoke/deny consumer access on failure or teardown and keep the resource id unreusable.
- Register the syscall at `0x5059_0160` in the existing syscall table without modifying PythTIG or creating a new PythOS `Driver` abstraction. Use the existing `ActiveUserProcess` mapping checks and copy helpers.
- Keep the network module testable with an injected/fake transport boundary where existing unit-test architecture requires it, while production uses the existing legacy adapter.

- [ ] Add failing unit tests for forged resource, wrong holder, missing rights, stale generation, malformed requests, bad pointers/permissions/overflow, exact frame bounds, empty receive, describe, send, reset, and revocation.
- [ ] Implement the runtime resource and syscall dispatch with reserved-field and exact-length validation before transport access.
- [ ] Run focused shared/core tests and `cargo test --workspace --quiet` for the current tree; do not claim live acceptance yet.
- [ ] Commit `feat(net): add capability-scoped NetworkPort syscall`.

### Task 4: Launch the opt-in native consumer through the bootstrap capability

**Files:**

- Create: `core/src/network_port_probe.rs`
- Modify: `core/src/main.rs`
- Modify: `core/Cargo.toml`
- Modify: `Cargo.toml`
- Create: `user/probes/network-port/Cargo.toml`
- Create: `user/probes/network-port/src/main.rs`
- Create: `user/probes/network-port/src/lib.rs` if the probe layout needs it
- Create: `user/probes/network-port/linker.ld`
- Create: `scripts/build-network-port-probe.py`
- Modify: `scripts/build-image.py`

**Interfaces and invariants:**

- Add an opt-in `network-port-probe = ["verify"]` feature mutually exclusive with other terminal probe profiles as appropriate; normal/default boot remains byte-for-byte behaviorally unchanged.
- Prepare one read-only `NetworkPortBootstrapV1` user mapping, grant only the probe's `READ | SEND` capability, retain administrative `WRITE` in PythCore, and pass only the bootstrap mapping and console capability through the existing native launch path.
- Build the probe as a named, loader-validated ELF using the shared ABI. It must exercise the exact accepted marker sequence: bootstrapped, describe, TX, RX, forged denied, wrong-holder denied, bad-buffer denied, teardown revoked, ready.
- Use the existing one-shot breakpoint/return-to-kernel pattern and `qemu_exit` contract. Any failed check emits a deterministic error/panic marker and exits nonzero; the feature must not expose network authority in production boot.
- Keep peer data at bounded Ethernet frame bytes; the probe must never see the private Virtio header or transport addresses.

- [ ] Add failing launch/ABI probe tests or build checks for bootstrap contents, feature exclusion, and marker sequencing.
- [ ] Implement the native probe, named-program packaging, and kernel launch wiring.
- [ ] Run the probe ELF build/verification and the opt-in core target build, plus focused Rust tests.
- [ ] Commit `feat(net): launch native NetworkPort consumer`.

### Task 5: Add deterministic host and QEMU acceptance

**Files:**

- Create: `scripts/test-network-port.py`
- Create: `tests/test_network_port.py`
- Modify: `scripts/run-qemu.py` only if the existing Virtio peer arguments need a narrow reusable extension
- Modify: `.github/workflows/qemu-acceptance.yml`

**Interfaces and invariants:**

- Reuse the accepted legacy QEMU socket peer and existing raw transport acceptance helpers; the new script validates the port-boundary profile, not a new network backend.
- Self-test exact marker ordering/count, required negative markers, absence of panic/timeout/failure after `READY`, frame bytes, no storage-path markers, and classification of timeout/nonzero/reset as failure.
- The live path explicitly builds the opt-in kernel and native probe, starts the loopback peer before QEMU, uses no non-boot virtio data disk, and prints one stable success line.
- Add host tests for ABI constants/marker contract, malformed requests represented in the probe oracle, peer frame validation, and runner command shape. Do not weaken existing raw `virtio-net` acceptance.
- Wire CI after existing acceptance prerequisites without changing default runner arguments.

- [ ] Add failing self-test/host tests for the exact marker and denial contract.
- [ ] Implement the isolated live acceptance script and CI command.
- [ ] Run `py -3 scripts/test-network-port.py --self-test` and focused host tests.
- [ ] Run live `py -3 scripts/test-network-port.py`; record QEMU version and artifact/log paths.
- [ ] Commit `test(net): accept NetworkPort capability boundary`.

### Task 6: Final verification and handoff closeout

**Files:**

- Modify only as necessary: `docs/HANDOVER.md`, `docs/ROADMAP.md`, `docs/TECHNICAL-OVERVIEW.md`, `README.md`
- Do not modify ADR 0095 decisions; update `D:/PythOS-Workspace/CURRENT-STATE.md` only after fresh evidence if the workspace checkpoint is still the project convention.

**Acceptance gates:**

- [ ] Run `cargo fmt --all -- --check`, `git diff --check`, `cargo test --workspace --quiet`, and the available Python test suite/self-tests.
- [ ] Run both existing raw `virtio-net` self/live acceptance and new `NetworkPort` self/live acceptance from fresh artifact/log locations.
- [ ] Confirm the default/normal-session path remains unchanged and no storage-path marker or non-boot data disk appears in the NetworkPort profile.
- [ ] Confirm no `compiler_fence` remains as the claimed Virtio ordering mechanism in the adapter and no architectural `Driver` noun was introduced.
- [ ] Perform an independent diff review over the implementation commits and record any deferred findings.
- [ ] Update status docs only to record the accepted runtime boundary and the explicit next/later work. Do not claim Phase 15, physical NIC/Wi-Fi, protocols, sockets, production service, zero-copy, or PythTIG changes.
- [ ] Commit `docs(net): record NetworkPort implementation evidence` only after all required checks pass.

## Deliberately deferred

The following remain ADR 0095 or later-phase work and must not be resolved in this plan: final resource-id evolution/reuse policy beyond the accepted one-port boot-local rule; new capability rights; syscall renumbering or ABI variants; variable receive buffers or maximum-copy policy changes; runtime/service capability import; teardown policy beyond the accepted terminal first ABI; selection of a Pyth runtime consumer; physical NIC or Lenovo Wi-Fi support; modern Virtio PCI, interrupts, MSI/MSI-X, multiqueue, offloads, generalized hardware memory models; sockets and protocol semantics; multi-consumer packet distribution; zero-copy; persistent network state; and any PythTIG v1 change.
