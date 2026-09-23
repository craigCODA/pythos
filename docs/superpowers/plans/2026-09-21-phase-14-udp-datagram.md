# Phase 14 Bounded UDP Datagram Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox syntax for tracking.

**Goal:** Prove one deterministic UDP datagram request/reply above the accepted
NetworkPort/Ethernet/ARP/IPv4 boundaries, with no reusable UDP service, port
namespace, socket API, or later networking scope.

**Architecture:** Add one opt-in native `udp-probe.elf` consumer that reuses
the accepted copied-frame NetworkPort, Ethernet/ARP helpers, and IPv4 codec.
Add a bounded no-std UDP codec that validates the IPv4 pseudo-header checksum,
then prove one exact request and one exact reversed-port reply through a
loopback QEMU frame oracle. The existing transport adapter and all frozen
interfaces remain unchanged.

**Tech Stack:** Rust `no_std` user probe and shared constants, existing PythOS
capability/syscall plumbing, Python QEMU/frame-oracle harness, Cargo workspace,
GitHub Actions acceptance workflow, RFC 768 and RFC 1122.

**Spec:** `docs/superpowers/specs/2026-09-21-phase-14-udp-datagram-design.md`

**Baseline:** accepted ICMP branch at `548c4d86ab985913fdf2222e4e459dc6444fa144`.

## Global Constraints

- Preserve `VirtioTransport` and transport adapter as the PythOS architectural
  terms. Do not add a `Driver` abstraction.
- Preserve the frozen NetworkPort and PythTIG v1 ABIs, existing capability
  rights, syscall numbers/layouts, copy-in/copy-out behavior, bootstrap,
  teardown, and legacy Virtio lifecycle.
- UDP is a new opt-in native proof only. Default boot and normal-session boot
  must not select it.
- Exact identity: `udp-probe.elf`, principal `0x5059_5544_5000_0001`,
  consumer `0x5059_5544_4353_0001`, owner `0x5059_5544_4F57_0001`.
- Exact proof: one ARP setup; one IPv4 Protocol 17 request and one reversed
  UDP reply; ports `0x1405`/`0x1406`, data `PYTHUDP`, UDP length `15`, UDP
  checksum `0xF08A`, IPv4 IDs `0x1405`/`0x1406`, IPv4 checksums `0xC971`/
  `0xC970`, total length `35`, eleven zero Ethernet pad bytes, and 60-byte
  software frames excluding FCS.
- The UDP checksum uses the source/destination IPv4 pseudo-header, protocol
  17, UDP length 15, UDP header with checksum zeroed, and the seven data bytes;
  the odd arithmetic pad is zero and is not transmitted or counted.
- Any first received nonmatching frame is a terminal failure. Empty receives
  have a fixed bound and then use the existing terminal error/revocation path.
- No UDP server, port allocator/namespace, socket API, multiplexing,
  Port-Unreachable generation, ICMP error delivery, retries, timers, routing,
  fragmentation, multicast/broadcast delivery, TCP/DNS/TLS, physical NIC/Wi-Fi,
  modern Virtio PCI, interrupts, multiqueue, offloads, zero-copy, multiple
  consumers, persistent state, or Phase 15 work.
- Host-side TCP sockets are permitted only as the loopback QEMU frame oracle;
  no PythOS socket API is introduced.

## Review Focus

- Odd-length UDP data must be padded only for checksum arithmetic; a test pins
  the transmitted length at 15 and the checksum at `0xF08A` (Task 2).
- A valid checksum with the wrong pseudo-header address, protocol, length,
  port, or data must be rejected; tests cover each mismatch (Tasks 2 and 3).
- A zero checksum, short/overlong datagram, invalid length, or truncated frame
  must fail without slicing or arithmetic panic (Tasks 2 and 3).
- Default/normal-session profiles and every existing network probe must remain
  isolated from the new mutually exclusive `udp-probe` feature (Tasks 4 and 5).
- A nonmatching first receive, duplicate transmit, marker disorder, cleanup
  failure, or storage-path evidence must fail the live proof (Task 6).

## File map

- `shared/src/udp_markers.rs`: exact UDP identity and marker constants.
- `user/probes/udp/src/udp.rs`: bounded UDP header/data/checksum codec.
- `user/probes/udp/src/main.rs`: finite native NetworkPort consumer and policy
  tests.
- `core/src/udp_probe.rs`: opt-in privileged launch contract only; no new ABI.
- `scripts/build-udp-probe.py`: isolated ELF build and verification.
- `scripts/test-udp.py` / `tests/test_udp.py`: frame oracle, self-test, live
  runner, and host-side rejection tests.
- `docs/decisions/0100-phase-14-udp-datagram-consumer.md`: accepted local
  evidence and exact stopping point.

## Task 1: Add shared UDP identity and marker contract

**Files:** Create `shared/src/udp_markers.rs`; modify `shared/src/lib.rs` and
`shared/src/user_program_manifest.rs`; test shared constants and manifest
selection.

