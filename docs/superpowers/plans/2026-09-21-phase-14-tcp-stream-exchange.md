# Phase 14 Bounded TCP Stream Exchange Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox syntax for tracking.

**Goal:** Prove one deterministic bidirectional TCP stream exchange and
orderly close above the accepted NetworkPort/Ethernet/ARP/IPv4 boundaries,
without introducing a socket API or a general TCP service.

**Architecture:** Add one opt-in native `tcp-probe.elf` consumer that reuses
the accepted copied-frame NetworkPort, Ethernet/ARP helpers, and IPv4 codec.
Add a bounded no-std TCP codec and a finite connection-state proof, then verify
the exact exchange through a loopback QEMU frame oracle. The existing
transport adapter and all frozen interfaces remain unchanged.

**Tech Stack:** Rust `no_std` user probe and shared constants, existing PythOS
capability/syscall plumbing, Python QEMU/frame-oracle harness, Cargo
workspace, GitHub Actions acceptance workflow, and RFC 9293.

**Spec:** `docs/superpowers/specs/2026-09-21-phase-14-tcp-stream-design.md`

**Baseline:** accepted UDP branch after the final UDP broad review.

## Global Constraints

- Preserve `VirtioTransport` and transport adapter as the PythOS architectural
  terms. Do not add a `Driver` abstraction.
- Preserve the frozen NetworkPort and PythTIG v1 ABIs, existing capability
  rights, syscall numbers/layouts, copy-in/copy-out behavior, bootstrap,
  teardown, and legacy Virtio lifecycle.
- TCP is one new opt-in native proof only. Default boot and normal-session boot
  must not select it.
- Exact identity: `tcp-probe.elf`, principal `0x5059_5443_5000_0001`,
  consumer `0x5059_5443_4353_0001`, owner `0x5059_5443_4F57_0001`.
- Exact lower-layer setup: one ARP request/reply using the accepted private
  addresses and MACs; no persistent address or neighbor state.
- Exact TCP profile: protocol 6, ports `0x1505`/`0x1506`, local ISS
  `0x15050000`, peer ISS `0x25060000`, window `0x1000`, MSS option
  `02 04 04 00` on SYN segments only, request `PYTCPQ`, reply `PYTCPR`,
  ten TCP frames, and twelve total Ethernet frames including ARP.
- Exact checksums and frame sizes are those in the accepted TCP design spec.
  Ethernet FCS is excluded; minimum-frame padding is transmitted and TCP
  checksum arithmetic never includes Ethernet padding.
- Any first received nonmatching frame is a terminal failure. Empty receives
  have a fixed bound and then use the existing terminal error/revocation path.
- No general TCP socket/listener API, port allocator/namespace, multiple
  connections/consumers, retransmission timers, loss recovery, congestion
  control, reset/error service, routing, fragmentation, IPv6, DNS, TLS,
  physical NIC/Wi-Fi, modern Virtio PCI, interrupts, multiqueue, offloads,
  zero-copy, persistent state, or Phase 15 work.
- Host-side TCP sockets are permitted only as the loopback QEMU frame oracle;
  no PythOS socket API is introduced.

## Review Focus

- TCP checksum tests must cover the IPv4 pseudo-header, protocol 6, TCP
  length, checksum-zeroing, and non-transmitted arithmetic padding.
- MSS option parsing is exact on SYN/SYN-ACK and rejected on ordinary
  segments; no other options are accidentally accepted by the finite policy.
- Sequence-space accounting must treat SYN and FIN as consuming one number,
  data as six numbers, and ACK as consuming none. Cumulative ACKs and all
  ten exact segment transitions must be tested.
- Wrong MAC/EtherType, IPv4 field, TCP field, flags, option, sequence,
  acknowledgment, checksum, data, padding, direction, duplicate, reorder,
  unexpected RST, short frame, and extra transmit must fail.
- Default/normal-session profiles and every existing network proof must remain
  isolated from the new mutually exclusive `tcp-probe` feature.
- The live oracle must prove no storage-path evidence, no extra frames, exact
  marker order, exact QEMU success, and clean process teardown.
- The proof must not claim that a finite no-loss exchange implements the full
  RFC 9293 retransmission, timer, congestion, option, or user-interface
  requirements.

## File map

- `shared/src/tcp_markers.rs`: exact TCP identity and marker constants.
- `user/probes/tcp/Cargo.toml`, `linker.ld`, `src/lib.rs`, `src/tcp.rs`:
  bounded TCP header/options/checksum codec and unit tests.
