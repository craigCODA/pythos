# Phase 14 IPv4 Exchange Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement and verify the bounded IPv4 codec and one opt-in QEMU IPv4 request/reply proof above the accepted `NetworkPort` and ARP boundaries.

**Architecture:** Keep `VirtioTransport` as the privileged transport adapter and keep `NetworkPort` as the unchanged capability-scoped copied-Ethernet boundary. Add a pure no-allocation IPv4 codec and a separately named native `ipv4-probe.elf` consumer; the consumer performs one ARP setup exchange followed by one exact IPv4 exchange and then terminates.

**Tech Stack:** Rust 2024/no_std native probe, existing PythOS syscall and capability ABI, Python QEMU acceptance harness, Cargo workspace tests, Python unittest, GitHub Actions QEMU gates.

**Spec:** `docs/superpowers/specs/2026-09-16-phase-14-ipv4-design.md`

## Global Constraints

- Preserve `VirtioTransport`, transport adapter, and `NetworkPort` as the PythOS architectural names.
- Do not modify the `NetworkPort` ABI, capability rights, syscall numbers, bootstrap layout, PythTIG v1, or transport lifecycle.
- Use local IPv4 `192.168.14.2`, peer IPv4 `192.168.14.1`, peer MAC `02:00:00:00:00:02`, EtherType `0x0800`, Protocol `253`, request ID `0x1401`, reply ID `0x1402`, TTL `64`, and 60-byte Ethernet frames excluding FCS.
- Encode request payload `50 59 54 48 49 50 52 51` (`PYTHIPRQ`) and reply payload `50 59 54 48 49 50 52 50` (`PYTHIPRP`), with 18 zero Ethernet-padding bytes.
- Keep the IPv4 profile at IHL 5, total length 28, no options, and zero flags/fragment offset; reject fragments and options in the proof policy.
- Use Protocol value `253` only in the explicitly enabled QEMU probe; never enable it in default or normal-session boot.
- Do not add ICMP, UDP, TCP, DNS, DHCP, sockets, routing, forwarding, persistent network state, physical NIC/Wi-Fi support, modern Virtio, interrupts, multiqueue, offloads, zero-copy, or Phase 15 hardware abstractions.
- Keep the proof opt-in and mutually exclusive with the existing network probes, and retain raw Virtio, `NetworkPort`, link-layer, ARP, default-boot, and normal-session regressions.
- No implementation step may expose PCI, MMIO, queue, DMA, Virtio-header, or completion state to the native consumer.

---

## File Map

Create:

- `user/probes/ipv4/Cargo.toml` — native IPv4 probe package metadata and dependencies.
- `user/probes/ipv4/linker.ld` — probe linker layout matching the accepted ARP probe.
- `user/probes/ipv4/src/lib.rs` — library exports for codec and test-only probe helpers.
- `user/probes/ipv4/src/ipv4.rs` — bounded IPv4 header codec and checksum implementation.
- `user/probes/ipv4/src/main.rs` — no_std native consumer, NetworkPort calls, ARP setup, IPv4 exchange, and markers.
- `core/src/ipv4_probe.rs` — privileged opt-in launch/teardown wrapper, analogous to `core/src/arp_probe.rs`.
- `shared/src/ipv4_markers.rs` — exact shared marker constants.
- `scripts/build-ipv4-probe.py` — isolated ELF build script.
- `scripts/test-ipv4.py` — Python self-test, QEMU peer, frame oracle, and live acceptance runner.
- `tests/test_ipv4.py` — Python harness and packet-policy unit tests.
- `docs/decisions/0098-phase-14-ipv4-consumer.md` — accepted implementation/evidence ADR after the proof passes.

Modify:

- `Cargo.toml` — add the IPv4 probe workspace member.
- `core/Cargo.toml` — add the opt-in `ipv4-probe` feature.
- `core/src/main.rs` — add IPv4 feature exclusions, module declarations, and launch branch.
- `core/src/network_port.rs`, `core/src/syscall.rs`, and `core/src/virtio_net.rs` — include `ipv4-probe` in existing feature-gated NetworkPort/transport availability lists only. The `syscall.rs` change is strictly cfg plumbing for the existing ABI paths; it must not add syscall numbers, layouts, behavior, or architecture.
- `shared/src/lib.rs` and `shared/src/user_program_manifest.rs` — export markers and freeze the additive program name/principal.
- `scripts/build-image.py` — accept exactly one IPv4 probe ELF and package it as `ipv4-probe.elf`.
- `tests/test_build_orchestration.py` — cover IPv4 build arguments, manifest identity, and mutual exclusion.
- `.github/workflows/qemu-acceptance.yml` and `tests/test_ci_workflow.py` — add ordered IPv4 unit/build/live gates after ARP.
- `README.md`, `docs/ROADMAP.md`, `docs/HANDOVER.md`, `docs/TECHNICAL-OVERVIEW.md` — record only the accepted IPv4 proof after hosted evidence exists.

## Task 1: Add the shared IPv4 identity and marker contract

**Files:**
- Create: `shared/src/ipv4_markers.rs`
- Modify: `shared/src/lib.rs`
- Modify: `shared/src/user_program_manifest.rs`

**Interfaces:**
- Produces `IPV4_PROBE_PROGRAM_NAME = b"ipv4-probe.elf"` and `IPV4_PROBE_PRINCIPAL_ID = 0x5059_4950_5052_0001`.
- Produces markers, in order: `PYTHOS:CORE:IPV4:BOOTSTRAPPED`, `PYTHOS:CORE:IPV4:DESCRIBE_OK`, `PYTHOS:CORE:IPV4:ARP_SETUP_OK`, `PYTHOS:CORE:IPV4:TX_OK`, `PYTHOS:CORE:IPV4:RX_OK`, `PYTHOS:CORE:IPV4:TEARDOWN_REVOKED`, `PYTHOS:CORE:IPV4_READY`.

- [ ] **Step 1: Write the failing shared-contract tests**

Add tests matching the existing `arp_markers` and `user_program_manifest` tests. Assert every marker's exact spelling/order, the exact program name/principal, and inequality with the existing network-port, link-layer, and ARP identities.

- [ ] **Step 2: Run the focused shared tests**

Run: `cargo test -p pythos-shared ipv4`

Expected: FAIL because the new module/constants do not exist.

- [ ] **Step 3: Implement the constants and exports**

Add the marker constants and manifest constants, export the marker module from `shared/src/lib.rs`, and include the new identity in manifest validation tests without changing any existing identity.

- [ ] **Step 4: Run the focused tests and formatting**

Run: `cargo fmt --all -- --check; cargo test -p pythos-shared ipv4`

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add shared/src/lib.rs shared/src/ipv4_markers.rs shared/src/user_program_manifest.rs
git commit -m "feat(net): add IPv4 probe identity contract"
```

## Task 2: Implement the bounded IPv4 codec with unit coverage

**Files:**
- Create: `user/probes/ipv4/Cargo.toml`
- Create: `user/probes/ipv4/linker.ld`
- Create: `user/probes/ipv4/src/lib.rs`
- Create: `user/probes/ipv4/src/ipv4.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces `Ipv4Header`, `Ipv4Packet<'a>`, `EncodeError`, `DecodeError`, `encode_ipv4_packet`, `decode_ipv4_packet`, and `ipv4_header_checksum` from `pythos_user_ipv4_probe::ipv4`.
- `encode_ipv4_packet(header: Ipv4Header, payload: &[u8], output: &mut [u8]) -> Result<usize, EncodeError>` writes one canonical IPv4 datagram without allocation.
- `decode_ipv4_packet(datagram: &[u8]) -> Result<Ipv4Packet<'_>, DecodeError>` checks bounds/version/IHL/total length/checksum and returns a borrowed payload view.

- [ ] **Step 1: Add codec tests before implementation**

In `user/probes/ipv4/src/ipv4.rs`, add tests for: exact request header bytes with checksum `C890`; exact reply header bytes with checksum `C88F`; round-trip field/payload decoding; short headers; version other than 4; IHL below 5; header length beyond input; total length below header length; total length beyond input; bad checksum; output too small; payload length overflow; and nonzero options rejected by the fixed canonical encoder.

