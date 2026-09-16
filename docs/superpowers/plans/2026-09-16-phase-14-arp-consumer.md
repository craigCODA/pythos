# Phase 14 ARP Consumer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the accepted Phase 14 finite native ARP consumer proof above the existing capability-scoped `NetworkPort` boundary.

**Architecture:** Add a separately named opt-in native `arp` probe. It reuses the existing read-only bootstrap, `READ | SEND` capability, Ethernet-II codec, legacy/transitional QEMU `virtio-net-pci` transport, and terminal revocation path. The probe sends one exact 60-byte broadcast Ethernet/IPv4 ARP request and accepts one exact matching 60-byte unicast reply; no ARP cache, retry policy, service ABI, or persistent state is added.

**Tech Stack:** Rust 2024/no-`std` native ELF, Rust 1.93.1, existing `pythos-shared` ABI and marker modules, Python QEMU socket peer and serial oracle, `unittest`/`pytest`, and the existing ESP/image builder.

**Spec:** `docs/superpowers/specs/2026-09-16-phase-14-arp-design.md`

## Global Constraints

- Preserve `VirtioTransport`, “transport adapter”, and `NetworkPort` as the PythOS architectural names. Use “driver” only for Virtio specification terminology or existing implementation identifiers.
- Keep `VirtioTransport` privileged and keep ARP/Ethernet semantics in the native consumer; do not add protocol interpretation to transport or kernel code.
- Consume the existing `NetworkPort` ABI without adding a syscall, capability right, PythTIG v1 record, resource-id rule, or generalized protocol ABI.
- Freeze the additive native identity as `b"arp-probe.elf"` with principal `0x5059_4152_5052_0001`.
- Use the existing `NetworkPort` `READ | SEND` consumer capability; the owner-only `WRITE` capability remains kernel-owned.
- Send and receive exactly one 60-byte software Ethernet frame: 14-byte Ethernet header, 28-byte ARP payload, and 18 zero padding bytes; do not add or process FCS.
- Use Ethernet/IPv4 ARP only: hardware type `1`, protocol type `0x0800`, hardware length `6`, protocol length `4`, request opcode `1`, and reply opcode `2`.
- Use local IPv4 `192.0.2.2` and peer IPv4 `192.0.2.1` only as deterministic frame values; do not implement IP configuration or IP packets.
- Use peer MAC `02:00:00:00:00:02` and obtain the local MAC from `NetworkPort::DESCRIBE`.
- A malformed, unsupported, or nonmatching received frame is a terminal probe error. Do not add retries, timers, cache state, or timeout behavior.
- Leave the accepted link-layer marker sequence and implementation behavior unchanged. Existing raw Virtio, `NetworkPort`, link-layer, default-boot, and normal-session acceptance remain regressions.
- Keep the QEMU profile loopback-only, pass `--no-virtio-blk`, and require no storage-path markers.
- Do not add Lenovo Wi-Fi, physical NIC behavior, modern Virtio PCI capabilities, interrupts, MSI/MSI-X, multiqueue, offloads, VLAN policy, sockets, DHCP, routing, higher protocols, multiple consumers, packet distribution, zero-copy, or persistent state.
- Every unsafe block added to Rust code must have a local invariant comment. The serial transcript and `QEMU_OUTCOME success` remain the acceptance oracle.

## File Map