- `user/probes/tcp/src/main.rs`: finite native NetworkPort consumer and policy
  tests.
- `core/src/tcp_probe.rs`: opt-in privileged launch contract only; no new ABI.
- `scripts/build-tcp-probe.py`: isolated ELF build and verification.
- `scripts/test-tcp.py` / `tests/test_tcp.py`: exact frame oracle, self-test,
  live runner, and host-side rejection tests.
- `docs/decisions/0101-phase-14-tcp-stream-consumer.md`: accepted evidence
  and exact stopping point.

## Task 1: Add shared TCP identity and marker contract

**Files:** Create `shared/src/tcp_markers.rs`; modify `shared/src/lib.rs` and
`shared/src/user_program_manifest.rs`; add shared constant/manifest tests.

**Interfaces:** Produces `TCP_PROBE_PROGRAM_NAME`,
`TCP_PROBE_PRINCIPAL_ID`, `TCP_CONSUMER_SERVICE_ID`,
`TCP_OWNER_SERVICE_ID`, and the nine exact marker strings for Tasks 4--6.

- [ ] **Step 1: Write failing tests** for exact names, IDs, marker values,
  uniqueness, and unchanged default manifest membership.
- [ ] **Step 2: Run** the focused shared tests and observe the expected
  missing-module/constant failures.
- [ ] **Step 3: Add** no-allocation constants and module export; keep TCP
  absent from the default manifest and do not alter any record layout.
- [ ] **Step 4: Run** the shared suite and verify no ABI diff.
- [ ] **Step 5: Commit** `feat(net): add TCP probe identity contract`.

## Task 2: Add the bounded TCP codec

**Files:** Create `user/probes/tcp/Cargo.toml`, `linker.ld`, `src/lib.rs`, and
`src/tcp.rs`; modify the workspace manifest and lockfile only as required.

**Interfaces:** Produces a borrowed, allocation-free codec for the canonical
20-byte ordinary header and 24-byte SYN header, exact MSS option, TCP
pseudo-header checksum, and checked sequence/flag fields.

- [ ] **Step 1: Write failing codec tests** for all ten exact segment headers,
  MSS option bytes, checksums, sequence/ACK values, request/reply data,
  checksum arithmetic, borrowed payload, short/overlong input/output,
  malformed offset/options, wrong pseudo-header, and invalid flags.
- [ ] **Step 2: Run** `cargo test -p pythos-user-tcp-probe`; observe the
  expected missing crate/codec failures.
- [ ] **Step 3: Implement** the checked no-std codec. Treat TCP checksum
  arithmetic exactly as RFC 9293 specifies and keep Ethernet padding outside
  the IP/TCP input.
- [ ] **Step 4: Run** focused tests, format, and strict target Clippy.
- [ ] **Step 5: Commit** `feat(net): add bounded TCP codec`.

## Task 3: Add the finite native TCP proof consumer

**Files:** Create `user/probes/tcp/src/main.rs`.

**Interfaces:** Consumes shared TCP identity/markers, the TCP codec, and the
accepted IPv4/link-layer/ARP helpers. Produces one fixed NetworkPort consumer
with the nine-marker lifecycle and terminal revocation.

- [ ] **Step 1: Write policy tests** for exact ARP setup, every exact TCP
  segment, handshake state transitions, bidirectional data, FIN close,
  sequence/ACK accounting, wrong fields/options/checksums, RST, duplicate or
  reordered receives, bounded empty polling, and no extra transmit.
- [ ] **Step 2: Run** focused native tests and observe missing-consumer
  failures.
- [ ] **Step 3: Implement** the fixed storage/bootstrap/describe/ARP/TCP
  state flow using only the existing `READ | SEND` capability, one bounded
  connection record, owner reset, and consumer revocation. Do not expose a
  socket or listener resource.
- [ ] **Step 4: Run** package tests, format, strict target Clippy, and a diff
  scope check.
- [ ] **Step 5: Commit** `feat(net): add native TCP stream proof`.

## Task 4: Wire the privileged opt-in launch path

**Files:** Create `core/src/tcp_probe.rs`; modify `core/Cargo.toml`,
`core/src/main.rs`, `core/src/network_port.rs`, and `core/src/syscall.rs`.

**Interfaces:** Produces only additive cfg plumbing, the `tcp-probe =
["verify"]` feature, exact launch identity, existing bootstrap mapping,
marker order, owner RESET, and consumer revocation.