- [ ] **Step 2: Run the new crate tests to verify failure**

Run: `cargo test -p pythos-user-ipv4-probe`

Expected: FAIL because the workspace member and codec types are not implemented.

- [ ] **Step 3: Add the minimal no_std crate scaffold**

Copy only the accepted probe crate layout into `user/probes/ipv4`, use the existing `x86_64-unknown-none` linker layout, and add dependencies on `pythos-shared`, `pythos-user-link-layer-probe`, and `pythos-user-arp-probe` without changing those crates.

- [ ] **Step 4: Implement the codec**

Use checked arithmetic and network byte order. The encoder writes `0x45`, total length `20 + payload.len()`, the supplied header fields, a zero checksum while summing 16-bit words, and the one's-complement checksum. The decoder requires at least 20 bytes, version 4, IHL at least 5, a fitting header, total length between header length and input length, and a valid one's-complement checksum; it returns only the bytes within the declared total length.

- [ ] **Step 5: Run the codec tests and formatting**

Run: `cargo fmt --all -- --check; cargo test -p pythos-user-ipv4-probe`

Expected: PASS with all codec rejection cases covered.

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml user/probes/ipv4
git commit -m "feat(net): add bounded IPv4 codec"
```

## Task 3: Implement the native IPv4 proof consumer

**Files:**
- Modify: `user/probes/ipv4/src/main.rs`

**Interfaces:**
- Consumes the existing `NetworkPortBootstrapV1`, `NetworkPortRequestV1`, `NetworkPortResponseV1`, `DESCRIBE`, `SEND`, and `TRY_RECEIVE` ABI unchanged.
- Uses ARP codec helpers from `pythos-user-arp-probe::arp` and Ethernet helpers from `pythos-user-link-layer-probe::ethernet`.
- Emits only the seven shared IPv4 markers and terminal error markers on failure.

- [ ] **Step 1: Add failing policy tests**

Add tests for exact private-address ARP request/reply matching, exact 60-byte Ethernet/IP request construction, exact 60-byte reply matching, zero padding, wrong MACs, wrong EtherType, wrong IPv4 addresses, wrong Protocol, nonzero fragment fields, wrong IDs, wrong payload, and bad checksum.

- [ ] **Step 2: Run the focused probe tests to verify failure**

Run: `cargo test -p pythos-user-ipv4-probe`

Expected: FAIL because the policy helpers and `_start` flow are not implemented.

- [ ] **Step 3: Implement bootstrap, describe, and bounded buffers**

Mirror the accepted ARP probe's fixed single-threaded storage and syscall wrapper. Validate the read-only bootstrap, ABI version, reserved fields, nonzero capability, description frame limits, MAC-only/no-offload flags, and ready state before emitting `IPV4:DESCRIBE_OK`.

- [ ] **Step 4: Implement the one-shot ARP setup**

Build exactly one 60-byte broadcast ARP request using local `192.168.14.2`, target `192.168.14.1`, the described local MAC, and zero target hardware. Poll nonblocking receive until one exact peer reply arrives; reject all other relationships as terminal probe failure. Emit `IPV4:ARP_SETUP_OK` only after the exact reply.

- [ ] **Step 5: Implement exact IPv4 TX/RX**

Build a 20-byte IPv4 header plus the eight-byte `PYTHIPRQ` payload, wrap it in Ethernet-II with EtherType `0x0800`, append 18 zero bytes, and send exactly one frame. Accept exactly one peer frame whose header has ID `0x1402`, reverse addresses, TTL `64`, Protocol `253`, checksum `C88F`, and payload `PYTHIPRP`; reject fragments/options/nonzero padding. Emit `TX_OK`, `RX_OK`, then terminate through the existing owner revocation path.

- [ ] **Step 6: Run unit tests and clippy**

Run: `cargo fmt --all -- --check; cargo test -p pythos-user-ipv4-probe; cargo clippy -p pythos-user-ipv4-probe --target x86_64-unknown-none -- -D warnings`

Expected: PASS with no warnings promoted to errors.

- [ ] **Step 7: Commit**

```powershell
git add user/probes/ipv4/src/main.rs
git commit -m "feat(net): add native IPv4 exchange probe"
```

## Task 4: Wire the privileged opt-in launch path

**Files:**
- Create: `core/src/ipv4_probe.rs`
- Modify: `core/Cargo.toml`
- Modify: `core/src/main.rs`
- Modify: `core/src/network_port.rs`
- Modify: `core/src/syscall.rs` (existing-ABI cfg plumbing only)
- Modify: `core/src/virtio_net.rs`

**Interfaces:**
- Produces the `ipv4-probe` Cargo feature, `Ipv4ProbeLaunchContract`, `prepare`, `run`, and the existing owner teardown/revocation proof.
- Uses `NamedNetworkPortLaunch { program_name: IPV4_PROBE_PROGRAM_NAME, principal_id: IPV4_PROBE_PRINCIPAL_ID, consumer_service_id: 0x5059_4950_4353_0001 }` and owner service id `0x5059_4950_4F57_0001`.

- [ ] **Step 1: Add feature/launch contract tests**

Add `Ipv4ProbeLaunchContract` tests for bootstrap pointer `0x0000_0000_7300_0000`, read-only bootstrap, and the exact seven-marker order. Add compile-time feature-conflict tests by extending the existing source-contract assertions for probe mutual exclusion.

- [ ] **Step 2: Run the focused core tests to verify failure**

Run: `cargo test -p pythos-core ipv4_probe`

Expected: FAIL because the feature/module/contract do not exist.

- [ ] **Step 3: Add the feature and module declarations**

Add `ipv4-probe = ["verify"]` to `core/Cargo.toml`. Add `ipv4-probe` to the existing mutually exclusive probe lists and to the `cfg` lists that include `arp-probe` for `arp_probe`, `network_port`, `network_port_probe_support`, `virtio_net`, and the existing NetworkPort syscall helpers in `core/src/syscall.rs`. The syscall change is cfg plumbing only so the unchanged NetworkPort ABI paths compile for the opt-in feature; do not add syscall numbers, layouts, behavior, or architecture. Add the `mod ipv4_probe` declaration.

- [ ] **Step 4: Implement the launch wrapper**

Mirror `core/src/arp_probe.rs`: validate the named program manifest/principal and prepared ELF, initialize the accepted legacy transport, install the operational `NetworkPort`, grant the existing consumer/owner capabilities, write the existing bootstrap, run the user process, tear down with the owner capability, verify `RESET` and consumer revocation, emit the two terminal IPv4 markers, and return typed errors.

- [ ] **Step 5: Add the main boot branch**

Add `#[cfg(feature = "ipv4-probe")]` address-space setup and the `prepare`/`run` path in the same location and ordering as the ARP profile. Ensure default, normal-session, and existing probe profiles do not select IPv4.

