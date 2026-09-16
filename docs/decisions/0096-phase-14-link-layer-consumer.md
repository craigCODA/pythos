# ADR 0096: Phase 14 Link-Layer Consumer Proof

Date: 2026-09-15

Status: Accepted by owner on 2026-09-15; implementation follows the separately accepted plan

## Context

ADR 0095 accepts the capability-scoped `NetworkPort` raw Ethernet boundary.
The approved Phase 14 link-layer design now defines the smallest semantic
consumer above that boundary. This ADR records that successor decision. It
does not change the `NetworkPort` ABI, `NetworkPort` identity, Virtio
transport mechanics, PythTIG v1, default boot, or normal-session boot.

## Decision

The first link-layer consumer is an opt-in native user process launched by
the existing creator-supplied bootstrap path. Its private code parses bounded
Ethernet-II frames and proves a fixed unicast exchange. It runs one finite
QEMU acceptance exchange and exits through the established probe lifecycle;
it is not a persistent service and does not establish a new runtime service
ABI.

The architectural names remain:

- `VirtioTransport` is the privileged PythOS transport adapter.
- `NetworkPort` is the capability-scoped raw Ethernet boundary.
- The native process is a link-layer consumer, not a new PythOS `Driver`
  abstraction.

## Frame model

The consumer parses Ethernet-II bytes returned by `NetworkPort`:

```text
destination MAC: 6 bytes, offset 0
source MAC:      6 bytes, offset 6
EtherType:       2 bytes, offset 12, big-endian
payload:         bytes after offset 14
```

Only complete frame buffers from 60 through 1514 bytes are accepted. Buffers
shorter than the 14-byte header or longer than 1514 bytes are rejected as a
defense-in-depth rule. The payload is borrowed only for the synchronous proof
operation and is not retained. EtherType is an opaque link-layer
classification; this slice does not parse ARP, IP, VLAN, or any higher
protocol, and does not interpret or manufacture an Ethernet FCS.

## Fixed unicast policy

The proof uses the existing QEMU peer MAC and the MAC returned by
`NetworkPort::DESCRIBE`:

- transmit destination is `02:00:00:00:00:02`;
- transmit source is the described local MAC;
- receive destination is the described local MAC;
- receive source is `02:00:00:00:00:02`;
- the proof EtherType is `0x88B5`; and
- the proof payload is a fixed bounded test token.

Wrong destination, wrong source, and wrong EtherType are rejected before any
higher-layer handoff. Broadcast, multicast, VLAN handling, EtherType registry
policy, ARP, and protocol demultiplexing are outside this ADR.

## Capability and lifecycle policy

The consumer receives the existing `NetworkPort` `READ | SEND` capability
through the accepted bootstrap mechanism. The owner-only `WRITE` capability
remains kernel-owned. No second consumer, capability redistribution, resource
reuse, persistent network state, or new teardown behavior is introduced.

The proof is opt-in, leaves default and normal-session boot unchanged, uses
the existing terminal probe lifecycle, and preserves the accepted reset and
revocation semantics. The consumer never sees the private virtio-net header,
PCI registers, queue or DMA addresses, or transport completion state.

The additive named-program identity is frozen as:

```text
LINK_LAYER_PROBE_PROGRAM_NAME = b"link-layer-probe.elf"
LINK_LAYER_PROBE_PRINCIPAL_ID = 0x5059_4C4C_5052_0001
```

This identity is additive. `NETWORK_PORT_PROBE_PROGRAM_NAME` and
`NETWORK_PORT_PROBE_PRINCIPAL_ID` remain unchanged.

## Acceptance evidence

The opt-in profile must prove that the named native consumer is launched
with the existing read-only bootstrap, describes the port, obtains the local
MAC, transmits exactly one bounded Ethernet-II frame, parses exactly one
bounded peer-delivered frame, and rejects wrong destination and wrong
EtherType before higher-layer action. It must emit the following ordered
markers exactly once, followed by `QEMU_OUTCOME success`:

```text
PYTHOS:CORE:LINK_LAYER:BOOTSTRAPPED
PYTHOS:CORE:LINK_LAYER:DESCRIBE_OK
PYTHOS:CORE:LINK_LAYER:TX_OK
PYTHOS:CORE:LINK_LAYER:WRONG_DESTINATION_DENIED
PYTHOS:CORE:LINK_LAYER:WRONG_ETHERTYPE_DENIED
PYTHOS:CORE:LINK_LAYER:RX_OK
PYTHOS:CORE:LINK_LAYER:TEARDOWN_REVOKED
PYTHOS:CORE:LINK_LAYER_READY
```

Parser unit tests must reject short headers and out-of-range buffers. The
QEMU peer uses loopback-only framing and no non-boot virtio data disk. No
storage-path markers may appear. Default and normal-session boot must remain
unchanged, and the frozen `NetworkPort` marker sequence and ABI must not be
weakened or revised.

## Scope boundary and non-claims

This ADR does not add ARP, IPv4, IPv6, ICMP, UDP, TCP, DNS, DHCP, routing,
sockets, TLS, a production network service, a generalized link-layer ABI, a
PythTIG host operation, physical NIC or Lenovo Wi-Fi support, modern Virtio
PCI support, interrupts, MSI/MSI-X, multiqueue, offloads, VLAN support,
multiple consumers, packet distribution, zero-copy, persistent state, or
default/normal-session networking. It does not claim physical networking,
protocol semantics, or production network-service readiness.

## Consequences

The `NetworkPort` boundary gains a concrete semantic consumer while Ethernet
interpretation remains outside privileged transport code. The finite native
proof establishes a stopping point for this slice: raw copied frames below,
Ethernet-II semantics here, and protocol semantics above it. Broadcast and
protocol demultiplexing remain separate future ownership and acceptance
decisions.
