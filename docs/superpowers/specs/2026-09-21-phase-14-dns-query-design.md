# Phase 14 Bounded DNS Query Proof Design

Status: Proposed for owner review
Date: 2026-09-21
Next ADR on acceptance: 0102

## Purpose

This slice proves one deterministic DNS-over-UDP query and response above the
accepted copied-frame `NetworkPort`, Ethernet-II, ARP, IPv4, and UDP proof
boundaries. It is a finite native client proof, not a resolver service or a
socket API.

`VirtioTransport` remains the privileged PythOS transport adapter. It exposes
only the existing bounded copied Ethernet-frame resource. DNS message parsing
and the finite acceptance policy remain in one opt-in native consumer above
`NetworkPort`.

## Non-goals and stopping point

This design does not add a PythOS socket or resolver API, listener, DNS server,
cache, resolver configuration, search-list behavior, retries, timers, a
general UDP service, multiple queries or consumers, routing, fragmentation,
IPv6, DNSSEC, TCP fallback, EDNS, dynamic updates, negative caching, physical
NIC/Wi-Fi support, modern or interrupt Virtio, multiqueue, offloads, zero-copy,
persistent state, a PythTIG v1 or syscall change, or Phase 15 work.

The proof stops after one valid A response is checked. Cleanup is ordered:
deny new requests, revoke the consumer capability, stop queue admission, then
perform the owner `RESET` and exit.

## Exact proof identity and lower-layer relationship

The opt-in identity is reserved for the implementation slice:

```text
DNS_PROBE_PROGRAM_NAME = b"dns-probe.elf"
DNS_PROBE_PRINCIPAL_ID = 0x5059_444E_5300_0001
DNS_CONSUMER_SERVICE_ID = 0x5059_444E_5343_0001
DNS_OWNER_SERVICE_ID    = 0x5059_444E_534F_0001
```

The consumer reuses the existing read-only bootstrap and `READ | SEND`
capability. It does not allocate or expose a socket, UDP port resource, DNS
cache, or persistent network state.

The proof uses one private deterministic relationship:

```text
local MAC:       52:54:00:12:34:56
peer MAC:        02:00:00:00:00:02
local IPv4:      192.168.14.2
peer IPv4:       192.168.14.1
local UDP port:  0x1605
peer UDP port:   53
IPv4 protocol:   17
```

These values establish neither address configuration nor a reusable DNS
endpoint. The peer is a host-side frame oracle only.

## Exact DNS message profile

The local query is exactly one DNS message:

```text
transaction ID: 0xD14E
flags:          0x0100  (RD=1; QR=0; all other flags zero)
QDCOUNT:        1
ANCOUNT:        0
NSCOUNT:        0
ARCOUNT:        0
QNAME:          pythos.example.   (labels: pythos, example, root)
QTYPE:          A (0x0001)
QCLASS:         IN (0x0001)
```

The query message is exactly 32 bytes: a 12-byte header and a 20-byte
question section. The QNAME is encoded as `06 pythos 07 example 00`; no name
compression appears in the query. Its exact DNS bytes are:

```text
d14e0100000100000000000006707974686f73076578616d706c650000010001
```

The peer response is exactly one DNS message:

```text
transaction ID: 0xD14E
flags:          0x8180  (QR=1, RD=1, RA=1, RCODE=0)
QDCOUNT:        1
ANCOUNT:        1
NSCOUNT:        0
ARCOUNT:        0
question:       byte-for-byte identical to the query question
answer NAME:    pointer 0xC00C to the QNAME at message offset 12
answer TYPE:    A (0x0001)
answer CLASS:   IN (0x0001)
answer TTL:     60 seconds (0x0000003C)
answer RDLENGTH: 4
answer RDATA:   192.0.2.14
```

The answer is exactly one 16-byte resource record. The response message is
therefore exactly 48 bytes and uses only the one specified compression pointer.
Its exact DNS bytes are:

```text
d14e8180000100010000000006707974686f73076578616d706c650000010001c00c000100010000003c0004c000020e
```
The returned `192.0.2.14` is documentation-space data for this proof; it does
not establish name resolution or network configuration.

## Exact UDP/IPv4 and frame profile

