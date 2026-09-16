# Phase 14 `link-layer` Design Specification

Status: Proposed for owner review  
Date: 2026-09-15

## Goal

Add the smallest semantic Ethernet/link-layer proof above the accepted
capability-scoped `NetworkPort` boundary. The proof must parse and validate
bounded Ethernet-II frames in an opt-in native user process while leaving
transport mechanics in `VirtioTransport` and leaving ARP, IP, sockets, and
physical hardware to later slices.

This is a semantic consumer proof, not a production network service.

## Existing boundary

ADR 0094 accepts the legacy/transitional QEMU `virtio-net-pci` transport.
ADR 0095 accepts one runtime-only `NetworkPort` capability with bounded
copy-in/copy-out Ethernet frames. `NetworkPort` owns no Ethernet-header
interpretation, protocol dispatch, or packet distribution policy.

The link-layer slice consumes the existing `NetworkPort` ABI. It does not
modify `VirtioTransport`, extend PythTIG v1, add a capability right, add a
syscall, or expose queue/DMA state.

## Decision

The first link-layer consumer is an opt-in native user process launched by the
existing creator-supplied bootstrap path. Its private code contains a small
Ethernet-II parser and a unicast proof policy. The process runs one finite
QEMU acceptance exchange and exits through the established probe mechanism;
it is not registered as a persistent service and does not establish a new
runtime service ABI.

The architectural names remain:

- `VirtioTransport` is the privileged PythOS transport adapter;
- `NetworkPort` is the capability-scoped raw Ethernet boundary; and
- the native process is a link-layer consumer, not a new PythOS `Driver`
  abstraction.

## Frame model

The consumer parses an Ethernet-II header from the bytes returned by
`NetworkPort`:

```text
destination MAC: 6 bytes, offset 0
source MAC:      6 bytes, offset 6
EtherType:       2 bytes, offset 12, big-endian
payload:         bytes after offset 14
```

The parser accepts only a complete frame buffer in the existing bounded range
of 60 through 1514 bytes. It rejects buffers shorter than the 14-byte header
or longer than 1514 bytes as a defense-in-depth rule, although the accepted
`NetworkPort` ABI already prevents those values from reaching the consumer.
The payload is borrowed only for the current synchronous proof operation and
is not retained.

The parser treats the EtherType as an opaque link-layer classification value.
It does not parse ARP, IPv4, IPv6, VLAN tags, ICMP, UDP, TCP, or any other
protocol. It does not interpret or manufacture an Ethernet FCS; the accepted
QEMU transport frame bytes remain the source of truth.

## Unicast proof policy

The opt-in proof uses the existing QEMU peer MAC and the MAC returned by
`NetworkPort::DESCRIBE`. It accepts only the deterministic unicast exchange:

- transmitted destination is the fixed peer MAC `02:00:00:00:00:02`;
- transmitted source is the described local MAC;
- received destination is the described local MAC;
- received source is the fixed peer MAC;
- the proof EtherType is `0x88B5`; and
- the proof payload is a fixed bounded test token.

Wrong destination, wrong source, or wrong EtherType is rejected by the
link-layer policy before any higher-layer handoff. A rejected frame produces
no protocol action because no protocol layer exists in this slice.

Broadcast, multicast, VLAN handling, EtherType registry policy, ARP, and
protocol demultiplexing are not defined here. ARP may establish the later
broadcast policy in its own accepted slice.

## Data flow

Transmit:

```text
link-layer policy
  -> serialize typed Ethernet-II fields into a private 60-byte buffer
  -> NetworkPort SEND
  -> QEMU peer validates exact frame bytes
```

Receive:

```text
NetworkPort TRY_RECEIVE
  -> private bounded frame buffer
  -> Ethernet-II parse
  -> unicast MAC and EtherType policy
  -> fixed proof-payload validation
```

The consumer never sees the ten-byte virtio-net header, PCI registers, queue
addresses, DMA addresses, or transport completion state.

## Capability and lifecycle policy

The consumer receives the existing `NetworkPort` `READ | SEND` capability
through the accepted bootstrap mechanism. The owner-only `WRITE` capability
remains kernel-owned. No second consumer, capability redistribution, resource
reuse, persistent network state, or new teardown behavior is introduced.

The link-layer proof is opt-in and leaves default and normal-session boot
unchanged. It uses the existing terminal probe lifecycle and does not alter
the `NetworkPort` ABI or the accepted reset/revocation semantics.

## Acceptance evidence

The new opt-in profile must prove:

- the named native consumer is launched with the existing read-only bootstrap;
- the consumer describes the port and obtains the described local MAC;
- the peer observes exactly one bounded Ethernet-II transmit frame;
- the consumer parses exactly one bounded peer-delivered frame;
- wrong destination and wrong EtherType are rejected before any higher-layer
  action;
- parser unit tests reject short headers and out-of-range buffers;
- the exact ordered link-layer markers and `QEMU_OUTCOME success` appear once;
- the QEMU peer uses loopback-only framing and no non-boot virtio data disk;
- no storage-path markers appear; and
- default and normal-session boot remain unchanged.

The acceptance profile may add a new named probe and marker sequence through a
successor ADR/implementation plan. It must not weaken or revise the frozen
`NetworkPort` marker sequence or ABI.

## Scope boundary

This specification does not add:

- ARP, IPv4, IPv6, ICMP, UDP, TCP, DNS, DHCP, routing, sockets, or TLS;
- a production network service, a generalized link-layer ABI, or a PythTIG
  host operation;
- physical NIC or Lenovo Wi-Fi support, modern Virtio PCI support, interrupts,
  MSI/MSI-X, multiqueue, offloads, or VLAN support;
- multiple network consumers, packet distribution, zero-copy, or persistent
  state; or
- default-boot or normal-session networking.

Those concerns remain separate later-phase or follow-up ADR work.

## Consequences

The `NetworkPort` boundary gains a concrete semantic consumer without moving
Ethernet interpretation into privileged transport code. The finite native
proof makes the next ARP slice possible while preserving a clean stopping
point: raw copied frames below, Ethernet-II semantics here, and protocol
semantics above this slice.

The fixed unicast policy intentionally does not solve broadcast or protocol
demultiplexing. Those behaviors require their own ownership and acceptance
decisions rather than being inferred from this proof.
