# Phase 14 Capability-Gated Socket API Proof Design

Status: Proposed for owner review
Date: 2026-09-22
Next ADR on acceptance: 0103

## Purpose

This slice proves the smallest useful socket boundary above the accepted
`NetworkPort`, Ethernet-II, ARP, IPv4, UDP, TCP, and DNS boundaries: a process
with an explicitly granted network capability may open one bounded TCP socket,
while a process that knows the exact endpoint but has no valid network
capability is denied before any socket handle or network frame is created.

The proof is runtime-only and finite. `VirtioTransport` remains the privileged
PythOS transport adapter, `NetworkPort` remains the copied Ethernet-frame
boundary, and TCP interpretation remains in one native service/consumer layer
above it. This design does not freeze a general public socket ABI.

## Architectural decision

The existing boot-local `NetworkPort READ | SEND` capability is the network
authority for this proof. No new capability right, resource namespace, syscall
number, PythTIG v1 record, or kernel socket object is added.

The socket-facing service accepts a private, bounded request record containing:

```text
operation: OPEN | SEND | RECEIVE | CLOSE
network_authority: existing NetworkPort capability
endpoint: fixed TCP endpoint from the accepted ADR 0101 profile
socket_handle: service-local generation-checked handle, where applicable
payload: fixed six-byte request/reply buffer, where applicable
```

These records are implementation-local proof data, not a frozen PythOS ABI or
new syscall layout. The service validates the supplied authority before
allocating a handle. Endpoint knowledge never substitutes for capability
authority.

The service owns exactly one private socket record:

```text
slot: 0
generation: 1
state: Closed | Opening | Established | Closing
authority: the existing NetworkPort capability, while valid
endpoint: the fixed ADR 0101 endpoint
```

An invalid, null, forged, wrong-holder, or revoked authority returns `DENIED`,
allocates no handle, emits no network frame, and cannot advance the protocol
state. A valid authority with the required existing `READ | SEND` rights may
allocate only the one service-local handle. A stale or wrong-generation handle
is rejected without touching the transport.

## Exact finite proof

The acceptance suite contains two serialized cases using the same exact
endpoint:

1. `socket-denied`: a process knows the endpoint but receives no
   `NetworkPort` capability. `OPEN` returns `DENIED`; the host oracle observes
   zero Ethernet frames; the process performs local cleanup and exits, with no
   `NetworkPort` capability revoked.
2. `socket-granted`: the owner grants the existing `NetworkPort READ | SEND`
   capability. `OPEN` succeeds, returns the one service-local handle, and the
   service performs the accepted ADR 0101 TCP exchange. `SEND`, `RECEIVE`, and
   `CLOSE` are admitted only with that handle and authority.

The granted case reuses the accepted TCP wire profile exactly:

```text
local MAC:       52:54:00:12:34:56
peer MAC:        02:00:00:00:00:02
local IPv4:      192.168.14.2
peer IPv4:       192.168.14.1
local TCP port:  0x1505
peer TCP port:   0x1506
request data:    PYTCPQ
reply data:      PYTCPR
frames:          2 ARP + 10 TCP, exactly as ADR 0101
```

The socket service does not construct an Ethernet frame as an API operation.
It translates the bounded socket operations into the existing protocol-layer
consumer operations, which then use `NetworkPort`. No raw-frame capability is
re-exported to the caller, and no socket listener, port allocator, or second
consumer is created.

## Operation contract

The private proof contract is deliberately finite:

```text
OPEN(endpoint, authority)
  valid authority + exact endpoint + NetworkPort Operational
    -> handle { slot: 0, generation: 1 }
  anything else -> DENIED; no allocation; no frame

SEND(handle, authority, b"PYTCPQ")
  valid handle + authority + Established -> OK
  anything else -> DENIED or BAD_HANDLE; no frame

RECEIVE(handle, authority)
  valid handle + authority + matching peer data -> six borrowed/copy-out bytes
  anything else -> DENIED, BAD_HANDLE, or protocol failure

CLOSE(handle, authority)
  valid handle + authority + accepted orderly FIN/ACK -> OK; invalidate handle
  anything else -> DENIED or BAD_HANDLE; no additional consumer
```

The implementation may use fixed private buffers and bounded polling. It must
not add variable-size socket buffers, blocking policy, retry/timer policy,
listener state, DNS resolution, routing, or persistent socket state.

## Capability and lifecycle ordering