- [ ] **Step 1: Add failing core tests** for exact identity, feature
  declaration, mutual exclusion with every existing probe/session profile,
  bootstrap mapping, marker order, and unchanged default path.
- [ ] **Step 2: Run** focused core tests and observe missing launch failures.
- [ ] **Step 3: Implement** only additive launch/cfg gates. Preserve every
  existing NetworkPort/syscall number, layout, capability, transport
  lifecycle, and default/normal-session path.
- [ ] **Step 4: Run** focused core tests, format, and strict verify-feature
  Clippy.
- [ ] **Step 5: Commit** `feat(net): wire TCP probe launch path`.

## Task 5: Add build-image and CI orchestration

**Files:** Create `scripts/build-tcp-probe.py`; modify
`scripts/build-image.py`, the relevant orchestration/CI tests, and
`.github/workflows/qemu-acceptance.yml`.

**Interfaces:** Produces an explicitly selected verified `tcp-probe.elf`; it
does not alter default or normal-session packaging and preserves all existing
feature conflicts/order.

- [ ] **Step 1: Add failing tests** for isolated target output, exact Cargo
  package/target/linker invocation, ELF verification before publication,
  manifest identity, explicit selection, default exclusion, and conflicts.
- [ ] **Step 2: Run** focused orchestration and workflow tests.
- [ ] **Step 3: Implement** the isolated build helper and additive image/CI
  gates after UDP. Preserve existing build flags and the no-virtio-block
  live contract.
- [ ] **Step 4: Run** focused Python tests, syntax checks, and required build
  self-tests.
- [ ] **Step 5: Commit** `ci(net): add TCP probe orchestration`.

## Task 6: Add the QEMU oracle and live proof

**Files:** Create `scripts/test-tcp.py` and `tests/test_tcp.py`.

**Interfaces:** Produces deterministic host-peer frame functions, TCP parser
and state assertions, self-test, and serialized live runner. Host TCP sockets
are allowed only inside this loopback oracle.

- [ ] **Step 1: Add failing host tests** for exact ARP plus ten TCP frames,
  all checksums/options/sequence transitions, malformed fields, marker order,
  no storage evidence, required QEMU flags, extra transmit, cleanup, and
  runner outcome.
- [ ] **Step 2: Run** `py -3 -m unittest tests.test_tcp` and observe missing
  harness failures.
- [ ] **Step 3: Implement** the loopback peer and QEMU runner by reusing the
  accepted UDP/IPv4 lifecycle, replacing only the TCP profile and markers.
  Require twelve total frames, `--no-virtio-blk`, snapshot-backed ESP, clean
  teardown, and one exact `QEMU_OUTCOME success`.
- [ ] **Step 4: Run** focused host tests, syntax, and
  `py -3 scripts/test-tcp.py --self-test`.
- [ ] **Step 5: Run serially** `py -3 scripts/test-tcp.py`; record exact live
  marker/frame evidence and process cleanup.
- [ ] **Step 6: Commit** `test(net): prove bounded TCP stream exchange`.

## Task 7: Record accepted TCP evidence

**Files:** Create `docs/decisions/0101-phase-14-tcp-stream-consumer.md`;
modify README/status documents and only stale assertions required by tests.

- [ ] Record the local live evidence, exact RFC 9293 sections, MSS option,
  sequence/ACK profile, ten TCP/twelve total frames, marker order, no-hosted
  claim, and every TCP/socket/retransmission/Phase 15 non-claim.
- [ ] Preserve all accepted ARP, IPv4, ICMP, and UDP evidence while advancing
  the next boundary to the separately authorized DNS design/slice.
- [ ] Run focused status/TCP tests, syntax, and `git diff --check`; commit
  `docs(net): record accepted TCP stream proof` and write the complete report
  to `.superpowers/sdd/2026-09-21-phase-14-tcp-stream-exchange/task-7-report.md`.

## Task 8: Whole-slice verification and broad review

- [ ] Run Rust format/workspace tests, the complete Python suite, syntax
  checks, all prior network self-tests/live proofs, TCP self-test/live proof,
  default boot/recovery, normal-session two-boot/fault, diff check, and a
  baseline-to-HEAD scope audit serially.
- [ ] Record actual rerun evidence in the TCP SDD Task 8 report, including
  environmental permission reruns separately from product failures.
- [ ] Dispatch an independent broad reviewer against the complete TCP diff.
  Critical/Important findings require a tested fix and re-review; Minor
  findings are explicitly recorded as deferred. Only after a clean gate may
  the next Phase 14 DNS design slice begin.
