# Phase 14 Bounded UDP Datagram Proof Design

Date: 2026-09-21

Status: Accepted for implementation under the Phase 14 continuation authority

## Decision in one sentence

Add one opt-in native UDP datagram request/reply proof above the accepted
`NetworkPort`, Ethernet-II, ARP, and IPv4 boundaries; leave reusable UDP,
socket, port-namespace, ICMP-error, later transport, physical hardware, and
Phase 15 behavior outside this slice.

## Boundary and non-goals

The existing path remains:

```text
VirtioTransport
    ↓
NetworkPort: bounded copied Ethernet bytes
    ↓
Ethernet-II
    ↓
ARP: one setup request/reply
    ↓
IPv4: one canonical datagram
    ↓
UDP: this proof
```

`VirtioTransport` remains the privileged PythOS transport adapter and fulfills
the Virtio specification's driver role. `NetworkPort` remains the
capability-scoped runtime resource with copied frames. UDP interpretation stays
in the opt-in native consumer; no new PythOS `Driver` abstraction, ABI,
PythTIG record, syscall, capability right, transport behavior, or persistent
network state is introduced.

This slice does not authorize a UDP service, receive-port namespace, port
allocator, socket API, multiplexing policy, ICMP Port Unreachable generation
or delivery, retry/timer policy, routing, fragmentation, multicast/broadcast
delivery, TCP, DNS, TLS, physical NIC/Wi-Fi, modern Virtio PCI, interrupts,
multiqueue, offloads, zero-copy, multiple consumers, or any Phase 15 hardware
model.

The fixed source and destination ports below are proof constants only. They do
not establish a reusable UDP port interface or a PythOS socket contract.

## Exact proof identity

The additive native proof identity is:

```text
UDP_PROBE_PROGRAM_NAME = b"udp-probe.elf"
UDP_PROBE_PRINCIPAL_ID = 0x5059_5544_5000_0001
UDP_CONSUMER_SERVICE_ID = 0x5059_5544_4353_0001
UDP_OWNER_SERVICE_ID    = 0x5059_5544_4F57_0001
```

The accepted ARP, IPv4, and ICMP proofs remain separate and unchanged. The UDP
probe is disabled on default and normal-session boot and runs only after the
existing operational `NetworkPort` capability is installed.

## Exact address, frame, IPv4, and UDP profile

The proof reuses the deterministic QEMU relationship from ADR 0098:

```text
described local MAC: 52:54:00:12:34:56
peer MAC:            02:00:00:00:00:02
local IPv4:          192.168.14.2
peer IPv4:           192.168.14.1
proof-only prefix:   /24
```

The setup is exactly one matching ARP request and one matching ARP reply. It
does not establish persistent IP, ARP, or UDP state. The UDP exchange is one
request datagram and one deterministic reply datagram carried in IPv4:

| Field | Request | Reply |
| --- | --- | --- |
| Ethernet EtherType | `0x0800` | `0x0800` |
| IPv4 version/IHL | `0x45` | `0x45` |
| IPv4 TOS/DSCP/ECN | `0x00` | `0x00` |
| IPv4 total length | `35` | `35` |
| IPv4 identification | `0x1405` | `0x1406` |
| IPv4 flags/fragment offset | `0x0000` | `0x0000` |
| IPv4 TTL | `64` | `64` |
| IPv4 Protocol | `17` | `17` |
| IPv4 header checksum | `0xC971` | `0xC970` |
| IPv4 source | `192.168.14.2` | `192.168.14.1` |
| IPv4 destination | `192.168.14.1` | `192.168.14.2` |
| UDP source port | `0x1405` | `0x1406` |
| UDP destination port | `0x1406` | `0x1405` |
| UDP length | `15` | `15` |
| UDP checksum | `0xF08A` | `0xF08A` |
| UDP data | `PYTHUDP` | `PYTHUDP` |
| Ethernet padding | 11 zero bytes | 11 zero bytes |

Each UDP datagram has an 8-byte UDP header and 7 data octets, so its UDP
length is 15. The IPv4 total length is 35 bytes. Each software Ethernet frame
is exactly 60 bytes excluding FCS: 14 Ethernet bytes, 35 IPv4/UDP datagram
bytes, and 11 zero padding bytes. FCS is neither created nor interpreted at
the `NetworkPort` boundary.

