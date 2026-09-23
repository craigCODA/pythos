# Phase 14 Secure-Transport Proof Design

Status: Accepted

## Purpose

This slice proves one finite, client-only TLS 1.3 exchange above the accepted
capability-gated TCP/socket proof. It demonstrates that a process with the
existing `NetworkPort READ | SEND` authority can establish one authenticated
protected stream, exchange one bounded request/reply, reject altered protected
data, and revoke the authority during teardown.

The result is an acceptance proof, not a public TLS API, a reusable socket
service, or a production update channel. `VirtioTransport` remains the
privileged transport adapter and `NetworkPort` remains the raw bounded copied
frame boundary. TLS semantics remain in the private native consumer above the
accepted TCP/socket path.

## Architectural boundary

```text
VirtioTransport / transport adapter
        |
        v
runtime-only NetworkPort capability
        |
        v
private bounded TCP/socket stream
        |
        v
finite TLS 1.3 client proof
```

The proof may add an opt-in native consumer, test harness, and acceptance-only
feature gates. It must not modify the existing `NetworkPort` ABI, frozen PythTIG
v1 ABI, capability-right definitions, Virtio lifecycle, or transport adapter.

## Frozen protocol profile

The implementation must use a reviewed no-`std`, no-allocator TLS 1.3 client
backend. Hand-written cryptography is prohibited. The selected backend and
its exact version must be recorded in the ADR after a target build and API
review.

The acceptance profile is:

- TLS version 1.3 only;
- one client connection and one full 1-RTT handshake;
- one explicitly authenticated test server using a pinned test identity;
- one approved TLS 1.3 AEAD suite selected by the backend profile;
- cryptographically secure randomness is required for any real deployment
  backend;
- deterministic xorshift randomness is permitted only in the finite secure
  acceptance image and deterministic host/unit/self-tests because Phase 14 has
  no approved entropy source; this is a protocol-proof limitation and provides
  no deployment-security claim;
- one bounded encrypted request and one bounded encrypted response;
- bounded TCP reassembly that accepts segmentation and coalescing;
- sequence-derived record nonces and AEAD tag validation performed by the TLS
  backend before application bytes are released;
- no 0-RTT, session resumption, tickets, post-handshake authentication,
  KeyUpdate, client certificates, certificate store, DNS, or clock-dependent
  trust policy;
- no application data sent before handshake completion;
- a tampered ciphertext/tag case that is rejected without releasing plaintext.

The pinned identity is an acceptance fixture only. It does not establish a
trust store, key provisioning policy, update authenticity, or persistent key
lifetime. Phase 16 image signing and verification remains independent.

## Private consumer lifecycle

The consumer owns one service-local connection record and fixed buffers only:

1. validate the existing bootstrap header and capability;
2. open the private socket authority and complete the already accepted bounded
   TCP exchange;
3. drive the TLS 1.3 client handshake through the bounded TCP stream;
4. emit `TLS_HANDSHAKE_OK` only after server identity and transcript checks
   succeed;
5. send one bounded plaintext request through the TLS writer;
6. receive and authenticate one bounded encrypted response through the TLS
   reader;
7. run a separate tamper case in which the protected response is altered and
   confirm that the TLS reader returns an authentication error without making
   application bytes available;
8. close the stream, revoke the existing authority, invalidate the local
   handle, and exit.

The denied profile stops before TCP or TLS setup, as in the accepted socket
proof. It must produce zero Ethernet frames and no TLS markers.

## Bounded buffers and record handling

The consumer uses fixed storage sized for the selected backend's minimum valid
TLS record and the exact finite handshake fixture. It must reject oversized
records, truncated records, unexpected content types, handshake messages after
the bounded handshake point, and application data before handshake completion.

TCP is a byte stream: the consumer must not assume one TCP segment equals one
TLS record. It must accumulate only the bounded number of bytes needed by the
next TLS record and pass complete records to the backend. It must not expose a
partial or unauthenticated plaintext buffer to the service.

## Acceptance markers

Granted success markers, exactly once and in order:

```text
PYTHOS:CORE:SECURE:BOOTSTRAPPED
PYTHOS:CORE:SECURE:OPEN_GRANTED
PYTHOS:CORE:SECURE:TCP_READY
PYTHOS:CORE:SECURE:TLS_HANDSHAKE_OK
PYTHOS:CORE:SECURE:REQUEST_ENCRYPTED
PYTHOS:CORE:SECURE:RESPONSE_DECRYPTED
PYTHOS:CORE:SECURE:CLOSE_OK
PYTHOS:CORE:SECURE:TEARDOWN_REVOKED
PYTHOS:CORE:SECURE_READY
```

Tamper-case markers, exactly once and in order:

```text
PYTHOS:CORE:SECURE:BOOTSTRAPPED
PYTHOS:CORE:SECURE:OPEN_GRANTED
PYTHOS:CORE:SECURE:TCP_READY
PYTHOS:CORE:SECURE:TLS_HANDSHAKE_OK
PYTHOS:CORE:SECURE:REQUEST_ENCRYPTED
PYTHOS:CORE:SECURE:TAMPER_REJECTED
PYTHOS:CORE:SECURE:TEARDOWN_REVOKED
PYTHOS:CORE:SECURE_TAMPER_READY
```

Denied-case markers, exactly once and in order:

```text
PYTHOS:CORE:SECURE:DENIED_BOOTSTRAPPED
PYTHOS:CORE:SECURE:OPEN_WITHOUT_CAP_DENIED
PYTHOS:CORE:SECURE:DENIED_TEARDOWN_COMPLETE
PYTHOS:CORE:SECURE_DENIED_READY
```

The host oracle must additionally prove that the protected request and
response bytes differ from their plaintext, that the valid response decrypts
to the exact expected bytes, and that the tampered response produces no
application response or success marker.

## Explicit non-goals

This slice does not add:

- a public TLS/socket syscall, ABI, resource namespace, or reusable service;
- a general certificate store, CA bundle, hostname/DNS policy, clock, or key
  provisioning system;
- persistent keys, session tickets, resumption, 0-RTT, KeyUpdate, or multiple
  peers/consumers;
- HTTP, package download, update installation, image signing, rollback, or
  recovery behavior;
- changes to `VirtioTransport`, the transport adapter, `NetworkPort`, PythTIG,
  physical NIC/Wi-Fi, Lenovo hardware, modern Virtio PCI, interrupts,
  multiqueue, offloads, DMA sharing, or generalized hardware memory behavior;
- hosted/remote networking claims beyond the exact local QEMU oracle.

## Standards basis

The protocol profile is based on:

- RFC 8446 Section 2 for the TLS 1.3 protocol version and handshake/record model;
- RFC 8446 Sections 4.1–4.4 for the client handshake and server authentication;
- RFC 8446 Sections 5.1–5.5 for bounded record processing, AEAD protection,
  sequence-number-derived per-record nonces, and authenticated record usage;
- RFC 8446 Section 7.1 for TLS 1.3 key schedule terminology;
- RFC 8446 Section 8 for the explicit exclusion of 0-RTT in this proof;
- RFC 8439 Section 2.8 if the selected backend profile uses
  `AEAD_CHACHA20_POLY1305`;
- RFC 8448 for deterministic TLS 1.3 transcript/test-vector self-tests.

These references constrain the proof; they do not make the finite consumer a
complete TLS implementation or an interoperable production client.

## Acceptance requirements

Acceptance requires:

- backend unit tests, including the selected RFC vectors, pass;
- the native consumer builds for `x86_64-unknown-none` with the opt-in feature
  and with the denied companion feature;
- the granted QEMU case completes the authenticated handshake and exact
  encrypted request/reply;
- the tamper QEMU case rejects the altered protected record and releases no
  application bytes;
- the denied QEMU case has zero frames and no TLS activity;
- the granted and tamper QEMU cases each exchange exactly 11 guest TX frames,
  10 host RX frames, and 21 total frames; the denied case remains at zero;
- all three cases emit exact marker order and one successful terminal outcome;
- no storage, panic, timeout, transport-error, or extra-frame evidence occurs;
- default boot, normal-session boot, all accepted earlier networking proofs,
  `VirtioTransport`, the transport adapter, `NetworkPort`, the existing ABI,
  and Phase 15 boundaries remain unchanged.