- [ ] **Step 6: Run focused core tests and clippy**

Run: `cargo fmt --all -- --check; cargo test -p pythos-core ipv4_probe; cargo clippy -p pythos-core --target x86_64-unknown-none --features ipv4-probe -- -D warnings`

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add core/Cargo.toml core/src/main.rs core/src/network_port.rs core/src/syscall.rs core/src/virtio_net.rs core/src/ipv4_probe.rs
git commit -m "feat(net): wire IPv4 probe launch path"
```

## Task 5: Add build-image and ELF orchestration

**Files:**
- Create: `scripts/build-ipv4-probe.py`
- Modify: `scripts/build-image.py`
- Modify: `tests/test_build_orchestration.py`
- Modify: `.github/workflows/qemu-acceptance.yml`
- Modify: `tests/test_ci_workflow.py`

**Interfaces:**
- `scripts/build-ipv4-probe.py` builds `pythos-user-ipv4-probe` into `target/ipv4-probe/ipv4-probe.elf` using the same verification flow as `build-arp-probe.py`.
- `scripts/build-image.py --ipv4-probe-elf PATH` accepts the IPv4 ELF as the sole network probe and packages the exact name/principal.

- [ ] **Step 1: Add failing orchestration tests**

Extend `tests/test_build_orchestration.py` to assert the build command/package name, `ipv4-probe.elf`, principal `0x5059_4950_5052_0001`, `--ipv4-probe-elf` resolution/verification, and conflict rejection with each existing network probe. Extend `tests/test_ci_workflow.py` with the exact IPv4 crate, clippy, py_compile, Python unit, self-test, and live-gate commands after ARP.

- [ ] **Step 2: Run orchestration tests to verify failure**

Run: `py -3 -m unittest tests.test_build_orchestration tests.test_ci_workflow`

Expected: FAIL because the new argument and workflow commands do not exist.

- [ ] **Step 3: Implement the isolated build script**

Build `pythos-user-ipv4-probe` for `x86_64-unknown-none`, use `user/probes/ipv4/linker.ld`, verify the ELF, and print the artifact path without mutating unrelated image state.

- [ ] **Step 4: Extend image packaging**

Add the IPv4 principal, parameter, resolver, verifier, named ELF record, one-probe conflict count, session-runtime/normal-session conflict checks, and CLI argument. Preserve every existing probe argument and error message contract unless the added IPv4 name is required.

- [ ] **Step 5: Add CI commands and run tests**

Add the IPv4 crate test, core/probe clippy gates, Python compilation, Python harness test, self-test, and live test in milestone order after ARP. Run: `py -3 -m unittest tests.test_build_orchestration tests.test_ci_workflow`.

- [ ] **Step 6: Commit**

```powershell
git add scripts/build-ipv4-probe.py scripts/build-image.py tests/test_build_orchestration.py tests/test_ci_workflow.py .github/workflows/qemu-acceptance.yml
git commit -m "ci(net): add IPv4 probe orchestration"
```

## Task 6: Build the QEMU peer oracle and live acceptance

**Files:**
- Create: `scripts/test-ipv4.py`
- Create: `tests/test_ipv4.py`

**Interfaces:**
- Python constants mirror the spec exactly: peer MAC `020000000002`, described local MAC `525400123456`, private addresses `c0a80e02`/`c0a80e01`, EtherType `0x0800`, Protocol `253`, IDs `0x1401`/`0x1402`, payloads `PYTHIPRQ`/`PYTHIPRP`, 60-byte frames.
- `scripts/test-ipv4.py --self-test` validates packet/frame construction, checksum, parser rejection, marker sequence, exact frame counts, and process cleanup.
- `scripts/test-ipv4.py` builds the image with `--ipv4-probe-elf`, runs the QEMU socket peer, verifies exact ARP/IP request and reply bytes, rejects storage-path evidence, and requires `QEMU_OUTCOME success`.

- [ ] **Step 1: Add failing Python tests**

Create `tests/test_ipv4.py` with tests for one's-complement checksum, exact header bytes, exact frame padding, reverse reply fields, malformed version/IHL/length/checksum, fragment/options policy, wrong MAC/EtherType/address/protocol/payload, marker duplicates/order, and no extra peer transmit.

- [ ] **Step 2: Run the focused Python tests to verify failure**

Run: `py -3 -m unittest tests.test_ipv4`

Expected: FAIL because the harness module does not exist.

- [ ] **Step 3: Implement pure Python oracle helpers**

Implement `ipv4_header`, `ipv4_frame`, `arp_frame`, `assert_exact_arp_request`, `assert_exact_ipv4_request`, `send_ipv4_reply`, `assert_exact_ipv4_reply`, and the marker/storage/outcome assertions with explicit lengths and no permissive wildcard acceptance.

- [ ] **Step 4: Implement self-test and QEMU runner**

Reuse the accepted ARP harness process cleanup and QEMU invocation pattern. The peer must accept exactly one ARP request, send one matching ARP reply, accept exactly one IPv4 request, send one matching IPv4 reply, and reject any extra transmit. The runner must use `--no-virtio-blk`, snapshot-backed ESP, and the existing serial marker oracle.

- [ ] **Step 5: Run Python tests and self-test**

Run: `py -3 -m unittest tests.test_ipv4; py -3 scripts/test-ipv4.py --self-test`

Expected: PASS.

- [ ] **Step 6: Run the live QEMU proof**

Run: `py -3 scripts/test-ipv4.py`

Expected: exactly the seven ordered markers, exact ARP/IP exchange, no storage markers, and `QEMU_OUTCOME success`.

- [ ] **Step 7: Commit**

```powershell
git add scripts/test-ipv4.py tests/test_ipv4.py
git commit -m "test(net): prove bounded IPv4 exchange"
```

## Task 7: Record accepted IPv4 evidence and update Phase 14 status

**Files:**
- Create: `docs/decisions/0098-phase-14-ipv4-consumer.md`
- Modify: `README.md`
- Modify: `docs/ROADMAP.md`
- Modify: `docs/HANDOVER.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`

**Interfaces:**
- Documents the exact IPv4 proof, local/hosted commit and run evidence, marker sequence, frame values, non-claims, and preserved Phase 15 boundary.

- [ ] **Step 1: Add documentation contract tests if existing status tests require them**

Update only the existing current-status assertions that still say IP is merely the next design boundary. Preserve all storage evidence phrases and ensure the new status states IPv4 accepted only after live evidence is recorded.

- [ ] **Step 2: Run documentation/status tests before editing**

Run: `py -3 -m pytest tests/test_virtio_net.py tests/test_arp.py -q`

Expected: the pre-IP status assertions identify the exact current-status lines to update; existing ARP and transport contracts remain green.

- [ ] **Step 3: Write ADR 0098 and status updates**

Record the exact standards basis from the spec, the QEMU evidence, the private test addresses, experimental Protocol 253 restriction, the seven markers, and explicit non-claims. Do not claim ICMP, sockets, routing, physical networking, or Phase 15 support.

- [ ] **Step 4: Run documentation and full tests**

Run: `git diff --check; py -3 -m pytest tests/test_virtio_net.py tests/test_arp.py -q; cargo test --workspace; py -3 -m unittest discover -s tests`.

Expected: PASS with no stale status assertion.

- [ ] **Step 5: Commit**

```powershell
git add docs/decisions/0098-phase-14-ipv4-consumer.md README.md docs/ROADMAP.md docs/HANDOVER.md docs/TECHNICAL-OVERVIEW.md tests
git commit -m "docs(net): record accepted IPv4 proof"
```

## Task 8: Whole-branch verification and hosted gate

**Files:**
- Modify only if a failing test exposes a real integration defect; otherwise no additional files.

- [ ] **Step 1: Run local quality gates**

Run: `cargo fmt --all -- --check; cargo test --workspace; py -3 -m unittest discover -s tests; py -3 scripts/test-virtio-net.py --self-test; py -3 scripts/test-network-port.py --self-test; py -3 scripts/test-link-layer.py --self-test; py -3 scripts/test-arp.py --self-test; py -3 scripts/test-ipv4.py --self-test`.

- [ ] **Step 2: Run serialized live QEMU gates**

Run the existing raw Virtio, NetworkPort, link-layer, ARP, and new IPv4 live proofs in that order. Confirm default and normal-session regressions remain green and no non-boot Virtio block disk is attached to the IPv4 profile.

- [ ] **Step 3: Inspect the complete diff**

Run: `git diff --check 8694457..HEAD; git status --short; git diff --stat 8694457..HEAD` and verify that no implementation path changed the accepted NetworkPort ABI or Phase 15 boundary.

- [ ] **Step 4: Commit any verification-only correction through the task review loop**

Do not patch verification failures directly in the controller. Route every code/test defect through the task implementer and scoped reviewer before rerunning the full gate.

## Continuation after this plan

After Task 8 is green, continue automatically with the next Phase 14 design/implementation slice rather than waiting: ICMP echo is next, followed by the remaining roadmap layers. Each later slice must preserve the same `NetworkPort` boundary, receive its own bounded design/spec, and remain opt-in until its evidence is green.