- Create `docs/decisions/0097-phase-14-arp-consumer.md` to record the accepted ARP decision, additive identity, wire contract, markers, acceptance proof, and non-claims.
- Create `shared/src/arp_markers.rs` and modify `shared/src/lib.rs` to export the six additive ARP markers. Modify `shared/src/user_program_manifest.rs` only to add the named-program identity constants.
- Add `user/probes/arp/` as a native package. `src/arp.rs` owns only the bounded 28-byte ARP codec; `src/main.rs` owns bootstrap, capability requests, Ethernet framing, fixed ARP policy, markers, and terminal exit; `src/lib.rs`, `Cargo.toml`, and `linker.ld` provide the tested library/bin and static ELF layout.
- Modify the workspace `Cargo.toml` to include `user/probes/arp`.
- Create `core/src/arp_probe.rs` for the additive named-probe launch wrapper and marker contract. Modify `core/Cargo.toml`, `core/src/main.rs`, `core/src/network_port.rs`, `core/src/network_port_probe_support.rs`, and `core/src/syscall.rs` only to wire the opt-in feature into existing NetworkPort plumbing and exclusion sets.
- Create `scripts/build-arp-probe.py`; modify `scripts/build-image.py` to package the named ARP ELF only when explicitly selected and to reject multiple network-probe ELF selections.
- Create `scripts/test-arp.py` and `tests/test_arp.py` for the loopback frame peer, serial/runner oracle, exact frame contract, and QEMU acceptance.
- Modify `tests/test_build_orchestration.py`, `tests/test_ci_workflow.py`, and `.github/workflows/qemu-acceptance.yml` for additive build, test, lint, script, and live-gate coverage.
- After live acceptance, modify `docs/ROADMAP.md`, `docs/HANDOVER.md`, and the ADR closeout text to record the accepted ARP evidence and state that IP is the next Phase 14 boundary.
- Do not modify `core/src/virtio_net.rs`, `shared/src/network_port_abi.rs`, the accepted link-layer consumer files, `scripts/build-iso.py`, or any implementation for Phase 15 hardware.

---

### Task 1: Record the ARP decision and freeze additive identities

**Files:**
- Create: `docs/decisions/0097-phase-14-arp-consumer.md`
- Create: `shared/src/arp_markers.rs`
- Modify: `shared/src/lib.rs`
- Modify: `shared/src/user_program_manifest.rs`
- Test: `shared/src/arp_markers.rs` unit tests and existing shared manifest tests

**Interfaces:**
- Consumes: accepted spec `docs/superpowers/specs/2026-09-16-phase-14-arp-design.md`, frozen `NetworkPort` ABI, and existing named-program manifest format.
- Produces: `ARP_PROBE_PROGRAM_NAME`, `ARP_PROBE_PRINCIPAL_ID`, six `pythos_shared::arp_markers` constants, and ADR 0097 for every later task.

- [ ] **Step 1: Write the failing marker and identity assertions.** Add test assertions for the exact values below before defining the constants:

```rust
assert_eq!(ARP_BOOTSTRAPPED_MARKER, "PYTHOS:CORE:ARP:BOOTSTRAPPED");
assert_eq!(ARP_DESCRIBE_OK_MARKER, "PYTHOS:CORE:ARP:DESCRIBE_OK");
assert_eq!(ARP_REQUEST_OK_MARKER, "PYTHOS:CORE:ARP:REQUEST_OK");
assert_eq!(ARP_REPLY_OK_MARKER, "PYTHOS:CORE:ARP:REPLY_OK");
assert_eq!(ARP_TEARDOWN_REVOKED_MARKER, "PYTHOS:CORE:ARP:TEARDOWN_REVOKED");
assert_eq!(ARP_READY_MARKER, "PYTHOS:CORE:ARP_READY");
```

Add manifest assertions for `b"arp-probe.elf"` and `0x5059_4152_5052_0001`.

- [ ] **Step 2: Run the focused shared tests and verify the new symbols fail to compile.**

Run:

```text
cargo test -p pythos-shared arp_markers
```

Expected: FAIL because the additive module/constants do not exist yet.

- [ ] **Step 3: Add the marker module and named-program constants.** Define the six marker constants in `shared/src/arp_markers.rs`, export the module from `shared/src/lib.rs`, and add:

```rust
pub const ARP_PROBE_PROGRAM_NAME: &[u8] = b"arp-probe.elf";
pub const ARP_PROBE_PRINCIPAL_ID: u64 = 0x5059_4152_5052_0001;
```

Do not alter existing NetworkPort or link-layer identities.

- [ ] **Step 4: Write ADR 0097 from the accepted spec.** Record status `Accepted by owner on 2026-09-16; implementation follows the separately accepted plan`, the separate native consumer decision, exact MAC/IP/ARP fields, marker order, capability/lifecycle reuse, exact QEMU evidence, and every Phase 15/later-protocol non-claim. State that `VirtioTransport` fulfills the Virtio driver role without making “driver” a PythOS architectural noun.

- [ ] **Step 5: Run tests, formatting, and commit the documentation/identity unit.**

Run:

```text
cargo test -p pythos-shared
cargo fmt --all -- --check
git diff --check
```

Expected: PASS. Commit:

```text
git add docs/decisions/0097-phase-14-arp-consumer.md shared/src/arp_markers.rs shared/src/lib.rs shared/src/user_program_manifest.rs
git commit -m "docs(net): record ARP consumer decision"
```