The DNS query and response are carried by the existing bounded UDP/IPv4
profile. The query UDP datagram is 40 bytes (8-byte UDP header plus 32-byte
DNS message), so the IPv4 total length is 60 bytes and the software Ethernet
frame is 74 bytes. The response UDP datagram is 56 bytes (8-byte UDP header
plus 48-byte DNS message), so its IPv4 total length is 76 bytes and its
software Ethernet frame is 90 bytes. IPv4 uses TTL 64, no fragmentation, and
IDs `0x1601` for the query and `0x1602` for the response. The exact checksums
are:

```text
query:    IPv4 0xC75C, UDP 0x8210, UDP length 40, IPv4 total 60, frame 74
response: IPv4 0xC74B, UDP 0x7F11, UDP length 56, IPv4 total 76, frame 90
```

UDP checksums use the existing IPv4 pseudo-header, Protocol 17, UDP length,
and checksum-zeroing rule. Both UDP datagrams are even-length, so no
conceptual checksum pad is needed for this exact profile. Ethernet FCS is
excluded and neither frame requires Ethernet minimum padding.

The complete exchange is exactly four Ethernet frames:

```text
1. local -> peer: ARP request
2. peer -> local: matching ARP reply
3. local -> peer: one DNS query over UDP/IPv4
4. peer -> local: one DNS response over UDP/IPv4
```

Any first nonmatching frame, wrong transaction ID, wrong flags/counts, bad
label encoding, unexpected compression pointer, wrong question, wrong answer,
bad checksum, wrong direction, duplicate, reorder, malformed length, or extra
frame is a terminal proof failure. Empty receive polling remains bounded by the
existing transport path and uses the terminal error/revocation path.

## Lifecycle and markers

The consumer emits exactly these seven markers once and in order:

```text
PYTHOS:CORE:DNS:BOOTSTRAPPED
PYTHOS:CORE:DNS:DESCRIBE_OK
PYTHOS:CORE:DNS:ARP_SETUP_OK
PYTHOS:CORE:DNS:QUERY_OK
PYTHOS:CORE:DNS:RESPONSE_OK
PYTHOS:CORE:DNS:TEARDOWN_REVOKED
PYTHOS:CORE:DNS_READY
```

`QUERY_OK` means the exact query was copied into a private bounded frame and
transmitted. `RESPONSE_OK` means the exact DNS response, UDP checksum, IPv4
fields, and Ethernet ownership checks passed. `DNS_READY` is emitted only
after reset, revocation, and terminal cleanup have succeeded. Error, panic,
timeout, transport-error, storage-path, duplicate-marker, and extra-frame
evidence fail acceptance.

## Standards basis

The proof uses RFC 1035:

- §4.1.1 for the fixed DNS header, flags, counts, and transaction ID;
- §4.1.2 for the question section, label encoding, QTYPE, and QCLASS;
- §4.1.3 for the resource-record owner, type, class, TTL, length, and data;
- §4.1.4 for the single permitted compressed owner-name pointer;
- §3.4.1 for the IPv4 A RDATA format;
- §4.2.1 for the DNS-over-UDP message transport relationship.

UDP checksum and IPv4 framing remain governed by the already accepted Phase 14
UDP and IPv4 records. This slice does not claim complete DNS resolver or host
compliance.

## Acceptance evidence

Acceptance requires a deterministic host-side frame oracle and one serialized
QEMU run that proves:

- exactly four frames and the exact two-direction ordering above;
- exact DNS bytes, label encoding, pointer, flags/counts, question, answer,
  IPv4 IDs, UDP ports/lengths/checksums, and 74/90-byte frame lengths;
- exact seven-marker order and one `QEMU_OUTCOME success`;
- no storage-path evidence, no non-boot Virtio block disk, snapshot-backed ESP,
  no PythOS storage writes, and clean QEMU/QMP/peer/temporary-state teardown;
- default boot and normal-session boot remain unchanged;
- `VirtioTransport`, transport adapter, `NetworkPort`, frozen ABI, syscall
  numbers/layouts, capability rights, and PythTIG v1 remain unchanged.

The host may use a loopback TCP socket only to carry the QEMU frame oracle. That
host mechanism is not a PythOS socket interface.
