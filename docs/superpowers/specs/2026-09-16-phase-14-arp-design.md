# Phase 14 ARP Consumer Design Specification

Status: Proposed for owner review  
Date: 2026-09-16

## Goal

Add the smallest semantic ARP proof above the accepted capability-scoped
`NetworkPort` boundary. The proof performs one deterministic, ephemeral
Ethernet/IPv4 ARP request/reply exchange in an opt-in native user process.
It makes broadcast framing and ARP field validation concrete without turning
`NetworkPort` into a protocol interface or claiming production networking.

This slice follows the accepted Phase 14 legacy/transitional QEMU
`virtio-net-pci` and Ethernet-II link-layer proofs. It does not begin Phase 15
hardware expansion.

## Existing boundary

ADR 0094 accepts the legacy/transitional QEMU `virtio-net-pci` transport.
ADR 0095 accepts one runtime-only `NetworkPort` capability with bounded
copy-in/copy-out Ethernet frames. ADR 0096 accepts an opt-in native
Ethernet-II link-layer consumer that proves fixed unicast behavior.

`NetworkPort` exposes raw copied Ethernet frame bytes only. `VirtioTransport`
remains the privileged PythOS transport adapter and owns the PCI and Virtio
mechanics. Ethernet and ARP interpretation remain above that boundary in the
native consumer. The existing `NetworkPort` ABI, capability rights, syscall
numbers, PythTIG v1 records, and transport lifecycle are unchanged.

The PythOS architectural names remain:

- `VirtioTransport` is the privileged transport adapter;
- `NetworkPort` is the capability-scoped raw Ethernet boundary; and
- the native process is an ARP consumer, not a new PythOS `Driver` abstraction.

## Decision

The ARP proof is a separately named, opt-in native user process. It reuses the
accepted creator-supplied bootstrap, existing `NetworkPort` `READ | SEND`
capability, bounded request/response ABI, and terminal probe lifecycle. It
does not extend the accepted link-layer probe or alter its marker contract.

The consumer performs exactly one exchange:

```text
NetworkPort DESCRIBE
  -> obtain the local MAC
  -> serialize one ARP request in an Ethernet-II broadcast frame
  -> NetworkPort SEND
  -> NetworkPort TRY_RECEIVE
  -> parse Ethernet-II and ARP
  -> validate one matching ARP reply
  -> terminal capability revocation and process exit
```

There is no reusable ARP service, cache, retry policy, timeout ABI, routing
table, or state retained after the process exits. A malformed, unsupported, or
nonmatching received frame is a terminal error for this finite proof.

## Wire model

### Ethernet-II envelope

The consumer reuses the existing bounded Ethernet-II parser and frame model:

```text
destination MAC: 6 bytes, offset 0
source MAC:      6 bytes, offset 6
EtherType:       2 bytes, offset 12, big-endian
payload:         bytes after offset 14
```

ARP uses EtherType `0x0806`. The proof sends and receives a 60-byte minimum
Ethernet frame: 14 bytes of Ethernet header, 28 bytes of ARP payload, and 18
zero padding bytes. Padding is an Ethernet framing detail, not an ARP field;
the ARP parser consumes only the first 28 payload bytes. The QEMU acceptance
peer nevertheless validates the complete 60-byte frame, including zero
padding, as deterministic test evidence. Ethernet FCS is neither interpreted
nor manufactured by the consumer.

### ARP payload

The bounded ARP codec handles the fixed Ethernet/IPv4 form only:

| Offset | Size | Field | Required value |
| ---: | ---: | --- | --- |
| 0 | 2 | hardware type | `1` (Ethernet), big-endian |
| 2 | 2 | protocol type | `0x0800` (IPv4), big-endian |
| 4 | 1 | hardware length | `6` |
| 5 | 1 | protocol length | `4` |
| 6 | 2 | operation | `1` request or `2` reply, big-endian |
| 8 | 6 | sender hardware address | MAC bytes |
| 14 | 4 | sender protocol address | IPv4 bytes |
| 18 | 6 | target hardware address | MAC bytes |
| 24 | 4 | target protocol address | IPv4 bytes |

The pure codec decodes a payload only when at least 28 bytes are available.
The consumer policy rejects unsupported hardware/protocol types or lengths;
it does not generalize the codec to other address families or link types.

The fixed proof addresses are documentation-only frame values from the TEST-NET
range:

```text
local IPv4: 192.0.2.2
peer IPv4:  192.0.2.1
```

They do not establish an IP configuration and are not used for IP packet
processing.

### Exact request and reply

The request transmitted by the consumer is:

```text
Ethernet destination: ff:ff:ff:ff:ff:ff
Ethernet source:      described local MAC
EtherType:            0x0806
ARP operation:        request (1)
ARP sender MAC:       described local MAC
ARP sender IPv4:      192.0.2.2
ARP target MAC:       00:00:00:00:00:00
ARP target IPv4:      192.0.2.1
```

The reply supplied by the loopback-only QEMU peer is:

