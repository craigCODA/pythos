# Phase 14 Bounded IPv4 Packet and Exchange Design

Date: 2026-09-16

Status: Accepted; implementation authorized by ADR 0098 and completed Task 6 local live evidence

## Decision in one sentence

Add a bounded IPv4 packet codec and one opt-in native QEMU exchange above the
accepted `NetworkPort`, using the accepted Ethernet-II and ARP boundaries;
leave ICMP and every later protocol or hardware layer for a separate decision.

## Context

Phase 14 has accepted the raw copied Ethernet boundary in ADR 0095, the
Ethernet-II consumer proof in ADR 0096, and the finite ARP proof in ADR 0097.
The next locked networking slice is `ip`, followed by `icmp` with ping as the
first working protocol proof. This document defines only that next IPv4
boundary.

The IP proof must demonstrate that a consumer can interpret a bounded IPv4
datagram carried by the existing Ethernet-II path, validate the fields that
are meaningful at this boundary, and exchange one deterministic datagram with
the QEMU peer. It must not turn the raw-frame capability into a socket API,
move protocol interpretation into PythCore, or imply that PythOS has a
persistent configured network interface.

The existing transport and port contracts are prerequisites, not targets of
this slice:

```text
VirtioTransport
        ↓
NetworkPort: bounded copied Ethernet bytes
        ↓
Ethernet-II
        ↓
ARP: one-shot address-resolution proof
        ↓
IPv4: this document
        ↓
ICMP, UDP, TCP, and later services: separate decisions
```

`VirtioTransport` remains the privileged PythOS transport adapter and fulfills
the Virtio specification's driver role. `NetworkPort` remains the
capability-scoped runtime resource. The IPv4 consumer is an unprivileged,
opt-in native process, not a new PythOS `Driver` abstraction.

## Goals

1. Define a pure, bounded codec for the canonical IPv4 header with no options
   and one bounded payload view.
2. Validate IPv4 version, header length, total length, fragmentation fields,
   protocol value, addresses, TTL, and header checksum before accepting the
   proof datagram.
3. Prove one direct Ethernet-II IPv4 request/reply exchange after one
   deterministic ARP setup exchange.
4. Reuse the accepted `NetworkPort` capability, copy-in/copy-out operations,
   Ethernet framing, bootstrap, and terminal revocation behavior unchanged.
5. Keep all IP interpretation in the native consumer and keep the proof
   finite, QEMU-only, and disabled on default and normal-session boot.

## Non-goals

This document does not authorize:

- ICMP, ping, UDP, TCP, DNS, DHCP, sockets, TLS, or a production network
  service;
- routing, forwarding, gateways, subnet discovery, multihoming, NAT, a
  firewall, or packet distribution;
- IPv4 fragmentation or reassembly, IPv4 options, IPv6, multicast, broadcast
  IP delivery, or a generalized protocol demultiplexer;
- a persistent IP address, subnet, route, ARP cache, retry policy, timer, or
  network configuration object;
- changes to the `NetworkPort` ABI, capability rights, syscall numbers,
  PythTIG v1, graph records, or launch/import mechanisms;
- physical NIC or Lenovo Wi-Fi support, modern Virtio PCI capabilities,
  interrupts, MSI/MSI-X, multiqueue, offloads, or a physical hardware memory
  model;
- implementation beyond the accepted bounded IPv4 proof, a generalized Phase
  15 hardware abstraction, or a new privileged networking component.

The canonical no-options/no-fragment subset is a deliberate proof boundary.
It is not a claim that this slice implements every IPv4 option or the complete
host requirements profile.

## Architectural decision

The IP slice has two logical pieces:

1. A pure IPv4 codec that reads and writes bounded byte slices, performs
   checked arithmetic, and has no capability, transport, device, or process
   authority.
2. An opt-in native `ipv4-probe.elf` consumer that uses the existing
   `NetworkPort` capability to perform one ARP setup and one IPv4 exchange.

The codec does not become a kernel ABI. The consumer does not receive a
separate `Ipv4Port`, socket handle, raw DMA address, PCI field, or persistent
address object. No PythTIG opcode or record is added.

The proof uses an IPv4 Protocol field value reserved for explicitly enabled
experimentation, `253`. This value identifies the test payload only; it does
not create a PythOS protocol registry or a production protocol. The probe is
opt-in and must remain disabled by default, consistent with the experimental
number guidance in RFC 3692 and RFC 4727.

## Components and ownership

### `VirtioTransport`

No transport change is authorized here. The accepted adapter continues to
own Virtio PCI mechanics, queue memory, device-visible ordering, private
Virtio-net headers, DMA buffers, completion polling, and transport failure
handling. It exposes only the already accepted `NetworkPort` operations to the
consumer.

### `NetworkPort`

No port change is authorized here. The consumer uses the existing:

- read-only bootstrap and `READ | SEND` capability;
- `DESCRIBE` operation to obtain the runtime MAC and bounded frame limits;
- `SEND` operation for one copied Ethernet frame at a time;
- nonblocking `TRY_RECEIVE` operation for one copied Ethernet frame at a time;
- owner-controlled teardown and terminal capability revocation.

The port remains usable only in `Operational`. It exposes Ethernet frame bytes
with the FCS excluded and never exposes Virtio headers, descriptors, queue
addresses, DMA mappings, MMIO, or completion state.

### IPv4 codec

The codec operates on an IPv4 datagram after the Ethernet-II header has been
validated and removed by the consumer. It returns borrowed field views or a
bounded encoding result. It does not allocate, retain a packet, perform
routing, or call `NetworkPort`.

The codec must reject malformed input before the consumer applies the fixed
proof policy. All offsets, lengths, and multiplication/addition operations
are checked before indexing or slicing.

### `ipv4-probe.elf`

The native consumer owns Ethernet-II and IPv4 proof policy above the port. It
performs no work on default or normal-session boot. Its identity is additive:

```text
IPV4_PROBE_PROGRAM_NAME = b"ipv4-probe.elf"
IPV4_PROBE_PRINCIPAL_ID = 0x5059_4950_5052_0001
```

This identity does not change the existing network-port, link-layer, or ARP
probe identities.

## Deterministic proof profile

### Address and link profile

The proof uses a private, directly connected IPv4 pair:

```text
local IPv4: 192.168.14.2
peer IPv4:  192.168.14.1
prefix:     /24, proof-only assumption
```

The addresses are an explicit, ephemeral QEMU test assignment from the
RFC 1918 private-use range. They do not establish PythOS IP configuration,
route state, or externally reachable networking. The peer hardware address is
the accepted deterministic ARP peer address:

```text
peer MAC: 02:00:00:00:00:02
local MAC: `NetworkPort::DESCRIBE.mac`
```

Before the IPv4 exchange, the probe performs exactly one ARP request and
accepts exactly one matching ARP reply using the same Ethernet-II and ARP
field rules established by ADR 0097, with this profile's IPv4 pair. The ARP
exchange is setup evidence for the next layer; it does not add an ARP cache,
retry policy, or reusable address-resolution service. The already accepted
ADR 0097 proof and its `192.0.2.x` documentation addresses remain unchanged.

### IPv4 datagram shape

Each proof datagram has:

```text
20 bytes  IPv4 header, IHL = 5, no options
8 bytes   fixed opaque experimental payload
28 bytes  IPv4 total length
18 bytes  Ethernet minimum-payload padding
60 bytes  complete software Ethernet frame, excluding FCS
```

The Ethernet-II envelope is:

| Offset | Size | Field | Required value |
| ---: | ---: | --- | --- |
| 0 | 6 | destination MAC | peer MAC on transmit; local MAC on receive |
| 6 | 6 | source MAC | local MAC on transmit; peer MAC on receive |
| 12 | 2 | EtherType | `0x0800`, big-endian |
| 14 | 28 | IPv4 datagram | exact profile below |
| 42 | 18 | Ethernet padding | all zero |

The software frame excludes the Ethernet FCS. The consumer neither creates
nor interprets an FCS.

### IPv4 header fields

Offsets below are relative to the first byte of the IPv4 datagram. All
multi-byte fields are network byte order.

| Offset | Size | Field | Request | Reply |
| ---: | ---: | --- | --- | --- |
| 0 | 1 | version/IHL | `0x45` | `0x45` |
| 1 | 1 | TOS/DSCP/ECN byte | `0x00` | `0x00` |
| 2 | 2 | total length | `28` | `28` |
| 4 | 2 | identification | `0x1401` | `0x1402` |
| 6 | 2 | flags/fragment offset | `0x0000` | `0x0000` |
| 8 | 1 | TTL | `64` | `64` |
| 9 | 1 | Protocol | `253` | `253` |
| 10 | 2 | header checksum | computed | computed |
| 12 | 4 | source address | `192.168.14.2` | `192.168.14.1` |
| 16 | 4 | destination address | `192.168.14.1` | `192.168.14.2` |
| 20 | 8 | payload | `50 59 54 48 49 50 52 51` | `50 59 54 48 49 50 52 50` |

The payload bytes are the fixed opaque tokens `PYTHIPRQ` and `PYTHIPRP`.
They are not an application protocol and are not handed to an ICMP,
transport, or socket layer.

The resulting header checksums are:

```text
request: C890
reply:   C88F
```

The exact frame patterns, with `LOCAL_MAC` replaced by the MAC returned from
`DESCRIBE`, are:

```text
TX:
02 00 00 00 00 02 LOCAL_MAC 08 00
45 00 00 1C 14 01 00 00 40 FD C8 90
C0 A8 0E 02 C0 A8 0E 01 50 59 54 48 49 50 52 51
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00

RX:
LOCAL_MAC 02 00 00 00 00 02 08 00
45 00 00 1C 14 02 00 00 40 FD C8 8F
C0 A8 0E 01 C0 A8 0E 02 50 59 54 48 49 50 52 50
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

The peer replies only to the exact request relationship. A response is
accepted only when its Ethernet addresses, EtherType, IPv4 fields, checksum,
and payload all match this profile.

## Codec contract

The pure codec exposes the following conceptual operations; exact Rust names
and crate placement belong to the implementation plan:

```text
encode_ipv4_packet(header_fields, payload, output)
decode_ipv4_packet(datagram)
ipv4_header_checksum(header_bytes)
```

`encode_ipv4_packet` must:

- require the canonical 20-byte header and bounded payload;
- write version 4 and IHL 5;
- compute the total length using checked arithmetic;
- write the checksum field as zero while summing 16-bit network-order words;
- write the one's-complement checksum into the header;
- reject a payload that cannot fit in the selected bounded profile.

`decode_ipv4_packet` must, before returning a view:

1. require at least 20 bytes;
2. read version and IHL without indexing beyond the input;
3. require version 4 and canonical IHL 5 (no options);
4. compute the header length with checked arithmetic and require it to fit in
   the input;
5. read total length and require it to be at least the header length and no
   greater than the supplied datagram length;
6. verify the one's-complement header checksum over the complete header;
7. return only bytes within the declared total length as the payload view.

The fixed proof policy then additionally requires the canonical IHL 5/no-options
condition, total length 28, zero flags and fragment offset, TTL 64, Protocol
253, the exact source and destination pair, the exact payload, and no nonzero
Ethernet padding. A datagram with options or fragmentation is rejected by the
codec/policy boundary before any higher-layer interpretation.

No payload checksum is invented at the IPv4 layer. RFC 791 defines the
checksum used here as a checksum of the IPv4 header only; payload integrity is
outside this slice.

## Consumer lifecycle and data flow

The probe is admitted only after `NetworkPort` is `Operational` and has a
valid `READ | SEND` capability. The finite flow is:

```text
bootstrap capability
  → DESCRIBE and validate bounded frame metadata
  → one ARP request/reply setup using the profile addresses
  → build exact IPv4 request with pure codec
  → Ethernet-II envelope + zero padding
  → NetworkPort SEND
  → one NetworkPort TRY_RECEIVE
  → Ethernet-II destination/source/EtherType validation
  → IPv4 codec validation
  → exact IPv4 reply policy validation
  → terminal capability revocation and process exit