### Task 2: Add the bounded ARP codec with unit tests

**Files:**
- Create: `user/probes/arp/Cargo.toml`
- Create: `user/probes/arp/linker.ld`
- Create: `user/probes/arp/src/lib.rs`
- Create: `user/probes/arp/src/arp.rs`
- Modify: `Cargo.toml`
- Test: `user/probes/arp/src/arp.rs`

**Interfaces:**
- Consumes: byte slices and fixed arrays only; no kernel, `NetworkPort`, or transport symbols.
- Produces: `arp::{ArpPacket, ParseError, ARP_PAYLOAD_BYTES, parse, encode}` and the package `pythos-user-arp-probe` for the native consumer and CI.

- [ ] **Step 1: Add the package manifest and failing codec tests.** Define the workspace package with `pythos-shared` and the existing `pythos-user-link-layer-probe` library as its only dependencies, a library at `src/lib.rs`, and binary `pythos-user-arp-probe` at `src/main.rs`. Add the package to the workspace. Write tests for these exact cases before implementing `arp.rs`:

```rust
#[test]
fn parse_decodes_network_byte_order_and_all_addresses() { /* 28-byte fixture -> exact fields */ }

#[test]
fn encode_writes_the_28_byte_wire_layout() { /* exact expected byte array */ }

#[test]
fn parse_rejects_a_payload_shorter_than_28_bytes() { /* ParseError::TooShort */ }

#[test]
fn parse_ignores_ethernet_padding_after_the_28_byte_payload() { /* 28 bytes plus 18 padding */ }
```

- [ ] **Step 2: Run the new package tests and verify the codec tests fail.**

Run:

```text
cargo test -p pythos-user-arp-probe
```

Expected: FAIL because `ArpPacket`, `parse`, and `encode` are not defined.

- [ ] **Step 3: Implement the minimal pure codec.** Define:

```rust
pub const ARP_PAYLOAD_BYTES: usize = 28;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArpPacket {
    pub hardware_type: u16,
    pub protocol_type: u16,
    pub hardware_len: u8,
    pub protocol_len: u8,
    pub operation: u16,
    pub sender_hardware: [u8; 6],
    pub sender_protocol: [u8; 4],
    pub target_hardware: [u8; 6],
    pub target_protocol: [u8; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError { TooShort }

pub fn parse(payload: &[u8]) -> Result<ArpPacket, ParseError>;
pub fn encode(packet: ArpPacket) -> [u8; ARP_PAYLOAD_BYTES];
```

Read multi-byte fields with `u16::from_be_bytes`, write them with
`to_be_bytes`, copy fixed address ranges, and do not allocate or retain a
borrowed payload. `parse` may accept a longer Ethernet payload because the
parent parser has already bounded the frame and the bytes after offset 28 are
Ethernet padding.

- [ ] **Step 4: Add the exact codec fixtures and complete the unit tests.** Use hardware type `1`, protocol type `0x0800`, lengths `6`/`4`, operation `1`, sender MAC `02:00:00:00:00:01`, sender IPv4 `192.0.2.2`, zero target MAC, and target IPv4 `192.0.2.1`. Assert all 28 bytes, network byte order, and the short-body error.

- [ ] **Step 5: Build and commit the isolated codec.**

Run:

```text
cargo test -p pythos-user-arp-probe
cargo fmt --all -- --check
cargo build -p pythos-user-arp-probe --target x86_64-unknown-none
```

Expected: PASS. Commit:

```text
git add Cargo.toml user/probes/arp
git commit -m "feat(net): add bounded ARP codec"
```

### Task 3: Implement the finite native ARP consumer

**Files:**
- Modify: `user/probes/arp/src/lib.rs`
- Create: `user/probes/arp/src/main.rs`
- Test: `user/probes/arp/src/main.rs` unit tests

**Interfaces:**
- Consumes: `arp::{ArpPacket, encode, parse}`, `pythos_user_link_layer_probe::ethernet`, frozen `NetworkPort` records/syscall, `pythos_shared::arp_markers`, and `ARP_PROBE_*` manifest constants.
- Produces: a static `pythos-user-arp-probe` ELF that emits the consumer markers and terminates through the existing returnable-user-process `int3` path.

- [ ] **Step 1: Write failing policy tests.** Add host-side unit tests in the no-`std` crate for:

```rust
#[test]
fn request_policy_uses_broadcast_and_zero_target_hardware() { /* exact ArpPacket */ }

#[test]
fn reply_policy_accepts_only_the_exact_peer_relationship() { /* exact positive */ }

#[test]
fn reply_policy_rejects_wrong_operation_sender_target_or_address_pair() { /* each mutation */ }

#[test]
fn policy_rejects_non_ethernet_or_non_ipv4_arp_fields() { /* type/length mutations */ }
```

- [ ] **Step 2: Run the focused probe tests and verify policy symbols fail.**

Run:

```text
cargo test -p pythos-user-arp-probe
```

Expected: FAIL because the consumer policy functions and fixed values do not exist.

- [ ] **Step 3: Add the native entry point and fixed proof policy.** Use `#![cfg_attr(not(test), no_std)]` and `#![cfg_attr(not(test), no_main)]`, a single `ProbeStorage` with the existing description/request/response buffers, a 60-byte TX buffer, and a 1514-byte RX buffer. Add these constants:

```rust
const BROADCAST_MAC: [u8; 6] = [0xff; 6];
const PEER_MAC: [u8; 6] = [2, 0, 0, 0, 0, 2];
const LOCAL_IPV4: [u8; 4] = [192, 0, 2, 2];
const PEER_IPV4: [u8; 4] = [192, 0, 2, 1];
const ARP_ETHER_TYPE: u16 = 0x0806;
```

Implement `valid_bootstrap`, `describe`, `valid_description_fields`, `request_packet`, `reply_matches`, `send_request`, `receive`, `valid_response`, `request`, `write_marker`, `success_breakpoint`, and the terminal `error` path. Reuse the established x86-64 syscall assembly shape and capability/status validation.

- [ ] **Step 4: Implement the exact consumer sequence.** `_start` must validate the bootstrap and emit `ARP_BOOTSTRAPPED_MARKER`; issue `DESCRIBE`, validate frame bounds `60..=1514`, flags `MAC_ONLY | NO_OFFLOAD`, state `READY`, reserved fields, and emit `ARP_DESCRIBE_OK_MARKER`; construct and send one 60-byte request, require `NETWORK_PORT_STATUS_OK`, and emit `ARP_REQUEST_OK_MARKER`; spin only on `EMPTY`, parse the bounded Ethernet frame and first 28 ARP payload bytes, require exact local destination/peer source/`0x0806` and `reply_matches`, then emit `ARP_REPLY_OK_MARKER` and invoke `int3`.

Do not put a transmit branch behind initialization state: the consumer is admitted only after `Operational`, so its one transmit is steady-state `NetworkPort SEND`. Do not retry or retain the reply.

- [ ] **Step 5: Document every unsafe invariant and finish unit tests.** Document that the bootstrap read is from the aligned read-only page supplied by PythCore, `ProbeStorage` is single-threaded, fixed buffers are exclusively accessed between syscalls, the returned frame length is bounded before slicing, and the terminal assembly paths do not return. Test short Ethernet frames through the reused parser, nonzero/unsupported ARP fields, wrong operation, wrong MAC/IP relationships, and exact marker-triggering policy.

- [ ] **Step 6: Verify the native consumer and commit it.**

Run:

```text
cargo test -p pythos-user-arp-probe
cargo fmt --all -- --check
cargo build -p pythos-user-arp-probe --target x86_64-unknown-none
cargo clippy -p pythos-user-arp-probe --target x86_64-unknown-none -- -D warnings
```

Expected: PASS. Commit:

```text
git add user/probes/arp/src/lib.rs user/probes/arp/src/main.rs
git commit -m "feat(net): add native ARP consumer"
```

### Task 4: Wire the ARP launch wrapper without changing NetworkPort semantics

**Files:**
- Create: `core/src/arp_probe.rs`
- Modify: `core/Cargo.toml`
- Modify: `core/src/main.rs`
- Modify: `core/src/network_port.rs`
- Modify: `core/src/network_port_probe_support.rs`
- Modify: `core/src/syscall.rs`
- Test: `core/src/arp_probe.rs` and `cargo test -p pythos-core`