The UDP checksum is the 16-bit one's-complement checksum over the conceptual
IPv4 pseudo-header, UDP header with checksum zeroed, and UDP data. The odd
seventh data octet is followed by one zero octet for checksum arithmetic only;
that arithmetic pad is not transmitted and is not counted in the UDP length.
The pseudo-header uses the source address, destination address, protocol 17,
and UDP length 15. A zero transmitted checksum is not accepted; the proof
requires checksum generation and validation to be enabled.

A wrong MAC, EtherType, IPv4 version/IHL, TOS, total length, identification,
fragmentation field, TTL, protocol, IPv4 checksum, address, source/destination
port, UDP length, UDP checksum, data octet, padding byte, or frame length is a
terminal proof failure.

## Consumer flow and lifecycle

```text
NetworkPort DESCRIBE
  → one exact ARP request/reply setup
  → encode one IPv4 Protocol 17 / UDP datagram request
  → SEND one copied Ethernet frame
  → TRY_RECEIVE boundedly until one frame arrives or exhaustion
  → validate Ethernet, IPv4, UDP, data, and zero padding
  → owner reset, consumer revocation, and terminal exit
```

The consumer reuses the existing `NetworkPort` ABI, Ethernet/ARP helpers, and
canonical IPv4 codec. Its UDP codec is bounded, allocation-free in production,
and uses checked slicing/arithmetic. It returns borrowed data views rather than
allocating. The receive loop has a fixed poll bound, matching the accepted
IPv4/ICMP proof behavior. Any received nonmatching frame is terminal failure;
no frame is accepted merely because its UDP checksum is valid.

The reply must reverse the request's IPv4 addresses and UDP ports and preserve
the exact seven data octets. The request and reply use their separately fixed
IPv4 identifiers above; no generic transaction identifier or port allocation
mechanism is introduced.

## Marker evidence

The exact marker order is:

```text
PYTHOS:CORE:UDP:BOOTSTRAPPED
PYTHOS:CORE:UDP:DESCRIBE_OK
PYTHOS:CORE:UDP:ARP_SETUP_OK
PYTHOS:CORE:UDP:TX_OK
PYTHOS:CORE:UDP:RX_OK
PYTHOS:CORE:UDP:TEARDOWN_REVOKED
PYTHOS:CORE:UDP_READY
```

Each marker must occur exactly once. Terminal failures use the existing
bounded console/error path and do not emit `UDP_READY`.

## Acceptance boundary

The proof is accepted only after local unit tests, the self-test, and one
serialized live QEMU run show exactly one ARP setup, one UDP request/reply (four
total Ethernet frames including ARP), the seven markers in order, no extra peer
transmit, no storage-path evidence, and `QEMU_OUTCOME success`. Default and
normal-session profiles must remain unchanged. No hosted UDP acceptance is
claimed unless a separate hosted run is recorded.

This is a client-only bounded datagram proof. It does not implement or claim a
complete RFC 1122 UDP host, UDP application interface, port-unreachable
behavior, ICMP error delivery, IP-options interface, multihoming behavior,
invalid-source-address handling, or general UDP service.

## Standards basis

[RFC 768](https://www.rfc-editor.org/rfc/rfc768.html) defines the UDP header,
source and destination ports, datagram length including the UDP header and
data, the one's-complement checksum over the IPv4 pseudo-header/UDP/data, and
protocol number 17. Its checksum arithmetic pads an odd-length datagram for
the calculation without adding that octet to the transmitted length.

[RFC 1122 sections 4.1.1, 4.1.3.1--4.1.3.6, and 4.1.4](https://www.rfc-editor.org/rfc/rfc1122.html)
describe UDP's minimal non-guaranteed datagram service and the broader host
requirements for ports, ICMP messages, checksums, IP options, multihoming,
invalid addresses, and the UDP/application interface. This proof implements
the checksum generation/validation required by section 4.1.3.4 but adopts a
stricter finite client profile and does not claim the complete host obligations.

## Explicit stopping point

This slice stops at one controlled UDP datagram exchange above IPv4. It does
not establish sockets, a reusable UDP service, a port namespace, or a general
transport API. TCP, DNS, capability-gated sockets, secure transport, and Phase
15 physical-hardware work remain separate decisions and implementations.