```

The consumer may retain only the bounded private buffers needed for this
synchronous proof. It does not retain frames, ARP state, IP configuration,
routes, or peer identity after exit.

The lower-layer lifecycle is unchanged. `NetworkPort` operations are not used
before `Operational`; queue preparation, `DRIVER_OK`, notification, and
completion ordering remain the accepted `VirtioTransport` contract. The IP
consumer sees only completed copied frames.

## Error and rejection policy

- Capability, holder, generation, and missing-right failures are handled by
  the existing `NetworkPort` contract before transport mutation.
- Invalid pointers, lengths, permissions, and frame bounds are handled by the
  existing port ABI before the IPv4 consumer receives a frame.
- The codec rejects truncation, invalid version, invalid IHL, inconsistent
  total length, and bad header checksum without returning a payload view.
- The profile policy rejects non-Ethernet-II frames, wrong MAC direction,
  wrong EtherType, fragmented datagrams, IPv4 options, zero TTL, wrong
  protocol, wrong address pair, wrong fixed identifiers, wrong payload, and
  nonzero Ethernet padding.
- A malformed or nonmatching frame is not transformed into an ICMP error and
  is not forwarded. For the finite proof, it is a terminal probe failure;
  unit tests cover each rejection without requiring a second live exchange.
- No error path changes port reset, resource-id reuse, capability revocation,
  or transport failure semantics.

## Acceptance evidence

The acceptance profile is opt-in and QEMU-only. It must use the existing
legacy/transitional Virtio loopback path, no non-boot Virtio block disk, a
snapshot-backed boot ESP, and the existing storage-path oracle. Default and
normal-session boot must not launch this probe.

The proposed ordered marker subsequence is:

```text
PYTHOS:CORE:IPV4:BOOTSTRAPPED
PYTHOS:CORE:IPV4:DESCRIBE_OK
PYTHOS:CORE:IPV4:ARP_SETUP_OK
PYTHOS:CORE:IPV4:TX_OK
PYTHOS:CORE:IPV4:RX_OK
PYTHOS:CORE:IPV4:TEARDOWN_REVOKED
PYTHOS:CORE:IPV4_READY
```

The marker/oracle contract and bounded implementation are accepted by ADR 0098
and completed Task 6 local live evidence. No hosted IPv4 run is claimed.

The proof must show:

- a valid read-only bootstrap and existing `READ | SEND` capability;
- expected `NetworkPort::DESCRIBE` limits, flags, state, and local MAC;
- exactly one ARP setup request and one matching ARP setup reply;
- exactly one 60-byte IPv4 request with the fields and padding above;
- exactly one 60-byte IPv4 reply with the fields and padding above;
- valid header checksum calculation and verification;
- native unit coverage for truncated headers, invalid version/IHL/length,
  bad checksum, fragments, options, wrong protocol, wrong addresses, wrong
  payload, and nonzero Ethernet padding;
- terminal capability revocation with no replacement resource;
- no storage-path markers, panic/error markers, duplicate/reordered markers,
  extra peer transmit, or `QEMU_OUTCOME` failure.

The acceptance remains evidence of one controlled IPv4 packet exchange. It is
not evidence of a general IPv4 host, routing, IP configuration, or protocol
service.

## Migration and implementation boundary

The accepted `VirtioTransport`, `NetworkPort`, Ethernet-II, and ARP work is
not reopened. In particular, the following remain unchanged:

1. The Virtio initialization/queue ownership correction and
   `DRIVER_OK`/notification ordering contract.
2. The `NetworkPort` runtime resource, capability rights, copy-in/copy-out
   ABI, frame bounds, bootstrap, and teardown rules.
3. The accepted Ethernet-II and ARP marker contracts and named identities.

With this specification accepted by ADR 0098 and the bounded IPv4 proof
completed locally, the implementation record is:

1. pure IPv4 codec tests and the bounded native IPv4 policy are implemented;
2. exact frame tests and the opt-in QEMU peer/oracle are implemented;
3. the acceptance markers and terminal revocation proof are implemented;
4. the existing raw Virtio, port, link-layer, ARP, default-boot, and
   normal-session regressions remain covered;
5. local live evidence is recorded; no hosted IPv4 run is claimed before a
   separate hosted gate.

No Virtio transport refactor, `NetworkPort` ABI change, PythTIG change, boot
cutover, or Phase 15 hardware work belongs in that plan.

## Standards basis

The IPv4 field layout, total-length meaning, fragmentation fields, TTL,
Protocol field, and one's-complement header checksum follow [RFC 791 §3.1,
Internet Header Format](https://www.rfc-editor.org/rfc/rfc791.html#section-3.1).
RFC 791 defines total length as the header plus data and defines the checksum
over the header with the checksum field zero while calculating it.

The receive-side version and checksum rejection policy follows [RFC 1122
§3.2.1.1--§3.2.1.2, Internet Protocol](https://www.rfc-editor.org/rfc/rfc1122.html#section-3.2.1),
which requires a host to discard a datagram whose version is not 4 and to
verify and discard a datagram with a bad IP header checksum. This slice uses a
stricter fixed profile for deterministic evidence and does not claim the full
RFC 1122 host implementation.

The `192.168.14.0/24` proof addresses are within the private-use
`192.168.0.0/16` range specified by [RFC 1918 §3, Private Address
Space](https://www.rfc-editor.org/rfc/rfc1918.html#section-3). They are
ephemeral test values and are not persisted or routed.

Protocol value 253 is used only by the explicitly enabled QEMU experiment.
[RFC 3692 §2.1, IP Protocol Field](https://www.rfc-editor.org/rfc/rfc3692.html#section-2.1)
and [RFC 4727 §2.3, IPv4 Protocol Field](https://www.rfc-editor.org/rfc/rfc4727.html#section-2.3)
identify 253 and 254 as experimental IPv4 Protocol values. The probe stays
opt-in and does not ship a general/default interpretation for this value.

## Follow-up decisions deliberately left open

The following are intentionally not decided here:

- the final IP consumer capability/resource model, if a future reusable IP
  service needs more than the existing `NetworkPort` capability;
- persistent or operator-supplied address configuration and subnet ownership;
- routing, gateway selection, forwarding, multihoming, and fragmentation;
- ARP cache and retry/timer policy;
- ICMP and ping, then UDP, TCP, DNS, sockets, and secure transport;
- exact implementation crate/module names and the final acceptance-marker
  contract;
- physical NIC/Wi-Fi adapters, interrupts, modern Virtio, and all Phase 15
  hardware concerns;
- multi-consumer distribution, zero-copy leases, persistent state, and
  service teardown/revocation changes.

This accepted decision authorizes only the bounded IPv4 proof recorded above
and its completed local evidence. It does not authorize changes to the ABI,
transport, boot path, Phase 15 hardware, or the follow-up ADR scope. The
preserved scope and non-claims below remain in force, and no hosted IPv4 run is
claimed.