**Interfaces:**
- Consumes: `NamedNetworkPortLaunch`, `PreparedNetworkPortLaunch`, `run_user_then_restore`, `write_bootstrap`, `initialize_transport`, `install_operational_transport`, existing NetworkPort capability helpers, `ARP_PROBE_*` constants, and the accepted bootstrap address.
- Produces: feature `arp-probe`, `ArpProbeLaunchContract`, `prepare`, and `run` with the exact six-marker contract.

- [ ] **Step 1: Write the failing launch-contract tests.** Define tests that require:

```rust
let contract = ArpProbeLaunchContract::new();
assert_eq!(contract.bootstrap_user_ptr(), 0x0000_0000_7300_0000);
assert!(!contract.bootstrap_writable());
assert_eq!(contract.accepted_markers(), [
    ARP_BOOTSTRAPPED_MARKER,
    ARP_DESCRIBE_OK_MARKER,
    ARP_REQUEST_OK_MARKER,
    ARP_REPLY_OK_MARKER,
    ARP_TEARDOWN_REVOKED_MARKER,
    ARP_READY_MARKER,
]);
```

- [ ] **Step 2: Run the core contract test and verify the wrapper is absent.**

Run:

```text
cargo test -p pythos-core arp_probe
```

Expected: FAIL because `ArpProbeLaunchContract` and the `arp-probe` feature wiring do not exist.

- [ ] **Step 3: Add the feature and wrapper contract.** Add `arp-probe = ["verify"]` to `core/Cargo.toml`. Implement `core/src/arp_probe.rs` with consumer service `0x5059_4152_4353_0001`, owner service `0x5059_4152_4F57_0001`, `ArpProbeError`, `minimal_kernel_address_space_options`, `prepare`, `run`, and `ArpProbeLaunchContract`.

`prepare` must call `prepare_named` with `ARP_PROBE_PROGRAM_NAME`, `ARP_PROBE_PRINCIPAL_ID`, and the consumer service id. `run` must re-load and revalidate the named manifest, principal, ELF entry, and segment count before initializing the existing legacy `VirtioTransport`, installing the existing operational `NetworkPort`, granting the existing consumer/owner capabilities, writing the read-only bootstrap, launching exactly one native consumer, restoring the kernel root, resetting through the owner capability, requiring `NETWORK_PORT_STATUS_OK` plus `NETWORK_PORT_STATE_RESET`, requiring consumer revocation, and emitting teardown/readiness markers.

- [ ] **Step 4: Add only the required cfg wiring.** Add `arp-probe` to the existing feature sets that currently include `network-port-probe` or `link-layer-probe` in `core/src/main.rs`, `core/src/network_port.rs`, `core/src/network_port_probe_support.rs`, and `core/src/syscall.rs`. This includes the early-exit warning, mutual-exclusion checks, module declarations, shared NetworkPort support, minimal address-space selection, address-space feature exclusions, prepare/run branches, normal-boot exclusions, and syscall/capability/transport availability. Add explicit mutual exclusion between `arp-probe` and every other opt-in NetworkPort probe (`virtio-net-probe`, `network-port-probe`, and `link-layer-probe`) plus existing incompatible verify probes.

Do not alter the NetworkPort request/response types, operation values, rights, transport initialization, or link-layer cfg behavior beyond adding the new opt-in feature to the same existing sets.

- [ ] **Step 5: Verify core unit/build gates and commit the wrapper.**

Run:

```text
cargo test -p pythos-core
cargo fmt --all -- --check
cargo build -p pythos-core --target x86_64-unknown-none --no-default-features --features arp-probe
cargo build -p pythos-core --target x86_64-unknown-none --no-default-features --features link-layer-probe
```

Expected: PASS, including the unchanged link-layer feature build. Commit:

```text
git add core/Cargo.toml core/src/arp_probe.rs core/src/main.rs core/src/network_port.rs core/src/network_port_probe_support.rs core/src/syscall.rs
git commit -m "feat(net): wire opt-in ARP probe"
```

### Task 5: Build and package the named ARP ELF explicitly

**Files:**
- Create: `scripts/build-arp-probe.py`
- Modify: `scripts/build-image.py`
- Test: `tests/test_build_orchestration.py`

**Interfaces:**
- Consumes: `user/probes/arp/linker.ld`, named manifest constants, `verify-user-elf.py`, and existing `build_default_init_pak` selectors.
- Produces: isolated `target/arp-probe` build output, `--arp-probe-elf`, and one named `arp-probe.elf` INIT.PAK record only when explicitly selected.