**Interfaces:** Produces `UDP_PROBE_PROGRAM_NAME`,
`UDP_PROBE_PRINCIPAL_ID`, `UDP_CONSUMER_SERVICE_ID`, `UDP_OWNER_SERVICE_ID`,
and the seven exact marker strings for Tasks 4--6.

- [ ] **Step 1: Write failing tests** for exact names, IDs, seven marker
  values, uniqueness, and unchanged default manifest membership.
- [ ] **Step 2: Run** `cargo test -p pythos-shared udp` and observe the expected
  missing-module/constant failures.
- [ ] **Step 3: Add** the no-allocation constants and module export; keep UDP
  absent from the default manifest and do not alter any existing record layout.
- [ ] **Step 4: Run** `cargo test -p pythos-shared`; expect the complete shared
  suite to pass and verify no ABI diff.
- [ ] **Step 5: Commit** `feat(net): add UDP probe identity contract`.

## Task 2: Add the bounded UDP codec

**Files:** Create `user/probes/udp/Cargo.toml`, `linker.ld`, `src/lib.rs`, and
`src/udp.rs`; modify the workspace manifest and lockfile only as required.

**Interfaces:** Consumes fixed source/destination IPv4 addresses from the
consumer; produces `pythos_user_udp_probe::udp::{UdpDatagram, EncodeError,
DecodeError, encode_datagram, decode_datagram, udp_checksum}`. The encoder
accepts `(datagram, source_ipv4, destination_ipv4, output)`; the decoder
accepts `(bytes, source_ipv4, destination_ipv4)` and returns a borrowed data
view.

- [ ] **Step 1: Write failing codec tests** for the exact request/reply bytes,
  `0xF08A` checksum, odd-length arithmetic padding, round-trip borrowed data,
  reversed ports, short/overlong output, length mismatch, bad checksum,
  zero checksum, and pseudo-header address/protocol/length mismatch.
- [ ] **Step 2: Run** `cargo test -p pythos-user-udp-probe`; expect failures
  because the crate and codec do not exist.
- [ ] **Step 3: Implement** an allocation-free, checked codec. Encode exactly
  the eight-byte header plus seven-byte `PYTHUDP` profile, use network byte
  order, compute the pseudo-header checksum with an arithmetic-only zero pad,
  reject transmitted checksum zero, and return a borrowed `&[u8]` data view.
- [ ] **Step 4: Run** `cargo test -p pythos-user-udp-probe`,
  `cargo fmt --all -- --check`, and
  `cargo clippy -p pythos-user-udp-probe --target x86_64-unknown-none -- -D warnings`.
  Expect all codec tests and strict linting to pass.
- [ ] **Step 5: Commit** `feat(net): add bounded UDP codec`.

## Task 3: Add the native UDP proof consumer

**Files:** Create `user/probes/udp/src/main.rs`.

**Interfaces:** Consumes the shared UDP identity/markers, the UDP codec from
Task 2, and the accepted IPv4/link-layer/ARP helpers. Produces one fixed
NetworkPort consumer with the seven-marker lifecycle and terminal revocation.

- [ ] **Step 1: Write policy tests** for exact ARP setup, request frame bytes,
  reply frame bytes, reversed addresses/ports, protocol 17, both IPv4
  checksums, UDP checksum/data/length, wrong MAC/EtherType, every wrong IPv4
  field, wrong port/length/checksum/data, nonzero padding, short frame,
  nonmatching first receive, bounded empty polling, and no extra frame.
- [ ] **Step 2: Run** the focused native probe tests and observe failures for
  the missing consumer.
- [ ] **Step 3: Implement** the fixed storage/bootstrap/describe/send/receive
  flow by mirroring the accepted ICMP consumer's existing syscall wrappers and
  terminal paths. Use only the existing `READ | SEND` capability, one ARP
  setup, one request, one receive, owner reset, and consumer revocation.
- [ ] **Step 4: Run** `cargo test -p pythos-user-udp-probe`, format, strict
  target Clippy, and a diff scope check. Expect all policy tests to pass.
- [ ] **Step 5: Commit** `feat(net): add native UDP datagram proof`.

## Task 4: Wire the privileged opt-in launch path

**Files:** Create `core/src/udp_probe.rs`; modify `core/Cargo.toml`,
`core/src/main.rs`, `core/src/network_port.rs`, and `core/src/syscall.rs`.

**Interfaces:** Produces only the existing-ABI cfg plumbing, the
`udp-probe = ["verify"]` feature, exact launch identity, read-only bootstrap
at `0x0000_0000_7300_0000`, marker order, owner RESET, and consumer revocation.

- [ ] **Step 1: Add failing core tests** for the feature declaration, exact
  identity, mutual exclusion against every existing probe/session profile,
  bootstrap mapping, marker order, and unchanged default path.
- [ ] **Step 2: Run** focused core tests and a no-code feature check; observe
  the expected missing launch module/feature failures.
- [ ] **Step 3: Implement** only the additive launch module and cfg gates,
  preserving every existing NetworkPort/syscall number, layout, capability,
  transport lifecycle, and default/normal-session path.
