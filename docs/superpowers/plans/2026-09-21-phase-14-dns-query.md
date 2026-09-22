# Phase 14 Bounded DNS Query Implementation Plan

> Use the subagent-driven workflow: implement one task, independently review
> it, correct Critical/Important findings, and rerun the review before the
> next task.

**Goal:** Prove one exact DNS A query/response over the accepted bounded
NetworkPort/Ethernet/ARP/IPv4/UDP boundaries without adding sockets or a
resolver service.

**Spec:** `docs/superpowers/specs/2026-09-21-phase-14-dns-query-design.md`

**Acceptance ADR:** reserve ADR 0102 for the accepted evidence; do not create
it until the live proof and broad review pass.

## Locked constraints

- Preserve `VirtioTransport`, transport adapter, `NetworkPort`, the frozen
  ABI/PythTIG v1/syscall contracts, capability rights, and legacy transport
  lifecycle.
- Add one opt-in native `dns-probe.elf` only.
- Exactly two ARP plus one DNS query and one DNS response; no extra frame,
  cache, retry, timeout policy, resolver service, socket API, or persistent
  state.
- Use local UDP port `0x1605` and DNS server port `0x0035`; the exact query is
  a 32-byte DNS/40-byte UDP/60-byte IPv4/74-byte Ethernet frame and the exact
  response is a 48-byte DNS/56-byte UDP/76-byte IPv4/90-byte Ethernet frame.
- Keep physical NIC/Wi-Fi, modern/interrupt Virtio, multiqueue, offloads,
  zero-copy, routing, IPv6, DNSSEC, EDNS, DNS-over-TCP, and Phase 15 out.

## Tasks

### Task 1 — identity and bounded DNS codec

Add shared DNS identity/markers and a no-std bounded codec for the exact DNS
header, question, label encoding, compressed answer pointer, A record, and
checked lengths. Add tests for exact bytes, counts/flags, pointer bounds,
malformed labels, unsupported types/classes, and no allocation.

### Task 2 — finite native DNS consumer

Add the opt-in native consumer using the existing `READ | SEND` capability and
the accepted ARP/IPv4/UDP frame flow. Emit the first five DNS markers in order,
reject any first nonmatching frame or malformed DNS message, and expose no
socket/resource. Leave capability revocation, queue stop, owner reset, and the
final two teardown markers to the privileged launch path in Task 3.

### Task 3 — privileged launch and image/CI orchestration

Add only additive cfg/manifest/build/CI plumbing for `dns-probe.elf`. Keep it
mutually exclusive with existing probes and session/diagnostic profiles. Add
compile, package, Clippy, and self-test CI gates after TCP with exact ordering
assertions. Host-oracle and live-QEMU proof remain reserved for Task 4.

### Task 4 — host oracle and live QEMU proof

Add `scripts/test-dns.py` and `tests/test_dns.py`. Reuse the accepted UDP
runner lifecycle and prove exact four-frame ordering, DNS bytes, UDP/IPv4
checksums, response pointer/answer, seven markers, one success outcome, no
storage evidence, no extra frame, and clean teardown.

### Task 5 — ADR/evidence closeout

Record actual live evidence in ADR 0102 and update only current status/handover
paragraphs needed to move the next boundary to capability-gated socket API.
Preserve all prior boundaries and non-claims.

### Task 6 — whole-slice verification and broad review

Run formatting, workspace tests, complete Python tests, prior network proofs,
DNS self/live proof, default/recovery, normal-session/fault, diff/scope audit,
and an independent broad review. Only after approval may the capability-gated
socket API design begin.