- [ ] **Step 1: Write failing build and packaging tests.** Add tests that assert:

```text
cargo build -p pythos-user-arp-probe --target x86_64-unknown-none --bin pythos-user-arp-probe --target-dir <target>
```

is invoked with static relocation and `user/probes/arp/linker.ld`; the opt-in named record has name `b"arp-probe.elf"`, principal `0x5059_4152_5052_0001`, and the exact ELF digest; default INIT.PAK bytes contain no ARP record; verification happens before packaging; and any two of `network_port_probe_elf`, `link_layer_probe_elf`, and `arp_probe_elf` are rejected before ESP mutation.

- [ ] **Step 2: Run the focused orchestration tests and verify the new selector fails.**

Run:

```text
python -m unittest tests.test_build_orchestration
```

Expected: FAIL because the build helper, image argument, and packaging selector do not exist.

- [ ] **Step 3: Implement the isolated builder and image selector.** Make `build-arp-probe.py` invoke exactly one Cargo build with default target directory `ROOT / "target" / "arp-probe"`, target `x86_64-unknown-none`, binary `pythos-user-arp-probe`, static relocation, and the ARP linker script. In `build-image.py`, add `arp_probe_elf` to `build_default_init_pak`, add resolve/verify helpers and `--arp-probe-elf`, append one named record after the shell when selected, and reject multiple NetworkPort/link-layer/ARP probe selections before writing the ESP.

Do not add ARP selectors to `scripts/build-iso.py`; the accepted QEMU probe path uses the snapshot-backed ESP/image builder.

- [ ] **Step 4: Verify packaging order, conflicts, default bytes, and commit.**

Run:

```text
python -m unittest tests.test_build_orchestration
python -m py_compile scripts/build-arp-probe.py scripts/build-image.py
cargo build -p pythos-user-arp-probe --target x86_64-unknown-none
```

Expected: PASS. Commit:

```text
git add scripts/build-arp-probe.py scripts/build-image.py tests/test_build_orchestration.py
git commit -m "build(net): package ARP probe explicitly"
```

### Task 6: Add the exact QEMU ARP peer and acceptance oracle

**Files:**
- Create: `scripts/test-arp.py`
- Test: `scripts/test-arp.py --self-test` and `tests/test_arp.py`

**Interfaces:**
- Consumes: `qemu_probe_support`, `run-qemu.py`, `test-virtio-net.py` socket framing helpers, the ARP builder, `build-image.py`, and the six shared marker literals.
- Produces: `ARP_QEMU_ACCEPTANCE_OK`, exact frame helpers, a loopback-only one-request/one-reply peer, and live QEMU evidence.

- [ ] **Step 1: Write failing host-oracle tests.** Define `tests/test_arp.py` with assertions for:

```python
EXPECTED_MARKERS = (
    "PYTHOS:CORE:ARP:BOOTSTRAPPED",
    "PYTHOS:CORE:ARP:DESCRIBE_OK",
    "PYTHOS:CORE:ARP:REQUEST_OK",
    "PYTHOS:CORE:ARP:REPLY_OK",
    "PYTHOS:CORE:ARP:TEARDOWN_REVOKED",
    "PYTHOS:CORE:ARP_READY",
)
```

Also test exact request/reply bytes, wrong local MAC rejection, extra TX rejection, missing/reordered/duplicate markers, non-success runner output, required `--virtio-net`/`--virtio-net-peer-port`, and required `--no-virtio-blk`.

- [ ] **Step 2: Run the host tests and verify the acceptance module is absent.**

Run:

```text
python -m unittest tests.test_arp
```

Expected: FAIL because the ARP acceptance script and exact peer functions do not exist.

- [ ] **Step 3: Implement exact wire helpers.** Define `ARP_ETHER_TYPE = 0x0806`, `PEER_MAC = bytes.fromhex("020000000002")`, `LOCAL_IPV4 = bytes.fromhex("c0000202")`, and `PEER_IPV4 = bytes.fromhex("c0000201")`. Implement a host `arp_payload` using `struct.pack("!HHBBH6s4s6s4s", ...)`, an Ethernet `arp_frame` that creates exactly 60 bytes with zero padding, `assert_exact_arp_request(device_mac, frame)`, and `peer_reply(device_mac)`.