The denied case must occur before owner capability issuance or queue admission
for that process. The granted case is ordered:

```text
owner grants existing NetworkPort capability
  -> service validates authority and exact endpoint
  -> one local socket handle is allocated
  -> TCP exchange uses NetworkPort through the service
  -> CLOSE invalidates the handle
  -> deny new requests
  -> revoke consumer authority
  -> stop queue admission through existing owner teardown
  -> owner RESET through the existing NetworkPort lifecycle
  -> terminal exit
```

The service never treats a socket handle as authority by itself. Revocation
invalidates both the authority path and the handle, and no request is admitted
after teardown begins.

## Markers

The two proof cases use distinct prefixes and exact marker order. The denied
case emits:

```text
PYTHOS:CORE:SOCKET:DENIED_BOOTSTRAPPED
PYTHOS:CORE:SOCKET:OPEN_WITHOUT_CAP_DENIED
PYTHOS:CORE:SOCKET:DENIED_TEARDOWN_COMPLETE
PYTHOS:CORE:SOCKET_DENIED_READY
```

The granted case emits:

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

Each marker occurs exactly once in its case. The denied case has no
`SOCKET_READY` granted marker and the granted case has no denied marker.

## Non-goals and stopping point

This slice does not add:

- a new public syscall, frozen ABI, PythTIG record, or new capability right;
- a reusable socket resource namespace or generalized handle allocator;
- listeners, accept queues, port namespaces, multiple sockets, or multiple
  consumers;
- routing, default-gateway behavior, firewall/NAT, DNS-based discovery, IPv6,
  UDP sockets, or complete host compliance;
- retransmission, timers, loss recovery, congestion control, reset/error
  service, or general TCP behavior;
- TLS or secure transport;
- physical NIC/Wi-Fi support, Lenovo hardware behavior, modern Virtio PCI,
  interrupts/MSI/MSI-X, multiqueue, offloads, DMA sharing, zero-copy, or
  generalized hardware memory behavior;
- persistent network or socket state, a Pyth runtime socket consumer, or
  Phase 15 work.

The stopping point is one explicit capability gate and one finite granted TCP
socket proof. Secure transport is the next separately authorized boundary.

## Decisions resolved here and decisions still deferred

This phase resolves only the finite proof policy: authority is required before
`OPEN`, endpoint knowledge is insufficient, the handle is service-local and
generation-checked, operations are bounded, and revocation invalidates the
handle and authority path.

The following remain deferred and are not silently generalized by this proof:

- reusable socket capability/resource allocation and lifetime/reuse rules;
- final public capability-right selection;
- public syscall numbers and fixed request/response layouts;
- general capability delivery/import into runtime services;
- maximum reusable copy sizes and receive-buffer behavior;
- reusable service teardown/revocation semantics;
- listener/port namespace and multi-consumer policy;
- selection of the first reusable Pyth runtime socket consumer.

## Standards basis

The granted exchange reuses the accepted TCP evidence and its RFC 9293 basis:

- §3.1 for TCP header fields, flags, sequence/acknowledgment numbers, window,
  and checksum inputs;
- §§3.3.2–3.4 for sequence-space/state handling and cumulative acknowledgment;
- §3.5 for the three-way handshake;
- §3.6 for orderly FIN/ACK close.

See [RFC 9293](https://www.rfc-editor.org/rfc/rfc9293.html). This proof does
not claim complete RFC 9293 endpoint compliance or interoperability beyond the
recorded finite exchange.

## Acceptance evidence

Acceptance requires host and QEMU evidence for both cases:

- denied case: exact endpoint request, no valid authority, `DENIED`, no handle,
  zero frames, exact denied markers, one successful terminal outcome, and clean
  teardown;
- granted case: exact 2 ARP + 10 TCP frames from ADR 0101, exact socket marker
  order, one successful terminal outcome, authority revocation, handle
  invalidation, no extra frame, no storage evidence, no non-boot Virtio block
  disk, snapshot-backed ESP, and clean teardown;
- default boot, recovery, normal-session boot, and every accepted earlier
  network proof remain unchanged;
- `VirtioTransport`, transport adapter, `NetworkPort`, existing NetworkPort
  ABI, frozen PythTIG v1, and Phase 15 boundaries remain unchanged.

The host may use a loopback TCP socket only as the QEMU frame oracle. That host
mechanism is not a PythOS socket interface.