- [ ] **Step 4: Run** focused tests, `cargo test -p pythos-core --bin pythcore`,
  `cargo fmt --all -- --check`, and strict Clippy for
  `--features udp-probe`; expect pass.
- [ ] **Step 5: Commit** `feat(net): wire UDP probe launch path`.

## Task 5: Add build-image and CI orchestration

**Files:** Create `scripts/build-udp-probe.py`; modify `scripts/build-image.py`,
`tests/test_build_orchestration.py`, `.github/workflows/qemu-acceptance.yml`,
and `tests/test_ci_workflow.py`.

**Interfaces:** Produces an explicitly selected verified `udp-probe.elf`; it
does not alter default or normal-session packaging and preserves all existing
feature conflicts/order.

- [ ] **Step 1: Add failing orchestration tests** for isolated target output,
  exact Cargo package/target/linker invocation, ELF verification before
  publication, manifest identity, explicit selection, default exclusion, and
  all probe/session conflicts.
- [ ] **Step 2: Run** `py -3 -m unittest tests.test_build_orchestration
  tests.test_ci_workflow`; observe missing UDP orchestration failures.
- [ ] **Step 3: Implement** the isolated build helper and additive image/CI
  gates after ICMP. Scope subprocess mocks with restoring contexts and preserve
  the existing build flags and no-virtio-block live contract.
- [ ] **Step 4: Run** the focused Python suites, `py -3 -m py_compile
  scripts/build-udp-probe.py scripts/build-image.py`, and the exact build/self
  tests required by the workflow; expect pass.
- [ ] **Step 5: Commit** `ci(net): add UDP probe orchestration`.

## Task 6: Add the QEMU oracle and live proof

**Files:** Create `scripts/test-udp.py` and `tests/test_udp.py`.

**Interfaces:** Produces deterministic host-peer frame functions, parser
assertions, self-test, and serialized live runner for the UDP marker contract.
Host TCP sockets are allowed only inside this loopback oracle.

- [ ] **Step 1: Add failing host tests** for the exact four-frame exchange,
  `0xC971`/`0xC970` IPv4 checksums, `0xF08A` UDP checksum, odd data/padding,
  reversed ports, malformed fields, marker order/multiplicity, no storage
  evidence, required QEMU flags, extra transmit, cleanup, and runner outcome.
- [ ] **Step 2: Run** `py -3 -m unittest tests.test_udp` and observe the
  expected missing-harness failures.
- [ ] **Step 3: Implement** the loopback peer and QEMU runner by reusing the
  accepted ICMP/IPv4 lifecycle, replacing only packet/profile/marker policy.
  Require one ARP request/reply, one UDP request/reply, exactly four frames,
  `--no-virtio-blk`, snapshot-backed ESP, clean teardown, and one exact
  `QEMU_OUTCOME success`.
- [ ] **Step 4: Run** `py -3 -m unittest tests.test_udp`,
  `py -3 -m py_compile scripts/test-udp.py tests/test_udp.py`, and
  `py -3 scripts/test-udp.py --self-test`; expect pass.
- [ ] **Step 5: Run serially** `py -3 scripts/test-udp.py`; record the actual
  live result and exact marker/frame evidence in the task report.
- [ ] **Step 6: Commit** `test(net): prove bounded UDP datagram exchange`.

## Task 7: Record accepted UDP evidence

**Files:** Create `docs/decisions/0100-phase-14-udp-datagram-consumer.md`;
modify README/status documents and only existing stale status assertions needed
by tests.

- [ ] Record the local live evidence and exact RFC 768/RFC 1122 basis; state
  the 7-byte payload, UDP length 15, IPv4 total length 35, UDP checksum
  `0xF08A`, IPv4 checksums `0xC971`/`0xC970`, marker order, four-frame
  exchange, no-hosted claim, and all UDP/socket/ICMP-error/Phase 15 non-claims.
- [ ] Preserve all accepted ARP, IPv4, and ICMP evidence while advancing the
  next boundary to the separately authorized TCP design/slice.
- [ ] Run focused status/UDP tests, syntax, and `git diff --check`; commit
  `docs(net): record accepted UDP datagram proof` and write the full report to
  `.superpowers/sdd/2026-09-21-phase-14-udp-datagram/task-7-report.md`.

## Task 8: Whole-slice verification and broad review

- [ ] Run `cargo fmt --all -- --check`, `cargo test --workspace`, the complete
  Python suite, syntax checks, all prior network self-tests/live proofs, UDP
  self-test/live proof, default boot/recovery, normal-session two-boot/fault,
  `git diff --check`, and a baseline-to-HEAD scope audit serially.
- [ ] Record actual rerun evidence in
  `.superpowers/sdd/2026-09-21-phase-14-udp-datagram/task-8-report.md`,
  distinguishing historical accepted proof evidence from fresh commands.
- [ ] Dispatch the final broad reviewer against the full branch diff. Critical
  or Important findings require one tested fix pass; Minor findings are
  recorded as deferred. Only after a clean gate may the next Phase 14 TCP
  design slice begin.
