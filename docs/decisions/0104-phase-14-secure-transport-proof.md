# ADR 0104: Phase 14 Secure-Transport Proof

Status: Accepted

## Decision

The final accepted Phase 14 networking slice is one finite, client-only TLS 1.3 proof
above the accepted capability-gated socket/TCP path. The proof uses
`embedded-tls` `0.19.0` with default features disabled and the `rustpki`
feature, compiled for `x86_64-unknown-none`. Its PythOS consumer remains an
opt-in native probe and does not add a public TLS API, socket syscall, network
resource namespace, or new capability right.

`embedded-tls` was selected because the checked-in target/API gate confirms all
of the following without hand-written cryptography:

- the no-`std`, no-allocator library compiles for `x86_64-unknown-none`;
- its blocking connection accepts a custom `embedded_io::Read + Write`
  transport adapter;
- its TLS 1.3 client exposes a `TlsVerifier` hook;
- its no-`std` `rustpki::CertVerifier` path compiles for the guest target;
- its fixed-buffer API can be driven above the existing copied TCP stream.

The `webpki` feature is deliberately not selected. Its `ring`/`getrandom`
dependency chain does not support the guest target. The crate describes itself
as work in progress; therefore this ADR records a finite protocol/acceptance
proof only and does not claim production-grade TLS, update authenticity, or a
security-reviewed reusable transport implementation.

## Frozen proof profile

- TLS 1.3 only;
- `TLS_AES_128_GCM_SHA256` with the backend's P-256 key exchange;
- one full 1-RTT client handshake;
- one self-signed P-256 test certificate with the fixed SHA-256 DER fingerprint
  `05fbe163a52218a9f419c17b540e73b963f9a265a43b7bc05587934b07ea7a4e`, pinned
  byte-for-byte and verified by the backend's `rustpki::CertVerifier` using
  `NoClock`; the trust anchor is the same exact DER fixture passed to
  `CertVerifier::new(Certificate::X509(...))`;
- one bounded encrypted request and one bounded encrypted response;
- one independent tamper case in which a protected response byte is changed;
- no 0-RTT, resumption, tickets, KeyUpdate, client certificate, CA bundle,
  hostname discovery, DNS, or persistent key state;
- deterministic RNG only in the acceptance image and unit/self-tests because
  PythOS has no approved entropy source in this phase; this is an explicit
  protocol proof limitation, not a deployment security claim;
- all TLS state and buffers are private to one finite native consumer.

The certificate is an acceptance fixture, not a trust-store or key-provisioning
decision. Phase 16 image signing and verification remains independent; this
proof cannot authorize package updates or recovery behavior.

## Scope and ownership

The existing `VirtioTransport`, transport adapter, `NetworkPort`, NetworkPort
ABI, capability rights, frozen PythTIG v1 ABI, and legacy Virtio lifecycle are
unchanged. The consumer owns only a service-local connection record and fixed
buffers. It uses the existing `NetworkPort READ | SEND` capability and the
existing capability revocation/teardown path.

The host TCP socket remains only the QEMU Virtio frame oracle. No hosted or
remote-network acceptance is claimed.

## Acceptance boundary

Three serialized cases are required:

1. granted: authenticated handshake, protected request/reply, close, revoke;
2. tamper: authenticated handshake, altered protected response rejected with
   no plaintext release, revoke;
3. denied: no NetworkPort authority, zero Ethernet/TCP/TLS frames, teardown.

Every case requires exact markers, one `QEMU_OUTCOME success`, no storage or
panic evidence, and clean serial/ESP artifact teardown. Earlier Phase 14
proofs and default/normal-session boot must remain green.

The accepted local evidence is:

- granted: 21 frames, exactly nine ordered markers, one `QEMU_OUTCOME
  success`, and clean serial/ESP artifact teardown;
- tamper: 21 frames, exactly eight ordered markers including
  `REQUEST_ENCRYPTED > TAMPER_REJECTED`, no plaintext release, one
  `QEMU_OUTCOME success`, and clean serial/ESP artifact teardown;
- denied: zero frames, exactly four ordered markers, one `QEMU_OUTCOME
  success`, and clean serial/ESP artifact teardown.

The tamper image uses the private `secure-transport-tamper-probe` core profile
to select `SECURE_TAMPER_READY`; the granted profile selects `SECURE_READY`.
This distinction is acceptance-only and adds no public ABI or service surface.

The finite TLS proof uses the existing guarded user-stack contract with 64 KiB
(16 pages) of usable stack headroom and the retained unmapped guard page. This
is bounded static headroom for the proof; it does not change the Phase 8
guard-page authority/permission contract or add dynamic stack allocation.

The tamper case's frozen marker order is
`BOOTSTRAPPED > OPEN_GRANTED > TCP_READY > TLS_HANDSHAKE_OK >
REQUEST_ENCRYPTED > TAMPER_REJECTED > TEARDOWN_REVOKED > SECURE_TAMPER_READY`.

## Standards basis

The profile follows RFC 8446 §§2, 4.1–4.4, 5.1–5.5, 7.1, and 8. The TLS 1.3
AEAD and sequence-number nonce requirements are enforced by the selected
backend. RFC 8448 is used only for deterministic transcript/vector self-tests.

## Deferred decisions

This ADR does not decide public TLS/socket ABI design, reusable capability or
resource allocation, entropy provisioning, certificate/key lifecycle, CA
policy, update transport, package signing, session resumption, multiple peers,
or any Phase 15 hardware behavior.

This accepted bounded proof is Phase 14's current stopping point. It does not
establish production TLS, update authenticity, physical networking, or a
generalized socket/TLS service. Phase 15 hardware expansion, including Lenovo
Wi-Fi, remains separate.