```text
Ethernet destination: described local MAC
Ethernet source:      02:00:00:00:00:02
EtherType:            0x0806
ARP operation:        reply (2)
ARP sender MAC:       02:00:00:00:00:02
ARP sender IPv4:      192.0.2.1
ARP target MAC:       described local MAC
ARP target IPv4:      192.0.2.2
```

The consumer accepts only this exact relationship. It does not treat any
other broadcast, multicast, ARP operation, address pair, or hardware address
as a successful proof.

## Data flow and ownership

Transmit:

```text
ARP policy
  -> construct a private 28-byte request payload
  -> construct a private 60-byte Ethernet frame
  -> NetworkPort SEND copies the frame through the accepted ABI
  -> QEMU peer validates the exact broadcast request
```

Receive:

```text
NetworkPort TRY_RECEIVE
  -> private bounded frame buffer
  -> Ethernet-II parse
  -> require local destination, peer source, and EtherType 0x0806
  -> ARP parse of the first 28 payload bytes
  -> require the exact peer reply relationship
```

The parsed payload is borrowed only for the synchronous validation operation.
No descriptor, DMA address, Virtio header, PCI field, or transport completion
is visible to the consumer. The private request and receive buffers are not
retained after the finite proof.

## Capability and lifecycle policy

The consumer receives the existing read-only bootstrap and `READ | SEND`
`NetworkPort` capability through the accepted named-probe mechanism. The
owner-only `WRITE` capability remains kernel-owned. No second consumer,
capability redistribution, resource reuse, or new teardown behavior is added.

The consumer is launched only by the opt-in ARP acceptance profile. Default
boot and normal-session boot remain unchanged. After the native process
returns through the established terminal probe path, the existing owner reset
and capability revocation proof is used. This slice does not broaden reset
semantics.

## Marker and acceptance contract

The additive ARP marker sequence is:

```text
PYTHOS:CORE:ARP:BOOTSTRAPPED
PYTHOS:CORE:ARP:DESCRIBE_OK
PYTHOS:CORE:ARP:REQUEST_OK
PYTHOS:CORE:ARP:REPLY_OK
PYTHOS:CORE:ARP:TEARDOWN_REVOKED
PYTHOS:CORE:ARP_READY
```

The opt-in QEMU profile must prove, with exact-once ordering, that:

- the named ARP consumer received a valid read-only bootstrap;
- `DESCRIBE` returned the expected bounded frame limits, flags, state, and
  local MAC;
- the peer observed exactly one 60-byte broadcast ARP request with the exact
  bytes specified above;
- the consumer accepted exactly one 60-byte matching ARP reply;
- the consumer reached terminal revocation through the existing lifecycle;
- all six ARP markers appear exactly once in the listed order;
- no error or storage-path markers appear;
- the QEMU command uses the legacy/transitional loopback-only Virtio network
  path with no non-boot Virtio block disk; and
- the run ends with `QEMU_OUTCOME success`.

Native unit tests must cover:

- network-byte-order encoding and decoding of every ARP field;
- exact request serialization, including zero target hardware address;
- exact reply matching against local and peer MAC/IP values;
- rejection of bodies shorter than 28 bytes;
- rejection of unsupported hardware/protocol types or lengths;
- rejection of the wrong operation, sender, target, or address pair; and
- preservation of Ethernet padding outside the ARP payload boundary.

The existing raw Virtio, `NetworkPort`, link-layer, default-boot, and
normal-session acceptance profiles remain required regressions. They are not
weakened or replaced by the ARP profile.

## Scope boundary and non-claims

This specification does not add:

- ARP caching, retries, timers, timeout behavior, gratuitous ARP, proxy ARP,
  reverse ARP, DHCP, IPv4, IPv6, ICMP, UDP, TCP, DNS, routing, sockets, TLS,
  or protocol multiplexing;
- a production network service, generalized protocol ABI, new syscall,
  capability right, resource-id policy, PythTIG host operation, or PythTIG v1
  record;
- multiple network consumers, packet distribution, zero-copy leases,
  persistent network state, service teardown/revocation policy, or a
  selection policy for a first production consumer;
- physical NIC or Lenovo Wi-Fi support, modern Virtio PCI capabilities,
  interrupts, MSI/MSI-X, multiqueue, offloads, VLAN behavior, or a physical
  hardware memory model; or
- default-boot or normal-session networking.

Those matters remain follow-up ADR work or later-phase work. In particular,
Phase 15 remains the separate physical-hardware expansion phase.

## Consequences

The Phase 14 boundary gains a concrete ARP semantic proof while keeping
protocol interpretation in an unprivileged consumer above `NetworkPort`.
Broadcast delivery, ARP field ownership, and exact request/reply behavior are
now testable without changing the frozen transport or capability ABI.

The finite one-peer exchange deliberately stops before reusable networking
state or higher-layer protocol behavior. A later design can build on the
validated frame boundary without inheriting an accidental cache, service, or
physical-hardware contract from this proof.
