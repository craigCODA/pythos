# Phase 14 Bounded ICMP Echo Proof Design

Date: 2026-09-17

Status: Accepted for implementation under the Phase 14 continuation authority

## Decision in one sentence

Add one opt-in native ICMP Echo Request/Reply proof above the accepted
`NetworkPort`, Ethernet-II, ARP, and IPv4 boundaries; leave reusable ICMP,
socket, routing, later transport, physical hardware, and Phase 15 behavior
outside this slice.

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
ICMP Echo: this proof
```

`VirtioTransport` remains the privileged PythOS transport adapter and fulfills
the Virtio specification's driver role. `NetworkPort` remains the
capability-scoped runtime resource with copied frames. ICMP interpretation
stays in the opt-in native consumer; no new PythOS `Driver` abstraction, ABI,
PythTIG record, syscall, capability right, transport behavior, or persistent
network state is introduced.

This slice does not authorize an ICMP dispatcher, echo daemon, error-message
generation, rate limiting, retry/timer policy, routing, fragmentation,
multicast/broadcast echo, UDP/TCP/DNS, sockets, TLS, physical NIC/Wi-Fi,
modern Virtio PCI, interrupts, multiqueue, offloads, zero-copy, multiple
consumers, or any Phase 15 hardware model.

## Exact proof identity

The additive native proof identity is:

```text
ICMP_PROBE_PROGRAM_NAME = b"icmp-probe.elf"
ICMP_PROBE_PRINCIPAL_ID = 0x5059_4943_4D50_0001
ICMP_CONSUMER_SERVICE_ID = 0x5059_4943_4353_0001
ICMP_OWNER_SERVICE_ID    = 0x5059_4943_4F57_0001
```

The existing IPv4 probe remains unchanged and remains accepted separately.
The ICMP probe is disabled on default and normal-session boot and runs only
after the existing operational NetworkPort capability is installed.

## Exact address, frame, and ICMP profile

The proof reuses the deterministic QEMU relationship from ADR 0098:

```text
described local MAC: 52:54:00:12:34:56
peer MAC:            02:00:00:00:00:02
local IPv4:          192.168.14.2
peer IPv4:            192.168.14.1
proof-only prefix:   /24
```

The setup is exactly one matching ARP request and one matching ARP reply. It
does not establish persistent IP or ARP state. The ICMP exchange is exactly
one request and one reply carried in IPv4 datagrams:

| Field | Request | Reply |
| --- | --- | --- |
| Ethernet EtherType | `0x0800` | `0x0800` |
| IPv4 version/IHL | `0x45` | `0x45` |
| IPv4 TOS/DSCP/ECN | `0x00` | `0x00` |
| IPv4 total length | `36` | `36` |
| IPv4 identification | `0x1403` | `0x1404` |
| IPv4 flags/fragment offset | `0x0000` | `0x0000` |
| IPv4 TTL | `64` | `64` |
| IPv4 Protocol | `1` | `1` |
| IPv4 header checksum | `0xC982` | `0xC981` |
| IPv4 source | `192.168.14.2` | `192.168.14.1` |
| IPv4 destination | `192.168.14.1` | `192.168.14.2` |
| ICMP Type | `8` | `0` |
| ICMP Code | `0` | `0` |
| ICMP checksum | `0xA8C6` | `0xB0C6` |
| ICMP identifier | `0x1403` | `0x1403` |
| ICMP sequence | `0x0001` | `0x0001` |
| ICMP data | `PYTHICMP` | `PYTHICMP` |
| Ethernet padding | 10 zero bytes | 10 zero bytes |

Each software frame is exactly 60 bytes excluding FCS: 14 Ethernet bytes,
36 IPv4/ICMP datagram bytes, and 10 zero padding bytes. FCS is neither
created nor interpreted at the NetworkPort boundary.

The ICMP checksum is the one's-complement checksum over the ICMP message
starting at Type with the checksum field zeroed. The request and reply must
return the same identifier, sequence, and data. A wrong type, code, checksum,
identifier, sequence, payload, address, protocol, fragmentation field, MAC,
EtherType, padding byte, or length is a terminal proof failure.

## Consumer flow and lifecycle

```text
NetworkPort DESCRIBE
  → one exact ARP request/reply setup
  → encode one IPv4 Protocol 1 / ICMP Echo Request
  → SEND one copied Ethernet frame
  → TRY_RECEIVE boundedly until one frame arrives or exhaustion; any received nonmatching frame is terminal failure
  → validate Ethernet, IPv4, ICMP, and zero padding
  → owner reset, consumer revocation, and terminal exit
```

The consumer reuses the existing NetworkPort ABI, Ethernet/ARP helpers, and
canonical IPv4 codec. Its ICMP codec is bounded, allocation-free in
production, and returns borrowed payload views. The receive loop has a fixed
poll bound, matching the accepted IPv4 proof's terminal behavior. No frame is
accepted merely because it has a valid checksum; every deterministic field
above must match.

## Marker evidence

The exact marker order is:

```text
PYTHOS:CORE:ICMP:BOOTSTRAPPED
PYTHOS:CORE:ICMP:DESCRIBE_OK
PYTHOS:CORE:ICMP:ARP_SETUP_OK
PYTHOS:CORE:ICMP:TX_OK
PYTHOS:CORE:ICMP:RX_OK
PYTHOS:CORE:ICMP:TEARDOWN_REVOKED
PYTHOS:CORE:ICMP_READY
```

Each marker must occur exactly once. Terminal failures use the existing
bounded console/error path and do not emit `ICMP_READY`.

## Acceptance boundary

The proof is accepted only after local unit tests, the self-test, and one
serialized live QEMU run show exactly one ARP setup, one ICMP request/reply,
the seven markers in order, no extra peer transmit, no storage-path evidence,
and `QEMU_OUTCOME success`. No hosted ICMP acceptance is claimed unless a
separate hosted run is recorded. This is a client-only bounded proof; it does
not implement or claim RFC 1122's complete-host ICMP Echo server or
user-interface obligations.

## Standards basis

RFC 792, “Echo or Echo Reply Message,” defines Type 8 Echo, Type 0 Echo Reply,
Code 0, the ICMP checksum, and preservation of identifier, sequence number,
and data in the reply:
<https://www.rfc-editor.org/rfc/rfc792.html#echo-or-echo-reply-message>.

RFC 1122 section 3.2.2.6 defines the host Echo Request/Reply requirements and
requires the reply to preserve the request data and use the corresponding
specific-destination address:
<https://www.rfc-editor.org/rfc/rfc1122.html#section-3.2.2.6>.

This client-only bounded proof adopts a stricter deterministic profile than a
general ICMP implementation. It does not implement or claim RFC 1122's
complete-host ICMP Echo server or user-interface obligations.

## Explicit stopping point

This slice stops at one controlled ICMP Echo exchange above IPv4. The next
roadmap layers remain separate decisions and implementations. Phase 15
physical-hardware work remains separate and is not implied by this design.
