# Phase 14 Capability-Gated Socket API Proof Plan

> Use the subagent-driven workflow: implement one task, independently review
> it, correct Critical/Important findings, and rerun the review before the
> next task.

**Goal:** Prove one capability-gated runtime-only socket boundary above the
accepted `NetworkPort` and TCP proof without freezing a general socket ABI.

**Spec:** `docs/superpowers/specs/2026-09-22-phase-14-capability-gated-socket-api-design.md`

**Acceptance ADR:** reserve ADR 0103 for the accepted evidence; do not create
it until both denied and granted cases, live evidence, and broad review pass.

## Locked constraints

- Preserve `VirtioTransport`, transport adapter, `NetworkPort`, the existing
  NetworkPort ABI, frozen PythTIG v1, and the legacy transport lifecycle.
- Reuse the existing `READ | SEND` NetworkPort capability as authority; add no
  new capability right or public syscall.
- Prove both an exact-endpoint denied `OPEN` with zero frames and a granted
  finite TCP exchange reusing ADR 0101's 2 ARP + 10 TCP profile.
- Keep the socket handle service-local, generation-checked, bounded, and
  invalidated by revocation. No reusable socket namespace or public ABI.
- Keep TLS/secure transport, physical NIC/Wi-Fi, Phase 15, modern/interrupt
  Virtio, multiqueue/offloads, routing/firewall/NAT, DNS service discovery,
  multiple consumers, zero-copy, and persistent state out.

## Tasks

### Task 1 — policy identity and private socket contract

Add shared identity/markers and private policy tests for authority-before-open,
exact endpoint matching, one generation-checked handle, bounded operations, and
denied/granted marker order. Do not add a public ABI, syscall, or kernel socket
object.

### Task 2 — finite socket service/consumer

Implement the denied and granted native cases using the existing NetworkPort
capability path and accepted TCP codec/exchange. Prove no handle/frame on
denial, exact `OPEN`/`SEND`/`RECEIVE`/`CLOSE` transitions on grant, and handle
invalidity after revocation.

### Task 3 — opt-in launch, capability issuance, and packaging

Add only additive opt-in feature/manifest/linker/image/CI plumbing. Launch the
denied process without NetworkPort authority and the granted process with the
existing `READ | SEND` authority. Keep default, recovery, normal-session, and
earlier network profiles mutually exclusive and unchanged.

### Task 4 — host oracle and serialized QEMU proof

Add `scripts/test-socket.py` and `tests/test_socket.py`. Prove the denied case
has zero frames and the granted case has exactly ADR 0101's frames, exact
socket markers, one success outcome, no storage evidence, and clean teardown.
Use the host TCP socket only for QEMU frame transport.

### Task 5 — ADR/evidence closeout

Record actual denied/granted evidence in ADR 0103 and update only current
roadmap/handover paragraphs needed to move the next boundary to secure
transport. Preserve all earlier claims, non-claims, and Phase 15 separation.

### Task 6 — whole-slice verification and broad review

Run format, workspace tests, complete Python tests, every prior network proof,
both socket cases, default/recovery, normal-session/fault, diff/scope audit,
and an independent broad review. Only after approval may secure-transport
design begin.

## Deferred ADR decisions

Reusable socket resource-id allocation/reuse, public capability-right selection,
syscall numbers, fixed public layouts, general capability delivery, reusable
copy-size/receive behavior, reusable teardown/revocation, listener/port
namespace policy, multi-consumer distribution, and the first reusable Pyth
runtime socket consumer remain deferred.