The request validator must require broadcast destination, described local source, EtherType `0x0806`, operation `1`, local/peer addresses, and zero target MAC. The reply validator must require described local destination, peer source, EtherType `0x0806`, operation `2`, peer/local addresses, and the exact target MAC. The host peer must read one TX frame, reject EOF/extra TX data, send one reply, then require clean peer completion.

- [ ] **Step 4: Implement the QEMU runner and oracle.** Build loader, core with `--no-default-features --features arp-probe`, isolated ARP ELF, verified shell ELF, and image with `--arp-probe-elf`. Require all expected artifacts before QEMU. Launch `run-qemu.py` with `--no-audio-device --no-virtio-blk --virtio-net --virtio-net-peer-port <peer> --shell-port <shell> --expect-outcome success`, drain COM1/COM2 and runner output, finalize after runner exit, and assert the six consumer/kernel events plus `QEMU_OUTCOME success` through one `AcceptanceTimeline`.

Reject `PYTHOS:CORE:ARP:ERROR`, `PYTHOS:PANIC`, any storage marker, missing/duplicate/reordered markers, additional frames, timeout, transport error, nonzero runner exit, and any QEMU outcome other than success. Keep process-tree cleanup and abortive socket close routed through the existing portable helper so Windows and Linux use the same cleanup contract.

- [ ] **Step 5: Complete self-tests and commit the host oracle.**

Run:

```text
python scripts/test-arp.py --self-test
python -m unittest tests.test_arp
python -m py_compile scripts/test-arp.py
```

Expected: PASS. Commit:

```text
git add scripts/test-arp.py tests/test_arp.py
git commit -m "test(net): add deterministic ARP acceptance oracle"
```

### Task 7: Add strict CI gates and preserve all predecessor proofs

**Files:**
- Modify: `.github/workflows/qemu-acceptance.yml`
- Modify: `tests/test_ci_workflow.py`
- Modify: `tests/test_arp.py` only if the workflow contract tests require a shared literal
- Test: `tests/test_ci_workflow.py`

**Interfaces:**
- Consumes: the new ARP package, builder, host oracle, core feature, and existing sequential Phase 14 gates.
- Produces: hosted strict format/test/clippy/script/QEMU coverage with ARP after the link-layer proof and all existing lower-layer gates retained.

- [ ] **Step 1: Write failing workflow contract assertions.** Require exactly once, in the existing sections and order:

```text
cargo test -p pythos-user-arp-probe
cargo clippy -p pythos-core --target x86_64-unknown-none --features arp-probe -- -D warnings
cargo clippy -p pythos-user-arp-probe --target x86_64-unknown-none -- -D warnings
python -m py_compile scripts/build-arp-probe.py scripts/test-arp.py
python -m unittest tests.test_arp
python scripts/test-arp.py --self-test
python scripts/test-arp.py
```

Assert the live ARP gate follows the live link-layer gate and the raw Virtio/NetworkPort gates remain before both. Assert default and normal-session gates remain present.

- [ ] **Step 2: Run the workflow contract tests and verify the commands are missing.**

Run:

```text
python -m unittest tests.test_ci_workflow
```

Expected: FAIL because the workflow has no ARP commands.

- [ ] **Step 3: Add additive CI commands.** Add the ARP Rust unit test beside the existing native probe tests; add ARP core/native Clippy beside the link-layer profiles; extend script compilation with both ARP scripts; add `tests.test_arp` to Python harness tests; and add `python scripts/test-arp.py --self-test` followed by the live ARP command after the link-layer commands. Do not remove or reorder predecessor network gates.

- [ ] **Step 4: Verify the workflow contract and all local non-QEMU gates.**

Run:

```text
python -m unittest tests.test_ci_workflow tests.test_arp tests.test_build_orchestration
cargo fmt --all -- --check
cargo test -p pythos-shared
cargo test -p pythos-core
cargo test -p pythos-user-link-layer-probe
cargo test -p pythos-user-arp-probe
cargo clippy -p pythos-core --target x86_64-unknown-none --features arp-probe -- -D warnings
cargo clippy -p pythos-user-arp-probe --target x86_64-unknown-none -- -D warnings
python scripts/test-arp.py --self-test
```

Expected: PASS. Commit:

```text
git add .github/workflows/qemu-acceptance.yml tests/test_ci_workflow.py
git commit -m "ci(net): gate the ARP consumer proof"
```

