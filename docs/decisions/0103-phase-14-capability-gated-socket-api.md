# ADR 0103: Phase 14 Capability-Gated Socket Proof

Date: 2026-09-22

Status: Accepted locally; hosted or remote socket evidence is not recorded

## Context

ADR 0095 accepts the bounded copied-frame `NetworkPort` capability, and ADRs
0096 through 0102 accept the finite Ethernet-II, ARP, IPv4, ICMP, UDP, TCP,
and DNS consumer proofs above it. This ADR records the next finite native
proof: an exact endpoint is insufficient without network authority, while the
same endpoint is usable through one private bounded socket contract when the
existing `NetworkPort` authority is granted.

The accepted architectural boundary is preserved. `VirtioTransport` remains
the privileged PythOS transport adapter, the transport adapter continues to
expose only completed copied Ethernet frames through `NetworkPort`, and the
private socket service/consumer remains above `NetworkPort`. This decision
does not promote the proof into a general socket API or service.

## Decision

The accepted proof is runtime-only, private, deterministic, and finite. It
uses the existing boot-local `NetworkPort` `READ | SEND` capability as its
only network authority. Endpoint knowledge and a service-local socket handle
never substitute for that capability.

The accepted private identities are:

```text
SOCKET_PROBE_PRINCIPAL_ID  = 0x5059_534F_4300_0001
SOCKET_CONSUMER_SERVICE_ID = 0x5059_534F_4353_0001
SOCKET_OWNER_SERVICE_ID    = 0x5059_534F_4F57_0001
```

The private service admits only the fixed `OPEN`, `SEND`, `RECEIVE`, and
`CLOSE` proof operations, the exact ADR 0101 endpoint, fixed six-byte
`PYTCPQ`/`PYTCPR` data, and one bounded generation-checked handle. Authority
is validated before handle allocation or frame emission. Close or revocation
invalidates the handle. These records and identities are implementation-local
proof data, not a public ABI, capability namespace, or reusable socket
resource model.

## Accepted denied and granted profiles

The serialized denied profile launches with no `NetworkPort` capability. Its
exact-endpoint `OPEN` is denied before any handle or frame is created. The
host oracle observed `tx=0 rx=0 total=0`, exactly these four markers once and
in order, one exact `QEMU_OUTCOME success`, and clean teardown:

```text
PYTHOS:CORE:SOCKET:DENIED_BOOTSTRAPPED
PYTHOS:CORE:SOCKET:OPEN_WITHOUT_CAP_DENIED
PYTHOS:CORE:SOCKET:DENIED_TEARDOWN_COMPLETE
PYTHOS:CORE:SOCKET_DENIED_READY
```

The denied profile does not claim capability revocation because no consumer
capability was issued.

The serialized granted profile receives the existing `NetworkPort` `READ |
SEND` capability. It performs the exact ADR 0101 frame oracle: two ARP frames
plus ten TCP frames, with seven transmitted and five received frames, twelve
frames total. The eight socket markers appeared exactly once in this order:

```text
PYTHOS:CORE:SOCKET:BOOTSTRAPPED
PYTHOS:CORE:SOCKET:OPEN_GRANTED
PYTHOS:CORE:SOCKET:HANDSHAKE_OK
PYTHOS:CORE:SOCKET:REQUEST_OK
PYTHOS:CORE:SOCKET:RESPONSE_OK
PYTHOS:CORE:SOCKET:CLOSE_OK
PYTHOS:CORE:SOCKET:TEARDOWN_REVOKED
PYTHOS:CORE:SOCKET_READY
```

The granted profile produced one exact `QEMU_OUTCOME success`, no extra
frame, and no storage-path evidence. The live harness used `--no-virtio-blk`,
no non-boot Virtio block disk, and a snapshot-backed ESP; both profiles had
clean serial-log and ESP teardown. Both profiles rejected error, panic,
transport-error, duplicate/reordered marker, and unexpected frame evidence.

## Local acceptance evidence

Task 4 was accepted at commit `cbd5ed4` (`test(net): prove capability-gated
socket cases`). The focused evidence commands and results were:

- `py -3 -m py_compile scripts/test-socket.py tests/test_socket.py` passed;
- `py -3 -m unittest tests.test_socket` passed 5/5;
- `py -3 scripts/test-socket.py --self-test` passed 3/3 and emitted
  `SOCKET_QEMU_ACCEPTANCE_OK`;
- serialized `py -3 scripts/test-socket.py` passed both profiles; and
- `git diff --check` passed.

The accepted live output recorded these exact bounded results:

```text
SOCKET_LIVE_CASE denied tx=0 rx=0 total=0
SOCKET_LIVE_MARKERS denied PYTHOS:CORE:SOCKET:DENIED_BOOTSTRAPPED > PYTHOS:CORE:SOCKET:OPEN_WITHOUT_CAP_DENIED > PYTHOS:CORE:SOCKET:DENIED_TEARDOWN_COMPLETE > PYTHOS:CORE:SOCKET_DENIED_READY
SOCKET_LIVE_OUTCOME denied QEMU_OUTCOME success
SOCKET_LIVE_CLEANUP denied serial_log_exists=False esp_snapshot_exists=False
SOCKET_LIVE_CASE granted tx=7 rx=5 total=12
SOCKET_LIVE_MARKERS granted PYTHOS:CORE:SOCKET:BOOTSTRAPPED > PYTHOS:CORE:SOCKET:OPEN_GRANTED > PYTHOS:CORE:SOCKET:HANDSHAKE_OK > PYTHOS:CORE:SOCKET:REQUEST_OK > PYTHOS:CORE:SOCKET:RESPONSE_OK > PYTHOS:CORE:SOCKET:CLOSE_OK > PYTHOS:CORE:SOCKET:TEARDOWN_REVOKED > PYTHOS:CORE:SOCKET_READY
SOCKET_LIVE_OUTCOME granted QEMU_OUTCOME success
SOCKET_LIVE_CLEANUP granted serial_log_exists=False esp_snapshot_exists=False
SOCKET_QEMU_ACCEPTANCE_OK
```

The host TCP socket was used only as the loopback transport carrying QEMU
Virtio Ethernet frames between the oracle and guest. It is not a PythOS
socket ABI or service. This is local QEMU evidence only; no hosted or remote
socket acceptance is claimed.

## Scope boundary and non-claims

This Task 5 closeout changes no implementation code. The accepted slice adds
no public ABI, syscall number or layout, PythTIG v1 record, capability right,
kernel socket object, or change to `VirtioTransport`, the transport adapter,
`NetworkPort`, the existing NetworkPort ABI, transport lifecycle, teardown,
revocation, or reset behavior.

Default boot, recovery boot, normal-session boot, and every earlier accepted
network proof remain unchanged.

This finite native proof is not a general socket API or service. It does not
add a reusable socket namespace, public handle allocator, listeners, accept
queues, port allocation or multiplexing, multiple sockets or consumers, UDP
sockets, routing, default gateways, firewall/NAT, DNS-based discovery, IPv6,
general TCP behavior, retransmission, timers, loss recovery, congestion
control, zero-copy, or persistent network/socket state.

TLS and secure transport are not implemented by this decision. Physical
NIC/Wi-Fi support, Lenovo hardware behavior, modern or interrupt Virtio PCI,
interrupts/MSI/MSI-X, multiqueue, offloads, generalized DMA or hardware-memory
behavior, Phase 15 hardware expansion, and later update/recovery or SMP work
remain excluded.

## Follow-up ADR decisions

The prior follow-up ADR deferrals remain unresolved and are not generalized
by this finite proof: reusable socket resource-id allocation/reuse, public
capability-right selection, syscall numbers, fixed public layouts, general
capability delivery, reusable copy-size/receive behavior, reusable
teardown/revocation, listener/port namespace policy, multi-consumer
distribution, and the first reusable Pyth runtime socket consumer.

## Consequences

Phase 14 now has local evidence that the existing capability boundary denies
an exact endpoint without `NetworkPort` authority and admits the same finite
TCP exchange with the existing authority. The stopping point is this one
private native proof, not a reusable socket architecture. Secure transport is the next separately authorized Phase 14 boundary. Phase 15 hardware expansion
and later phases remain separate.
