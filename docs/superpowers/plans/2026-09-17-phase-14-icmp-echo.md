# Phase 14 Bounded ICMP Echo Implementation Plan

> For the implementation agent: use the subagent-driven workflow, one fresh
> implementer per task, a scoped reviewer after each task, and a final broad
> review. Do not pause for routine owner approval; the Phase 14 continuation
> authority is active. Do not push, merge, or change hosted state.

**Goal:** Prove one deterministic ICMP Echo Request/Reply above the accepted
NetworkPort/Ethernet/ARP/IPv4 boundaries, with no reusable ICMP service or
later networking scope.

**Design:** `docs/superpowers/specs/2026-09-17-phase-14-icmp-echo-design.md`

**Baseline:** the accepted IPv4 branch at `878d6f9dcb907781d2d524761b2dbcf280a363a3`.

## Global constraints

- Preserve `VirtioTransport` and transport adapter as the PythOS architectural
  terms. Do not add a `Driver` abstraction.
- Preserve the frozen NetworkPort and PythTIG v1 ABIs, existing capability
  rights, syscall numbers/layouts, copy-in/copy-out behavior, bootstrap,
  teardown, and legacy Virtio lifecycle.
- ICMP is a new opt-in native proof only. Default boot and normal-session boot
  must not select it.
- Exact identity: `icmp-probe.elf`, principal `0x5059_4943_4D50_0001`,
  consumer `0x5059_4943_4353_0001`, owner `0x5059_4943_4F57_0001`.
- Exact proof: one ARP setup; one IPv4 Protocol 1 Echo Request and one Echo
  Reply; type 8/0, code 0, identifier `0x1403`, sequence `0x0001`, data
  `PYTHICMP`, IPv4 IDs `0x1403`/`0x1404`, checksums `C982`/`C981` and
  `A8C6`/`B0C6`, total length 36, ten zero Ethernet pad bytes, 60-byte
  software frame excluding FCS.
- Any first received nonmatching frame is a terminal failure. Empty receives
  have a fixed bound and then use the existing terminal error/revocation path.
- No ICMP server, dispatcher, errors, rate limiting, retry/timer policy,
  routing, fragmentation, multicast/broadcast echo, sockets, UDP/TCP/DNS/TLS,
  physical NIC/Wi-Fi, modern Virtio PCI, interrupts, multiqueue, offloads,
  zero-copy, multiple consumers, persistent state, or Phase 15 work.
- Host-side TCP sockets are permitted only as the loopback QEMU frame oracle;
  no PythOS socket API is introduced.

## Task 1: Add shared identity and marker contract

**Files:** create `shared/src/icmp_markers.rs`; modify `shared/src/lib.rs` and
`shared/src/user_program_manifest.rs`.

Add exact program/principal constants and seven markers:

```text
PYTHOS:CORE:ICMP:BOOTSTRAPPED
PYTHOS:CORE:ICMP:DESCRIBE_OK
PYTHOS:CORE:ICMP:ARP_SETUP_OK
PYTHOS:CORE:ICMP:TX_OK
PYTHOS:CORE:ICMP:RX_OK
PYTHOS:CORE:ICMP:TEARDOWN_REVOKED
PYTHOS:CORE:ICMP_READY
```

Add exactness and uniqueness tests. No ABI or default manifest behavior changes.

## Task 2: Add bounded ICMP codec

**Files:** create `user/probes/icmp/Cargo.toml`, `linker.ld`,
`src/lib.rs`, `src/icmp.rs`; modify workspace `Cargo.toml`/lockfile.

Expose `IcmpEcho`, `EncodeError`, `DecodeError`, `encode_echo`,
`decode_echo`, and `icmp_checksum` from `pythos_user_icmp_probe::icmp`.
Use no allocation in production and checked slicing/arithmetic. Encode only
Type 8/0, Code 0, the fixed 8-byte data profile, identifier/sequence in
network order, and validate the complete checksum on decode. Return a borrowed
data view. Add exact request/reply bytes, round-trip, short/bad type/code,
length, identifier/sequence/data, checksum, and output-bound tests.

## Task 3: Add the native ICMP proof consumer

**Files:** create/modify only `user/probes/icmp/src/main.rs`.

Mirror the accepted IPv4 consumer's fixed storage, bootstrap validation,
NetworkPort syscall wrappers, exact Ethernet/ARP setup, bounded receive, and
terminal error/success paths. Reuse the accepted IPv4 codec, link-layer, and
ARP helpers. Encode Protocol 1 and the exact ICMP Echo Request; accept only
the exact reverse Echo Reply and zero padding. Add policy tests for wrong
MAC/EtherType, IP fields, ICMP type/code/checksum/id/sequence/data, fragments,
padding, and no extra frame.

## Task 4: Wire the privileged opt-in launch path

**Files:** create `core/src/icmp_probe.rs`; modify `core/Cargo.toml`,
`core/src/main.rs`, `core/src/network_port.rs`, and `core/src/syscall.rs`.

Add `icmp-probe = ["verify"]`, mutual exclusions, cfg-only existing-ABI
NetworkPort/syscall plumbing, the launch contract, exact program/principal and
service IDs, read-only bootstrap at `0x0000_0000_7300_0000`, seven marker order,
owner RESET and consumer revocation. Preserve existing launch/lifecycle paths.

## Task 5: Add build-image and CI orchestration

**Files:** create `scripts/build-icmp-probe.py`; modify
`scripts/build-image.py`, `tests/test_build_orchestration.py`,
`.github/workflows/qemu-acceptance.yml`, and `tests/test_ci_workflow.py`.

Build `pythos-user-icmp-probe` for `x86_64-unknown-none`, verify the ELF
before publication, package `icmp-probe.elf` with the exact principal, preserve
all existing arguments/conflicts, and add ordered unit/build/clippy/self/live
CI gates after IPv4. Scope subprocess mocks with restoring contexts.

## Task 6: Add QEMU oracle and live proof

**Files:** create `scripts/test-icmp.py` and `tests/test_icmp.py`.

Mirror the accepted IPv4 harness. Validate exact ARP setup, 60-byte ICMP
request/reply frames, checksums, marker order, no extra transmit, no storage
evidence, cleanup, `--no-virtio-blk`, snapshot-backed ESP, and
`QEMU_OUTCOME success`. Add malformed-field and parser rejection tests plus a
self-test, then run the serialized live proof.

## Task 7: Record accepted ICMP evidence

**Files:** create `docs/decisions/0099-phase-14-icmp-echo-consumer.md`; modify
README/status documents and only the existing stale status assertions required
by tests.

Record local live evidence, exact RFC 792/RFC 1122 basis, non-claims, and the
next boundary (UDP) without claiming hosted ICMP evidence or a complete ICMP
host/server.

## Task 8: Whole-slice verification

Run Rust/Python unit suites, all prior self-tests/live proofs, the ICMP
self-test/live proof, default/normal-session regressions, and a complete diff
scope audit. Route any real defect through the implementer/reviewer loop. After
this task is green, continue automatically to the next Phase 14 UDP design
slice.