### Task 8: Run the full serial QEMU proof and close the Phase 14 slice

**Files:**
- Modify: `docs/decisions/0097-phase-14-arp-consumer.md`
- Modify: `docs/ROADMAP.md`
- Modify: `docs/HANDOVER.md`
- Test: all raw Virtio, NetworkPort, link-layer, ARP, default, and normal-session profiles

**Interfaces:**
- Consumes: completed code/CI tasks, exact ARP marker/frame contracts, and accepted Phase 14 predecessor evidence.
- Produces: reproducible ARP acceptance evidence and a documented handoff to the next Phase 14 boundary, IP, without starting it.

- [ ] **Step 1: Run the new ARP proof locally.**

Run:

```text
python scripts/test-arp.py --self-test
python scripts/test-arp.py
```

Expected: the peer observes one exact 60-byte broadcast request, sends one exact 60-byte reply, COM2 contains the six consumer markers exactly once in order, COM1 contains teardown/readiness markers, no storage markers appear, the runner reports `QEMU_OUTCOME success`, and the process tree is reaped.

- [ ] **Step 2: Run predecessor and regression profiles serially.**

Run:

```text
python scripts/test-virtio-net.py --self-test
python scripts/test-virtio-net.py
python scripts/test-network-port.py --self-test
python scripts/test-network-port.py
python scripts/test-link-layer.py --self-test
python scripts/test-link-layer.py
python scripts/test-pyth-default-boot.py
python scripts/test-normal-session.py --self-test
python scripts/test-normal-session.py
```

Expected: every predecessor profile passes unchanged; default and normal-session boots do not launch the ARP consumer.

- [ ] **Step 3: Run the complete local quality gate.**

Run:

```text
cargo fmt --all -- --check
cargo test --workspace
python -m unittest discover -s tests
python scripts/test-arp.py --self-test
python scripts/test-arp.py
```

Expected: PASS with no formatter, Rust, Python, marker, storage, or QEMU failures.

- [ ] **Step 4: Record the accepted evidence and next boundary.** Update ADR 0097 with the exact local/hosted evidence identifiers and final marker/frame proof. Update `docs/ROADMAP.md` and `docs/HANDOVER.md` to state that Phase 14 ARP is accepted, raw bytes remain below `NetworkPort`, ARP semantics live in the native consumer, IP is the next Phase 14 design boundary, and Phase 15 hardware remains separate. Do not claim physical networking or production ARP readiness.

- [ ] **Step 5: Review the final diff and commit the closeout.**

Run:

```text
git diff --check
git status --short
git log --oneline -8
```

Expected: only the planned ARP implementation, tests, CI, and closeout documentation are present; no `NetworkPort` ABI or Phase 15 files changed. Commit:

```text
git add docs/decisions/0097-phase-14-arp-consumer.md docs/ROADMAP.md docs/HANDOVER.md
git commit -m "docs(net): close Phase 14 ARP proof"
```

## Plan Self-Review Checklist

- Spec coverage: Tasks 1 and 8 cover the ADR/status/evidence; Tasks 2 and 3 cover the 28-byte codec, Ethernet-II envelope, exact request/reply, padding, byte order, and terminal policy; Task 4 covers the existing bootstrap/capability/lifecycle; Task 5 covers named packaging; Task 6 covers exact QEMU frames and markers; Task 7 covers strict CI and predecessor regressions.
- Scope coverage: no task adds ARP cache/state, retry/timer behavior, IP or higher protocols, physical NIC/Wi-Fi, modern/interrupt Virtio, multiqueue/offloads, sockets, multiple consumers, zero-copy, persistent state, PythTIG changes, or a new `NetworkPort` ABI.
- Type consistency: `ARP_PROBE_PROGRAM_NAME`/`ARP_PROBE_PRINCIPAL_ID` feed the named manifest, builder, core wrapper, and acceptance packaging; `arp::{ArpPacket, parse, encode}` feed the native policy; the six shared markers feed the native process, core launch contract, host oracle, and CI contract.
- Ordering consistency: the native process emits bootstrap, describe, request, and reply markers; core emits teardown and ready markers after user return and revocation; the host requires those six markers exactly once in order and separately validates the peer's one-request/one-reply exchange.
- Completeness scan: the plan contains no unfinished-marker terms or unspecified implementation step. Every test step names a command and expected result, and every code unit has an explicit file/interface boundary.
